# QuantaLang Language Reference

This document provides a complete reference for the QuantaLang programming language.

## Table of Contents

1. [Lexical Structure](#lexical-structure)
2. [Types](#types)
3. [Expressions](#expressions)
4. [Statements](#statements)
5. [Functions](#functions)
6. [Structs and Enums](#structs-and-enums)
7. [Traits](#traits)
8. [Generics](#generics)
9. [Error Handling](#error-handling)
10. [Modules](#modules)
11. [Macros](#macros)
12. [Attributes](#attributes)
13. [Memory Model](#memory-model)
14. [Concurrency](#concurrency)

---

## Lexical Structure

### Keywords

```
as      async   await   break   const   continue
crate   dyn     else    enum    extern  false
fn      for     if      impl    in      let
loop    match   mod     move    mut     pub
ref     return  self    Self    static  struct
super   trait   true    type    union   unsafe
use     where   while   yield
```

### Operators

| Category | Operators |
|----------|-----------|
| Arithmetic | `+` `-` `*` `/` `%` |
| Comparison | `==` `!=` `<` `>` `<=` `>=` |
| Logical | `&&` `\|\|` `!` |
| Bitwise | `&` `\|` `^` `~` `<<` `>>` |
| Assignment | `=` `+=` `-=` `*=` `/=` `%=` `&=` `\|=` `^=` `<<=` `>>=` |
| Range | `..` `..=` |
| Other | `?` `@` `::` `->` `=>` |

### Comments

```quanta
// Line comment

/* Block comment */

/// Documentation comment for items
//! Documentation comment for enclosing item

/** 
 * Multi-line documentation 
 */
```

### Literals

```quanta
// Integers
42          // Decimal
0x2A        // Hexadecimal
0o52        // Octal
0b101010    // Binary
1_000_000   // With separators

// Floats
3.14
2.5e10
1.0e-5

// Characters
'a'
'\n'        // Escape
'\u{1F600}' // Unicode

// Strings
"hello"
"line1\nline2"
r"raw string with \n literal"
r#"can contain "quotes""#

// Byte strings
b"hello"
br"raw bytes"

// Boolean
true
false
```

---

## Types

### Primitive Types

| Type | Description | Size |
|------|-------------|------|
| `bool` | Boolean | 1 byte |
| `i8`, `i16`, `i32`, `i64`, `i128` | Signed integers | 1-16 bytes |
| `u8`, `u16`, `u32`, `u64`, `u128` | Unsigned integers | 1-16 bytes |
| `isize`, `usize` | Pointer-sized integers | Platform |
| `f32`, `f64` | Floating point | 4-8 bytes |
| `char` | Unicode scalar | 4 bytes |
| `()` | Unit type | 0 bytes |
| `!` | Never type | 0 bytes |

### Compound Types

```quanta
// Tuples
let pair: (i32, String) = (42, "hello".to_string());
let (x, y) = pair;  // Destructuring

// Arrays (fixed size)
let arr: [i32; 5] = [1, 2, 3, 4, 5];
let zeros: [i32; 100] = [0; 100];

// Slices (views into arrays)
let slice: &[i32] = &arr[1..4];

// References
let ref_x: &i32 = &x;
let ref_mut_x: &mut i32 = &mut x;

// Pointers (unsafe)
let ptr: *const i32 = &x;
let ptr_mut: *mut i32 = &mut x;
```

### Type Aliases

```quanta
type Result<T> = std::result::Result<T, Error>;
type Callback = fn(i32) -> i32;
type BoxedFuture<T> = Box<dyn Future<Output = T>>;
```

### Never Type

```quanta
fn infinite_loop() -> ! {
    loop { }
}

fn panic_always() -> ! {
    panic!("This function never returns")
}
```

---

## Expressions

### Arithmetic Expressions

```quanta
let sum = a + b;
let diff = a - b;
let product = a * b;
let quotient = a / b;
let remainder = a % b;
let power = a.pow(2);
```

### Comparison Expressions

```quanta
a == b  // Equality
a != b  // Inequality
a < b   // Less than
a > b   // Greater than
a <= b  // Less or equal
a >= b  // Greater or equal
```

### Logical Expressions

```quanta
a && b  // Short-circuit AND
a || b  // Short-circuit OR
!a      // Negation
```

### Block Expressions

```quanta
let result = {
    let x = compute();
    let y = process(x);
    x + y  // Last expression is the value
};
```

### If Expressions

```quanta
let max = if a > b { a } else { b };

let category = if score >= 90 {
    "A"
} else if score >= 80 {
    "B"
} else if score >= 70 {
    "C"
} else {
    "F"
};
```

### Match Expressions

```quanta
let description = match value {
    0 => "zero",
    1 => "one",
    2..=9 => "single digit",
    10 | 20 | 30 => "round number",
    n if n < 0 => "negative",
    _ => "other",
};

// Destructuring in match
match point {
    Point { x: 0, y } => println!("on y-axis at {}", y),
    Point { x, y: 0 } => println!("on x-axis at {}", x),
    Point { x, y } => println!("at ({}, {})", x, y),
}
```

### Loop Expressions

```quanta
// Infinite loop (with break value)
let result = loop {
    if condition {
        break 42;
    }
};

// While loop
while condition {
    // ...
}

// While let
while let Some(item) = iter.next() {
    process(item);
}

// For loop
for i in 0..10 {
    println!("{}", i);
}

// For with pattern
for (index, value) in array.iter().enumerate() {
    println!("{}: {}", index, value);
}

// Loop labels
'outer: for i in 0..10 {
    for j in 0..10 {
        if condition {
            break 'outer;
        }
    }
}
```

### Closure Expressions

```quanta
// Basic closure
let add = |a, b| a + b;

// With type annotations
let multiply: fn(i32, i32) -> i32 = |a, b| a * b;

// Capturing environment
let multiplier = 3;
let triple = |x| x * multiplier;

// Move closure
let data = vec![1, 2, 3];
let closure = move || {
    println!("{:?}", data);
};
```

---

## Statements

### Let Statements

```quanta
// Immutable binding
let x = 42;

// Mutable binding
let mut y = 0;

// With type annotation
let z: i64 = 100;

// Pattern destructuring
let (a, b) = (1, 2);
let Point { x, y } = point;
let [first, second, ..] = array;
```

### Expression Statements

```quanta
x + y;  // Expression evaluated, result discarded
func(); // Function call
```

---

## Functions

### Function Definition

```quanta
fn function_name(param1: Type1, param2: Type2) -> ReturnType {
    // body
}

// No return value
fn greet(name: &str) {
    println!("Hello, {}!", name);
}

// With return type
fn add(a: i32, b: i32) -> i32 {
    a + b
}

// Early return
fn find(items: &[i32], target: i32) -> Option<usize> {
    for (i, &item) in items.iter().enumerate() {
        if item == target {
            return Some(i);
        }
    }
    None
}
```

### Associated Functions and Methods

```quanta
struct Circle {
    radius: f64,
}

impl Circle {
    // Associated function (constructor)
    fn new(radius: f64) -> Self {
        Circle { radius }
    }
    
    // Method (takes self)
    fn area(&self) -> f64 {
        std::f64::consts::PI * self.radius * self.radius
    }
    
    // Mutable method
    fn scale(&mut self, factor: f64) {
        self.radius *= factor;
    }
    
    // Consuming method
    fn into_diameter(self) -> f64 {
        self.radius * 2.0
    }
}
```

### Generic Functions

```quanta
fn largest<T: Ord>(list: &[T]) -> &T {
    let mut largest = &list[0];
    for item in list {
        if item > largest {
            largest = item;
        }
    }
    largest
}

// Multiple type parameters
fn combine<A, B, C>(a: A, b: B, f: fn(A, B) -> C) -> C {
    f(a, b)
}

// Where clauses
fn process<T, U>(t: T, u: U) -> T
where
    T: Clone + Debug,
    U: AsRef<str>,
{
    println!("{:?}", t);
    t.clone()
}
```

### Async Functions

```quanta
async fn fetch_data(url: &str) -> Result<String, Error> {
    let response = http::get(url).await?;
    response.text().await
}

// Calling async functions
async fn main() {
    let data = fetch_data("https://example.com").await.unwrap();
    println!("{}", data);
}
```

---

## Structs and Enums

### Struct Definition

```quanta
// Named fields
struct Point {
    x: f64,
    y: f64,
}

// Tuple struct
struct Color(u8, u8, u8);

// Unit struct
struct Marker;

// Generic struct
struct Container<T> {
    value: T,
    count: usize,
}

// With lifetime
struct Borrowed<'a> {
    data: &'a str,
}
```

### Struct Instantiation

```quanta
// Named fields
let p = Point { x: 1.0, y: 2.0 };

// Field shorthand
let x = 1.0;
let y = 2.0;
let p = Point { x, y };

// Update syntax
let p2 = Point { x: 3.0, ..p };

// Tuple struct
let red = Color(255, 0, 0);
```

### Enum Definition

```quanta
// Simple enum
enum Direction {
    North,
    South,
    East,
    West,
}

// With associated data
enum Message {
    Quit,
    Move { x: i32, y: i32 },
    Write(String),
    ChangeColor(u8, u8, u8),
}

// Generic enum
enum Option<T> {
    Some(T),
    None,
}

enum Result<T, E> {
    Ok(T),
    Err(E),
}
```

### Pattern Matching on Enums

```quanta
fn handle_message(msg: Message) {
    match msg {
        Message::Quit => {
            println!("Quitting");
        }
        Message::Move { x, y } => {
            println!("Moving to ({}, {})", x, y);
        }
        Message::Write(text) => {
            println!("Writing: {}", text);
        }
        Message::ChangeColor(r, g, b) => {
            println!("Changing color to ({}, {}, {})", r, g, b);
        }
    }
}
```

---

## Traits

### Trait Definition

```quanta
trait Drawable {
    // Required method
    fn draw(&self);
    
    // Provided method (default implementation)
    fn draw_twice(&self) {
        self.draw();
        self.draw();
    }
    
    // Associated constant
    const DEFAULT_SIZE: u32 = 100;
    
    // Associated type
    type Output;
    
    // Associated function
    fn create() -> Self where Self: Sized;
}
```

### Trait Implementation

```quanta
impl Drawable for Circle {
    type Output = f64;
    
    fn draw(&self) {
        println!("Drawing circle with radius {}", self.radius);
    }
    
    fn create() -> Self {
        Circle { radius: 1.0 }
    }
}
```

### Trait Bounds

```quanta
// In function signature
fn print_debug<T: Debug>(value: T) {
    println!("{:?}", value);
}

// Multiple bounds
fn compare<T: PartialOrd + Debug>(a: T, b: T) {
    if a > b {
        println!("{:?} > {:?}", a, b);
    }
}

// Where clause
fn complex<T, U>(t: T, u: U)
where
    T: Clone + Debug + PartialEq,
    U: ToString + Default,
{
    // ...
}
```

### Trait Objects

```quanta
// Dynamic dispatch
fn draw_all(shapes: &[&dyn Drawable]) {
    for shape in shapes {
        shape.draw();
    }
}

// Box<dyn Trait>
fn create_shape(kind: &str) -> Box<dyn Drawable> {
    match kind {
        "circle" => Box::new(Circle::new(1.0)),
        "square" => Box::new(Square::new(1.0)),
        _ => panic!("Unknown shape"),
    }
}
```

### Common Traits

| Trait | Purpose |
|-------|---------|
| `Clone` | Create a copy |
| `Copy` | Bitwise copy (implicit) |
| `Debug` | Debug formatting `{:?}` |
| `Display` | User-facing formatting `{}` |
| `Default` | Default value |
| `PartialEq`, `Eq` | Equality comparison |
| `PartialOrd`, `Ord` | Ordering comparison |
| `Hash` | Hashing for HashMap |
| `Iterator` | Iteration |
| `From`, `Into` | Type conversion |
| `Drop` | Cleanup on destruction |

---

## Generics

### Type Parameters

```quanta
// Generic struct
struct Pair<T, U> {
    first: T,
    second: U,
}

// Generic enum
enum Either<L, R> {
    Left(L),
    Right(R),
}

// Generic impl
impl<T, U> Pair<T, U> {
    fn new(first: T, second: U) -> Self {
        Pair { first, second }
    }
}

// Specialized impl
impl<T: Display> Pair<T, T> {
    fn print(&self) {
        println!("({}, {})", self.first, self.second);
    }
}
```

### Const Generics

```quanta
// Array with const size parameter
struct Array<T, const N: usize> {
    data: [T; N],
}

impl<T: Default + Copy, const N: usize> Array<T, N> {
    fn new() -> Self {
        Array { data: [T::default(); N] }
    }
}

let arr: Array<i32, 10> = Array::new();
```

### Lifetime Parameters

```quanta
// Lifetime annotation
fn longest<'a>(x: &'a str, y: &'a str) -> &'a str {
    if x.len() > y.len() { x } else { y }
}

// Struct with lifetime
struct Parser<'a> {
    input: &'a str,
    position: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Parser { input, position: 0 }
    }
}

// Multiple lifetimes
fn select<'a, 'b>(x: &'a str, y: &'b str, use_first: bool) -> &'a str
where
    'b: 'a,  // 'b outlives 'a
{
    if use_first { x } else { y }
}
```

---

## Error Handling

### Result Type

```quanta
enum Result<T, E> {
    Ok(T),
    Err(E),
}

fn divide(a: f64, b: f64) -> Result<f64, String> {
    if b == 0.0 {
        Err("Division by zero".to_string())
    } else {
        Ok(a / b)
    }
}
```

### The ? Operator

```quanta
fn read_config() -> Result<Config, Error> {
    let contents = std::io::read_to_string("config.toml")?;
    let config = parse_toml(&contents)?;
    Ok(config)
}
```

### Option Type

```quanta
enum Option<T> {
    Some(T),
    None,
}

fn find_first_even(numbers: &[i32]) -> Option<i32> {
    for &n in numbers {
        if n % 2 == 0 {
            return Some(n);
        }
    }
    None
}
```

### Panic and Unwrap

```quanta
// Panic immediately
panic!("Something went wrong!");

// Unwrap (panic on None/Err)
let value = option.unwrap();
let value = result.unwrap();

// Expect (panic with message)
let value = option.expect("Value should be present");

// Unwrap or default
let value = option.unwrap_or(default);
let value = option.unwrap_or_else(|| compute_default());
```

---

## Modules

### Module Definition

```quanta
// Inline module
mod math {
    pub fn add(a: i32, b: i32) -> i32 {
        a + b
    }
    
    fn internal() { }  // Private
}

// File: src/math.quanta
mod math;

// Directory: src/math/mod.quanta
mod math;
```

### Visibility

```quanta
pub struct Public { }          // Public to all
pub(crate) struct Crate { }    // Public within crate
pub(super) struct Parent { }   // Public to parent module
pub(in path) struct Path { }   // Public to specific path
struct Private { }             // Private (default)
```

### Use Statements

```quanta
// Single import
use std::collections::HashMap;

// Multiple imports
use std::io::{Read, Write, BufReader};

// Glob import
use std::collections::*;

// Alias
use std::collections::HashMap as Map;

// Re-export
pub use internal::PublicType;
```

---

## Macros

### Declarative Macros

```quanta
macro_rules! vec {
    () => { Vec::new() };
    ($($x:expr),+ $(,)?) => {
        {
            let mut temp = Vec::new();
            $(temp.push($x);)+
            temp
        }
    };
}

// Usage
let v = vec![1, 2, 3];
```

### Macro Patterns

| Pattern | Matches |
|---------|---------|
| `$name:expr` | Expression |
| `$name:ty` | Type |
| `$name:ident` | Identifier |
| `$name:path` | Path |
| `$name:stmt` | Statement |
| `$name:pat` | Pattern |
| `$name:block` | Block |
| `$name:item` | Item |
| `$name:tt` | Token tree |
| `$name:lifetime` | Lifetime |

---

## Attributes

### Common Attributes

```quanta
// Derive traits
#[derive(Debug, Clone, PartialEq)]
struct Point { x: i32, y: i32 }

// Conditional compilation
#[cfg(target_os = "linux")]
fn linux_only() { }

#[cfg(test)]
mod tests { }

// Allow/deny lints
#[allow(unused_variables)]
#[deny(unsafe_code)]
#[warn(missing_docs)]

// Documentation
#[doc = "Description"]
#[doc(hidden)]

// Testing
#[test]
fn test_something() { }

#[bench]
fn bench_something(b: &mut Bencher) { }

// Inline hints
#[inline]
#[inline(always)]
#[inline(never)]

// Must use return value
#[must_use]
fn important() -> Result<(), Error> { Ok(()) }
```

---

## Memory Model

### Ownership Rules

1. Each value has exactly one owner
2. When the owner goes out of scope, the value is dropped
3. Values can be moved or borrowed

```quanta
// Move
let s1 = String::from("hello");
let s2 = s1;  // s1 is moved to s2
// s1 is no longer valid

// Clone
let s1 = String::from("hello");
let s2 = s1.clone();  // Deep copy
// Both s1 and s2 are valid

// Borrow
let s1 = String::from("hello");
let len = calculate_length(&s1);  // Borrow s1
// s1 is still valid
```

### Borrowing Rules

1. Any number of immutable references OR one mutable reference
2. References must be valid (no dangling)

```quanta
let mut s = String::from("hello");

// Multiple immutable borrows OK
let r1 = &s;
let r2 = &s;
println!("{} {}", r1, r2);

// Single mutable borrow OK
let r3 = &mut s;
r3.push_str(" world");

// Cannot mix mutable and immutable
// let r4 = &s;      // Error if r3 is still in scope
```

---

## Concurrency

### Threads

```quanta
use std::thread;

// Spawn thread
let handle = thread::spawn(|| {
    println!("Hello from thread!");
});

// Wait for completion
handle.join().unwrap();

// Move data into thread
let data = vec![1, 2, 3];
let handle = thread::spawn(move || {
    println!("{:?}", data);
});
```

### Async/Await

```quanta
async fn fetch_data() -> Result<String, Error> {
    let response = http::get("https://example.com").await?;
    Ok(response.text().await?)
}

async fn process_multiple() {
    // Sequential
    let a = fetch_a().await;
    let b = fetch_b().await;
    
    // Concurrent
    let (a, b) = join!(fetch_a(), fetch_b());
    
    // Select first to complete
    select! {
        result = fetch_a() => handle_a(result),
        result = fetch_b() => handle_b(result),
    }
}
```

### Synchronization

```quanta
use std::sync::{Arc, Mutex, RwLock};

// Mutex for exclusive access
let data = Arc::new(Mutex::new(0));
{
    let mut guard = data.lock().unwrap();
    *guard += 1;
}

// RwLock for read-heavy workloads
let data = Arc::new(RwLock::new(vec![]));
{
    let read = data.read().unwrap();
    println!("{:?}", *read);
}
{
    let mut write = data.write().unwrap();
    write.push(1);
}
```

---

## Appendix: Grammar Summary

```
Program     = Item*
Item        = Function | Struct | Enum | Trait | Impl | Module | Use | Const | Static
Function    = "fn" IDENT GenericParams? "(" Params? ")" ReturnType? Block
Struct      = "struct" IDENT GenericParams? StructBody
Enum        = "enum" IDENT GenericParams? "{" EnumVariants "}"
Trait       = "trait" IDENT GenericParams? "{" TraitItem* "}"
Impl        = "impl" GenericParams? Type "for"? Type "{" ImplItem* "}"

Type        = PathType | TupleType | ArrayType | RefType | FnType | TraitObject
Expr        = Literal | Path | Block | If | Match | Loop | Closure | Call | ...
Stmt        = Let | Expr ";" | Item
Pattern     = Literal | Ident | Tuple | Struct | Enum | Ref | ...
```

This reference covers the core language features. For the complete specification, see the [QuantaLang Specification](specification.md).
