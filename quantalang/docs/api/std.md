# QuantaLang Standard Library Reference

The QuantaLang standard library provides essential functionality for building applications. This document provides a comprehensive reference for all modules.

## Module Overview

| Module | Description |
|--------|-------------|
| [std::vec](#stdvec) | Dynamic arrays |
| [std::string](#stdstring) | UTF-8 strings |
| [std::hashmap](#stdhashmap) | Hash-based key-value maps |
| [std::btree](#stdbtree) | Ordered B-tree maps |
| [std::io](#stdio) | Input/output operations |
| [std::path](#stdpath) | Filesystem path manipulation |
| [std::process](#stdprocess) | Process spawning and management |
| [std::env](#stdenv) | Environment variables |
| [std::net](#stdnet) | Networking (TCP, UDP, HTTP) |
| [std::sync](#stdsync) | Synchronization primitives |
| [std::time](#stdtime) | Time and duration |
| [std::regex](#stdregex) | Regular expressions |
| [std::json](#stdjson) | JSON parsing and serialization |
| [std::crypto](#stdcrypto) | Cryptographic hashing |
| [std::rand](#stdrand) | Random number generation |
| [std::compress](#stdcompress) | Compression (gzip, zlib) |
| [std::base64](#stdbase64) | Base64 encoding |
| [std::uuid](#stduuid) | UUID generation |

---

## std::vec

Dynamic array type with amortized O(1) push/pop operations.

### Types

```quanta
pub struct Vec<T> { ... }
```

### Creating Vectors

```quanta
// Empty vector
let v: Vec<i32> = Vec::new();

// With capacity
let v = Vec::with_capacity(100);

// Using macro
let v = vec![1, 2, 3, 4, 5];

// Repeated element
let v = vec![0; 10];  // [0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
```

### Key Methods

| Method | Description |
|--------|-------------|
| `push(value)` | Add element to end |
| `pop() -> Option<T>` | Remove and return last element |
| `len() -> usize` | Number of elements |
| `is_empty() -> bool` | Check if empty |
| `get(index) -> Option<&T>` | Get element by index |
| `first() -> Option<&T>` | Get first element |
| `last() -> Option<&T>` | Get last element |
| `clear()` | Remove all elements |
| `insert(index, value)` | Insert at position |
| `remove(index) -> T` | Remove at position |
| `sort()` | Sort in place |
| `reverse()` | Reverse in place |
| `iter() -> Iterator` | Get iterator |

### Example

```quanta
use std::vec::Vec;

let mut numbers = vec![3, 1, 4, 1, 5, 9];

// Add elements
numbers.push(2);
numbers.push(6);

// Sort
numbers.sort();

// Iterate
for n in numbers.iter() {
    println!("{}", n);
}

// Filter and collect
let even: Vec<i32> = numbers.iter()
    .filter(|n| *n % 2 == 0)
    .collect();
```

---

## std::string

UTF-8 encoded string type with dynamic growth.

### Types

```quanta
pub struct String { ... }
```

### Creating Strings

```quanta
// Empty string
let s = String::new();

// From literal
let s = String::from("hello");

// Using to_string()
let s = "hello".to_string();

// With capacity
let s = String::with_capacity(100);
```

### Key Methods

| Method | Description |
|--------|-------------|
| `len() -> usize` | Length in bytes |
| `chars() -> Iterator` | Iterate over characters |
| `push(char)` | Append character |
| `push_str(&str)` | Append string slice |
| `contains(&str) -> bool` | Check for substring |
| `starts_with(&str) -> bool` | Check prefix |
| `ends_with(&str) -> bool` | Check suffix |
| `trim() -> &str` | Remove whitespace |
| `split(&str) -> Iterator` | Split by delimiter |
| `replace(&str, &str) -> String` | Replace occurrences |
| `to_uppercase() -> String` | Convert to uppercase |
| `to_lowercase() -> String` | Convert to lowercase |

### Example

```quanta
use std::string::String;

let mut message = String::from("Hello");
message.push_str(", World!");

if message.contains("World") {
    println!("Found it!");
}

let words: Vec<&str> = message.split(", ").collect();
```

---

## std::hashmap

Hash table implementation with O(1) average-case lookup.

### Types

```quanta
pub struct HashMap<K, V> { ... }
```

### Key Methods

| Method | Description |
|--------|-------------|
| `new() -> HashMap` | Create empty map |
| `insert(key, value) -> Option<V>` | Insert key-value pair |
| `get(&key) -> Option<&V>` | Get value by key |
| `remove(&key) -> Option<V>` | Remove by key |
| `contains_key(&key) -> bool` | Check if key exists |
| `len() -> usize` | Number of entries |
| `keys() -> Iterator` | Iterate over keys |
| `values() -> Iterator` | Iterate over values |
| `iter() -> Iterator` | Iterate over pairs |

### Example

```quanta
use std::hashmap::HashMap;

let mut scores = HashMap::new();
scores.insert("Alice", 100);
scores.insert("Bob", 85);
scores.insert("Carol", 92);

// Access
if let Some(score) = scores.get("Alice") {
    println!("Alice's score: {}", score);
}

// Iterate
for (name, score) in scores.iter() {
    println!("{}: {}", name, score);
}

// Entry API
scores.entry("Dave")
    .or_insert(0);
```

---

## std::io

Input/output traits and file operations.

### Traits

```quanta
pub trait Read {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize>;
    fn read_to_string(&mut self, buf: &mut String) -> Result<usize>;
    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> Result<usize>;
}

pub trait Write {
    fn write(&mut self, buf: &[u8]) -> Result<usize>;
    fn write_all(&mut self, buf: &[u8]) -> Result<()>;
    fn flush(&mut self) -> Result<()>;
}

pub trait BufRead: Read {
    fn read_line(&mut self, buf: &mut String) -> Result<usize>;
    fn lines() -> Lines;
}
```

### Types

```quanta
pub struct File { ... }
pub struct BufReader<R> { ... }
pub struct BufWriter<W> { ... }
```

### Example

```quanta
use std::io::{File, Read, Write, BufReader, BufRead};

// Write to file
let mut file = File::create("output.txt")?;
file.write_all(b"Hello, World!")?;

// Read entire file
let contents = std::io::read_to_string("input.txt")?;

// Read line by line
let file = File::open("data.txt")?;
let reader = BufReader::new(file);
for line in reader.lines() {
    println!("{}", line?);
}

// Stdin/stdout
use std::io::{stdin, stdout};
let mut input = String::new();
stdin().read_line(&mut input)?;
```

---

## std::net

Networking with TCP, UDP, and HTTP support.

### TCP Example

```quanta
use std::net::{TcpListener, TcpStream};
use std::io::{Read, Write};

// Server
let listener = TcpListener::bind("127.0.0.1:8080")?;
for stream in listener.incoming() {
    let mut stream = stream?;
    let mut buf = [0; 1024];
    let n = stream.read(&mut buf)?;
    stream.write_all(&buf[..n])?;
}

// Client
let mut stream = TcpStream::connect("127.0.0.1:8080")?;
stream.write_all(b"Hello")?;
let mut response = String::new();
stream.read_to_string(&mut response)?;
```

### HTTP Example

```quanta
use std::net::http::{Client, Request, Response};

// GET request
let client = Client::new();
let response = client.get("https://api.example.com/data")?;
println!("Status: {}", response.status());
println!("Body: {}", response.text()?);

// POST with JSON
let response = client
    .post("https://api.example.com/users")
    .header("Content-Type", "application/json")
    .body(r#"{"name": "Alice"}"#)
    .send()?;
```

---

## std::sync

Thread synchronization primitives.

### Types

| Type | Description |
|------|-------------|
| `Mutex<T>` | Mutual exclusion lock |
| `RwLock<T>` | Read-write lock |
| `Arc<T>` | Atomic reference counting |
| `Channel<T>` | Message passing channel |
| `Barrier` | Thread synchronization point |
| `Condvar` | Condition variable |

### Example

```quanta
use std::sync::{Arc, Mutex, Channel};
use std::thread;

// Shared state with Mutex
let counter = Arc::new(Mutex::new(0));
let handles: Vec<_> = (0..10).map(|_| {
    let counter = Arc::clone(&counter);
    thread::spawn(move || {
        let mut num = counter.lock().unwrap();
        *num += 1;
    })
}).collect();

for handle in handles {
    handle.join().unwrap();
}
println!("Counter: {}", *counter.lock().unwrap());

// Message passing
let (tx, rx) = Channel::new();
thread::spawn(move || {
    tx.send("Hello from thread").unwrap();
});
println!("Received: {}", rx.recv().unwrap());
```

---

## std::regex

Regular expression matching and replacement.

### Types

```quanta
pub struct Regex { ... }
pub struct Match { ... }
pub struct Captures { ... }
```

### Example

```quanta
use std::regex::Regex;

let re = Regex::new(r"\d{3}-\d{4}")?;

// Check match
if re.is_match("Call 555-1234") {
    println!("Found phone number!");
}

// Find matches
for m in re.find_iter("555-1234 and 555-5678") {
    println!("Found: {}", m.as_str());
}

// Capture groups
let re = Regex::new(r"(\w+)@(\w+)\.(\w+)")?;
if let Some(caps) = re.captures("user@example.com") {
    println!("User: {}", &caps[1]);
    println!("Domain: {}", &caps[2]);
}

// Replace
let result = re.replace_all("test@example.com", "***@***.***");
```

---

## std::json

JSON parsing and serialization.

### Types

```quanta
pub enum Value {
    Null,
    Bool(bool),
    Number(Number),
    String(String),
    Array(Vec<Value>),
    Object(Map),
}
```

### Example

```quanta
use std::json::{self, Value};

// Parse JSON
let data = r#"
{
    "name": "Alice",
    "age": 30,
    "hobbies": ["reading", "gaming"]
}
"#;

let value: Value = json::from_str(data)?;

// Access fields
println!("Name: {}", value["name"].as_str().unwrap());
println!("Age: {}", value["age"].as_i64().unwrap());

// Create JSON
let obj = json::json!({
    "name": "Bob",
    "active": true,
    "scores": [85, 90, 78]
});

// Serialize
let json_string = json::to_string_pretty(&obj)?;
```

---

## std::crypto

Cryptographic hashing and authentication.

### Functions

| Function | Description |
|----------|-------------|
| `sha256(data) -> Digest256` | SHA-256 hash |
| `sha512(data) -> Digest512` | SHA-512 hash |
| `blake3(data) -> Digest256` | BLAKE3 hash |
| `hmac_sha256(key, data) -> Digest256` | HMAC-SHA256 |
| `pbkdf2_sha256(password, salt, iterations, output)` | Key derivation |

### Example

```quanta
use std::crypto::{sha256, hmac_sha256, pbkdf2_sha256};

// Hash data
let hash = sha256(b"Hello, World!");
println!("SHA-256: {}", hash.to_hex());

// HMAC
let mac = hmac_sha256(b"secret-key", b"message");
println!("HMAC: {}", mac.to_hex());

// Key derivation
let mut key = [0u8; 32];
pbkdf2_sha256(b"password", b"salt", 100_000, &mut key);
```

---

## std::rand

Random number generation.

### Types

```quanta
pub struct Xoshiro256StarStar { ... }  // Fast PRNG
pub struct ChaCha20Rng { ... }          // Cryptographic RNG
```

### Example

```quanta
use std::rand::{self, Rng, thread_rng};

let mut rng = thread_rng();

// Random numbers
let n: u64 = rng.gen_u64();
let f: f64 = rng.gen_f64();  // [0, 1)

// Range
let die = rng.gen_range_u64(1, 7);

// Collections
let items = vec!["a", "b", "c", "d"];
let choice = rng.choose(&items).unwrap();

// Shuffle
let mut deck = vec![1, 2, 3, 4, 5];
rng.shuffle(&mut deck);

// Cryptographic random
use std::rand::getrandom;
let mut bytes = [0u8; 32];
getrandom(&mut bytes)?;
```

---

## std::compress

Compression and decompression.

### Functions

| Function | Description |
|----------|-------------|
| `gzip(data) -> Result<Vec<u8>>` | Gzip compress |
| `gunzip(data) -> Result<Vec<u8>>` | Gzip decompress |
| `zlib_compress(data) -> Result<Vec<u8>>` | Zlib compress |
| `zlib_decompress(data) -> Result<Vec<u8>>` | Zlib decompress |

### Example

```quanta
use std::compress::{gzip, gunzip, Level};

let data = b"Hello, World! ".repeat(100);

// Compress
let compressed = gzip(&data)?;
println!("Compressed: {} -> {} bytes", data.len(), compressed.len());

// Decompress
let decompressed = gunzip(&compressed)?;
assert_eq!(data, decompressed);

// Custom compression level
use std::compress::gzip_level;
let fast = gzip_level(&data, Level::FAST)?;
let best = gzip_level(&data, Level::BEST)?;
```

---

## Quick Reference Card

### Commonly Used Imports

```quanta
// Collections
use std::vec::Vec;
use std::string::String;
use std::hashmap::HashMap;

// I/O
use std::io::{File, Read, Write, BufReader, BufRead};
use std::path::Path;

// Concurrency
use std::sync::{Arc, Mutex, Channel};
use std::thread;

// Serialization
use std::json;
use std::regex::Regex;

// Utilities
use std::time::{Duration, Instant};
use std::env;
```

### Common Patterns

```quanta
// Error handling with ?
fn process() -> Result<(), Error> {
    let data = read_file("input.txt")?;
    let parsed = parse_data(&data)?;
    write_output(&parsed)?;
    Ok(())
}

// Option handling
let value = map.get("key").unwrap_or(&default);
let value = map.get("key").map(|v| v * 2);

// Iterators
let sum: i32 = numbers.iter().sum();
let doubled: Vec<_> = numbers.iter().map(|n| n * 2).collect();
let filtered: Vec<_> = numbers.iter().filter(|n| *n > 0).collect();
```
