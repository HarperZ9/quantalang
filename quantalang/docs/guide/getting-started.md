# Getting Started with QuantaLang

Welcome to QuantaLang! This guide will help you get up and running with the language quickly.

## Table of Contents

1. [Installation](#installation)
2. [Your First Program](#your-first-program)
3. [Basic Concepts](#basic-concepts)
4. [Working with the Compiler](#working-with-the-compiler)
5. [Package Management](#package-management)
6. [Next Steps](#next-steps)

## Installation

### From Binary Release (Recommended)

Download the latest release for your platform:

```bash
# Linux/macOS
curl -sSL https://quantalang.org/install.sh | sh

# Or using wget
wget -qO- https://quantalang.org/install.sh | sh
```

### From Source

```bash
# Clone the repository
git clone https://github.com/quantalang/quantalang.git
cd quantalang

# Build the compiler
./build.sh release

# Add to PATH
export PATH="$PWD/target/release:$PATH"
```

### Verify Installation

```bash
quanta --version
# QuantaLang 1.0.0
```

## Your First Program

Create a file named `hello.quanta`:

```quanta
fn main() {
    println!("Hello, World!");
}
```

Compile and run:

```bash
quanta run hello.quanta
# Hello, World!
```

Or compile to an executable:

```bash
quanta build hello.quanta -o hello
./hello
# Hello, World!
```

## Basic Concepts

### Variables and Types

QuantaLang uses type inference with optional explicit annotations:

```quanta
// Immutable by default
let x = 42;              // i32 inferred
let name = "Alice";      // String inferred
let pi: f64 = 3.14159;   // Explicit type

// Mutable variables
let mut count = 0;
count += 1;

// Constants (compile-time)
const MAX_SIZE: usize = 1024;
```

### Functions

```quanta
// Basic function
fn greet(name: String) {
    println!("Hello, {}!", name);
}

// Function with return type
fn add(a: i32, b: i32) -> i32 {
    a + b  // No semicolon = implicit return
}

// Generic function
fn first<T>(items: &[T]) -> Option<&T> {
    items.get(0)
}
```

### Control Flow

```quanta
// If expressions
let max = if a > b { a } else { b };

// Pattern matching
match value {
    0 => println!("zero"),
    1..=9 => println!("single digit"),
    n if n < 0 => println!("negative"),
    _ => println!("other"),
}

// Loops
for i in 0..10 {
    println!("{}", i);
}

while condition {
    // ...
}

loop {
    if done { break; }
}
```

### Structs and Enums

```quanta
// Struct definition
struct Point {
    x: f64,
    y: f64,
}

impl Point {
    // Constructor
    fn new(x: f64, y: f64) -> Self {
        Point { x, y }
    }
    
    // Method
    fn distance(&self, other: &Point) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }
}

// Enum with variants
enum Shape {
    Circle { radius: f64 },
    Rectangle { width: f64, height: f64 },
    Triangle { base: f64, height: f64 },
}

impl Shape {
    fn area(&self) -> f64 {
        match self {
            Shape::Circle { radius } => 3.14159 * radius * radius,
            Shape::Rectangle { width, height } => width * height,
            Shape::Triangle { base, height } => 0.5 * base * height,
        }
    }
}
```

### Error Handling

QuantaLang uses `Result` and `Option` for error handling:

```quanta
use std::io::{File, Read};

fn read_file(path: &str) -> Result<String, std::io::Error> {
    let mut file = File::open(path)?;  // ? propagates errors
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    Ok(contents)
}

fn main() {
    match read_file("config.txt") {
        Ok(contents) => println!("{}", contents),
        Err(e) => eprintln!("Error: {}", e),
    }
}
```

### Collections

```quanta
use std::vec::Vec;
use std::hashmap::HashMap;

// Vectors
let mut numbers = vec![1, 2, 3, 4, 5];
numbers.push(6);
let sum: i32 = numbers.iter().sum();

// Hash maps
let mut scores = HashMap::new();
scores.insert("Alice", 100);
scores.insert("Bob", 85);

if let Some(score) = scores.get("Alice") {
    println!("Alice's score: {}", score);
}
```

## Working with the Compiler

### Compilation Modes

```bash
# Debug build (faster compilation, slower runtime)
quanta build main.quanta

# Release build (optimized)
quanta build main.quanta --release

# Run directly
quanta run main.quanta

# Run with arguments
quanta run main.quanta -- arg1 arg2
```

### Compiler Targets

```bash
# Native (default)
quanta build main.quanta

# WebAssembly
quanta build main.quanta --target wasm32

# Specific architecture
quanta build main.quanta --target x86_64-linux
quanta build main.quanta --target aarch64-macos
```

### Useful Commands

```bash
# Format code
quanta fmt src/

# Run linter
quanta lint src/

# Generate documentation
quanta doc src/ -o docs/

# Run tests
quanta test

# Check without building
quanta check main.quanta

# Show dependencies
quanta deps

# Start REPL
quanta repl
```

## Package Management

### Creating a Project

```bash
quanta new my-project
cd my-project
```

This creates:
```
my-project/
├── quanta.toml       # Project configuration
├── src/
│   └── main.quanta   # Main source file
└── tests/
    └── test_main.quanta
```

### Project Configuration (quanta.toml)

```toml
[package]
name = "my-project"
version = "0.1.0"
authors = ["Your Name <you@example.com>"]
edition = "2024"

[dependencies]
serde = "1.0"
tokio = { version = "1.0", features = ["full"] }

[dev-dependencies]
test-utils = "0.5"

[build]
target = "native"
opt-level = 2
```

### Adding Dependencies

```bash
# Add from registry
quanta add serde

# Add with version
quanta add serde@1.0

# Add with features
quanta add tokio --features full

# Remove dependency
quanta remove serde
```

### Building and Running

```bash
# Build project
quanta build

# Run project
quanta run

# Run specific binary
quanta run --bin my-binary

# Run tests
quanta test

# Run benchmarks
quanta bench
```

## Next Steps

Now that you have the basics, explore these topics:

1. **[Language Reference](reference/language.md)** - Complete language specification
2. **[Standard Library](api/std.md)** - Full API documentation
3. **[Tutorials](tutorials/README.md)** - Step-by-step guides
4. **[Best Practices](guide/best-practices.md)** - Idiomatic QuantaLang
5. **[Concurrency](guide/concurrency.md)** - Async/await and threads
6. **[FFI](guide/ffi.md)** - Interoperating with C/C++

### Community Resources

- **Documentation**: https://docs.quantalang.org
- **Forum**: https://forum.quantalang.org
- **Discord**: https://discord.gg/quantalang
- **GitHub**: https://github.com/quantalang/quantalang

### Getting Help

```bash
# Built-in help
quanta help
quanta help <command>

# Search documentation
quanta doc --search "HashMap"
```

Welcome to the QuantaLang community! 🎉
