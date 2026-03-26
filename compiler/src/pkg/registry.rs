// ===============================================================================
// QUANTALANG PACKAGE REGISTRY CLIENT
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Registry client for downloading and publishing packages.
//!
//! Supports:
//! - Default Quanta registry (registry.quantalang.org)
//! - Custom registries
//! - Git repositories
//! - Local paths

use std::collections::HashMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use super::{Manifest, Version, VersionReq};

/// Registry configuration
#[derive(Debug, Clone)]
pub struct RegistryConfig {
    /// Registry URL
    pub url: String,
    /// Authentication token
    pub token: Option<String>,
    /// Cache directory
    pub cache_dir: PathBuf,
    /// Request timeout
    pub timeout: Duration,
    /// Maximum concurrent downloads
    pub max_concurrent: usize,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            url: "https://registry.quantalang.org".to_string(),
            token: None,
            cache_dir: dirs_cache().join("quanta").join("registry"),
            timeout: Duration::from_secs(30),
            max_concurrent: 4,
        }
    }
}

/// Get cache directory
fn dirs_cache() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        std::env::var("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("C:\\Users\\Default\\AppData\\Local"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                std::env::var("HOME")
                    .map(|h| PathBuf::from(h).join(".cache"))
                    .unwrap_or_else(|_| PathBuf::from("/tmp"))
            })
    }
}

/// Registry error types
#[derive(Debug)]
pub enum RegistryError {
    /// Network error
    Network(String),
    /// Package not found
    NotFound(String),
    /// Version not found
    VersionNotFound(String, String),
    /// Authentication required
    AuthRequired,
    /// Authentication failed
    AuthFailed,
    /// Rate limited
    RateLimited(Duration),
    /// Invalid response
    InvalidResponse(String),
    /// Cache error
    CacheError(String),
    /// IO error
    Io(io::Error),
    /// Checksum mismatch
    ChecksumMismatch { expected: String, actual: String },
    /// Package yanked
    Yanked(String, Version),
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network(msg) => write!(f, "network error: {}", msg),
            Self::NotFound(name) => write!(f, "package '{}' not found", name),
            Self::VersionNotFound(name, ver) => {
                write!(f, "version {} of package '{}' not found", ver, name)
            }
            Self::AuthRequired => write!(f, "authentication required"),
            Self::AuthFailed => write!(f, "authentication failed"),
            Self::RateLimited(dur) => write!(f, "rate limited, retry after {:?}", dur),
            Self::InvalidResponse(msg) => write!(f, "invalid response: {}", msg),
            Self::CacheError(msg) => write!(f, "cache error: {}", msg),
            Self::Io(e) => write!(f, "IO error: {}", e),
            Self::ChecksumMismatch { expected, actual } => {
                write!(f, "checksum mismatch: expected {}, got {}", expected, actual)
            }
            Self::Yanked(name, ver) => write!(f, "package {}@{} has been yanked", name, ver),
        }
    }
}

impl std::error::Error for RegistryError {}

impl From<io::Error> for RegistryError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// Package metadata from registry
#[derive(Debug, Clone)]
pub struct PackageMetadata {
    /// Package name
    pub name: String,
    /// Description
    pub description: Option<String>,
    /// Repository URL
    pub repository: Option<String>,
    /// Documentation URL
    pub documentation: Option<String>,
    /// Homepage
    pub homepage: Option<String>,
    /// Keywords
    pub keywords: Vec<String>,
    /// Categories
    pub categories: Vec<String>,
    /// License
    pub license: Option<String>,
    /// All published versions
    pub versions: Vec<VersionInfo>,
    /// Download count
    pub downloads: u64,
    /// Created timestamp
    pub created_at: Option<SystemTime>,
    /// Updated timestamp
    pub updated_at: Option<SystemTime>,
}

/// Information about a specific version
#[derive(Debug, Clone)]
pub struct VersionInfo {
    /// Version number
    pub version: Version,
    /// Dependencies
    pub dependencies: HashMap<String, VersionReq>,
    /// Dev dependencies
    pub dev_dependencies: HashMap<String, VersionReq>,
    /// Features
    pub features: HashMap<String, Vec<String>>,
    /// Checksum (SHA256)
    pub checksum: String,
    /// Tarball size
    pub size: u64,
    /// Whether version is yanked
    pub yanked: bool,
    /// Published timestamp
    pub published_at: Option<SystemTime>,
    /// Minimum Quanta version required
    pub quanta_version: Option<VersionReq>,
}

/// Downloaded package
#[derive(Debug)]
pub struct DownloadedPackage {
    /// Package name
    pub name: String,
    /// Version
    pub version: Version,
    /// Path to extracted package
    pub path: PathBuf,
    /// Manifest
    pub manifest: Manifest,
}

/// Registry client
pub struct Registry {
    config: RegistryConfig,
    cache: PackageCache,
}

impl Registry {
    /// Create new registry client
    pub fn new(config: RegistryConfig) -> Self {
        let cache = PackageCache::new(config.cache_dir.clone());
        Self { config, cache }
    }

    /// Create with default configuration
    pub fn default_registry() -> Self {
        Self::new(RegistryConfig::default())
    }

    /// Search for packages
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<PackageMetadata>, RegistryError> {
        let url = format!("{}/api/v1/search?q={}&limit={}",
            self.config.url,
            urlencoding::encode(query),
            limit
        );

        let response = self.http_get(&url)?;
        self.parse_search_response(&response)
    }

    /// Get package metadata
    pub fn get_package(&self, name: &str) -> Result<PackageMetadata, RegistryError> {
        // Check cache first
        if let Some(meta) = self.cache.get_metadata(name) {
            return Ok(meta);
        }

        let url = format!("{}/api/v1/packages/{}", self.config.url, name);
        let response = self.http_get(&url)?;
        let meta = self.parse_package_response(&response)?;

        // Cache the metadata
        self.cache.store_metadata(name, &meta)?;

        Ok(meta)
    }

    /// Get specific version info
    pub fn get_version(&self, name: &str, version: &Version) -> Result<VersionInfo, RegistryError> {
        let meta = self.get_package(name)?;

        meta.versions
            .into_iter()
            .find(|v| &v.version == version)
            .ok_or_else(|| RegistryError::VersionNotFound(name.to_string(), version.to_string()))
    }

    /// Find best matching version
    pub fn find_version(&self, name: &str, req: &VersionReq) -> Result<VersionInfo, RegistryError> {
        let meta = self.get_package(name)?;

        // Find all matching non-yanked versions
        let mut matching: Vec<_> = meta.versions
            .into_iter()
            .filter(|v| !v.yanked && req.matches(&v.version))
            .collect();

        // Sort by version descending
        matching.sort_by(|a, b| b.version.cmp(&a.version));

        matching
            .into_iter()
            .next()
            .ok_or_else(|| RegistryError::VersionNotFound(name.to_string(), req.to_string()))
    }

    /// Download a package
    pub fn download(&self, name: &str, version: &Version) -> Result<DownloadedPackage, RegistryError> {
        // Check cache first
        if let Some(path) = self.cache.get_package(name, version) {
            let manifest = self.load_manifest(&path)?;
            return Ok(DownloadedPackage {
                name: name.to_string(),
                version: version.clone(),
                path,
                manifest,
            });
        }

        // Get version info for checksum
        let info = self.get_version(name, version)?;

        if info.yanked {
            return Err(RegistryError::Yanked(name.to_string(), version.clone()));
        }

        // Download tarball
        let url = format!("{}/api/v1/packages/{}/{}/download",
            self.config.url, name, version);
        let data = self.http_get_binary(&url)?;

        // Verify checksum
        let checksum = sha256_hex(&data);
        if checksum != info.checksum {
            return Err(RegistryError::ChecksumMismatch {
                expected: info.checksum,
                actual: checksum,
            });
        }

        // Extract to cache
        let path = self.cache.store_package(name, version, &data)?;
        let manifest = self.load_manifest(&path)?;

        Ok(DownloadedPackage {
            name: name.to_string(),
            version: version.clone(),
            path,
            manifest,
        })
    }

    /// Publish a package
    pub fn publish(&self, tarball: &[u8], manifest: &Manifest) -> Result<(), RegistryError> {
        let token = self.config.token.as_ref()
            .ok_or(RegistryError::AuthRequired)?;

        let url = format!("{}/api/v1/packages/new", self.config.url);

        // Create multipart body
        let boundary = "----QuantaPublishBoundary";
        let mut body = Vec::new();

        // Add manifest part
        write!(body, "--{}\r\n", boundary)?;
        write!(body, "Content-Disposition: form-data; name=\"manifest\"\r\n")?;
        write!(body, "Content-Type: application/toml\r\n\r\n")?;
        body.extend_from_slice(manifest.to_toml().as_bytes());
        write!(body, "\r\n")?;

        // Add tarball part
        write!(body, "--{}\r\n", boundary)?;
        write!(body, "Content-Disposition: form-data; name=\"tarball\"; filename=\"package.tar.gz\"\r\n")?;
        write!(body, "Content-Type: application/gzip\r\n\r\n")?;
        body.extend_from_slice(tarball);
        write!(body, "\r\n--{}--\r\n", boundary)?;

        let _response = self.http_post(&url, &body, token, boundary)?;

        Ok(())
    }

    /// Yank a version
    pub fn yank(&self, name: &str, version: &Version) -> Result<(), RegistryError> {
        let token = self.config.token.as_ref()
            .ok_or(RegistryError::AuthRequired)?;

        let url = format!("{}/api/v1/packages/{}/{}/yank",
            self.config.url, name, version);

        self.http_delete(&url, token)?;

        // Invalidate cache
        self.cache.invalidate(name);

        Ok(())
    }

    /// Unyank a version
    pub fn unyank(&self, name: &str, version: &Version) -> Result<(), RegistryError> {
        let token = self.config.token.as_ref()
            .ok_or(RegistryError::AuthRequired)?;

        let url = format!("{}/api/v1/packages/{}/{}/unyank",
            self.config.url, name, version);

        self.http_put(&url, &[], token)?;

        // Invalidate cache
        self.cache.invalidate(name);

        Ok(())
    }

    /// Get owners of a package
    pub fn get_owners(&self, name: &str) -> Result<Vec<Owner>, RegistryError> {
        let url = format!("{}/api/v1/packages/{}/owners", self.config.url, name);
        let response = self.http_get(&url)?;
        self.parse_owners_response(&response)
    }

    /// Add owner to a package
    pub fn add_owner(&self, name: &str, user: &str) -> Result<(), RegistryError> {
        let token = self.config.token.as_ref()
            .ok_or(RegistryError::AuthRequired)?;

        let url = format!("{}/api/v1/packages/{}/owners", self.config.url, name);
        let body = format!(r#"{{"login":"{}"}}"#, user);

        self.http_put(&url, body.as_bytes(), token)?;

        Ok(())
    }

    /// Remove owner from a package
    pub fn remove_owner(&self, name: &str, user: &str) -> Result<(), RegistryError> {
        let token = self.config.token.as_ref()
            .ok_or(RegistryError::AuthRequired)?;

        let url = format!("{}/api/v1/packages/{}/owners/{}",
            self.config.url, name, user);

        self.http_delete(&url, token)?;

        Ok(())
    }

    // HTTP helpers - placeholder implementations
    // In a real implementation, these would use an HTTP client

    fn http_get(&self, url: &str) -> Result<String, RegistryError> {
        // Placeholder - would use HTTP client
        let _ = url;
        let _ = self.config.timeout;
        Err(RegistryError::Network("HTTP client not implemented".to_string()))
    }

    fn http_get_binary(&self, url: &str) -> Result<Vec<u8>, RegistryError> {
        let _ = url;
        Err(RegistryError::Network("HTTP client not implemented".to_string()))
    }

    fn http_post(&self, url: &str, body: &[u8], token: &str, boundary: &str) -> Result<String, RegistryError> {
        let _ = (url, body, token, boundary);
        Err(RegistryError::Network("HTTP client not implemented".to_string()))
    }

    fn http_put(&self, url: &str, body: &[u8], token: &str) -> Result<String, RegistryError> {
        let _ = (url, body, token);
        Err(RegistryError::Network("HTTP client not implemented".to_string()))
    }

    fn http_delete(&self, url: &str, token: &str) -> Result<(), RegistryError> {
        let _ = (url, token);
        Err(RegistryError::Network("HTTP client not implemented".to_string()))
    }

    fn parse_search_response(&self, _response: &str) -> Result<Vec<PackageMetadata>, RegistryError> {
        // Placeholder JSON parsing
        Ok(Vec::new())
    }

    fn parse_package_response(&self, _response: &str) -> Result<PackageMetadata, RegistryError> {
        Err(RegistryError::InvalidResponse("JSON parsing not implemented".to_string()))
    }

    fn parse_owners_response(&self, _response: &str) -> Result<Vec<Owner>, RegistryError> {
        Ok(Vec::new())
    }

    fn load_manifest(&self, path: &Path) -> Result<Manifest, RegistryError> {
        let manifest_path = path.join("Quanta.toml");
        let content = std::fs::read_to_string(&manifest_path)?;
        Manifest::from_str(&content)
            .map_err(|e| RegistryError::InvalidResponse(format!("invalid manifest: {}", e)))
    }
}

/// Package owner information
#[derive(Debug, Clone)]
pub struct Owner {
    /// Login/username
    pub login: String,
    /// Display name
    pub name: Option<String>,
    /// Email
    pub email: Option<String>,
}

/// Package cache
pub struct PackageCache {
    root: PathBuf,
}

impl PackageCache {
    /// Create new cache
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Get cached metadata
    pub fn get_metadata(&self, name: &str) -> Option<PackageMetadata> {
        let path = self.metadata_path(name);
        if !path.exists() {
            return None;
        }

        // Check freshness (1 hour)
        if let Ok(meta) = std::fs::metadata(&path) {
            if let Ok(modified) = meta.modified() {
                if let Ok(elapsed) = SystemTime::now().duration_since(modified) {
                    if elapsed > Duration::from_secs(3600) {
                        return None;
                    }
                }
            }
        }

        // Parse cached metadata
        let content = std::fs::read_to_string(&path).ok()?;
        self.parse_cached_metadata(&content)
    }

    /// Store metadata in cache
    pub fn store_metadata(&self, name: &str, _meta: &PackageMetadata) -> Result<(), RegistryError> {
        let path = self.metadata_path(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Serialize metadata (placeholder)
        let content = "{}"; // Would serialize to JSON
        std::fs::write(&path, content)?;

        Ok(())
    }

    /// Get cached package
    pub fn get_package(&self, name: &str, version: &Version) -> Option<PathBuf> {
        let path = self.package_path(name, version);
        if path.exists() && path.join("Quanta.toml").exists() {
            Some(path)
        } else {
            None
        }
    }

    /// Store package in cache
    pub fn store_package(&self, name: &str, version: &Version, data: &[u8]) -> Result<PathBuf, RegistryError> {
        let path = self.package_path(name, version);

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Extract tarball
        self.extract_tarball(data, &path)?;

        Ok(path)
    }

    /// Invalidate cache for a package
    pub fn invalidate(&self, name: &str) {
        let path = self.metadata_path(name);
        let _ = std::fs::remove_file(path);
    }

    /// Clear all cached data
    pub fn clear(&self) -> Result<(), RegistryError> {
        if self.root.exists() {
            std::fs::remove_dir_all(&self.root)?;
        }
        Ok(())
    }

    fn metadata_path(&self, name: &str) -> PathBuf {
        self.root.join("metadata").join(format!("{}.json", name))
    }

    fn package_path(&self, name: &str, version: &Version) -> PathBuf {
        self.root.join("packages").join(name).join(version.to_string())
    }

    fn parse_cached_metadata(&self, _content: &str) -> Option<PackageMetadata> {
        // Placeholder JSON parsing
        None
    }

    fn extract_tarball(&self, _data: &[u8], _dest: &Path) -> Result<(), RegistryError> {
        // Placeholder - would use tar/gzip libraries
        Ok(())
    }
}

/// URL encoding helper
mod urlencoding {
    pub fn encode(s: &str) -> String {
        let mut result = String::with_capacity(s.len() * 3);
        for c in s.chars() {
            match c {
                'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => {
                    result.push(c);
                }
                _ => {
                    for b in c.to_string().as_bytes() {
                        result.push_str(&format!("%{:02X}", b));
                    }
                }
            }
        }
        result
    }
}

/// SHA256 hash helper (placeholder)
fn sha256_hex(data: &[u8]) -> String {
    // Placeholder - would use sha2 crate
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    data.hash(&mut hasher);
    format!("{:016x}{:016x}{:016x}{:016x}",
        hasher.finish(), hasher.finish(), hasher.finish(), hasher.finish())
}

/// Git source for packages
#[derive(Debug, Clone)]
pub struct GitSource {
    /// Repository URL
    pub url: String,
    /// Branch
    pub branch: Option<String>,
    /// Tag
    pub tag: Option<String>,
    /// Commit
    pub rev: Option<String>,
}

impl GitSource {
    /// Create from URL
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            branch: None,
            tag: None,
            rev: None,
        }
    }

    /// Set branch
    pub fn with_branch(mut self, branch: impl Into<String>) -> Self {
        self.branch = Some(branch.into());
        self
    }

    /// Set tag
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = Some(tag.into());
        self
    }

    /// Set revision
    pub fn with_rev(mut self, rev: impl Into<String>) -> Self {
        self.rev = Some(rev.into());
        self
    }

    /// Clone/fetch the repository
    pub fn fetch(&self, dest: &Path) -> Result<(), RegistryError> {
        // Placeholder - would use git2 crate
        let _ = dest;
        Err(RegistryError::Network("Git support not implemented".to_string()))
    }
}

/// Path source for local packages
#[derive(Debug, Clone)]
pub struct PathSource {
    /// Local path
    pub path: PathBuf,
}

impl PathSource {
    /// Create from path
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Resolve the path relative to a base
    pub fn resolve(&self, base: &Path) -> PathBuf {
        if self.path.is_absolute() {
            self.path.clone()
        } else {
            base.join(&self.path)
        }
    }

    /// Load manifest from path
    pub fn load_manifest(&self) -> Result<Manifest, RegistryError> {
        let manifest_path = self.path.join("Quanta.toml");
        let content = std::fs::read_to_string(&manifest_path)?;
        Manifest::from_str(&content)
            .map_err(|e| RegistryError::InvalidResponse(format!("invalid manifest: {}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_encoding() {
        assert_eq!(urlencoding::encode("hello"), "hello");
        assert_eq!(urlencoding::encode("hello world"), "hello%20world");
        assert_eq!(urlencoding::encode("a+b=c"), "a%2Bb%3Dc");
    }

    #[test]
    fn test_cache_paths() {
        let cache = PackageCache::new(PathBuf::from("/cache"));

        let meta_path = cache.metadata_path("my-package");
        assert!(meta_path.to_str().unwrap().contains("my-package.json"));

        let pkg_path = cache.package_path("my-package", &Version::new(1, 2, 3));
        assert!(pkg_path.to_str().unwrap().contains("1.2.3"));
    }

    #[test]
    fn test_git_source() {
        let source = GitSource::new("https://github.com/user/repo")
            .with_branch("main")
            .with_tag("v1.0.0");

        assert_eq!(source.url, "https://github.com/user/repo");
        assert_eq!(source.branch, Some("main".to_string()));
        assert_eq!(source.tag, Some("v1.0.0".to_string()));
    }

    #[test]
    fn test_path_source() {
        let source = PathSource::new("../other-package");
        let resolved = source.resolve(Path::new("/home/user/project"));

        assert!(resolved.to_str().unwrap().contains("other-package"));
    }
}
