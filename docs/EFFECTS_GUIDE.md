# QuantaLang Algebraic Effects Guide

Algebraic effects are QuantaLang's signature feature. Think of them as checked exceptions crossed with dependency injection: a function declares what side effects it performs, and the caller decides how to handle them. This gives you compile-time control over I/O, rendering, logging, and anything else that touches the outside world.

---

## Why Effects Matter for Graphics

In a game engine, your rendering code calls into Vulkan, DirectX, or OpenGL. With effects, you write the rendering logic once and swap the backend at the call site:

- Production: Vulkan handler
- Testing: mock handler that logs draw calls
- Profiling: handler that records timing
- Replay: handler that plays back recorded frames

No interfaces, no virtual dispatch, no runtime overhead. The effect handler is resolved at compile time.

---

## Defining an Effect

An effect declares a set of operations. It does not implement them -- that is the handler's job.

```quanta
effect Greeting {
    fn greet(name: str) -> (),
}
```

This says: "There exists a side effect called `Greeting` with one operation `greet` that takes a string and returns nothing." It is a contract, like a trait but for side effects.

An effect can have multiple operations:

```quanta
effect Render {
    fn draw(description: str) -> (),
    fn clear(r: f64, g: f64, b: f64) -> (),
    fn swap_buffers() -> (),
}
```

---

## Performing an Effect

A function that uses an effect must declare it in its signature with `~`:

```quanta
fn welcome() ~ Greeting {
    perform Greeting.greet("Alice");
}
```

The `~ Greeting` annotation means: "this function performs the Greeting effect." The compiler tracks this -- if you forget the annotation, you get a compile error. If you call a function that performs effects, your function must either handle them or propagate them in its own signature.

```quanta
fn welcome_everyone() ~ Greeting {
    perform Greeting.greet("Alice");
    perform Greeting.greet("Bob");
    perform Greeting.greet("Charlie");
}
```

---

## Handling an Effect

The caller wraps the effectful code in a `handle/with` block and provides implementations for each operation:

```quanta
fn main() {
    handle {
        welcome()
    } with {
        Greeting.greet(name) => {
            println!("Hello, {}!", name)
        },
    }
}
```

When `welcome()` executes `perform Greeting.greet("Alice")`, control transfers to the handler. The handler runs `println!("Hello, Alice!")`, then control returns to the point after the `perform`.

---

## Full Example: Render Effect

Here is the pattern that makes effects powerful for game engines:

```quanta
effect Render {
    fn draw(description: str) -> (),
}

// Pure math -- no effects, no side effects
fn phong_lighting(normal: vec3, light_dir: vec3) -> vec3 {
    let ambient = vec3(0.1, 0.1, 0.1);
    let n = normalize(normal);
    let l = normalize(light_dir);
    let diff = dot(n, l);
    let diffuse = if diff > 0.0 {
        vec3(diff, diff, diff)
    } else {
        vec3(0.0, 0.0, 0.0)
    };
    ambient + diffuse
}

// Scene logic -- performs the Render effect but does not know HOW rendering works
fn render_scene() ~ Render {
    let normal = vec3(0.0, 1.0, 0.0);
    let light_dir = normalize(vec3(1.0, 1.0, 0.5));
    let color = phong_lighting(normal, light_dir);
    println!("Lighting: ({}, {}, {})", color.x, color.y, color.z);

    let model = mat4_translate(vec3(5.0, 1.0, 3.0));
    let world_pos = model * vec4(0.0, 0.0, 0.0, 1.0);

    perform Render.draw("player at (5, 1, 3)")
}
```

### Production Handler (Vulkan)

```quanta
fn main() {
    handle {
        render_scene()
    } with {
        Render.draw(desc) => {
            // In production: submit Vulkan draw commands
            println!("VULKAN DRAW: {}", desc)
        },
    }
}
```

### Test Handler (Mock)

```quanta
fn test_render() {
    handle {
        render_scene()
    } with {
        Render.draw(desc) => {
            // In tests: just log what would be drawn
            println!("MOCK DRAW: {}", desc)
        },
    }
}
```

### Profiling Handler

```quanta
fn profile_render() {
    handle {
        render_scene()
    } with {
        Render.draw(desc) => {
            println!("PROFILE: draw call recorded: {}", desc)
        },
    }
}
```

The `render_scene` function is identical in all three cases. Only the handler changes. This is the power of algebraic effects: the function that performs work does not decide how side effects are executed.

---

## Effect Propagation

Effects propagate through the call stack. If function A calls function B which performs an effect, function A must either handle it or declare it:

```quanta
effect Logger {
    fn log(message: str) -> (),
}

effect Render {
    fn draw(description: str) -> (),
}

// This function performs Logger
fn compute_something() ~ Logger {
    perform Logger.log("Computing...");
}

// This function performs both Logger and Render
fn render_with_logging() ~ Logger, Render {
    perform Logger.log("Starting render");
    perform Render.draw("scene");
    perform Logger.log("Render complete");
}

fn main() {
    handle {
        handle {
            render_with_logging()
        } with {
            Render.draw(desc) => {
                println!("DRAW: {}", desc)
            },
        }
    } with {
        Logger.log(msg) => {
            println!("[LOG] {}", msg)
        },
    }
}
```

Handlers can be nested. The inner handler resolves `Render`, the outer handler resolves `Logger`.

---

## Effects vs. Alternatives

| Approach               | Problem                                      |
|------------------------|----------------------------------------------|
| Global state           | Untraceable, untestable                      |
| Dependency injection   | Boilerplate, runtime overhead                |
| Virtual dispatch       | Vtable indirection, allocation               |
| Monads (Haskell)       | Complex types, hard to compose               |
| **Algebraic effects**  | Declared in signature, handled at call site, zero-cost |

Effects give you:
- **Compile-time tracking:** The type checker knows which effects a function performs.
- **Caller control:** The handler is at the call site, not baked into the callee.
- **Composability:** Multiple effects compose naturally -- just list them with commas.
- **Testability:** Swap a Vulkan handler for a mock handler in one line.

---

## Implementation Details

Under the hood, QuantaLang compiles effects to `setjmp`/`longjmp` on the C backend. When you `perform` an effect:

1. The runtime saves the current continuation (registers + stack pointer) with `setjmp`
2. Control jumps to the nearest matching handler via `longjmp`
3. The handler executes, then resumes the continuation

This is efficient -- no heap allocation, no garbage collection, no virtual dispatch. The overhead is one `setjmp` per `perform`, which is comparable to a function call on modern hardware.

---

## Quick Reference

```quanta
// Define an effect
effect EffectName {
    fn operation(param: Type) -> ReturnType,
}

// Declare that a function performs an effect
fn my_function() ~ EffectName {
    perform EffectName.operation(value);
}

// Multiple effects
fn my_function() ~ Effect1, Effect2 {
    perform Effect1.op1();
    perform Effect2.op2();
}

// Handle an effect
handle {
    my_function()
} with {
    EffectName.operation(param) => {
        // handler body
    },
}
```

---

## Patterns for Game Engines

### Swap rendering backend

```quanta
effect GPU {
    fn submit_draw_call(mesh: str, shader: str) -> (),
}

fn render_frame() ~ GPU {
    perform GPU.submit_draw_call("player_mesh", "pbr_shader");
    perform GPU.submit_draw_call("terrain_mesh", "terrain_shader");
}

// Vulkan backend
handle { render_frame() } with {
    GPU.submit_draw_call(mesh, shader) => {
        println!("vkCmdDraw: {} with {}", mesh, shader)
    },
}
```

### Record and replay

```quanta
effect Input {
    fn get_key(key: str) -> bool,
}

fn game_tick() ~ Input {
    let fire = perform Input.get_key("space");
}

// Live input
handle { game_tick() } with {
    Input.get_key(key) => {
        // poll real keyboard
        println!("Polling key: {}", key)
    },
}

// Replay from recording
handle { game_tick() } with {
    Input.get_key(key) => {
        // return recorded value
        println!("Replaying key: {}", key)
    },
}
```

---

## Next Steps

- `tests/programs/27_effects_showcase.quanta` -- minimal working effect example
- `tests/programs/38_graphics_demo.quanta` -- effects + vector math + rendering
- [SHADER_GUIDE.md](SHADER_GUIDE.md) -- write shaders that compile to CPU and GPU
- [GETTING_STARTED.md](GETTING_STARTED.md) -- full language overview
