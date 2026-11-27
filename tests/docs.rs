//! Keeps the documentation in step with the code it documents, and turns the
//! rules the agent docs state into checks.

use std::path::{Path, PathBuf};
use std::process::Command;

use regex::Regex;
use sarda::catalog;
use sarda::error::Kind;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

/// Every failure kind; extend it together with `Kind`.
const KINDS: [Kind; 8] = [
    Kind::Other,
    Kind::Usage,
    Kind::NoRuntime,
    Kind::MissingImage,
    Kind::NoData,
    Kind::IncompatibleFile,
    Kind::GrafanaFailed,
    Kind::PortUnavailable,
];

fn root() -> &'static Path {
    Path::new(ROOT)
}

fn readme() -> String {
    std::fs::read_to_string(root().join("README.md")).unwrap()
}

/// `(path below src/, text)` of every source file, including submodules.
fn sources() -> Vec<(String, String)> {
    fn walk(dir: &Path, out: &mut Vec<(String, String)>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|x| x == "rs") {
                let name = path
                    .strip_prefix(root().join("src"))
                    .unwrap()
                    .display()
                    .to_string();
                out.push((name, std::fs::read_to_string(&path).unwrap()));
            }
        }
    }
    let mut out = Vec::new();
    walk(&root().join("src"), &mut out);
    out.sort();
    out
}

/// `AGENTS.md` and every file under `docs/agents/`, as `(path, text)`.
fn agent_docs() -> Vec<(String, String)> {
    let mut paths: Vec<PathBuf> = vec![root().join("AGENTS.md")];
    let mut sub: Vec<PathBuf> = std::fs::read_dir(root().join("docs/agents"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    sub.sort();
    paths.extend(sub);
    paths
        .into_iter()
        .map(|p| {
            let rel = p.strip_prefix(root()).unwrap().display().to_string();
            (rel, std::fs::read_to_string(&p).unwrap())
        })
        .collect()
}

#[test]
fn readme_catalog_table_is_current() {
    let text = readme();
    let start =
        text.find("<!-- catalog:start -->").expect("start marker") + "<!-- catalog:start -->".len();
    let end = text.find("<!-- catalog:end -->").expect("end marker");
    assert_eq!(
        text[start..end].trim(),
        catalog::markdown_table().trim(),
        "README catalog table is stale; regenerate it with `cargo run -q -- --print-catalog`"
    );
}

#[test]
fn readme_lists_every_exit_code() {
    let text = readme();
    for kind in KINDS {
        let row = format!("| {} |", kind.exit_code());
        assert!(
            text.contains(&row),
            "README exit code table lacks {row} ({})",
            kind.name()
        );
    }
}

#[test]
fn readme_names_the_default_images() {
    let text = readme();
    assert!(text.contains(sarda::cli::DEFAULT_GRAFANA_IMAGE));
    assert!(text.contains(sarda::cli::DEFAULT_SADF_IMAGE));
}

#[test]
fn help_lists_every_exit_code() {
    let out = Command::new(env!("CARGO_BIN_EXE_sarda"))
        .arg("--help")
        .output()
        .unwrap();
    let help = String::from_utf8_lossy(&out.stdout);
    let codes = std::iter::once(0).chain(KINDS.iter().map(|k| k.exit_code()));
    for code in codes {
        let prefix = format!("{code}  ");
        assert!(
            help.lines().any(|l| l.trim_start().starts_with(&prefix)),
            "`--help` has no line for exit code {code}"
        );
    }
}

#[test]
fn stdout_is_written_only_by_the_reporter_and_the_print_modes() {
    let writes_stdout = Regex::new(r"(^|[^e])print(ln)?!\(").unwrap();
    for (name, text) in sources() {
        if name == "session.rs" || name == "main.rs" {
            continue;
        }
        for (i, line) in text.lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            assert!(
                !writes_stdout.is_match(line),
                "src/{name}:{}: writes to stdout; only `session::Reporter` and the --print-* modes in main.rs may",
                i + 1
            );
        }
    }
}

#[test]
fn nothing_binds_beyond_loopback() {
    for (name, text) in sources() {
        for needle in ["0.0.0.0", "[::]"] {
            assert!(
                !text.contains(needle),
                "src/{name} mentions {needle}: everything must bind 127.0.0.1 (ADR 0005)"
            );
        }
    }
}

#[test]
fn agent_docs_point_only_at_things_that_exist() {
    let sources = sources();
    let backticked = Regex::new(r"`([^`\n]+)`").unwrap();
    let test_run = Regex::new(r"--test (\w+) (\w+)").unwrap();
    let word = |name: &str| Regex::new(&format!(r"\b{}\b", regex::escape(name))).unwrap();
    let in_source = |module: &str, name: &str| {
        let own = format!("{module}.rs");
        let dir = format!("{module}/");
        let files: Vec<_> = sources
            .iter()
            .filter(|(f, _)| f == &own || f.starts_with(&dir))
            .collect();
        (!files.is_empty()).then(|| files.iter().any(|(_, text)| word(name).is_match(text)))
    };

    for (doc, text) in agent_docs() {
        for cap in backticked.captures_iter(&text) {
            let token = &cap[1];
            let is_path = ["src/", "tests/", "docs/", "scripts/", "container/"]
                .iter()
                .any(|p| token.starts_with(p));
            if is_path || (token.ends_with(".md") && !token.contains(' ')) {
                let path = token.trim_end_matches('/');
                assert!(
                    root().join(path).exists(),
                    "{doc}: `{token}` does not exist"
                );
            }
            if let Some((module, name)) = token.split_once("::") {
                if let Some(found) = in_source(module, name) {
                    assert!(found, "{doc}: `{token}`: no `{name}` in src/{module}.rs");
                }
            }
            if let Some(c) = test_run.captures(token) {
                let file = root().join(format!("tests/{}.rs", &c[1]));
                let body = std::fs::read_to_string(&file)
                    .unwrap_or_else(|_| panic!("{doc}: `{token}`: no tests/{}.rs", &c[1]));
                assert!(
                    word(&format!("fn {}", &c[2])).is_match(&body),
                    "{doc}: `{token}`: no test `{}` in tests/{}.rs",
                    &c[2],
                    &c[1]
                );
            }
        }
    }
}

#[test]
fn every_agent_doc_is_reachable_from_agents_md() {
    let docs = agent_docs();
    let agents = &docs[0].1;
    for (path, _) in docs.iter().skip(1) {
        assert!(
            agents.contains(path),
            "AGENTS.md does not mention {path}, so no agent will load it"
        );
    }
}
