//! Strict SemVer 2.0.0 版本与受限版本范围（ADR-004 决策 3）。
//!
//! 范围语法只覆盖插件生态实际需要的子集：
//! 精确、`^`、`~`、`>=`/`>`/`<=`/`<`/`=`、空格分隔的合取、`1.x`/`1.2.x`/`*`。
//! **解析失败一律拒绝**——绝不"解析失败就放行"。

use std::cmp::Ordering;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Identifier {
    Numeric(u64),
    Alpha(String),
}

impl Identifier {
    fn parse(raw: &str) -> Option<Self> {
        if raw.is_empty() || !raw.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return None;
        }
        if raw.chars().all(|c| c.is_ascii_digit()) {
            // 数字标识符不允许前导零（SemVer 2.0.0 §9）。
            if raw.len() > 1 && raw.starts_with('0') {
                return None;
            }
            raw.parse().ok().map(Identifier::Numeric)
        } else {
            Some(Identifier::Alpha(raw.to_string()))
        }
    }
}

impl Ord for Identifier {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Identifier::Numeric(a), Identifier::Numeric(b)) => a.cmp(b),
            (Identifier::Numeric(_), Identifier::Alpha(_)) => Ordering::Less,
            (Identifier::Alpha(_), Identifier::Numeric(_)) => Ordering::Greater,
            (Identifier::Alpha(a), Identifier::Alpha(b)) => a.cmp(b),
        }
    }
}

impl PartialOrd for Identifier {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// 严格 SemVer 版本。`build` 参与序列化但**不参与比较**（SemVer §10）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pre: Vec<Identifier>,
    build: Vec<String>,
}

impl Version {
    pub fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self {
            major,
            minor,
            patch,
            pre: Vec::new(),
            build: Vec::new(),
        }
    }

    /// 解析 `MAJOR.MINOR.PATCH[-pre][+build]`；三段必须齐全。
    pub fn parse(input: &str) -> Option<Self> {
        let input = input.trim();
        if input.is_empty() || input.len() > 64 {
            return None;
        }
        let (head, build) = match input.split_once('+') {
            Some((head, build)) => (head, Some(build)),
            None => (input, None),
        };
        let (core, pre) = match head.split_once('-') {
            Some((core, pre)) => (core, Some(pre)),
            None => (head, None),
        };

        let mut parts = core.split('.');
        let major = parse_number(parts.next()?)?;
        let minor = parse_number(parts.next()?)?;
        let patch = parse_number(parts.next()?)?;
        if parts.next().is_some() {
            return None;
        }

        let pre = match pre {
            None => Vec::new(),
            Some(raw) => {
                if raw.is_empty() {
                    return None;
                }
                let mut out = Vec::new();
                for item in raw.split('.') {
                    out.push(Identifier::parse(item)?);
                }
                out
            }
        };

        let build = match build {
            None => Vec::new(),
            Some(raw) => {
                if raw.is_empty() {
                    return None;
                }
                let mut out = Vec::new();
                for item in raw.split('.') {
                    if item.is_empty()
                        || !item.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
                    {
                        return None;
                    }
                    out.push(item.to_string());
                }
                out
            }
        };

        Some(Self {
            major,
            minor,
            patch,
            pre,
            build,
        })
    }

    pub fn is_prerelease(&self) -> bool {
        !self.pre.is_empty()
    }
}

fn parse_number(raw: &str) -> Option<u64> {
    if raw.is_empty() || !raw.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    if raw.len() > 1 && raw.starts_with('0') {
        return None;
    }
    raw.parse().ok()
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if !self.pre.is_empty() {
            let pre: Vec<String> = self
                .pre
                .iter()
                .map(|i| match i {
                    Identifier::Numeric(n) => n.to_string(),
                    Identifier::Alpha(s) => s.clone(),
                })
                .collect();
            write!(f, "-{}", pre.join("."))?;
        }
        if !self.build.is_empty() {
            write!(f, "+{}", self.build.join("."))?;
        }
        Ok(())
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.major
            .cmp(&other.major)
            .then_with(|| self.minor.cmp(&other.minor))
            .then_with(|| self.patch.cmp(&other.patch))
            .then_with(|| match (self.pre.is_empty(), other.pre.is_empty()) {
                (true, true) => Ordering::Equal,
                // 有 prerelease 的版本小于正式版。
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                (false, false) => {
                    for (a, b) in self.pre.iter().zip(other.pre.iter()) {
                        let ord = a.cmp(b);
                        if ord != Ordering::Equal {
                            return ord;
                        }
                    }
                    self.pre.len().cmp(&other.pre.len())
                }
            })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Exact,
    Greater,
    GreaterEq,
    Less,
    LessEq,
    Caret,
    Tilde,
}

#[derive(Debug, Clone)]
struct Comparator {
    op: Op,
    major: u64,
    minor: Option<u64>,
    patch: Option<u64>,
    pre: Vec<Identifier>,
}

impl Comparator {
    fn matches(&self, v: &Version) -> bool {
        use Ordering::*;
        match self.op {
            Op::Exact => {
                v.major == self.major
                    && self.minor.map(|m| v.minor == m).unwrap_or(true)
                    && self.patch.map(|p| v.patch == p).unwrap_or(true)
                    && (self.pre.is_empty() || cmp_pre(&self.pre, &v.pre) == Equal)
            }
            Op::Greater => v.cmp(&self.lower_bound()) == Greater,
            Op::GreaterEq => v.cmp(&self.lower_bound()) != Less,
            Op::Less => v.cmp(&self.lower_bound()) == Less,
            Op::LessEq => v.cmp(&self.lower_bound()) != Greater,
            Op::Caret => {
                let lower = self.lower_bound();
                let upper = self.caret_upper();
                v.cmp(&lower) != Less && v.cmp(&upper) == Less
            }
            Op::Tilde => {
                let lower = self.lower_bound();
                let upper = self.tilde_upper();
                v.cmp(&lower) != Less && v.cmp(&upper) == Less
            }
        }
    }

    fn lower_bound(&self) -> Version {
        Version {
            major: self.major,
            minor: self.minor.unwrap_or(0),
            patch: self.patch.unwrap_or(0),
            pre: self.pre.clone(),
            build: Vec::new(),
        }
    }

    fn caret_upper(&self) -> Version {
        match (self.major, self.minor, self.patch) {
            (0, Some(0), Some(p)) => Version::new(0, 0, p + 1),
            (0, Some(m), _) => Version::new(0, m + 1, 0),
            (major, _, _) => Version::new(major + 1, 0, 0),
        }
    }

    fn tilde_upper(&self) -> Version {
        match self.minor {
            Some(m) => Version::new(self.major, m + 1, 0),
            None => Version::new(self.major + 1, 0, 0),
        }
    }
}

fn cmp_pre(a: &[Identifier], b: &[Identifier]) -> Ordering {
    for (x, y) in a.iter().zip(b.iter()) {
        let ord = x.cmp(y);
        if ord != Ordering::Equal {
            return ord;
        }
    }
    a.len().cmp(&b.len())
}

/// 版本范围：多个比较器之间是逻辑与。
#[derive(Debug, Clone)]
pub struct VersionReq {
    raw: String,
    comparators: Vec<Comparator>,
}

impl VersionReq {
    pub fn parse(input: &str) -> Option<Self> {
        let raw = input.trim();
        if raw.is_empty() || raw.len() > 128 {
            return None;
        }
        if raw == "*" {
            return Some(Self {
                raw: raw.to_string(),
                comparators: Vec::new(),
            });
        }

        // 先按空白切分；单独出现的运算符与其后的版本合并（`>= 1.0.0`）。
        let mut tokens: Vec<String> = Vec::new();
        let mut pending_op: Option<String> = None;
        for piece in raw.split_whitespace() {
            if matches!(piece, ">=" | ">" | "<=" | "<" | "=" | "^" | "~") {
                if pending_op.is_some() {
                    return None;
                }
                pending_op = Some(piece.to_string());
                continue;
            }
            if let Some(op) = pending_op.take() {
                tokens.push(format!("{op}{piece}"));
            } else {
                tokens.push(piece.to_string());
            }
        }
        if pending_op.is_some() {
            return None;
        }

        let mut comparators = Vec::new();
        for token in tokens {
            comparators.push(parse_comparator(&token)?);
        }
        if comparators.is_empty() {
            return None;
        }
        Some(Self {
            raw: raw.to_string(),
            comparators,
        })
    }

    pub fn matches(&self, version: &Version) -> bool {
        if self.comparators.is_empty() {
            return true;
        }
        // 标准 SemVer 行为：范围未显式包含 prerelease 时，prerelease 不匹配。
        if version.is_prerelease() && !self.comparators.iter().any(|c| !c.pre.is_empty()) {
            return false;
        }
        self.comparators.iter().all(|c| c.matches(version))
    }

    pub fn as_str(&self) -> &str {
        &self.raw
    }
}

fn parse_comparator(token: &str) -> Option<Comparator> {
    let (op, rest) = if let Some(rest) = token.strip_prefix(">=") {
        (Op::GreaterEq, rest)
    } else if let Some(rest) = token.strip_prefix("<=") {
        (Op::LessEq, rest)
    } else if let Some(rest) = token.strip_prefix('>') {
        (Op::Greater, rest)
    } else if let Some(rest) = token.strip_prefix('<') {
        (Op::Less, rest)
    } else if let Some(rest) = token.strip_prefix('^') {
        (Op::Caret, rest)
    } else if let Some(rest) = token.strip_prefix('~') {
        (Op::Tilde, rest)
    } else if let Some(rest) = token.strip_prefix('=') {
        (Op::Exact, rest)
    } else {
        (Op::Exact, token)
    };

    let rest = rest.trim();
    if rest.is_empty() {
        return None;
    }

    let (core, pre) = match rest.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (rest, None),
    };
    let parts: Vec<&str> = core.split('.').collect();
    if parts.len() > 3 {
        return None;
    }

    let is_wild = |s: &str| s == "x" || s == "X" || s == "*";
    let mut nums: [Option<u64>; 3] = [None, None, None];
    let mut saw_wild = false;
    for (idx, part) in parts.iter().enumerate() {
        if is_wild(part) {
            saw_wild = true;
            // 通配符之后不允许再出现数字（`1.x.2` 是非法写法）。
            if parts[idx + 1..].iter().any(|p| !is_wild(p)) {
                return None;
            }
            break;
        }
        nums[idx] = Some(parse_number(part)?);
    }
    let major = nums[0]?;

    if pre.is_some() && (saw_wild || nums[2].is_none()) {
        return None;
    }
    let pre = match pre {
        None => Vec::new(),
        Some(raw) => {
            if raw.is_empty() {
                return None;
            }
            let mut out = Vec::new();
            for item in raw.split('.') {
                out.push(Identifier::parse(item)?);
            }
            out
        }
    };

    // `1.x` / `1` → 主版本通配；`1.2.x` / `1.2` → 次版本通配。
    //
    // 通配写法忽略 `^`/`~` 的原始语义：`1.x` 就是 `<2.0.0`，`1.2.x` 就是 `<1.3.0`。
    // 未写通配但缺 patch 时保留运算符语义：`^1.2` = `<2.0.0`，`~1.2` = `<1.3.0`。
    if saw_wild || nums[2].is_none() {
        let wildcard_op = if nums[1].is_none() {
            Op::Caret
        } else {
            Op::Tilde
        };
        let op = if saw_wild {
            wildcard_op
        } else {
            match op {
                Op::Exact => wildcard_op,
                Op::Caret | Op::Tilde => op,
                _ => return None,
            }
        };
        return Some(Comparator {
            op,
            major,
            minor: nums[1],
            patch: None,
            pre: Vec::new(),
        });
    }

    Some(Comparator {
        op,
        major,
        minor: nums[1],
        patch: nums[2],
        pre,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    #[test]
    fn parses_valid_semver() {
        assert_eq!(v("1.2.3").to_string(), "1.2.3");
        assert_eq!(v("0.1.0").major, 0);
        assert_eq!(v("1.0.0-alpha.1").to_string(), "1.0.0-alpha.1");
        assert_eq!(v("1.0.0+build.5").to_string(), "1.0.0+build.5");
        assert_eq!(
            v("1.0.0-rc.1+exp.sha.5114f85").to_string(),
            "1.0.0-rc.1+exp.sha.5114f85"
        );
    }

    #[test]
    fn rejects_invalid_semver() {
        for bad in [
            "1.2",
            "1",
            "v1.2.3",
            "01.2.3",
            "1.02.3",
            "1.2.03",
            "1.2.3-",
            "1.2.3+",
            "1.2.3.4",
            "1.2.3-alpha..1",
            "1.2.3-01",
            "1.2.3-α",
            "",
        ] {
            assert!(Version::parse(bad).is_none(), "should reject {bad:?}");
        }
    }

    #[test]
    fn orders_prereleases_per_spec() {
        let ordered = [
            "1.0.0-alpha",
            "1.0.0-alpha.1",
            "1.0.0-alpha.beta",
            "1.0.0-beta",
            "1.0.0-beta.2",
            "1.0.0-beta.11",
            "1.0.0-rc.1",
            "1.0.0",
        ];
        for pair in ordered.windows(2) {
            assert!(
                v(pair[0]) < v(pair[1]),
                "{} should be < {}",
                pair[0],
                pair[1]
            );
        }
        assert_eq!(
            v("1.0.0+a").cmp(&v("1.0.0+b")),
            Ordering::Equal,
            "build metadata 不参与优先级比较"
        );
    }

    #[test]
    fn range_caret_and_tilde() {
        let caret = VersionReq::parse("^0.1.2").unwrap();
        assert!(caret.matches(&v("0.1.2")));
        assert!(caret.matches(&v("0.1.99")));
        assert!(!caret.matches(&v("0.2.0")));

        let caret0 = VersionReq::parse("^0.0.3").unwrap();
        assert!(caret0.matches(&v("0.0.3")));
        assert!(!caret0.matches(&v("0.0.4")));

        let tilde = VersionReq::parse("~1.2.3").unwrap();
        assert!(tilde.matches(&v("1.2.9")));
        assert!(!tilde.matches(&v("1.3.0")));
    }

    #[test]
    fn range_conjunction_and_wildcards() {
        let range = VersionReq::parse(">=0.1.0 <0.2.0").unwrap();
        assert!(range.matches(&v("0.1.0")));
        assert!(range.matches(&v("0.1.9")));
        assert!(!range.matches(&v("0.2.0")));
        assert!(!range.matches(&v("0.0.9")));

        let major = VersionReq::parse("1.x").unwrap();
        assert!(major.matches(&v("1.0.0")));
        assert!(major.matches(&v("1.99.0")));
        assert!(!major.matches(&v("2.0.0")));

        let minor = VersionReq::parse("1.2.x").unwrap();
        assert!(minor.matches(&v("1.2.7")));
        assert!(!minor.matches(&v("1.3.0")));

        assert!(VersionReq::parse("*").unwrap().matches(&v("9.9.9")));
    }

    #[test]
    fn range_rejects_garbage() {
        for bad in [
            "",
            ">=",
            "^",
            "1.2.3 <",
            "1.x.2",
            ">=1.0.0, <2.0.0",
            "1.0.0 || 2.0.0",
            "one",
        ] {
            assert!(VersionReq::parse(bad).is_none(), "should reject {bad:?}");
        }
    }

    #[test]
    fn prerelease_only_matches_with_explicit_prerelease() {
        let stable = VersionReq::parse(">=1.0.0 <2.0.0").unwrap();
        assert!(!stable.matches(&v("1.1.0-beta")));
        let explicit = VersionReq::parse(">=1.1.0-beta <2.0.0").unwrap();
        assert!(explicit.matches(&v("1.1.0-beta")));
    }
}
