# Tutorial: Building a Command-Line Application

This tutorial walks you through building a complete command-line application in QuantaLang. We'll create `minigrep`, a simplified version of the `grep` text search tool.

## What We'll Build

Our `minigrep` will:
- Accept a search pattern and filename as arguments
- Read the file contents
- Find and display lines matching the pattern
- Support case-insensitive search
- Handle errors gracefully

## Prerequisites

- QuantaLang installed ([Getting Started](../guide/getting-started.md))
- Basic familiarity with the language

## Project Setup

Create a new project:

```bash
quanta new minigrep
cd minigrep
```

## Step 1: Parsing Command-Line Arguments

First, let's handle command-line arguments. Edit `src/main.quanta`:

```quanta
use std::env;
use std::process;

struct Config {
    query: String,
    filename: String,
    case_insensitive: bool,
}

impl Config {
    fn new(args: &[String]) -> Result<Config, &'static str> {
        if args.len() < 3 {
            return Err("not enough arguments");
        }
        
        let query = args[1].clone();
        let filename = args[2].clone();
        
        // Check for -i flag
        let case_insensitive = args.iter().any(|arg| arg == "-i");
        
        Ok(Config {
            query,
            filename,
            case_insensitive,
        })
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    
    let config = Config::new(&args).unwrap_or_else(|err| {
        eprintln!("Error parsing arguments: {}", err);
        eprintln!("Usage: minigrep [-i] <query> <filename>");
        process::exit(1);
    });
    
    println!("Searching for '{}' in '{}'", config.query, config.filename);
}
```

Test it:

```bash
quanta run -- hello poem.txt
# Searching for 'hello' in 'poem.txt'
```

## Step 2: Reading the File

Now let's read the file contents:

```quanta
use std::env;
use std::process;
use std::io::{File, Read};

struct Config {
    query: String,
    filename: String,
    case_insensitive: bool,
}

impl Config {
    fn new(args: &[String]) -> Result<Config, &'static str> {
        if args.len() < 3 {
            return Err("not enough arguments");
        }
        
        let query = args[1].clone();
        let filename = args[2].clone();
        let case_insensitive = args.iter().any(|arg| arg == "-i");
        
        Ok(Config { query, filename, case_insensitive })
    }
}

fn run(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = File::open(&config.filename)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    
    println!("File contents:\n{}", contents);
    
    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().collect();
    
    let config = Config::new(&args).unwrap_or_else(|err| {
        eprintln!("Error: {}", err);
        eprintln!("Usage: minigrep [-i] <query> <filename>");
        process::exit(1);
    });
    
    if let Err(e) = run(&config) {
        eprintln!("Application error: {}", e);
        process::exit(1);
    }
}
```

Create a test file:

```bash
echo "Hello, World!
How are you today?
hello there, friend
HELLO EVERYONE" > poem.txt
```

## Step 3: Implementing the Search

Now let's implement the actual search functionality:

```quanta
use std::env;
use std::process;
use std::io::{File, Read};

struct Config {
    query: String,
    filename: String,
    case_insensitive: bool,
}

impl Config {
    fn new(args: &[String]) -> Result<Config, &'static str> {
        if args.len() < 3 {
            return Err("not enough arguments");
        }
        
        let query = args[1].clone();
        let filename = args[2].clone();
        let case_insensitive = args.iter().any(|arg| arg == "-i");
        
        Ok(Config { query, filename, case_insensitive })
    }
}

/// Search for lines containing the query (case-sensitive)
fn search<'a>(query: &str, contents: &'a str) -> Vec<&'a str> {
    contents
        .lines()
        .filter(|line| line.contains(query))
        .collect()
}

/// Search for lines containing the query (case-insensitive)
fn search_case_insensitive<'a>(query: &str, contents: &'a str) -> Vec<&'a str> {
    let query = query.to_lowercase();
    
    contents
        .lines()
        .filter(|line| line.to_lowercase().contains(&query))
        .collect()
}

fn run(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = File::open(&config.filename)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    
    let results = if config.case_insensitive {
        search_case_insensitive(&config.query, &contents)
    } else {
        search(&config.query, &contents)
    };
    
    for line in results {
        println!("{}", line);
    }
    
    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().collect();
    
    let config = Config::new(&args).unwrap_or_else(|err| {
        eprintln!("Error: {}", err);
        eprintln!("Usage: minigrep [-i] <query> <filename>");
        process::exit(1);
    });
    
    if let Err(e) = run(&config) {
        eprintln!("Application error: {}", e);
        process::exit(1);
    }
}
```

Test it:

```bash
# Case-sensitive search
quanta run -- hello poem.txt
# hello there, friend

# Case-insensitive search
quanta run -- -i hello poem.txt
# Hello, World!
# hello there, friend
# HELLO EVERYONE
```

## Step 4: Adding Line Numbers

Let's enhance the output with line numbers:

```quanta
/// Search result with line information
struct Match<'a> {
    line_number: usize,
    content: &'a str,
}

fn search_with_lines<'a>(query: &str, contents: &'a str, case_insensitive: bool) -> Vec<Match<'a>> {
    let query_lower = query.to_lowercase();
    
    contents
        .lines()
        .enumerate()
        .filter(|(_, line)| {
            if case_insensitive {
                line.to_lowercase().contains(&query_lower)
            } else {
                line.contains(query)
            }
        })
        .map(|(num, line)| Match {
            line_number: num + 1,
            content: line,
        })
        .collect()
}

fn run(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = File::open(&config.filename)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    
    let results = search_with_lines(&config.query, &contents, config.case_insensitive);
    
    if results.is_empty() {
        println!("No matches found.");
    } else {
        println!("Found {} match(es):", results.len());
        for m in results {
            println!("{:4}: {}", m.line_number, m.content);
        }
    }
    
    Ok(())
}
```

Output:

```bash
quanta run -- -i hello poem.txt
# Found 3 match(es):
#    1: Hello, World!
#    3: hello there, friend
#    4: HELLO EVERYONE
```

## Step 5: Adding Tests

Add tests to verify our search functions. Create `tests/test_search.quanta`:

```quanta
use minigrep::{search, search_case_insensitive};

#[test]
fn test_case_sensitive() {
    let query = "duct";
    let contents = "\
Rust:
safe, fast, productive.
Pick three.
Duct tape.";

    let results = search(query, contents);
    assert_eq!(results, vec!["safe, fast, productive."]);
}

#[test]
fn test_case_insensitive() {
    let query = "rUsT";
    let contents = "\
Rust:
safe, fast, productive.
Pick three.
Trust me.";

    let results = search_case_insensitive(query, contents);
    assert_eq!(results, vec!["Rust:", "Trust me."]);
}

#[test]
fn test_no_matches() {
    let query = "xyz";
    let contents = "abc\ndef\nghi";
    
    let results = search(query, contents);
    assert!(results.is_empty());
}

#[test]
fn test_empty_query() {
    let query = "";
    let contents = "line1\nline2";
    
    // Empty query matches everything
    let results = search(query, contents);
    assert_eq!(results.len(), 2);
}
```

Run tests:

```bash
quanta test
# Running 4 tests...
# test_case_sensitive ... ok
# test_case_insensitive ... ok
# test_no_matches ... ok
# test_empty_query ... ok
# 
# 4 passed, 0 failed
```

## Step 6: Environment Variables

Let's support an environment variable for case-insensitivity:

```quanta
impl Config {
    fn new(args: &[String]) -> Result<Config, &'static str> {
        if args.len() < 3 {
            return Err("not enough arguments");
        }
        
        let query = args[1].clone();
        let filename = args[2].clone();
        
        // Check flag OR environment variable
        let case_insensitive = args.iter().any(|arg| arg == "-i") 
            || env::var("MINIGREP_CASE_INSENSITIVE").is_ok();
        
        Ok(Config { query, filename, case_insensitive })
    }
}
```

Now you can use either:

```bash
# With flag
quanta run -- -i hello poem.txt

# With environment variable
MINIGREP_CASE_INSENSITIVE=1 quanta run -- hello poem.txt
```

## Step 7: Colorized Output

Let's add color highlighting for matches:

```quanta
/// ANSI color codes
mod colors {
    pub const RED: &str = "\x1b[31m";
    pub const GREEN: &str = "\x1b[32m";
    pub const YELLOW: &str = "\x1b[33m";
    pub const RESET: &str = "\x1b[0m";
    pub const BOLD: &str = "\x1b[1m";
}

fn highlight_match(line: &str, query: &str, case_insensitive: bool) -> String {
    if case_insensitive {
        // For case-insensitive, we need to find the actual match position
        let lower_line = line.to_lowercase();
        let lower_query = query.to_lowercase();
        
        let mut result = String::new();
        let mut last_end = 0;
        
        for (start, _) in lower_line.match_indices(&lower_query) {
            result.push_str(&line[last_end..start]);
            result.push_str(colors::RED);
            result.push_str(colors::BOLD);
            result.push_str(&line[start..start + query.len()]);
            result.push_str(colors::RESET);
            last_end = start + query.len();
        }
        
        result.push_str(&line[last_end..]);
        result
    } else {
        line.replace(query, &format!("{}{}{}{}", 
            colors::RED, colors::BOLD, query, colors::RESET))
    }
}

fn run(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = File::open(&config.filename)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    
    let results = search_with_lines(&config.query, &contents, config.case_insensitive);
    
    if results.is_empty() {
        println!("{}No matches found.{}", colors::YELLOW, colors::RESET);
    } else {
        println!("{}Found {} match(es):{}", colors::GREEN, results.len(), colors::RESET);
        for m in results {
            let highlighted = highlight_match(m.content, &config.query, config.case_insensitive);
            println!("{}{:4}:{} {}", colors::YELLOW, m.line_number, colors::RESET, highlighted);
        }
    }
    
    Ok(())
}
```

## Final Code

Here's the complete `src/main.quanta`:

```quanta
//! minigrep - A simplified grep implementation
//!
//! Usage: minigrep [-i] <query> <filename>

use std::env;
use std::process;
use std::io::{File, Read};

/// ANSI color codes for terminal output
mod colors {
    pub const RED: &str = "\x1b[31m";
    pub const GREEN: &str = "\x1b[32m";
    pub const YELLOW: &str = "\x1b[33m";
    pub const RESET: &str = "\x1b[0m";
    pub const BOLD: &str = "\x1b[1m";
}

/// Configuration parsed from command-line arguments
struct Config {
    query: String,
    filename: String,
    case_insensitive: bool,
}

impl Config {
    /// Parse configuration from command-line arguments
    fn new(args: &[String]) -> Result<Config, &'static str> {
        if args.len() < 3 {
            return Err("not enough arguments");
        }
        
        let query = args[1].clone();
        let filename = args[2].clone();
        let case_insensitive = args.iter().any(|arg| arg == "-i") 
            || env::var("MINIGREP_CASE_INSENSITIVE").is_ok();
        
        Ok(Config { query, filename, case_insensitive })
    }
}

/// A match result with line information
struct Match<'a> {
    line_number: usize,
    content: &'a str,
}

/// Search for matching lines with line numbers
fn search_with_lines<'a>(query: &str, contents: &'a str, case_insensitive: bool) -> Vec<Match<'a>> {
    let query_lower = query.to_lowercase();
    
    contents
        .lines()
        .enumerate()
        .filter(|(_, line)| {
            if case_insensitive {
                line.to_lowercase().contains(&query_lower)
            } else {
                line.contains(query)
            }
        })
        .map(|(num, line)| Match {
            line_number: num + 1,
            content: line,
        })
        .collect()
}

/// Highlight matching text in a line
fn highlight_match(line: &str, query: &str, case_insensitive: bool) -> String {
    let highlight = |s: &str| format!("{}{}{}{}", colors::RED, colors::BOLD, s, colors::RESET);
    
    if case_insensitive {
        let lower_line = line.to_lowercase();
        let lower_query = query.to_lowercase();
        let mut result = String::new();
        let mut last_end = 0;
        
        for (start, _) in lower_line.match_indices(&lower_query) {
            result.push_str(&line[last_end..start]);
            result.push_str(&highlight(&line[start..start + query.len()]));
            last_end = start + query.len();
        }
        result.push_str(&line[last_end..]);
        result
    } else {
        line.replace(query, &highlight(query))
    }
}

/// Run the search and display results
fn run(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = File::open(&config.filename)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    
    let results = search_with_lines(&config.query, &contents, config.case_insensitive);
    
    if results.is_empty() {
        println!("{}No matches found.{}", colors::YELLOW, colors::RESET);
    } else {
        println!("{}Found {} match(es):{}", colors::GREEN, results.len(), colors::RESET);
        for m in results {
            let highlighted = highlight_match(m.content, &config.query, config.case_insensitive);
            println!("{}{:4}:{} {}", colors::YELLOW, m.line_number, colors::RESET, highlighted);
        }
    }
    
    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().collect();
    
    let config = Config::new(&args).unwrap_or_else(|err| {
        eprintln!("{}Error:{} {}", colors::RED, colors::RESET, err);
        eprintln!("Usage: minigrep [-i] <query> <filename>");
        process::exit(1);
    });
    
    if let Err(e) = run(&config) {
        eprintln!("{}Error:{} {}", colors::RED, colors::RESET, e);
        process::exit(1);
    }
}
```

## What We Learned

In this tutorial, you learned how to:

1. **Parse command-line arguments** using `std::env`
2. **Read files** with `std::io::File`
3. **Handle errors** with `Result` and `?` operator
4. **Use iterators** for data transformation
5. **Write tests** to verify functionality
6. **Use environment variables** for configuration
7. **Add colors** to terminal output

## Next Steps

Try extending minigrep with:

- Regular expression support using `std::regex`
- Recursive directory searching
- Multiple file support
- Count-only mode (`-c` flag)
- Invert match (`-v` flag)
- Context lines (`-A`, `-B`, `-C` flags)

## See Also

- [Standard Library Reference](../api/std.md)
- [Error Handling Guide](../guide/error-handling.md)
- [Testing Guide](../guide/testing.md)
