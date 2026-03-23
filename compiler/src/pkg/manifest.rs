// ===============================================================================
// QUANTALANG PACKAGE MANIFEST
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Package manifest parsing (Quanta.toml).

use super::version::{Version, VersionReq, VersionError};
use std::collections::HashMap;
use std::path::Path;

// =============================================================================
// MANIFEST
// =============================================================================

/// Package manifest (Quanta.toml).
#[derive(Debug, Clone)]
pub struct Manifest {
    /// Package metadata.
    pub package: Package,
    /// Dependencies.
    pub dependencies: HashMap<String, Dependency>,
    /// Dev dependencies.
    pub dev_dependencies: HashMap<String, Dependency>,
    /// Build dependencies.
    pub build_dependencies: HashMap<String, Dependency>,
    /// Features.
    pub features: HashMap<String, Vec<String>>,
    /// Default features.
    pub default_features: Vec<String>,
    /// Workspace configuration.
    pub workspace: Option<Workspace>,
    /// Binary targets.
    pub bin: Vec<Target>,
    /// Library target.
    pub lib: Option<Target>,
    /// Example targets.
    pub example: Vec<Target>,
    /// Test targets.
    pub test: Vec<Target>,
    /// Bench targets.
    pub bench: Vec<Target>,
}

impl Manifest {
    /// Create a new manifest with default values.
    pub fn new(name: impl Into<String>, version: Version) -> Self {
        Self {
            package: Package::new(name, version),
            dependencies: HashMap::new(),
            dev_dependencies: HashMap::new(),
            build_dependencies: HashMap::new(),
            features: HashMap::new(),
            default_features: Vec::new(),
            workspace: None,
            bin: Vec::new(),
            lib: None,
            example: Vec::new(),
            test: Vec::new(),
            bench: Vec::new(),
        }
    }

    /// Add a dependency.
    pub fn add_dependency(&mut self, name: impl Into<String>, dep: Dependency) {
        self.dependencies.insert(name.into(), dep);
    }

    /// Add a dev dependency.
    pub fn add_dev_dependency(&mut self, name: impl Into<String>, dep: Dependency) {
        self.dev_dependencies.insert(name.into(), dep);
    }

    /// Parse manifest from a TOML string.
    pub fn from_str(s: &str) -> Result<Self, ManifestError> {
        parse_manifest(s)
    }

    /// Parse manifest from a file.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, ManifestError> {
        let content = std::fs::read_to_string(path.as_ref())
            .map_err(|e| ManifestError::Io(e.to_string()))?;
        Self::from_str(&content)
    }

    /// Serialize to TOML string.
    pub fn to_toml(&self) -> String {
        let mut output = String::new();

        // [package]
        output.push_str("[package]\n");
        output.push_str(&format!("name = \"{}\"\n", self.package.name));
        output.push_str(&format!("version = \"{}\"\n", self.package.version));
        if !self.package.authors.is_empty() {
            output.push_str(&format!(
                "authors = [{}]\n",
                self.package
                    .authors
                    .iter()
                    .map(|a| format!("\"{}\"", a))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if let Some(ref edition) = self.package.edition {
            output.push_str(&format!("edition = \"{}\"\n", edition));
        }
        if let Some(ref desc) = self.package.description {
            output.push_str(&format!("description = \"{}\"\n", desc));
        }
        if let Some(ref license) = self.package.license {
            output.push_str(&format!("license = \"{}\"\n", license));
        }
        if let Some(ref repo) = self.package.repository {
            output.push_str(&format!("repository = \"{}\"\n", repo));
        }
        output.push('\n');

        // [dependencies]
        if !self.dependencies.is_empty() {
            output.push_str("[dependencies]\n");
            for (name, dep) in &self.dependencies {
                output.push_str(&dep.to_toml_line(name));
            }
            output.push('\n');
        }

        // [dev-dependencies]
        if !self.dev_dependencies.is_empty() {
            output.push_str("[dev-dependencies]\n");
            for (name, dep) in &self.dev_dependencies {
                output.push_str(&dep.to_toml_line(name));
            }
            output.push('\n');
        }

        // [features]
        if !self.features.is_empty() {
            output.push_str("[features]\n");
            if !self.default_features.is_empty() {
                output.push_str(&format!(
                    "default = [{}]\n",
                    self.default_features
                        .iter()
                        .map(|f| format!("\"{}\"", f))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            for (name, deps) in &self.features {
                if name != "default" {
                    output.push_str(&format!(
                        "{} = [{}]\n",
                        name,
                        deps.iter()
                            .map(|d| format!("\"{}\"", d))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
            }
            output.push('\n');
        }

        output
    }
}

/// Package metadata.
#[derive(Debug, Clone)]
pub struct Package {
    /// Package name.
    pub name: String,
    /// Package version.
    pub version: Version,
    /// Authors.
    pub authors: Vec<String>,
    /// Edition (e.g., "2025").
    pub edition: Option<String>,
    /// Description.
    pub description: Option<String>,
    /// License identifier.
    pub license: Option<String>,
    /// License file path.
    pub license_file: Option<String>,
    /// Repository URL.
    pub repository: Option<String>,
    /// Homepage URL.
    pub homepage: Option<String>,
    /// Documentation URL.
    pub documentation: Option<String>,
    /// Readme file path.
    pub readme: Option<String>,
    /// Keywords.
    pub keywords: Vec<String>,
    /// Categories.
    pub categories: Vec<String>,
    /// Exclude patterns.
    pub exclude: Vec<String>,
    /// Include patterns.
    pub include: Vec<String>,
    /// Whether to publish.
    pub publish: bool,
}

impl Package {
    /// Create a new package.
    pub fn new(name: impl Into<String>, version: Version) -> Self {
        Self {
            name: name.into(),
            version,
            authors: Vec::new(),
            edition: Some("2025".to_string()),
            description: None,
            license: None,
            license_file: None,
            repository: None,
            homepage: None,
            documentation: None,
            readme: None,
            keywords: Vec::new(),
            categories: Vec::new(),
            exclude: Vec::new(),
            include: Vec::new(),
            publish: true,
        }
    }
}

// =============================================================================
// DEPENDENCY
// =============================================================================

/// A dependency specification.
#[derive(Debug, Clone)]
pub struct Dependency {
    /// Version requirement.
    pub version: Option<VersionReq>,
    /// Git repository URL.
    pub git: Option<String>,
    /// Git branch.
    pub branch: Option<String>,
    /// Git tag.
    pub tag: Option<String>,
    /// Git revision.
    pub rev: Option<String>,
    /// Local path.
    pub path: Option<String>,
    /// Registry name.
    pub registry: Option<String>,
    /// Features to enable.
    pub features: Vec<String>,
    /// Whether to use default features.
    pub default_features: bool,
    /// Whether this is optional.
    pub optional: bool,
    /// Package name (if different from key).
    pub package: Option<String>,
}

impl Dependency {
    /// Create a version dependency.
    pub fn version(req: impl Into<String>) -> Result<Self, VersionError> {
        let req_str = req.into();
        let version_req = req_str.parse()?;
        Ok(Self {
            version: Some(version_req),
            git: None,
            branch: None,
            tag: None,
            rev: None,
            path: None,
            registry: None,
            features: Vec::new(),
            default_features: true,
            optional: false,
            package: None,
        })
    }

    /// Create a git dependency.
    pub fn git(url: impl Into<String>) -> Self {
        Self {
            version: None,
            git: Some(url.into()),
            branch: None,
            tag: None,
            rev: None,
            path: None,
            registry: None,
            features: Vec::new(),
            default_features: true,
            optional: false,
            package: None,
        }
    }

    /// Create a path dependency.
    pub fn path(path: impl Into<String>) -> Self {
        Self {
            version: None,
            git: None,
            branch: None,
            tag: None,
            rev: None,
            path: Some(path.into()),
            registry: None,
            features: Vec::new(),
            default_features: true,
            optional: false,
            package: None,
        }
    }

    /// Add a feature.
    pub fn with_feature(mut self, feature: impl Into<String>) -> Self {
        self.features.push(feature.into());
        self
    }

    /// Set features.
    pub fn with_features(mut self, features: Vec<String>) -> Self {
        self.features = features;
        self
    }

    /// Disable default features.
    pub fn no_default_features(mut self) -> Self {
        self.default_features = false;
        self
    }

    /// Mark as optional.
    pub fn optional(mut self) -> Self {
        self.optional = true;
        self
    }

    /// Set git branch.
    pub fn with_branch(mut self, branch: impl Into<String>) -> Self {
        self.branch = Some(branch.into());
        self
    }

    /// Set git tag.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = Some(tag.into());
        self
    }

    /// Convert to TOML line.
    fn to_toml_line(&self, name: &str) -> String {
        if let Some(ref v) = self.version {
            if self.features.is_empty() && self.default_features && !self.optional {
                // Simple version
                return format!("{} = \"{}\"\n", name, v);
            }
        }

        // Complex dependency
        let mut parts = Vec::new();

        if let Some(ref v) = self.version {
            parts.push(format!("version = \"{}\"", v));
        }
        if let Some(ref git) = self.git {
            parts.push(format!("git = \"{}\"", git));
        }
        if let Some(ref branch) = self.branch {
            parts.push(format!("branch = \"{}\"", branch));
        }
        if let Some(ref tag) = self.tag {
            parts.push(format!("tag = \"{}\"", tag));
        }
        if let Some(ref path) = self.path {
            parts.push(format!("path = \"{}\"", path));
        }
        if !self.features.is_empty() {
            parts.push(format!(
                "features = [{}]",
                self.features
                    .iter()
                    .map(|f| format!("\"{}\"", f))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if !self.default_features {
            parts.push("default-features = false".to_string());
        }
        if self.optional {
            parts.push("optional = true".to_string());
        }

        format!("{} = {{ {} }}\n", name, parts.join(", "))
    }
}

// =============================================================================
// WORKSPACE
// =============================================================================

/// Workspace configuration.
#[derive(Debug, Clone)]
pub struct Workspace {
    /// Workspace members.
    pub members: Vec<String>,
    /// Excluded members.
    pub exclude: Vec<String>,
    /// Default members.
    pub default_members: Vec<String>,
    /// Resolver version.
    pub resolver: Option<String>,
}

// =============================================================================
// TARGET
// =============================================================================

/// Build target configuration.
#[derive(Debug, Clone)]
pub struct Target {
    /// Target name.
    pub name: String,
    /// Source path.
    pub path: Option<String>,
    /// Required features.
    pub required_features: Vec<String>,
    /// Whether to run tests.
    pub test: bool,
    /// Whether to run doc tests.
    pub doctest: bool,
    /// Whether to build benchmarks.
    pub bench: bool,
    /// Whether to build documentation.
    pub doc: bool,
    /// Edition override.
    pub edition: Option<String>,
}

impl Target {
    /// Create a new target.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            path: None,
            required_features: Vec::new(),
            test: true,
            doctest: true,
            bench: true,
            doc: true,
            edition: None,
        }
    }
}

// =============================================================================
// PARSING
// =============================================================================

/// Parse a manifest from TOML string.
fn parse_manifest(s: &str) -> Result<Manifest, ManifestError> {
    let mut manifest = Manifest {
        package: Package::new("unknown", Version::new(0, 1, 0)),
        dependencies: HashMap::new(),
        dev_dependencies: HashMap::new(),
        build_dependencies: HashMap::new(),
        features: HashMap::new(),
        default_features: Vec::new(),
        workspace: None,
        bin: Vec::new(),
        lib: None,
        example: Vec::new(),
        test: Vec::new(),
        bench: Vec::new(),
    };

    let mut current_section = String::new();

    for line in s.lines() {
        let line = line.trim();

        // Skip comments and empty lines
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Section header
        if line.starts_with('[') && line.ends_with(']') {
            current_section = line[1..line.len() - 1].to_string();
            continue;
        }

        // Key = value
        if let Some(eq_pos) = line.find('=') {
            let key = line[..eq_pos].trim();
            let value = line[eq_pos + 1..].trim();

            match current_section.as_str() {
                "package" => {
                    parse_package_field(&mut manifest.package, key, value)?;
                }
                "dependencies" => {
                    let dep = parse_dependency(value)?;
                    manifest.dependencies.insert(key.to_string(), dep);
                }
                "dev-dependencies" => {
                    let dep = parse_dependency(value)?;
                    manifest.dev_dependencies.insert(key.to_string(), dep);
                }
                "build-dependencies" => {
                    let dep = parse_dependency(value)?;
                    manifest.build_dependencies.insert(key.to_string(), dep);
                }
                "features" => {
                    let features = parse_string_array(value)?;
                    if key == "default" {
                        manifest.default_features = features;
                    } else {
                        manifest.features.insert(key.to_string(), features);
                    }
                }
                _ => {}
            }
        }
    }

    Ok(manifest)
}

/// Parse a package field.
fn parse_package_field(pkg: &mut Package, key: &str, value: &str) -> Result<(), ManifestError> {
    let value = value.trim_matches('"');

    match key {
        "name" => pkg.name = value.to_string(),
        "version" => {
            pkg.version = value
                .parse()
                .map_err(|e: VersionError| ManifestError::Parse(e.to_string()))?;
        }
        "authors" => pkg.authors = parse_string_array(value)?,
        "edition" => pkg.edition = Some(value.to_string()),
        "description" => pkg.description = Some(value.to_string()),
        "license" => pkg.license = Some(value.to_string()),
        "license-file" => pkg.license_file = Some(value.to_string()),
        "repository" => pkg.repository = Some(value.to_string()),
        "homepage" => pkg.homepage = Some(value.to_string()),
        "documentation" => pkg.documentation = Some(value.to_string()),
        "readme" => pkg.readme = Some(value.to_string()),
        "keywords" => pkg.keywords = parse_string_array(value)?,
        "categories" => pkg.categories = parse_string_array(value)?,
        "publish" => pkg.publish = value == "true",
        _ => {}
    }

    Ok(())
}

/// Parse a dependency value.
fn parse_dependency(value: &str) -> Result<Dependency, ManifestError> {
    let value = value.trim();

    // Simple version string
    if value.starts_with('"') && value.ends_with('"') {
        let version_str = &value[1..value.len() - 1];
        return Dependency::version(version_str)
            .map_err(|e| ManifestError::Parse(e.to_string()));
    }

    // Complex dependency { ... }
    if value.starts_with('{') && value.ends_with('}') {
        let inner = &value[1..value.len() - 1];
        let mut dep = Dependency {
            version: None,
            git: None,
            branch: None,
            tag: None,
            rev: None,
            path: None,
            registry: None,
            features: Vec::new(),
            default_features: true,
            optional: false,
            package: None,
        };

        for part in inner.split(',') {
            let part = part.trim();
            if let Some(eq_pos) = part.find('=') {
                let key = part[..eq_pos].trim();
                let val = part[eq_pos + 1..].trim().trim_matches('"');

                match key {
                    "version" => {
                        dep.version = Some(
                            val.parse()
                                .map_err(|e: VersionError| ManifestError::Parse(e.to_string()))?,
                        );
                    }
                    "git" => dep.git = Some(val.to_string()),
                    "branch" => dep.branch = Some(val.to_string()),
                    "tag" => dep.tag = Some(val.to_string()),
                    "rev" => dep.rev = Some(val.to_string()),
                    "path" => dep.path = Some(val.to_string()),
                    "features" => dep.features = parse_string_array(val)?,
                    "default-features" => dep.default_features = val == "true",
                    "optional" => dep.optional = val == "true",
                    "package" => dep.package = Some(val.to_string()),
                    _ => {}
                }
            }
        }

        return Ok(dep);
    }

    // Default: treat as version
    Dependency::version(value).map_err(|e| ManifestError::Parse(e.to_string()))
}

/// Parse a string array.
fn parse_string_array(value: &str) -> Result<Vec<String>, ManifestError> {
    let value = value.trim();

    if !value.starts_with('[') || !value.ends_with(']') {
        return Ok(vec![value.trim_matches('"').to_string()]);
    }

    let inner = &value[1..value.len() - 1];
    Ok(inner
        .split(',')
        .map(|s| s.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .collect())
}

// =============================================================================
// ERRORS
// =============================================================================

/// Manifest error.
#[derive(Debug)]
pub enum ManifestError {
    /// I/O error.
    Io(String),
    /// Parse error.
    Parse(String),
    /// Missing required field.
    MissingField(String),
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ManifestError::Io(e) => write!(f, "I/O error: {}", e),
            ManifestError::Parse(e) => write!(f, "parse error: {}", e),
            ManifestError::MissingField(field) => write!(f, "missing field: {}", field),
        }
    }
}

impl std::error::Error for ManifestError {}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_manifest() {
        let toml = r#"
[package]
name = "my-package"
version = "1.0.0"

[dependencies]
serde = "1.0"
"#;

        let manifest = Manifest::from_str(toml).unwrap();
        assert_eq!(manifest.package.name, "my-package");
        assert_eq!(manifest.package.version, Version::new(1, 0, 0));
        assert!(manifest.dependencies.contains_key("serde"));
    }

    #[test]
    fn test_dependency_version() {
        let dep = Dependency::version("^1.0.0").unwrap();
        assert!(dep.version.is_some());
    }

    #[test]
    fn test_dependency_git() {
        let dep = Dependency::git("https://github.com/user/repo")
            .with_branch("main")
            .with_features(vec!["feature1".to_string()]);
        assert_eq!(dep.git, Some("https://github.com/user/repo".to_string()));
        assert_eq!(dep.branch, Some("main".to_string()));
        assert_eq!(dep.features.len(), 1);
    }
}
