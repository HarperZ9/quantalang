# Status: runtime/

Last audited: 2026-03-21

## Working
- None of these modules are called by the compiler's code generation pipeline at runtime. They exist as Rust library code defining data structures and algorithms.

## Partial
- **FFI** (`ffi.rs`, 1038 lines): Defines calling conventions (C, stdcall, fastcall, vectorcall, Win64, SysV64, AAPCS), type layouts, ABI parameter classification, struct layout computation. Comprehensive type mapping between QuantaLang and C. 7 unit tests. The FFI type system is complete as a design document in code, but **no code path in the compiler invokes these definitions** during actual compilation. The C backend handles FFI through direct C interop, not through this module.
- **Garbage Collector** (`gc.rs`, 786 lines): Defines reference counting types (`RcHeader` with strong/weak counts, color for cycle detection), `TypeInfo` metadata, and cycle detection algorithm structure. 4 unit tests. This is a **design for a runtime GC** but no compiled QuantaLang program links against it. The C backend does not emit calls to these GC functions.
- **Async Runtime** (`async_rt.rs`, 1216 lines): Defines a work-stealing scheduler with `TaskState` machine, worker threads, global queue, `Future`/`Task` representations. 6 unit tests. This is a **design for an async runtime** but no compiled QuantaLang program links against it. The language does not currently have `async`/`await` syntax that compiles.

## Aspirational
- All three modules. They contain well-structured Rust code with real data structures and algorithms, but they describe a runtime that does not yet exist as a linkable library for compiled QuantaLang programs.

## Not Started
- Compilation of these modules into a linkable runtime library (.a / .so / .dll).
- Integration with any code generation backend.
- Async/await language syntax support in the parser/type checker.
- FFI `extern` block compilation using these ABI definitions.

## Honest Assessment
Total: 3,090 lines across 4 files, 17 unit tests, zero `todo!()` or `unimplemented!()` calls. All three modules contain real, well-designed Rust code -- not scaffolding. But they are **architectural designs expressed as code**, not functioning runtime components. No compiled QuantaLang program references or links against any of this code. The C backend's runtime support comes from `codegen/runtime.rs` (an embedded C header), not from this module.
