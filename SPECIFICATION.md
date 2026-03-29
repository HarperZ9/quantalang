# QuantaLang Language Specification

Version 1.0 — March 2026

## 1. Lexical Structure

### 1.1 Source Encoding
Source files are UTF-8 encoded. Identifiers may contain Unicode characters following UAX #31 (`XID_Start` followed by `XID_Continue*`).

### 1.2 Whitespace and Comments
Whitespace characters (space, tab, newline, carriage return) separate tokens but are otherwise insignificant.

```
// Line comment (to end of line)
/* Block comment (may nest) */
/// Outer doc comment
//! Inner doc comment
```

Block comments nest: `/* /* inner */ outer */` is valid.

### 1.3 Keywords
```
fn let mut const static struct enum trait impl
if else match for while loop break continue return
true false self Self super
use mod pub
async await move unsafe extern
type where as in
effect handle perform resume
```

### 1.4 Literals

**Integer literals:**
```
42          // decimal (inferred integer type)
42i32       // explicitly i32
0xFF        // hexadecimal
0o77        // octal
0b1010      // binary
1_000_000   // underscores for readability
```

Integer types: `i8`, `i16`, `i32`, `i64`, `i128`, `isize`, `u8`, `u16`, `u32`, `u64`, `u128`, `usize`.

Unsuffixed integer literals have type `{integer}` — a type variable constrained to resolve to a concrete integer type through inference.

**Float literals:**
```
3.14        // inferred float type
3.14f32     // explicitly f32
2.5e10      // scientific notation
```

Float types: `f32`, `f64`. Unsuffixed float literals have type `{float}`.

**String literals:**
```
"hello"         // str (owned string)
b"bytes"        // &'static [u8; 5]
'c'             // char
b'A'            // u8
r#"raw string"# // raw string (no escape processing)
```

**Boolean literals:** `true`, `false`.

### 1.5 Operators and Delimiters

**Arithmetic:** `+`, `-`, `*`, `/`, `%`
**Comparison:** `==`, `!=`, `<`, `>`, `<=`, `>=`
**Logical:** `&&`, `||`, `!`
**Bitwise:** `&`, `|`, `^`, `~`, `<<`, `>>`
**Assignment:** `=`, `+=`, `-=`, `*=`, `/=`, `%=`, `&=`, `|=`, `^=`, `<<=`, `>>=`
**Reference:** `&`, `&mut`, `*` (deref)
**Other:** `->`, `=>`, `..`, `..=`, `?`, `|>` (pipe)

**Delimiters:** `(`, `)`, `[`, `]`, `{`, `}`, `,`, `;`, `:`, `.`

### 1.6 Operator Precedence

From lowest to highest binding power:

| Level | Operators | Associativity |
|-------|-----------|---------------|
| 1 | `=`, `+=`, `-=`, etc. | Right |
| 2 | `..`, `..=` | None |
| 3 | `\|\|` | Left |
| 4 | `&&` | Left |
| 5 | `==`, `!=`, `<`, `>`, `<=`, `>=` | Left |
| 6 | `\|` | Left |
| 7 | `^` | Left |
| 8 | `&` | Left |
| 9 | `<<`, `>>` | Left |
| 10 | `\|>` (pipe) | Left |
| 11 | `+`, `-` | Left |
| 12 | `*`, `/`, `%` | Left |
| 13 | `as` (cast) | Left |
| 14 | `-` (unary), `!`, `~`, `&`, `&mut`, `*` | Prefix |
| 15 | `.`, `()`, `[]`, `?` | Postfix |

## 2. Type System

### 2.1 Primitive Types
- `bool` — boolean (true/false)
- `char` — Unicode scalar value
- `i8`, `i16`, `i32`, `i64`, `i128`, `isize` — signed integers
- `u8`, `u16`, `u32`, `u64`, `u128`, `usize` — unsigned integers
- `f32`, `f64` — IEEE 754 floating point
- `str` — owned string
- `()` — unit type (zero-size)
- `!` — never type (no values)

### 2.2 Compound Types
- `(T, U, V)` — tuple
- `[T; N]` — fixed-size array
- `[T]` — slice
- `&T` — shared reference
- `&mut T` — mutable reference
- `*const T`, `*mut T` — raw pointers
- `fn(T) -> U` — function pointer

### 2.3 User-Defined Types

**Structs:**
```
struct Point { x: f64, y: f64 }        // named fields
struct Wrapper(i32)                      // tuple struct
struct Unit                              // unit struct
```

**Enums:**
```
enum Option<T> { Some(T), None }
enum Color { Red, Green, Blue }
```

**Type aliases:**
```
type Result<T> = std::result::Result<T, Error>;
```

### 2.4 Generics
```
fn identity<T>(x: T) -> T { x }
struct Pair<A, B> { first: A, second: B }
```

Generic type parameters are monomorphized at each call site. Each distinct instantiation generates a separate function.

### 2.5 Trait Bounds
```
fn print_it<T: Display>(x: T) { ... }
fn complex<T>(x: T) where T: Clone + Debug { ... }
```

### 2.6 Type Inference

QuantaLang uses **bidirectional type inference**:
- **Synthesis** (bottom-up): the type of an expression is computed from its subexpressions
- **Checking** (top-down): an expected type flows down to constrain subexpressions

Unsuffixed integer literals resolve through inference: in `let x: u64 = 42`, the `42` is inferred as `u64`. Without context, integer literals default to `i32` and float literals to `f64`.

### 2.7 Type Annotations

Types may carry string annotations for domain-specific checking:
```
type LinearRGB = @[ColorSpace:Linear] Vec3;
```

The unifier checks annotation compatibility: if both operands carry annotations in the same category, they must match.

## 3. Ownership and Borrowing

### 3.1 References
```
let r: &i32 = &x;          // shared reference
let mr: &mut i32 = &mut x; // mutable reference
let v: i32 = *r;            // dereference
```

### 3.2 Borrowing Rules
1. A value may have **either** one mutable reference **or** any number of shared references at any point in time (no aliasing).
2. A mutable reference cannot be created while any other reference (shared or mutable) to the same value is active.
3. References cannot outlive the value they refer to (no returning references to local variables).

### 3.3 Non-Lexical Lifetimes
Borrows expire at last use, not at scope end:
```
let r = &x;
println!("{}", *r);  // last use of r
let mr = &mut x;     // OK: r's borrow has expired
```

### 3.4 Scope-Based Expiry
Borrows created in an inner scope expire when that scope ends:
```
{
    let r = &mut x;
} // r's borrow expires here
let r2 = &x; // OK
```

## 4. Items

### 4.1 Functions
```
fn name(param: Type, ...) -> ReturnType {
    body
}
```
The last expression in a function body is the implicit return value.

### 4.2 Structs
```
struct Name {
    field: Type,
}

impl Name {
    fn method(&self) -> Type { ... }
    fn method_mut(&mut self) { ... }
}
```

### 4.3 Enums
```
enum Name {
    Variant,
    Variant(Type),
    Variant { field: Type },
}
```

Match expressions over enum types must be exhaustive:
```
match color {
    Color::Red => ...,
    Color::Green => ...,
    Color::Blue => ...,   // all variants covered
}
```
Missing variants produce a compile-time error.

### 4.4 Traits
```
trait Name {
    fn method(&self) -> Type;
}

impl Name for ConcreteType {
    fn method(&self) -> Type { ... }
}
```

### 4.5 Modules
```
mod name {          // inline module
    pub fn foo() {}
}

mod name;           // external module (loads name.quanta from disk)

use module::item;   // import single item
use module::*;      // glob import
use module::{a, b}; // nested import
```

## 5. Expressions

### 5.1 Control Flow
```
if condition { ... } else { ... }
if let pattern = expr { ... }
match expr { pattern => expr, ... }
while condition { ... }
for pattern in iterator { ... }
loop { ... }
break;
break value;
continue;
return value;
```

### 5.2 Closures
```
let f = |x, y| x + y;
let g = |x: i32| -> i32 { x * 2 };
let captured = move || use_captured_var;
```

### 5.3 Pattern Matching
```
match value {
    42 => ...,              // literal
    x => ...,               // binding
    _ => ...,               // wildcard
    (a, b) => ...,          // tuple
    Point { x, y } => ..., // struct
    Some(v) => ...,         // enum variant
    A | B => ...,           // or-pattern
    x if x > 0 => ...,     // guard
}
```

## 6. Algebraic Effects

### 6.1 Effect Declaration
```
effect Error {
    fn raise(msg: str) -> !;
}
```

### 6.2 Performing Effects
```
fn might_fail() {
    perform Error.raise("something went wrong");
}
```

### 6.3 Handling Effects
```
handle Error {
    might_fail()
} with {
    raise(msg) => {
        println!("caught: {}", msg);
        resume(default_value)
    }
}
```

Effects use one-shot continuations (setjmp/longjmp). `resume` may be called at most once per `perform`.

## 7. Standard Library

### 7.1 Core Module (`core.quanta`)
Constants: `pi()`, `e()`, `tau()`
Integer: `i32_min`, `i32_max`, `i32_abs`, `i32_clamp`
Float: `f64_min`, `f64_max`, `f64_abs`, `f64_clamp`
Predicates: `is_even`, `is_odd`, `is_positive`, `is_negative`, `is_zero`
Number theory: `gcd`, `lcm`, `factorial`, `fibonacci`, `power`

### 7.2 Math Module (`math.quanta`)
Angle conversion: `deg_to_rad`, `rad_to_deg`
Interpolation: `lerp`, `smoothstep`, `inverse_lerp`

### 7.3 Built-in Functions
I/O: `println!`, `print!`, `eprintln!`
Collections: `vec_new`, `vec_push`, `vec_get`, `vec_len`, `vec_pop`
HashMap: `map_new`, `map_insert`, `map_get`, `map_contains`, `map_len`
String: `to_string_i32`, `to_string_f64`
File I/O: `read_file`, `write_file`, `file_exists`
Environment: `getenv`, `clock_ms`, `args_count`, `args_get`
Math: `sqrt`, `sin`, `cos`, `tan`, `log`, `exp`, `pow`, `abs`, `floor`, `ceil`
Assert: `assert`, `assert_eq`

## 8. Code Generation

### 8.1 Compilation Targets
- **C99** (primary): Portable, debuggable, uses system C compiler for native output
- **HLSL**: DirectX shaders and ReShade effects
- **GLSL**: OpenGL and Vulkan shaders
- **SPIR-V**: GPU compute and graphics (binary format)
- **LLVM IR**: Textual LLVM intermediate representation
- **WebAssembly**: Web and edge deployment (text format)
- **x86-64**: Experimental native machine code
- **ARM64**: Experimental native machine code

### 8.2 C Backend
References compile to pointers. Structs compile to C structs. Enums compile to tagged unions. The runtime library (~190 C functions) is embedded in every output.

## 9. Grammar Summary (EBNF)

```
module     = item* ;
item       = function | struct_def | enum_def | trait_def | impl_def
           | type_alias | const_def | static_def | mod_def | use_def ;
function   = "fn" IDENT generics? "(" params? ")" ("->" type)? block ;
struct_def = "struct" IDENT generics? ( "{" fields "}" | "(" types ")" | ";" ) ;
enum_def   = "enum" IDENT generics? "{" variants "}" ;
block      = "{" stmt* expr? "}" ;
stmt       = local | expr_stmt | item ;
local      = "let" "mut"? pattern (":" type)? ("=" expr)? ";" ;
expr       = literal | ident | path | unary | binary | call | method_call
           | field | index | if | match | for | while | loop | block
           | closure | ref | deref | return | break | continue ;
pattern    = "_" | ident | literal | tuple | struct | enum | or | slice ;
type       = path | "&" lifetime? "mut"? type | "[" type ";" expr "]"
           | "(" types ")" | "fn" "(" types ")" "->" type ;
```

## 10. Conformance

A conforming implementation must:
1. Accept all programs that conform to this specification
2. Reject programs that violate the borrowing rules (Section 3.2)
3. Reject non-exhaustive match expressions over enum types (Section 4.3)
4. Produce correct output for all built-in operations on primitive types
5. Implement the precedence table in Section 1.6
