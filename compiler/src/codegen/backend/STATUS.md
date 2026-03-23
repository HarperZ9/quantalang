# Status: codegen/backend/

Last audited: 2026-03-21

## Hello-World Viability Assessment

Each backend was evaluated against a simple hello-world-equivalent program:
a function that takes two i32 values, adds them, and returns the result.
This exercises: function definition, parameter handling, binary operations,
return values, and basic type mapping.

---

## 1. C Backend (`c.rs`) -- WORKING

- **Implements Backend trait:** Yes (line 820)
- **Output format:** C99 source text (OutputFormat::CSource)
- **MIR operations handled:**
  - Use, BinaryOp (all variants), UnaryOp (Neg, Not): fully handled
  - Ref, AddressOf, Cast, Aggregate, Repeat, Discriminant, Len, NullaryOp: fully handled
  - FieldAccess, VariantField, IndexAccess: fully handled
  - All terminators (Goto, If, Switch, Call, Return, Unreachable, Abort, Drop, Assert): fully handled
- **Could produce a working hello-world:** Yes, verified. This is the only backend wired into `quantac build`.
- **Work needed:** None for basic programs. Already end-to-end functional.
- **Tests:** 11 unit tests.

## 2. x86-64 Backend (`x86_64.rs`) -- LIKELY WORKING (assembly mode)

- **Implements Backend trait:** Yes (line 578)
- **Output format:** GNU-syntax x86-64 assembly (OutputFormat::Assembly) or raw machine code (OutputFormat::Object)
- **MIR operations handled:**
  - Use, BinaryOp (Add, Sub, Mul, Div, Rem, BitAnd, BitOr, BitXor, Shl, Shr, comparisons): handled
  - UnaryOp (Neg, Not): handled
  - Other rvalues (Ref, Aggregate, FieldAccess, etc.): fallthrough to `# TODO` comment in output
  - Terminators: Goto, If, Return, Call (System V ABI with 6 register args), Unreachable, Abort: handled
  - Switch, Drop, Assert: fallthrough to `# TODO` comment
- **Could produce a working hello-world:** Yes, for assembly mode. The generated assembly for an add function would be correct: load args, `add rax, rcx`, store result, `ret` with proper prologue/epilogue. Would need `as` (GNU assembler) and `ld` (linker) to produce an executable. Machine code mode also works but produces raw bytes without ELF headers.
- **Work needed to get it working:**
  1. Wire into CLI with `--target x86-64` flag
  2. Invoke system assembler (as/nasm) on generated .s file
  3. Invoke linker (ld/gcc) to produce executable
  4. No proper register allocator (uses rax as single accumulator with push/pop spilling) -- correct but slow
  5. Struct/enum field access and array indexing not implemented (emits TODO comments)
- **Tests:** 22 unit tests.

## 3. ARM64 Backend (`arm64.rs`) -- LIKELY WORKING (assembly mode)

- **Implements Backend trait:** Yes (line 619)
- **Output format:** ARM64 assembly (OutputFormat::Assembly) or raw machine code (OutputFormat::Object)
- **MIR operations handled:**
  - Use, BinaryOp (Add, Sub, Mul, Div, Rem, BitAnd, BitOr, BitXor, Shl, Shr, comparisons): handled
  - UnaryOp (Neg via `neg`, Not via `mvn`): handled
  - Other rvalues: fallthrough to `// TODO` comment
  - Terminators: Goto (`b`), If (`cmp`/`b.ne`/`b`), Return (with stack restore + `ldp`/`ret`), Call (AAPCS64 with x0-x7), Unreachable (`brk #0`), Abort (`bl abort`): handled
  - Switch, Drop, Assert: fallthrough to `// TODO` comment
- **Could produce a working hello-world:** Yes, for assembly mode on ARM64 hardware. The generated assembly uses correct AAPCS64 calling convention: `stp x29, x30, [sp, #-16]!` / `mov x29, sp` prologue, proper argument register mapping, correct epilogue. Would need cross-assembler and linker on ARM64 Linux.
- **Work needed to get it working:**
  1. Wire into CLI with `--target arm64` flag
  2. Invoke assembler and linker (or cross-compiler toolchain)
  3. Callee-saved register save/restore is stubbed (`// TODO: Save X19-X28`)
  4. No register allocator (uses x0 as accumulator with x9 temp)
  5. Struct/enum field access and array indexing not implemented
  6. Only testable on ARM64 hardware or via QEMU emulation
- **Tests:** 21 unit tests plus machine code tests.

## 4. WASM Backend (`wasm.rs`) -- LIKELY WORKING (WAT text mode)

- **Implements Backend trait:** Yes (line 1630)
- **Output format:** WAT text format (OutputFormat::Wat), not binary .wasm
- **MIR operations handled:**
  - Use, BinaryOp (all arithmetic, comparison, bitwise -- proper i32/i64/f32/f64 selection): handled
  - UnaryOp (Neg: float `neg` / int `0 - x`, Not: bool `xor 1` / bitwise `xor -1`): handled
  - Ref/AddressOf, Cast, Aggregate, Repeat, Discriminant, Len, NullaryOp: handled (some simplified)
  - FieldAccess, VariantField: emits `i32.const 0` with TODO comment
  - IndexAccess: partial (assumes 4-byte elements)
  - Terminators: Goto (comment only -- WASM structured control flow), If (generates `if/then/else`), Call (proper `call $name`), Return, Unreachable, Drop, Assert: handled
  - Switch: emits comments only (no `br_table` generation)
- **Could produce a working hello-world:** Partially. The WAT output for an add function would be structurally correct (`local.get`, `i32.add`, `return`). However:
  - Output is WAT text, not binary .wasm. Would need `wat2wasm` (from WABT toolkit) to convert.
  - WASI mode generates full module structure with memory, imports, `_start` export.
  - Goto terminator only emits a comment -- breaks any multi-block control flow. Single-block functions would work.
- **Work needed to get it working:**
  1. Wire into CLI with `--target wasm` flag
  2. Either emit binary .wasm directly or invoke `wat2wasm`
  3. Fix Goto terminator to use WASM structured control flow (`block`/`loop`/`br`)
  4. Fix Switch terminator to use `br_table`
  5. Implement FieldAccess for struct member access
  6. Test with `wasmtime` or browser runtime
- **Tests:** 11 unit tests.

## 5. LLVM Backend (`llvm.rs`) -- LIKELY WORKING

- **Implements Backend trait:** Yes (line 1705)
- **Output format:** LLVM IR text (OutputFormat::LlvmIr)
- **MIR operations handled:**
  - Use, BinaryOp (full mapping to LLVM `add`/`sub`/`mul`/`sdiv`/`fadd`/etc.): handled
  - UnaryOp (Neg: `fneg` for float, `sub 0` for int; Not: `xor true`/`xor -1`): handled
  - Ref, AddressOf: generates `store ptr` correctly
  - Cast: full mapping to LLVM cast instructions (sext, zext, trunc, fpext, fptrunc, etc.)
  - Aggregate: generates GEP + store for each element
  - Repeat: generates loop of GEP + store
  - Discriminant, Len, NullaryOp: handled with proper GEP
  - FieldAccess, VariantField: emits `; TODO` comment (not implemented)
  - IndexAccess: emits `; TODO` comment (not implemented)
  - Terminators: Goto (`br label`), If (`br i1`), Switch (`switch`), Call (with proper signatures), Return (`ret`), Unreachable, Drop, Assert: all handled
- **Could produce a working hello-world:** Yes. The LLVM IR for an add function would be fully correct and valid. Module header with target triple and data layout, proper function definition with `define` keyword, SSA-form `alloca`/`store`/`load`/`add`/`ret`. Can be compiled via `clang output.ll -o program` or `llc output.ll && gcc output.s`.
- **This is the closest to working after the C backend.**
- **Work needed to get it working:**
  1. Wire into CLI with `--target llvm` flag
  2. Invoke `clang` or `llc` on generated .ll file to produce executable
  3. Implement FieldAccess and IndexAccess (currently emit TODO comments)
  4. The `target()` method returns `Target::X86_64` instead of `Target::LlvmIr` (line 1735) -- should be fixed
- **Tests:** 11 unit tests.

## 6. SPIR-V Backend (`spirv.rs`) -- SPECIALIZED (GPU compute only)

- **Implements Backend trait:** Yes (line 1749)
- **Output format:** SPIR-V binary (OutputFormat::SpirV) -- raw u32 words converted to little-endian bytes
- **MIR operations handled:**
  - Use: loads via OpLoad, constants via OpConstant/OpConstantTrue/etc.
  - BinaryOp: maps to SPIR-V opcodes (OpIAdd, OpFAdd, OpISub, OpIMul, OpSDiv, OpFDiv, etc.)
  - UnaryOp (Neg: OpSNegate/OpFNegate, Not: OpNot): handled
  - Cast: maps to OpConvertFToS/OpConvertSToF/OpUConvert/OpSConvert/etc.
  - Aggregate, Repeat, Ref, AddressOf, FieldAccess, IndexAccess, etc.: returns zero constant (default fallback)
  - Terminators: Goto (OpBranch), If (OpSelectionMerge + OpBranchConditional), Switch (OpSwitch), Return (OpReturn/OpReturnValue), Call (OpFunctionCall), Unreachable (OpUnreachable): handled
- **Could produce a working hello-world:** No, not in the traditional sense. SPIR-V is a GPU shader/compute format, not a CPU program format. There is no concept of `printf` or stdout. An add function would compile to a valid SPIR-V compute shader entry point, but "running" it requires a Vulkan or OpenCL host program to dispatch it. The binary header (magic number, version, generator ID) is correct, and the module structure follows the SPIR-V spec.
- **Work needed to get it working (as a compute shader):**
  1. Wire into CLI with `--target spirv` flag
  2. Validate output with `spirv-val` from the Vulkan SDK
  3. Global variables emit zero (not wired to storage buffers)
  4. No descriptor set/binding decorations for buffer I/O
  5. Need a host-side Vulkan program to dispatch the shader and read results
  6. Implement buffer bindings so the shader can read input / write output
- **Tests:** 7 unit tests.

---

## Summary Table

| Backend | Backend Trait | Output Format | Hello-World Viable | Biggest Blocker |
|---------|-------------|---------------|-------------------|-----------------|
| C | Yes | C99 source | **Yes (working)** | None |
| x86-64 | Yes | Assembly / machine code | Likely (asm) | No assembler/linker integration |
| ARM64 | Yes | Assembly / machine code | Likely (asm) | No assembler/linker integration, needs ARM hardware |
| WASM | Yes | WAT text | Partial | Goto terminator is comment-only, no binary output |
| LLVM | Yes | LLVM IR text | **Likely (nearest to working)** | No clang/llc invocation, wrong target() return |
| SPIR-V | Yes | SPIR-V binary | No (GPU-only format) | Needs Vulkan host, no buffer I/O |

## Priority Order for Getting Backends Working

1. **LLVM** -- Lowest effort. Output is already valid LLVM IR. Wire into CLI + invoke clang.
2. **x86-64** -- Medium effort. Assembly output is reasonable. Need assembler + linker.
3. **WASM** -- Medium effort. Fix structured control flow, add wat2wasm or binary emission.
4. **ARM64** -- Medium effort but hardware-dependent. Same as x86-64 but for ARM.
5. **SPIR-V** -- High effort. Fundamentally different target (GPU). Needs buffer I/O, Vulkan host.

## What All Non-C Backends Share

- All implement `Backend` trait with `generate()` returning `CodegenResult<GeneratedCode>`
- All handle the core MIR operation set (Use, BinaryOp, UnaryOp, basic terminators)
- None handle FieldAccess or VariantField (struct/enum member access)
- None are wired into the CLI (`quantac build` only uses CBackend)
- None have end-to-end tests that produce and run an executable/module
