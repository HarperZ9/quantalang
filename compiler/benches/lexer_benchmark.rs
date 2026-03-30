// ===============================================================================
// QUANTALANG LEXER - BENCHMARKS
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use quantalang::lexer::{Lexer, SourceFile};

/// Sample QuantaLang source code for benchmarking.
const SAMPLE_CODE: &str = r#"
// QuantaLang Sample Code
module sample

use std::collections::HashMap
use std::io::{Read, Write}

/// A simple point structure
pub struct Point<T> {
    x: T,
    y: T,
}

impl<T: Add<Output = T>> Point<T> {
    pub fn new(x: T, y: T) -> Self {
        Point { x, y }
    }

    pub fn add(&self, other: &Point<T>) -> Point<T> {
        Point {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }
}

/// Enum with different variants
pub enum Result<T, E> {
    Ok(T),
    Err(E),
}

/// Main function
pub fn main() {
    let x = 42;
    let y = 3.14159;
    let s = "Hello, QuantaLang!";
    let b = true;

    // Control flow
    if x > 0 {
        println("positive")
    } else if x < 0 {
        println("negative")
    } else {
        println("zero")
    }

    // Match expression
    match result {
        Result::Ok(v) => {
            println("Got value: {}", v)
        }
        Result::Err(e) => {
            println("Error: {}", e)
        }
    }

    // Loops
    for i in 0..10 {
        println("i = {}", i)
    }

    while condition {
        do_something()
    }

    loop {
        if done {
            break
        }
        process()
    }

    // Closures
    let add = |a, b| a + b;
    let result = add(1, 2);

    // DSL blocks
    let query = sql! {
        SELECT id, name, email
        FROM users
        WHERE age > 18
        ORDER BY name ASC
    };

    let pattern = regex! {
        ^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$
    };

    // Async/await
    async fn fetch_data() -> Result<Data, Error> {
        let response = http::get("https://api.example.com/data").await?;
        let data = response.json().await?;
        Ok(data)
    }

    // Unsafe block
    unsafe {
        let ptr = &x as *const i32;
        let value = *ptr;
    }

    // Generic function
    fn identity<T>(x: T) -> T {
        x
    }

    // Trait definition
    trait Drawable {
        fn draw(&self);
        fn bounds(&self) -> Rectangle;
    }

    // Trait implementation
    impl Drawable for Circle {
        fn draw(&self) {
            // Implementation
        }

        fn bounds(&self) -> Rectangle {
            Rectangle::new(
                self.center.x - self.radius,
                self.center.y - self.radius,
                self.radius * 2.0,
                self.radius * 2.0,
            )
        }
    }
}
"#;

/// Generate repeated code for larger benchmarks.
fn generate_large_source(repetitions: usize) -> String {
    let mut source = String::with_capacity(SAMPLE_CODE.len() * repetitions);
    for _ in 0..repetitions {
        source.push_str(SAMPLE_CODE);
        source.push('\n');
    }
    source
}

fn bench_lexer_small(c: &mut Criterion) {
    let source = SourceFile::anonymous(SAMPLE_CODE);

    let mut group = c.benchmark_group("lexer_small");
    group.throughput(Throughput::Bytes(SAMPLE_CODE.len() as u64));

    group.bench_function("tokenize", |b| {
        b.iter(|| {
            let mut lexer = Lexer::new(&source);
            black_box(lexer.tokenize().unwrap())
        })
    });

    group.finish();
}

fn bench_lexer_medium(c: &mut Criterion) {
    let code = generate_large_source(10);
    let source = SourceFile::anonymous(&code);

    let mut group = c.benchmark_group("lexer_medium");
    group.throughput(Throughput::Bytes(code.len() as u64));

    group.bench_function("tokenize", |b| {
        b.iter(|| {
            let mut lexer = Lexer::new(&source);
            black_box(lexer.tokenize().unwrap())
        })
    });

    group.finish();
}

fn bench_lexer_large(c: &mut Criterion) {
    let code = generate_large_source(100);
    let source = SourceFile::anonymous(&code);

    let mut group = c.benchmark_group("lexer_large");
    group.throughput(Throughput::Bytes(code.len() as u64));
    group.sample_size(50);

    group.bench_function("tokenize", |b| {
        b.iter(|| {
            let mut lexer = Lexer::new(&source);
            black_box(lexer.tokenize().unwrap())
        })
    });

    group.finish();
}

fn bench_specific_tokens(c: &mut Criterion) {
    let mut group = c.benchmark_group("specific_tokens");

    // Benchmark string parsing
    let strings = r#""hello" "world" "with\nnewline" "unicode\u{1F600}" "escaped\"quote""#;
    let string_source = SourceFile::anonymous(strings);
    group.bench_function("strings", |b| {
        b.iter(|| {
            let mut lexer = Lexer::new(&string_source);
            black_box(lexer.tokenize().unwrap())
        })
    });

    // Benchmark number parsing
    let numbers = "42 3.14 0xFF 0b1010 1e10 1_000_000 0o755 123.456e-7";
    let number_source = SourceFile::anonymous(numbers);
    group.bench_function("numbers", |b| {
        b.iter(|| {
            let mut lexer = Lexer::new(&number_source);
            black_box(lexer.tokenize().unwrap())
        })
    });

    // Benchmark identifiers/keywords
    let idents = "fn struct enum trait impl let mut pub mod use if else match for while loop";
    let ident_source = SourceFile::anonymous(idents);
    group.bench_function("identifiers", |b| {
        b.iter(|| {
            let mut lexer = Lexer::new(&ident_source);
            black_box(lexer.tokenize().unwrap())
        })
    });

    // Benchmark operators
    let operators = "+ - * / % ^ & | ! < > = += -= *= /= == != <= >= && || << >> :: -> =>";
    let op_source = SourceFile::anonymous(operators);
    group.bench_function("operators", |b| {
        b.iter(|| {
            let mut lexer = Lexer::new(&op_source);
            black_box(lexer.tokenize().unwrap())
        })
    });

    group.finish();
}

fn bench_comments(c: &mut Criterion) {
    let mut group = c.benchmark_group("comments");

    // Code with many comments
    let commented = r#"
        // Line comment
        let x = 42; // Inline comment
        /* Block comment */
        let y = /* inline block */ 3.14;
        /* Nested /* comments /* are /* supported */ */ */ */
        /// Doc comment
        fn foo() {}
    "#
    .repeat(100);

    let source = SourceFile::anonymous(&commented);
    group.throughput(Throughput::Bytes(commented.len() as u64));

    group.bench_function("skip_comments", |b| {
        b.iter(|| {
            let mut lexer = Lexer::new(&source);
            black_box(lexer.tokenize().unwrap())
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_lexer_small,
    bench_lexer_medium,
    bench_lexer_large,
    bench_specific_tokens,
    bench_comments,
);

criterion_main!(benches);
