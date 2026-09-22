//! Version comparison and range matching for mod dependencies.
//!
//! Two syntaxes appear in the wild and both must work:
//!
//! - **Fabric / Quilt** use semver-flavored predicates: `*`, `1.2.3`,
//!   `>=1.20`, `<1.21`, `^1.0.0`, `~1.2`, `1.2.x`, and space- or
//!   comma-separated conjunctions, with `||` between alternatives.
//! - **Forge / NeoForge** use Maven intervals: `[1.20,1.21)`, `[47,)`,
//!   `[1.0]`. A bare string there means "this version or newer".
//!
//! Rather than pull in a semver crate that speaks neither dialect exactly,
//! comparison is implemented directly: numeric components compare
//! numerically, a pre-release sorts before the same version without one,
//! and build metadata (`+mc26.2`, `+build.5`) is ignored, as semver says —
//! `0.9.1+mc26.2` *is* `0.9.1`.

use std::cmp::Ordering;

/// A parsed version: numeric components plus an optional pre-release tail.
///
/// Equality is defined by [`Ord`], not structurally: `1.2` and `1.2.0` are
/// the same version, so a mod requiring exactly `1.2` is satisfied by an
/// installed `1.2.0`. Deriving `PartialEq` here would make `Ord` and `Eq`
/// disagree and would reject that legitimate match.
#[derive(Debug, Clone)]
pub struct Version {
    pub parts: Vec<u64>,
    /// Text after `-`, e.g. `beta.1`; `None` for a release. A bare trailing
    /// `-` (Fabric's `>=26.2-`, "26.2 or any of its pre-releases") gives
    /// `Some("")`, which sorts below every named pre-release.
    pub pre: Option<String>,
}

impl Version {
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        // Build metadata identifies a build, not a version.
        let text = text.split('+').next().unwrap_or(text);
        let (numeric, pre) = match text.find('-') {
            Some(i) => (&text[..i], Some(text[i + 1..].to_string())),
            None => (text, None),
        };
        let mut parts = Vec::new();
        for component in numeric.split('.') {
            if component.is_empty() {
                continue;
            }
            // Tolerate trailing junk in a component (e.g. `1.20.1a`).
            let digits: String = component
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if digits.is_empty() {
                return None;
            }
            parts.push(digits.parse().ok()?);
        }
        if parts.is_empty() {
            return None;
        }
        Some(Self { parts, pre })
    }

    fn component(&self, index: usize) -> u64 {
        self.parts.get(index).copied().unwrap_or(0)
    }
}

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Version {}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        let width = self.parts.len().max(other.parts.len());
        for i in 0..width {
            match self.component(i).cmp(&other.component(i)) {
                Ordering::Equal => {}
                ordering => return ordering,
            }
        }
        // 1.0.0-beta < 1.0.0: a pre-release precedes its release.
        match (&self.pre, &other.pre) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(a), Some(b)) => compare_prerelease(a, b),
        }
    }
}

/// Semver pre-release ordering: dot-separated identifiers, numeric ones
/// compared numerically and sorting below alphanumeric ones, and a list that
/// is a prefix of the other sorting first (`beta.2 < beta.10 < rc`).
fn compare_prerelease(a: &str, b: &str) -> Ordering {
    let mut left = a.split('.').filter(|s| !s.is_empty());
    let mut right = b.split('.').filter(|s| !s.is_empty());
    loop {
        match (left.next(), right.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => {
                let ordering = match (x.parse::<u64>(), y.parse::<u64>()) {
                    (Ok(x), Ok(y)) => x.cmp(&y),
                    (Ok(_), Err(_)) => Ordering::Less,
                    (Err(_), Ok(_)) => Ordering::Greater,
                    (Err(_), Err(_)) => x.cmp(y),
                };
                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
        }
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let numeric: Vec<String> = self.parts.iter().map(|p| p.to_string()).collect();
        write!(f, "{}", numeric.join("."))?;
        if let Some(pre) = &self.pre {
            write!(f, "-{pre}")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Eq,
    Lt,
    Lte,
    Gt,
    Gte,
}

#[derive(Debug, Clone)]
enum Term {
    Any,
    Compare(Op, Version),
    /// A low/high window, used by `^`, `~`, `x`, and Maven intervals alike.
    Window {
        low: Option<Version>,
        low_inclusive: bool,
        high: Option<Version>,
        high_inclusive: bool,
    },
}

impl Term {
    fn matches(&self, v: &Version) -> bool {
        match self {
            Term::Any => true,
            Term::Compare(op, target) => match op {
                Op::Eq => v == target,
                Op::Lt => v < target,
                Op::Lte => v <= target,
                Op::Gt => v > target,
                Op::Gte => v >= target,
            },
            Term::Window {
                low,
                low_inclusive,
                high,
                high_inclusive,
            } => {
                if let Some(low) = low {
                    let ok = if *low_inclusive { v >= low } else { v > low };
                    if !ok {
                        return false;
                    }
                }
                if let Some(high) = high {
                    let ok = if *high_inclusive { v <= high } else { v < high };
                    if !ok {
                        return false;
                    }
                }
                true
            }
        }
    }
}

/// A dependency requirement: a disjunction of conjunctions.
///
/// An unparseable requirement becomes [`VersionReq::Unparsed`], which matches
/// everything but is surfaced to the user. A wrong "incompatible" verdict is
/// worse than an honest "could not check".
#[derive(Debug, Clone)]
pub struct VersionReq {
    kind: ReqKind,
}

#[derive(Debug, Clone)]
enum ReqKind {
    /// Disjunction of conjunctions: any alternative satisfying all its terms.
    Any(Vec<Vec<Term>>),
    Unparsed(String),
}

impl VersionReq {
    /// Parse a Fabric/Quilt-style predicate.
    pub fn parse(text: &str) -> Self {
        Self::parse_alternatives(text, false)
    }

    /// Parse a Forge/NeoForge-style Maven range.
    pub fn parse_maven(text: &str) -> Self {
        Self::parse_alternatives(text, true)
    }

    fn parse_alternatives(text: &str, maven: bool) -> Self {
        let trimmed = text.trim();
        if trimmed.is_empty() || trimmed == "*" || trimmed.eq_ignore_ascii_case("any") {
            return VersionReq {
                kind: ReqKind::Any(vec![vec![Term::Any]]),
            };
        }
        let mut alternatives = Vec::new();
        for alternative in trimmed.split("||") {
            match parse_conjunction(alternative.trim(), maven) {
                Some(terms) => alternatives.push(terms),
                None => {
                    return VersionReq {
                        kind: ReqKind::Unparsed(text.to_string()),
                    }
                }
            }
        }
        VersionReq {
            kind: ReqKind::Any(alternatives),
        }
    }

    pub fn matches(&self, version: &Version) -> bool {
        match &self.kind {
            // Unparseable: never claim an incompatibility we cannot prove.
            ReqKind::Unparsed(_) => true,
            ReqKind::Any(alternatives) => alternatives
                .iter()
                .any(|terms| terms.iter().all(|t| t.matches(version))),
        }
    }

    /// Match a version string, tolerating unparseable versions the same
    /// permissive way.
    pub fn matches_str(&self, version: &str) -> bool {
        match Version::parse(version) {
            Some(v) => self.matches(&v),
            None => true,
        }
    }

    pub fn is_unparsed(&self) -> bool {
        matches!(self.kind, ReqKind::Unparsed(_))
    }

    /// The original requirement text when it could not be parsed, for
    /// telling the user exactly which predicate was not understood.
    pub fn unparsed_text(&self) -> Option<&str> {
        match &self.kind {
            ReqKind::Unparsed(text) => Some(text),
            ReqKind::Any(_) => None,
        }
    }
}

fn parse_conjunction(text: &str, maven: bool) -> Option<Vec<Term>> {
    if text.is_empty() || text == "*" {
        return Some(vec![Term::Any]);
    }
    if maven && (text.starts_with('[') || text.starts_with('(')) {
        return parse_maven_interval(text).map(|t| vec![t]);
    }
    let mut terms = Vec::new();
    for piece in text.split([',', ' ']).filter(|p| !p.trim().is_empty()) {
        terms.push(parse_predicate(piece.trim(), maven)?);
    }
    if terms.is_empty() {
        return Some(vec![Term::Any]);
    }
    Some(terms)
}

fn parse_maven_interval(text: &str) -> Option<Term> {
    let low_inclusive = text.starts_with('[');
    let high_inclusive = text.ends_with(']');
    if !(text.starts_with('[') || text.starts_with('(')) {
        return None;
    }
    if !(text.ends_with(']') || text.ends_with(')')) {
        return None;
    }
    let inner = &text[1..text.len() - 1];
    let (low_text, high_text) = match inner.split_once(',') {
        Some((l, h)) => (l.trim(), h.trim()),
        // `[1.0]` — a single pinned version.
        None => {
            let v = Version::parse(inner.trim())?;
            return Some(Term::Compare(Op::Eq, v));
        }
    };
    let low = if low_text.is_empty() {
        None
    } else {
        Some(Version::parse(low_text)?)
    };
    let high = if high_text.is_empty() {
        None
    } else {
        Some(Version::parse(high_text)?)
    };
    Some(Term::Window {
        low,
        low_inclusive,
        high,
        high_inclusive,
    })
}

fn parse_predicate(text: &str, maven: bool) -> Option<Term> {
    if text == "*" {
        return Some(Term::Any);
    }
    // `!=` is rare and negation is not modeled; treat it as unconstrained
    // rather than risk a false verdict.
    if let Some(_rest) = text.strip_prefix("!=") {
        return Some(Term::Any);
    }
    for (prefix, op) in [
        (">=", Op::Gte),
        ("<=", Op::Lte),
        (">", Op::Gt),
        ("<", Op::Lt),
        ("=", Op::Eq),
    ] {
        if let Some(rest) = text.strip_prefix(prefix) {
            let v = Version::parse(rest)?;
            return Some(Term::Compare(op, v));
        }
    }
    if let Some(rest) = text.strip_prefix('^') {
        // ^1.2.3 → >=1.2.3, <2.0.0
        let low = Version::parse(rest)?;
        let high = Version {
            parts: vec![low.component(0) + 1],
            pre: None,
        };
        return Some(Term::Window {
            low: Some(low),
            low_inclusive: true,
            high: Some(high),
            high_inclusive: false,
        });
    }
    if let Some(rest) = text.strip_prefix('~') {
        // ~1.2.3 → >=1.2.3, <1.3.0
        let low = Version::parse(rest)?;
        let high = Version {
            parts: vec![low.component(0), low.component(1) + 1],
            pre: None,
        };
        return Some(Term::Window {
            low: Some(low),
            low_inclusive: true,
            high: Some(high),
            high_inclusive: false,
        });
    }
    // `1.2.x` / `1.2.*` → >=1.2, <1.3
    let lowered = text.to_ascii_lowercase();
    if lowered.ends_with(".x") || lowered.ends_with(".*") {
        let base = &text[..text.len() - 2];
        let low = Version::parse(base)?;
        let mut high_parts = low.parts.clone();
        if let Some(last) = high_parts.last_mut() {
            *last += 1;
        }
        return Some(Term::Window {
            low: Some(low),
            low_inclusive: true,
            high: Some(Version {
                parts: high_parts,
                pre: None,
            }),
            high_inclusive: false,
        });
    }

    let exact = Version::parse(text)?;
    if maven {
        // Forge reads a bare version as a minimum, not a pin.
        Some(Term::Compare(Op::Gte, exact))
    } else {
        Some(Term::Compare(Op::Eq, exact))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> Version {
        Version::parse(text).expect("valid version")
    }

    #[test]
    fn versions_compare_numerically_not_lexically() {
        assert!(v("1.10.0") > v("1.9.0"), "10 > 9 numerically");
        assert!(v("1.20.1") > v("1.20"));
        assert!(v("2.0") > v("1.99.99"));
        assert_eq!(v("1.2"), v("1.2.0"), "missing components are zero");
    }

    #[test]
    fn prerelease_sorts_before_release() {
        assert!(v("1.0.0-beta.1") < v("1.0.0"));
        assert!(v("1.0.0-alpha") < v("1.0.0-beta"));
        assert!(
            v("1.0.0-beta.2") < v("1.0.0-beta.10"),
            "numeric identifiers"
        );
        assert!(
            v("1.0.0-beta.10") < v("1.0.0-rc"),
            "numbers sort below words"
        );
        assert!(v("1.0.0-beta") < v("1.0.0-beta.1"), "a prefix sorts first");
    }

    #[test]
    fn build_metadata_is_not_part_of_the_version() {
        assert_eq!(v("0.9.1+mc26.2"), v("0.9.1"));
        assert!(VersionReq::parse(">=0.9.1").matches(&v("0.9.1+mc26.2")));
        assert!(VersionReq::parse(">=2.0.0").matches(&v("6.0.4+06488ac19e")));
        assert_eq!(v("1.0.0-beta.1+build.5"), v("1.0.0-beta.1"));
        assert_eq!(v("1.0.0+x").to_string(), "1.0.0");
    }

    #[test]
    fn a_trailing_dash_admits_prereleases_of_that_version() {
        // Fabric API: `"minecraft": "~26.2-"`; Dynamic FPS: `">=26.2.0-"`.
        let tilde = VersionReq::parse("~26.2-");
        assert!(tilde.matches(&v("26.2")));
        assert!(tilde.matches(&v("26.2-rc.1")));
        assert!(tilde.matches(&v("26.2.1")));
        assert!(!tilde.matches(&v("26.3")));
        let gte = VersionReq::parse(">=26.2.0-");
        assert!(gte.matches(&v("26.2")));
        assert!(gte.matches(&v("26.2-pre.1")));
        assert!(!gte.matches(&v("26.1.9")));
    }

    #[test]
    fn fabric_style_predicates() {
        assert!(VersionReq::parse("*").matches(&v("9.9.9")));
        assert!(VersionReq::parse(">=1.20").matches(&v("1.20.4")));
        assert!(!VersionReq::parse(">=1.20").matches(&v("1.19.4")));
        assert!(VersionReq::parse("<1.21").matches(&v("1.20.6")));
        assert!(VersionReq::parse("1.20.1").matches(&v("1.20.1")));
        assert!(!VersionReq::parse("1.20.1").matches(&v("1.20.2")));
    }

    #[test]
    fn caret_tilde_and_wildcard_windows() {
        let caret = VersionReq::parse("^1.2.0");
        assert!(caret.matches(&v("1.2.0")));
        assert!(caret.matches(&v("1.9.9")));
        assert!(!caret.matches(&v("2.0.0")));

        let tilde = VersionReq::parse("~1.2.0");
        assert!(tilde.matches(&v("1.2.9")));
        assert!(!tilde.matches(&v("1.3.0")));

        let wild = VersionReq::parse("1.20.x");
        assert!(wild.matches(&v("1.20.6")));
        assert!(!wild.matches(&v("1.21.0")));
    }

    #[test]
    fn conjunctions_and_alternatives() {
        let both = VersionReq::parse(">=1.20 <1.21");
        assert!(both.matches(&v("1.20.4")));
        assert!(!both.matches(&v("1.21.0")));
        assert!(!both.matches(&v("1.19.0")));

        let either = VersionReq::parse(">=1.21 || 1.20.1");
        assert!(either.matches(&v("1.21.3")));
        assert!(either.matches(&v("1.20.1")));
        assert!(!either.matches(&v("1.19.2")));
    }

    #[test]
    fn maven_intervals() {
        let window = VersionReq::parse_maven("[1.20,1.21)");
        assert!(window.matches(&v("1.20.0")));
        assert!(window.matches(&v("1.20.9")));
        assert!(!window.matches(&v("1.21.0")));

        let open = VersionReq::parse_maven("[47,)");
        assert!(open.matches(&v("47.1.0")));
        assert!(open.matches(&v("100.0")));
        assert!(!open.matches(&v("46.9")));

        let pinned = VersionReq::parse_maven("[1.0]");
        assert!(pinned.matches(&v("1.0")));
        assert!(!pinned.matches(&v("1.1")));

        // A bare version in Forge-land means "at least".
        let bare = VersionReq::parse_maven("47.1.0");
        assert!(bare.matches(&v("47.2.0")));
        assert!(!bare.matches(&v("47.0.0")));
    }

    #[test]
    fn unparseable_requirements_are_permissive_but_flagged() {
        let weird = VersionReq::parse("!!! nonsense @@@");
        assert!(
            weird.is_unparsed(),
            "should be reported, not silently dropped"
        );
        assert!(
            weird.matches(&v("1.0.0")),
            "must not claim a conflict it cannot prove"
        );
    }

    #[test]
    fn unparseable_versions_do_not_fail_a_check() {
        assert!(VersionReq::parse(">=1.20").matches_str("not-a-version"));
    }
}
