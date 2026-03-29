// ===============================================================================
// QUANTALANG LOCKFILE
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Lockfile handling for reproducible builds.
//!
//! The lockfile (`Quanta.lock`) records exact versions of all dependencies
//! to ensure reproducible builds across different machines and times.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Write as FmtWrite};
use std::io;
use std::path::Path;

use super::{Version, Resolution, ResolvedPackage};

/// Lockfile format version
const LOCKFILE_VERSION: u32 = 1;

/// Lockfile name
pub const LOCKFILE_NAME: &str = "Quanta.lock";

/// Lockfile error types
#[derive(Debug)]
pub enum LockfileError {
    /// IO error
    Io(io::Error),
    /// Parse error
    Parse(String),
    /// Version mismatch
    VersionMismatch { expected: u32, found: u32 },
    /// Integrity error
    IntegrityError(String),
    /// Formatting error
    Fmt(fmt::Error),
}

impl fmt::Display for LockfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "IO error: {}", e),
            Self::Parse(msg) => write!(f, "parse error: {}", msg),
            Self::VersionMismatch { expected, found } => {
                write!(f, "lockfile version mismatch: expected {}, found {}", expected, found)
            }
            Self::IntegrityError(msg) => write!(f, "integrity error: {}", msg),
            Self::Fmt(e) => write!(f, "formatting error: {}", e),
        }
    }
}

impl std::error::Error for LockfileError {}

impl From<io::Error> for LockfileError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<fmt::Error> for LockfileError {
    fn from(e: fmt::Error) -> Self {
        Self::Fmt(e)
    }
}

/// A locked package entry
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockedPackage {
    /// Package name
    pub name: String,
    /// Exact version
    pub version: Version,
    /// Source (registry, git, path)
    pub source: PackageSource,
    /// Checksum for verification
    pub checksum: Option<String>,
    /// Dependencies with their locked versions
    pub dependencies: BTreeMap<String, Version>,
    /// Enabled features
    pub features: BTreeSet<String>,
}

/// Package source
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageSource {
    /// Registry package
    Registry {
        registry: String,
    },
    /// Git repository
    Git {
        url: String,
        branch: Option<String>,
        tag: Option<String>,
        rev: String,
    },
    /// Local path
    Path {
        path: String,
    },
}

impl fmt::Display for PackageSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Registry { registry } => write!(f, "registry+{}", registry),
            Self::Git { url, rev, .. } => write!(f, "git+{}#{}", url, rev),
            Self::Path { path } => write!(f, "path+{}", path),
        }
    }
}

impl PackageSource {
    /// Parse source string
    pub fn parse(s: &str) -> Result<Self, LockfileError> {
        if let Some(registry) = s.strip_prefix("registry+") {
            Ok(Self::Registry { registry: registry.to_string() })
        } else if let Some(rest) = s.strip_prefix("git+") {
            if let Some((url, rev)) = rest.split_once('#') {
                Ok(Self::Git {
                    url: url.to_string(),
                    branch: None,
                    tag: None,
                    rev: rev.to_string(),
                })
            } else {
                Err(LockfileError::Parse(format!("invalid git source: {}", s)))
            }
        } else if let Some(path) = s.strip_prefix("path+") {
            Ok(Self::Path { path: path.to_string() })
        } else {
            Err(LockfileError::Parse(format!("unknown source type: {}", s)))
        }
    }
}

/// The lockfile
#[derive(Debug, Clone)]
pub struct Lockfile {
    /// Lockfile format version
    pub version: u32,
    /// Root package name
    pub root: String,
    /// All locked packages
    pub packages: BTreeMap<String, LockedPackage>,
    /// Metadata (arbitrary key-value pairs)
    pub metadata: BTreeMap<String, String>,
}

impl Lockfile {
    /// Create new lockfile from resolution
    pub fn from_resolution(resolution: &Resolution) -> Self {
        let mut packages = BTreeMap::new();

        for (name, pkg) in &resolution.packages {
            let locked = LockedPackage {
                name: name.clone(),
                version: pkg.version.clone(),
                source: PackageSource::Registry {
                    registry: "https://registry.quantalang.org".to_string(),
                },
                checksum: None,
                dependencies: pkg.dependencies.clone(),
                features: pkg.features.clone(),
            };
            packages.insert(name.clone(), locked);
        }

        Self {
            version: LOCKFILE_VERSION,
            root: resolution.root.name.clone(),
            packages,
            metadata: BTreeMap::new(),
        }
    }

    /// Load lockfile from path
    pub fn load(path: &Path) -> Result<Self, LockfileError> {
        let content = std::fs::read_to_string(path)?;
        Self::parse(&content)
    }

    /// Save lockfile to path
    pub fn save(&self, path: &Path) -> Result<(), LockfileError> {
        let content = self.serialize()?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Parse lockfile from string
    pub fn parse(content: &str) -> Result<Self, LockfileError> {
        let mut lockfile = Lockfile {
            version: 0,
            root: String::new(),
            packages: BTreeMap::new(),
            metadata: BTreeMap::new(),
        };

        let mut current_section: Option<&str> = None;
        let mut current_package: Option<LockedPackage> = None;
        let mut in_dependencies = false;
        let mut in_features = false;

        for (line_num, line) in content.lines().enumerate() {
            let line = line.trim();

            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Section header
            if line.starts_with('[') && line.ends_with(']') {
                // Save previous package
                if let Some(pkg) = current_package.take() {
                    lockfile.packages.insert(pkg.name.clone(), pkg);
                }
                in_dependencies = false;
                in_features = false;

                let section = &line[1..line.len()-1];

                if section == "lockfile" {
                    current_section = Some("lockfile");
                } else if section == "metadata" {
                    current_section = Some("metadata");
                } else if let Some(name) = section.strip_prefix("package.") {
                    current_section = Some("package");
                    current_package = Some(LockedPackage {
                        name: name.to_string(),
                        version: Version::new(0, 0, 0),
                        source: PackageSource::Registry {
                            registry: "https://registry.quantalang.org".to_string(),
                        },
                        checksum: None,
                        dependencies: BTreeMap::new(),
                        features: BTreeSet::new(),
                    });
                } else if section == "dependencies" {
                    in_dependencies = true;
                } else if section == "features" {
                    in_features = true;
                } else {
                    return Err(LockfileError::Parse(
                        format!("unknown section '{}' at line {}", section, line_num + 1)
                    ));
                }
                continue;
            }

            // Key-value pair
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim().trim_matches('"');

                match current_section {
                    Some("lockfile") => {
                        if key == "version" {
                            lockfile.version = value.parse()
                                .map_err(|_| LockfileError::Parse(
                                    format!("invalid version at line {}", line_num + 1)
                                ))?;
                        } else if key == "root" {
                            lockfile.root = value.to_string();
                        }
                    }
                    Some("metadata") => {
                        lockfile.metadata.insert(key.to_string(), value.to_string());
                    }
                    Some("package") => {
                        if let Some(ref mut pkg) = current_package {
                            if in_dependencies {
                                let ver = Version::parse(value)
                                    .map_err(|e| LockfileError::Parse(
                                        format!("invalid version '{}' at line {}: {}", value, line_num + 1, e)
                                    ))?;
                                pkg.dependencies.insert(key.to_string(), ver);
                            } else if in_features {
                                // Features are stored as feature = "true" or listed
                                pkg.features.insert(key.to_string());
                            } else {
                                match key {
                                    "version" => {
                                        pkg.version = Version::parse(value)
                                            .map_err(|e| LockfileError::Parse(
                                                format!("invalid version at line {}: {}", line_num + 1, e)
                                            ))?;
                                    }
                                    "source" => {
                                        pkg.source = PackageSource::parse(value)?;
                                    }
                                    "checksum" => {
                                        pkg.checksum = Some(value.to_string());
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        // Save last package
        if let Some(pkg) = current_package {
            lockfile.packages.insert(pkg.name.clone(), pkg);
        }

        // Validate version
        if lockfile.version != LOCKFILE_VERSION {
            return Err(LockfileError::VersionMismatch {
                expected: LOCKFILE_VERSION,
                found: lockfile.version,
            });
        }

        Ok(lockfile)
    }

    /// Serialize to TOML-like string
    pub fn serialize(&self) -> Result<String, LockfileError> {
        let mut output = String::new();

        // Header
        writeln!(output, "# This file is automatically generated by quanta-pkg.")?;
        writeln!(output, "# Do not edit manually.")?;
        writeln!(output)?;

        // Lockfile section
        writeln!(output, "[lockfile]")?;
        writeln!(output, "version = {}", self.version)?;
        writeln!(output, "root = \"{}\"", self.root)?;
        writeln!(output)?;

        // Packages (sorted for determinism)
        for (name, pkg) in &self.packages {
            writeln!(output, "[package.{}]", name)?;
            writeln!(output, "version = \"{}\"", pkg.version)?;
            writeln!(output, "source = \"{}\"", pkg.source)?;

            if let Some(checksum) = &pkg.checksum {
                writeln!(output, "checksum = \"{}\"", checksum)?;
            }

            if !pkg.dependencies.is_empty() {
                writeln!(output)?;
                writeln!(output, "[dependencies]")?;
                for (dep_name, dep_ver) in &pkg.dependencies {
                    writeln!(output, "{} = \"{}\"", dep_name, dep_ver)?;
                }
            }

            if !pkg.features.is_empty() {
                writeln!(output)?;
                writeln!(output, "[features]")?;
                for feature in &pkg.features {
                    writeln!(output, "{} = true", feature)?;
                }
            }

            writeln!(output)?;
        }

        // Metadata
        if !self.metadata.is_empty() {
            writeln!(output, "[metadata]")?;
            for (key, value) in &self.metadata {
                writeln!(output, "{} = \"{}\"", key, value)?;
            }
        }

        Ok(output)
    }

    /// Check if lockfile is up to date with manifest
    pub fn is_up_to_date(&self, resolution: &Resolution) -> bool {
        // Check all packages match
        if self.packages.len() != resolution.packages.len() {
            return false;
        }

        for (name, locked) in &self.packages {
            match resolution.packages.get(name) {
                Some(resolved) if locked.version == resolved.version => {}
                _ => return false,
            }
        }

        true
    }

    /// Get locked version for a package
    pub fn get_version(&self, name: &str) -> Option<&Version> {
        self.packages.get(name).map(|p| &p.version)
    }

    /// Convert back to resolution
    pub fn to_resolution(&self) -> Resolution {
        use super::DependencyGraph;

        let mut packages = BTreeMap::new();
        let mut graph = DependencyGraph::new();

        for (name, locked) in &self.packages {
            let resolved = ResolvedPackage {
                name: name.clone(),
                version: locked.version.clone(),
                features: locked.features.clone(),
                dependencies: locked.dependencies.clone(),
                is_dev: false,
            };

            for dep_name in locked.dependencies.keys() {
                graph.add_edge(name, dep_name);
            }

            packages.insert(name.clone(), resolved);
        }

        let root = ResolvedPackage {
            name: self.root.clone(),
            version: Version::new(0, 0, 0),
            features: BTreeSet::new(),
            dependencies: BTreeMap::new(),
            is_dev: false,
        };

        Resolution { root, packages, graph }
    }

    /// Merge with another lockfile (for workspace support)
    pub fn merge(&mut self, other: &Lockfile) -> Result<(), LockfileError> {
        for (name, pkg) in &other.packages {
            if let Some(existing) = self.packages.get(name) {
                if existing.version != pkg.version {
                    return Err(LockfileError::IntegrityError(format!(
                        "conflicting versions for '{}': {} vs {}",
                        name, existing.version, pkg.version
                    )));
                }
            } else {
                self.packages.insert(name.clone(), pkg.clone());
            }
        }

        for (key, value) in &other.metadata {
            self.metadata.entry(key.clone()).or_insert_with(|| value.clone());
        }

        Ok(())
    }

    /// Diff with another lockfile
    pub fn diff(&self, other: &Lockfile) -> LockfileDiff {
        let mut added = Vec::new();
        let mut removed = Vec::new();
        let mut updated = Vec::new();

        for (name, pkg) in &other.packages {
            match self.packages.get(name) {
                None => added.push((name.clone(), pkg.version.clone())),
                Some(old) if old.version != pkg.version => {
                    updated.push((name.clone(), old.version.clone(), pkg.version.clone()));
                }
                _ => {}
            }
        }

        for (name, pkg) in &self.packages {
            if !other.packages.contains_key(name) {
                removed.push((name.clone(), pkg.version.clone()));
            }
        }

        LockfileDiff { added, removed, updated }
    }
}

/// Difference between two lockfiles
#[derive(Debug, Clone)]
pub struct LockfileDiff {
    /// Newly added packages
    pub added: Vec<(String, Version)>,
    /// Removed packages
    pub removed: Vec<(String, Version)>,
    /// Updated packages (name, old_version, new_version)
    pub updated: Vec<(String, Version, Version)>,
}

impl LockfileDiff {
    /// Check if there are any changes
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.updated.is_empty()
    }

    /// Get total number of changes
    pub fn len(&self) -> usize {
        self.added.len() + self.removed.len() + self.updated.len()
    }
}

impl fmt::Display for LockfileDiff {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_empty() {
            return writeln!(f, "No changes");
        }

        for (name, version) in &self.added {
            writeln!(f, "+ {} v{}", name, version)?;
        }

        for (name, version) in &self.removed {
            writeln!(f, "- {} v{}", name, version)?;
        }

        for (name, old, new) in &self.updated {
            writeln!(f, "~ {} v{} -> v{}", name, old, new)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_package_source_parse() -> Result<(), LockfileError> {
        let registry = PackageSource::parse("registry+https://registry.quantalang.org")?;
        assert!(matches!(registry, PackageSource::Registry { .. }));

        let git = PackageSource::parse("git+https://github.com/user/repo#abc123")?;
        assert!(matches!(git, PackageSource::Git { rev, .. } if rev == "abc123"));

        let path = PackageSource::parse("path+../local-pkg")?;
        assert!(matches!(path, PackageSource::Path { path } if path == "../local-pkg"));

        Ok(())
    }

    #[test]
    fn test_lockfile_roundtrip() -> Result<(), LockfileError> {
        let mut lockfile = Lockfile {
            version: LOCKFILE_VERSION,
            root: "my-project".to_string(),
            packages: BTreeMap::new(),
            metadata: BTreeMap::new(),
        };

        lockfile.packages.insert("dep-a".to_string(), LockedPackage {
            name: "dep-a".to_string(),
            version: Version::new(1, 2, 3),
            source: PackageSource::Registry {
                registry: "https://registry.quantalang.org".to_string(),
            },
            checksum: Some("abc123".to_string()),
            dependencies: BTreeMap::new(),
            features: BTreeSet::new(),
        });

        let serialized = lockfile.serialize()?;
        assert!(serialized.contains("dep-a"));
        assert!(serialized.contains("1.2.3"));

        Ok(())
    }

    #[test]
    fn test_lockfile_diff() {
        let mut old = Lockfile {
            version: LOCKFILE_VERSION,
            root: "test".to_string(),
            packages: BTreeMap::new(),
            metadata: BTreeMap::new(),
        };

        old.packages.insert("a".to_string(), LockedPackage {
            name: "a".to_string(),
            version: Version::new(1, 0, 0),
            source: PackageSource::Registry { registry: "r".to_string() },
            checksum: None,
            dependencies: BTreeMap::new(),
            features: BTreeSet::new(),
        });

        old.packages.insert("b".to_string(), LockedPackage {
            name: "b".to_string(),
            version: Version::new(1, 0, 0),
            source: PackageSource::Registry { registry: "r".to_string() },
            checksum: None,
            dependencies: BTreeMap::new(),
            features: BTreeSet::new(),
        });

        let mut new = old.clone();
        new.packages.remove("a");
        if let Some(pkg_b) = new.packages.get_mut("b") {
            pkg_b.version = Version::new(2, 0, 0);
        }
        new.packages.insert("c".to_string(), LockedPackage {
            name: "c".to_string(),
            version: Version::new(1, 0, 0),
            source: PackageSource::Registry { registry: "r".to_string() },
            checksum: None,
            dependencies: BTreeMap::new(),
            features: BTreeSet::new(),
        });

        let diff = old.diff(&new);
        assert_eq!(diff.added.len(), 1);
        assert_eq!(diff.removed.len(), 1);
        assert_eq!(diff.updated.len(), 1);
    }
}
