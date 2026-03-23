# QuantaLang Self-Hosting Progress

## Phase 6: Self-Hosting Roadmap

**Date:** 2026-03-21
**Goal:** Progressively compile the self-hosted QuantaLang codebase

---

## Files Attempted

### 1. `stdlib/core/option.quanta` (Option + Result types)

**Status: PARTIALLY COMPILED (simplified)**

Original features used:
- Generic enums: `enum Option<T> { None, Some(T) }`
- Generic impl blocks: `impl<T> Option<T> { ... }`
- Trait bounds on methods: `where T: Default`, `T: Copy`, `T: Deref`
- Closures as parameters: `FnOnce(T) -> U`, `FnOnce() -> T`
- References: `&self`, `&mut self`, `&T`
- `unsafe` blocks with `core::hint::unreachable_unchecked()`
- Derive macros: `#[derive(Clone, Copy, PartialEq, Eq, Hash)]`
- Lang items: `#[lang = "option"]`, `#[lang = "result"]`
- `pub use` re-exports
- Associated types: `type Output`, `type Residual`
- Never type: `!`
- Try trait / `?` operator support
- `ControlFlow` enum
- `impl` on nested generics: `impl<T> Option<Option<T>>`
- Module declarations and use statements

**What compiled:** Monomorphized `Option` enum (i32), `is_some`, `is_none`, `unwrap_or` methods. Free functions for `map` and `or` operations. See `tests/programs/30_self_hosted_option.quanta`.

**What blocked:**
- Generic type parameters (the compiler handles generics but not for enum self-return)
- Methods returning the same enum type (method resolution fails for `fn map_add(self) -> Option`)
- Closures as method parameters
- References (`&self` vs `self`)
- Trait bounds on type parameters

---

### 2. `stdlib/core/cmp.quanta` (Comparison traits + Ordering)

**Status: PARTIALLY COMPILED (simplified)**

Original features used:
- Trait definitions with supertraits: `Ord: Eq + PartialOrd<Self>`
- Default trait methods with references: `fn ne(&self, other: &Rhs) -> bool`
- Generic type parameter defaults: `PartialEq<Rhs: ?Sized = Self>`
- `?Sized` bounds
- Generic wrapper struct: `Reverse<T>(pub T)`
- Generic trait impls: `impl<T: PartialEq> PartialEq for Reverse<T>`
- `macro_rules!` for code generation (impl_cmp_int!, impl_cmp_float!, impl_tuple_cmp!)
- Never type impls: `impl PartialEq for !`
- Raw pointer impls: `impl<T: ?Sized> PartialEq for *const T`
- Reference impls: `impl<T: ?Sized + PartialEq> PartialEq for &T`
- Const generics: `impl<T: PartialEq, const N: usize> PartialEq for [T; N]`
- Slice impls: `impl<T: PartialEq> PartialEq for [T]`
- Iterator methods in slice comparison
- `#[repr(i8)]` enum representation

**What compiled:** `Ordering` enum with `is_lt`, `is_eq`, `is_gt` methods. Free functions for `reverse`, `then` (chaining), and `compare_i32`. See `tests/programs/31_self_hosted_cmp.quanta`.

**What blocked:**
- Trait definitions (compiler has traits but not the full supertraits/defaults needed)
- `&self`/`&Rhs` reference parameters in trait methods
- Generic type parameters with bounds
- Macros (`macro_rules!`)
- Never type
- Raw pointer types
- Const generics
- `#[repr(i8)]` attribute

---

### 3. `stdlib/core/primitives.quanta` (Primitive type methods)

**Status: NOT COMPILABLE**

Original features used:
- `macro_rules!` extensively (impl_uint!, impl_sint!)
- Impl blocks on primitive types: `impl u8 { ... }`, `impl f32 { ... }`
- `const fn` methods
- Intrinsic function calls: `intrinsics::ctpop`, `intrinsics::bswap`
- `#[inline]` attributes
- `#[cfg(target_endian)]` conditional compilation
- `unsafe` blocks with `intrinsics::transmute`
- Type casts: `self as $uty`
- Compile-time constant expressions: `[u8; $bits / 8]`
- `Self` type in return positions
- `Option<Self>` return type
- Tuple returns: `(Self, bool)`
- `panic!` macro
- `pub const` items
- `matches!` macro

**What blocked:**
- This file is almost entirely macro-driven -- cannot compile without macro support
- Impl blocks on primitive types (i32, f32, etc.) not supported
- Intrinsic functions
- `const fn`
- Conditional compilation (`#[cfg]`)

---

### 4. `stdlib/core/marker.quanta` (Marker traits)

**Status: NOT COMPILABLE**

This is pure trait definitions (Sized, Copy, Clone, Drop, Send, Sync, etc.) with:
- Auto traits: `pub unsafe auto trait Send {}`
- Negative impls: `impl<T: ?Sized> !Send for *const T {}`
- `#[lang = "..."]` attributes
- `PhantomData<T: ?Sized>` zero-sized generic type
- `#[repr(transparent)]` attribute

**What blocked:**
- Auto traits, negative impls, lang items -- all compiler-internal features
- This file defines the language's type system foundations, not compilable until the compiler itself understands these concepts natively

---

### 5. `src/lexer.quanta` (Self-hosted lexer)

**Status: STRUCTURE COMPILED, LOGIC NOT YET**

Original features used (1,179 lines):
- Lifetime parameters: `struct Lexer<'src>`, `impl<'src> Lexer<'src>`
- Byte slice references: `source: &'src [u8]`
- `&str` / `String` types
- `Vec<Token>` with `push`, `with_capacity`
- `Vec<u8>` for byte strings
- Method calls on `u8`: `.is_ascii_digit()`, `.is_ascii_alphanumeric()`
- Byte literal patterns: `b'('`, `b'0'..=b'9'`
- `Result<char, LexError>` return types
- `Option<IntSuffix>` return types
- Enum variants with associated data: `StringLiteral(String)`, `IntegerLiteral { value: u128, suffix: ... }`
- `match` on byte values with range patterns
- `String::from_utf8_lossy`, `.to_string()`
- `Ok()`, `Err()`, `Some()`, `None` return values
- `#[inline]` attributes
- `#[derive(Debug, Clone, PartialEq)]`
- `#[cfg(test)]` conditional test module
- `assert_eq!` and `matches!` macros
- `loop` with `break`
- Complex nested `if/else if/else` chains
- `continue` in loops

**What compiled (simplified):**
- The `Span` struct with `new`, `len`, `is_empty`, `contains_pos` methods
- A simplified `Token` struct
- The `TokenKind` enum (21-variant subset covering operators and punctuation)
- Character classification logic (simplified version of `scan_token`)
- `is_operator` query function

See `tests/programs/32_self_hosted_span.quanta` and `tests/programs/33_self_hosted_lexer_tokens.quanta`.

**What blocked full lexer compilation:**
- Lifetime parameters (`<'src>`)
- Byte slice references (`&'src [u8]`)
- String/Vec heap types as enum variant data
- Range patterns in match (`b'0'..=b'9'`)
- Method calls on primitive types (`.is_ascii_digit()`)
- `Result<T, E>` generic return types
- Byte literals (`b'x'`)
- `&mut self` methods (the lexer mutates its position)
- Array/slice indexing (`self.source[self.pos as usize]`)
- Type casts (`as usize`, `as char`, `as u32`)

---

### 6. `src/diagnostics/span.quanta` (Source spans)

**Status: CORE LOGIC COMPILED (simplified)**

Original features used (858 lines):
- Tuple structs: `BytePos(pub u32)`, `LineNumber(pub u32)`
- `From` trait impls
- Operator overloading (`Add`, `Sub`)
- `Display` trait impls
- `PathBuf`, `Arc`, `RwLock`, `HashMap`
- Generic `Spanned<T>` wrapper struct
- `impl<T: PartialEq> PartialEq for Spanned<T>`
- Iterator methods (`.binary_search()`, `.filter_map()`, `.collect()`)
- `Option<Span>` returns
- `impl Into<String>` parameter types
- `#[cfg(test)]` test module

**What compiled:** `Span` struct with `new`, `len`, `is_empty`, `contains_pos` methods, plus `span_merge` function. See `tests/programs/32_self_hosted_span.quanta`.

---

## Successfully Compiled Test Files

| Test | Source File(s) | Description | Status |
|------|---------------|-------------|--------|
| `30_self_hosted_option.quanta` | `stdlib/core/option.quanta` | Option enum with is_some, is_none, unwrap_or, map, or | Compiles + type-checks |
| `31_self_hosted_cmp.quanta` | `stdlib/core/cmp.quanta` | Ordering enum with is_lt/eq/gt, reverse, then, compare | Compiles + type-checks |
| `32_self_hosted_span.quanta` | `src/lexer.quanta` + `src/diagnostics/span.quanta` | Span struct with len, is_empty, contains, merge + Token | Compiles + type-checks |
| `33_self_hosted_lexer_tokens.quanta` | `src/lexer.quanta` | TokenKind enum (21 variants), classify_char, is_operator | Compiles + type-checks |

All four files pass `quantac check` and compile to C output.

---

## Features Needed for Full Self-Hosting

### Tier 1: High Priority (Blocks lexer compilation)

| Feature | Needed By | Status |
|---------|-----------|--------|
| `&self` / `&mut self` methods | Lexer, all stdlib | Not yet |
| Byte literals (`b'x'`) | Lexer | Not yet |
| Range patterns (`b'0'..=b'9'`) | Lexer | Not yet |
| Array/slice indexing (`arr[i]`) | Lexer | Basic arrays exist, not slices |
| Type casts (`x as u32`) | Lexer, primitives | Not yet |
| `String` as first-class type | Lexer, all of stdlib | Strings exist but not as enum data |
| `Vec<T>` with push/pop | Lexer | Collections exist but not generic |
| `Result<T, E>` generic | Lexer, option.quanta | Monomorphized only |
| `loop` with `break` value | Lexer | `loop` exists, not break-with-value |

### Tier 2: Medium Priority (Blocks stdlib compilation)

| Feature | Needed By | Status |
|---------|-----------|--------|
| Generic enums with methods returning Self | option.quanta, cmp.quanta | Blocked |
| Trait supertraits (`Ord: Eq + PartialOrd`) | cmp.quanta | Not yet |
| Default trait method implementations | cmp.quanta | Not yet |
| Closures as method parameters (`FnOnce`) | option.quanta | Closures exist, not as params |
| `const fn` | primitives.quanta | Not yet |
| `#[repr(i8)]` / `#[repr(transparent)]` | cmp.quanta, marker.quanta | Not yet |

### Tier 3: Lower Priority (Polish)

| Feature | Needed By | Status |
|---------|-----------|--------|
| Macro definitions (`macro_rules!`) | primitives.quanta, cmp.quanta | Not yet |
| Lifetime parameters (`<'a>`) | Lexer | Not yet |
| `unsafe` blocks | primitives.quanta | Not yet |
| Conditional compilation (`#[cfg]`) | primitives.quanta | Not yet |
| Auto traits / negative impls | marker.quanta | Not yet |
| Const generics (`const N: usize`) | cmp.quanta | Not yet |
| Never type (`!`) | option.quanta (Try) | Not yet |

---

## Recommended Next Steps

### Step 1: Methods returning own type (Immediate)
Fix method resolution for `impl Option { fn map(self) -> Option }`. This would let us move `option_map_add` and `option_or` from free functions into the impl block, matching the stdlib structure more closely.

### Step 2: References (`&self`, `&mut self`) (High Impact)
Almost every stdlib method uses `&self`. The lexer struct requires `&mut self` for position tracking. This is the single highest-impact feature for self-hosting progress.

### Step 3: Byte literals and type casts (Unlocks lexer)
Adding `b'x'` byte literals and `x as u32` type casts would let us compile significant portions of the lexer's character classification logic.

### Step 4: Range patterns in match (Unlocks lexer)
The lexer's `scan_token` method relies heavily on `b'0'..=b'9'` style range patterns. Adding these to match would let us compile the number scanning logic.

### Step 5: String/Vec as enum variant data (Unlocks token types)
The full `TokenKind` enum has variants like `StringLiteral(String)` and `Identifier(String)`. Being able to use heap types in enum variants would let us compile the complete token definition.

---

## Distance Assessment

| Component | Lines in Self-Hosted | Lines Compilable Now | Percentage |
|-----------|---------------------|---------------------|------------|
| Option type (core logic) | ~400 | ~50 (simplified) | ~12% |
| Ordering + compare | ~115 | ~80 (simplified) | ~70% |
| Span struct | ~210 | ~60 (simplified) | ~28% |
| TokenKind enum | ~215 | ~130 (simplified) | ~60% |
| Lexer struct + methods | ~810 | ~0 (needs &mut self) | 0% |
| Full stdlib/core | ~4000+ | ~0 (needs generics/traits) | 0% |

**Bottom line:** The compiler can handle the *data structures* and *matching logic* of the self-hosted codebase when simplified. The primary blockers are references (`&self`/`&mut self`), generic type resolution for methods, and heap types in enums. Addressing references alone would unlock roughly 30-40% of the lexer code.
