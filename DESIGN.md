# QuantaLang Compiler Design

> Architectural documentation for the QuantaLang compiler (`quantac`).
> ~80K lines of Rust across 85 source files.

## Pipeline Overview

```
source.quanta
    |
    v
Preprocessor (include!() expansion, double-inclusion guard)
    |
    v
Lexer (src/lexer/) -----> Token stream with Spans
    |
    v
Parser (src/parser/) ---> Untyped AST (src/ast/)
    |
    v
Type Checker (src/types/)
  Pass 1: collect_item() -- register all types, traits, impls
  Pass 2: check_item()   -- type check all items
  Expression inference via TypeInfer + Unifier
    |
    v
Validated AST
    |
    v
MIR Lowerer (src/codegen/lower/) --> MirModule (SSA-style IR)
    |
    v
Backend (src/codegen/backend/)
    |
    +---> C99 (primary, production)
    +---> LLVM IR (textual .ll)
    +---> WebAssembly (text format)
    +---> SPIR-V (GPU shaders)
    +---> HLSL (DirectX / ReShade)
    +---> GLSL (OpenGL / Vulkan)
    +---> x86-64 (experimental native)
    +---> ARM64 (experimental native)
```

## Lexer (`src/lexer/`)

**~4,910 lines** across 6 files: `scanner.rs` (2,302), `token.rs` (1,385), `span.rs` (409), `cursor.rs` (347), `error.rs` (467), `mod.rs` (80+).

The `Lexer` struct in `scanner.rs` consumes a `SourceFile` via a character `Cursor` and produces a `Vec<Token>`. Each `Token` carries a `TokenKind` and a `Span` (byte offset range + source ID) for error reporting.

Key capabilities:
- **Unicode identifiers** (UAX #31) via `is_id_start`/`is_id_continue` in `cursor.rs`
- **All numeric literal formats**: decimal, hex (`0x`), octal (`0o`), binary (`0b`), float, scientific notation, with numeric suffix validation (`i32`, `f64`, etc.)
- **String literals**: regular, raw (`r#"..."#`), byte strings (`b"..."`), with escape sequences and Unicode escapes
- **Interpolated strings**: tracked via `InterpolatedPart` for string template expansion
- **Nested block comments**: `/* ... /* ... */ ... */` handled by depth tracking in the scanner
- **DSL block recognition**: `sql!`, `regex!`, `math!`, etc. detected via `is_dsl_name()` and returned as special tokens
- **Lifetime annotations**: `'a`, `'static` etc.
- **Doc comments**: `///` (outer) and `//!` (inner) extracted via `tokenize_with_docs()`
- **Configurable**: `LexerConfig` controls preservation of whitespace/comment tokens and shebang handling

The `Keyword` enum covers the full language keyword set including `fn`, `let`, `struct`, `enum`, `trait`, `impl`, `match`, `if`, `for`, `while`, `loop`, `async`, `await`, `unsafe`, `move`, `box`, `dyn`, `module`, `effect`, `handle`, `resume`, and more.

## Parser (`src/parser/`)

**~4,940 lines** across 7 files: `expr.rs` (1,764), `item.rs` (1,523), `pattern.rs` (631), `ty.rs` (473), `stmt.rs` (277), `error.rs` (272), `mod.rs` (100+).

A **recursive descent parser** with **Pratt parsing** (top-down operator precedence) for expressions.

### Architecture

- **`Parser` struct**: holds the token stream, current position, accumulated errors, and `Restrictions` flags (e.g., `no_struct_literal` to disambiguate `if x { ... }` from struct literals)
- **`parse_module()`**: entry point. Parses inner attributes, then iterates `parse_item()` with error recovery via `recover_to_item()`
- **Items** (`item.rs`): functions, structs, enums, traits, impls, type aliases, consts, statics, modules, use declarations, extern blocks, macro rules, effect declarations
- **Statements** (`stmt.rs`): local bindings (`let`), expression statements, semicolon statements, item statements
- **Expressions** (`expr.rs`): Pratt parser with explicit binding power levels:
  - Assignment (1) < Range (2) < Or (3) < And (4) < Compare (5) < BitOr (6) < BitXor (7) < BitAnd (8) < Shift (9) < Pipe (10) < Sum (11) < Product (12) < Cast (13) < Prefix (14) < Postfix (15)
  - Prefix expressions: literals, identifiers, blocks, `if`, `match`, `for`, `while`, `loop`, closures, unary ops
  - Postfix expressions: method calls, field access, index, `?` (try), function calls
  - Infix expressions: all binary operators, `as` casts, range operators
- **Types** (`ty.rs`): paths, references, pointers, arrays, slices, tuples, function types, `dyn Trait`, `impl Trait`
- **Patterns** (`pattern.rs`): identifiers, literals, tuples, structs, enums, wildcards, ranges, `or` patterns

### Produced AST (`src/ast/`)

**~1,730 lines** across 7 files. The AST is an untyped tree of `Item`, `Stmt`, `Expr`, `Pattern`, and `Type` nodes. Each node carries a `Span`. Item kinds include: `Function`, `Struct`, `Enum`, `Trait`, `Impl`, `TypeAlias`, `Const`, `Static`, `Mod`, `Use`, `ExternCrate`, `ExternBlock`, `Macro`, `MacroRules`, `Effect`.

## Type System (`src/types/`)

**~7,635 lines** across 11 files: `infer.rs` (2,445), `check.rs` (1,132), `ty.rs` (835), `effects.rs` (876), `hkt.rs` (699), `traits.rs` (648), `context.rs` (500+), `unify.rs` (400+), `const_generics.rs`, `builtins.rs`, `error.rs`.

### Two-Pass Approach

**`TypeChecker`** (`check.rs`) orchestrates module-level type checking:

1. **Collection pass** (`collect_item`): walks all items and registers type definitions (structs, enums, type aliases), trait definitions, function signatures, and impl blocks into the `TypeContext`. Also registers prelude constructors (`Ok`, `Err`, `Some`, `None`) and built-in vector/matrix struct types (`vec2`, `vec3`, `vec4`, `mat4`) for shader support. After user types are collected, `register_builtin_traits()` ensures built-in trait stubs (like `Iterator`, `Display`) have consistent `DefId`s.

2. **Checking pass** (`check_item`): type-checks each item. For functions, it delegates expression-level inference to `TypeInfer`.

### Type Inference (`infer.rs`)

**`TypeInfer`** implements **bidirectional type inference** combining:
- **Synthesis**: infer the type of an expression bottom-up
- **Checking**: verify an expression against an expected type top-down

The engine uses a `Unifier` for constraint solving and a `TraitResolver` for trait method lookup. It tracks the current function's return type, effect row, and whether an explicit `return` was found.

Key type state:
- `WellKnownTypes`: cached `DefId`s for `Range<T>`, `Option<T>`, `Result<T, E>` etc.
- `trait_env` / `builtin_traits`: trait resolution environment
- `effect_ctx` / `current_effects`: algebraic effect tracking

### Unification (`unify.rs`)

The `Unifier` maintains a `Substitution` (mapping from type variables to types) and implements the standard unification algorithm:
- Equal types unify trivially
- A type variable unifies with any type (occurs check)
- Structured types (tuples, references, etc.) unify component-wise
- **Type annotations** (e.g., `ColorSpace:Linear`) are checked for compatibility: if both types carry annotations in the same category, they must match (prevents accidental mixing of color spaces)

### Type Context (`context.rs`)

`TypeContext` is the central registry holding:
- **Scope stack**: `Vec<Scope>` where each scope has variable bindings (`name -> TypeScheme`) and type parameters. Scope kinds: `Module`, `Function`, `Block`, `Loop`, `Match`
- **Type definitions**: `HashMap<DefId, TypeDef>` (structs, enums)
- **Trait definitions**: `HashMap<DefId, TraitDef>` with implementations (`Vec<TraitImpl>`)
- **Function signatures**: `HashMap<DefId, FnSig>`
- **Inherent methods**: `(type_name, method_name) -> TraitMethod` for resolving method calls on user types without traits
- **Type parameter bounds**: `param_name -> [trait_name, ...]` for resolving trait methods on generic type parameters

### Additional Type System Features

- **Traits** (`traits.rs`, 648 lines): `TraitEnv`, `TraitResolver`, `BuiltinTraits` for trait resolution and method lookup
- **Higher-kinded types** (`hkt.rs`, 699 lines): kind system for type constructors
- **Algebraic effects** (`effects.rs`, 876 lines): `EffectContext`, `EffectRow` for tracking and checking effect annotations
- **Const generics** (`const_generics.rs`): compile-time constant values as type parameters

## MIR (Mid-level IR) (`src/codegen/ir.rs`)

**1,631 lines.** An SSA-style intermediate representation organized as a control-flow graph.

### Module Structure

`MirModule` is the compilation unit containing:
- `functions: Vec<MirFunction>` -- each with basic blocks, locals, optional shader stage
- `globals: Vec<MirGlobal>` -- module-level variables
- `types: Vec<MirTypeDef>` -- struct/enum definitions
- `strings: Vec<Arc<str>>` -- interned string table (deduped via `intern_string()`)
- `externals: Vec<MirExternal>` -- FFI declarations
- `vtables: Vec<MirVtable>` -- dynamic dispatch tables: `(trait_name, type_name, [(method, mangled_fn, sig)])`
- `trait_methods: HashMap<Arc<str>, Vec<(Arc<str>, MirFnSig)>>` -- trait method signatures
- `uniforms: Vec<MirUniform>` -- shader uniform declarations

### Functions and Blocks

`MirFunction` has a `MirFnSig` (params, return type, variadic, calling convention) and an `Option<Vec<MirBlock>>` (None = declaration, Some = definition). Each `MirBlock` holds a list of `MirStmt` and a `MirTerminator`.

### Statements (`MirStmtKind`)

| Kind | Description |
|------|-------------|
| `Assign { dest, value }` | `local = rvalue` |
| `DerefAssign { ptr, value }` | `*ptr = value` |
| `FieldAssign { base, field_name, value }` | `local.field = value` |
| `FieldDerefAssign { ptr, field_name, value }` | `ptr->field = value` |
| `StorageLive(local)` | Mark local as valid |
| `StorageDead(local)` | Mark local as invalid |
| `Nop` | No-op placeholder |

### RValues (`MirRValue`)

`Use`, `BinaryOp`, `UnaryOp`, `Ref`, `AddressOf`, `Cast`, `Aggregate` (tuple/struct/array/enum variant/closure), `Repeat`, `Discriminant`, `Len`, `FieldAccess`, `VariantField`, `IndexAccess`, `Deref`, `TextureSample`.

### Terminators (`MirTerminator`)

`Goto`, `If` (conditional branch), `Switch` (multi-way), `Call` (with dest, target, unwind), `Return`, `Unreachable`, `Drop`, `Assert`, `Resume`, `Abort`.

### MIR Types (`MirType`)

`Void`, `Bool`, `Int(size, signed)`, `Float(size)`, `Ptr`, `Array`, `Slice`, `Struct`, `FnPtr`, `Never`, `Vector(elem, lanes)`, `Texture2D`, `Sampler`, `SampledImage`, `TraitObject` (fat pointer), `Vec` (heap handle), `Map` (heap handle), `Tuple`.

Integer sizes: I8, I16, I32, I64, I128, ISize. Float sizes: F32, F64. SIMD vector constructors: `v4f32`, `v8f32`, `v2f64`, `v4f64`, `v4i32`, `v8i32`, `v16i8`, `v32i8`.

## Lowering (`src/codegen/lower/`)

**8,048 lines** split into 4 modules. The `MirLowerer` struct walks the validated AST and builds MIR via `MirBuilder` and `MirModuleBuilder`.

### `mod.rs` (1,609 lines) -- Item and Function Lowering

`MirLowerer` holds:
- **`ctx`**: read-only reference to the `TypeContext` from type checking
- **`module`**: `MirModuleBuilder` accumulating the output
- **`var_map`**: per-function mapping of variable names to `LocalId`s
- **`loop_stack`**: `(continue_block, break_block)` for loop lowering
- **`impl_methods`**: `(TypeName, MethodName) -> mangled_fn_name`
- **`generic_functions`** / **`generic_structs`** / **`generic_enums`** / **`generic_impls`**: stored ASTs for deferred monomorphization
- **`monomorphized`**: set of already-generated specialization names
- **`trait_methods`** / **`trait_impls`**: for vtable generation
- **`closure_captures`** / **`local_closure_name`**: closure capture tracking
- **`module_prefix`**: stack for inline `pub mod` name mangling
- **`tuple_type_defs`**: deduplication for generated tuple structs

Entry point: `lower_module()` iterates all AST items, lowering functions, structs, enums, traits, impls, consts, statics, and effect declarations. Generic items are stored for later monomorphization when a concrete instantiation is encountered.

### `expr.rs` (3,634 lines) -- Expression and Statement Lowering

The largest module. Handles:
- **Block/statement lowering**: `lower_block()`, `lower_stmt()`, `lower_local()` (let bindings with destructuring)
- **Expression lowering**: `lower_expr()` dispatches on `ExprKind` -- literals, identifiers, binary ops, unary ops, field access, method calls, function calls, index access, `if`/`match`/`for`/`while`/`loop`, closures, struct init, enum variant construction, tuple construction, array literals, range expressions, `?` (try), `as` casts, block expressions, `return`/`break`/`continue`
- **Method call resolution**: checks `impl_methods` registry, falls back to builtin methods on Vec/HashMap/String
- **Pattern matching**: `match` arms with guard expressions, destructuring, wildcards, `or` patterns
- **Loop desugaring**: `for` loops desugar to iterator protocol (`.iter()` + `while` with `.next()`)

### `types.rs` (777 lines) -- Type Lowering and Const Evaluation

- **`lower_type_from_ast()`**: converts AST types to `MirType`. Handles `Never`, `Infer` (defaults to i32), `Tuple`, `Array` (with const eval for length), `Slice`, `Ptr`, `Ref`, named types (resolves structs, enums, builtins like `Vec<T>`, `HashMap<K,V>`, `String`)
- **`try_const_eval()`**: evaluates constant expressions at compile time for array lengths and const generics
- **Generic monomorphization**: `monomorphize_function()`, `monomorphize_struct()`, `monomorphize_enum()` -- clones the generic AST, substitutes type parameters, and lowers the specialized version with a mangled name

### `macros.rs` (2,028 lines) -- Closures, Builtins, Iterator Desugaring

- **Closure lowering**: `collect_free_vars()` finds captured variables by walking the closure body and comparing against the enclosing scope's `var_map`. Captures are appended as extra parameters to the generated closure function. `closure_captures` and `local_closure_name` registries enable the caller to pass captured values at call sites.
- **Builtin macro expansion**: `println!`, `format!`, `vec!`, `assert!`, `dbg!`, `todo!`, `unimplemented!`, `env!`, etc.
- **Iterator chain desugaring**: `IterChain` with `IterStep` variants (`Map`, `Enumerate`, `Cloned`) and `IterTerminal` (`ForEach`, `Collect`, `Count`, `Sum`, `Any`, `All`, `Find`, `Filter`). Chains like `v.iter().map(|x| x*2).filter(|x| x>5).collect()` are desugared into fused loops.
- **Effect lowering**: `handle`/`resume` for algebraic effects

## Backends (`src/codegen/backend/`)

**~14,368 lines** across 10 files. All backends implement the `Backend` trait, consuming a `MirModule` and producing `GeneratedCode`.

### C Backend (Primary) -- `c.rs` (2,530 lines)

The production backend. `CBackend` emits C99-compliant code:

1. **Standard includes**: `stdint.h`, `stdbool.h`, `stdio.h`, `stdlib.h`, `string.h`, `math.h`, `time.h`
2. **Embedded runtime**: the full `runtime_header()` is inlined (QuantaString, QuantaVec, QuantaHashMap, print helpers, math builtins)
3. **Type definitions**: structs, enums (tagged unions)
4. **Vtable types and instances**: for `dyn Trait` dynamic dispatch
5. **String table**: interned string literals
6. **Forward declarations**: all function prototypes
7. **Function bodies**: basic blocks emitted as labeled statements with `goto`-based control flow

The generated C is compiled to native executables via `gcc` (Linux/MSYS2) or MSVC (`cl.exe` on Windows). The `quantac build` command handles this automatically, with `--emit c` to stop at the C source stage and `--keep-c` to preserve intermediates.

### LLVM Backend -- `llvm.rs` (2,255 lines)

Emits LLVM IR in textual format (`.ll` files). Maps MIR types to LLVM types (`i32`, `double`, `%struct.Name`, etc.), emits SSA instructions, and generates correct `phi` nodes at block joins.

### WebAssembly Backend -- `wasm.rs` (2,095 lines)

Emits WebAssembly text format (`.wat`). Maps MIR types to Wasm value types (`i32`, `i64`, `f32`, `f64`), uses linear memory for structs and arrays, and generates Wasm function imports for runtime support.

### SPIR-V Backend -- `spirv.rs` (4,403 lines)

The largest backend. Emits SPIR-V binary words for GPU compute and graphics shaders. Handles:
- Descriptor set / binding decorations for uniform buffers, textures, samplers
- Input/output variable decorations with locations
- `OpTypeImage` / `OpTypeSampler` / `OpTypeSampledImage` for texture sampling
- Shader entry point generation with execution model annotations
- Built-in variable access (`gl_Position`, `gl_FragCoord`, etc.)

### HLSL Backend -- `hlsl.rs` (988 lines)

Emits High-Level Shading Language for DirectX and ReShade. Supports optional ReShade `.fx` boilerplate wrapping.

### GLSL Backend -- `glsl.rs` (799 lines)

Emits GLSL for OpenGL and Vulkan shader source.

### x86-64 Backend -- `x86_64.rs` (1,642 lines) + `x86_64_enc.rs`

Experimental direct native code generation. Emits x86-64 machine code with instruction encoding in `x86_64_enc.rs`. System V AMD64 ABI calling convention.

### ARM64 Backend -- `arm64.rs` (1,656 lines) + `arm64_enc.rs`

Experimental direct native code generation for AArch64. Instruction encoding in `arm64_enc.rs`. AAPCS64 calling convention.

## Runtime (`src/codegen/runtime.rs`)

**1,890 lines** of Rust that generates ~187 C static functions embedded in every compiled program's output. The runtime provides:

- **QuantaString**: ptr/len/cap representation. Concat, length, equality, substring, split, trim, contains, starts_with, ends_with, replace, to_uppercase/lowercase, char_at, free
- **QuantaVec**: generic dynamic array (void* + len + cap + elem_size). Push, get, pop, free. Type-specialized handle variants for i32, i64, f64 with heap-allocated backing
- **QuantaHashMap**: open-addressing hash map with string keys. Put, get, contains, remove, keys iteration, free
- **Format helpers**: `quanta_format_i32`, `quanta_format_f64`, `quanta_format_str` -- snprintf wrappers returning QuantaString
- **Print helpers**: type-specialized print functions for formatted output
- **Math builtins**: `quanta_min`, `quanta_max`, `quanta_abs`, `quanta_pow`, `quanta_sqrt`, trigonometric functions
- **File I/O**: `quanta_read_file`, `quanta_write_file`, `quanta_file_exists`
- **Environment**: `quanta_getenv`, `quanta_clock`
- **I/O initialization**: `__quanta_init_io()` disables stdout/stderr buffering

## Preprocessor (`src/main.rs`)

**~80 lines** of preprocessing logic. Before tokenization, the source text is scanned for `include!("path")` directives.

`preprocess_includes()` implements:
- **Line-by-line scanning** for `include!("...")` directives
- **Path resolution** relative to the including file's directory
- **Double-inclusion guard**: `HashSet<PathBuf>` of canonical paths. Already-included files are silently replaced with a comment
- **Recursion depth limit**: `MAX_INCLUDE_DEPTH` prevents circular includes
- **Recursive expansion**: included files' own `include!()` directives are expanded transitively

This is a pragmatic text-level preprocessor rather than a proper module system. It works well for the current stdlib (`include!("../stdlib/lines.quanta")`) and multi-file programs. A proper module system with namespaced imports is planned.

## Macro System (`src/macro_expand/`)

**~2,389 lines** across 5 files: `builtins.rs` (734), `pattern.rs` (563), `mod.rs` (462), `expand.rs` (341), `hygiene.rs` (289).

Supports `macro_rules!` definitions with pattern matching and template expansion. `builtins.rs` provides built-in macros (`println!`, `vec!`, `assert!`, etc.). `hygiene.rs` implements macro hygiene to prevent name collisions.

## Language Server Protocol (`src/lsp/`)

**~5,794 lines** across 12 files. A full LSP server implementation providing:
- **Diagnostics** (`diagnostics.rs`): real-time error reporting
- **Completion** (`completion.rs`): context-aware code completion
- **Go to Definition** (`definition.rs`): jump to symbol definition
- **Hover** (`hover.rs`): type and documentation display
- **Document Symbols** (`symbols.rs`): outline view
- **Code Actions** (`actions.rs`): quick fixes and refactorings
- **Transport** (`transport.rs`): JSON-RPC over stdin/stdout

## Formatter (`src/fmt/`)

**~1,626 lines** across 4 files. Accessed via `quantac fmt`. `formatter.rs` implements a pretty-printer that reformats QuantaLang source with configurable options (`config.rs`): indent width, max line width, trailing commas, etc.

## Package Manager (`src/pkg/`)

**~3,347 lines** across 6 files. Accessed via `quantac pkg`. Implements:
- `manifest.rs`: `Quanta.toml` parsing
- `lockfile.rs`: lockfile generation and reading
- `resolver.rs`: dependency resolution
- `registry.rs`: package registry client
- `version.rs`: semantic versioning

## CLI Commands (`src/main.rs`)

**2,651 lines.** The `quantac` binary supports these subcommands via `clap`:

| Command | Description |
|---------|-------------|
| `quantac <file>` | Compile a file (default) |
| `quantac build` | Build a project (`--emit c`/`exe`, `--target c`/`llvm`/`wasm`/`spirv`/`hlsl`/`glsl`, `--keep-c`) |
| `quantac run <file>` | Compile and run |
| `quantac lex <file>` | Tokenize and print tokens |
| `quantac parse <file>` | Parse and print AST (with `--json` option) |
| `quantac check <file>` | Type-check only |
| `quantac fmt <file>` | Format source (`--check`, `--write`) |
| `quantac lsp` | Start LSP server |
| `quantac watch <path>` | Watch and recompile shaders |
| `quantac pkg init/add/resolve/search` | Package management |
| `quantac version` | Print version info |

## Key Design Decisions

### Why MIR?

The mid-level IR decouples the frontend (lexer, parser, type checker) from the backends. This provides several benefits:

1. **Backend independence**: adding a new backend (e.g., GLSL, HLSL) requires only implementing the `Backend` trait against MIR, not re-implementing AST traversal
2. **Optimization opportunities**: MIR's SSA form and basic-block structure enable future optimization passes (constant folding, dead code elimination, inlining) that benefit all backends
3. **Simplification**: MIR eliminates high-level constructs (match, for-in, closures, iterator chains) into basic blocks with gotos and calls, making backend code generation straightforward

### Why C as the primary backend?

1. **Portability**: C99 compiles on virtually every platform with mature toolchains (gcc, clang, MSVC)
2. **Mature optimizers**: gcc -O2/-O3 and MSVC /O2 provide decades of optimization work for free
3. **Easy debugging**: the generated C is readable, and standard debuggers (gdb, lldb, Visual Studio) work directly on the output
4. **Bootstrapping**: a C backend can compile the self-hosted compiler on any machine with a C compiler, without requiring LLVM or custom codegen infrastructure
5. **Incremental development**: new language features can be tested immediately through C emission without building native codegen for each target

### Why flat structs in generated code?

Struct field assignment went through several iterations:

1. **Initial approach**: struct fields were set only via aggregate initialization (`Struct { field: value }`)
2. **Problem**: this forced constructing entire structs at once, making incremental field modification impossible
3. **Fix** (task #107): `FieldAssign` and `FieldDerefAssign` MIR statements were added, generating `local.field = value` and `ptr->field = value` in C. This required the C backend to emit structs as flat C structs with named fields rather than opaque blobs
4. **Result**: QuantaLang structs map directly to C structs, and field assignment generates direct field stores. This keeps the generated code simple and cache-friendly

### Why `include!()` instead of proper modules?

Pragmatic choice driven by development priorities:

1. **Immediate need**: the stdlib (`lines.quanta`, `args.quanta`, `string_pool.quanta`, `tokenizer.quanta`) needed to be sharable across 60+ programs
2. **Simple implementation**: ~80 lines of text-level preprocessing vs. hundreds of lines for a proper module resolver with namespaces, visibility rules, and separate compilation
3. **Works now**: programs use `include!("../stdlib/lines.quanta")` and get textual inclusion with double-include guards and recursion depth limits
4. **Proper modules planned**: the AST already has `ItemKind::Mod` and `ItemKind::Use`, and the package manager has dependency resolution. Wiring these into the compiler pipeline for namespace-qualified imports is the next major infrastructure milestone

### Why SSA with basic blocks (not tree-based codegen)?

The MIR uses SSA form with explicit basic blocks rather than directly walking the AST during code generation:

1. **Control flow clarity**: `if`/`match`/`loop` become explicit branch/goto graphs, making it impossible to miscompile nested control flow
2. **Backend simplicity**: each backend only needs to emit straight-line code per block plus terminators, rather than handling recursive AST patterns
3. **Future optimization**: SSA is the standard form for dataflow analysis, enabling future passes like GVN, LICM, and register allocation for the native backends

## Type System Design Rationale

### Why bidirectional inference instead of Algorithm W?

Algorithm W (Damas-Milner) infers types purely bottom-up: it synthesizes the type of every expression, then unifies at usage sites. This works for Haskell-style languages where every expression has exactly one principal type.

QuantaLang has features that break the Algorithm W assumption:
- **Integer literal overloading**: `42` could be `i8`, `i32`, `i64`, `u32`, etc. Algorithm W would either default to one type or require explicit annotation on every literal.
- **Struct literal disambiguation**: `Point { x: 1, y: 2 }` needs to know the target type to resolve which `Point` is being constructed when there are multiple types with the same name across modules.
- **Method call resolution**: `x.foo()` requires knowing the type of `x` to look up `foo` in the correct impl. With traits, there may be multiple `foo` methods, and only the expected return type can disambiguate.

Bidirectional inference solves this by combining synthesis (bottom-up) with checking (top-down). When a `let x: i32 = 42;` is encountered, the `i32` annotation flows *down* into the literal, constraining it directly. When calling `f(42)` where `f` expects `u64`, the expected parameter type flows down to resolve the literal.

The cost is implementation complexity — `infer_expr` (synthesis) and `check_expr` (checking) are separate code paths that must stay in sync. In practice, `check_expr` delegates to `infer_expr` for most node types and only intervenes where top-down information is useful (literals, closures, struct literals).

### Why Pratt parsing for expressions?

Recursive descent handles statements and items well, but expression parsing with 15 precedence levels would require 15 mutually recursive functions (`parse_or_expr` calling `parse_and_expr` calling `parse_compare_expr` calling...). Adding a new precedence level means inserting a function in the middle of the chain and updating all callers.

Pratt parsing collapses this into a single loop driven by a binding power table. Adding a new operator means adding one entry to `infix_binding_power()`. The code is shorter (the entire expression parser is one function with helpers) and the precedence relationships are explicit in the `bp` module rather than implicit in the call graph.

The tradeoff: Pratt parsing is harder to read for someone unfamiliar with the technique. The `parse_expr_with_bp(min_bp)` function is a tight loop that's not obviously correct on first read. The 42 precedence tests exist partly to compensate for this — they prove the binding powers are correct even though the code is dense.

### Why setjmp/longjmp for algebraic effects?

Algebraic effects need non-local control flow: `perform` jumps from the effect site to the enclosing `handle` block, and `resume` continues execution after the `perform`. There are three implementation strategies:

1. **CPS transform**: Rewrite every effectful function into continuation-passing style. Correct but doubles code size and makes generated C unreadable.
2. **Stack switching**: Use fibers or coroutines. Correct and efficient but requires platform-specific assembly (ucontext on Linux, fibers on Windows) and defeats C compiler optimizations.
3. **setjmp/longjmp**: Use the C runtime's non-local goto. Works on every platform, no assembly needed, zero overhead when no effect is performed.

We chose setjmp/longjmp because it matches the C backend's portability goal. The handler saves state with `setjmp`, the body runs normally, and `perform` calls `longjmp` to jump back to the handler with an operation ID. The handler dispatches on the ID and can `resume` by calling the continuation function directly.

The limitation: `longjmp` destroys stack frames between the handler and the perform site, so `resume` can only be called once (one-shot continuations). Multi-shot continuations (calling `resume` multiple times for the same `perform`) would require stack copying, which setjmp doesn't support. In practice, one-shot is sufficient for error handling, async simulation, and resource management — the primary use cases.

### Why color space annotations in the type system?

Color science has a class of bugs that type systems normally can't catch: passing a linear-light RGB value to a function expecting sRGB, or mixing Display P3 and Rec.709 primaries. These are all `(f32, f32, f32)` at the type level but semantically incompatible.

QuantaLang's type annotations attach metadata strings (like `ColorSpace:Linear` or `ColorSpace:sRGB`) to types. The unifier checks annotation compatibility: if both operands of a binary operation carry annotations in the same category, they must match. This catches `linear_rgb + srgb_rgb` at compile time.

The design is intentionally minimal — annotations are strings, not a full dependent type system. They're checked structurally (category:value matching) rather than requiring a dedicated solver. This keeps the type checker simple while catching the most common class of color space bugs.

The limitation: annotations are per-type, not per-value. If a function takes `Vec3` and you want one `Vec3` to be linear and another to be sRGB, you need different type aliases. This is a pragmatic compromise — full dependent types would be more expressive but dramatically more complex.

### Known Limitations

1. **Generics are monomorphized eagerly**: every generic instantiation generates a separate function. No polymorphic compilation. This means compile times scale with the number of instantiations, not the number of generic definitions.
2. **Partial borrow checking**: The compiler enforces basic borrowing rules: no mutable aliasing (`&mut` while `&` or `&mut` is active), no returning references to local variables, and scope-based borrow expiry. References are properly typed (`&x` → `Ref(T)`, `*non_ref` → error). However, the borrow checker does not yet implement: (a) NLL — borrows expire at scope boundaries, not at last use, (b) interprocedural lifetime analysis — function signatures don't carry lifetime parameters, (c) full region inference with constraint solving. The C backend still emits raw pointers. These are the next items for the borrow checker.
3. **Module system is partial**: Inline `mod foo { ... }` blocks work with proper scoping, and `use` statements resolve through a module registry. However, external file modules (`mod foo;` loading from `foo.quanta`) and the `include!()` preprocessor are not yet unified into a single module resolver. Separate compilation and incremental builds are not supported.
4. **Effect system is one-shot only**: `resume` can be called at most once per `perform` due to the setjmp/longjmp implementation. This is a deliberate trade-off for C backend portability — CPS transform would enable multi-shot but doubles code size and makes generated C unreadable.

### Resolved (Previously Listed as Limitations)

- **Pattern exhaustiveness** (resolved in v1.0.2): The compiler now performs exhaustiveness checking on match expressions over enum types. Missing variants produce a type error naming the uncovered variants. Wildcard and binding patterns are recognized as catch-all arms. The check resolves the enum from pattern paths when the scrutinee type is an unresolved inference variable.

## Source File Index

### Core Pipeline
| File | Lines | Purpose |
|------|-------|---------|
| `src/lexer/scanner.rs` | 2,302 | Main tokenizer implementation |
| `src/lexer/token.rs` | 1,385 | Token and keyword definitions |
| `src/parser/expr.rs` | 1,764 | Pratt expression parser |
| `src/parser/item.rs` | 1,523 | Item (function, struct, trait) parsing |
| `src/types/infer.rs` | 2,445 | Type inference engine |
| `src/types/check.rs` | 1,132 | Type checker coordinator |
| `src/codegen/ir.rs` | 1,631 | MIR definition |
| `src/codegen/lower/mod.rs` | 1,609 | Item/function lowering |
| `src/codegen/lower/expr.rs` | 3,634 | Expression/statement lowering |
| `src/codegen/lower/types.rs` | 777 | Type lowering, monomorphization |
| `src/codegen/lower/macros.rs` | 2,028 | Closures, builtins, iterators |
| `src/codegen/backend/c.rs` | 2,530 | C99 backend (primary) |
| `src/codegen/runtime.rs` | 1,890 | Embedded C runtime library |
| `src/main.rs` | 2,651 | CLI, preprocessor, compile pipeline |

### Backends
| File | Lines | Purpose |
|------|-------|---------|
| `src/codegen/backend/spirv.rs` | 4,403 | SPIR-V GPU shader backend |
| `src/codegen/backend/llvm.rs` | 2,255 | LLVM IR backend |
| `src/codegen/backend/wasm.rs` | 2,095 | WebAssembly backend |
| `src/codegen/backend/arm64.rs` | 1,656 | ARM64 native (experimental) |
| `src/codegen/backend/x86_64.rs` | 1,642 | x86-64 native (experimental) |
| `src/codegen/backend/hlsl.rs` | 988 | HLSL shader backend |
| `src/codegen/backend/glsl.rs` | 799 | GLSL shader backend |

### Type System
| File | Lines | Purpose |
|------|-------|---------|
| `src/types/effects.rs` | 876 | Algebraic effect system |
| `src/types/ty.rs` | 835 | Core type representation |
| `src/types/hkt.rs` | 699 | Higher-kinded types |
| `src/types/traits.rs` | 648 | Trait resolution |
| `src/types/context.rs` | ~500 | Type environment and scopes |
| `src/types/unify.rs` | ~400 | Unification algorithm |
