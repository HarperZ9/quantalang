# QuantaLang Roadmap

**The Effects Language** — Algebraic effects as a first-class feature.

## Current State (v0.1.0+)

### Verified Working (55/55 end-to-end tests pass)
- Variables, functions, if/else, while, for (range), match expressions
- Structs, enums (tagged unions), match destructuring
- Strings (QuantaString with concat, compare, len)
- Arrays (creation, indexing)
- Methods (impl blocks with self)
- Closures (non-capturing and capturing lambdas)
- Higher-order functions (function pointers)
- Generics (monomorphization with type inference, multi-type-parameter)
- Generic structs (`Pair<T>`, `KeyValue<K,V>` — monomorphized at construction site)
- Generic enums (`Option<T>`, `Result<T,E>` — monomorphized, construct, match, unit variants)
- Modules (mod declarations, path resolution)
- Algebraic effects (define, perform, handle — proven with `Hello from effects!`)
- Dynamic dispatch (vtables, trait objects, fat pointers)
- Format strings (type-aware: i32, i64, f64, bool, str, debug, precision)
- Math built-ins (sqrt, sin, cos, pow, abs, min, max)
- Auto-compile pipeline (quantac build discovers gcc/clang/cl.exe)
- C backend (fully verified), LLVM backend (wired into CLI)

### Compiler Stats
- 70K+ lines of Rust
- 474 unit tests pass
- 55 end-to-end test programs verified with MSVC
- Hindley-Milner type inference with effect rows
- 6 code generation backends (C verified, LLVM wired, 4 need integration)

### Full-Year Roadmap
See [ROADMAP_2026_2027.md](ROADMAP_2026_2027.md) for the Apr 2026 — Mar 2027 plan.

---

## Phase 1: Language Completeness (v0.2.0)

### 1.1 Trait System in Codegen
- **What**: `trait Display { fn display(self) -> str; }` with `impl Display for Point { ... }`
- **Why**: Required for generic programming, operator overloading, and stdlib design
- **Effort**: Medium — type checker already has trait resolution, codegen needs vtable or monomorphization dispatch
- **Approach**: Static dispatch via monomorphization (like Rust, no runtime cost)

### 1.2 Pattern Matching Completeness
- **What**: Nested patterns, guard clauses, or-patterns, tuple destructuring in match
- **Why**: Core to the language's expressiveness, especially for effect handlers
- **Effort**: Medium — parser supports most patterns, codegen needs guard evaluation and nested destructuring

### 1.3 Error Handling Integration
- **What**: `Result<T, E>` and `Option<T>` types that work with both `?` operator AND effects
- **Why**: Pragmatic — not everyone will use effects for error handling immediately
- **Effort**: Low-Medium — `?` is partially implemented in type checker, needs codegen

### 1.4 Closures with Captures
- **What**: `let x = 42; let f = |y| x + y;` — closures that capture environment
- **Why**: Essential for functional programming, iterators, callbacks
- **Effort**: Medium — requires generating a capture struct + function pointer pair

### 1.5 Iterator Protocol
- **What**: `for item in collection { ... }` with trait-based iteration
- **Why**: Core language ergonomic, blocks stdlib collections
- **Effort**: Medium — depends on trait dispatch (1.1)

---

## Phase 2: Standard Library (v0.3.0)

### 2.1 Core Types
- `Option<T>` — None/Some with pattern matching
- `Result<T, E>` — Ok/Err with `?` operator
- `Vec<T>` — Dynamic array (backed by QuantaVec runtime)
- `String` — Owned string (backed by QuantaString runtime)
- `HashMap<K, V>` — Hash map

### 2.2 Core Traits
- `Display` — String formatting
- `Debug` — Debug formatting
- `Clone`, `Copy` — Value semantics
- `Eq`, `Ord` — Comparison
- `Iterator` — Iteration protocol
- `Into`, `From` — Type conversion

### 2.3 I/O
- `File` — Read/write files (via C stdio)
- `stdin`, `stdout`, `stderr` — Standard streams
- `print!`, `println!`, `eprintln!` — Already partially working

### 2.4 Math
- Constants: `PI`, `E`, `TAU`
- Functions: Already working via C math.h
- Numeric traits: `Add`, `Sub`, `Mul`, `Div` for operator overloading

---

## Phase 3: Backend Verification (v0.4.0)

### 3.1 LLVM Backend Completion
- **Current**: Generates `.ll` files, wired into CLI
- **Needed**: FieldAccess/VariantField support, end-to-end test verification
- **Benefit**: Optimized native code via LLVM's optimization passes

### 3.2 WASM Backend
- **Current**: 69K lines, outputs WAT text format
- **Needed**: Fix Goto → structured control flow (br/block/loop), FieldAccess support, binary emission
- **Benefit**: Run QuantaLang in browsers, edge computing, serverless

### 3.3 x86-64 Backend
- **Current**: 58K lines + 57K encoder, generates assembly
- **Needed**: Assembler/linker integration (invoke `nasm` + `ld`), register allocator, FieldAccess
- **Benefit**: Direct native compilation without C or LLVM dependency

### 3.4 ARM64 Backend
- **Current**: 59K lines + 65K encoder
- **Needed**: Same as x86-64 but for ARM hardware or QEMU
- **Benefit**: Apple Silicon, mobile, embedded

### 3.5 SPIR-V Backend
- **Current**: 63K lines, produces valid SPIR-V binary headers
- **Needed**: Vulkan host integration, buffer I/O, compute kernel patterns
- **Benefit**: GPU compute from QuantaLang — effects on the GPU

---

## Phase 4: Ecosystem (v0.5.0)

### 4.1 Package Manager
- **Existing infrastructure**: manifest.rs (Quanta.toml), lockfile.rs, resolver.rs (PubGrub), version.rs (SemVer)
- **Needed**: `quantac pkg init`, `quantac pkg add`, dependency fetching, build caching
- **Registry**: pkg.quantalang.org (future hosted service)

### 4.2 Module System Expansion
- **Current**: `mod foo;` resolves `foo.quanta` in same directory
- **Needed**: Nested modules (`mod foo::bar`), `pub` visibility, `use foo::*` glob imports
- **Benefit**: Real library organization

### 4.3 LSP Completion
- **Current**: 6,448 lines, all providers implemented, server loop is stub
- **Needed**: Wire JSON-RPC dispatch, add `quantac lsp` subcommand, test with VS Code
- **Benefit**: IDE support (autocomplete, go-to-definition, inline errors)

### 4.4 Formatter
- **Current**: 1,631 lines with unit tests
- **Needed**: CLI subcommand `quantac fmt`, opinionated defaults
- **Benefit**: Consistent code style across the ecosystem

---

## Phase 5: Effects Ecosystem (v1.0.0)

This is what makes QuantaLang worth choosing over Rust, Go, or any other language.

### 5.1 Standard Effect Library
```quanta
// Built-in effects that every program can use
effect IO { fn read(path: str) -> str, fn write(path: str, content: str) -> () }
effect Async { fn spawn(task: fn() -> ()) -> (), fn yield_() -> () }
effect State<S> { fn get() -> S, fn set(value: S) -> () }
effect Fail<E> { fn fail(error: E) -> ! }
effect Log { fn log(level: str, message: str) -> () }
effect Random { fn random() -> f64 }
```

### 5.2 Effect Composition
- Multiple effects on one function: `fn process() ~ IO, Fail<str>, Log -> Result`
- Effect polymorphism: `fn map<F, A, B, E>(f: fn(A) ~ E -> B, items: Vec<A>) ~ E -> Vec<B>`
- Effect elimination: compiler proves pure functions have no effects

### 5.3 Resumable Effects (Multi-Shot)
- **Current**: One-shot handlers via setjmp/longjmp
- **Needed**: Coroutine-based resumable handlers for generators, async streams
- **Approach**: Stackful coroutines or CPS transform

### 5.4 Effect-Based Concurrency
- Async/await as syntax sugar for the Async effect
- Green threads via effect handlers
- Structured concurrency via scoped effects

### 5.5 Effect-Based Testing
```quanta
// Test a function by providing mock handlers
#[test]
fn test_file_processor() {
    let mut log = Vec::new();
    handle {
        process_files()
    } with {
        IO.read(path) => |resume| { resume("mock file content") },
        Log.log(_, msg) => |resume| { log.push(msg); resume(()) },
    }
    assert(log.len() > 0);
}
```

---

## Phase 6: Self-Hosting (v2.0.0)

### 6.1 Compile the Self-Hosted Compiler
- The `quantalang/quantalang/src/` directory contains 248 `.quanta` files
- Progressively compile more as the Rust compiler gains features
- Milestone: the QuantaLang compiler can compile its own lexer

### 6.2 Compile the Standard Library
- The `quantalang/quantalang/stdlib/` directory contains 31 `.quanta` files
- Start with `core::option`, `core::cmp`, `core::ops`
- Work up to `alloc::vec`, `alloc::string`, `std::io`

### 6.3 Bootstrap
- QuantaLang compiler compiles itself
- No longer depends on Rust
- The language stands on its own

---

## Non-Goals (By Design)

- **Not a systems language**: QuantaLang doesn't compete with Rust on memory safety. It uses GC and effects instead.
- **Not a scripting language**: QuantaLang compiles to native code. It's not interpreted.
- **Not OOP**: No classes, no inheritance. Structs + traits + effects.
- **Not gradual typing**: Every expression has a type. Effects are tracked in the type system.

---

## Timeline

| Phase | Version | Focus |
|-------|---------|-------|
| Current | v0.1.0 | 19 features verified, effects proven, modules working |
| Phase 1 | v0.2.0 | Traits, pattern matching, captures, iterators |
| Phase 2 | v0.3.0 | Standard library (Option, Result, Vec, HashMap, I/O) |
| Phase 3 | v0.4.0 | LLVM verified, WASM working, native backends |
| Phase 4 | v0.5.0 | Package manager, LSP, formatter, ecosystem |
| Phase 5 | v1.0.0 | Effect standard library, resumable effects, concurrency |
| Phase 6 | v2.0.0 | Self-hosting compiler |

---

*QuantaLang: Better error handling than Rust. Better concurrency than Go. Mathematical purity with practical power.*
