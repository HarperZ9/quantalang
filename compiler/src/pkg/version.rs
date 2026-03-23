// ===============================================================================
// QUANTALANG SEMANTIC VERSIONING
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Semantic versioning implementation (SemVer 2.0.0).

use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

// =============================================================================
// VERSION
// =============================================================================

/// A semantic version (major.minor.patch-prerelease+build).
#[derive(Debug, Clone, Eq)]
pub struct Version {
    /// Major version (breaking changes).
    pub major: u64,
    /// Minor version (backwards-compatible features).
    pub minor: u64,
    /// Patch version (backwards-compatible fixes).
    pub patch: u64,
    /// Pre-release identifiers (e.g., alpha, beta, rc.1).
    pub pre: Vec<PreRelease>,
    /// Build metadata (e.g., build.123).
    pub build: Vec<String>,
}

impl Version {
    /// Create a new version.
    pub fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self {
            major,
            minor,
            patch,
            pre: Vec::new(),
            build: Vec::new(),
        }
    }

    /// Create version 0.0.0.
    pub fn zero() -> Self {
        Self::new(0, 0, 0)
    }

    /// Add pre-release identifier.
    pub fn with_pre(mut self, pre: impl Into<String>) -> Self {
        let s = pre.into();
        if let Ok(n) = s.parse::<u64>() {
            self.pre.push(PreRelease::Numeric(n));
        } else {
            self.pre.push(PreRelease::Alphanumeric(s));
        }
        self
    }

    /// Add build metadata.
    pub fn with_build(mut self, build: impl Into<String>) -> Self {
        self.build.push(build.into());
        self
    }

    /// Check if this is a pre-release version.
    pub fn is_prerelease(&self) -> bool {
        !self.pre.is_empty()
    }

    /// Get the next major version.
    pub fn next_major(&self) -> Self {
        Self::new(self.major + 1, 0, 0)
    }

    /// Get the next minor version.
    pub fn next_minor(&self) -> Self {
        Self::new(self.major, self.minor + 1, 0)
    }

    /// Get the next patch version.
    pub fn next_patch(&self) -> Self {
        Self::new(self.major, self.minor, self.patch + 1)
    }

    /// Parse a version string.
    pub fn parse(s: &str) -> Result<Self, VersionError> {
        parse_version(s)
    }
}

impl Default for Version {
    fn default() -> Self {
        Self::new(0, 1, 0)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if !self.pre.is_empty() {
            write!(f, "-")?;
            for (i, p) in self.pre.iter().enumerate() {
                if i > 0 {
                    write!(f, ".")?;
                }
                write!(f, "{}", p)?;
            }
        }
        if !self.build.is_empty() {
            write!(f, "+")?;
            for (i, b) in self.build.iter().enumerate() {
                if i > 0 {
                    write!(f, ".")?;
                }
                write!(f, "{}", b)?;
            }
        }
        Ok(())
    }
}

impl FromStr for Version {
    type Err = VersionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_version(s)
    }
}

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        // Build metadata is ignored in equality
        self.major == other.major
            && self.minor == other.minor
            && self.patch == other.patch
            && self.pre == other.pre
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        // Compare major.minor.patch
        match self.major.cmp(&other.major) {
            Ordering::Equal => {}
            ord => return ord,
        }
        match self.minor.cmp(&other.minor) {
            Ordering::Equal => {}
            ord => return ord,
        }
        match self.patch.cmp(&other.patch) {
            Ordering::Equal => {}
            ord => return ord,
        }

        // Pre-release handling
        match (self.pre.is_empty(), other.pre.is_empty()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater, // No pre > has pre
            (false, true) => Ordering::Less,
            (false, false) => {
                // Compare pre-release identifiers
                for (a, b) in self.pre.iter().zip(other.pre.iter()) {
                    match a.cmp(b) {
                        Ordering::Equal => continue,
                        ord => return ord,
                    }
                }
                self.pre.len().cmp(&other.pre.len())
            }
        }
    }
}

/// Pre-release identifier.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum PreRelease {
    /// Numeric identifier.
    Numeric(u64),
    /// Alphanumeric identifier.
    Alphanumeric(String),
}

impl fmt::Display for PreRelease {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PreRelease::Numeric(n) => write!(f, "{}", n),
            PreRelease::Alphanumeric(s) => write!(f, "{}", s),
        }
    }
}

impl Ord for PreRelease {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            // Numeric < Alphanumeric
            (PreRelease::Numeric(a), PreRelease::Numeric(b)) => a.cmp(b),
            (PreRelease::Alphanumeric(a), PreRelease::Alphanumeric(b)) => a.cmp(b),
            (PreRelease::Numeric(_), PreRelease::Alphanumeric(_)) => Ordering::Less,
            (PreRelease::Alphanumeric(_), PreRelease::Numeric(_)) => Ordering::Greater,
        }
    }
}

impl PartialOrd for PreRelease {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

// =============================================================================
// VERSION REQUIREMENT
// =============================================================================

/// A version requirement (constraint).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionReq {
    /// Exact version match.
    Exact(Version),
    /// Greater than.
    Greater(Version),
    /// Greater than or equal.
    GreaterEq(Version),
    /// Less than.
    Less(Version),
    /// Less than or equal.
    LessEq(Version),
    /// Tilde requirement (~1.2.3 means >=1.2.3 and <1.3.0).
    Tilde(Version),
    /// Caret requirement (^1.2.3 means >=1.2.3 and <2.0.0).
    Caret(Version),
    /// Wildcard (1.2.* means >=1.2.0 and <1.3.0).
    Wildcard(u64, Option<u64>),
    /// Range (>=1.0.0, <2.0.0).
    Range(Box<VersionReq>, Box<VersionReq>),
    /// Any version.
    Any,
}

impl VersionReq {
    /// Check if a version matches this requirement.
    pub fn matches(&self, version: &Version) -> bool {
        match self {
            VersionReq::Exact(v) => version == v,
            VersionReq::Greater(v) => version > v,
            VersionReq::GreaterEq(v) => version >= v,
            VersionReq::Less(v) => version < v,
            VersionReq::LessEq(v) => version <= v,
            VersionReq::Tilde(v) => {
                version >= v && version.major == v.major && version.minor == v.minor
            }
            VersionReq::Caret(v) => {
                if v.major == 0 {
                    if v.minor == 0 {
                        // ^0.0.x means =0.0.x
                        version.major == 0 && version.minor == 0 && version.patch == v.patch
                    } else {
                        // ^0.y.z means >=0.y.z and <0.(y+1).0
                        version.major == 0 && version.minor == v.minor && version >= v
                    }
                } else {
                    // ^x.y.z means >=x.y.z and <(x+1).0.0
                    version >= v && version.major == v.major
                }
            }
            VersionReq::Wildcard(major, minor) => {
                if let Some(minor) = minor {
                    version.major == *major && version.minor == *minor
                } else {
                    version.major == *major
                }
            }
            VersionReq::Range(a, b) => a.matches(version) && b.matches(version),
            VersionReq::Any => true,
        }
    }

    /// Create a caret requirement.
    pub fn caret(version: Version) -> Self {
        VersionReq::Caret(version)
    }

    /// Create a tilde requirement.
    pub fn tilde(version: Version) -> Self {
        VersionReq::Tilde(version)
    }

    /// Create an exact requirement.
    pub fn exact(version: Version) -> Self {
        VersionReq::Exact(version)
    }
}

impl Default for VersionReq {
    fn default() -> Self {
        VersionReq::Any
    }
}

impl fmt::Display for VersionReq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VersionReq::Exact(v) => write!(f, "={}", v),
            VersionReq::Greater(v) => write!(f, ">{}", v),
            VersionReq::GreaterEq(v) => write!(f, ">={}", v),
            VersionReq::Less(v) => write!(f, "<{}", v),
            VersionReq::LessEq(v) => write!(f, "<={}", v),
            VersionReq::Tilde(v) => write!(f, "~{}", v),
            VersionReq::Caret(v) => write!(f, "^{}", v),
            VersionReq::Wildcard(major, minor) => {
                if let Some(minor) = minor {
                    write!(f, "{}.{}.*", major, minor)
                } else {
                    write!(f, "{}.*", major)
                }
            }
            VersionReq::Range(a, b) => write!(f, "{}, {}", a, b),
            VersionReq::Any => write!(f, "*"),
        }
    }
}

impl FromStr for VersionReq {
    type Err = VersionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_version_req(s)
    }
}

// =============================================================================
// PARSING
// =============================================================================

/// Parse a version string.
pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    let s = s.trim();
    if s.is_empty() {
        return Err(VersionError::Empty);
    }

    // Split off build metadata
    let (version_pre, build) = if let Some(pos) = s.find('+') {
        (&s[..pos], Some(&s[pos + 1..]))
    } else {
        (s, None)
    };

    // Split off pre-release
    let (version, pre) = if let Some(pos) = version_pre.find('-') {
        (&version_pre[..pos], Some(&version_pre[pos + 1..]))
    } else {
        (version_pre, None)
    };

    // Parse major.minor.patch
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() < 2 || parts.len() > 3 {
        return Err(VersionError::InvalidFormat(s.to_string()));
    }

    let major = parts[0]
        .parse()
        .map_err(|_| VersionError::InvalidNumber(parts[0].to_string()))?;
    let minor = parts[1]
        .parse()
        .map_err(|_| VersionError::InvalidNumber(parts[1].to_string()))?;
    let patch = if parts.len() > 2 {
        parts[2]
            .parse()
            .map_err(|_| VersionError::InvalidNumber(parts[2].to_string()))?
    } else {
        0
    };

    let mut version = Version::new(major, minor, patch);

    // Parse pre-release
    if let Some(pre_str) = pre {
        for part in pre_str.split('.') {
            if let Ok(n) = part.parse::<u64>() {
                version.pre.push(PreRelease::Numeric(n));
            } else {
                version.pre.push(PreRelease::Alphanumeric(part.to_string()));
            }
        }
    }

    // Parse build
    if let Some(build_str) = build {
        for part in build_str.split('.') {
            version.build.push(part.to_string());
        }
    }

    Ok(version)
}

/// Parse a version requirement string.
pub fn parse_version_req(s: &str) -> Result<VersionReq, VersionError> {
    let s = s.trim();
    if s.is_empty() || s == "*" {
        return Ok(VersionReq::Any);
    }

    // Check for comma-separated requirements
    if s.contains(',') {
        let parts: Vec<&str> = s.split(',').collect();
        if parts.len() == 2 {
            let a = parse_version_req(parts[0].trim())?;
            let b = parse_version_req(parts[1].trim())?;
            return Ok(VersionReq::Range(Box::new(a), Box::new(b)));
        }
        return Err(VersionError::InvalidFormat(s.to_string()));
    }

    // Check for operators
    if let Some(rest) = s.strip_prefix(">=") {
        return Ok(VersionReq::GreaterEq(parse_version(rest.trim())?));
    }
    if let Some(rest) = s.strip_prefix("<=") {
        return Ok(VersionReq::LessEq(parse_version(rest.trim())?));
    }
    if let Some(rest) = s.strip_prefix('>') {
        return Ok(VersionReq::Greater(parse_version(rest.trim())?));
    }
    if let Some(rest) = s.strip_prefix('<') {
        return Ok(VersionReq::Less(parse_version(rest.trim())?));
    }
    if let Some(rest) = s.strip_prefix('=') {
        return Ok(VersionReq::Exact(parse_version(rest.trim())?));
    }
    if let Some(rest) = s.strip_prefix('~') {
        return Ok(VersionReq::Tilde(parse_version(rest.trim())?));
    }
    if let Some(rest) = s.strip_prefix('^') {
        return Ok(VersionReq::Caret(parse_version(rest.trim())?));
    }

    // Check for wildcard
    if s.ends_with(".*") || s.ends_with(".x") || s.ends_with(".X") {
        let parts: Vec<&str> = s.split('.').collect();
        let major = parts[0]
            .parse()
            .map_err(|_| VersionError::InvalidNumber(parts[0].to_string()))?;
        let minor = if parts.len() > 1 && parts[1] != "*" && parts[1] != "x" && parts[1] != "X" {
            Some(
                parts[1]
                    .parse()
                    .map_err(|_| VersionError::InvalidNumber(parts[1].to_string()))?,
            )
        } else {
            None
        };
        return Ok(VersionReq::Wildcard(major, minor));
    }

    // Default to caret
    Ok(VersionReq::Caret(parse_version(s)?))
}

// =============================================================================
// ERRORS
// =============================================================================

/// Version parsing error.
#[derive(Debug, Clone)]
pub enum VersionError {
    /// Empty version string.
    Empty,
    /// Invalid version format.
    InvalidFormat(String),
    /// Invalid number in version.
    InvalidNumber(String),
}

impl fmt::Display for VersionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VersionError::Empty => write!(f, "empty version string"),
            VersionError::InvalidFormat(s) => write!(f, "invalid version format: {}", s),
            VersionError::InvalidNumber(s) => write!(f, "invalid number in version: {}", s),
        }
    }
}

impl std::error::Error for VersionError {}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_parse() {
        let v: Version = "1.2.3".parse().unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
    }

    #[test]
    fn test_version_prerelease() {
        let v: Version = "1.0.0-alpha.1".parse().unwrap();
        assert_eq!(v.pre.len(), 2);
        assert_eq!(v.pre[0], PreRelease::Alphanumeric("alpha".to_string()));
        assert_eq!(v.pre[1], PreRelease::Numeric(1));
    }

    #[test]
    fn test_version_ordering() {
        let v1: Version = "1.0.0".parse().unwrap();
        let v2: Version = "2.0.0".parse().unwrap();
        let v3: Version = "1.0.0-alpha".parse().unwrap();

        assert!(v1 < v2);
        assert!(v3 < v1); // Pre-release < release
    }

    #[test]
    fn test_caret_matches() {
        let req = VersionReq::caret("1.2.3".parse().unwrap());
        assert!(req.matches(&"1.2.3".parse().unwrap()));
        assert!(req.matches(&"1.5.0".parse().unwrap()));
        assert!(!req.matches(&"2.0.0".parse().unwrap()));
        assert!(!req.matches(&"1.2.2".parse().unwrap()));
    }

    #[test]
    fn test_tilde_matches() {
        let req = VersionReq::tilde("1.2.3".parse().unwrap());
        assert!(req.matches(&"1.2.3".parse().unwrap()));
        assert!(req.matches(&"1.2.5".parse().unwrap()));
        assert!(!req.matches(&"1.3.0".parse().unwrap()));
    }
}
