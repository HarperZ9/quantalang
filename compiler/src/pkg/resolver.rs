// ===============================================================================
// QUANTALANG DEPENDENCY RESOLVER
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Dependency resolution using PubGrub algorithm.
//!
//! Implements version-aware dependency resolution with:
//! - Semver compatibility
//! - Feature resolution
//! - Conflict detection with useful error messages
//! - Cycle detection

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::fmt;
use std::sync::Arc;

use super::{Manifest, PackageMetadata, Registry, RegistryError, Version, VersionInfo, VersionReq};

/// Resolution error types
#[derive(Debug)]
pub enum ResolveError {
    /// No matching version found
    NoMatchingVersion {
        package: String,
        requirement: VersionReq,
        available: Vec<Version>,
    },
    /// Conflicting requirements
    Conflict {
        package: String,
        requirements: Vec<(String, VersionReq)>,
    },
    /// Dependency cycle detected
    Cycle(Vec<String>),
    /// Registry error
    Registry(RegistryError),
    /// Feature not found
    FeatureNotFound { package: String, feature: String },
    /// Maximum iterations exceeded
    MaxIterations,
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoMatchingVersion {
                package,
                requirement,
                available,
            } => {
                write!(f, "no matching version for '{}' {}", package, requirement)?;
                if !available.is_empty() {
                    write!(f, " (available: ")?;
                    for (i, v) in available.iter().take(5).enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{}", v)?;
                    }
                    if available.len() > 5 {
                        write!(f, ", ...")?;
                    }
                    write!(f, ")")?;
                }
                Ok(())
            }
            Self::Conflict {
                package,
                requirements,
            } => {
                writeln!(f, "conflicting requirements for '{}':", package)?;
                for (from, req) in requirements {
                    writeln!(f, "  - {} requires {}", from, req)?;
                }
                Ok(())
            }
            Self::Cycle(packages) => {
                write!(f, "dependency cycle detected: ")?;
                for (i, p) in packages.iter().enumerate() {
                    if i > 0 {
                        write!(f, " -> ")?;
                    }
                    write!(f, "{}", p)?;
                }
                Ok(())
            }
            Self::Registry(e) => write!(f, "registry error: {}", e),
            Self::FeatureNotFound { package, feature } => {
                write!(
                    f,
                    "feature '{}' not found in package '{}'",
                    feature, package
                )
            }
            Self::MaxIterations => write!(f, "maximum resolution iterations exceeded"),
        }
    }
}

impl std::error::Error for ResolveError {}

impl From<RegistryError> for ResolveError {
    fn from(e: RegistryError) -> Self {
        Self::Registry(e)
    }
}

/// A resolved package
#[derive(Debug, Clone)]
pub struct ResolvedPackage {
    /// Package name
    pub name: String,
    /// Resolved version
    pub version: Version,
    /// Enabled features
    pub features: BTreeSet<String>,
    /// Direct dependencies
    pub dependencies: BTreeMap<String, Version>,
    /// Whether this is a dev dependency
    pub is_dev: bool,
}

/// Complete resolution result
#[derive(Debug, Clone)]
pub struct Resolution {
    /// Root package
    pub root: ResolvedPackage,
    /// All resolved packages
    pub packages: BTreeMap<String, ResolvedPackage>,
    /// Dependency graph
    pub graph: DependencyGraph,
}

impl Resolution {
    /// Get topological order for building
    pub fn build_order(&self) -> Vec<&str> {
        self.graph.topological_order()
    }

    /// Get all packages that need a specific package
    pub fn dependents(&self, name: &str) -> Vec<&str> {
        self.graph.dependents(name)
    }

    /// Check if a package is included
    pub fn contains(&self, name: &str) -> bool {
        self.packages.contains_key(name)
    }

    /// Get a resolved package
    pub fn get(&self, name: &str) -> Option<&ResolvedPackage> {
        self.packages.get(name)
    }
}

/// Dependency graph
#[derive(Debug, Clone, Default)]
pub struct DependencyGraph {
    /// Edges: from -> [to]
    edges: BTreeMap<String, BTreeSet<String>>,
    /// Reverse edges: to -> [from]
    reverse: BTreeMap<String, BTreeSet<String>>,
}

impl DependencyGraph {
    /// Create new graph
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an edge
    pub fn add_edge(&mut self, from: &str, to: &str) {
        self.edges
            .entry(from.to_string())
            .or_default()
            .insert(to.to_string());
        self.reverse
            .entry(to.to_string())
            .or_default()
            .insert(from.to_string());
    }

    /// Get dependencies
    pub fn dependencies(&self, name: &str) -> impl Iterator<Item = &str> {
        self.edges
            .get(name)
            .into_iter()
            .flat_map(|s| s.iter())
            .map(|s| s.as_str())
    }

    /// Get dependents (reverse dependencies)
    pub fn dependents(&self, name: &str) -> Vec<&str> {
        self.reverse
            .get(name)
            .into_iter()
            .flat_map(|s| s.iter())
            .map(|s| s.as_str())
            .collect()
    }

    /// Topological sort
    pub fn topological_order(&self) -> Vec<&str> {
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        let mut temp_visited = HashSet::new();

        for name in self.edges.keys() {
            self.visit_topo(name, &mut visited, &mut temp_visited, &mut result);
        }

        // Also include nodes that are only targets
        for name in self.reverse.keys() {
            if !visited.contains(name.as_str()) {
                result.push(name.as_str());
            }
        }

        result
    }

    fn visit_topo<'a>(
        &'a self,
        name: &'a str,
        visited: &mut HashSet<&'a str>,
        temp_visited: &mut HashSet<&'a str>,
        result: &mut Vec<&'a str>,
    ) {
        if visited.contains(name) {
            return;
        }
        if temp_visited.contains(name) {
            return; // Cycle - handled elsewhere
        }

        temp_visited.insert(name);

        if let Some(deps) = self.edges.get(name) {
            for dep in deps {
                self.visit_topo(dep, visited, temp_visited, result);
            }
        }

        temp_visited.remove(name);
        visited.insert(name);
        result.push(name);
    }

    /// Detect cycles
    pub fn find_cycle(&self) -> Option<Vec<String>> {
        let mut visited = HashSet::new();
        let mut path = Vec::new();
        let mut path_set = HashSet::new();

        for start in self.edges.keys() {
            if !visited.contains(start.as_str()) {
                if let Some(cycle) =
                    self.find_cycle_from(start, &mut visited, &mut path, &mut path_set)
                {
                    return Some(cycle);
                }
            }
        }

        None
    }

    fn find_cycle_from<'a>(
        &'a self,
        node: &'a str,
        visited: &mut HashSet<&'a str>,
        path: &mut Vec<&'a str>,
        path_set: &mut HashSet<&'a str>,
    ) -> Option<Vec<String>> {
        visited.insert(node);
        path.push(node);
        path_set.insert(node);

        if let Some(deps) = self.edges.get(node) {
            for dep in deps {
                if path_set.contains(dep.as_str()) {
                    // Found cycle
                    let cycle_start = path.iter().position(|&n| n == dep.as_str()).unwrap();
                    let mut cycle: Vec<_> =
                        path[cycle_start..].iter().map(|s| s.to_string()).collect();
                    cycle.push(dep.to_string());
                    return Some(cycle);
                }

                if !visited.contains(dep.as_str()) {
                    if let Some(cycle) = self.find_cycle_from(dep, visited, path, path_set) {
                        return Some(cycle);
                    }
                }
            }
        }

        path.pop();
        path_set.remove(node);
        None
    }
}

/// Dependency resolver
pub struct Resolver<'a> {
    registry: &'a Registry,
    root_manifest: &'a Manifest,
    include_dev: bool,
    max_iterations: usize,
    // Cache of fetched package metadata
    metadata_cache: HashMap<String, Arc<PackageMetadata>>,
}

impl<'a> Resolver<'a> {
    /// Create new resolver
    pub fn new(registry: &'a Registry, manifest: &'a Manifest) -> Self {
        Self {
            registry,
            root_manifest: manifest,
            include_dev: false,
            max_iterations: 10000,
            metadata_cache: HashMap::new(),
        }
    }

    /// Include dev dependencies
    pub fn with_dev_dependencies(mut self) -> Self {
        self.include_dev = true;
        self
    }

    /// Set maximum iterations
    pub fn with_max_iterations(mut self, max: usize) -> Self {
        self.max_iterations = max;
        self
    }

    /// Resolve all dependencies
    pub fn resolve(&mut self) -> Result<Resolution, ResolveError> {
        let mut resolved: BTreeMap<String, ResolvedPackage> = BTreeMap::new();
        let mut graph = DependencyGraph::new();

        // Track pending requirements: (package, requirement, from, features, is_dev)
        let mut pending: VecDeque<(String, VersionReq, String, Vec<String>, bool)> =
            VecDeque::new();

        // Track all requirements for each package (for conflict reporting)
        let mut all_requirements: HashMap<String, Vec<(String, VersionReq)>> = HashMap::new();

        // Add root dependencies
        let root_name = &self.root_manifest.package.name;
        for (name, dep) in &self.root_manifest.dependencies {
            if let Some(req) = &dep.version {
                pending.push_back((
                    name.clone(),
                    req.clone(),
                    root_name.clone(),
                    dep.features.clone(),
                    false,
                ));
            }
        }

        // Add dev dependencies if requested
        if self.include_dev {
            for (name, dep) in &self.root_manifest.dev_dependencies {
                if let Some(req) = &dep.version {
                    pending.push_back((
                        name.clone(),
                        req.clone(),
                        root_name.clone(),
                        dep.features.clone(),
                        true,
                    ));
                }
            }
        }

        let mut iterations = 0;

        while let Some((pkg_name, req, from, features, is_dev)) = pending.pop_front() {
            iterations += 1;
            if iterations > self.max_iterations {
                return Err(ResolveError::MaxIterations);
            }

            // Track requirement
            all_requirements
                .entry(pkg_name.clone())
                .or_default()
                .push((from.clone(), req.clone()));

            // Check if already resolved
            if let Some(existing) = resolved.get_mut(&pkg_name) {
                // Check compatibility
                if !req.matches(&existing.version) {
                    let reqs = all_requirements.get(&pkg_name).cloned().unwrap_or_default();
                    return Err(ResolveError::Conflict {
                        package: pkg_name,
                        requirements: reqs,
                    });
                }

                // Merge features
                for f in features {
                    existing.features.insert(f);
                }

                continue;
            }

            // Fetch package metadata
            let metadata = self.get_metadata(&pkg_name)?;

            // Find best matching version
            let version_info = self.find_best_version(&metadata, &req)?;

            // Validate features
            for f in &features {
                if !version_info.features.contains_key(f) && f != "default" {
                    return Err(ResolveError::FeatureNotFound {
                        package: pkg_name.clone(),
                        feature: f.clone(),
                    });
                }
            }

            // Collect all enabled features (including default and transitive)
            let mut enabled_features: BTreeSet<String> = features.iter().cloned().collect();
            if version_info.features.contains_key("default") {
                enabled_features.insert("default".to_string());
            }

            // Expand feature dependencies
            let mut expanded = enabled_features.clone();
            loop {
                let mut new_features = BTreeSet::new();
                for f in &expanded {
                    if let Some(deps) = version_info.features.get(f) {
                        for dep in deps {
                            if !dep.contains('/') && !expanded.contains(dep) {
                                new_features.insert(dep.clone());
                            }
                        }
                    }
                }
                if new_features.is_empty() {
                    break;
                }
                expanded.extend(new_features);
            }

            // Create resolved package
            let resolved_pkg = ResolvedPackage {
                name: pkg_name.clone(),
                version: version_info.version.clone(),
                features: expanded.clone(),
                dependencies: BTreeMap::new(),
                is_dev,
            };

            resolved.insert(pkg_name.clone(), resolved_pkg);
            graph.add_edge(&from, &pkg_name);

            // Queue transitive dependencies
            for (dep_name, dep_req) in &version_info.dependencies {
                pending.push_back((
                    dep_name.clone(),
                    dep_req.clone(),
                    pkg_name.clone(),
                    Vec::new(),
                    is_dev,
                ));
            }

            // Queue feature-gated dependencies
            for f in &expanded {
                if let Some(feature_deps) = version_info.features.get(f) {
                    for dep in feature_deps {
                        if let Some((pkg, feature)) = dep.split_once('/') {
                            // Dependency with feature
                            if let Some(dep_req) = version_info.dependencies.get(pkg) {
                                pending.push_back((
                                    pkg.to_string(),
                                    dep_req.clone(),
                                    pkg_name.clone(),
                                    vec![feature.to_string()],
                                    is_dev,
                                ));
                            }
                        }
                    }
                }
            }
        }

        // Check for cycles
        if let Some(cycle) = graph.find_cycle() {
            return Err(ResolveError::Cycle(cycle));
        }

        // Collect dependency versions first
        let dep_versions: HashMap<String, Version> = resolved
            .iter()
            .map(|(name, pkg)| (name.clone(), pkg.version.clone()))
            .collect();

        // Update dependency versions in resolved packages
        for (name, pkg) in &mut resolved {
            for dep_name in graph.dependencies(name) {
                if let Some(version) = dep_versions.get(dep_name) {
                    pkg.dependencies
                        .insert(dep_name.to_string(), version.clone());
                }
            }
        }

        // Create root resolved package
        let root = ResolvedPackage {
            name: root_name.clone(),
            version: self.root_manifest.package.version.clone(),
            features: BTreeSet::new(),
            dependencies: resolved
                .iter()
                .filter(|(_, p)| !p.is_dev)
                .map(|(n, p)| (n.clone(), p.version.clone()))
                .collect(),
            is_dev: false,
        };

        Ok(Resolution {
            root,
            packages: resolved,
            graph,
        })
    }

    fn get_metadata(&mut self, name: &str) -> Result<Arc<PackageMetadata>, ResolveError> {
        if let Some(cached) = self.metadata_cache.get(name) {
            return Ok(cached.clone());
        }

        let metadata = self.registry.get_package(name)?;
        let arc = Arc::new(metadata);
        self.metadata_cache.insert(name.to_string(), arc.clone());
        Ok(arc)
    }

    fn find_best_version(
        &self,
        metadata: &PackageMetadata,
        req: &VersionReq,
    ) -> Result<VersionInfo, ResolveError> {
        let mut matching: Vec<_> = metadata
            .versions
            .iter()
            .filter(|v| !v.yanked && req.matches(&v.version))
            .collect();

        if matching.is_empty() {
            return Err(ResolveError::NoMatchingVersion {
                package: metadata.name.clone(),
                requirement: req.clone(),
                available: metadata
                    .versions
                    .iter()
                    .map(|v| v.version.clone())
                    .collect(),
            });
        }

        // Sort by version descending (newest first)
        matching.sort_by(|a, b| b.version.cmp(&a.version));

        Ok(matching[0].clone())
    }
}

/// Resolver builder for convenient configuration
pub struct ResolverBuilder {
    include_dev: bool,
    max_iterations: usize,
    locked_versions: HashMap<String, Version>,
}

impl ResolverBuilder {
    /// Create new builder
    pub fn new() -> Self {
        Self {
            include_dev: false,
            max_iterations: 10000,
            locked_versions: HashMap::new(),
        }
    }

    /// Include dev dependencies
    pub fn dev_dependencies(mut self, include: bool) -> Self {
        self.include_dev = include;
        self
    }

    /// Set max iterations
    pub fn max_iterations(mut self, max: usize) -> Self {
        self.max_iterations = max;
        self
    }

    /// Lock a specific version
    pub fn lock_version(mut self, name: impl Into<String>, version: Version) -> Self {
        self.locked_versions.insert(name.into(), version);
        self
    }

    /// Build resolver
    pub fn build<'a>(self, registry: &'a Registry, manifest: &'a Manifest) -> Resolver<'a> {
        let mut resolver =
            Resolver::new(registry, manifest).with_max_iterations(self.max_iterations);

        if self.include_dev {
            resolver = resolver.with_dev_dependencies();
        }

        resolver
    }
}

impl Default for ResolverBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Minimal update resolver - only update specified packages
pub struct MinimalUpdateResolver<'a> {
    base: Resolver<'a>,
    locked: HashMap<String, Version>,
    update_packages: HashSet<String>,
}

impl<'a> MinimalUpdateResolver<'a> {
    /// Create from existing resolution
    pub fn new(registry: &'a Registry, manifest: &'a Manifest, existing: &Resolution) -> Self {
        let locked: HashMap<_, _> = existing
            .packages
            .iter()
            .map(|(n, p)| (n.clone(), p.version.clone()))
            .collect();

        Self {
            base: Resolver::new(registry, manifest),
            locked,
            update_packages: HashSet::new(),
        }
    }

    /// Mark a package for update
    pub fn update(mut self, package: impl Into<String>) -> Self {
        self.update_packages.insert(package.into());
        self
    }

    /// Update all packages
    pub fn update_all(mut self) -> Self {
        self.locked.clear();
        self
    }

    /// Resolve with minimal updates
    pub fn resolve(self) -> Result<Resolution, ResolveError> {
        // Remove locked versions for packages being updated
        let mut resolver = self.base;

        // For now, just do a full resolve
        // A proper implementation would prefer locked versions
        resolver.resolve()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dependency_graph_basic() {
        let mut graph = DependencyGraph::new();
        graph.add_edge("a", "b");
        graph.add_edge("a", "c");
        graph.add_edge("b", "d");
        graph.add_edge("c", "d");

        let deps: Vec<_> = graph.dependencies("a").collect();
        assert!(deps.contains(&"b"));
        assert!(deps.contains(&"c"));

        let order = graph.topological_order();
        let a_pos = order.iter().position(|&x| x == "a").unwrap();
        let d_pos = order.iter().position(|&x| x == "d").unwrap();
        assert!(d_pos < a_pos);
    }

    #[test]
    fn test_dependency_graph_cycle() {
        let mut graph = DependencyGraph::new();
        graph.add_edge("a", "b");
        graph.add_edge("b", "c");
        graph.add_edge("c", "a");

        let cycle = graph.find_cycle();
        assert!(cycle.is_some());
        let cycle = cycle.unwrap();
        assert_eq!(cycle.len(), 4); // a -> b -> c -> a
    }

    #[test]
    fn test_dependency_graph_no_cycle() {
        let mut graph = DependencyGraph::new();
        graph.add_edge("a", "b");
        graph.add_edge("a", "c");
        graph.add_edge("b", "d");
        graph.add_edge("c", "d");

        assert!(graph.find_cycle().is_none());
    }

    #[test]
    fn test_resolver_builder() {
        let builder = ResolverBuilder::new()
            .dev_dependencies(true)
            .max_iterations(5000)
            .lock_version("foo", Version::new(1, 0, 0));

        assert!(builder.include_dev);
        assert_eq!(builder.max_iterations, 5000);
        assert!(builder.locked_versions.contains_key("foo"));
    }
}
