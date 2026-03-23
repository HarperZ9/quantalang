# QuantaLang Package Ecosystem Design

Version: 0.1 (Draft)
Date: 2026-03-21

---

## 1. Package Manifest (Quanta.toml)

Every QuantaLang project has a `Quanta.toml` at its root. This file declares
the package identity, dependencies, build targets, and features.

### 1.1 Full Manifest Format

```toml
[package]
name = "my-project"
version = "0.1.0"
edition = "2026"
authors = ["Zain Harper <zain@example.com>"]
description = "A short description of the package"
license = "MIT"
repository = "https://github.com/user/my-project"

[dependencies]
json = "1.0"
http = { version = "0.2", features = ["tls"] }
local-lib = { path = "../local-lib" }
git-lib = { git = "https://github.com/user/lib.git", branch = "main" }

[dev-dependencies]
test-utils = "0.3"

[build-dependencies]
codegen-helper = "0.1"

[features]
default = ["std"]
std = []
serde = ["dep:serde"]
tls = ["http/tls"]

[[bin]]
name = "my-cli"
path = "src/main.quanta"

[lib]
name = "my_lib"
path = "src/lib.quanta"
```

### 1.2 What Already Exists in the Compiler

The `compiler/src/pkg/manifest.rs` module already implements:

- **`Manifest` struct** with fields for: `package` (name, version, authors,
  edition, description, license, repository), `dependencies`, `dev_dependencies`,
  `build_dependencies`, `features`, `default_features`, `workspace`, and target
  arrays (`bin`, `lib`, `example`, `test`, `bench`).
- **`Manifest::new()`** constructor with defaults.
- **`Manifest::from_str()`** and **`Manifest::from_file()`** for parsing TOML.
- **`Manifest::to_toml()`** for serialization back to TOML text.
- **`add_dependency()`** and **`add_dev_dependency()`** methods.
- **`Dependency` type** supporting version string, path, git, and feature lists.

This is a solid foundation. The manifest parser can already read and write the
full format shown above.

---

## 2. Module System

### 2.1 Import Syntax

```quanta
// Import from standard library
use std::collections::HashMap;
use std::io::Read;

// Import from a dependency declared in Quanta.toml
use json::Value;
use http::Client;

// Import from local file (sibling module)
mod my_module;
use my_module::MyType;

// Import from nested module directory
mod utils;
use utils::helpers::format_output;

// Glob import (all public items)
use std::collections::*;

// Aliased import
use json::Value as JsonValue;

// Multiple imports from same path
use std::collections::{HashMap, HashSet, BTreeMap};
```

### 2.2 Module Resolution Rules

When the compiler encounters `use foo::bar::Baz;`, it resolves the path as follows:

1. **`std::`** -- Standard library. Resolved from the compiler's built-in std
   implementation. Not a filesystem lookup.

2. **`<dep_name>::`** where `dep_name` is a key in `[dependencies]` -- External
   dependency. Resolved from the package cache (see Section 3).

3. **`<local_name>::`** where `local_name` matches a `mod <name>;` declaration --
   Local module. Resolved by filesystem convention:
   - `mod foo;` looks for `./foo.quanta` or `./foo/mod.quanta` relative to the
     current file.
   - `mod foo::bar;` looks for `./foo/bar.quanta` or `./foo/bar/mod.quanta`.

4. **`self::`** -- Current module. Relative import within the same file.

5. **`super::`** -- Parent module. One level up in the module hierarchy.

6. **`crate::`** -- Crate root. Absolute path from the package root (`src/lib.quanta`
   or `src/main.quanta`).

### 2.3 File Layout Convention

```
my-project/
  Quanta.toml
  Quanta.lock
  src/
    main.quanta          # Binary entry point
    lib.quanta           # Library root (if [lib] is defined)
    utils.quanta         # Module: `mod utils;` or `use crate::utils`
    utils/
      mod.quanta         # Alternative to utils.quanta
      helpers.quanta     # Sub-module: `use crate::utils::helpers`
  tests/
    integration.quanta   # Integration tests
  examples/
    demo.quanta          # Example programs
```

### 2.4 Visibility

```quanta
// Public (accessible from other modules)
pub fn public_function() { }
pub struct PublicStruct { }

// Private (default, accessible only within this module)
fn private_function() { }
struct PrivateStruct { }

// Public within the crate only (not exported to dependents)
pub(crate) fn crate_internal() { }

// Public within the parent module only
pub(super) fn parent_only() { }
```

---

## 3. Package Registry

### 3.1 Registry Design

- **URL:** `https://registry.quantalang.org` (future, already configured as
  default in `compiler/src/pkg/registry.rs`)
- **API:** RESTful JSON API for package metadata, tarball downloads for source
- **Index:** Git-based index (similar to crates.io) for fast metadata lookups
- **Authentication:** Token-based via `quantac login`

### 3.2 CLI Commands

```
quantac pkg init                    # Create Quanta.toml in current directory
quantac pkg add json@1.0            # Add dependency to Quanta.toml
quantac pkg add http --features tls # Add with features
quantac pkg remove json             # Remove dependency
quantac pkg update                  # Update all dependencies to latest compatible
quantac pkg update json             # Update specific dependency
quantac pkg publish                 # Publish package to registry
quantac pkg search "json parser"    # Search registry
quantac pkg info json               # Show package details
quantac pkg login                   # Authenticate with registry
quantac pkg yank 0.1.0              # Yank a published version
```

### 3.3 What Already Exists in the Compiler

The `compiler/src/pkg/registry.rs` module implements:

- **`RegistryConfig`** with url, auth token, cache directory, timeout, max
  concurrent downloads. Default URL is `https://registry.quantalang.org`.
- **`RegistryError`** enum covering: Network, NotFound, VersionNotFound,
  AuthRequired, AuthFailed, RateLimited, InvalidResponse, CacheError, Io,
  ChecksumMismatch, Yanked.
- **`PackageMetadata`** struct for registry responses.
- **Cross-platform cache directory** resolution (LOCALAPPDATA on Windows,
  XDG_CACHE_HOME on Linux).

The `compiler/src/pkg/resolver.rs` module implements:

- **`Resolver`** with PubGrub-style dependency resolution.
- **`ResolveError`** covering: NoMatchingVersion (with available versions),
  Conflict (with source requirements), Cycle (with package chain),
  FeatureNotFound, MaxIterations.
- **`ResolvedPackage`** with name, version, features, dependencies.
- **`Resolution`** with root package, all resolved packages, dependency graph.

The `compiler/src/pkg/lockfile.rs` module implements:

- **Lockfile format** (version 1) using `Quanta.lock` filename.
- **`LockfileError`** for IO, parse, version mismatch, and integrity errors.
- **BTreeMap-based** package storage for deterministic output.

The `compiler/src/pkg/version.rs` module implements:

- **`Version`** struct with major, minor, patch, pre-release, build metadata.
- **SemVer 2.0.0** compliant parsing, comparison, and ordering.
- **`VersionReq`** for version requirement matching (caret, tilde, exact, etc.).

### 3.4 Lockfile Format

```toml
# Quanta.lock - auto-generated, do not edit manually
version = 1

[[package]]
name = "json"
version = "1.2.3"
source = "registry+https://registry.quantalang.org"
checksum = "sha256:abc123..."

[[package]]
name = "http"
version = "0.2.1"
source = "registry+https://registry.quantalang.org"
checksum = "sha256:def456..."
dependencies = ["json 1.2.3"]
features = ["tls"]

[[package]]
name = "local-lib"
version = "0.1.0"
source = "path+../local-lib"
```

---

## 4. What Needs to Be Built

The compiler currently compiles single `.quanta` files to C via the C backend.
There is no multi-file compilation, no module resolution, and no dependency
fetching at compile time. The pkg infrastructure exists as data structures
and algorithms but is not connected to the compilation pipeline.

### 4.1 Module Resolution (finding .quanta files from `use` statements)

**Current state:** The parser recognizes `use` statements syntactically but
the compiler does not resolve them to files or load additional source.

**What needs to happen:**
1. After parsing a file, collect all `mod` declarations and `use` paths.
2. Resolve each to a filesystem path using the rules in Section 2.2.
3. Parse each resolved file recursively (with cycle detection).
4. Build a module tree that maps qualified names to their definitions.
5. Wire the module tree into name resolution so that `use json::Value` finds
   the `Value` type from the `json` package's module tree.

**Dependencies:** None. This can be built now with single-crate, multi-file
compilation as the scope.

### 4.2 Multi-File Compilation (compiling multiple .quanta files into one output)

**Current state:** The compiler processes one file -> one AST -> one HIR -> one
MIR -> one C output. There is no mechanism to merge multiple MIR modules.

**What needs to happen:**
1. Compile each source file to its own MIR module independently.
2. Merge MIR modules: combine function lists, type definitions, string tables,
   global variables. Handle name collisions with module-qualified names
   (e.g., `mod_name__func_name` in the C output).
3. Resolve cross-module function calls: when file A calls a function defined
   in file B, the MIR for file A contains an external function declaration
   that matches file B's function definition.
4. Generate a single C output (or per-file C outputs linked together).

**Dependencies:** Module resolution (4.1) must be done first.

### 4.3 Dependency Fetching (downloading packages from registry)

**Current state:** `RegistryConfig`, `PackageMetadata`, `Resolver`, and
`Lockfile` types exist but have no network implementation. No HTTP client
is embedded in the compiler.

**What needs to happen:**
1. Implement `Registry::fetch()` -- HTTP GET to registry API for package metadata.
2. Implement `Registry::download()` -- download and extract package tarball to
   cache directory.
3. Integrate resolver: read `Quanta.toml`, resolve all dependencies, write
   `Quanta.lock`, download missing packages.
4. Add cache management: check if package version already exists in cache
   before downloading.
5. Wire into CLI: `quantac pkg add`, `quantac pkg update`, etc.

**Dependencies:** Module resolution (4.1) must be done first so that downloaded
packages can be compiled. An HTTP client library is needed (either vendored
or via system libcurl).

### 4.4 Build Caching (not recompiling unchanged files)

**Current state:** Every `quantac build` recompiles from scratch.

**What needs to happen:**
1. Hash each source file (content hash, not timestamp).
2. Store compiled artifacts (MIR or C output) alongside hashes in a build
   directory (`target/` or `.quanta-cache/`).
3. On rebuild, compare current hashes to stored hashes. Only recompile files
   whose hash changed or whose dependencies changed.
4. For the C backend: compile each module to a separate `.c` file, compile
   each to `.o` with the system C compiler, then link. Only recompile changed
   `.c` files.

**Dependencies:** Multi-file compilation (4.2) must be done first. Build
caching only makes sense when there are multiple compilation units.

---

## 5. Standard Library Plan

The standard library (`std`) would be a built-in package that ships with the
compiler. It does not go through the registry.

### 5.1 Planned Modules

```
std::
  collections::    HashMap, HashSet, BTreeMap, BTreeSet, Vec, VecDeque
  io::             Read, Write, stdin, stdout, stderr, File
  fs::             read, write, create_dir, remove_file, metadata
  fmt::            Display, Debug, format!, println!
  string::         String, str methods
  math::           abs, sqrt, sin, cos, pow, min, max, PI, E
  net::            TcpStream, TcpListener, UdpSocket
  env::            args, vars, current_dir
  process::        Command, exit, abort
  sync::           Mutex, RwLock, Arc, atomic
  thread::         spawn, sleep, current
  time::           Instant, Duration, SystemTime
  error::          Error trait, Result extensions
```

### 5.2 Implementation Strategy

Since the compiler currently outputs C, the standard library would be
implemented as:
1. QuantaLang source files that define the public API (types, function signatures).
2. C runtime functions (already partially in `compiler/src/codegen/runtime.rs`)
   that implement the actual behavior.
3. The compiler maps `std::io::println` to a C `printf` call in the generated
   output.

This is the same strategy used by early Rust (pre-LLVM maturity) and by Zig
(which compiles to C as a fallback).

---

## 6. Realistic Roadmap

### Phase 0: Current State (now)
- Single-file compilation via C backend
- `use` statements parsed but not resolved
- Package infrastructure exists as data structures only
- No multi-file, no dependencies, no caching

### Phase 1: Multi-File Compilation (estimated: 2-3 weeks of focused work)
- Implement module resolution (find .quanta files from `mod` and `use`)
- Implement MIR module merging
- Support compiling a project directory (not just a single file)
- **Prerequisite for everything else**
- **Can be built now** -- no external dependencies needed

### Phase 2: Local Dependencies (estimated: 1-2 weeks after Phase 1)
- Support `path` dependencies in Quanta.toml: `my-lib = { path = "../my-lib" }`
- Compile dependency packages and link their MIR/C output
- This gives multi-package development without a registry
- **Can be built immediately after Phase 1**

### Phase 3: Build Caching (estimated: 1 week after Phase 2)
- Content-hash source files
- Cache compiled artifacts in `target/` directory
- Incremental recompilation
- **Can be built after Phase 2**

### Phase 4: Package Registry (estimated: 4-6 weeks after Phase 2)
- Build the registry server (separate project)
- Add HTTP client to compiler for fetching packages
- Implement `quantac pkg publish`, `quantac pkg add`
- Write Quanta.lock on resolve
- **Requires a hosted server and an HTTP client in the compiler**
- **This is the most infrastructure-heavy phase**

### Phase 5: Standard Library (ongoing, parallel to Phases 1-4)
- Start with `std::io::println` (already partially implemented via runtime)
- Add `std::string`, `std::math`, `std::collections` incrementally
- Each module is a .quanta file + C runtime backing
- **Can start in parallel with Phase 1**

### What Can Be Built Right Now

The following can be implemented immediately with zero external dependencies:

1. **Module resolution** -- pure compiler work, no network, no new dependencies.
   Parse `mod` and `use` statements, resolve to files, build module tree.

2. **Multi-file C output** -- extend the C backend to produce one .c file
   from multiple .quanta sources, or produce multiple .c files and a Makefile.

3. **`quantac pkg init`** -- generate a Quanta.toml template. The `Manifest`
   type already has `to_toml()`.

4. **Local path dependencies** -- once multi-file works, supporting
   `path = "../other-pkg"` is straightforward: read the other package's
   Quanta.toml, find its src/ files, compile and merge.

### What Requires Other Features First

| Feature | Requires |
|---------|----------|
| Registry fetch | HTTP client + hosted server |
| `quantac pkg publish` | Registry server + auth system |
| Git dependencies | Git CLI or libgit2 |
| Build caching | Multi-file compilation |
| Cross-package type checking | Module resolution + name resolution |
| Workspaces | Local path dependencies |
