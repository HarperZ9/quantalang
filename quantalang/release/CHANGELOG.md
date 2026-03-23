# Changelog

All notable changes to QuantaLang will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2025-01-15

### 🎉 Initial Public Release

This is the first stable release of QuantaLang, a modern systems programming language
designed for safety, performance, and expressiveness.

### Language Features

#### Core Language
- **Ownership System**: Memory safety without garbage collection
- **Type Inference**: Powerful type inference with explicit annotations when needed
- **Pattern Matching**: Exhaustive pattern matching with guards
- **Generics**: Generic types and functions with trait bounds
- **Traits**: Interface abstraction with default implementations
- **Enums**: Algebraic data types with associated data
- **Closures**: Anonymous functions capturing environment
- **Async/Await**: First-class asynchronous programming support

#### Type System
- Primitive types: `bool`, `i8`-`i128`, `u8`-`u128`, `f32`, `f64`, `char`
- Compound types: tuples, arrays, slices, references
- User-defined types: structs, enums, type aliases
- Const generics for compile-time array sizes
- Lifetime annotations for reference validity

#### Control Flow
- `if`/`else` expressions
- `match` expressions with pattern guards
- `for`/`while`/`loop` loops
- `break`/`continue` with labels
- Early return with `return`

### Standard Library

#### Collections (`std::collections`)
- `Vec<T>`: Dynamic array
- `HashMap<K, V>`: Hash-based key-value map
- `BTreeMap<K, V>`: Ordered B-tree map
- `String`: UTF-8 encoded string

#### I/O (`std::io`)
- File operations: `File`, `OpenOptions`
- Buffered I/O: `BufReader`, `BufWriter`
- Traits: `Read`, `Write`, `BufRead`, `Seek`
- Standard streams: `stdin`, `stdout`, `stderr`

#### Networking (`std::net`)
- TCP: `TcpListener`, `TcpStream`
- UDP: `UdpSocket`
- HTTP client with async support
- Address types: `SocketAddr`, `IpAddr`

#### Concurrency (`std::sync`)
- `Mutex<T>`: Mutual exclusion lock
- `RwLock<T>`: Read-write lock
- `Arc<T>`: Atomic reference counting
- `Channel<T>`: Message passing channel
- Atomic types: `AtomicBool`, `AtomicU64`, etc.

#### Text Processing
- `std::regex`: Regular expression matching
- `std::json`: JSON parsing and serialization
- `std::base64`: Base64 encoding/decoding

#### Security
- `std::crypto`: SHA-256, SHA-512, BLAKE3, HMAC, PBKDF2
- `std::rand`: Cryptographically secure random generation
- `std::uuid`: UUID generation

#### Compression
- `std::compress`: gzip, zlib, deflate

#### Time
- `std::time`: Duration, Instant, DateTime

#### System
- `std::env`: Environment variables, arguments
- `std::path`: Filesystem path manipulation
- `std::process`: Process spawning and management

### Compiler

#### Targets
- x86_64 (Linux, macOS, Windows)
- AArch64 (Linux, macOS)
- WebAssembly (wasm32)
- RISC-V (riscv64)

#### Optimizations
- 36 optimization passes
- LLVM-based code generation
- Link-time optimization (LTO)
- Profile-guided optimization (PGO)

#### Diagnostics
- Helpful error messages with source locations
- Suggestions for common mistakes
- Warnings for potential issues

### Tooling

#### Built-in Tools
- `quanta build`: Compile projects
- `quanta run`: Build and run
- `quanta test`: Run tests
- `quanta bench`: Run benchmarks
- `quanta fmt`: Format code
- `quanta lint`: Static analysis
- `quanta doc`: Generate documentation
- `quanta repl`: Interactive REPL

#### Package Management
- `quanta new`: Create new project
- `quanta add`: Add dependencies
- `quanta remove`: Remove dependencies
- `quanta update`: Update dependencies
- `quanta publish`: Publish package

#### IDE Support
- Language Server Protocol (LSP) implementation
- VS Code extension
- IntelliJ plugin
- Vim/Neovim support
- Emacs mode

### Documentation
- Getting Started Guide
- Language Reference
- Standard Library API Reference
- Tutorials and Examples

---

## [0.9.0] - 2024-12-01 (Beta)

### Added
- Async/await syntax
- HTTP client in std::net
- JSON module
- Regex module
- Compression module

### Changed
- Improved error messages
- Faster compilation times
- Better type inference

### Fixed
- Various parser edge cases
- Memory leak in async runtime
- Unicode handling in strings

---

## [0.8.0] - 2024-10-15 (Beta)

### Added
- WebAssembly target
- Const generics
- Module system improvements

### Changed
- Reworked trait resolution
- Updated standard library layout

---

## [0.7.0] - 2024-08-01 (Alpha)

### Added
- Basic async support
- Networking primitives
- Package manager prototype

---

## [0.6.0] - 2024-06-01 (Alpha)

### Added
- Generics with trait bounds
- Pattern matching improvements
- Standard library foundations

---

## [0.5.0] - 2024-04-01 (Alpha)

### Added
- Initial trait system
- Basic collections
- File I/O

---

## [0.1.0] - 2024-01-15 (Alpha)

### Added
- Initial language prototype
- Basic type system
- Simple code generation

---

## Versioning Policy

QuantaLang follows Semantic Versioning:

- **Major (X.0.0)**: Breaking changes to language or standard library
- **Minor (0.X.0)**: New features, backward-compatible
- **Patch (0.0.X)**: Bug fixes, backward-compatible

### Stability Guarantees

Starting with 1.0.0:
- Language syntax is stable
- Standard library APIs are stable
- Compiled programs will continue to work
- Deprecations will have at least one minor version warning

### Edition System

Future breaking changes will be managed through editions:
- Editions are opt-in via `edition` in `quanta.toml`
- Old code continues to work unchanged
- Automatic migration tools provided

---

[1.0.0]: https://github.com/quantalang/quantalang/releases/tag/v1.0.0
[0.9.0]: https://github.com/quantalang/quantalang/releases/tag/v0.9.0
[0.8.0]: https://github.com/quantalang/quantalang/releases/tag/v0.8.0
[0.7.0]: https://github.com/quantalang/quantalang/releases/tag/v0.7.0
[0.6.0]: https://github.com/quantalang/quantalang/releases/tag/v0.6.0
[0.5.0]: https://github.com/quantalang/quantalang/releases/tag/v0.5.0
[0.1.0]: https://github.com/quantalang/quantalang/releases/tag/v0.1.0
