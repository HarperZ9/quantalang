// ===============================================================================
// QUANTALANG COMPILER - MAIN ENTRY POINT
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! QuantaLang Compiler (`quantac`)
//!
//! This is the main entry point for the QuantaLang compiler command-line tool.

use clap::{Parser as ClapParser, Subcommand};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use quantalang::lexer::{SourceFile, Lexer, Span};
use quantalang::parser::{Parser, parse};
use quantalang::ast::{self, Module, ItemKind, Ident, Visibility, ExprKind, StmtKind, Block, Expr};
use quantalang::types::{TypeContext, TypeChecker};
use quantalang::codegen::{CodeGenerator, Target};

/// QuantaLang Compiler
#[derive(ClapParser)]
#[command(name = "quantac")]
#[command(author = "Zain Dana Harper")]
#[command(version)]
#[command(about = "The QuantaLang compiler - a multi-paradigm systems programming language")]
#[command(long_about = None)]
struct Cli {
    /// The command to run
    #[command(subcommand)]
    command: Option<Commands>,

    /// Input file to compile
    #[arg(value_name = "FILE")]
    input: Option<PathBuf>,

    /// Output file
    #[arg(short, long, value_name = "FILE")]
    output: Option<PathBuf>,

    /// Enable verbose output
    #[arg(short, long)]
    verbose: bool,

    /// Emit debug information
    #[arg(short = 'g', long)]
    debug: bool,

    /// Optimization level (0-3)
    #[arg(short = 'O', long, default_value = "0")]
    opt_level: u8,

    /// Code generation target (c, llvm, wasm, spirv, x86-64, arm64)
    #[arg(long)]
    target: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Tokenize a file and print the tokens
    Lex {
        /// Input file
        file: PathBuf,

        /// Print token details
        #[arg(short, long)]
        verbose: bool,
    },

    /// Parse a file and print the AST
    Parse {
        /// Input file
        file: PathBuf,

        /// Print AST in JSON format
        #[arg(long)]
        json: bool,
    },

    /// Type-check a file
    Check {
        /// Input file
        file: PathBuf,
    },

    /// Build a project
    Build {
        /// Project directory
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Build in release mode
        #[arg(long)]
        release: bool,

        /// Emit type: 'c' for C source only, 'exe' for executable (default)
        #[arg(long, default_value = "exe")]
        emit: String,

        /// Keep the intermediate .c file after compilation
        #[arg(long)]
        keep_c: bool,

        /// Code generation target: 'c' (default) or 'llvm'
        #[arg(long, default_value = "c")]
        target: String,
    },

    /// Run a file directly
    Run {
        /// Input file
        file: PathBuf,

        /// Arguments to pass to the program
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },

    /// Start a REPL session
    Repl,

    /// Start the Language Server Protocol server
    Lsp,

    /// Watch shader files and recompile on change
    Watch {
        /// Directory or file to watch
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Target format: 'spirv' (default), 'c'
        #[arg(long, default_value = "spirv")]
        target: String,
    },

    /// Format QuantaLang source files
    Fmt {
        /// Input file to format
        file: PathBuf,

        /// Check formatting without modifying (exit 1 if changes needed)
        #[arg(long)]
        check: bool,

        /// Write formatted output back to the file
        #[arg(short, long)]
        write: bool,
    },

    /// Package manager
    Pkg {
        #[command(subcommand)]
        command: PkgCommands,
    },

    /// Print version information
    Version,
}

#[derive(Subcommand)]
enum PkgCommands {
    /// Initialize a new Quanta.toml manifest
    Init {
        /// Project directory
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Add a dependency
    Add {
        /// Package name
        name: String,
        /// Version requirement (e.g., "^1.0")
        #[arg(long)]
        version: Option<String>,
    },
    /// Resolve dependencies and generate lockfile
    Resolve {
        /// Project directory
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Search the package registry
    Search {
        /// Search query
        query: String,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let result = match cli.command {
        Some(Commands::Lex { file, verbose }) => cmd_lex(&file, verbose),
        Some(Commands::Parse { file, json }) => cmd_parse(&file, json),
        Some(Commands::Check { file }) => cmd_check(&file),
        Some(Commands::Build { path, release, emit, keep_c, target }) => cmd_build(&path, release, &emit, keep_c, &target),
        Some(Commands::Run { file, args }) => cmd_run(&file, &args),
        Some(Commands::Repl) => cmd_repl(),
        Some(Commands::Lsp) => cmd_lsp(),
        Some(Commands::Watch { path, target }) => cmd_watch(&path, &target),
        Some(Commands::Fmt { file, check, write }) => cmd_fmt(&file, check, write),
        Some(Commands::Pkg { command }) => cmd_pkg(command),
        Some(Commands::Version) => {
            print_version();
            Ok(())
        }
        None => {
            if let Some(input) = cli.input {
                cmd_compile(&input, cli.output.as_deref(), cli.opt_level, cli.debug, cli.target.as_deref())
            } else {
                eprintln!("No input file specified. Use --help for usage information.");
                Err(1)
            }
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(code) => ExitCode::from(code as u8),
    }
}

fn print_version() {
    println!("QuantaLang Compiler (quantac) {}", quantalang::VERSION);
    println!("Language version: {}.{}.{}",
        quantalang::LANGUAGE_VERSION.0,
        quantalang::LANGUAGE_VERSION.1,
        quantalang::LANGUAGE_VERSION.2
    );
    println!("{}", quantalang::COPYRIGHT);
}

fn cmd_lex(file: &PathBuf, verbose: bool) -> Result<(), i32> {
    let source = std::fs::read_to_string(file).map_err(|e| {
        eprintln!("Error reading file '{}': {}", file.display(), e);
        1
    })?;

    let source_file = SourceFile::new(file.to_string_lossy(), source);
    let mut lexer = Lexer::new(&source_file);

    let tokens = lexer.tokenize().map_err(|e| {
        eprintln!("Lexer error: {}", e);
        1
    })?;

    for token in &tokens {
        if verbose {
            let (start, end) = source_file.span_to_positions(token.span);
            let text = source_file.slice(token.span);
            println!(
                "{:4}:{:<3} - {:4}:{:<3}  {:20} {:?}",
                start.line, start.column,
                end.line, end.column,
                format!("{}", token.kind),
                text
            );
        } else {
            println!("{}", token.kind);
        }
    }

    println!("\nTotal: {} tokens", tokens.len());
    Ok(())
}

fn cmd_parse(file: &PathBuf, json: bool) -> Result<(), i32> {
    // Read source file
    let source = std::fs::read_to_string(file).map_err(|e| {
        eprintln!("Error reading file '{}': {}", file.display(), e);
        1
    })?;

    let source_file = SourceFile::new(file.to_string_lossy(), source);

    // Tokenize
    let mut lexer = Lexer::new(&source_file);
    let tokens = lexer.tokenize().map_err(|e| {
        eprintln!("Lexer error: {}", e);
        1
    })?;

    // Parse
    let mut parser = Parser::new(&source_file, tokens);
    let ast = parser.parse().map_err(|e| {
        eprintln!("Parse error: {}", e);
        // Print any accumulated errors
        for err in parser.errors() {
            eprintln!("  {}", err);
        }
        1
    })?;

    // Display AST
    if json {
        // JSON output using serde if available
        println!("{}", format_ast_json(&ast));
    } else {
        // Pretty print AST
        println!("=== Abstract Syntax Tree ===");
        println!("File: {}", file.display());
        println!("Items: {}", ast.items.len());
        println!();

        for (i, item) in ast.items.iter().enumerate() {
            println!("Item {}: {}", i + 1, item_kind_name(&item.kind));
            print_item_summary(item, 1);
        }
    }

    Ok(())
}

fn item_kind_name(kind: &quantalang::ast::ItemKind) -> &'static str {
    match kind {
        quantalang::ast::ItemKind::Function(_) => "Function",
        quantalang::ast::ItemKind::Struct(_) => "Struct",
        quantalang::ast::ItemKind::Enum(_) => "Enum",
        quantalang::ast::ItemKind::Trait(_) => "Trait",
        quantalang::ast::ItemKind::Impl(_) => "Impl",
        quantalang::ast::ItemKind::TypeAlias(_) => "TypeAlias",
        quantalang::ast::ItemKind::Const(_) => "Const",
        quantalang::ast::ItemKind::Static(_) => "Static",
        quantalang::ast::ItemKind::Mod(_) => "Mod",
        quantalang::ast::ItemKind::Use(_) => "Use",
        quantalang::ast::ItemKind::ExternCrate(_) => "ExternCrate",
        quantalang::ast::ItemKind::ExternBlock(_) => "ExternBlock",
        quantalang::ast::ItemKind::Macro(_) => "Macro",
        quantalang::ast::ItemKind::MacroRules(_) => "MacroRules",
        quantalang::ast::ItemKind::Effect(_) => "Effect",
    }
}

fn format_ast_json(ast: &Module) -> String {
    // Simple JSON representation
    let mut output = String::new();
    output.push_str("{\n");
    output.push_str(&format!("  \"items\": {},\n", ast.items.len()));
    output.push_str("  \"item_kinds\": [\n");
    for (i, item) in ast.items.iter().enumerate() {
        let comma = if i < ast.items.len() - 1 { "," } else { "" };
        output.push_str(&format!("    \"{}\"{}\n", item_kind_name(&item.kind), comma));
    }
    output.push_str("  ]\n");
    output.push_str("}\n");
    output
}

fn struct_field_count(fields: &quantalang::ast::StructFields) -> usize {
    match fields {
        quantalang::ast::StructFields::Named(f) => f.len(),
        quantalang::ast::StructFields::Tuple(f) => f.len(),
        quantalang::ast::StructFields::Unit => 0,
    }
}

fn print_item_summary(item: &quantalang::ast::Item, indent: usize) {
    let prefix = "  ".repeat(indent);
    match &item.kind {
        quantalang::ast::ItemKind::Function(f) => {
            println!("{}fn {}()", prefix, f.name.name);
            if let Some(ret) = &f.sig.return_ty {
                println!("{}  -> {:?}", prefix, ret);
            }
        }
        quantalang::ast::ItemKind::Struct(s) => {
            println!("{}struct {} ({} fields)", prefix, s.name.name, struct_field_count(&s.fields));
        }
        quantalang::ast::ItemKind::Enum(e) => {
            println!("{}enum {} ({} variants)", prefix, e.name.name, e.variants.len());
        }
        quantalang::ast::ItemKind::Trait(t) => {
            println!("{}trait {} ({} items)", prefix, t.name.name, t.items.len());
        }
        quantalang::ast::ItemKind::Impl(i) => {
            println!("{}impl ({} items)", prefix, i.items.len());
        }
        quantalang::ast::ItemKind::TypeAlias(t) => {
            println!("{}type {}", prefix, t.name.name);
        }
        quantalang::ast::ItemKind::Const(c) => {
            println!("{}const {}", prefix, c.name.name);
        }
        quantalang::ast::ItemKind::Static(s) => {
            println!("{}static {}", prefix, s.name.name);
        }
        quantalang::ast::ItemKind::Mod(m) => {
            println!("{}mod {}", prefix, m.name.name);
        }
        quantalang::ast::ItemKind::Use(u) => {
            println!("{}use {:?}", prefix, u.tree);
        }
        quantalang::ast::ItemKind::ExternCrate(e) => {
            println!("{}extern crate {}", prefix, e.name.name);
        }
        quantalang::ast::ItemKind::ExternBlock(e) => {
            println!("{}extern \"{}\" ({} items)", prefix, e.abi.as_deref().unwrap_or("C"), e.items.len());
        }
        quantalang::ast::ItemKind::Macro(m) => {
            println!("{}macro {:?}!", prefix, m.name.as_ref().map(|n| &n.name));
        }
        quantalang::ast::ItemKind::MacroRules(m) => {
            println!("{}macro_rules! {}", prefix, m.name.name);
        }
        quantalang::ast::ItemKind::Effect(e) => {
            println!("{}effect {}", prefix, e.name.name);
        }
    }
}

fn cmd_check(file: &PathBuf) -> Result<(), i32> {
    // Read source file
    let source = std::fs::read_to_string(file).map_err(|e| {
        eprintln!("Error reading file '{}': {}", file.display(), e);
        1
    })?;

    let source_file = SourceFile::new(file.to_string_lossy(), source);

    // Tokenize
    let mut lexer = Lexer::new(&source_file);
    let tokens = lexer.tokenize().map_err(|e| {
        eprintln!("Lexer error: {}", e);
        1
    })?;

    println!("Lexing... OK ({} tokens)", tokens.len());

    // Parse
    let mut parser = Parser::new(&source_file, tokens);
    let ast = parser.parse().map_err(|e| {
        eprintln!("Parse error: {}", e);
        for err in parser.errors() {
            eprintln!("  {}", err);
        }
        1
    })?;

    println!("Parsing... OK ({} items)", ast.items.len());

    // Type check
    let mut ctx = TypeContext::new();
    let mut checker = TypeChecker::new(&mut ctx);
    checker.check_module(&ast);

    if checker.has_errors() {
        eprintln!("Type errors found:");
        for err in checker.errors() {
            eprintln!("  {}", err);
        }
        Err(1)
    } else {
        println!("Type checking... OK");
        println!();
        println!("No errors found in '{}'", file.display());
        Ok(())
    }
}

// =============================================================================
// C COMPILER DISCOVERY AND INVOCATION
// =============================================================================

/// Try to locate a working C compiler on the system.
///
/// On Windows: tries `cl.exe` (MSVC), then `gcc`, then `clang`.
/// On Unix: tries `cc`, then `gcc`, then `clang`.
///
/// Returns the compiler command name if found.
fn find_c_compiler() -> Option<String> {
    let candidates: &[&str] = if cfg!(windows) {
        &["cl.exe", "cl", "gcc", "clang"]
    } else {
        &["cc", "gcc", "clang"]
    };

    for &compiler in candidates {
        let probe = std::process::Command::new(compiler)
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();

        // MSVC `cl.exe` doesn't support --version; it prints help to stderr
        // and exits 0, or it might fail. Try a second probe without args.
        let ok = match probe {
            Ok(status) => status.success(),
            Err(_) if compiler.starts_with("cl") => {
                std::process::Command::new(compiler)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .map(|_| true)
                    .unwrap_or(false)
            }
            Err(_) => false,
        };

        if ok {
            return Some(compiler.to_string());
        }
    }

    None
}

/// Build the argument list for the chosen C compiler and invoke it.
///
/// `c_file`  - path to the generated `.c` source
/// `exe_file` - desired output executable path
/// `release` - if true, pass `-O2`; otherwise pass `-g`
/// `compiler` - the C compiler command (e.g. "gcc", "cl.exe")
///
/// Returns `Ok(())` on success, `Err(code)` on failure.
fn invoke_c_compiler(
    compiler: &str,
    c_file: &std::path::Path,
    exe_file: &std::path::Path,
    release: bool,
) -> Result<(), i32> {
    let is_msvc = compiler.starts_with("cl");

    let mut cmd = std::process::Command::new(compiler);

    if is_msvc {
        // MSVC cl.exe uses /Fe for output, /std:c11, /O2 or /Zi
        cmd.arg(c_file);
        cmd.arg(format!("/Fe:{}", exe_file.display()));
        cmd.arg("/std:c11");
        if release {
            cmd.arg("/O2");
        } else {
            cmd.arg("/Zi");
        }
        cmd.arg("/nologo");
    } else {
        // GCC / Clang / cc - POSIX-style flags
        cmd.arg(c_file);
        cmd.arg("-o");
        cmd.arg(exe_file);
        cmd.arg("-std=c99");
        if release {
            cmd.arg("-O2");
        } else {
            cmd.arg("-g");
        }
        // Link math library on non-Windows
        if !cfg!(windows) {
            cmd.arg("-lm");
        }
    }

    let output = cmd.output().map_err(|e| {
        eprintln!("Failed to invoke C compiler '{}': {}", compiler, e);
        1
    })?;

    if output.status.success() {
        Ok(())
    } else {
        eprintln!("C compilation failed:");
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.is_empty() {
            eprintln!("{}", stderr);
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.is_empty() {
            eprintln!("{}", stdout);
        }
        Err(1)
    }
}

// =============================================================================
// BUILD COMMAND
// =============================================================================

fn cmd_build(path: &PathBuf, release: bool, emit: &str, keep_c: bool, target_str: &str) -> Result<(), i32> {
    // Look for Quanta.toml or main.quanta in the project directory
    let manifest_path = path.join("Quanta.toml");
    let main_path = if manifest_path.exists() {
        // Read manifest to find entry point
        path.join("src").join("main.quanta")
    } else {
        // Look for main.quanta directly
        let main_file = path.join("main.quanta");
        if main_file.exists() {
            main_file
        } else {
            path.join("src").join("main.quanta")
        }
    };

    if !main_path.exists() {
        eprintln!("Could not find entry point. Expected one of:");
        eprintln!("  - {}/main.quanta", path.display());
        eprintln!("  - {}/src/main.quanta", path.display());
        return Err(1);
    }

    let emit_c_only = emit == "c";

    // Resolve the code generation target.
    let target = match target_str {
        "c" => Target::C,
        "llvm" => Target::LlvmIr,
        "spirv" | "spir-v" | "spv" => Target::SpirV,
        "hlsl" | "dx" | "directx" => Target::Hlsl,
        "glsl" | "opengl" | "gl" => Target::Glsl,
        other => {
            eprintln!("Unknown target '{}'. Supported targets: c, llvm, spirv, hlsl, glsl", other);
            return Err(1);
        }
    };
    let use_llvm = target == Target::LlvmIr;
    let use_spirv = target == Target::SpirV;

    println!("Building project at '{}'", path.display());
    println!("Entry point: {}", main_path.display());
    println!("Mode: {}", if release { "release" } else { "debug" });
    println!("Target: {}", target);
    if emit_c_only && !use_llvm {
        println!("Emit: C source only");
    }
    println!();

    // Read source file
    let source = std::fs::read_to_string(&main_path).map_err(|e| {
        eprintln!("Error reading file '{}': {}", main_path.display(), e);
        1
    })?;

    let source_file = SourceFile::new(main_path.to_string_lossy(), source);

    // Tokenize
    let mut lexer = Lexer::new(&source_file);
    let tokens = lexer.tokenize().map_err(|e| {
        eprintln!("Lexer error: {}", e);
        1
    })?;

    let total_steps = if emit_c_only || use_llvm { 4 } else { 5 };
    println!("[1/{}] Lexing... OK ({} tokens)", total_steps, tokens.len());

    // Parse
    let mut parser = Parser::new(&source_file, tokens);
    let mut ast = parser.parse().map_err(|e| {
        eprintln!("Parse error: {}", e);
        for err in parser.errors() {
            eprintln!("  {}", err);
        }
        1
    })?;
    println!("[2/{}] Parsing... OK ({} items)", total_steps, ast.items.len());

    // Resolve `mod foo;` declarations — load and merge external module files
    let source_dir = main_path.parent().unwrap_or(Path::new("."));
    resolve_modules(&mut ast, source_dir)?;

    // Type check
    let mut ctx = TypeContext::new();
    let mut checker = TypeChecker::new(&mut ctx);
    checker.check_module(&ast);

    if checker.has_errors() {
        eprintln!("Type errors found:");
        for err in checker.errors() {
            eprintln!("  {}", err);
        }
        return Err(1);
    }
    println!("[3/{}] Type checking... OK", total_steps);

    // Code generation
    let mut codegen = CodeGenerator::new(&ctx, target);
    let output = codegen.generate(&ast).map_err(|e| {
        eprintln!("Code generation error: {}", e);
        1
    })?;
    println!("[4/{}] Code generation ({})... OK ({} bytes)", total_steps, target, output.data.len());

    // Write output
    let output_dir = path.join("target").join(if release { "release" } else { "debug" });
    std::fs::create_dir_all(&output_dir).map_err(|e| {
        eprintln!("Failed to create output directory: {}", e);
        1
    })?;

    if use_spirv {
        // SPIR-V target: write .spv binary
        let spv_output_file = output_dir.join("main.spv");
        std::fs::write(&spv_output_file, &output.data).map_err(|e| {
            eprintln!("Failed to write SPIR-V output: {}", e);
            1
        })?;
        println!("[5/5] SPIR-V written to {}", spv_output_file.display());
        println!();
        println!("Validate with: spirv-val {}", spv_output_file.display());
        return Ok(());
    } else if use_llvm {
        // LLVM IR target: write .ll file
        let ll_output_file = output_dir.join("main.ll");
        std::fs::write(&ll_output_file, &output.data).map_err(|e| {
            eprintln!("Failed to write LLVM IR output: {}", e);
            1
        })?;

        // If --emit=exe (default), try to compile the .ll to an executable with clang
        if !emit_c_only {
            let exe_name = if cfg!(windows) { "main.exe" } else { "main" };
            let exe_output_file = output_dir.join(exe_name);

            // Check if clang is available
            let clang_ok = std::process::Command::new("clang")
                .arg("--version")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);

            if clang_ok {
                println!("[5/5] Compiling LLVM IR -> executable (using clang)...");

                let mut cmd = std::process::Command::new("clang");
                cmd.arg(&ll_output_file);
                cmd.arg("-o");
                cmd.arg(&exe_output_file);
                if release {
                    cmd.arg("-O2");
                } else {
                    cmd.arg("-g");
                }
                if !cfg!(windows) {
                    cmd.arg("-lm");
                }

                let clang_output = cmd.output().map_err(|e| {
                    eprintln!("Failed to invoke clang: {}", e);
                    1
                })?;

                if clang_output.status.success() {
                    println!("     Compilation... OK");
                    println!();
                    println!("Build successful!");
                    println!("Output: {}", exe_output_file.display());
                    return Ok(());
                } else {
                    eprintln!("clang compilation failed:");
                    let stderr = String::from_utf8_lossy(&clang_output.stderr);
                    if !stderr.is_empty() {
                        eprintln!("{}", stderr);
                    }
                    return Err(1);
                }
            } else {
                println!();
                println!("Build successful! (LLVM IR only)");
                println!("Output: {}", ll_output_file.display());
                println!();
                if cfg!(windows) {
                    println!("To compile to executable, install clang and run:");
                    println!("  clang {} -o {}", ll_output_file.display(), output_dir.join("main.exe").display());
                } else {
                    println!("To compile to executable, install clang and run:");
                    println!("  clang {} -o {} -lm", ll_output_file.display(), output_dir.join("main").display());
                }
                return Ok(());
            }
        }

        println!();
        println!("Build successful!");
        println!("Output: {}", ll_output_file.display());
        return Ok(());
    }

    // C target path
    let c_output_file = output_dir.join("main.c");
    std::fs::write(&c_output_file, &output.data).map_err(|e| {
        eprintln!("Failed to write C output: {}", e);
        1
    })?;

    // If --emit=c, stop here
    if emit_c_only {
        println!();
        println!("Build successful!");
        println!("Output: {}", c_output_file.display());
        return Ok(());
    }

    // Otherwise compile the .c file to an executable
    let exe_name = if cfg!(windows) { "main.exe" } else { "main" };
    let exe_output_file = output_dir.join(exe_name);

    let compiler = find_c_compiler().ok_or_else(|| {
        eprintln!("Error: No C compiler found on the system.");
        eprintln!("QuantaLang needs a C compiler to produce executables.");
        eprintln!();
        if cfg!(windows) {
            eprintln!("Install one of the following:");
            eprintln!("  - Visual Studio Build Tools (cl.exe): https://visualstudio.microsoft.com/downloads/");
            eprintln!("  - MinGW-w64 (gcc): https://www.mingw-w64.org/");
            eprintln!("  - LLVM/Clang: https://releases.llvm.org/");
        } else {
            eprintln!("Install one of the following:");
            eprintln!("  - GCC: sudo apt install gcc  (Debian/Ubuntu)");
            eprintln!("  - Clang: sudo apt install clang");
        }
        eprintln!();
        eprintln!("Or use --emit=c to output only the C source file.");
        1
    })?;

    println!("[5/{}] Compiling C -> executable (using {})...", total_steps, compiler);

    invoke_c_compiler(&compiler, &c_output_file, &exe_output_file, release)?;

    println!("     Compilation... OK");

    // Clean up .c file unless --keep-c
    if !keep_c {
        let _ = std::fs::remove_file(&c_output_file);
    }

    println!();
    println!("Build successful!");
    println!("Output: {}", exe_output_file.display());

    Ok(())
}

// =============================================================================
// RUN COMMAND
// =============================================================================

fn cmd_run(file: &PathBuf, args: &[String]) -> Result<(), i32> {
    // Read source file
    let source = std::fs::read_to_string(file).map_err(|e| {
        eprintln!("Error reading file '{}': {}", file.display(), e);
        1
    })?;

    let source_file = SourceFile::new(file.to_string_lossy(), source);

    // Tokenize
    let mut lexer = Lexer::new(&source_file);
    let tokens = lexer.tokenize().map_err(|e| {
        eprintln!("Lexer error: {}", e);
        1
    })?;

    // Parse
    let mut parser = Parser::new(&source_file, tokens);
    let mut ast = parser.parse().map_err(|e| {
        eprintln!("Parse error: {}", e);
        for err in parser.errors() {
            eprintln!("  {}", err);
        }
        1
    })?;

    // Resolve `mod foo;` declarations — load and merge external module files
    let source_dir = file.parent().unwrap_or(Path::new("."));
    resolve_modules(&mut ast, source_dir)?;

    // Type check
    let mut ctx = TypeContext::new();
    let mut checker = TypeChecker::new(&mut ctx);
    checker.check_module(&ast);

    if checker.has_errors() {
        for err in checker.errors() {
            eprintln!("Type error: {}", err);
        }
        return Err(1);
    }

    // Generate C code
    let mut codegen = CodeGenerator::new(&ctx, Target::C);
    let output = codegen.generate(&ast).map_err(|e| {
        eprintln!("Code generation error: {}", e);
        1
    })?;

    // Write to temp file
    let temp_dir = std::env::temp_dir().join("quantalang");
    std::fs::create_dir_all(&temp_dir).map_err(|e| {
        eprintln!("Failed to create temp directory: {}", e);
        1
    })?;

    let c_file = temp_dir.join("temp.c");
    let exe_file = if cfg!(windows) {
        temp_dir.join("temp.exe")
    } else {
        temp_dir.join("temp")
    };

    std::fs::write(&c_file, &output.data).map_err(|e| {
        eprintln!("Failed to write temp file: {}", e);
        1
    })?;

    // Find and invoke C compiler
    let compiler = find_c_compiler().ok_or_else(|| {
        eprintln!("Error: No C compiler found on the system.");
        eprintln!("QuantaLang needs a C compiler to compile and run programs.");
        eprintln!();
        if cfg!(windows) {
            eprintln!("Install one of: cl.exe (MSVC), gcc (MinGW), or clang");
        } else {
            eprintln!("Install one of: cc, gcc, or clang");
        }
        1
    })?;

    invoke_c_compiler(&compiler, &c_file, &exe_file, false)?;

    // Run the compiled program
    let mut cmd = std::process::Command::new(&exe_file);
    cmd.args(args);

    let status = cmd.status().map_err(|e| {
        eprintln!("Failed to run program: {}", e);
        1
    })?;

    // Clean up temp files
    let _ = std::fs::remove_file(&c_file);
    let _ = std::fs::remove_file(&exe_file);

    if status.success() {
        Ok(())
    } else {
        Err(status.code().unwrap_or(1))
    }
}

fn cmd_repl() -> Result<(), i32> {
    println!("QuantaLang REPL v{}", quantalang::VERSION);
    println!("Type :help for help, :quit to exit");
    println!();

    let mut ctx = TypeContext::new();
    let mut history: Vec<String> = Vec::new();

    loop {
        use std::io::{self, Write};

        print!(">>> ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_err() {
            break;
        }

        let input = input.trim();
        if input.is_empty() {
            continue;
        }

        history.push(input.to_string());

        if input.starts_with(':') {
            match input {
                ":quit" | ":q" | ":exit" => break,
                ":help" | ":h" => {
                    println!("Commands:");
                    println!("  :quit, :q      - Exit the REPL");
                    println!("  :help, :h      - Show this help");
                    println!("  :tokens <expr> - Show tokens for expression");
                    println!("  :ast <expr>    - Show AST for expression");
                    println!("  :type <expr>   - Show type of expression");
                    println!("  :history       - Show command history");
                    println!("  :clear         - Clear the screen");
                    println!();
                    println!("Or enter QuantaLang code to parse and analyze.");
                }
                ":history" => {
                    for (i, cmd) in history.iter().enumerate() {
                        println!("{:4}: {}", i + 1, cmd);
                    }
                }
                ":clear" => {
                    print!("\x1B[2J\x1B[1;1H");
                    io::stdout().flush().unwrap();
                }
                cmd if cmd.starts_with(":tokens ") => {
                    let expr = &cmd[8..];
                    let file = SourceFile::anonymous(expr);
                    let mut lexer = Lexer::new(&file);
                    match lexer.tokenize() {
                        Ok(tokens) => {
                            for token in tokens {
                                if !token.is_eof() {
                                    println!("  {:?}", token);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Error: {}", e);
                        }
                    }
                }
                cmd if cmd.starts_with(":ast ") => {
                    let expr = &cmd[5..];
                    // Wrap in a function to make it parseable
                    let wrapped = format!("fn __repl__() {{ {} }}", expr);
                    let file = SourceFile::anonymous(wrapped.clone());
                    let mut lexer = Lexer::new(&file);
                    match lexer.tokenize() {
                        Ok(tokens) => {
                            let mut parser = Parser::new(&file, tokens);
                            match parser.parse() {
                                Ok(ast) => {
                                    println!("AST:");
                                    for item in &ast.items {
                                        println!("  {:?}", item);
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Parse error: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Lexer error: {}", e);
                        }
                    }
                }
                cmd if cmd.starts_with(":type ") => {
                    let expr = &cmd[6..];
                    let wrapped = format!("fn __repl__() {{ {} }}", expr);
                    let file = SourceFile::anonymous(wrapped.clone());
                    let mut lexer = Lexer::new(&file);
                    match lexer.tokenize() {
                        Ok(tokens) => {
                            let mut parser = Parser::new(&file, tokens);
                            match parser.parse() {
                                Ok(ast) => {
                                    let mut checker = TypeChecker::new(&mut ctx);
                                    checker.check_module(&ast);
                                    if checker.has_errors() {
                                        for err in checker.errors() {
                                            eprintln!("Type error: {}", err);
                                        }
                                    } else {
                                        println!("Type check passed!");
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Parse error: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Lexer error: {}", e);
                        }
                    }
                }
                _ => {
                    eprintln!("Unknown command: {}", input);
                    eprintln!("Type :help for available commands");
                }
            }
            continue;
        }

        // Parse as a module item or expression
        let file = SourceFile::anonymous(input);
        let mut lexer = Lexer::new(&file);

        match lexer.tokenize() {
            Ok(tokens) => {
                println!("Tokens: {}", tokens.len());

                // Try to parse
                let mut parser = Parser::new(&file, tokens.clone());
                match parser.parse() {
                    Ok(ast) => {
                        println!("Parsed {} item(s)", ast.items.len());
                        for item in &ast.items {
                            println!("  - {}", item_kind_name(&item.kind));
                        }

                        // Type check
                        let mut checker = TypeChecker::new(&mut ctx);
                        checker.check_module(&ast);
                        if checker.has_errors() {
                            println!("Type errors:");
                            for err in checker.errors() {
                                println!("  {}", err);
                            }
                        } else {
                            println!("Type check: OK");
                        }
                    }
                    Err(e) => {
                        // Show tokens on parse failure
                        println!("Tokens:");
                        for token in &tokens {
                            if !token.is_eof() {
                                print!("{} ", token.kind);
                            }
                        }
                        println!();
                        eprintln!("Parse error: {}", e);
                    }
                }
            }
            Err(e) => {
                eprintln!("Lexer error: {}", e);
            }
        }
    }

    println!("\nGoodbye!");
    Ok(())
}

// =============================================================================
// LSP COMMAND
// =============================================================================

fn cmd_lsp() -> Result<(), i32> {
    eprintln!("QuantaLang LSP server v{} starting on stdio...", quantalang::VERSION);

    match quantalang::lsp::run_server() {
        Ok(()) => {
            eprintln!("LSP server shut down cleanly.");
            Ok(())
        }
        Err(e) => {
            eprintln!("LSP server error: {}", e);
            Err(1)
        }
    }
}

fn cmd_fmt(file: &PathBuf, check: bool, write: bool) -> Result<(), i32> {
    let source = std::fs::read_to_string(file).map_err(|e| {
        eprintln!("Error reading '{}': {}", file.display(), e);
        1
    })?;

    let formatter = quantalang::fmt::Formatter::default_formatter();
    let formatted = formatter.format_str(&source).map_err(|e| {
        eprintln!("Format error: {}", e);
        1
    })?;

    if check {
        if source != formatted {
            eprintln!("{} would be reformatted", file.display());
            return Err(1);
        }
        println!("{}: OK", file.display());
        return Ok(());
    }

    if write {
        std::fs::write(file, &formatted).map_err(|e| {
            eprintln!("Error writing '{}': {}", file.display(), e);
            1
        })?;
        println!("Formatted {}", file.display());
    } else {
        print!("{}", formatted);
    }
    Ok(())
}

// =============================================================================
// LOCAL PACKAGE REGISTRY
// =============================================================================

/// An entry in the local registry index (registry/index.json).
#[derive(Debug, serde::Deserialize)]
struct LocalRegistryEntry {
    version: String,
    description: String,
    #[allow(dead_code)]
    author: String,
    #[allow(dead_code)]
    checksum: String,
    #[allow(dead_code)]
    path: String,
}

/// Top-level shape of registry/index.json.
#[derive(Debug, serde::Deserialize)]
struct LocalRegistryIndex {
    packages: HashMap<String, LocalRegistryEntry>,
}

/// Load the local file-based package registry.
///
/// Searches for `registry/index.json` relative to the compiler executable, then
/// falls back to the compile-time `CARGO_MANIFEST_DIR` path (good for `cargo run`).
fn load_local_registry_index() -> HashMap<String, LocalRegistryEntry> {
    // Try relative to the running executable first
    let candidates: Vec<std::path::PathBuf> = vec![
        // Works when invoked via `cargo run` from compiler/
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .join("registry")
            .join("index.json"),
        // Works for an installed binary next to a registry/ sibling
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("../registry/index.json")))
            .unwrap_or_default(),
    ];

    for path in &candidates {
        if let Ok(data) = std::fs::read_to_string(path) {
            if let Ok(index) = serde_json::from_str::<LocalRegistryIndex>(&data) {
                return index.packages;
            }
        }
    }
    HashMap::new()
}

fn cmd_pkg(cmd: PkgCommands) -> Result<(), i32> {
    match cmd {
        PkgCommands::Init { path } => {
            let manifest_path = path.join("Quanta.toml");
            if manifest_path.exists() {
                eprintln!("Quanta.toml already exists in {}", path.display());
                return Err(1);
            }
            let dir_name = path.canonicalize().ok()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                .unwrap_or_else(|| "my-project".to_string());
            let manifest = format!(
                "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2026\"\n\n[dependencies]\n",
                dir_name
            );
            std::fs::write(&manifest_path, &manifest).map_err(|e| {
                eprintln!("Error creating Quanta.toml: {}", e);
                1
            })?;
            println!("Created {}", manifest_path.display());
            Ok(())
        }
        PkgCommands::Add { name, version } => {
            let manifest_path = Path::new("Quanta.toml");
            if !manifest_path.exists() {
                eprintln!("No Quanta.toml found. Run `quantac pkg init` first.");
                return Err(1);
            }
            let mut content = std::fs::read_to_string(manifest_path).map_err(|e| {
                eprintln!("Error reading Quanta.toml: {}", e);
                1
            })?;
            let ver = version.unwrap_or_else(|| "*".to_string());
            content.push_str(&format!("{} = \"{}\"\n", name, ver));
            std::fs::write(manifest_path, &content).map_err(|e| {
                eprintln!("Error writing Quanta.toml: {}", e);
                1
            })?;
            println!("Added {} = \"{}\"", name, ver);
            Ok(())
        }
        PkgCommands::Resolve { path } => {
            let manifest_path = path.join("Quanta.toml");
            if !manifest_path.exists() {
                eprintln!("No Quanta.toml found in {}", path.display());
                return Err(1);
            }
            println!("Resolving dependencies from {}...", manifest_path.display());
            let content = std::fs::read_to_string(&manifest_path).map_err(|e| {
                eprintln!("Error reading manifest: {}", e);
                1
            })?;
            println!("Manifest loaded ({} bytes)", content.len());

            // Check dependencies against the local registry
            let index = load_local_registry_index();
            // Parse [dependencies] lines from the manifest
            let mut in_deps = false;
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed == "[dependencies]" {
                    in_deps = true;
                    continue;
                }
                if trimmed.starts_with('[') {
                    in_deps = false;
                    continue;
                }
                if in_deps {
                    if let Some((name, _ver)) = trimmed.split_once('=') {
                        let dep_name = name.trim();
                        if dep_name.is_empty() { continue; }
                        if let Some(entry) = index.get(dep_name) {
                            println!("  {} = {} ... found ({})", dep_name, entry.version, entry.description);
                        } else {
                            println!("  {} ... NOT FOUND in local registry", dep_name);
                        }
                    }
                }
            }
            println!("Resolution complete.");
            Ok(())
        }
        PkgCommands::Search { query } => {
            let index = load_local_registry_index();
            let query_lower = query.to_lowercase();
            let mut found = 0u32;

            println!("Searching local registry for '{}'...", query);
            for (name, entry) in &index {
                if name.to_lowercase().contains(&query_lower)
                    || entry.description.to_lowercase().contains(&query_lower)
                {
                    println!("  {} v{} - {}", name, entry.version, entry.description);
                    found += 1;
                }
            }

            if found == 0 {
                println!("No packages found matching '{}'.", query);
            } else {
                println!("{} package(s) found.", found);
            }
            Ok(())
        }
    }
}

// =============================================================================
// MODULE RESOLUTION
// =============================================================================

/// Resolve `mod foo;` declarations by loading and parsing external module files.
///
/// For each `mod foo;` (a mod declaration with no body), this function:
/// 1. Looks for `foo.quanta` in the same directory, or `foo/mod.quanta`
/// 2. Parses that file
/// 3. Recursively resolves sub-module declarations
/// 4. Collects all item names defined in the module
/// 5. Prefixes each definition with `foo_` (functions, structs, enums)
/// 6. Renames intra-module references in function bodies
/// 7. Appends the prefixed items into the main AST
///
/// Multi-segment paths like `foo::bar::baz()` resolve to `foo_bar_baz`
/// during lowering since lower_path joins segments with `_`.
fn resolve_modules(ast: &mut Module, source_dir: &Path) -> Result<(), i32> {
    resolve_modules_with_prefix(ast, source_dir, "")
}

/// Resolve modules with a prefix for nested module support.
/// The prefix is prepended to all mangled names (e.g., "utils_" for sub-modules of utils).
fn resolve_modules_with_prefix(ast: &mut Module, source_dir: &Path, prefix: &str) -> Result<(), i32> {
    // Collect module names from `mod foo;` declarations (content == None).
    let mod_names: Vec<String> = ast.items.iter().filter_map(|item| {
        if let ItemKind::Mod(ref m) = item.kind {
            if m.content.is_none() {
                return Some(m.name.name.to_string());
            }
        }
        None
    }).collect();

    if mod_names.is_empty() {
        return Ok(());
    }

    let mut new_items: Vec<ast::Item> = Vec::new();

    for mod_name in &mod_names {
        // Look for foo.quanta or foo/mod.quanta
        let mod_file = source_dir.join(format!("{}.quanta", mod_name));
        let mod_dir_file = source_dir.join(mod_name).join("mod.quanta");

        let (actual_file, sub_source_dir) = if mod_file.exists() {
            (mod_file, source_dir.to_path_buf())
        } else if mod_dir_file.exists() {
            (mod_dir_file, source_dir.join(mod_name))
        } else {
            eprintln!(
                "Error: module '{}' declared but neither '{}' nor '{}' found",
                mod_name,
                source_dir.join(format!("{}.quanta", mod_name)).display(),
                source_dir.join(mod_name).join("mod.quanta").display(),
            );
            return Err(1);
        };

        // Read and parse the module file
        let mod_source = std::fs::read_to_string(&actual_file).map_err(|e| {
            eprintln!("Error reading module file '{}': {}", actual_file.display(), e);
            1
        })?;

        let mod_source_file = SourceFile::new(actual_file.to_string_lossy(), mod_source);
        let mut mod_lexer = Lexer::new(&mod_source_file);
        let mod_tokens = mod_lexer.tokenize().map_err(|e| {
            eprintln!("Lexer error in module '{}': {}", mod_name, e);
            1
        })?;

        let mut mod_parser = Parser::new(&mod_source_file, mod_tokens);
        let mut mod_ast = mod_parser.parse().map_err(|e| {
            eprintln!("Parse error in module '{}': {}", mod_name, e);
            for err in mod_parser.errors() {
                eprintln!("  {}", err);
            }
            1
        })?;

        // The full prefix for this module's items
        let full_prefix = if prefix.is_empty() {
            mod_name.clone()
        } else {
            format!("{}_{}", prefix, mod_name)
        };

        // Recursively resolve sub-modules within this module
        resolve_modules_with_prefix(&mut mod_ast, &sub_source_dir, &full_prefix)?;

        // Pass 1: Collect all item names defined in this module (functions, structs, enums).
        let module_fns: HashSet<String> = mod_ast.items.iter().filter_map(|item| {
            match &item.kind {
                ItemKind::Function(ref f) => Some(f.name.name.to_string()),
                _ => None,
            }
        }).collect();

        let module_types: HashSet<String> = mod_ast.items.iter().filter_map(|item| {
            match &item.kind {
                ItemKind::Struct(ref s) => Some(s.name.name.to_string()),
                ItemKind::Enum(ref e) => Some(e.name.name.to_string()),
                _ => None,
            }
        }).collect();

        // Pass 2: Prefix each definition and rename intra-module references.
        for item in mod_ast.items {
            match item.kind {
                ItemKind::Function(f) => {
                    let mut prefixed_fn = *f;
                    let mangled_name = format!("{}_{}", full_prefix, prefixed_fn.name.name);
                    prefixed_fn.name = Ident::dummy(mangled_name);

                    // Walk the function body and rename calls to same-module functions.
                    if let Some(ref mut body) = prefixed_fn.body {
                        rename_calls_in_block(body, &full_prefix, &module_fns);
                    }

                    new_items.push(ast::Item::new(
                        ItemKind::Function(Box::new(prefixed_fn)),
                        Visibility::default(),
                        Vec::new(),
                        Span::dummy(),
                    ));
                }
                ItemKind::Struct(_) | ItemKind::Enum(_) | ItemKind::Impl(_) => {
                    // Types (structs, enums) and impl blocks are NOT prefixed —
                    // they are referenced by their original name in user code.
                    // Only functions are prefixed with the module path.
                    new_items.push(item);
                }
                _ => {
                    new_items.push(item);
                }
            }
        }
    }

    // Append module items to the main AST
    ast.items.extend(new_items);

    Ok(())
}

/// Recursively walk a block and rename any calls to intra-module functions.
fn rename_calls_in_block(block: &mut Block, prefix: &str, module_fns: &HashSet<String>) {
    for stmt in &mut block.stmts {
        rename_calls_in_stmt(stmt, prefix, module_fns);
    }
}

/// Rename intra-module calls inside a statement.
fn rename_calls_in_stmt(stmt: &mut ast::Stmt, prefix: &str, module_fns: &HashSet<String>) {
    match &mut stmt.kind {
        StmtKind::Expr(ref mut expr) | StmtKind::Semi(ref mut expr) => {
            rename_calls_in_expr(expr, prefix, module_fns);
        }
        StmtKind::Local(ref mut local) => {
            if let Some(ref mut init) = local.init {
                rename_calls_in_expr(&mut init.expr, prefix, module_fns);
                if let Some(ref mut diverge) = init.diverge {
                    rename_calls_in_expr(diverge, prefix, module_fns);
                }
            }
        }
        StmtKind::Item(ref mut item) => {
            // Handle nested function definitions inside function bodies.
            if let ItemKind::Function(ref mut f) = item.kind {
                if let Some(ref mut body) = f.body {
                    rename_calls_in_block(body, prefix, module_fns);
                }
            }
        }
        StmtKind::Empty | StmtKind::Macro { .. } => {}
    }
}

/// Recursively rename intra-module function references inside an expression.
///
/// If the expression is a call whose callee is an identifier matching a
/// function defined in the same module, prefix it with `mod_name_`.
/// Recurses into all sub-expressions.
fn rename_calls_in_expr(expr: &mut Expr, prefix: &str, module_fns: &HashSet<String>) {
    match &mut expr.kind {
        // An identifier that matches a module function -- prefix it.
        ExprKind::Ident(ref mut ident) => {
            if module_fns.contains(ident.name.as_ref()) {
                let new_name = format!("{}_{}", prefix, ident.name);
                *ident = Ident::dummy(new_name);
            }
        }

        // A single-segment path that matches a module function -- prefix it.
        ExprKind::Path(ref mut path) => {
            if path.is_simple() {
                if let Some(ident) = path.last_ident() {
                    if module_fns.contains(ident.name.as_ref()) {
                        let new_name = format!("{}_{}", prefix, ident.name);
                        path.segments[0].ident = Ident::dummy(new_name);
                    }
                }
            }
        }

        // Function call -- rename callee then recurse into args.
        ExprKind::Call { ref mut func, ref mut args } => {
            rename_calls_in_expr(func, prefix, module_fns);
            for arg in args.iter_mut() {
                rename_calls_in_expr(arg, prefix, module_fns);
            }
        }

        // Method call -- recurse into receiver and args.
        ExprKind::MethodCall { ref mut receiver, ref mut args, .. } => {
            rename_calls_in_expr(receiver, prefix, module_fns);
            for arg in args.iter_mut() {
                rename_calls_in_expr(arg, prefix, module_fns);
            }
        }

        // Binary and unary operations.
        ExprKind::Binary { ref mut left, ref mut right, .. } => {
            rename_calls_in_expr(left, prefix, module_fns);
            rename_calls_in_expr(right, prefix, module_fns);
        }
        ExprKind::Unary { ref mut expr, .. } => {
            rename_calls_in_expr(expr, prefix, module_fns);
        }
        ExprKind::Assign { ref mut target, ref mut value, .. } => {
            rename_calls_in_expr(target, prefix, module_fns);
            rename_calls_in_expr(value, prefix, module_fns);
        }

        // Control flow.
        ExprKind::If { ref mut condition, ref mut then_branch, ref mut else_branch } => {
            rename_calls_in_expr(condition, prefix, module_fns);
            rename_calls_in_block(then_branch, prefix, module_fns);
            if let Some(ref mut else_expr) = else_branch {
                rename_calls_in_expr(else_expr, prefix, module_fns);
            }
        }
        ExprKind::Match { ref mut scrutinee, ref mut arms } => {
            rename_calls_in_expr(scrutinee, prefix, module_fns);
            for arm in arms.iter_mut() {
                rename_calls_in_expr(&mut arm.body, prefix, module_fns);
                if let Some(ref mut guard) = arm.guard {
                    rename_calls_in_expr(guard, prefix, module_fns);
                }
            }
        }
        ExprKind::Loop { ref mut body, .. } => {
            rename_calls_in_block(body, prefix, module_fns);
        }
        ExprKind::While { ref mut condition, ref mut body, .. } => {
            rename_calls_in_expr(condition, prefix, module_fns);
            rename_calls_in_block(body, prefix, module_fns);
        }
        ExprKind::WhileLet { ref mut expr, ref mut body, .. } => {
            rename_calls_in_expr(expr, prefix, module_fns);
            rename_calls_in_block(body, prefix, module_fns);
        }
        ExprKind::For { ref mut iter, ref mut body, .. } => {
            rename_calls_in_expr(iter, prefix, module_fns);
            rename_calls_in_block(body, prefix, module_fns);
        }

        // Blocks and wrappers.
        ExprKind::Block(ref mut block) | ExprKind::Unsafe(ref mut block) => {
            rename_calls_in_block(block, prefix, module_fns);
        }
        ExprKind::Async { ref mut body, .. } => {
            rename_calls_in_block(body, prefix, module_fns);
        }
        ExprKind::Paren(ref mut inner) => {
            rename_calls_in_expr(inner, prefix, module_fns);
        }

        // Access expressions.
        ExprKind::Field { ref mut expr, .. } | ExprKind::TupleField { ref mut expr, .. } => {
            rename_calls_in_expr(expr, prefix, module_fns);
        }
        ExprKind::Index { ref mut expr, ref mut index } => {
            rename_calls_in_expr(expr, prefix, module_fns);
            rename_calls_in_expr(index, prefix, module_fns);
        }
        ExprKind::Deref(ref mut inner) | ExprKind::Ref { expr: ref mut inner, .. } => {
            rename_calls_in_expr(inner, prefix, module_fns);
        }

        // Return / break / continue.
        ExprKind::Return(ref mut val) => {
            if let Some(ref mut v) = val {
                rename_calls_in_expr(v, prefix, module_fns);
            }
        }
        ExprKind::Break { ref mut value, .. } => {
            if let Some(ref mut v) = value {
                rename_calls_in_expr(v, prefix, module_fns);
            }
        }

        // Cast and type ascription.
        ExprKind::Cast { ref mut expr, .. } | ExprKind::TypeAscription { ref mut expr, .. } => {
            rename_calls_in_expr(expr, prefix, module_fns);
        }

        // Closures.
        ExprKind::Closure { ref mut body, .. } => {
            rename_calls_in_expr(body, prefix, module_fns);
        }

        // Try and await.
        ExprKind::Try(ref mut inner) | ExprKind::Await(ref mut inner) => {
            rename_calls_in_expr(inner, prefix, module_fns);
        }

        // Compound expressions.
        ExprKind::Array(ref mut elems) | ExprKind::Tuple(ref mut elems) => {
            for elem in elems.iter_mut() {
                rename_calls_in_expr(elem, prefix, module_fns);
            }
        }
        ExprKind::ArrayRepeat { ref mut element, ref mut count } => {
            rename_calls_in_expr(element, prefix, module_fns);
            rename_calls_in_expr(count, prefix, module_fns);
        }
        ExprKind::Struct { ref mut fields, ref mut rest, .. } => {
            for field in fields.iter_mut() {
                if let Some(ref mut val) = field.value {
                    rename_calls_in_expr(val, prefix, module_fns);
                }
            }
            if let Some(ref mut r) = rest {
                rename_calls_in_expr(r, prefix, module_fns);
            }
        }
        ExprKind::Range { ref mut start, ref mut end, .. } => {
            if let Some(ref mut s) = start {
                rename_calls_in_expr(s, prefix, module_fns);
            }
            if let Some(ref mut e) = end {
                rename_calls_in_expr(e, prefix, module_fns);
            }
        }

        // Effect handling.
        ExprKind::Handle { ref mut handlers, ref mut body, .. } => {
            rename_calls_in_block(body, prefix, module_fns);
            for handler in handlers.iter_mut() {
                rename_calls_in_expr(&mut handler.body, prefix, module_fns);
            }
        }
        ExprKind::Resume(ref mut val) => {
            if let Some(ref mut v) = val {
                rename_calls_in_expr(v, prefix, module_fns);
            }
        }
        ExprKind::Perform { ref mut args, .. } => {
            for arg in args.iter_mut() {
                rename_calls_in_expr(arg, prefix, module_fns);
            }
        }

        // Leaf nodes that need no recursion.
        ExprKind::Literal(_)
        | ExprKind::Continue { .. }
        | ExprKind::Error
        | ExprKind::Macro { .. }
        | ExprKind::AIQuery { .. }
        | ExprKind::AIInfer { .. } => {}
    }
}

fn cmd_compile(
    input: &PathBuf,
    output: Option<&std::path::Path>,
    opt_level: u8,
    debug: bool,
    target_override: Option<&str>,
) -> Result<(), i32> {
    // Read source file
    let source = std::fs::read_to_string(input).map_err(|e| {
        eprintln!("Error reading file '{}': {}", input.display(), e);
        1
    })?;

    let source_file = SourceFile::new(input.to_string_lossy(), source);

    // Tokenize
    let mut lexer = Lexer::new(&source_file);
    let tokens = lexer.tokenize().map_err(|e| {
        eprintln!("Lexer error: {}", e);
        1
    })?;

    // Parse
    let mut parser = Parser::new(&source_file, tokens);
    let mut ast = parser.parse().map_err(|e| {
        eprintln!("Parse error: {}", e);
        for err in parser.errors() {
            eprintln!("  {}", err);
        }
        1
    })?;

    // Resolve `mod foo;` declarations — load and merge external module files
    let source_dir = input.parent().unwrap_or(Path::new("."));
    resolve_modules(&mut ast, source_dir)?;

    // Type check
    let mut ctx = TypeContext::new();
    let mut checker = TypeChecker::new(&mut ctx);
    checker.check_module(&ast);

    if checker.has_errors() {
        for err in checker.errors() {
            // Show error with source location: file:line:col
            let line = source_file.lookup_line(err.span.start);
            let line_start = source_file.line_start(line).unwrap_or(err.span.start);
            let col = err.span.start.0.saturating_sub(line_start.0) as usize;
            eprintln!("error[{}:{}:{}]: {}", input.display(), line + 1, col + 1, err.error);

            // Show the source line with an underline
            if let Some(src_line) = source_file.source().lines().nth(line) {
                eprintln!("  {} | {}", line + 1, src_line);
                let padding = format!("{}", line + 1).len();
                let underline_pos = col;
                let underline_len = (err.span.end.0.saturating_sub(err.span.start.0) as usize).max(1);
                eprintln!("  {} | {}{}",
                    " ".repeat(padding),
                    " ".repeat(underline_pos),
                    "^".repeat(underline_len.min(src_line.len().saturating_sub(underline_pos))));
            }

            if let Some(help) = &err.help {
                eprintln!("  help: {}", help);
            }
            for note in &err.notes {
                eprintln!("  note: {}", note);
            }
        }
        return Err(1);
    }

    // Select target: explicit --target flag > output extension > default (C)
    let target = if let Some(t) = target_override {
        match t {
            "c" => Target::C,
            "llvm" | "ll" => Target::LlvmIr,
            "wasm" | "wat" => Target::Wasm,
            "spirv" | "spir-v" | "spv" => Target::SpirV,
            "x86-64" | "x86_64" | "x64" => Target::X86_64,
            "arm64" | "aarch64" => Target::Arm64,
            "hlsl" | "dx" | "directx" => Target::Hlsl,
            "glsl" | "opengl" | "gl" => Target::Glsl,
            other => {
                eprintln!("Unknown target '{}'. Supported: c, llvm, wasm, spirv, hlsl, glsl, x86-64, arm64", other);
                return Err(1);
            }
        }
    } else if let Some(ext) = output.and_then(|p| p.extension()).and_then(|e| e.to_str()) {
        match ext {
            "ll" => Target::LlvmIr,
            "spv" => Target::SpirV,
            "wasm" | "wat" => Target::Wasm,
            "s" | "asm" => Target::X86_64,
            "hlsl" | "fx" => Target::Hlsl,
            _ => Target::C,
        }
    } else {
        Target::C
    };

    // Determine output path using target's default extension
    let output_path = output
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| input.with_extension(target.extension()));

    // Code generation (pass source for macro expansion)
    let mut codegen = CodeGenerator::with_source(&ctx, target, source_file.source().into());
    // Enable ReShade boilerplate for .fx output files
    if output_path.extension().and_then(|e| e.to_str()) == Some("fx") {
        codegen.reshade = true;
    }
    let generated = codegen.generate(&ast).map_err(|e| {
        eprintln!("Code generation error: {}", e);
        1
    })?;

    // Write output
    std::fs::write(&output_path, &generated.data).map_err(|e| {
        eprintln!("Failed to write output: {}", e);
        1
    })?;

    println!("Compiled {} -> {}", input.display(), output_path.display());

    if debug {
        println!("Debug info: enabled");
    }
    if opt_level > 0 {
        println!("Optimization level: O{}", opt_level);
    }

    // For LLVM target, try to compile the .ll file to a native executable
    if target == Target::LlvmIr {
        let exe_ext = if cfg!(windows) { "exe" } else { "" };
        let exe_path = if exe_ext.is_empty() {
            input.with_extension("")
        } else {
            input.with_extension(exe_ext)
        };

        // Try clang first
        let clang_ok = std::process::Command::new("clang")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        if clang_ok {
            let mut cmd = std::process::Command::new("clang");
            cmd.arg(&output_path);
            cmd.arg("-o");
            cmd.arg(&exe_path);
            if opt_level > 0 {
                cmd.arg(format!("-O{}", opt_level));
            }
            if debug {
                cmd.arg("-g");
            }
            if !cfg!(windows) {
                cmd.arg("-lm");
            }

            match cmd.output() {
                Ok(result) if result.status.success() => {
                    println!("Linked {} -> {}", output_path.display(), exe_path.display());
                }
                Ok(result) => {
                    let stderr = String::from_utf8_lossy(&result.stderr);
                    eprintln!("clang linking failed: {}", stderr.trim());
                    eprintln!("LLVM IR file is still available at: {}", output_path.display());
                }
                Err(e) => {
                    eprintln!("Failed to invoke clang: {}", e);
                    eprintln!("LLVM IR file is still available at: {}", output_path.display());
                }
            }
        } else {
            println!();
            println!("LLVM IR generated at {}", output_path.display());
            if cfg!(windows) {
                println!("To compile: clang {} -o {}", output_path.display(), exe_path.display());
            } else {
                println!("To compile: clang {} -o {} -lm", output_path.display(), exe_path.display());
            }
        }
    }

    // x86-64: try nasm → ld pipeline for native executable
    if target == Target::X86_64 {
        let obj_path = input.with_extension("o");
        let exe_path = input.with_extension(if cfg!(windows) { "exe" } else { "" });
        let nasm_ok = std::process::Command::new("nasm").arg("--version")
            .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
            .status().map(|s| s.success()).unwrap_or(false);
        if nasm_ok {
            let fmt = if cfg!(windows) { "win64" } else { "elf64" };
            if let Ok(r) = std::process::Command::new("nasm").args(["-f", fmt])
                .arg(&output_path).arg("-o").arg(&obj_path).output() {
                if r.status.success() {
                    println!("Assembled -> {}", obj_path.display());
                    let lr = if cfg!(windows) {
                        std::process::Command::new("link.exe")
                            .args(["/entry:main", "/subsystem:console"])
                            .arg(&obj_path).arg(&format!("/out:{}", exe_path.display())).output()
                    } else {
                        std::process::Command::new("ld")
                            .arg(&obj_path).arg("-o").arg(&exe_path).arg("-lc").output()
                    };
                    if let Ok(r) = lr { if r.status.success() {
                        println!("Linked -> {}", exe_path.display());
                    }}
                }
            }
        } else {
            println!("\nx86-64 assembly at {}. Install nasm to build native.", output_path.display());
        }
    }

    // WASM: detect wasmtime/wasmer and show run instructions
    if target == Target::Wasm {
        let wt = std::process::Command::new("wasmtime").arg("--version")
            .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
            .status().map(|s| s.success()).unwrap_or(false);
        if wt { println!("Run: wasmtime {}", output_path.display()); }
        else { println!("\nWASM at {}. Install wasmtime to run.", output_path.display()); }
    }

    Ok(())
}

/// Watch shader files for changes and recompile automatically.
///
/// Usage:
///   quantac watch shaders/ --target=spirv
///   quantac watch shader.quanta --target=spirv
fn cmd_watch(path: &PathBuf, target_str: &str) -> Result<(), i32> {
    use std::time::{Duration, SystemTime};
    use std::collections::HashMap;

    let target_ext = match target_str {
        "spirv" | "spir-v" | "spv" => "spv",
        "c" => "c",
        "llvm" => "ll",
        other => {
            eprintln!("Unknown target '{}'. Supported: spirv, c, llvm", other);
            return Err(1);
        }
    };

    // Collect .quanta files to watch
    let files_to_watch: Vec<PathBuf> = if path.is_dir() {
        std::fs::read_dir(path).map_err(|e| {
            eprintln!("Failed to read directory '{}': {}", path.display(), e);
            1
        })?.filter_map(|entry| {
            let entry = entry.ok()?;
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) == Some("quanta") {
                Some(p)
            } else {
                None
            }
        }).collect()
    } else if path.extension().and_then(|e| e.to_str()) == Some("quanta") {
        vec![path.clone()]
    } else {
        eprintln!("Expected a .quanta file or directory");
        return Err(1);
    };

    if files_to_watch.is_empty() {
        eprintln!("No .quanta files found in '{}'", path.display());
        return Err(1);
    }

    println!("Watching {} file(s) for changes (target: {})...", files_to_watch.len(), target_str);
    for f in &files_to_watch {
        println!("  {}", f.display());
    }
    println!("Press Ctrl+C to stop.\n");

    // Track modification times
    let mut last_modified: HashMap<PathBuf, SystemTime> = HashMap::new();
    for f in &files_to_watch {
        if let Ok(meta) = std::fs::metadata(f) {
            if let Ok(modified) = meta.modified() {
                last_modified.insert(f.clone(), modified);
            }
        }
    }

    // Initial compilation
    for f in &files_to_watch {
        let output = f.with_extension(target_ext);
        match compile_single_file(f, &output) {
            Ok(()) => println!("[OK] {} -> {}", f.display(), output.display()),
            Err(msg) => eprintln!("[ERR] {}: {}", f.display(), msg),
        }
    }

    // Watch loop
    loop {
        std::thread::sleep(Duration::from_millis(500));

        for f in &files_to_watch {
            let modified = match std::fs::metadata(f) {
                Ok(meta) => meta.modified().ok(),
                Err(_) => continue,
            };

            if let Some(mod_time) = modified {
                let last = last_modified.get(f);
                if last.is_none() || last.unwrap() < &mod_time {
                    last_modified.insert(f.clone(), mod_time);

                    let output = f.with_extension(target_ext);
                    let start = std::time::Instant::now();
                    match compile_single_file(f, &output) {
                        Ok(()) => {
                            let elapsed = start.elapsed();
                            println!("[OK] {} -> {} ({:.1}ms)",
                                f.file_name().unwrap().to_string_lossy(),
                                output.file_name().unwrap().to_string_lossy(),
                                elapsed.as_secs_f64() * 1000.0);

                            // Auto-validate SPIR-V if spirv-val is available
                            if target_ext == "spv" {
                                let spirv_val_paths = [
                                    "C:\\VulkanSDK\\1.4.341.1\\Bin\\spirv-val.exe",
                                    "spirv-val",
                                ];
                                for val_path in &spirv_val_paths {
                                    if let Ok(result) = std::process::Command::new(val_path)
                                        .arg("--target-env").arg("vulkan1.0")
                                        .arg(&output)
                                        .output() {
                                        if result.status.success() {
                                            println!("     spirv-val: PASSED (Vulkan 1.0)");
                                        } else {
                                            let stderr = String::from_utf8_lossy(&result.stderr);
                                            eprintln!("     spirv-val: FAILED\n     {}", stderr.trim());
                                        }
                                        break;
                                    }
                                }
                            }
                        }
                        Err(msg) => eprintln!("[ERR] {}: {}", f.file_name().unwrap().to_string_lossy(), msg),
                    }
                }
            }
        }
    }
}

/// Compile a single .quanta file to the given output path.
fn compile_single_file(input: &Path, output: &Path) -> Result<(), String> {
    let source = std::fs::read_to_string(input)
        .map_err(|e| format!("read error: {}", e))?;

    let source_file = SourceFile::new(input.to_string_lossy(), source);

    let mut lexer = Lexer::new(&source_file);
    let tokens = lexer.tokenize()
        .map_err(|e| format!("lexer error: {}", e))?;

    let mut parser = Parser::new(&source_file, tokens);
    let ast = parser.parse()
        .map_err(|e| format!("parse error: {}", e))?;

    if !parser.errors().is_empty() {
        return Err(format!("parse errors: {}", parser.errors().len()));
    }

    let mut ctx = TypeContext::new();
    let mut checker = TypeChecker::new(&mut ctx);
    checker.check_module(&ast);

    if checker.has_errors() {
        let errs: Vec<String> = checker.errors().iter().map(|e| format!("{}", e)).collect();
        return Err(format!("type errors:\n  {}", errs.join("\n  ")));
    }

    let target = match output.extension().and_then(|e| e.to_str()) {
        Some("ll") => Target::LlvmIr,
        Some("spv") => Target::SpirV,
        _ => Target::C,
    };

    let mut codegen = CodeGenerator::with_source(&ctx, target, source_file.source().into());
    let generated = codegen.generate(&ast)
        .map_err(|e| format!("codegen error: {}", e))?;

    std::fs::write(output, &generated.data)
        .map_err(|e| format!("write error: {}", e))?;

    Ok(())
}
