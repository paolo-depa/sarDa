//! The shell script run inside the Parser container.

use crate::catalog::ACTIVITIES;

/// Shell script run inside the Parser container. `$1` and `$2` are the
/// inclusive date bounds of files to skip (`YYYY-MM-DD`, empty for none), and
/// the remaining positional parameters are the SA file paths.
pub fn build_script(skip_before: Option<&str>, skip_after: Option<&str>) -> String {
    // Stays within POSIX sh and coreutils (sh, mktemp, tr, cat, rm) next to
    // sadf, with no sed, grep or awk, so the parser image can move to a
    // minimal base without touching the script (ADR 0007).
    let mut s = SCRIPT
        .replace("__SB__", skip_before.unwrap_or(""))
        .replace("__SA__", skip_after.unwrap_or(""));
    for a in ACTIVITIES {
        s.push_str(&format!(
            "echo \"@@BEGIN {l}\"; sadf -d \"$f\" -- {o} 2>\"$T\"; emit \"$T\" \"@@ERR\"; echo \"@@END\"\n",
            l = a.label,
            o = a.sadf_opts
        ));
    }
    s.push_str("i=$((i+1))\ndone\nrm -f \"$T\" \"$H\" \"$C\"\n");
    s
}

/// Prologue of the script: `$1`/`$2` are the skip bounds, the rest are files.
const SCRIPT: &str = r##"T=$(mktemp 2>/dev/null || echo /tmp/sarda.$$)
H="$T.h"
C="$T.c"
SB="__SB__"
SA="__SA__"
shift 2
echo "@@VERSION $(sadf -V 2>/dev/null | head -n 1)"
i=0
emit() { while IFS= read -r l || [ -n "$l" ]; do printf '%s %s\n' "$2" "$l"; done <"$1"; }
for f in "$@"; do
echo "@@FILE $i"
: >"$T"
p=1
if sadf -j -H "$f" >"$H" 2>"$T"; then
p=0
elif sadf -c "$f" >"$C" 2>>"$T" && sadf -j -H "$C" >"$H" 2>>"$T"; then
p=0
f="$C"
echo "@@CONVERTED $i"
fi
echo "@@PROBE $i $p"
HDR=""
if [ -s "$H" ]; then HDR=$(tr -d '\n\r\t' <"$H"); echo "@@HDR $HDR"; fi
emit "$T" "@@PERR"
d=""
case "$HDR" in
*'"file-date"'*)
rest=${HDR#*'"file-date"'}
rest=${rest#:}
rest=${rest# }
rest=${rest#'"'}
d=${rest%%'"'*}
case "$d" in
????-??-??) ;;
*) d="" ;;
esac
;;
esac
skip=0
if [ "$p" != 0 ]; then skip=1; fi
if [ -n "$d" ] && [ -n "$SB" ] && expr "$d" "<" "$SB" >/dev/null 2>&1; then skip=1; fi
if [ -n "$d" ] && [ -n "$SA" ] && expr "$d" ">" "$SA" >/dev/null 2>&1; then skip=1; fi
if [ "$skip" = 1 ]; then echo "@@SKIP $i"; i=$((i+1)); continue; fi
"##;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_mentions_every_activity() {
        let s = build_script(None, None);
        for a in ACTIVITIES {
            assert!(s.contains(&format!("@@BEGIN {}", a.label)));
            assert!(s.contains(&format!("-- {} ", a.sadf_opts)));
        }
    }

    #[test]
    fn script_probes_skips_and_converts() {
        let s = build_script(Some("2026-01-01"), Some("2026-12-31"));
        assert!(s.contains("sadf -j -H \"$f\""));
        assert!(s.contains("sadf -c \"$f\""));
        assert!(s.contains("@@CONVERTED $i"));
        assert!(s.contains("@@VERSION $(sadf -V"));
        assert!(s.contains("@@SKIP $i"));
        assert!(s.contains("SB=\"2026-01-01\""));
        assert!(s.contains("SA=\"2026-12-31\""));
        assert!(s.contains("expr \"$d\" \"<\" \"$SB\""));
        let plain = build_script(None, None);
        assert!(plain.contains("SB=\"\""));
        assert!(plain.contains("SA=\"\""));
        for legacy in ["sed", "grep", "awk"] {
            assert!(
                !s.contains(legacy) && !plain.contains(legacy),
                "the script must stay within POSIX sh and coreutils (no {legacy}), ADR 0007"
            );
        }
    }
}
