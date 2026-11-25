//! The PromQL subset the Sarda API understands:
//!
//! * instant vector selectors: `name{label="v", other=~"a|b", x!="y", z!~"q.*"}`
//! * aggregations `sum`, `avg`, `min`, `max`, `count`, with `by (...)` or
//!   `without (...)` before or after the parenthesised operand
//! * number literals, parentheses and `+ - * /` between scalars
//!
//! Everything else is rejected with an explicit message.

use std::collections::BTreeMap;

use regex::Regex;

use crate::store::{Labels, Series, Store};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MatchOp {
    Eq,
    Ne,
    Re,
    Nre,
}

#[derive(Debug, Clone)]
pub struct Matcher {
    pub label: String,
    pub op: MatchOp,
    pub value: String,
    re: Option<Regex>,
}

impl Matcher {
    pub fn new(label: &str, op: MatchOp, value: &str) -> Result<Matcher, String> {
        let re = match op {
            MatchOp::Re | MatchOp::Nre => Some(
                Regex::new(&format!("^(?:{value})$"))
                    .map_err(|e| format!("invalid regular expression '{value}': {e}"))?,
            ),
            _ => None,
        };
        Ok(Matcher {
            label: label.to_string(),
            op,
            value: value.to_string(),
            re,
        })
    }

    fn matches(&self, actual: &str) -> bool {
        match self.op {
            MatchOp::Eq => actual == self.value,
            MatchOp::Ne => actual != self.value,
            MatchOp::Re => self.re.as_ref().is_some_and(|r| r.is_match(actual)),
            MatchOp::Nre => !self.re.as_ref().is_some_and(|r| r.is_match(actual)),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Selector {
    pub matchers: Vec<Matcher>,
}

impl Selector {
    pub fn matches(&self, s: &Series) -> bool {
        self.matchers.iter().all(|m| {
            if m.label == "__name__" {
                m.matches(&s.name)
            } else {
                m.matches(s.labels.get(&m.label).map_or("", String::as_str))
            }
        })
    }

    /// Series of `store` selected by this selector. A leading `__name__="x"`
    /// matcher is used as an index lookup.
    pub fn select<'a>(&'a self, store: &'a Store) -> Box<dyn Iterator<Item = &'a Series> + 'a> {
        if let Some(m) = self
            .matchers
            .iter()
            .find(|m| m.label == "__name__" && m.op == MatchOp::Eq)
        {
            Box::new(store.by_name(&m.value).filter(move |s| self.matches(s)))
        } else {
            Box::new(store.series.iter().filter(move |s| self.matches(s)))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AggOp {
    Sum,
    Avg,
    Min,
    Max,
    Count,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Grouping {
    All,
    By(Vec<String>),
    Without(Vec<String>),
}

#[derive(Debug, Clone)]
pub enum Expr {
    Number(f64),
    Selector(Selector),
    Aggregate {
        op: AggOp,
        grouping: Grouping,
        expr: Box<Expr>,
    },
    Binary {
        op: char,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
}

// ---------------------------------------------------------------- lexer

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Ident(String),
    Num(f64),
    Str(String),
    Sym(&'static str),
}

fn lex(input: &str) -> Result<Vec<Tok>, String> {
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    let mut out = Vec::new();
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_alphabetic() || c == '_' || c == ':' {
            let start = i;
            while i < chars.len()
                && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == ':')
            {
                i += 1;
            }
            out.push(Tok::Ident(chars[start..i].iter().collect()));
        } else if c.is_ascii_digit()
            || (c == '.' && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit()))
        {
            let start = i;
            while i < chars.len()
                && (chars[i].is_ascii_digit()
                    || matches!(chars[i], '.' | 'e' | 'E')
                    || (matches!(chars[i], '+' | '-') && matches!(chars[i - 1], 'e' | 'E')))
            {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            out.push(Tok::Num(
                text.parse()
                    .map_err(|_| format!("invalid number '{text}'"))?,
            ));
        } else if c == '"' || c == '\'' {
            let quote = c;
            i += 1;
            let mut s = String::new();
            loop {
                match chars.get(i) {
                    None => return Err("unterminated string literal".into()),
                    Some(&q) if q == quote => {
                        i += 1;
                        break;
                    }
                    Some('\\') => {
                        i += 1;
                        match chars.get(i) {
                            Some('n') => s.push('\n'),
                            Some('t') => s.push('\t'),
                            Some(&o) => {
                                // Keep regex escapes such as \. intact.
                                if !matches!(o, '"' | '\'' | '\\') {
                                    s.push('\\');
                                }
                                s.push(o);
                            }
                            None => return Err("unterminated string literal".into()),
                        }
                        i += 1;
                    }
                    Some(&o) => {
                        s.push(o);
                        i += 1;
                    }
                }
            }
            out.push(Tok::Str(s));
        } else {
            let two: String = chars[i..chars.len().min(i + 2)].iter().collect();
            let sym = match two.as_str() {
                "!=" => Some("!="),
                "=~" => Some("=~"),
                "!~" => Some("!~"),
                _ => None,
            };
            if let Some(s) = sym {
                out.push(Tok::Sym(s));
                i += 2;
                continue;
            }
            let s = match c {
                '{' => "{",
                '}' => "}",
                '(' => "(",
                ')' => ")",
                ',' => ",",
                '=' => "=",
                '+' => "+",
                '-' => "-",
                '*' => "*",
                '/' => "/",
                '[' => "[",
                ']' => "]",
                other => return Err(format!("unexpected character '{other}'")),
            };
            out.push(Tok::Sym(s));
            i += 1;
        }
    }
    Ok(out)
}

// --------------------------------------------------------------- parser

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
}

fn agg_op(name: &str) -> Option<AggOp> {
    match name {
        "sum" => Some(AggOp::Sum),
        "avg" => Some(AggOp::Avg),
        "min" => Some(AggOp::Min),
        "max" => Some(AggOp::Max),
        "count" => Some(AggOp::Count),
        _ => None,
    }
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }

    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        self.pos += 1;
        t
    }

    fn eat_sym(&mut self, s: &'static str) -> bool {
        if self.peek() == Some(&Tok::Sym(s)) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect_sym(&mut self, s: &'static str) -> Result<(), String> {
        if self.eat_sym(s) {
            Ok(())
        } else {
            Err(format!("expected '{s}' but found {}", self.describe()))
        }
    }

    fn describe(&self) -> String {
        match self.peek() {
            None => "end of expression".into(),
            Some(Tok::Ident(i)) => format!("'{i}'"),
            Some(Tok::Num(n)) => format!("'{n}'"),
            Some(Tok::Str(s)) => format!("\"{s}\""),
            Some(Tok::Sym(s)) => format!("'{s}'"),
        }
    }

    fn expr(&mut self) -> Result<Expr, String> {
        let mut lhs = self.term()?;
        while let Some(Tok::Sym(s @ ("+" | "-"))) = self.peek().cloned() {
            self.pos += 1;
            let rhs = self.term()?;
            lhs = Expr::Binary {
                op: s.chars().next().unwrap(),
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn term(&mut self) -> Result<Expr, String> {
        let mut lhs = self.unary()?;
        while let Some(Tok::Sym(s @ ("*" | "/"))) = self.peek().cloned() {
            self.pos += 1;
            let rhs = self.unary()?;
            lhs = Expr::Binary {
                op: s.chars().next().unwrap(),
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn unary(&mut self) -> Result<Expr, String> {
        if self.eat_sym("-") {
            let inner = self.unary()?;
            return Ok(Expr::Binary {
                op: '-',
                lhs: Box::new(Expr::Number(0.0)),
                rhs: Box::new(inner),
            });
        }
        self.primary()
    }

    fn grouping(&mut self) -> Result<Option<Grouping>, String> {
        let kind = match self.peek() {
            Some(Tok::Ident(i)) if i == "by" || i == "without" => i.clone(),
            _ => return Ok(None),
        };
        self.pos += 1;
        self.expect_sym("(")?;
        let mut labels = Vec::new();
        while !self.eat_sym(")") {
            match self.next() {
                Some(Tok::Ident(l)) => labels.push(l),
                _ => return Err("expected a label name in grouping".into()),
            }
            if !self.eat_sym(",") {
                self.expect_sym(")")?;
                break;
            }
        }
        Ok(Some(if kind == "by" {
            Grouping::By(labels)
        } else {
            Grouping::Without(labels)
        }))
    }

    fn primary(&mut self) -> Result<Expr, String> {
        match self.peek().cloned() {
            Some(Tok::Num(n)) => {
                self.pos += 1;
                Ok(Expr::Number(n))
            }
            Some(Tok::Sym("(")) => {
                self.pos += 1;
                let e = self.expr()?;
                self.expect_sym(")")?;
                Ok(e)
            }
            Some(Tok::Ident(name)) => {
                let next = self.toks.get(self.pos + 1);
                let starts_group =
                    matches!(next, Some(Tok::Ident(i)) if i == "by" || i == "without");
                if let Some(op) = agg_op(&name) {
                    if starts_group || next == Some(&Tok::Sym("(")) {
                        self.pos += 1;
                        let mut grouping = self.grouping()?;
                        self.expect_sym("(")?;
                        let inner = self.expr()?;
                        self.expect_sym(")")?;
                        if grouping.is_none() {
                            grouping = self.grouping()?;
                        }
                        return Ok(Expr::Aggregate {
                            op,
                            grouping: grouping.unwrap_or(Grouping::All),
                            expr: Box::new(inner),
                        });
                    }
                }
                if next == Some(&Tok::Sym("(")) {
                    return Err(format!(
                        "function or aggregation '{name}' is not supported (supported: sum, avg, min, max, count)"
                    ));
                }
                self.selector()
            }
            Some(Tok::Sym("{")) => self.selector(),
            _ => Err(format!("unexpected {}", self.describe())),
        }
    }

    fn selector(&mut self) -> Result<Expr, String> {
        let mut matchers = Vec::new();
        if let Some(Tok::Ident(name)) = self.peek().cloned() {
            self.pos += 1;
            matchers.push(Matcher::new("__name__", MatchOp::Eq, &name)?);
        }
        if self.eat_sym("{") {
            while !self.eat_sym("}") {
                let label = match self.next() {
                    Some(Tok::Ident(l)) => l,
                    _ => return Err("expected a label name in selector".into()),
                };
                let op = match self.next() {
                    Some(Tok::Sym("=")) => MatchOp::Eq,
                    Some(Tok::Sym("!=")) => MatchOp::Ne,
                    Some(Tok::Sym("=~")) => MatchOp::Re,
                    Some(Tok::Sym("!~")) => MatchOp::Nre,
                    _ => return Err(format!("expected a label matcher operator after '{label}'")),
                };
                let value = match self.next() {
                    Some(Tok::Str(v)) => v,
                    _ => return Err(format!("expected a quoted value for label '{label}'")),
                };
                matchers.push(Matcher::new(&label, op, &value)?);
                if !self.eat_sym(",") {
                    self.expect_sym("}")?;
                    break;
                }
            }
        }
        if matchers.is_empty() {
            return Err("a selector needs a metric name or at least one matcher".into());
        }
        if self.peek() == Some(&Tok::Sym("[")) {
            return Err("range vectors ('[...]') are not supported".into());
        }
        Ok(Expr::Selector(Selector { matchers }))
    }
}

pub fn parse(input: &str) -> Result<Expr, String> {
    let toks = lex(input)?;
    if toks.is_empty() {
        return Err("empty expression".into());
    }
    let mut p = Parser { toks, pos: 0 };
    let e = p.expr()?;
    if p.pos < p.toks.len() {
        return Err(format!("unexpected {} after expression", p.describe()));
    }
    Ok(e)
}

/// Parses a `match[]` argument, which must be a plain selector.
pub fn parse_selector(input: &str) -> Result<Selector, String> {
    match parse(input)? {
        Expr::Selector(s) => Ok(s),
        _ => Err(format!("'{input}' is not a series selector")),
    }
}

// ------------------------------------------------------------ evaluator

#[derive(Debug, Clone)]
pub struct Evaluated {
    /// Includes `__name__` for plain selectors.
    pub labels: Labels,
    /// `(unix ms, value)` for the evaluation steps that have data.
    pub points: Vec<(i64, f64)>,
}

#[derive(Debug)]
pub enum Value {
    Scalar(f64),
    Vector(Vec<Evaluated>),
}

/// Value of `s` at `t`: the latest sample at or before `t`, not older than
/// `lookback_ms`.
fn sample_at(s: &Series, t: i64, lookback_ms: i64) -> Option<f64> {
    let idx = s.samples.partition_point(|(ts, _)| *ts <= t);
    let (ts, v) = *s.samples.get(idx.checked_sub(1)?)?;
    (t - ts <= lookback_ms).then_some(v)
}

pub fn eval(expr: &Expr, store: &Store, steps: &[i64], lookback_ms: i64) -> Result<Value, String> {
    match expr {
        Expr::Number(n) => Ok(Value::Scalar(*n)),
        Expr::Selector(sel) => {
            let mut out = Vec::new();
            for s in sel.select(store) {
                let points: Vec<(i64, f64)> = steps
                    .iter()
                    .filter_map(|&t| sample_at(s, t, lookback_ms).map(|v| (t, v)))
                    .collect();
                if !points.is_empty() {
                    let mut labels = s.labels.clone();
                    labels.insert("__name__".into(), s.name.clone());
                    out.push(Evaluated { labels, points });
                }
            }
            Ok(Value::Vector(out))
        }
        Expr::Aggregate { op, grouping, expr } => {
            let Value::Vector(inner) = eval(expr, store, steps, lookback_ms)? else {
                return Err("aggregation needs an instant vector operand".into());
            };
            Ok(Value::Vector(aggregate(*op, grouping, inner, steps)))
        }
        Expr::Binary { op, lhs, rhs } => {
            match (
                eval(lhs, store, steps, lookback_ms)?,
                eval(rhs, store, steps, lookback_ms)?,
            ) {
                (Value::Scalar(a), Value::Scalar(b)) => Ok(Value::Scalar(match op {
                    '+' => a + b,
                    '-' => a - b,
                    '*' => a * b,
                    _ => a / b,
                })),
                _ => Err("binary operators are only supported between scalars".into()),
            }
        }
    }
}

#[derive(Clone, Copy)]
struct Acc {
    count: u32,
    sum: f64,
    min: f64,
    max: f64,
}

fn aggregate(
    op: AggOp,
    grouping: &Grouping,
    inner: Vec<Evaluated>,
    steps: &[i64],
) -> Vec<Evaluated> {
    let step_idx: std::collections::HashMap<i64, usize> =
        steps.iter().enumerate().map(|(i, t)| (*t, i)).collect();
    let empty = Acc {
        count: 0,
        sum: 0.0,
        min: f64::INFINITY,
        max: f64::NEG_INFINITY,
    };
    let mut groups: BTreeMap<Labels, Vec<Acc>> = BTreeMap::new();

    for series in inner {
        let key: Labels = match grouping {
            Grouping::All => Labels::new(),
            Grouping::By(keep) => series
                .labels
                .iter()
                .filter(|(k, _)| keep.contains(k))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            Grouping::Without(drop) => series
                .labels
                .iter()
                .filter(|(k, _)| *k != "__name__" && !drop.contains(k))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        };
        let accs = groups
            .entry(key)
            .or_insert_with(|| vec![empty; steps.len()]);
        for (t, v) in series.points {
            let a = &mut accs[step_idx[&t]];
            a.count += 1;
            a.sum += v;
            a.min = a.min.min(v);
            a.max = a.max.max(v);
        }
    }

    groups
        .into_iter()
        .map(|(labels, accs)| {
            let points = steps
                .iter()
                .zip(accs)
                .filter(|(_, a)| a.count > 0)
                .map(|(t, a)| {
                    let v = match op {
                        AggOp::Sum => a.sum,
                        AggOp::Avg => a.sum / f64::from(a.count),
                        AggOp::Min => a.min,
                        AggOp::Max => a.max,
                        AggOp::Count => f64::from(a.count),
                    };
                    (*t, v)
                })
                .collect();
            Evaluated { labels, points }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Store {
        let mk = |name: &str, host: &str, cpu: &str, vals: &[(i64, f64)]| {
            let mut labels = Labels::new();
            labels.insert("host".into(), host.into());
            labels.insert("cpu".into(), cpu.into());
            Series {
                name: name.into(),
                labels,
                samples: vals.to_vec(),
            }
        };
        Store::from_series(vec![
            mk("m", "h1", "0", &[(0, 1.0), (10_000, 2.0), (20_000, 3.0)]),
            mk("m", "h1", "1", &[(0, 10.0), (10_000, 20.0), (20_000, 30.0)]),
            mk("m", "h2", "0", &[(0, 100.0)]),
            mk("other", "h1", "0", &[(0, 5.0)]),
        ])
    }

    fn vector(e: &str, steps: &[i64]) -> Vec<Evaluated> {
        match eval(&parse(e).unwrap(), &store(), steps, 15_000).unwrap() {
            Value::Vector(v) => v,
            Value::Scalar(_) => panic!("scalar"),
        }
    }

    #[test]
    fn selector_with_matchers() {
        assert_eq!(vector(r#"m{host="h1"}"#, &[0]).len(), 2);
        assert_eq!(vector(r#"m{cpu=~"0|1"}"#, &[0]).len(), 3);
        assert_eq!(vector(r#"m{cpu!="0"}"#, &[0]).len(), 1);
        assert_eq!(vector(r#"m{host!~"h1"}"#, &[0]).len(), 1);
        assert_eq!(vector(r#"{__name__=~"m|other"}"#, &[0]).len(), 4);
    }

    #[test]
    fn regex_is_anchored() {
        assert_eq!(vector(r#"m{cpu=~"0"}"#, &[0]).len(), 2);
        assert_eq!(vector(r#"m{host=~"h"}"#, &[0]).len(), 0);
    }

    #[test]
    fn lookback_limits_stale_samples() {
        assert_eq!(vector("m", &[20_000]).len(), 2);
        assert_eq!(vector("m", &[200_000]).len(), 0);
        assert_eq!(vector("m", &[12_000])[0].points[0].1, 2.0);
    }

    #[test]
    fn aggregations() {
        let v = vector(r#"sum by (host) (m)"#, &[0]);
        assert_eq!(v.len(), 2);
        let h1 = v.iter().find(|e| e.labels["host"] == "h1").unwrap();
        assert_eq!(h1.points, vec![(0, 11.0)]);
        assert!(!h1.labels.contains_key("__name__"));

        let all = vector("avg(m)", &[0]);
        assert_eq!(all.len(), 1);
        assert!((all[0].points[0].1 - 111.0 / 3.0).abs() < 1e-9);

        assert_eq!(vector("count(m)", &[0])[0].points[0].1, 3.0);
        assert_eq!(vector("max(m)", &[0])[0].points[0].1, 100.0);
        assert_eq!(vector("min(m)", &[0])[0].points[0].1, 1.0);
    }

    #[test]
    fn trailing_and_without_grouping() {
        assert_eq!(vector("sum(m) by (cpu)", &[0]).len(), 2);
        let w = vector("sum without (cpu) (m)", &[0]);
        assert_eq!(w.len(), 2);
        assert!(w.iter().all(|e| !e.labels.contains_key("cpu")));
    }

    #[test]
    fn scalars() {
        let v = eval(&parse("1+1").unwrap(), &store(), &[0], 1).unwrap();
        assert!(matches!(v, Value::Scalar(x) if x == 2.0));
        let v = eval(&parse("-(2*3)-1").unwrap(), &store(), &[0], 1).unwrap();
        assert!(matches!(v, Value::Scalar(x) if x == -7.0));
    }

    #[test]
    fn unsupported_constructs_are_explained() {
        for (q, needle) in [
            ("rate(m[5m])", "not supported"),
            ("m[5m]", "range vectors"),
            ("topk(2, m)", "not supported"),
            ("m + m", "scalars"),
            ("", "empty"),
            ("m{cpu=0}", "quoted"),
            ("sum by (host", "expected"),
        ] {
            let err = parse(q)
                .and_then(|e| eval(&e, &store(), &[0], 1).map(|_| ()))
                .unwrap_err();
            assert!(err.contains(needle), "{q}: {err}");
        }
    }

    #[test]
    fn match_arguments_must_be_selectors() {
        assert!(parse_selector(r#"m{host="h1"}"#).is_ok());
        assert!(parse_selector("sum(m)").is_err());
    }
}
