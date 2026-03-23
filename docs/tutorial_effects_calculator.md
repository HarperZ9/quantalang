# Tutorial: Build a Calculator with Algebraic Effects

Learn QuantaLang's killer feature by building a calculator that handles errors,
logging, and state through algebraic effects.

## What You'll Learn
- How to define effects
- How to perform effect operations
- How to handle effects
- How effects compose naturally
- Why this is better than exceptions, Result types, or error codes

## Step 1: A Simple Calculator (No Effects)

Let's start with basic arithmetic:

```quanta
fn add(a: f64, b: f64) -> f64 { a + b }
fn sub(a: f64, b: f64) -> f64 { a - b }
fn mul(a: f64, b: f64) -> f64 { a * b }

fn main() {
    println!("2 + 3 = {}", add(2.0, 3.0));
    println!("10 - 4 = {}", sub(10.0, 4.0));
    println!("6 * 7 = {}", mul(6.0, 7.0));
}
```

This works, but what about division? Division by zero is an error.

## Step 2: Division with the Fail Effect

In most languages, you'd return `Result<f64, String>` or throw an exception.
In QuantaLang, we use an effect:

```quanta
effect Fail<E> {
    fn fail(error: E) -> !,
}

fn div(a: f64, b: f64) ~ Fail<str> -> f64 {
    if b == 0.0 {
        perform Fail.fail("division by zero")
    }
    a / b
}
```

Let's break this down:

- `effect Fail<E>` declares a new effect with a type parameter `E`.
- `fn fail(error: E) -> !` is an effect operation. The `!` return type means it
  never returns normally -- control transfers to the handler.
- `~ Fail<str>` in the function signature declares that `div` may perform the
  `Fail` effect with a `str` error type.
- `perform Fail.fail(...)` triggers the effect. Execution jumps to the nearest
  handler.

Notice: `div` returns `f64`, not `Result<f64, String>`. The `~ Fail<str>` annotation
tells the type system this function might fail, but the return type stays clean.

## Step 3: Handling the Effect

An effect must be handled before it reaches `main`. The `handle...with` block
catches effect operations and decides what to do:

```quanta
fn main() {
    handle {
        let result = div(10.0, 3.0);
        println!("10 / 3 = {}", result);

        let bad = div(5.0, 0.0);
        println!("This line never runs: {}", bad);
    } with {
        Fail.fail(msg) => |_resume| {
            println!("Error: {}", msg);
        },
    }
}
```

Output:
```
10 / 3 = 3.333333
Error: division by zero
```

The handler clause `Fail.fail(msg) => |_resume| { ... }` says: when a `Fail.fail`
is performed, bind the error to `msg` and run this block. The `_resume` parameter
is a continuation -- calling it would resume execution after the `perform`. We
prefix it with `_` because we don't resume on failure; we just print and stop.

## Step 4: Composing Effectful Functions

Here's where effects shine. Functions that perform effects compose naturally --
no `?` operator, no `Ok()` wrapping, no `if err != nil`:

```quanta
fn calculate(x: f64, y: f64, z: f64) ~ Fail<str> -> f64 {
    let sum = add(x, y);
    let product = mul(sum, z);
    let result = div(product, sub(x, y));
    result
}

fn main() {
    handle {
        let answer = calculate(10.0, 5.0, 3.0);
        println!("Answer: {}", answer);
    } with {
        Fail.fail(msg) => |_resume| {
            println!("Error: {}", msg);
        },
    }
}
```

`calculate` calls `div`, which might fail. But `calculate` doesn't need to check
for errors -- it just declares `~ Fail<str>` and lets the effect propagate. The
handler at the call site decides what to do.

Compare this to the same logic in Rust:
```rust
fn calculate(x: f64, y: f64, z: f64) -> Result<f64, String> {
    let sum = add(x, y);
    let product = mul(sum, z);
    let result = div(product, sub(x, y))?;  // ? required on every fallible call
    Ok(result)                                // must wrap return in Ok()
}
```

And in Go:
```go
func calculate(x, y, z float64) (float64, error) {
    sum := add(x, y)
    product := mul(sum, z)
    result, err := div(product, sub(x, y))  // must capture err
    if err != nil { return 0, err }          // must check err
    return result, nil                       // must return nil error
}
```

QuantaLang's version reads like the error handling isn't there -- because it
isn't in the business logic. It's in the handler.

## Step 5: Adding a Logging Effect

What if we want to log every operation? Define another effect:

```quanta
effect Log {
    fn log(message: str) -> (),
}
```

Now update `div` to use both effects:

```quanta
fn div_logged(a: f64, b: f64) ~ Fail<str>, Log -> f64 {
    perform Log.log("dividing");
    if b == 0.0 {
        perform Fail.fail("division by zero")
    }
    a / b
}
```

The `~ Fail<str>, Log` annotation says this function performs two effects.
Effects compose naturally -- no wrapping, no monad transformers, no dependency
injection framework.

## Step 6: Handling Multiple Effects

Handle both effects at the call site:

```quanta
fn main() {
    handle {
        let r = div_logged(10.0, 3.0);
        println!("Result: {}", r);

        let bad = div_logged(5.0, 0.0);
        println!("Never reached: {}", bad);
    } with {
        Fail.fail(msg) => |_resume| {
            println!("[ERROR] {}", msg);
        },
        Log.log(msg) => |resume| {
            println!("[LOG] {}", msg);
            resume(())
        },
    }
}
```

Output:
```
[LOG] dividing
Result: 3.333333
[LOG] dividing
[ERROR] division by zero
```

Notice the difference between the two handlers:

- `Fail.fail` uses `|_resume|` -- it does not call `resume`, so execution stops.
  The error is terminal.
- `Log.log` uses `|resume|` and calls `resume(())` -- execution continues after
  the `perform`. Logging is just an observation, not a disruption.

This is the key insight: **the handler decides whether to resume**. The function
that performs the effect doesn't know or care.

## Step 7: Different Handlers, Same Code

The same calculator code can behave differently depending on which handler wraps it.
This is the real power of algebraic effects.

```quanta
// Production: print errors, log to console
fn run_production() {
    handle {
        let r = div_logged(10.0, 0.0);
        println!("Result: {}", r);
    } with {
        Fail.fail(msg) => |_resume| {
            println!("[ERROR] {}", msg);
        },
        Log.log(msg) => |resume| {
            println!("[LOG] {}", msg);
            resume(())
        },
    }
}

// Silent: swallow errors, ignore logs
fn run_silent() {
    handle {
        let r = div_logged(10.0, 0.0);
        println!("Result: {}", r);
    } with {
        Fail.fail(msg) => |_resume| {
            // swallow the error silently
        },
        Log.log(msg) => |resume| {
            // skip logging
            resume(())
        },
    }
}
```

The calculator code (`div_logged`) is identical in both cases. Only the handlers
change. Zero modifications to business logic.

## Step 8: Resumption with a Value

A handler can resume with a different value, changing the result of the `perform`
expression. This lets you implement fallback behavior:

```quanta
fn div_with_fallback(a: f64, b: f64) ~ Fail<str> -> f64 {
    if b == 0.0 {
        perform Fail.fail("division by zero")
    }
    a / b
}

fn main() {
    handle {
        let r = div_with_fallback(10.0, 0.0);
        println!("Result: {}", r);
    } with {
        Fail.fail(msg) => |resume| {
            println!("[WARN] {}, returning 0.0", msg);
            resume(0.0)
        },
    }
}
```

Output:
```
[WARN] division by zero, returning 0.0
Result: 0.0
```

Instead of aborting, the handler resumes execution with `0.0` as the result of
the `perform` expression. The function continues as if `perform Fail.fail(...)`
returned `0.0`. This is something exceptions and Result types cannot do.

## Why This Matters

**1. Separation of concerns.** The calculator doesn't know HOW errors are
reported or HOW logs are written. It just performs the operations. Policy
lives in the handler, not the function.

**2. Testability.** Swap the console handler for a test handler that collects
logs into a list. No mocking frameworks, no dependency injection containers.

**3. Composability.** Add a new effect (caching, tracing, metrics) without
changing existing code. Declare it in the signature, perform it in the body,
handle it at the boundary.

**4. Type safety.** If you forget to handle an effect, the compiler tells you.
Unlike exceptions, effects cannot escape unhandled. Unlike error codes, they
cannot be silently ignored.

**5. Resumption.** Handlers can resume, retry, or substitute values -- something
no other mainstream error handling mechanism supports.

## Full Program

Here is the complete calculator with both effects, ready to compile:

```quanta
effect Fail<E> {
    fn fail(error: E) -> !,
}

effect Log {
    fn log(message: str) -> (),
}

fn add(a: f64, b: f64) -> f64 { a + b }
fn sub(a: f64, b: f64) -> f64 { a - b }
fn mul(a: f64, b: f64) -> f64 { a * b }

fn div(a: f64, b: f64) ~ Fail<str>, Log -> f64 {
    perform Log.log("dividing");
    if b == 0.0 {
        perform Fail.fail("division by zero")
    }
    a / b
}

fn calculate(x: f64, y: f64, z: f64) ~ Fail<str>, Log -> f64 {
    perform Log.log("starting calculation");
    let sum = add(x, y);
    let product = mul(sum, z);
    div(product, sub(x, y))
}

fn main() {
    handle {
        let answer = calculate(10.0, 5.0, 3.0);
        println!("Answer: {}", answer);

        let bad = calculate(5.0, 5.0, 2.0);
        println!("This won't print: {}", bad);
    } with {
        Fail.fail(msg) => |_resume| {
            println!("[ERROR] {}", msg);
        },
        Log.log(msg) => |resume| {
            println!("[LOG] {}", msg);
            resume(())
        },
    }
}
```

Expected output:
```
[LOG] starting calculation
[LOG] dividing
Answer: 9.0
[LOG] starting calculation
[LOG] dividing
[ERROR] division by zero
```

## Next Steps

- Read the effect examples in `quantalang/quantalang/examples/` for more patterns
  (async, state, comparison with Rust/Go)
- Look at the test programs in `tests/programs/` for working code you can compile today
- Check `STATUS.md` for the current state of the compiler
