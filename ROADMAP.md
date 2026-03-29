# QuantaLang Development Roadmap

Updated March 2026. Execution order, not wishlist.

## Phase 1: Interprocedural Lifetime Analysis (weeks 1-6)

The borrow checker tracks borrows within a single function. It does not
propagate lifetime information across function boundaries.

### 1.1 Lifetime Parameters in Function Signatures
```
fn first<'a>(x: &'a i32, y: &'a i32) -> &'a i32 { x }
```
- Parse lifetime parameters in generic position (parser already handles `'a`)
- Store lifetime params in FnSig alongside type params
- Instantiate lifetime variables at each call site
- Wire into the type checker's collection pass

### 1.2 Lifetime Elision Rules
When lifetime parameters are omitted, apply standard elision:
- Single input reference → output gets same lifetime
- `&self` → output gets self's lifetime
- Multiple inputs with no `&self` → must be explicit

### 1.3 Return Lifetime Validation
```
fn bad<'a, 'b>(x: &'a i32, y: &'b i32) -> &'a i32 {
    y  // ERROR: returning 'b where 'a is expected
}
```
- Unify return expression's lifetime with declared return lifetime
- Error when lifetimes conflict

### 1.4 Caller-Side Lifetime Checking
```
let r;
{
    let x = 42;
    r = first(&x, &x);  // ERROR: x doesn't live long enough
}
```
- At call sites, verify argument lifetimes satisfy parameter constraints
- Returned reference cannot outlive the shortest argument lifetime

### Success criterion
A program with lifetime parameters compiles correctly AND a program with
lifetime violations is rejected with a clear error message.

## Phase 2: Standard Library from Real Programs (weeks 1-8, parallel)

Build the stdlib by writing real programs and extracting common patterns.
Not by designing abstractions in advance.

### 2.1 String Builder
```
fn sb_new() -> StringBuilder
fn sb_push(sb: &mut StringBuilder, s: str)
fn sb_build(sb: &StringBuilder) -> str
```

### 2.2 Error Handling
```
enum Error { NotFound(str), ParseError(str), IoError(str) }
fn try_parse(input: str) -> Result<i32, Error>
```

### 2.3 Iterator Combinators (on Vec<i32>)
```
fn map(data: Vec<i32>, f: fn(i32) -> i32) -> Vec<i32>
fn filter(data: Vec<i32>, pred: fn(i32) -> bool) -> Vec<i32>
fn fold(data: Vec<i32>, init: i32, f: fn(i32, i32) -> i32) -> i32
fn any(data: Vec<i32>, pred: fn(i32) -> bool) -> bool
fn all(data: Vec<i32>, pred: fn(i32) -> bool) -> bool
```

### 2.4 Collections
```
fn vec_reverse(data: Vec<i32>) -> Vec<i32>
fn vec_sort(data: Vec<i32>) -> Vec<i32>
fn vec_dedup(data: Vec<i32>) -> Vec<i32>
```

### Success criterion
A non-trivial program uses 3+ stdlib modules together and produces
correct output verified against expected results.

## Phase 3: LLVM Executable Pipeline (weeks 7-12)

### 3.1 Install clang
Add clang/LLVM to the build environment.

### 3.2 End-to-End LLVM Pipeline
```
quantac build --target llvm program.quanta
# .quanta → .ll → clang → native executable
```
- Wire quantac to invoke clang on .ll output
- Link with C runtime (printf, malloc, etc.)
- Verify output matches C backend for all 108 runtime-tested programs

### 3.3 Optimization Verification
- `clang -O2` on generated IR — verify output still correct
- Benchmark LLVM -O2 vs gcc -O2 on C backend output

### Success criterion
`quantac build --target llvm hello.quanta` produces a working native
executable without manual intervention.

## Phase 4: 5,000-Line Program (weeks 12-18)

### 4.1 Choose Target
Write one genuinely useful program in QuantaLang:
- JSON parser/formatter (recursive descent + string building)
- Markdown-to-HTML converter (text processing + file I/O)
- Line-count tool with language detection (like tokei/cloc)

### 4.2 Grow Stdlib from Needs
Every missing function discovered while writing the target gets added
to the stdlib. Real standard libraries grow from use, not design.

### 4.3 Prove It
- Compile with C and LLVM backends
- Test against standard inputs, compare with reference implementations
- Document results in TEST_RESULTS.md

### Success criterion
A 5,000-line program compiles with both backends, runs correctly,
and matches reference output.

## Phase 5: WASM Playground (weeks 18-24)

### 5.1 WASM Backend Validation
Verify .wat output with wasmtime or wasmer.

### 5.2 Browser Playground
- Compile the QuantaLang compiler to WASM (Rust wasm32 target)
- Web page: editor + compile + output
- Host on GitHub Pages

### Success criterion
Anyone with a browser can write and run QuantaLang code.
