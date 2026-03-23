# QuantaLang

**Algebraic effects as a first-class feature. Better error handling than Rust. Better concurrency than Go. Mathematical purity with practical power.**

## Why QuantaLang?

Every programming language handles side effects differently — and most do it badly.

- **Rust** makes you wrap everything in `Result<T, E>` and chain `?` operators. Safe, but verbose.
- **Go** makes you check `if err != nil` after every function call. Simple, but tedious.
- **Haskell** uses monads. Powerful, but impenetrable to most programmers.
- **Java/Python** use exceptions. Easy, but invisible control flow that breaks reasoning.

QuantaLang uses **algebraic effects** — a system that is:
- **As safe as Rust** — effects are tracked in the type system
- **As simple as Go** — just call functions, no ceremony
- **As powerful as Haskell** — full effect composition and abstraction
- **As practical as Python** — you don't need a PhD to use it

## How Effects Work

An **effect** is a declaration of WHAT can happen:
```quanta
effect FileSystem {
    fn read(path: str) -> str,
    fn write(path: str, content: str) -> (),
}
```

A function that uses an effect declares it in its type:
```quanta
fn process_config() ~ FileSystem -> Config {
    let content = perform FileSystem.read("config.toml");
    parse_config(content)
}
```

A **handler** decides HOW the effect executes:
```quanta
handle { process_config() } with {
    FileSystem.read(path) => |resume| {
        let data = actual_read_file(path);
        resume(data)
    },
    FileSystem.write(path, content) => |resume| {
        actual_write_file(path, content);
        resume(())
    },
}
```

## Why This Matters

**Testing**: Swap the real file system handler for a mock — no dependency injection frameworks needed.

**Concurrency**: async/await is just an effect. Swap single-threaded for multi-threaded by changing the handler.

**Error handling**: No `Result`, no `?`, no `try/catch`. Errors are effects. Handle them where you want, not where they occur.

**Purity**: Functions with no effects are provably pure. The type system guarantees it.

## Language Features

- Algebraic effects with handler-based control flow
- Hindley-Milner type inference with effect rows
- Higher-kinded types for generic abstractions
- Const generics for compile-time computation
- Multi-target compilation (C, WASM, native)
- Effect polymorphism and row polymorphism
- Zero-cost abstraction through effect elimination
