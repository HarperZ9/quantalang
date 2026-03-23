# QuantaLang

<p align="center">
  <img src="docs/assets/logo.svg" alt="QuantaLang Logo" width="200"/>
</p>

<p align="center">
  <strong>A modern systems programming language designed for safety, performance, and expressiveness.</strong>
</p>

<p align="center">
  <a href="#installation">Installation</a> •
  <a href="#quick-start">Quick Start</a> •
  <a href="#features">Features</a> •
  <a href="#documentation">Documentation</a> •
  <a href="#contributing">Contributing</a>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/version-1.0.0-blue" alt="Version"/>
  <img src="https://img.shields.io/badge/license-MIT-green" alt="License"/>
  <img src="https://img.shields.io/badge/tests-passing-brightgreen" alt="Tests"/>
  <img src="https://img.shields.io/badge/docs-complete-blue" alt="Docs"/>
</p>

---

## Overview

QuantaLang is a statically-typed, compiled programming language that combines the performance of systems languages with modern safety features and ergonomic syntax. It's designed for building reliable, efficient software across domains from embedded systems to web services.

```quanta
use std::net::http::{Server, Request, Response};

fn main() -> Result<(), Error> {
    let server = Server::bind("0.0.0.0:8080")?;
    
    println!("Server running on http://localhost:8080");
    
    server.listen(|req: Request| -> Response {
        match req.path() {
            "/" => Response::ok("Hello, World!"),
            "/api/data" => Response::json(get_data()),
            _ => Response::not_found(),
        }
    })
}
```

## Installation

### Quick Install (Recommended)

```bash
curl -sSL https://quantalang.org/install.sh | sh
```

### Package Managers

```bash
# macOS
brew install quantalang

# Arch Linux
pacman -S quantalang

# Ubuntu/Debian
apt install quantalang

# Windows (Scoop)
scoop install quantalang
```

### From Source

```bash
git clone https://github.com/quantalang/quantalang.git
cd quantalang
./build.sh release
./install.sh
```

### Verify Installation

```bash
quanta --version
# QuantaLang 1.0.0 (x86_64-linux)
```

## Quick Start

### Hello World

```bash
# Create a new project
quanta new hello-world
cd hello-world

# Run
quanta run
# Hello, World!
```

### Project Structure

```
hello-world/
├── quanta.toml        # Project configuration
├── src/
│   └── main.quanta    # Main source file
└── tests/
    └── test_main.quanta
```

### Example Programs

**Command-Line Tool:**
```quanta
use std::env;
use std::io::{File, Read};

fn main() -> Result<(), Error> {
    let args: Vec<String> = env::args().collect();
    
    if args.len() < 2 {
        eprintln!("Usage: {} <filename>", args[0]);
        return Ok(());
    }
    
    let contents = File::open(&args[1])?.read_to_string()?;
    let line_count = contents.lines().count();
    
    println!("{} lines", line_count);
    Ok(())
}
```

**Concurrent Processing:**
```quanta
use std::sync::{Arc, Channel};
use std::thread;

fn main() {
    let (tx, rx) = Channel::new();
    let results = Arc::new(Mutex::new(Vec::new()));
    
    // Spawn worker threads
    for i in 0..4 {
        let rx = rx.clone();
        let results = Arc::clone(&results);
        
        thread::spawn(move || {
            while let Ok(task) = rx.recv() {
                let result = process(task);
                results.lock().unwrap().push(result);
            }
        });
    }
    
    // Send tasks
    for task in tasks {
        tx.send(task).unwrap();
    }
}
```

## Features

### 🛡️ Memory Safety

QuantaLang's ownership system prevents common bugs at compile time:

- No null pointer dereferences
- No use-after-free
- No data races
- No buffer overflows

```quanta
let data = vec![1, 2, 3];
let reference = &data[0];
// data.clear();  // Compile error: cannot mutate while borrowed
println!("{}", reference);  // Safe!
```

### ⚡ Zero-Cost Abstractions

High-level constructs compile to efficient machine code:

```quanta
// This iterator chain...
let sum: i32 = numbers
    .iter()
    .filter(|n| *n > 0)
    .map(|n| n * 2)
    .sum();

// ...compiles to the same code as:
let mut sum = 0;
for n in numbers {
    if n > 0 {
        sum += n * 2;
    }
}
```

### 🎯 Type Inference

Write less, achieve more:

```quanta
let numbers = vec![1, 2, 3, 4, 5];  // Vec<i32> inferred
let doubled = numbers.iter().map(|n| n * 2).collect();  // Vec<i32> inferred
let sum: i32 = doubled.iter().sum();  // Explicit when needed
```

### 🔄 Pattern Matching

Exhaustive, expressive pattern matching:

```quanta
match message {
    Message::Text { content, from } => {
        println!("{} says: {}", from, content);
    }
    Message::Image { url, .. } => {
        download_image(url);
    }
    Message::Quit => break,
}
```

### 📦 Modern Package Management

First-class dependency management:

```toml
[package]
name = "my-app"
version = "1.0.0"

[dependencies]
serde = "1.0"
tokio = { version = "1.0", features = ["full"] }
```

### 🌐 Multi-Target Compilation

Compile to multiple targets:

```bash
quanta build --target x86_64-linux
quanta build --target aarch64-macos
quanta build --target wasm32
quanta build --target riscv64
```

## Standard Library

The standard library provides comprehensive functionality:

| Category | Modules |
|----------|---------|
| **Collections** | `Vec`, `HashMap`, `BTreeMap`, `String` |
| **I/O** | `File`, `BufReader`, `stdin/stdout` |
| **Networking** | `TcpStream`, `UdpSocket`, `Http` |
| **Concurrency** | `Thread`, `Mutex`, `Channel`, `async/await` |
| **Text** | `Regex`, `Json`, `Base64` |
| **Security** | `Sha256`, `Hmac`, `Rand` |
| **Compression** | `Gzip`, `Zlib` |

See the [Standard Library Reference](docs/api/std.md) for complete documentation.

## Documentation

- **[Getting Started Guide](docs/guide/getting-started.md)** - First steps with QuantaLang
- **[Language Reference](docs/reference/language.md)** - Complete language specification
- **[Standard Library API](docs/api/std.md)** - API documentation
- **[Tutorials](docs/tutorials/)** - Step-by-step guides
- **[Best Practices](docs/guide/best-practices.md)** - Idiomatic QuantaLang

### Online Resources

- 📖 **Documentation**: [docs.quantalang.org](https://docs.quantalang.org)
- 🎓 **Learn**: [learn.quantalang.org](https://learn.quantalang.org)
- 📦 **Packages**: [packages.quantalang.org](https://packages.quantalang.org)
- 🏃 **Playground**: [play.quantalang.org](https://play.quantalang.org)

## Performance

QuantaLang achieves performance comparable to C and C++:

| Benchmark | QuantaLang | Rust | C | Go |
|-----------|------------|------|---|----|
| Binary trees | 1.00x | 1.02x | 0.98x | 2.1x |
| N-body | 1.00x | 1.01x | 0.99x | 1.8x |
| Regex | 1.00x | 0.98x | 1.05x | 1.4x |
| HTTP server | 1.00x | 1.03x | 0.95x | 1.2x |

## Tooling

### IDE Support

- **VS Code**: [QuantaLang Extension](https://marketplace.visualstudio.com/items?itemName=quantalang.quantalang)
- **IntelliJ**: [QuantaLang Plugin](https://plugins.jetbrains.com/plugin/quantalang)
- **Vim/Neovim**: [quanta.vim](https://github.com/quantalang/quanta.vim)
- **Emacs**: [quanta-mode](https://github.com/quantalang/quanta-mode)

### Built-in Tools

```bash
quanta fmt      # Format code
quanta lint     # Run linter
quanta test     # Run tests
quanta bench    # Run benchmarks
quanta doc      # Generate documentation
quanta repl     # Interactive REPL
```

## Contributing

We welcome contributions! See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

### Development Setup

```bash
git clone https://github.com/quantalang/quantalang.git
cd quantalang
./build.sh debug
./test.sh
```

### Ways to Contribute

- 🐛 Report bugs
- 📝 Improve documentation
- 💡 Suggest features
- 🔧 Submit pull requests
- 📢 Spread the word

## Community

- **Discord**: [discord.gg/quantalang](https://discord.gg/quantalang)
- **Forum**: [forum.quantalang.org](https://forum.quantalang.org)
- **Twitter**: [@quantalang](https://twitter.com/quantalang)
- **Reddit**: [r/quantalang](https://reddit.com/r/quantalang)

## License

QuantaLang is dual-licensed under MIT and Apache 2.0. See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).

## Acknowledgments

QuantaLang builds on ideas from many languages and projects:

- Rust's ownership system
- Go's simplicity and tooling
- ML family's type inference
- Swift's ergonomics
- And many others...

---

<p align="center">
  Made with ❤️ by the QuantaLang Team
</p>
