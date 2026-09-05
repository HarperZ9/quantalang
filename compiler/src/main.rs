// ===============================================================================
// BUILDLANG COMPILER - MAIN ENTRY POINT
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. BuildLang Fair-Source License v1.0 (see LICENSE).
// ===============================================================================

//! BuildLang Compiler (`buildc`)
//!
//! This is the main entry point for the BuildLang compiler command-line tool.

#[cfg(feature = "gpu")]
mod gpu;
// GPU cross-check receipt (Layer C): emit + verify. Verification is pure JSON +
// SHA-256, so it is ALWAYS compiled (even without the `gpu` feature) so
// `receipt verify` works on a gpu receipt in the default build.
mod gpu_receipt;
mod lsp_dispatch;
mod memory_layout;
mod mir_representation;
mod model_receipt;
mod module_graph;
mod scientific_runtime;
mod symbol_graph;
mod tool_receipt;

use clap::{Parser as ClapParser, Subcommand};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use buildlang::ast::{self, ItemKind, Module, Visibility};
use buildlang::codegen::{CodeGenerator, Target};
use buildlang::lexer::{Lexer, SourceFile, Span};
use buildlang::parser::{ParseError, Parser};
use buildlang::types::{
    capability_effect_names, FunctionEffectSummary, TypeChecker, TypeContext, TypeError,
    TypeErrorWithSpan,
};
use lsp_dispatch::{verify_lsp_dispatch_receipt, LspDispatchReceipt, LSP_DISPATCH_RECEIPT};
use memory_layout::{verify_memory_layout_receipt, MemoryLayoutReceipt, MEMORY_LAYOUT_RECEIPT};
#[allow(unused_imports)]
use mir_representation::{
    verify_mir_representation_receipt, MirRepresentationReceipt, MIR_REPRESENTATION_RECEIPT,
};
use model_receipt::{verify_model_boundary_receipt, MODEL_RECEIPT_SCHEMA};
use module_graph::{verify_module_graph_receipt, ModuleGraphReceipt, MODULE_GRAPH_RECEIPT};
use scientific_runtime::{
    build_receipt_chain, receipt_chain_seal_hex, ReceiptChainManifest, ScientificCorpusManifest,
    RECEIPT_CHAIN_SCHEMA, RECEIPT_CORPUS_SCHEMA,
};
use scientific_runtime::{
    build_scientific_runtime_receipt, build_self_test_cases, column_count_matches_invariant,
    compute_mc_executed, crucible_measurement_from_report, evaluate_scientific_runtime_receipt,
    parse_numeric_series, verify_scientific_runtime_receipt, RederivedFacts, RerunObservation,
    ScientificBudget, ScientificCrossBackend, ScientificDigest, ScientificEffectPolicy,
    ScientificMonteCarlo, ScientificReceiptInputs, ScientificRuntimeReceipt, ScientificToolchain,
    SecondaryObservation, BOUNDED_INVARIANT, CONSERVATION_INVARIANT, CONSERVED_BAND_INVARIANT,
    CROSS_BACKEND_INVARIANT, CRUCIBLE_MEASUREMENT_EXPORT_SCHEMA, ENERGY_IDENTITY_INVARIANT,
    ENERGY_MONOTONE_INVARIANT, MC_EXECUTED_ESTIMATOR_PROPORTION, NON_NEGATIVE_INVARIANT,
    RELATION_INVARIANT, SCIENTIFIC_RUNTIME_SCHEMA,
};
use symbol_graph::{verify_symbol_graph_receipt, SymbolGraphReceipt, SYMBOL_GRAPH_RECEIPT};
use tool_receipt::{verify_tool_call_receipt, TOOL_RECEIPT_SCHEMA};

fn parse_codegen_target(target: &str) -> Result<Target, String> {
    match target {
        "c" => Ok(Target::C),
        "llvm" | "llvm-ir" | "ll" => Ok(Target::LlvmIr),
        "x86-64" | "x86_64" | "x64" => Ok(Target::X86_64),
        "arm64" | "aarch64" => Ok(Target::Arm64),
        "wasm" | "wasm32" | "wat" => Ok(Target::Wasm),
        "spirv" | "spir-v" | "spv" => Ok(Target::SpirV),
        "hlsl" | "dx" | "directx" => Ok(Target::Hlsl),
        "glsl" | "opengl" | "gl" => Ok(Target::Glsl),
        "rust" | "rs" => Ok(Target::Rust),
        other => Err(format!(
            "Unknown target '{}'. Supported: c, llvm, wasm, spirv, hlsl, glsl, rust, x86-64, arm64",
            other
        )),
    }
}

fn target_from_extension(ext: &str) -> Option<Target> {
    match ext {
        "c" => Some(Target::C),
        "ll" => Some(Target::LlvmIr),
        "wasm" | "wat" => Some(Target::Wasm),
        "spv" => Some(Target::SpirV),
        "s" | "asm" => Some(Target::X86_64),
        "hlsl" | "fx" => Some(Target::Hlsl),
        "glsl" => Some(Target::Glsl),
        "rs" => Some(Target::Rust),
        _ => None,
    }
}

/// BuildLang Compiler
#[derive(ClapParser)]
#[command(name = "buildc")]
#[command(author = "Zain Dana Harper")]
#[command(version)]
#[command(about = "The BuildLang compiler - a multi-paradigm systems programming language")]
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

    /// Code generation target (c, llvm, wasm, spirv, rust, x86-64, arm64)
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

        /// Write a machine-readable check receipt to a path, or '-' for stdout
        #[arg(long, value_name = "PATH")]
        receipt: Option<PathBuf>,

        /// Evaluate a machine-readable check policy profile
        #[arg(long, value_name = "PATH", conflicts_with = "profile")]
        policy: Option<PathBuf>,

        /// Evaluate a built-in check policy profile
        #[arg(long, value_name = "NAME", conflicts_with = "policy")]
        profile: Option<String>,

        /// Require the selected built-in profile to match a SHA-256 digest
        #[arg(long, value_name = "HEX")]
        expect_profile_digest: Option<String>,
    },

    /// Build a project
    Build {
        /// Project directory
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Build in release mode
        #[arg(long)]
        release: bool,

        /// Emit type: 'c' for C source only, 'header' for a C export header,
        /// 'exe' for executable (default)
        #[arg(long, default_value = "exe")]
        emit: String,

        /// Keep the intermediate .c file after compilation
        #[arg(long)]
        keep_c: bool,

        /// Code generation target: c, llvm, x86-64, arm64, wasm, spirv, hlsl, glsl, rust
        #[arg(long, default_value = "c")]
        target: String,
    },

    /// Run a file directly
    Run {
        /// Input file
        file: PathBuf,

        /// Emit a sealed scientific-runtime receipt to PATH ('-' = stdout).
        /// When set, buildc captures the program's numeric stdout as a
        /// measurement series, checks the invariant, and writes the receipt.
        /// Without this flag, `run` behavior is byte-identical to before.
        #[arg(long, value_name = "PATH")]
        emit_receipt: Option<PathBuf>,

        /// Invariant to check over the captured series (v0: only
        /// `energy-monotone`). Ignored unless `--emit-receipt` is set.
        #[arg(long, default_value = "energy-monotone")]
        invariant: String,

        /// Measurement metric name recorded in the receipt.
        #[arg(long, default_value = "series")]
        metric: String,

        /// Physical unit of the measured series (e.g. `J`, `m/s`, `kg*m/s^2`,
        /// `1` for dimensionless). Parsed and CANONICALIZED through the
        /// dimensional-analysis core: a malformed or unknown unit is a hard
        /// error, and the receipt records the checked canonical form (so
        /// `m*s^-1` and `m/s` seal identically). Ignored unless
        /// `--emit-receipt` is set.
        #[arg(long, value_name = "UNIT")]
        units: Option<String>,

        /// Columns per row of the captured series (row-major). `1` (default)
        /// for the single-scalar invariants; `>= 2` for `--invariant relation`,
        /// whose verifier compares the columns of each row.
        #[arg(long, default_value = "1")]
        columns: usize,

        /// Free-text problem label recorded in the receipt (e.g.
        /// "1d-heat-equation-energy").
        #[arg(long)]
        problem: Option<String>,

        /// Declare the numerical method (recorded as author-DECLARED; buildc
        /// cannot derive scheme semantics from source).
        #[arg(long, value_name = "DESCRIPTION")]
        method: Option<String>,

        /// Declare this run a negative fixture: an invariant FAIL is EXPECTED
        /// and yields receipt_status FAIL_EXPECTED instead of FAIL_UNEXPECTED.
        #[arg(long)]
        negative_fixture: bool,

        /// Seed for the program's `random_f64()` stream (the Random
        /// capability). buildc sets BUILD_RANDOM_SEED for the program and,
        /// with `--emit-receipt`, seals the seed so `receipt verify` re-runs
        /// the exact stream. A Random-using program REQUIRES this (an
        /// unseeded draw aborts); a program with no Random capability
        /// refuses it (nothing would consume it).
        #[arg(long, value_name = "N")]
        seed: Option<u64>,

        /// Declare the run a Monte Carlo estimate: the estimator's id (e.g.
        /// `mean`). All three `--mc-*` flags declare together or not at all;
        /// a partial declaration is refused (the claim is the interval,
        /// never the point). Sealed into the receipt's `monte_carlo` block.
        #[arg(long, value_name = "ID")]
        mc_estimator: Option<String>,

        /// The MC declaration's sample count n (the denominator). Non-zero.
        #[arg(long, value_name = "N")]
        mc_samples: Option<u64>,

        /// The MC declaration's interval method (e.g. `normal-approx-95`).
        #[arg(long, value_name = "METHOD")]
        mc_interval: Option<String>,

        /// Declare the Monte Carlo run EXECUTED: the verifier re-derives the
        /// interval from raw sufficient-statistic columns the kernel prints
        /// (successes/trials counters beside the invariant scalar) instead of
        /// trusting the declaration. Requires all three --mc-* flags together;
        /// forces --columns to 3 (an unset default is silently upgraded, any
        /// other explicit value is refused, the --cross-backend idiom).
        #[arg(long)]
        mc_executed: bool,

        /// Declare the run a budgeted search: the step ceiling. Both
        /// --budget-* flags declare together or not at all. A budgeted
        /// receipt carries NOT_PROVES_OPTIMALITY and refuses free text
        /// claiming optimality.
        #[arg(long, value_name = "LIMIT")]
        budget_steps: Option<u64>,

        /// The declared steps consumed (at most the ceiling).
        #[arg(long, value_name = "N")]
        budget_consumed: Option<u64>,

        /// The declared wall-clock ceiling in seconds: a member of the
        /// budget declaration, not a freestanding knob. Requires the
        /// --budget-steps/--budget-consumed pair and a positive, finite
        /// value. The `wall_exceeded` flag is DERIVED at emit from this
        /// ceiling against the SEALED measured wall time.
        #[arg(long, value_name = "LIMIT")]
        budget_wall_seconds: Option<f64>,

        /// Run the kernel through a SECOND backend as well and seal a
        /// 2-column cross-backend receipt (the C anchor and the secondary
        /// lane's outputs, checked for agreement). v0 supports `rust` (the
        /// repo's validation lane) only. Requires `--invariant cross-backend`
        /// (and vice versa). Refused with `--gpu`, with `--seed`, with
        /// `--mc-*` (Monte Carlo needs Random, which this refuses anyway),
        /// and on a `Random`-observing kernel (the Rust lane has no seeded
        /// PRNG builtin, so the streams could not agree).
        #[arg(long, value_name = "TARGET")]
        cross_backend: Option<String>,

        /// Execute a `#[compute]` kernel on the physical GPU (Vulkan) and
        /// cross-check the readback against the CPU-C scalar loop within
        /// tolerance. Requires a build with `--features gpu` and a Vulkan device.
        #[arg(long)]
        gpu: bool,

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

    /// Format BuildLang source files
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

    /// Semantic corpus verification and receipt checks
    Corpus {
        #[command(subcommand)]
        command: CorpusCommands,
    },

    /// Built-in check policy profiles for CI and release gates
    Policy {
        #[command(subcommand)]
        command: PolicyCommands,
    },

    /// Verify saved accountability receipts against current source inputs
    Receipt {
        #[command(subcommand)]
        command: ReceiptCommands,
    },

    /// Run tests - compile .bld programs and verify output against .expected files
    Test {
        /// Directory containing test programs [default: tests/programs]
        #[arg(default_value = "tests/programs")]
        directory: PathBuf,

        /// Only run tests matching this substring
        #[arg(short, long)]
        filter: Option<String>,

        /// Show output for passing tests
        #[arg(long)]
        verbose: bool,

        /// Don't stop on first failure
        #[arg(long)]
        no_fail_fast: bool,
    },

    /// Lint BuildLang source files
    Lint {
        /// Input file to lint
        file: PathBuf,
    },

    /// Diagnose local compiler, toolchain, backend, and package readiness
    Doctor,

    /// Mid-level IR (MIR) interlingua: emit and load the versioned JSON form
    Mir {
        #[command(subcommand)]
        command: MirCommands,
    },

    /// Build Data Format (BDF): convert between the canonical JSON projection
    /// and the canonical binary form, or validate a file
    Bdf {
        #[command(subcommand)]
        command: BdfCommands,
    },

    /// Print version information
    Version,
}

#[derive(Subcommand)]
enum BdfCommands {
    /// Encode a canonical-JSON BDF value into the canonical binary form
    Encode {
        /// Input JSON file (the BDF JSON projection)
        input: PathBuf,

        /// Output binary file (defaults to stdout as raw bytes)
        #[arg(short, long, value_name = "FILE")]
        output: Option<PathBuf>,
    },

    /// Decode a canonical-binary BDF value into the canonical JSON projection
    Decode {
        /// Input binary file written by `buildc bdf encode`
        input: PathBuf,

        /// Output JSON file (defaults to stdout)
        #[arg(short, long, value_name = "FILE")]
        output: Option<PathBuf>,
    },

    /// Validate a BDF file (binary `.bdf` or JSON) and print its digest/summary
    Validate {
        /// Input file (auto-detected: binary if it starts with the BDF magic)
        file: PathBuf,
    },

    /// Bridge a `project-telos.flagship-action/v1` JSON envelope into a
    /// canonical-binary BDF message (lossless)
    FromFlagshipAction {
        /// Input flagship-action/v1 JSON file
        input: PathBuf,

        /// Output binary `.bdf` message file (defaults to stdout as raw bytes)
        #[arg(short, long, value_name = "FILE")]
        output: Option<PathBuf>,
    },

    /// Reconstruct a `project-telos.flagship-action/v1` JSON envelope from a
    /// canonical-binary BDF message (lossless)
    ToFlagshipAction {
        /// Input binary `.bdf` message written by `from-flagship-action`
        input: PathBuf,

        /// Output JSON file (defaults to stdout)
        #[arg(short, long, value_name = "FILE")]
        output: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum MirCommands {
    /// Lower a program to MIR and write the versioned `buildlang.mir/v0` JSON
    Emit {
        /// Input BuildLang source file
        file: PathBuf,

        /// Output JSON file (defaults to stdout)
        #[arg(short, long, value_name = "FILE")]
        output: Option<PathBuf>,
    },

    /// Load a `buildlang.mir/v0` JSON file and print its digest and summary
    Load {
        /// Input MIR JSON file written by `buildc mir emit`
        file: PathBuf,
    },
}

#[derive(Subcommand)]
enum PkgCommands {
    /// Initialize a new Build.toml manifest
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

#[derive(Subcommand)]
enum CorpusCommands {
    /// Verify manifest, receipts, and C backend stdout against the semantic corpus
    Verify {
        /// Semantic corpus root directory
        #[arg(long, value_name = "DIR")]
        root: Option<PathBuf>,
        /// Rewrite the C execution receipt after C stdout verification passes
        #[arg(long)]
        write: bool,
    },
}

#[derive(Subcommand)]
enum PolicyCommands {
    /// List built-in check policy profiles
    List {
        /// Emit the built-in policy catalog as machine-readable JSON
        #[arg(long)]
        json: bool,
    },
    /// Print a built-in check policy profile as JSON
    Print {
        /// Built-in profile name
        name: String,
        /// Write the profile to a file instead of stdout
        #[arg(short, long, value_name = "PATH")]
        output: Option<PathBuf>,
    },
    /// Scaffold an exact strict policy from a check receipt
    Scaffold {
        /// Check receipt JSON written by `buildc check --receipt`
        receipt: PathBuf,
        /// Write the scaffolded policy to a file instead of stdout
        #[arg(short, long, value_name = "PATH")]
        output: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum ReceiptCommands {
    /// Verify a buildc check receipt against current source inputs
    Verify {
        /// Check receipt JSON written by `buildc check --receipt`
        receipt: PathBuf,
        /// Source file to verify instead of the source path embedded in the receipt
        #[arg(long, value_name = "PATH")]
        source: Option<PathBuf>,
        /// Require the receipt to have been checked under a built-in policy profile
        #[arg(long, value_name = "NAME")]
        expect_profile: Option<String>,
        /// Require the receipt policy source digest to match a SHA-256 digest
        #[arg(long, value_name = "HEX")]
        expect_policy_digest: Option<String>,
        /// Emit a machine-readable verification report
        #[arg(long)]
        json: bool,
        /// Instead of verifying, prove the verifier can FAIL: tamper each sealed
        /// field of this (scientific-runtime) receipt and assert each tamper is
        /// rejected with its expected failure_class
        #[arg(long)]
        self_test: bool,
    },
    /// Build or verify a receipt chain: an ordered, tamper-evident bundle of
    /// scientific-runtime receipts carrying one re-checkable provenance thread
    Chain {
        #[command(subcommand)]
        command: ChainCommands,
    },
    /// Verify a receipt corpus: emit and re-verify every declared example kernel
    /// and assert each classifies (PASS / FAIL_EXPECTED) exactly as declared
    Corpus {
        /// Corpus manifest JSON (schema `buildlang-scientific-receipt-corpus/v0`)
        manifest: PathBuf,
    },
    /// Export a scientific-runtime receipt as a Crucible-ingestible measurement
    /// (the Telos bridge). The receipt is RE-VERIFIED first and the measurement's
    /// deviation is derived from the fresh re-run, never copied from stored
    /// values; the replay command is sealed in a `recheck` descriptor so the
    /// measurement is witnessed, not asserted.
    Export {
        /// Scientific-runtime receipt JSON written by `buildc run --emit-receipt`
        receipt: PathBuf,
        /// Output path for the measurement JSON (`-` = stdout)
        #[arg(short, long, value_name = "PATH", default_value = "-")]
        output: PathBuf,
        /// Crucible claim id to bind the measurement to (the thesis side owns
        /// claim identity; without it the ingester must bind before assessment)
        #[arg(long, value_name = "ID", default_value = "")]
        claim_id: String,
        /// sha256 of the bound claim's canonical form (same binding note)
        #[arg(long, value_name = "HEX", default_value = "")]
        claim_sha256: String,
        /// The bound claim PREDICTS the invariant failure (valid only for a
        /// negative-fixture receipt): deviation becomes claim-relative, so a
        /// fixture failing as predicted measures 0 and an unexpected pass
        /// measures 1. Without this, Crucible's pure margin math would read
        /// every expected failure as DRIFT (there is no thesis-side reframe).
        #[arg(long)]
        claim_expects_failure: bool,
    },
}

#[derive(Subcommand)]
enum ChainCommands {
    /// Build a chain manifest over an ordered list of scientific-runtime receipts
    Build {
        /// Member receipt files, in chain order (two or more)
        #[arg(required = true, num_args = 1..)]
        receipts: Vec<PathBuf>,
        /// Output path for the chain manifest JSON (`-` = stdout)
        #[arg(short, long, value_name = "PATH", default_value = "-")]
        output: PathBuf,
    },
    /// Verify a chain manifest: re-check the chain seal, pin each member to its
    /// recorded seal, and re-verify each member receipt
    Verify {
        /// Chain manifest JSON written by `buildc receipt chain build`
        manifest: PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    // Type checking and the other AST-recursive passes recurse on the native
    // stack in proportion to a program's nesting and size. A large but finite
    // program -- deep expression trees, or a long function reachable from an
    // entry point, which triggers a second whole-program effect pass -- can
    // exceed the default 8 MB main-thread stack and abort the process. The
    // engine must fail closed with a diagnostic and never abort, so run the
    // whole command on a worker thread with a large stack. This is the same
    // technique rustc uses for the same recursion. A stack this large clears
    // every realistic program; a truly pathological input would still need a
    // recursion-depth guard in the checker to turn a would-be overflow into a
    // diagnostic, which this does not add.
    let result = std::thread::Builder::new()
        .name("buildc".into())
        .stack_size(256 * 1024 * 1024)
        .spawn(move || run_cli(cli))
        .expect("failed to spawn compiler worker thread")
        .join()
        .unwrap_or_else(|_| {
            eprintln!("error: internal compiler error (worker thread panicked)");
            Err(70)
        });

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(code) => ExitCode::from(code as u8),
    }
}

/// Dispatch a parsed CLI invocation to its command handler.
///
/// Split out of `main` so it can run on a large-stack worker thread; see the
/// comment there for why.
fn run_cli(cli: Cli) -> Result<(), i32> {
    match cli.command {
        Some(Commands::Lex { file, verbose }) => cmd_lex(&file, verbose),
        Some(Commands::Parse { file, json }) => cmd_parse(&file, json),
        Some(Commands::Check {
            file,
            receipt,
            policy,
            profile,
            expect_profile_digest,
        }) => cmd_check(
            &file,
            receipt.as_deref(),
            policy.as_deref(),
            profile.as_deref(),
            expect_profile_digest.as_deref(),
        ),
        Some(Commands::Build {
            path,
            release,
            emit,
            keep_c,
            target,
        }) => cmd_build(&path, release, &emit, keep_c, &target),
        Some(Commands::Run {
            file,
            emit_receipt,
            invariant,
            metric,
            units,
            columns,
            problem,
            method,
            negative_fixture,
            seed,
            mc_estimator,
            mc_samples,
            mc_interval,
            mc_executed,
            budget_steps,
            budget_consumed,
            budget_wall_seconds,
            cross_backend,
            gpu,
            args,
        }) => {
            if gpu {
                if seed.is_some() {
                    eprintln!(
                        "--seed is not supported with --gpu (the GPU cross-check has no Random capability)"
                    );
                    Err(1)
                } else if mc_estimator.is_some()
                    || mc_samples.is_some()
                    || mc_interval.is_some()
                    || mc_executed
                {
                    eprintln!(
                        "--mc-* flags are not supported with --gpu (the GPU cross-check has no Random capability)"
                    );
                    Err(1)
                } else if budget_steps.is_some()
                    || budget_consumed.is_some()
                    || budget_wall_seconds.is_some()
                {
                    eprintln!(
                        "--budget-* flags are not supported with --gpu (the GPU cross-check produces no budget block)"
                    );
                    Err(1)
                } else if cross_backend.is_some() {
                    eprintln!(
                        "--cross-backend is not supported with --gpu (the GPU cross-check is a separate secondary lane)"
                    );
                    Err(1)
                } else {
                    cmd_run_gpu(&file, emit_receipt.as_deref())
                }
            } else {
                cmd_run(
                    &file,
                    &args,
                    emit_receipt.as_deref(),
                    &invariant,
                    &metric,
                    units.as_deref(),
                    columns,
                    problem.as_deref(),
                    method.as_deref(),
                    negative_fixture,
                    seed,
                    mc_estimator.as_deref(),
                    mc_samples,
                    mc_interval.as_deref(),
                    mc_executed,
                    budget_steps,
                    budget_consumed,
                    budget_wall_seconds,
                    cross_backend.as_deref(),
                )
            }
        }
        Some(Commands::Repl) => cmd_repl(),
        Some(Commands::Lsp) => cmd_lsp(),
        Some(Commands::Watch { path, target }) => cmd_watch(&path, &target),
        Some(Commands::Fmt { file, check, write }) => cmd_fmt(&file, check, write),
        Some(Commands::Pkg { command }) => cmd_pkg(command),
        Some(Commands::Corpus { command }) => cmd_corpus(command),
        Some(Commands::Policy { command }) => cmd_policy(command),
        Some(Commands::Receipt { command }) => cmd_receipt(command),
        Some(Commands::Lint { file }) => cmd_lint(&file),
        Some(Commands::Doctor) => cmd_doctor(),
        Some(Commands::Mir { command }) => cmd_mir(command),
        Some(Commands::Bdf { command }) => cmd_bdf(command),
        Some(Commands::Test {
            directory,
            filter,
            verbose,
            no_fail_fast,
        }) => cmd_test(&directory, filter.as_deref(), verbose, no_fail_fast),
        Some(Commands::Version) => {
            print_version();
            Ok(())
        }
        None => {
            if let Some(input) = cli.input {
                cmd_compile(
                    &input,
                    cli.output.as_deref(),
                    cli.opt_level,
                    cli.debug,
                    cli.target.as_deref(),
                )
            } else {
                eprintln!("No input file specified. Use --help for usage information.");
                Err(1)
            }
        }
    }
}

fn print_version() {
    println!("BuildLang Compiler (buildc) {}", buildlang::VERSION);
    println!(
        "Language version: {}.{}.{}",
        buildlang::LANGUAGE_VERSION.0,
        buildlang::LANGUAGE_VERSION.1,
        buildlang::LANGUAGE_VERSION.2
    );
    println!("{}", buildlang::COPYRIGHT);
}

fn command_version(command: &str, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new(command)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let text = if output.stdout.is_empty() {
        String::from_utf8_lossy(&output.stderr)
    } else {
        String::from_utf8_lossy(&output.stdout)
    };
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

fn print_tool_probe(label: &str, command: &str, args: &[&str]) {
    match command_version(command, args) {
        Some(version) => println!("  {:<10} found    {}", label, version),
        None => println!("  {:<10} missing  install or add to PATH", label),
    }
}

/// Probe and print the GPU-path readiness row(s) for `buildc doctor`.
///
/// Layer A only needs `spirv-val` (valid compute SPIR-V emission). Layer B
/// (device dispatch) additionally needs the `gpu` cargo feature and a real
/// Vulkan device. The row prints exactly what was probed and never overclaims:
/// without the feature it says the device path is not compiled in even if a GPU
/// is present.
fn print_gpu_probe() {
    println!("GPU path:");

    let spirv_val = find_spirv_val();
    match &spirv_val {
        Some(tool) => println!("  spirv-val    found    {} (Layer A emission gate)", tool),
        None => println!("  spirv-val    missing  install Vulkan SDK for the Layer A gate"),
    }

    let _ = &spirv_val;

    #[cfg(feature = "gpu")]
    {
        match crate::gpu::vulkan_host::probe_device() {
            Some(name) => println!("  gpu      ready    Vulkan compute device: {}", name),
            None => println!(
                "  gpu      absent   `gpu` feature built, but no Vulkan compute device enumerated"
            ),
        }
    }
    #[cfg(not(feature = "gpu"))]
    {
        // vulkaninfo presence is a cheap, dependency-free device signal.
        let vulkaninfo = command_version("vulkaninfo", &["--summary"])
            .or_else(|| command_version("vulkaninfo", &[]));
        match vulkaninfo {
            Some(_) => println!(
                "  gpu      off      Vulkan present, but device dispatch needs `--features gpu` (Layer B)"
            ),
            None => println!(
                "  gpu      off      device dispatch (Layer B) needs `--features gpu` + a Vulkan device"
            ),
        }
    }
}

fn print_substrate_evidence(corpus_root: Option<&Path>) {
    println!();
    println!("Substrate evidence:");
    for row in substrate_evidence_rows(corpus_root) {
        println!("{row}");
    }
}

fn cmd_doctor() -> Result<(), i32> {
    println!("BuildLang Doctor");
    println!("=================");
    println!();
    println!("buildc: {} ({})", buildlang::VERSION, std::env::consts::OS);

    let c_compiler = find_c_compiler();
    match &c_compiler {
        Some(compiler) => println!("C backend: ready via {}", compiler),
        None => println!("C backend: missing C compiler; install MSVC, gcc, clang, or cc"),
    }

    match find_stdlib_path() {
        Some(path) => println!("stdlib: {}", path.display()),
        None => {
            println!("stdlib: not found; set BUILDLANG_STDLIB or install stdlib/ beside buildc")
        }
    }

    let registry = load_local_registry_index();
    if registry.is_empty() {
        println!("registry: no local packages found");
    } else {
        println!("registry: {} local package(s)", registry.len());
    }

    println!();
    println!("Optional tools:");
    print_tool_probe("rustc", "rustc", &["--version"]);
    print_tool_probe("clang", "clang", &["--version"]);
    if cfg!(windows) {
        print_tool_probe("nasm", "nasm", &["--version"]);
    } else {
        print_tool_probe("as", "as", &["--version"]);
    }
    print_tool_probe("wasmtime", "wasmtime", &["--version"]);
    print_tool_probe("spirv-val", "spirv-val", &["--version"]);

    println!();
    println!("Backend maturity:");
    println!("  c        primary       executable C99 path used by buildc run");
    println!("  hlsl     supported     shader source output");
    println!("  glsl     supported     shader source output");
    println!("  rust     experimental  source output with semantic-corpus subset checks");
    println!("  llvm     experimental  LLVM IR; executable path depends on clang");
    println!("  wasm     experimental  WASM/WAT output; runtime depends on wasmtime");
    println!("  spirv    experimental  SPIR-V output; validate with spirv-val");
    println!("  x86-64   experimental  assembly/object output; linker integration is partial");
    println!("  arm64    experimental  assembly/object output; linker integration is partial");

    // GPU row: report exactly what was probed for the real GPU path. Layer A
    // (valid compute SPIR-V) needs only spirv-val; Layer B (device dispatch)
    // additionally needs the `gpu` cargo feature AND a Vulkan device.
    println!();
    print_gpu_probe();

    let corpus_root = find_semantic_corpus_root();
    print_substrate_evidence(corpus_root.as_deref());

    println!();
    if c_compiler.is_some() {
        println!("Ready for practical C-backend examples: yes");
    } else {
        println!("Ready for practical C-backend examples: no");
    }

    Ok(())
}

#[derive(serde::Deserialize)]
struct SemanticCorpusManifest {
    schema: String,
    programs: Vec<SemanticCorpusProgram>,
}

#[derive(Clone, Debug, serde::Serialize)]
struct CheckReceiptSourceDigest {
    algorithm: &'static str,
    hex: String,
}

#[derive(Clone, Debug, serde::Serialize)]
struct CheckReceiptInputDigest {
    role: String,
    source: String,
    digest: CheckReceiptSourceDigest,
}

#[derive(Default)]
struct InputDigestLedger {
    records: BTreeMap<String, CheckReceiptInputDigest>,
    normalize_text: bool,
}

impl InputDigestLedger {
    fn text_normalized() -> Self {
        Self {
            records: BTreeMap::new(),
            normalize_text: true,
        }
    }

    fn record(&mut self, role: &str, path: &Path, bytes: &[u8]) {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let source = canonical.to_string_lossy().to_string();
        let hex = if self.normalize_text {
            source_text_digest_hex(bytes)
        } else {
            source_digest_hex(bytes)
        };
        self.records
            .entry(source.clone())
            .or_insert_with(|| CheckReceiptInputDigest {
                role: role.to_string(),
                source,
                digest: CheckReceiptSourceDigest {
                    algorithm: "sha256",
                    hex,
                },
            });
    }

    fn into_sorted_records(self) -> Vec<CheckReceiptInputDigest> {
        let mut records = self.records.into_values().collect::<Vec<_>>();
        records.sort_by(|left, right| {
            (left.role.as_str(), left.source.as_str())
                .cmp(&(right.role.as_str(), right.source.as_str()))
        });
        records
    }
}

#[derive(serde::Serialize)]
struct CheckReceipt {
    schema: &'static str,
    compiler: &'static str,
    compiler_version: &'static str,
    language_version: String,
    source: String,
    source_digest: CheckReceiptSourceDigest,
    input_graph_digest: CheckReceiptSourceDigest,
    input_digests: Vec<CheckReceiptInputDigest>,
    status: &'static str,
    items: usize,
    tokens: usize,
    declared_effects: BTreeMap<String, Vec<String>>,
    observed_capabilities: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    propagated_effects: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    diagnostics: Vec<CheckReceiptDiagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    policy: Option<CheckReceiptPolicy>,
}

#[derive(serde::Serialize)]
struct CheckReceiptDiagnostic {
    stage: &'static str,
    kind: String,
    message: String,
    /// 1-based line of the diagnostic's start, when the stage resolved it.
    /// Omitted (not `null`) when absent, so a v1 consumer that never read the
    /// field keeps parsing unchanged.
    #[serde(skip_serializing_if = "Option::is_none")]
    line: Option<usize>,
    /// 1-based column of the diagnostic's start. Omitted when absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    col: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    help: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    notes: Vec<String>,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
struct CheckPolicyProfile {
    schema: String,
    #[serde(default)]
    allowed_effects: Vec<String>,
    #[serde(default)]
    denied_effects: Vec<String>,
    #[serde(default)]
    direct_effect_allowlist: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    direct_capability_source_allowlist: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    #[serde(default)]
    propagated_effect_allowlist: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    propagated_effect_source_allowlist: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    #[serde(default)]
    require_source_digest: bool,
    #[serde(default)]
    require_input_graph_digest: bool,
    #[serde(default)]
    require_effect_allowlist: bool,
    #[serde(default)]
    require_provenance_allowlists: bool,
    #[serde(default)]
    require_source_allowlists: bool,
    #[serde(default)]
    require_allowlist_coverage: bool,
}

#[derive(Clone, Debug)]
struct LoadedCheckPolicy {
    source: String,
    source_digest: CheckReceiptSourceDigest,
    builtin_profile: Option<String>,
    builtin_profile_digest: Option<CheckReceiptSourceDigest>,
    profile: CheckPolicyProfile,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct CheckPolicyEvidence {
    function: String,
    effect: String,
    surface: &'static str,
    source: String,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
struct CheckPolicyViolation {
    kind: &'static str,
    effect: String,
    function: String,
    surface: &'static str,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    source: String,
    message: String,
}

impl Ord for CheckPolicyViolation {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (
            self.function.as_str(),
            self.effect.as_str(),
            self.surface,
            self.source.as_str(),
            self.kind,
            self.message.as_str(),
        )
            .cmp(&(
                other.function.as_str(),
                other.effect.as_str(),
                other.surface,
                other.source.as_str(),
                other.kind,
                other.message.as_str(),
            ))
    }
}

impl PartialOrd for CheckPolicyViolation {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Debug)]
struct CheckPolicyDecision {
    schema: String,
    source: String,
    source_digest: CheckReceiptSourceDigest,
    builtin_profile: Option<String>,
    builtin_profile_digest: Option<CheckReceiptSourceDigest>,
    violations: Vec<CheckPolicyViolation>,
}

#[derive(serde::Serialize)]
struct CheckReceiptPolicy {
    schema: String,
    source: String,
    source_digest: CheckReceiptSourceDigest,
    #[serde(skip_serializing_if = "Option::is_none")]
    profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    profile_digest: Option<CheckReceiptSourceDigest>,
    status: &'static str,
    violations: Vec<CheckPolicyViolation>,
}

#[derive(serde::Serialize)]
struct ReceiptVerificationReport {
    schema: &'static str,
    receipt: String,
    source: String,
    status: &'static str,
    checks: Vec<ReceiptVerificationCheck>,
}

#[derive(serde::Serialize)]
struct ReceiptVerificationCheck {
    name: String,
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    expected: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    actual: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

/// A parse diagnostic with its source location resolved once, at check time,
/// while the `SourceFile` is still live. `CheckOutcome` outlives that borrow,
/// so a raw `ParseError` (which only carries a byte `Span`) could not be
/// turned into `line:col` later. Resolving here lets both the human renderer
/// and the receipt report a location, matching the `error[path:line:col]`
/// shape that `build`/`run` already print via `report_parse_errors`.
struct ParseDiagnostic {
    /// The kind message alone (`ParseError::message`), with no path, location,
    /// help, or notes folded in. Help and notes ride their own fields so the
    /// receipt entry matches the shape of a type diagnostic.
    message: String,
    /// 1-based line of the error's start.
    line: usize,
    /// 1-based column of the error's start.
    col: usize,
    /// The full source line, kept for the caret underline. `None` when the
    /// span points past the last line (recovered EOF errors).
    snippet: Option<String>,
    /// Caret length under the start column (at least 1), clamped to the
    /// snippet at render time.
    underline: usize,
    help: Option<String>,
    notes: Vec<String>,
}

struct CheckOutcome {
    source: String,
    compiler_version: &'static str,
    language_version: String,
    source_digest: CheckReceiptSourceDigest,
    input_graph_digest: CheckReceiptSourceDigest,
    input_digests: Vec<CheckReceiptInputDigest>,
    items: usize,
    tokens: usize,
    parse_errors: Vec<ParseDiagnostic>,
    type_errors: Vec<TypeErrorWithSpan>,
    /// 1-based `(line, col)` for each `type_errors` entry, resolved while the
    /// `SourceFile` was live (a type error carries only a byte `Span`, and
    /// `CheckOutcome` outlives that borrow). Index-aligned with `type_errors`;
    /// `None` where the error's span is a synthetic-node dummy (no location).
    type_error_locations: Vec<Option<(usize, usize)>>,
    function_summaries: Vec<FunctionEffectSummary>,
}

#[derive(serde::Deserialize)]
struct SemanticCorpusProgram {
    id: String,
    path: String,
    #[serde(default)]
    surfaces: Vec<String>,
    expected_stdout: String,
}

#[derive(Clone, serde::Deserialize, serde::Serialize)]
struct CorpusExecutionReceipt {
    receipt_id: String,
    created_at: String,
    compiler: String,
    backend: String,
    evidence_class: String,
    command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    execution_mode: Option<String>,
    result: CorpusExecutionResult,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    declared_effects: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    observed_capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    capability_gate: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    capability_gate_test: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    manifest_execution_test: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    receipt_consistency_test: Option<String>,
    #[serde(default)]
    validator_chain: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    notes: Vec<String>,
    programs: Vec<CorpusExecutionProgram>,
    #[serde(flatten)]
    extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Clone, serde::Deserialize, serde::Serialize)]
struct CorpusExecutionResult {
    passed: usize,
    failed: usize,
    ignored: usize,
}

#[derive(Clone, serde::Deserialize, serde::Serialize)]
struct CorpusExecutionProgram {
    id: String,
    path: String,
    expected_stdout: String,
}

#[derive(serde::Deserialize)]
struct SubstrateReceipt {
    schema: String,
    receipt_id: String,
    created_at: String,
    compiler: String,
    language: String,
    source_set: SubstrateSourceSet,
    semantic_surface: SubstrateSemanticSurface,
    execution_surface: BTreeMap<String, SubstrateExecutionTarget>,
    memory_surface: SubstrateMemorySurface,
    representation_surface: SubstrateRepresentationSurface,
    module_surface: SubstrateModuleSurface,
    symbol_surface: SubstrateSymbolSurface,
    lsp_surface: SubstrateLspSurface,
    evidence_surface: SubstrateEvidenceSurface,
}

#[derive(serde::Deserialize)]
struct SubstrateSourceSet {
    kind: String,
    manifest: String,
    program_count: usize,
}

#[derive(serde::Deserialize)]
struct SubstrateSemanticSurface {
    check_receipt_schema: String,
    requires_source_digest: bool,
    requires_input_graph_digest: bool,
    #[serde(default)]
    effect_surfaces: Vec<String>,
}

#[derive(serde::Deserialize)]
struct SubstrateExecutionTarget {
    target: String,
    maturity: String,
    evidence_class: String,
    #[serde(default)]
    receipt: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    unsupported_mir_policy: Option<String>,
}

#[derive(serde::Deserialize)]
struct SubstrateMemorySurface {
    ownership_model: String,
    #[serde(default)]
    verified_surfaces: Vec<String>,
    #[serde(default)]
    known_gaps: Vec<String>,
    memory_receipt: String,
}

#[derive(serde::Deserialize)]
struct SubstrateRepresentationSurface {
    ir: String,
    fallback_policy: String,
    backend_maturity_descriptor: String,
    representation_receipt: String,
}

#[derive(serde::Deserialize)]
struct SubstrateModuleSurface {
    resolver: String,
    digest_anchor: String,
    module_receipt: String,
    #[serde(default)]
    known_gaps: Vec<String>,
}

#[derive(serde::Deserialize)]
struct SubstrateSymbolSurface {
    source: String,
    representation: String,
    effect_anchor: String,
    symbol_receipt: String,
    #[serde(default)]
    known_gaps: Vec<String>,
}

#[derive(serde::Deserialize)]
struct SubstrateLspSurface {
    protocol: String,
    dispatch: String,
    request_parser: String,
    lsp_receipt: String,
    #[serde(default)]
    known_gaps: Vec<String>,
}

#[derive(serde::Deserialize)]
struct SubstrateEvidenceSurface {
    #[serde(default)]
    commands: Vec<String>,
}

struct BuiltinPolicyTemplate {
    name: &'static str,
    summary: &'static str,
}

const BUILTIN_POLICY_TEMPLATES: &[BuiltinPolicyTemplate] = &[
    BuiltinPolicyTemplate {
        name: "pure",
        summary: "deny all built-in ambient capability effects",
    },
    BuiltinPolicyTemplate {
        name: "console-only",
        summary: "allow Console only; deny other ambient capability effects",
    },
    BuiltinPolicyTemplate {
        name: "offline",
        summary: "allow local file/env/clock/console work; deny network/process/FFI/GPU",
    },
    BuiltinPolicyTemplate {
        name: "ci-review",
        summary: "require digests and deny Network, Process, Foreign, and Gpu",
    },
    BuiltinPolicyTemplate {
        name: "strict-accountability",
        summary: "require digests, exact allowlists, and deny Network/Process/FFI/GPU",
    },
];

fn builtin_policy_profile(name: &str) -> Option<serde_json::Value> {
    match name {
        "pure" => Some(serde_json::json!({
            "schema": "buildlang-check-policy/v1",
            "denied_effects": [
                "FileSystem",
                "Network",
                "Process",
                "Environment",
                "Clock",
                "Console",
                "Foreign",
                "Gpu"
            ],
            "require_source_digest": true,
            "require_input_graph_digest": true
        })),
        "console-only" => Some(serde_json::json!({
            "schema": "buildlang-check-policy/v1",
            "allowed_effects": ["Console"],
            "denied_effects": [
                "FileSystem",
                "Network",
                "Process",
                "Environment",
                "Clock",
                "Foreign",
                "Gpu"
            ],
            "require_source_digest": true,
            "require_input_graph_digest": true
        })),
        "offline" => Some(serde_json::json!({
            "schema": "buildlang-check-policy/v1",
            "allowed_effects": [
                "FileSystem",
                "Environment",
                "Clock",
                "Console"
            ],
            "denied_effects": [
                "Network",
                "Process",
                "Foreign",
                "Gpu"
            ],
            "require_source_digest": true,
            "require_input_graph_digest": true
        })),
        "ci-review" => Some(serde_json::json!({
            "schema": "buildlang-check-policy/v1",
            "denied_effects": [
                "Network",
                "Process",
                "Foreign",
                "Gpu"
            ],
            "require_source_digest": true,
            "require_input_graph_digest": true
        })),
        "strict-accountability" => Some(serde_json::json!({
            "schema": "buildlang-check-policy/v1",
            "denied_effects": [
                "Network",
                "Process",
                "Foreign",
                "Gpu"
            ],
            "require_source_digest": true,
            "require_input_graph_digest": true,
            "require_effect_allowlist": true,
            "require_provenance_allowlists": true,
            "require_source_allowlists": true,
            "require_allowlist_coverage": true
        })),
        _ => None,
    }
}

fn builtin_policy_names() -> String {
    BUILTIN_POLICY_TEMPLATES
        .iter()
        .map(|template| template.name)
        .collect::<Vec<_>>()
        .join(", ")
}

fn builtin_policy_json(name: &str) -> Option<String> {
    let profile = builtin_policy_profile(name)?;
    let mut json = serde_json::to_string_pretty(&profile).expect("built-in policy profile is JSON");
    json.push('\n');
    Some(json)
}

fn builtin_policy_digest(name: &str) -> Option<CheckReceiptSourceDigest> {
    let json = builtin_policy_json(name)?;
    Some(CheckReceiptSourceDigest {
        algorithm: "sha256",
        hex: source_digest_hex(json.as_bytes()),
    })
}

fn normalize_digest_pin(pin: &str) -> &str {
    pin.strip_prefix("sha256:")
        .or_else(|| pin.strip_prefix("SHA256:"))
        .unwrap_or(pin)
}

fn builtin_policy_catalog_json() -> String {
    let profiles = BUILTIN_POLICY_TEMPLATES
        .iter()
        .map(|template| {
            let digest =
                builtin_policy_digest(template.name).expect("built-in policy has a digest");
            serde_json::json!({
                "name": template.name,
                "summary": template.summary,
                "policy_schema": "buildlang-check-policy/v1",
                "digest": digest
            })
        })
        .collect::<Vec<_>>();
    let mut json = serde_json::to_string_pretty(&serde_json::json!({
        "schema": "buildlang-policy-catalog/v1",
        "profiles": profiles
    }))
    .expect("built-in policy catalog is JSON");
    json.push('\n');
    json
}

fn receipt_effect_sources_by_effect(
    receipt: &serde_json::Value,
    field: &'static str,
) -> Result<BTreeMap<String, BTreeMap<String, Vec<String>>>, i32> {
    let functions = receipt
        .get(field)
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| {
            eprintln!("Error: receipt is missing object field `{field}`");
            1
        })?;
    let mut effects = BTreeMap::<String, BTreeMap<String, Vec<String>>>::new();
    for (function, effect_value) in functions {
        let effect_map = effect_value.as_object().ok_or_else(|| {
            eprintln!("Error: receipt field `{field}.{function}` must be an object");
            1
        })?;
        for (effect, sources_value) in effect_map {
            let sources = sources_value.as_array().ok_or_else(|| {
                eprintln!("Error: receipt field `{field}.{function}.{effect}` must be an array");
                1
            })?;
            let mut sorted_sources = BTreeSet::new();
            for source in sources {
                let source = source.as_str().ok_or_else(|| {
                    eprintln!(
                        "Error: receipt field `{field}.{function}.{effect}` must contain only strings"
                    );
                    1
                })?;
                sorted_sources.insert(source.to_string());
            }
            if !sorted_sources.is_empty() {
                effects
                    .entry(effect.clone())
                    .or_default()
                    .insert(function.clone(), sorted_sources.into_iter().collect());
            }
        }
    }
    Ok(effects)
}

fn receipt_declared_effects(receipt: &serde_json::Value) -> Result<BTreeSet<String>, i32> {
    let functions = receipt
        .get("declared_effects")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| {
            eprintln!("Error: receipt is missing object field `declared_effects`");
            1
        })?;
    let mut effects = BTreeSet::new();
    for (function, effect_value) in functions {
        let declared = effect_value.as_array().ok_or_else(|| {
            eprintln!("Error: receipt field `declared_effects.{function}` must be an array");
            1
        })?;
        for effect in declared {
            let effect = effect.as_str().ok_or_else(|| {
                eprintln!(
                    "Error: receipt field `declared_effects.{function}` must contain only strings"
                );
                1
            })?;
            effects.insert(effect.to_string());
        }
    }
    Ok(effects)
}

fn effect_function_allowlist(
    sources_by_effect: &BTreeMap<String, BTreeMap<String, Vec<String>>>,
) -> BTreeMap<String, Vec<String>> {
    sources_by_effect
        .iter()
        .map(|(effect, functions)| {
            (
                effect.clone(),
                functions.keys().cloned().collect::<Vec<String>>(),
            )
        })
        .collect()
}

fn scaffold_policy_from_receipt(receipt: &serde_json::Value) -> Result<CheckPolicyProfile, i32> {
    let schema = receipt_field_str(receipt, "/schema", "schema")?;
    if schema != "buildlang-check-receipt/v1" {
        eprintln!("Error: unsupported check receipt schema `{}`", schema);
        return Err(1);
    }

    let direct_sources = receipt_effect_sources_by_effect(receipt, "observed_capabilities")?;
    let propagated_sources = receipt_effect_sources_by_effect(receipt, "propagated_effects")?;
    let mut allowed_effects = receipt_declared_effects(receipt)?;
    allowed_effects.extend(direct_sources.keys().cloned());
    allowed_effects.extend(propagated_sources.keys().cloned());

    Ok(CheckPolicyProfile {
        schema: "buildlang-check-policy/v1".to_string(),
        allowed_effects: allowed_effects.into_iter().collect(),
        denied_effects: Vec::new(),
        direct_effect_allowlist: effect_function_allowlist(&direct_sources),
        direct_capability_source_allowlist: direct_sources,
        propagated_effect_allowlist: effect_function_allowlist(&propagated_sources),
        propagated_effect_source_allowlist: propagated_sources,
        require_source_digest: true,
        require_input_graph_digest: true,
        require_effect_allowlist: true,
        require_provenance_allowlists: true,
        require_source_allowlists: true,
        require_allowlist_coverage: true,
    })
}

fn write_policy_json(output: Option<&Path>, profile: &CheckPolicyProfile) -> Result<(), i32> {
    if let Some(path) = output {
        write_json(path, profile)
    } else {
        let json = serde_json::to_string_pretty(profile).map_err(|err| {
            eprintln!("Error serializing scaffolded policy: {}", err);
            1
        })?;
        println!("{json}");
        Ok(())
    }
}

fn cmd_policy(command: PolicyCommands) -> Result<(), i32> {
    match command {
        PolicyCommands::List { json } => {
            if json {
                print!("{}", builtin_policy_catalog_json());
            } else {
                println!("Built-in check policy profiles:");
                for template in BUILTIN_POLICY_TEMPLATES {
                    println!("  {:<14} {}", template.name, template.summary);
                }
            }
            Ok(())
        }
        PolicyCommands::Print { name, output } => {
            let json = builtin_policy_json(&name).ok_or_else(|| {
                eprintln!(
                    "Unknown built-in policy profile '{}'. Available: {}",
                    name,
                    builtin_policy_names()
                );
                1
            })?;
            if let Some(path) = output {
                std::fs::write(&path, json).map_err(|err| {
                    eprintln!("Error writing policy profile '{}': {}", path.display(), err);
                    1
                })?;
            } else {
                print!("{json}");
            }
            Ok(())
        }
        PolicyCommands::Scaffold { receipt, output } => {
            let receipt: serde_json::Value = read_json(&receipt)?;
            let profile = scaffold_policy_from_receipt(&receipt)?;
            write_policy_json(output.as_deref(), &profile)
        }
    }
}

fn receipt_field_str<'a>(
    receipt: &'a serde_json::Value,
    pointer: &str,
    label: &str,
) -> Result<&'a str, i32> {
    receipt
        .pointer(pointer)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            eprintln!("Error: receipt is missing string field `{}`", label);
            1
        })
}

fn receipt_digest_hex<'a>(
    receipt: &'a serde_json::Value,
    pointer: &str,
    label: &str,
) -> Result<&'a str, i32> {
    let algorithm = receipt_field_str(receipt, &format!("{pointer}/algorithm"), label)?;
    if algorithm != "sha256" {
        eprintln!(
            "Error: receipt field `{}` uses unsupported digest algorithm `{}`",
            label, algorithm
        );
        return Err(1);
    }
    let hex = receipt_field_str(receipt, &format!("{pointer}/hex"), label)?;
    if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        eprintln!(
            "Error: receipt field `{}` is not a sha256 hex digest",
            label
        );
        return Err(1);
    }
    Ok(hex)
}

fn verify_receipt_digest(
    receipt: &serde_json::Value,
    pointer: &str,
    label: &str,
    actual: &CheckReceiptSourceDigest,
) -> Result<(), i32> {
    let expected_hex = receipt_digest_hex(receipt, pointer, label)?;
    if actual.algorithm != "sha256" || !actual.hex.eq_ignore_ascii_case(expected_hex) {
        eprintln!(
            "Error: {} mismatch: expected sha256:{}, actual sha256:{}",
            label, expected_hex, actual.hex
        );
        return Err(1);
    }
    Ok(())
}

fn current_policy_source_digest(policy_source: &str) -> Result<CheckReceiptSourceDigest, i32> {
    if let Some(profile) = policy_source.strip_prefix("builtin:") {
        return builtin_policy_digest(profile).ok_or_else(|| {
            eprintln!("Error: unknown built-in policy profile `{}`", profile);
            1
        });
    }

    let path = Path::new(policy_source);
    let bytes = std::fs::read(path).map_err(|err| {
        eprintln!("Error reading policy '{}': {}", path.display(), err);
        1
    })?;
    Ok(CheckReceiptSourceDigest {
        algorithm: "sha256",
        hex: source_digest_hex(&bytes),
    })
}

fn cmd_receipt(command: ReceiptCommands) -> Result<(), i32> {
    match command {
        ReceiptCommands::Verify {
            receipt,
            source,
            expect_profile,
            expect_policy_digest,
            json,
            self_test,
        } => {
            if self_test {
                cmd_receipt_verify_self_test(&receipt)
            } else {
                cmd_receipt_verify(
                    &receipt,
                    source.as_deref(),
                    expect_profile.as_deref(),
                    expect_policy_digest.as_deref(),
                    json,
                )
            }
        }
        ReceiptCommands::Export {
            receipt,
            output,
            claim_id,
            claim_sha256,
            claim_expects_failure,
        } => cmd_receipt_export(
            &receipt,
            &output,
            &claim_id,
            &claim_sha256,
            claim_expects_failure,
        ),
        ReceiptCommands::Chain { command } => match command {
            ChainCommands::Build { receipts, output } => {
                cmd_receipt_chain_build(&receipts, &output)
            }
            ChainCommands::Verify { manifest } => cmd_receipt_chain_verify(&manifest),
        },
        ReceiptCommands::Corpus { manifest } => cmd_receipt_corpus(&manifest),
    }
}

/// `receipt verify --self-test`: prove the verifier can FAIL. Take a valid
/// scientific-runtime receipt, tamper each of several sealed fields, and assert
/// that each tamper is rejected by the real `receipt verify` path with its
/// expected `failure_class`. A verifier that cannot distinguish these tampers
/// would report a class it did not actually derive, so this closes the same
/// can-it-FAIL gap on the verifier that the negative-fixture kernels close on
/// the invariants. All tamper cases are rejected before any program re-run, so
/// this needs no toolchain.
fn cmd_receipt_verify_self_test(receipt_path: &Path) -> Result<(), i32> {
    let receipt: serde_json::Value = read_json(receipt_path).map_err(|code| {
        eprintln!("Error: could not read receipt for self-test");
        code
    })?;
    let cases = build_self_test_cases(&receipt).map_err(|err| {
        eprintln!("Error: {err}");
        1
    })?;

    let exe = std::env::current_exe().map_err(|err| {
        eprintln!("Error: cannot locate the buildc binary for self-test: {err}");
        1
    })?;
    let tmp_dir = std::env::temp_dir().join(format!("buildc_selftest_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp_dir);
    std::fs::create_dir_all(&tmp_dir).map_err(|err| {
        eprintln!("Error: cannot create self-test scratch dir: {err}");
        1
    })?;

    let total = cases.len();
    let mut failures = 0usize;
    for (i, case) in cases.iter().enumerate() {
        let path = tmp_dir.join(format!("case_{i}.json"));
        let bytes = serde_json::to_vec_pretty(&case.tampered).expect("serialize tamper case");
        if std::fs::write(&path, &bytes).is_err() {
            eprintln!("  FAIL {} (could not write case file)", case.label);
            failures += 1;
            continue;
        }
        let output = match std::process::Command::new(&exe)
            .args(["receipt", "verify"])
            .arg(&path)
            .arg("--json")
            .output()
        {
            Ok(output) => output,
            Err(err) => {
                eprintln!(
                    "  FAIL {} (verify subprocess failed to run: {err})",
                    case.label
                );
                failures += 1;
                continue;
            }
        };
        // A tamper MUST be rejected (non-zero exit) with the expected class.
        let rejected = !output.status.success();
        let actual_class = serde_json::from_slice::<serde_json::Value>(&output.stdout)
            .ok()
            .and_then(|value| {
                value
                    .get("failure_class")
                    .and_then(|c| c.as_str())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "(none)".to_string());
        if rejected && actual_class == case.expected_class {
            println!("  ok   {} => {}", case.label, actual_class);
        } else {
            eprintln!(
                "  FAIL {} => expected {} (rejected), got {} (rejected={})",
                case.label, case.expected_class, actual_class, rejected
            );
            failures += 1;
        }
    }
    let _ = std::fs::remove_dir_all(&tmp_dir);

    if failures == 0 {
        println!("self-test: {total}/{total} tampers rejected with the expected failure_class");
        Ok(())
    } else {
        eprintln!(
            "self-test: {}/{} tampers did NOT produce the expected failure_class",
            failures, total
        );
        Err(1)
    }
}

/// Report a stable chain `failure_class` on stderr and return the exit code, so
/// tests and CI can pin the specific break instead of accepting "chain failed".
fn chain_failure(code: &str, exit: i32) -> i32 {
    eprintln!("failure_class: {code}");
    exit
}

/// `receipt chain build`: assemble an ordered chain manifest over member
/// scientific-runtime receipts. Each member's seal is pinned into the chain and
/// the chain seal binds their order and membership.
fn cmd_receipt_chain_build(receipts: &[PathBuf], output: &Path) -> Result<(), i32> {
    if receipts.len() < 2 {
        eprintln!("Error: a receipt chain needs at least two member receipts");
        return Err(1);
    }
    let mut members: Vec<(String, String, String)> = Vec::new();
    for path in receipts {
        let receipt: serde_json::Value = read_json(path).map_err(|code| {
            eprintln!("Error: could not read receipt '{}'", path.display());
            code
        })?;
        let schema = receipt.get("schema").and_then(|v| v.as_str()).unwrap_or("");
        // Allowlist widened for model boundary receipts (design section 6):
        // a model receipt can be a chain member beside scientific-runtime
        // receipts, demonstrating propose (model) / dispose (oracle) as a
        // single chained bundle. `source` extraction below needs no change:
        // both schemas carry a top-level `source` label. Chain VERIFY needs
        // zero changes beyond this: pinned seals and subprocess
        // re-verification (`buildc receipt verify <member>`) already compose
        // through the schema-agnostic dispatch this widening exercises.
        if schema != SCIENTIFIC_RUNTIME_SCHEMA
            && schema != MODEL_RECEIPT_SCHEMA
            && schema != TOOL_RECEIPT_SCHEMA
        {
            eprintln!(
                "Error: '{}' is not a chainable receipt (schema `{}`; expected `{}`, `{}`, or `{}`)",
                path.display(),
                schema,
                SCIENTIFIC_RUNTIME_SCHEMA,
                MODEL_RECEIPT_SCHEMA,
                TOOL_RECEIPT_SCHEMA
            );
            return Err(1);
        }
        let seal = receipt
            .pointer("/seal/hex")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if seal.is_empty() {
            eprintln!("Error: receipt '{}' has no seal", path.display());
            return Err(1);
        }
        let source = receipt.get("source").and_then(|v| v.as_str()).unwrap_or("");
        members.push((
            path.to_string_lossy().to_string(),
            source.to_string(),
            seal.to_string(),
        ));
    }

    let manifest = build_receipt_chain(&members);
    let text = serde_json::to_string_pretty(&manifest).map_err(|err| {
        eprintln!("Error serializing chain manifest: {err}");
        1
    })?;
    if output == Path::new("-") {
        println!("{text}");
    } else {
        let tmp = output.with_extension("tmp");
        std::fs::write(&tmp, format!("{text}\n")).map_err(|err| {
            eprintln!("Error writing chain manifest '{}': {err}", tmp.display());
            let _ = std::fs::remove_file(&tmp);
            1
        })?;
        std::fs::rename(&tmp, output).map_err(|err| {
            eprintln!(
                "Error finalizing chain manifest '{}': {err}",
                output.display()
            );
            1
        })?;
    }
    Ok(())
}

/// `receipt chain verify`: re-check the chain seal (order and membership), pin
/// each member to its recorded seal, and re-verify each member receipt through
/// the real `receipt verify` path. Any break in the ordered bundle fails with a
/// stable chain `failure_class`.
fn cmd_receipt_chain_verify(manifest_path: &Path) -> Result<(), i32> {
    let manifest: ReceiptChainManifest =
        read_json(manifest_path).map_err(|_| chain_failure("CHAIN_MALFORMED", 1))?;
    if manifest.schema != RECEIPT_CHAIN_SCHEMA {
        eprintln!("Error: unsupported chain schema `{}`", manifest.schema);
        return Err(chain_failure("CHAIN_SCHEMA_UNSUPPORTED", 1));
    }
    if manifest.links.len() < 2 {
        eprintln!("Error: a receipt chain needs at least two links");
        return Err(chain_failure("CHAIN_MALFORMED", 1));
    }

    // 1. Chain integrity: the chain seal must bind the exact ordered member seals.
    let recomputed = receipt_chain_seal_hex(&manifest.links);
    if !recomputed.eq_ignore_ascii_case(&manifest.chain_seal.hex) {
        eprintln!(
            "Error: chain seal mismatch: manifest {}, recomputed {}",
            manifest.chain_seal.hex, recomputed
        );
        return Err(chain_failure("CHAIN_SEAL_MISMATCH", 1));
    }

    // 2. Each member: pinned seal, then full re-verification of the receipt.
    let exe = std::env::current_exe().map_err(|err| {
        eprintln!("Error: cannot locate the buildc binary: {err}");
        1
    })?;
    for link in &manifest.links {
        let receipt_path = Path::new(&link.receipt);
        let receipt: serde_json::Value = match read_json(receipt_path) {
            Ok(value) => value,
            Err(_) => {
                eprintln!(
                    "Error: chain member '{}' is missing or unreadable",
                    link.receipt
                );
                return Err(chain_failure("CHAIN_LINK_MISSING", 1));
            }
        };
        let seal = receipt
            .pointer("/seal/hex")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if !seal.eq_ignore_ascii_case(&link.receipt_seal) {
            eprintln!(
                "Error: chain member '{}' seal {} does not match its pinned seal {}",
                link.receipt, seal, link.receipt_seal
            );
            return Err(chain_failure("CHAIN_LINK_TAMPERED", 1));
        }
        let output = match std::process::Command::new(&exe)
            .args(["receipt", "verify"])
            .arg(receipt_path)
            .output()
        {
            Ok(output) => output,
            Err(err) => {
                eprintln!(
                    "Error: could not re-verify chain member '{}': {err}",
                    link.receipt
                );
                return Err(chain_failure("CHAIN_LINK_UNVERIFIED", 1));
            }
        };
        if !output.status.success() {
            eprint!("{}", String::from_utf8_lossy(&output.stderr));
            eprintln!("Error: chain member '{}' did not re-verify", link.receipt);
            return Err(chain_failure(
                "CHAIN_LINK_UNVERIFIED",
                output.status.code().unwrap_or(1),
            ));
        }
    }

    println!(
        "chain verified: {} members, sealed in order and each receipt re-verified",
        manifest.links.len()
    );
    Ok(())
}

/// `receipt corpus`: emit and re-verify every declared example kernel and assert
/// each classifies exactly as the manifest declares. This is the runnable
/// accountability gate for the whole example suite: a kernel whose verdict
/// silently changes, or a receipt that stops re-deriving, fails the corpus.
fn cmd_receipt_corpus(manifest_path: &Path) -> Result<(), i32> {
    let manifest: ScientificCorpusManifest = read_json(manifest_path).map_err(|code| {
        eprintln!(
            "Error: could not read corpus manifest '{}'",
            manifest_path.display()
        );
        code
    })?;
    if manifest.schema != RECEIPT_CORPUS_SCHEMA {
        eprintln!("Error: unsupported corpus schema `{}`", manifest.schema);
        return Err(1);
    }
    if manifest.members.is_empty() {
        eprintln!("Error: corpus manifest has no members");
        return Err(1);
    }

    let exe = std::env::current_exe().map_err(|err| {
        eprintln!("Error: cannot locate the buildc binary: {err}");
        1
    })?;
    let tmp_dir = std::env::temp_dir().join(format!("buildc_corpus_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp_dir);
    std::fs::create_dir_all(&tmp_dir).map_err(|err| {
        eprintln!("Error: cannot create corpus scratch dir: {err}");
        1
    })?;

    let total = manifest.members.len();
    let mut failures = 0usize;
    for (i, member) in manifest.members.iter().enumerate() {
        let receipt = tmp_dir.join(format!("member_{i}.json"));

        // Emit the member's receipt under its declared invariant and flags.
        let mut emit = std::process::Command::new(&exe);
        emit.arg("run")
            .arg(&member.source)
            .args(["--emit-receipt"])
            .arg(&receipt)
            .args(["--invariant", &member.invariant]);
        if member.columns > 1 {
            emit.args(["--columns", &member.columns.to_string()]);
        }
        if member.negative_fixture {
            emit.arg("--negative-fixture");
        }
        if let Some(seed) = member.seed {
            emit.args(["--seed", &seed.to_string()]);
        }
        if let Some(estimator) = &member.mc_estimator {
            emit.args(["--mc-estimator", estimator]);
        }
        if let Some(samples) = member.mc_samples {
            emit.args(["--mc-samples", &samples.to_string()]);
        }
        if let Some(interval) = &member.mc_interval {
            emit.args(["--mc-interval", interval]);
        }
        if member.mc_executed {
            emit.arg("--mc-executed");
        }
        if let Some(steps) = member.budget_steps {
            emit.args(["--budget-steps", &steps.to_string()]);
        }
        if let Some(consumed) = member.budget_consumed {
            emit.args(["--budget-consumed", &consumed.to_string()]);
        }
        if let Some(target) = &member.cross_backend {
            emit.args(["--cross-backend", target]);
        }
        let emit_out = match emit.output() {
            Ok(out) => out,
            Err(err) => {
                eprintln!("  FAIL {} (emit failed to run: {err})", member.source);
                failures += 1;
                continue;
            }
        };
        if !emit_out.status.success() {
            eprintln!(
                "  FAIL {} (emit failed)\n{}",
                member.source,
                String::from_utf8_lossy(&emit_out.stderr)
            );
            failures += 1;
            continue;
        }

        // The emitted receipt must classify exactly as declared.
        let status = std::fs::read(&receipt)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .and_then(|value| {
                value
                    .get("receipt_status")
                    .and_then(|s| s.as_str())
                    .map(str::to_string)
            });
        let status = match status {
            Some(status) => status,
            None => {
                eprintln!(
                    "  FAIL {} (could not read emitted receipt_status)",
                    member.source
                );
                failures += 1;
                continue;
            }
        };
        if status != member.expected_status {
            eprintln!(
                "  FAIL {} => declared {}, emitted {}",
                member.source, member.expected_status, status
            );
            failures += 1;
            continue;
        }

        // The receipt must also re-derive clean through the real verify path.
        let verify_out = match std::process::Command::new(&exe)
            .args(["receipt", "verify"])
            .arg(&receipt)
            .output()
        {
            Ok(out) => out,
            Err(err) => {
                eprintln!("  FAIL {} (verify failed to run: {err})", member.source);
                failures += 1;
                continue;
            }
        };
        if !verify_out.status.success() {
            eprintln!(
                "  FAIL {} (receipt did not re-verify)\n{}",
                member.source,
                String::from_utf8_lossy(&verify_out.stderr)
            );
            failures += 1;
            continue;
        }

        println!(
            "  ok   {} [{}] => {}",
            member.source, member.invariant, status
        );
    }
    let _ = std::fs::remove_dir_all(&tmp_dir);

    if failures == 0 {
        println!("corpus: {total}/{total} members classified and re-verified as declared");
        Ok(())
    } else {
        eprintln!(
            "corpus: {}/{} members did NOT match their declared classification",
            failures, total
        );
        Err(1)
    }
}

/// The Telos/Crucible bridge: export a scientific-runtime receipt as one
/// Crucible-ingestible measurement row inside a versioned envelope.
///
/// The receipt is RE-VERIFIED first through the exact evaluation path
/// `receipt verify` uses; a receipt that does not reproduce exports NOTHING
/// (exit 1/4 propagate). The measurement's deviation comes from the fresh
/// re-run, and the sealed `recheck` descriptor carries the full replay
/// command, so the exported row is witnessed rather than asserted.
fn cmd_receipt_export(
    receipt_path: &Path,
    output: &Path,
    claim_id: &str,
    claim_sha256: &str,
    claim_expects_failure: bool,
) -> Result<(), i32> {
    // Read ONCE and both hash and parse the same buffer: the hash sealed into
    // recheck.receipt_sha256 must attest exactly the bytes that were verified
    // and exported (re-reading the file would open a hash-vs-parse TOCTOU).
    let receipt_bytes = std::fs::read(receipt_path).map_err(|err| {
        eprintln!(
            "Error reading receipt '{}': {}",
            receipt_path.display(),
            err
        );
        receipt_load_failure(false, "MALFORMED", 1)
    })?;
    let receipt_file_sha256 = source_digest_hex(&receipt_bytes);
    let receipt_text = std::str::from_utf8(&receipt_bytes).map_err(|err| {
        eprintln!(
            "Error: receipt '{}' is not UTF-8: {}",
            receipt_path.display(),
            err
        );
        receipt_load_failure(false, "MALFORMED", 1)
    })?;
    assert_no_duplicate_json_keys(receipt_text).map_err(|err| {
        eprintln!(
            "Error parsing receipt '{}': {}",
            receipt_path.display(),
            err
        );
        receipt_load_failure(false, "MALFORMED", 1)
    })?;
    let receipt: serde_json::Value = serde_json::from_str(receipt_text).map_err(|err| {
        eprintln!(
            "Error parsing receipt '{}': {}",
            receipt_path.display(),
            err
        );
        receipt_load_failure(false, "MALFORMED", 1)
    })?;
    let schema = receipt_field_str(&receipt, "/schema", "schema")
        .map_err(|code| receipt_load_failure(false, "SCHEMA_UNSUPPORTED", code))?;
    if schema != SCIENTIFIC_RUNTIME_SCHEMA {
        eprintln!(
            "Error: receipt export supports `{}` only (got `{}`); the check-receipt and corpus surfaces are documented follow-ons",
            SCIENTIFIC_RUNTIME_SCHEMA, schema
        );
        return Err(receipt_load_failure(false, "SCHEMA_UNSUPPORTED", 1));
    }

    // Re-verify through the same evaluation path `receipt verify` uses.
    let probed_toolchain = probe_c_toolchain(false);
    let report = evaluate_scientific_runtime_receipt(
        &receipt,
        None,
        false,
        env!("CARGO_PKG_VERSION"),
        &language_version_string(),
        probed_toolchain.as_ref(),
        |source_path| {
            let outcome = run_check(source_path)?;
            Ok(RederivedFacts {
                source_digest: ScientificDigest::from(&outcome.source_digest),
                input_graph_digest: ScientificDigest::from(&outcome.input_graph_digest),
                effect_policy: derive_effect_policy(&outcome),
            })
        },
        |source_path, args, seed, secondary_target| {
            rerun_scientific_receipt(
                source_path,
                args,
                seed,
                secondary_target,
                probed_toolchain.as_ref(),
            )
        },
    )?;

    // The expected-failure binding is claim semantics, so it is valid only
    // when the receipt actually IS a declared negative fixture; applying it
    // to an ordinary receipt would let a plain failure masquerade as a
    // predicted one.
    if claim_expects_failure && !report.negative_fixture {
        eprintln!(
            "Error: --claim-expects-failure requires a negative-fixture receipt (this receipt was not emitted with --negative-fixture)"
        );
        return Err(1);
    }

    let measured_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    let measurement = crucible_measurement_from_report(
        &report,
        claim_id,
        claim_sha256,
        claim_expects_failure,
        &receipt_path.to_string_lossy(),
        &receipt_file_sha256,
        measured_at,
    );

    let mut envelope = serde_json::json!({
        "schema": CRUCIBLE_MEASUREMENT_EXPORT_SCHEMA,
        "generated_by": format!("buildc {}", env!("CARGO_PKG_VERSION")),
        "faithful": true,
        "invariant_held": report.invariant_held,
        "measurements": [measurement],
    });
    if claim_id.is_empty() || claim_sha256.is_empty() {
        envelope["binding_note"] = serde_json::Value::String(
            "claim_id/claim_sha256 are empty: bind this measurement to a thesis claim before assessment (Crucible UNVERIFIABLEs an unbound measurement, fail-closed)".to_string(),
        );
    }
    let text = serde_json::to_string_pretty(&envelope).map_err(|err| {
        eprintln!("Error serializing measurement export: {}", err);
        1
    })?;
    if output == Path::new("-") {
        println!("{}", text);
    } else {
        // Atomic write: a failed export must never destroy or truncate a
        // previously good measurement at the same path. Write a sibling temp
        // file, then rename over the target (atomic on the same volume).
        let tmp = output.with_extension("tmp");
        std::fs::write(&tmp, format!("{}\n", text)).map_err(|err| {
            eprintln!(
                "Error writing measurement export '{}': {}",
                tmp.display(),
                err
            );
            let _ = std::fs::remove_file(&tmp);
            1
        })?;
        std::fs::rename(&tmp, output).map_err(|err| {
            eprintln!(
                "Error finalizing measurement export '{}': {}",
                output.display(),
                err
            );
            let _ = std::fs::remove_file(&tmp);
            1
        })?;
        eprintln!(
            "exported: 1 witnessed measurement ({}, violation_count={}) -> {}",
            report.receipt_status,
            report.violation_count,
            output.display()
        );
    }
    Ok(())
}

fn digest_label(digest: &CheckReceiptSourceDigest) -> String {
    format!("{}:{}", digest.algorithm, digest.hex)
}

fn push_receipt_verification_check(
    checks: &mut Vec<ReceiptVerificationCheck>,
    name: &str,
    expected: Option<String>,
    actual: Option<String>,
    profile: Option<String>,
    message: Option<String>,
) {
    checks.push(ReceiptVerificationCheck {
        name: name.to_string(),
        status: if message.is_none() {
            "passed"
        } else {
            "failed"
        },
        expected,
        actual,
        profile,
        message,
    });
}

fn receipt_builtin_profile(receipt: &serde_json::Value) -> Option<&str> {
    if let Some(profile) = receipt
        .pointer("/policy/profile")
        .and_then(serde_json::Value::as_str)
    {
        return Some(profile);
    }
    receipt
        .pointer("/policy/source")
        .and_then(serde_json::Value::as_str)
        .and_then(|source| source.strip_prefix("builtin:"))
}

fn builtin_profile_label(profile: Option<&str>) -> Option<String> {
    profile.map(|profile| format!("builtin:{profile}"))
}

fn receipt_policy_digest_hex(receipt: &serde_json::Value) -> Option<&str> {
    receipt
        .pointer("/policy/source_digest/hex")
        .and_then(serde_json::Value::as_str)
}

fn receipt_policy_digest_is_sha256(receipt: &serde_json::Value) -> bool {
    receipt
        .pointer("/policy/source_digest/algorithm")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|algorithm| algorithm.eq_ignore_ascii_case("sha256"))
}

fn receipt_policy_digest_matches(receipt: &serde_json::Value, expected_hex: &str) -> bool {
    receipt_policy_digest_is_sha256(receipt)
        && receipt_policy_digest_hex(receipt)
            .is_some_and(|actual| actual.eq_ignore_ascii_case(expected_hex))
}

fn receipt_policy_digest_label(receipt: &serde_json::Value) -> Option<String> {
    let digest = receipt.pointer("/policy/source_digest")?;
    let algorithm = digest.get("algorithm")?.as_str()?;
    let hex = digest.get("hex")?.as_str()?;
    Some(format!("{algorithm}:{hex}"))
}

fn verify_receipt_expected_policy_digest(
    receipt: &serde_json::Value,
    expected_policy_digest: Option<&str>,
) -> Result<(), i32> {
    let Some(expected_policy_digest) = expected_policy_digest else {
        return Ok(());
    };
    let expected_hex = normalize_digest_pin(expected_policy_digest);
    if !receipt_policy_digest_matches(receipt, expected_hex) {
        let actual = receipt_policy_digest_label(receipt).unwrap_or_else(|| "none".to_string());
        eprintln!(
            "Error: receipt policy digest mismatch: expected sha256:{}, actual {}",
            expected_hex, actual
        );
        return Err(1);
    }

    Ok(())
}

fn push_receipt_expected_policy_digest_check(
    checks: &mut Vec<ReceiptVerificationCheck>,
    receipt: &serde_json::Value,
    expected_policy_digest: Option<&str>,
) {
    let Some(expected_policy_digest) = expected_policy_digest else {
        return;
    };
    let expected_hex = normalize_digest_pin(expected_policy_digest);
    let mismatch = !receipt_policy_digest_matches(receipt, expected_hex);
    push_receipt_verification_check(
        checks,
        "expected_policy_digest",
        Some(format!("sha256:{expected_hex}")),
        receipt_policy_digest_label(receipt),
        None,
        mismatch.then(|| "receipt policy digest mismatch".to_string()),
    );
}

fn verify_receipt_expected_profile(
    receipt: &serde_json::Value,
    expected_profile: Option<&str>,
) -> Result<(), i32> {
    let Some(expected_profile) = expected_profile else {
        return Ok(());
    };
    if builtin_policy_profile(expected_profile).is_none() {
        eprintln!(
            "Error: unknown built-in policy profile `{}`",
            expected_profile
        );
        return Err(1);
    }

    let actual_profile = receipt_builtin_profile(receipt);
    if actual_profile != Some(expected_profile) {
        let actual = builtin_profile_label(actual_profile).unwrap_or_else(|| "none".to_string());
        eprintln!(
            "Error: receipt built-in profile mismatch: expected builtin:{}, actual {}",
            expected_profile, actual
        );
        return Err(1);
    }

    Ok(())
}

fn push_receipt_expected_profile_check(
    checks: &mut Vec<ReceiptVerificationCheck>,
    receipt: &serde_json::Value,
    expected_profile: Option<&str>,
) -> Result<(), i32> {
    let Some(expected_profile) = expected_profile else {
        return Ok(());
    };
    if builtin_policy_profile(expected_profile).is_none() {
        eprintln!(
            "Error: unknown built-in policy profile `{}`",
            expected_profile
        );
        return Err(1);
    }

    let actual_profile = receipt_builtin_profile(receipt);
    let mismatch = actual_profile != Some(expected_profile);
    push_receipt_verification_check(
        checks,
        "expected_profile",
        Some(format!("builtin:{expected_profile}")),
        builtin_profile_label(actual_profile),
        Some(expected_profile.to_string()),
        mismatch.then(|| "receipt built-in profile mismatch".to_string()),
    );
    Ok(())
}

fn compact_receipt_value(value: Option<&serde_json::Value>) -> Option<String> {
    value.map(|value| {
        serde_json::to_string(value).unwrap_or_else(|err| format!("<unserializable: {err}>"))
    })
}

fn receipt_replay_fields(
    receipt: &serde_json::Value,
    current_receipt: &serde_json::Value,
) -> Vec<(&'static str, &'static str)> {
    let mut fields = vec![
        ("/status", "status"),
        ("/items", "items"),
        ("/tokens", "tokens"),
        ("/declared_effects", "declared_effects"),
        ("/observed_capabilities", "observed_capabilities"),
        ("/propagated_effects", "propagated_effects"),
        ("/diagnostics", "diagnostics"),
    ];
    if receipt.pointer("/policy").is_some() || current_receipt.pointer("/policy").is_some() {
        fields.push(("/policy/status", "policy_status"));
        fields.push(("/policy/violations", "policy_violations"));
    }
    fields
}

fn load_receipt_policy(receipt: &serde_json::Value) -> Result<Option<LoadedCheckPolicy>, i32> {
    if let Some(profile) = receipt
        .pointer("/policy/profile")
        .and_then(serde_json::Value::as_str)
    {
        return load_builtin_check_policy(profile).map(Some);
    }

    let Some(policy_source) = receipt
        .pointer("/policy/source")
        .and_then(serde_json::Value::as_str)
    else {
        return Ok(None);
    };

    if let Some(profile) = policy_source.strip_prefix("builtin:") {
        load_builtin_check_policy(profile).map(Some)
    } else {
        load_check_policy(Path::new(policy_source)).map(Some)
    }
}

fn current_replayed_receipt_value(
    receipt: &serde_json::Value,
    current: &CheckOutcome,
) -> Result<serde_json::Value, i32> {
    let loaded_policy = load_receipt_policy(receipt)?;
    let policy_decision = loaded_policy
        .as_ref()
        .map(|policy| evaluate_check_policy(policy, current));
    let current_receipt = build_check_receipt(current, policy_decision.as_ref());
    serde_json::to_value(current_receipt).map_err(|err| {
        eprintln!("Error rebuilding receipt for verification: {}", err);
        1
    })
}

fn verify_receipt_replay_fields(
    receipt: &serde_json::Value,
    current_receipt: &serde_json::Value,
) -> Result<(), i32> {
    for (pointer, name) in receipt_replay_fields(receipt, current_receipt) {
        if receipt.pointer(pointer) != current_receipt.pointer(pointer) {
            eprintln!("Error: receipt {} mismatch", name);
            return Err(1);
        }
    }
    Ok(())
}

fn push_receipt_replay_checks(
    checks: &mut Vec<ReceiptVerificationCheck>,
    receipt: &serde_json::Value,
    current_receipt: &serde_json::Value,
) {
    for (pointer, name) in receipt_replay_fields(receipt, current_receipt) {
        let expected = compact_receipt_value(receipt.pointer(pointer));
        let actual = compact_receipt_value(current_receipt.pointer(pointer));
        let mismatch = receipt.pointer(pointer) != current_receipt.pointer(pointer);
        push_receipt_verification_check(
            checks,
            name,
            expected,
            actual,
            None,
            mismatch.then(|| format!("receipt {} mismatch", name)),
        );
    }
}

/// Dispatch a scientific-runtime receipt to its verifier, supplying the local
/// toolchain probe plus the two re-derivation callbacks the module needs.
/// `run_check` re-derives the source + input-graph digests through the exact
/// check pipeline that produced them, and `compile_and_capture_run` re-runs the
/// program so the invariant is re-checked (not trusted). Shared by the human
/// and `--json` verify paths.
fn verify_scientific_receipt_dispatch(
    receipt: &serde_json::Value,
    source_override: Option<&Path>,
    json: bool,
) -> Result<(), i32> {
    // Probe once; None routes to TOOL_UNAVAILABLE (exit 4) inside the module,
    // BEFORE any re-run is attempted.
    let probed_toolchain = probe_c_toolchain(false);
    verify_scientific_runtime_receipt(
        receipt,
        source_override,
        json,
        env!("CARGO_PKG_VERSION"),
        &language_version_string(),
        probed_toolchain.as_ref(),
        |source_path| {
            let outcome = run_check(source_path)?;
            Ok(RederivedFacts {
                source_digest: ScientificDigest::from(&outcome.source_digest),
                input_graph_digest: ScientificDigest::from(&outcome.input_graph_digest),
                effect_policy: derive_effect_policy(&outcome),
            })
        },
        |source_path, args, seed, secondary_target| {
            rerun_scientific_receipt(
                source_path,
                args,
                seed,
                secondary_target,
                probed_toolchain.as_ref(),
            )
        },
    )
}

/// Verify a GPU cross-check receipt (Layer C). Delegates to the always-compiled
/// `gpu_receipt` module: recompute the seal over the body and re-check the
/// gpu-cpu agreement invariant against the recorded series.
fn gpu_receipt_verify(receipt_path: &Path) -> Result<(), String> {
    gpu_receipt::verify_gpu_receipt(receipt_path)
}

fn cmd_receipt_verify(
    receipt_path: &Path,
    source_override: Option<&Path>,
    expected_profile: Option<&str>,
    expected_policy_digest: Option<&str>,
    json: bool,
) -> Result<(), i32> {
    if json {
        return cmd_receipt_verify_json(
            receipt_path,
            source_override,
            expected_profile,
            expected_policy_digest,
        );
    }

    let receipt: serde_json::Value =
        read_json(receipt_path).map_err(|code| receipt_load_failure(false, "MALFORMED", code))?;

    // GPU cross-check receipts (Layer C) nest their schema under `/body/schema`
    // and carry a top-level seal. Verification is pure JSON + SHA-256 (no
    // Vulkan), so it works in the default build. Detect and route before the
    // flat-schema lookup below.
    if receipt.pointer("/body/schema").and_then(|v| v.as_str()) == Some("buildlang.gpu-receipt/v0")
    {
        return match gpu_receipt_verify(receipt_path) {
            Ok(()) => {
                println!("gpu receipt: VERIFIED (seal intact, gpu-cpu agreement re-checked PASS)");
                Ok(())
            }
            Err(msg) => {
                eprintln!("gpu receipt: FAILED\n  {msg}");
                Err(1)
            }
        };
    }

    let schema = receipt_field_str(&receipt, "/schema", "schema")
        .map_err(|code| receipt_load_failure(false, "SCHEMA_UNSUPPORTED", code))?;
    if schema == SCIENTIFIC_RUNTIME_SCHEMA {
        return verify_scientific_receipt_dispatch(&receipt, source_override, false);
    }
    if schema == MODEL_RECEIPT_SCHEMA {
        return verify_model_boundary_receipt(&receipt, false);
    }
    if schema == TOOL_RECEIPT_SCHEMA {
        return verify_tool_call_receipt(&receipt, false);
    }
    if schema != "buildlang-check-receipt/v1" {
        eprintln!("Error: unsupported check receipt schema `{}`", schema);
        return Err(1);
    }
    let compiler = receipt_field_str(&receipt, "/compiler", "compiler")?;
    if compiler != "buildc" {
        eprintln!(
            "Error: receipt compiler mismatch: expected buildc, got {}",
            compiler
        );
        return Err(1);
    }
    let compiler_version = receipt_field_str(&receipt, "/compiler_version", "compiler_version")?;
    if compiler_version != env!("CARGO_PKG_VERSION") {
        eprintln!(
            "Error: compiler version mismatch: expected {}, actual {}",
            compiler_version,
            env!("CARGO_PKG_VERSION")
        );
        return Err(1);
    }
    let language_version = receipt_field_str(&receipt, "/language_version", "language_version")?;
    let current_language_version = language_version_string();
    if language_version != current_language_version {
        eprintln!(
            "Error: language version mismatch: expected {}, actual {}",
            language_version, current_language_version
        );
        return Err(1);
    }
    verify_receipt_expected_profile(&receipt, expected_profile)?;
    verify_receipt_expected_policy_digest(&receipt, expected_policy_digest)?;

    let source_path = if let Some(source_override) = source_override {
        source_override.to_path_buf()
    } else {
        PathBuf::from(receipt_field_str(&receipt, "/source", "source")?)
    };
    let current = run_check(&source_path)?;
    verify_receipt_digest(
        &receipt,
        "/source_digest",
        "source digest",
        &current.source_digest,
    )?;
    verify_receipt_digest(
        &receipt,
        "/input_graph_digest",
        "input graph digest",
        &current.input_graph_digest,
    )?;

    if let Some(policy_source) = receipt
        .pointer("/policy/source")
        .and_then(serde_json::Value::as_str)
    {
        let expected_hex =
            receipt_digest_hex(&receipt, "/policy/source_digest", "policy source digest")?;
        let actual = current_policy_source_digest(policy_source)?;
        if !actual.hex.eq_ignore_ascii_case(expected_hex) {
            eprintln!(
                "Error: policy source digest mismatch for '{}': expected sha256:{}, actual sha256:{}",
                policy_source, expected_hex, actual.hex
            );
            return Err(1);
        }
    }

    if let Some(profile) = receipt
        .pointer("/policy/profile")
        .and_then(serde_json::Value::as_str)
    {
        let expected_hex =
            receipt_digest_hex(&receipt, "/policy/profile_digest", "policy profile digest")?;
        let actual = builtin_policy_digest(profile).ok_or_else(|| {
            eprintln!("Error: unknown built-in policy profile `{}`", profile);
            1
        })?;
        if !actual.hex.eq_ignore_ascii_case(expected_hex) {
            eprintln!(
                "Error: built-in policy profile digest mismatch for '{}': expected sha256:{}, actual sha256:{}",
                profile, expected_hex, actual.hex
            );
            return Err(1);
        }
    }

    let current_receipt = current_replayed_receipt_value(&receipt, &current)?;
    verify_receipt_replay_fields(&receipt, &current_receipt)?;

    println!("Receipt verified: {}", receipt_path.display());
    Ok(())
}

/// Classify a LOAD-stage receipt-verify failure (unreadable file, invalid or
/// duplicate-key JSON, missing `/schema`) with a machine-readable class. Fires
/// before schema dispatch, so the report is schema-agnostic: no schema tag is
/// claimed for a document whose schema could not be established. Verdict-stage
/// classes live in `scientific_runtime::verify_failure_class`.
fn receipt_load_failure(json: bool, failure_class: &str, code: i32) -> i32 {
    eprintln!("failure_class: {failure_class}");
    if json {
        let report = serde_json::json!({
            "status": "failed",
            "failure_class": failure_class,
        });
        if let Ok(text) = serde_json::to_string_pretty(&report) {
            println!("{text}");
        }
    }
    code
}

fn cmd_receipt_verify_json(
    receipt_path: &Path,
    source_override: Option<&Path>,
    expected_profile: Option<&str>,
    expected_policy_digest: Option<&str>,
) -> Result<(), i32> {
    let receipt: serde_json::Value =
        read_json(receipt_path).map_err(|code| receipt_load_failure(true, "MALFORMED", code))?;

    // Route the scientific-runtime schema to its own re-run verifier BEFORE the
    // check-receipt schema guard, so the existing check-receipt path stays
    // byte-identical.
    if receipt_field_str(&receipt, "/schema", "schema")
        .map_err(|code| receipt_load_failure(true, "SCHEMA_UNSUPPORTED", code))?
        == SCIENTIFIC_RUNTIME_SCHEMA
    {
        return verify_scientific_receipt_dispatch(&receipt, source_override, true);
    }

    // Model boundary receipts (design:
    // docs/superpowers/specs/2026-07-29-model-boundary-receipts-design.md):
    // offline schema/seal/field-contract verification only, no re-run. Routed
    // the same way as the scientific-runtime arm above, before the
    // check-receipt schema guard.
    if receipt_field_str(&receipt, "/schema", "schema")
        .map_err(|code| receipt_load_failure(true, "SCHEMA_UNSUPPORTED", code))?
        == MODEL_RECEIPT_SCHEMA
    {
        return verify_model_boundary_receipt(&receipt, true);
    }

    // Tool-call receipts: offline schema/seal/field-contract verification only,
    // no re-run. Same dispatch pattern, before the check-receipt schema guard.
    if receipt_field_str(&receipt, "/schema", "schema")
        .map_err(|code| receipt_load_failure(true, "SCHEMA_UNSUPPORTED", code))?
        == TOOL_RECEIPT_SCHEMA
    {
        return verify_tool_call_receipt(&receipt, true);
    }

    let mut checks = Vec::new();

    let schema = receipt_field_str(&receipt, "/schema", "schema")?;
    let expected_schema = "buildlang-check-receipt/v1".to_string();
    push_receipt_verification_check(
        &mut checks,
        "schema",
        Some(expected_schema.clone()),
        Some(schema.to_string()),
        None,
        (schema != expected_schema).then(|| "unsupported check receipt schema".to_string()),
    );

    let compiler = receipt_field_str(&receipt, "/compiler", "compiler")?;
    push_receipt_verification_check(
        &mut checks,
        "compiler",
        Some("buildc".to_string()),
        Some(compiler.to_string()),
        None,
        (compiler != "buildc").then(|| "receipt compiler mismatch".to_string()),
    );

    let compiler_version = receipt_field_str(&receipt, "/compiler_version", "compiler_version")?;
    let current_compiler_version = env!("CARGO_PKG_VERSION");
    push_receipt_verification_check(
        &mut checks,
        "compiler_version",
        Some(compiler_version.to_string()),
        Some(current_compiler_version.to_string()),
        None,
        (compiler_version != current_compiler_version)
            .then(|| "compiler version mismatch".to_string()),
    );

    let language_version = receipt_field_str(&receipt, "/language_version", "language_version")?;
    let current_language_version = language_version_string();
    push_receipt_verification_check(
        &mut checks,
        "language_version",
        Some(language_version.to_string()),
        Some(current_language_version.clone()),
        None,
        (language_version != current_language_version)
            .then(|| "language version mismatch".to_string()),
    );
    push_receipt_expected_profile_check(&mut checks, &receipt, expected_profile)?;
    push_receipt_expected_policy_digest_check(&mut checks, &receipt, expected_policy_digest);

    let source_path = if let Some(source_override) = source_override {
        source_override.to_path_buf()
    } else {
        PathBuf::from(receipt_field_str(&receipt, "/source", "source")?)
    };
    let current = run_check(&source_path)?;

    let expected_source_digest = receipt_digest_hex(&receipt, "/source_digest", "source digest")?;
    let actual_source_digest = digest_label(&current.source_digest);
    push_receipt_verification_check(
        &mut checks,
        "source_digest",
        Some(format!("sha256:{expected_source_digest}")),
        Some(actual_source_digest),
        None,
        (!current
            .source_digest
            .hex
            .eq_ignore_ascii_case(expected_source_digest))
        .then(|| "source digest mismatch".to_string()),
    );

    let expected_graph_digest =
        receipt_digest_hex(&receipt, "/input_graph_digest", "input graph digest")?;
    let actual_graph_digest = digest_label(&current.input_graph_digest);
    push_receipt_verification_check(
        &mut checks,
        "input_graph_digest",
        Some(format!("sha256:{expected_graph_digest}")),
        Some(actual_graph_digest),
        None,
        (!current
            .input_graph_digest
            .hex
            .eq_ignore_ascii_case(expected_graph_digest))
        .then(|| "input graph digest mismatch".to_string()),
    );

    if let Some(policy_source) = receipt
        .pointer("/policy/source")
        .and_then(serde_json::Value::as_str)
    {
        let expected_policy_digest =
            receipt_digest_hex(&receipt, "/policy/source_digest", "policy source digest")?;
        let actual_policy_digest = current_policy_source_digest(policy_source)?;
        push_receipt_verification_check(
            &mut checks,
            "policy_source_digest",
            Some(format!("sha256:{expected_policy_digest}")),
            Some(digest_label(&actual_policy_digest)),
            None,
            (!actual_policy_digest
                .hex
                .eq_ignore_ascii_case(expected_policy_digest))
            .then(|| "policy source digest mismatch".to_string()),
        );
    }

    if let Some(profile) = receipt
        .pointer("/policy/profile")
        .and_then(serde_json::Value::as_str)
    {
        let expected_profile_digest =
            receipt_digest_hex(&receipt, "/policy/profile_digest", "policy profile digest")?;
        let actual_profile_digest = builtin_policy_digest(profile).ok_or_else(|| {
            eprintln!("Error: unknown built-in policy profile `{}`", profile);
            1
        })?;
        push_receipt_verification_check(
            &mut checks,
            "policy_profile_digest",
            Some(format!("sha256:{expected_profile_digest}")),
            Some(digest_label(&actual_profile_digest)),
            Some(profile.to_string()),
            (!actual_profile_digest
                .hex
                .eq_ignore_ascii_case(expected_profile_digest))
            .then(|| "built-in policy profile digest mismatch".to_string()),
        );
    }

    let current_receipt = current_replayed_receipt_value(&receipt, &current)?;
    push_receipt_replay_checks(&mut checks, &receipt, &current_receipt);

    let passed = checks.iter().all(|check| check.status == "passed");
    let report = ReceiptVerificationReport {
        schema: "buildlang-receipt-verification/v1",
        receipt: receipt_path.to_string_lossy().to_string(),
        source: source_path.to_string_lossy().to_string(),
        status: if passed { "passed" } else { "failed" },
        checks,
    };
    let json = serde_json::to_string_pretty(&report).map_err(|err| {
        eprintln!(
            "Error serializing receipt verification report '{}': {}",
            receipt_path.display(),
            err
        );
        1
    })?;
    println!("{}", json);
    if passed {
        Ok(())
    } else {
        Err(1)
    }
}

fn cmd_corpus(command: CorpusCommands) -> Result<(), i32> {
    match command {
        CorpusCommands::Verify { root, write } => cmd_corpus_verify(root.as_deref(), write),
    }
}

fn cmd_corpus_verify(root: Option<&Path>, write: bool) -> Result<(), i32> {
    let corpus_root = match root {
        Some(path) => {
            if !path.join("manifest.json").is_file() {
                eprintln!(
                    "semantic corpus manifest not found at {}",
                    path.join("manifest.json").display()
                );
                return Err(1);
            }
            path.to_path_buf()
        }
        None => find_semantic_corpus_root().ok_or_else(|| {
            eprintln!(
                "semantic corpus not found; run from the repository or install semantic-corpus/"
            );
            1
        })?,
    };

    let manifest_path = corpus_root.join("manifest.json");
    let manifest: SemanticCorpusManifest = read_json(&manifest_path)?;
    if manifest.schema != "buildlang-semantic-corpus/v1" {
        eprintln!(
            "semantic corpus manifest has unsupported schema '{}'",
            manifest.schema
        );
        return Err(1);
    }

    let receipts_dir = corpus_root.join("receipts");
    let c_receipt_path = receipts_dir.join("c-execution-2026-06-13.json");
    let rust_receipt_path = receipts_dir.join("rust-execution-2026-06-13.json");
    let substrate_receipt_path = receipts_dir.join("substrate-semantic-corpus-2026-06-18.json");
    let mir_receipt_path = receipts_dir.join(MIR_REPRESENTATION_RECEIPT);
    let memory_receipt_path = receipts_dir.join(MEMORY_LAYOUT_RECEIPT);
    let module_receipt_path = receipts_dir.join(MODULE_GRAPH_RECEIPT);
    let symbol_receipt_path = receipts_dir.join(SYMBOL_GRAPH_RECEIPT);
    let lsp_receipt_path = receipts_dir.join(LSP_DISPATCH_RECEIPT);

    if write {
        refresh_representation_receipts(
            &corpus_root,
            &manifest,
            &mir_receipt_path,
            &memory_receipt_path,
            &module_receipt_path,
            &symbol_receipt_path,
            &lsp_receipt_path,
        )?;
    }

    let substrate_receipt: SubstrateReceipt = read_json(&substrate_receipt_path)?;
    let mir_receipt: MirRepresentationReceipt = read_json(&mir_receipt_path)?;
    let memory_receipt: MemoryLayoutReceipt = read_json(&memory_receipt_path)?;
    let module_receipt: ModuleGraphReceipt = read_json(&module_receipt_path)?;
    let symbol_receipt: SymbolGraphReceipt = read_json(&symbol_receipt_path)?;
    let lsp_receipt: LspDispatchReceipt = read_json(&lsp_receipt_path)?;
    verify_substrate_receipt(&corpus_root, &substrate_receipt, &manifest)?;
    verify_mir_representation_receipt(&corpus_root, &mir_receipt, &manifest)?;
    verify_memory_layout_receipt(&corpus_root, &memory_receipt, &manifest)?;
    verify_module_graph_receipt(&corpus_root, &module_receipt, &manifest)?;
    verify_symbol_graph_receipt(&corpus_root, &symbol_receipt, &manifest)?;
    verify_lsp_dispatch_receipt(&corpus_root, &lsp_receipt, &manifest)?;
    // Re-derive the capability facts from program source ONCE per run; both
    // execution receipts are then verified against this fresh derivation, and
    // the manifest's per-program surfaces must agree with the derivation.
    let derived_capabilities = derive_corpus_capabilities(&corpus_root, &manifest)?;
    verify_manifest_surfaces_match_derivation(&manifest, &derived_capabilities)?;

    let c_passed = if write {
        let rust_receipt: CorpusExecutionReceipt = read_json(&rust_receipt_path)?;
        verify_receipt(
            "rust",
            &rust_receipt,
            &manifest,
            &derived_capabilities,
            manifest.programs.len() + 1,
        )?;

        let c_passed = verify_c_corpus_stdout(&corpus_root, &manifest)?;
        let c_receipt: CorpusExecutionReceipt = read_json(&c_receipt_path)?;
        let c_receipt =
            refresh_c_receipt_from_manifest(c_receipt, &manifest, &derived_capabilities, c_passed);
        // Verify the refreshed receipt IN MEMORY before persisting: a failing
        // verifier run must not mutate the artifact it just rejected. Only a
        // receipt that verifies is written, then read back and re-verified as
        // a serialization round-trip check.
        verify_receipt("c", &c_receipt, &manifest, &derived_capabilities, c_passed)?;
        write_json(&c_receipt_path, &c_receipt)?;

        let c_receipt: CorpusExecutionReceipt = read_json(&c_receipt_path)?;
        verify_receipt("c", &c_receipt, &manifest, &derived_capabilities, c_passed)?;
        c_passed
    } else {
        let c_receipt: CorpusExecutionReceipt = read_json(&c_receipt_path)?;
        let rust_receipt: CorpusExecutionReceipt = read_json(&rust_receipt_path)?;

        verify_receipt(
            "c",
            &c_receipt,
            &manifest,
            &derived_capabilities,
            manifest.programs.len(),
        )?;
        verify_receipt(
            "rust",
            &rust_receipt,
            &manifest,
            &derived_capabilities,
            manifest.programs.len() + 1,
        )?;
        verify_c_corpus_stdout(&corpus_root, &manifest)?
    };

    println!("Semantic Corpus Verify");
    println!("manifest: {} program(s)", manifest.programs.len());
    println!("c receipt: ok");
    println!("rust receipt: ok");
    println!("substrate receipt: ok");
    println!("mir representation receipt: ok");
    println!("memory layout receipt: ok");
    println!("module graph receipt: ok");
    println!("symbol graph receipt: ok");
    println!("lsp dispatch receipt: ok");
    println!("c execution: {} passed", c_passed);
    if write {
        println!("c receipt: written");
        println!("representation receipts: written");
    }
    Ok(())
}

/// Regenerate the representation receipts (mir, memory layout, module graph,
/// symbol graph, lsp dispatch) from current corpus source and write them back to
/// disk. This is the sanctioned `--write` refresh mode: each receipt is rebuilt
/// through the same `build_*` builder the verifier uses, so the written receipt
/// is self-consistent with the current source rather than a hand-edited digest.
#[allow(clippy::too_many_arguments)]
fn refresh_representation_receipts(
    corpus_root: &Path,
    manifest: &SemanticCorpusManifest,
    mir_receipt_path: &Path,
    memory_receipt_path: &Path,
    module_receipt_path: &Path,
    symbol_receipt_path: &Path,
    lsp_receipt_path: &Path,
) -> Result<(), i32> {
    let mir_receipt = mir_representation::build_mir_representation_receipt(corpus_root, manifest)
        .map_err(report_corpus_error)?;
    write_json(mir_receipt_path, &mir_receipt)?;

    let memory_receipt = memory_layout::build_memory_layout_receipt(corpus_root, manifest)
        .map_err(report_corpus_error)?;
    write_json(memory_receipt_path, &memory_receipt)?;

    let module_receipt = module_graph::build_module_graph_receipt(corpus_root, manifest)
        .map_err(report_corpus_error)?;
    write_json(module_receipt_path, &module_receipt)?;

    let symbol_receipt = symbol_graph::build_symbol_graph_receipt(corpus_root, manifest)
        .map_err(report_corpus_error)?;
    write_json(symbol_receipt_path, &symbol_receipt)?;

    let lsp_receipt = lsp_dispatch::build_lsp_dispatch_receipt(corpus_root, manifest)
        .map_err(report_corpus_error)?;
    write_json(lsp_receipt_path, &lsp_receipt)?;

    Ok(())
}

fn report_corpus_error(message: String) -> i32 {
    eprintln!("{message}");
    1
}

fn find_semantic_corpus_root() -> Option<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(cwd) = std::env::current_dir() {
        for ancestor in cwd.ancestors() {
            candidates.push(ancestor.join("semantic-corpus"));
        }
    }

    candidates.push(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")))
            .join("semantic-corpus"),
    );

    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            for ancestor in parent.ancestors() {
                candidates.push(ancestor.join("semantic-corpus"));
            }
        }
    }

    candidates
        .into_iter()
        .find(|path| path.join("manifest.json").is_file())
}

/// Reject JSON text containing a duplicate object key anywhere in the tree.
///
/// serde_json's default map handling is last-duplicate-wins, which is a
/// seal-forgery vector for receipts: a document with two `receipt_status`
/// keys can show one value to a hasher and another to a reader depending on
/// which parse discipline each uses. Receipts (and every other JSON artifact
/// buildc verifies) must therefore be loaded through this strict probe first.
/// Non-finite literals (`NaN`, `Infinity`) are already invalid JSON to
/// serde_json's parser and need no extra handling here.
fn assert_no_duplicate_json_keys(text: &str) -> Result<(), String> {
    use serde::de::{DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};
    use std::fmt;

    struct Probe;

    impl<'de> DeserializeSeed<'de> for Probe {
        type Value = ();
        fn deserialize<D>(self, deserializer: D) -> Result<(), D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_any(Probe)
        }
    }

    impl<'de> Visitor<'de> for Probe {
        type Value = ();

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("any JSON value without duplicate object keys")
        }

        fn visit_bool<E>(self, _: bool) -> Result<(), E> {
            Ok(())
        }
        fn visit_i64<E>(self, _: i64) -> Result<(), E> {
            Ok(())
        }
        fn visit_u64<E>(self, _: u64) -> Result<(), E> {
            Ok(())
        }
        fn visit_f64<E>(self, _: f64) -> Result<(), E> {
            Ok(())
        }
        fn visit_str<E>(self, _: &str) -> Result<(), E> {
            Ok(())
        }
        fn visit_unit<E>(self) -> Result<(), E> {
            Ok(())
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<(), A::Error>
        where
            A: SeqAccess<'de>,
        {
            while seq.next_element_seed(Probe)?.is_some() {}
            Ok(())
        }

        fn visit_map<A>(self, mut map: A) -> Result<(), A::Error>
        where
            A: MapAccess<'de>,
        {
            let mut seen: HashSet<String> = HashSet::new();
            while let Some(key) = map.next_key::<String>()? {
                if !seen.insert(key.clone()) {
                    return Err(serde::de::Error::custom(format!(
                        "duplicate object key `{key}`"
                    )));
                }
                map.next_value_seed(Probe)?;
            }
            Ok(())
        }
    }

    let mut de = serde_json::Deserializer::from_str(text);
    Probe.deserialize(&mut de).map_err(|err| err.to_string())?;
    de.end().map_err(|err| err.to_string())
}

fn read_json_quiet<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|err| format!("failed to read {}: {}", path.display(), err))?;
    assert_no_duplicate_json_keys(&content)
        .map_err(|err| format!("failed to parse {}: {}", path.display(), err))?;
    serde_json::from_str(&content)
        .map_err(|err| format!("failed to parse {}: {}", path.display(), err))
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, i32> {
    read_json_quiet(path).map_err(|message| {
        eprintln!("{message}");
        1
    })
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), i32> {
    let json = serde_json::to_string_pretty(value).map_err(|err| {
        eprintln!("failed to serialize {}: {}", path.display(), err);
        1
    })?;
    std::fs::write(path, format!("{}\n", json)).map_err(|err| {
        eprintln!("failed to write {}: {}", path.display(), err);
        1
    })
}

fn validate_non_empty(value: &str, field: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("substrate {field} must not be empty"));
    }
    Ok(())
}

fn is_lexically_invalid_substrate_relative_path(relative: &str) -> bool {
    if relative.starts_with('\\') {
        return true;
    }

    let bytes = relative.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn validate_substrate_path(root: &Path, relative: &str, field: &str) -> Result<PathBuf, String> {
    validate_non_empty(relative, field)?;
    if is_lexically_invalid_substrate_relative_path(relative) {
        return Err(format!(
            "substrate {field} must stay within corpus root: {}",
            relative
        ));
    }
    let relative_path = Path::new(relative);
    if relative_path.is_absolute()
        || relative_path.has_root()
        || relative_path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(format!(
            "substrate {field} must stay within corpus root: {}",
            relative
        ));
    }
    let canonical_root = root.canonicalize().map_err(|err| {
        format!(
            "substrate {field} failed to canonicalize corpus root {}: {err}",
            root.display()
        )
    })?;
    let path = root.join(relative_path);
    if !path.is_file() {
        return Err(format!(
            "substrate {field} path not found: {}",
            path.display()
        ));
    }
    let canonical_path = path.canonicalize().map_err(|err| {
        format!(
            "substrate {field} failed to canonicalize path {}: {err}",
            path.display()
        )
    })?;
    if !canonical_path.starts_with(&canonical_root) {
        return Err(format!(
            "substrate {field} must stay within corpus root: {}",
            relative
        ));
    }
    Ok(canonical_path)
}

fn receipt_has_stdout_validator(receipt: &CorpusExecutionReceipt) -> bool {
    receipt
        .validator_chain
        .iter()
        .any(|entry| entry.eq_ignore_ascii_case("stdout assertion"))
}

/// Map a manifest `surfaces` entry to the capability it implies. This is the
/// full vocabulary (mirroring `types/capabilities.rs`), so growing the corpus
/// beyond Console programs is a manifest DATA change, not a compiler edit.
/// Surfaces with no capability implication (e.g. structural tags used by the
/// representation receipts) map to None.
fn surface_capability(surface: &str) -> Option<&'static str> {
    match surface {
        "stdout" | "stdin" | "console" => Some("Console"),
        "filesystem" => Some("FileSystem"),
        "network" => Some("Network"),
        "process" => Some("Process"),
        "environment" => Some("Environment"),
        "clock" => Some("Clock"),
        "foreign" => Some("Foreign"),
        "gpu" => Some("Gpu"),
        _ => None,
    }
}

/// The capability facts RE-DERIVED from corpus program SOURCE through the real
/// type checker. This is what makes the corpus capability gate a verifier that
/// can FAIL: the stored `declared_effects` / `observed_capabilities` /
/// `capability_gate` fields are compared against a fresh derivation, not merely
/// echoed back from the manifest (an author-supplied "passed" stamp that verify
/// only string-compares is a self-confirming loop and proves nothing).
struct DerivedCorpusCapabilities {
    /// Union of `~ Effect` declarations across every corpus program function.
    declared: Vec<String>,
    /// Union of checker-observed capability names across every corpus program.
    observed: Vec<String>,
    /// Checker-observed capability set PER PROGRAM (id, capabilities). Kept
    /// alongside the unions so the manifest cross-check can be per-program: a
    /// union-only comparison cannot see one program's surface entry deleted
    /// while another program still contributes the same capability.
    per_program: Vec<(String, BTreeSet<String>)>,
}

/// Run the actual check pipeline over every corpus program and union the
/// resulting effect/capability facts. Fails when any program no longer checks
/// cleanly (a program whose capability surface cannot be derived cannot back a
/// capability claim).
fn derive_corpus_capabilities(
    corpus_root: &Path,
    manifest: &SemanticCorpusManifest,
) -> Result<DerivedCorpusCapabilities, i32> {
    let mut declared = BTreeSet::new();
    let mut observed = BTreeSet::new();
    let mut per_program = Vec::new();
    for program in &manifest.programs {
        let path = corpus_root.join(&program.path);
        let outcome = run_check(&path)?;
        if !outcome.parse_errors.is_empty() || !outcome.type_errors.is_empty() {
            eprintln!(
                "corpus capability derivation failed: program '{}' does not check cleanly",
                program.id
            );
            return Err(1);
        }
        let mut program_observed = BTreeSet::new();
        for summary in &outcome.function_summaries {
            for effect in &summary.declared_effects {
                declared.insert(effect.clone());
            }
            for capability in summary.observed_capabilities.keys() {
                observed.insert(capability.clone());
                program_observed.insert(capability.clone());
            }
        }
        per_program.push((program.id.clone(), program_observed));
    }
    Ok(DerivedCorpusCapabilities {
        declared: declared.into_iter().collect(),
        observed: observed.into_iter().collect(),
        per_program,
    })
}

/// Cross-check the checker derivation against the manifest's declared surfaces
/// PER PROGRAM: every capability a program's source observes must be declared
/// by one of that program's manifest surfaces, and every capability-bearing
/// surface must be backed by the source actually observing that capability.
/// Runs once per corpus-verify invocation (it relates the manifest to the
/// derivation, not to any single receipt).
fn verify_manifest_surfaces_match_derivation(
    manifest: &SemanticCorpusManifest,
    derived: &DerivedCorpusCapabilities,
) -> Result<(), i32> {
    for (program, (derived_id, program_observed)) in
        manifest.programs.iter().zip(derived.per_program.iter())
    {
        debug_assert_eq!(&program.id, derived_id);
        let surface_caps: BTreeSet<String> = program
            .surfaces
            .iter()
            .filter_map(|surface| surface_capability(surface).map(str::to_string))
            .collect();
        if surface_caps != *program_observed {
            eprintln!(
                "corpus manifest surface drift for program '{}': manifest surfaces imply capabilities {:?}, checker derives {:?} (the surface->capability map lives in surface_capability(); extend it when a new capability class joins the corpus)",
                program.id, surface_caps, program_observed
            );
            return Err(1);
        }
    }
    Ok(())
}

fn apply_capability_receipt_metadata(
    receipt: &mut CorpusExecutionReceipt,
    derived: &DerivedCorpusCapabilities,
) {
    // Stamp the DERIVED facts (from the checker over program source), not a
    // manifest echo. The gate is "passed" only on this path, which is reached
    // only after `derive_corpus_capabilities` succeeded.
    receipt.declared_effects = derived.declared.clone();
    receipt.observed_capabilities = derived.observed.clone();
    receipt.capability_gate = Some("passed".to_string());
    receipt.capability_gate_test =
        Some("cargo test --manifest-path compiler/Cargo.toml capability --quiet".to_string());
}

fn refresh_c_receipt_from_manifest(
    mut receipt: CorpusExecutionReceipt,
    manifest: &SemanticCorpusManifest,
    derived: &DerivedCorpusCapabilities,
    passed: usize,
) -> CorpusExecutionReceipt {
    receipt.result.passed = passed;
    receipt.result.failed = 0;
    receipt.result.ignored = 0;
    receipt.programs = manifest
        .programs
        .iter()
        .map(|program| CorpusExecutionProgram {
            id: program.id.clone(),
            path: format!("../{}", program.path),
            expected_stdout: program.expected_stdout.clone(),
        })
        .collect();
    apply_capability_receipt_metadata(&mut receipt, derived);
    receipt
}

fn verify_receipt(
    label: &str,
    receipt: &CorpusExecutionReceipt,
    manifest: &SemanticCorpusManifest,
    derived: &DerivedCorpusCapabilities,
    expected_passed: usize,
) -> Result<(), i32> {
    if receipt.backend != label {
        eprintln!(
            "{} receipt backend mismatch: expected '{}', found '{}'",
            label, label, receipt.backend
        );
        return Err(1);
    }
    if receipt.result.failed != 0 || receipt.result.ignored != 0 {
        eprintln!(
            "{} receipt is not clean: {} failed, {} ignored",
            label, receipt.result.failed, receipt.result.ignored
        );
        return Err(1);
    }
    if receipt.result.passed != expected_passed {
        eprintln!(
            "{} receipt pass count mismatch: expected {}, found {}",
            label, expected_passed, receipt.result.passed
        );
        return Err(1);
    }
    if receipt.programs.len() != manifest.programs.len() {
        eprintln!(
            "{} receipt program count mismatch: expected {}, found {}",
            label,
            manifest.programs.len(),
            receipt.programs.len()
        );
        return Err(1);
    }

    for (manifest_program, receipt_program) in manifest.programs.iter().zip(receipt.programs.iter())
    {
        let receipt_path = receipt_program.path.trim_start_matches("../");
        if receipt_program.id != manifest_program.id
            || receipt_path != manifest_program.path
            || receipt_program.expected_stdout != manifest_program.expected_stdout
        {
            eprintln!(
                "{} receipt drift for program '{}'",
                label, manifest_program.id
            );
            return Err(1);
        }
    }

    // The stored capability facts must match a FRESH derivation from program
    // source through the type checker (computed once per corpus-verify run by
    // `derive_corpus_capabilities`). This is genuine re-derivation: editing the
    // stored fields OR changing a program's capability surface makes it fail.
    // Previously this block compared the stored fields against a manifest echo
    // and an unconditional "passed" stamp, which could not fail for a receipt
    // produced by the writer: a self-confirming loop.
    if receipt.declared_effects != derived.declared
        || receipt.observed_capabilities != derived.observed
        || receipt.capability_gate.as_deref() != Some("passed")
        || receipt.capability_gate_test.as_deref()
            != Some("cargo test --manifest-path compiler/Cargo.toml capability --quiet")
    {
        eprintln!(
            "{} receipt capability metadata drift: stored declared={:?} observed={:?}, checker-derived declared={:?} observed={:?}",
            label,
            receipt.declared_effects,
            receipt.observed_capabilities,
            derived.declared,
            derived.observed
        );
        if label == "rust" {
            // The rust execution receipt has NO tool-supported writer: it is
            // hand-maintained, and its capability fields are additionally
            // pinned by unit tests. Say so, or a legitimate capability-surface
            // change dead-ends here with no path forward.
            eprintln!(
                "note: the rust execution receipt is hand-maintained (semantic-corpus/receipts/rust-execution-*.json); update it and the pinned assertions in compiler/src/codegen/backend/rust.rs alongside any legitimate capability change"
            );
        }
        return Err(1);
    }

    Ok(())
}

fn validate_substrate_receipt(
    corpus_root: &Path,
    receipt: &SubstrateReceipt,
    manifest: &SemanticCorpusManifest,
) -> Result<(), String> {
    if receipt.schema != "buildlang-substrate-receipt/v0" {
        return Err(format!(
            "substrate receipt has unsupported schema '{}'",
            receipt.schema
        ));
    }
    if receipt.compiler != "buildc" {
        return Err(format!(
            "substrate compiler mismatch: expected 'buildc', found '{}'",
            receipt.compiler
        ));
    }
    if receipt.language != "buildlang" {
        return Err(format!(
            "substrate language mismatch: expected 'buildlang', found '{}'",
            receipt.language
        ));
    }
    validate_non_empty(&receipt.receipt_id, "receipt_id")?;
    validate_non_empty(&receipt.created_at, "created_at")?;

    if receipt.source_set.kind != "semantic-corpus" {
        return Err(format!(
            "substrate source_set.kind mismatch: expected 'semantic-corpus', found '{}'",
            receipt.source_set.kind
        ));
    }
    let manifest_path = validate_substrate_path(
        corpus_root,
        &receipt.source_set.manifest,
        "source_set.manifest",
    )?;
    let expected_manifest_path =
        corpus_root
            .join("manifest.json")
            .canonicalize()
            .map_err(|err| {
                format!(
            "substrate source_set.manifest failed to canonicalize expected manifest {}: {err}",
            corpus_root.join("manifest.json").display()
        )
            })?;
    if manifest_path != expected_manifest_path {
        return Err(format!(
            "substrate source_set.manifest must point at manifest.json, found {}",
            receipt.source_set.manifest
        ));
    }
    if receipt.source_set.program_count != manifest.programs.len() {
        return Err(format!(
            "substrate source_set.program_count mismatch: expected {}, found {}",
            manifest.programs.len(),
            receipt.source_set.program_count
        ));
    }

    if receipt.semantic_surface.check_receipt_schema != "buildlang-check-receipt/v1" {
        return Err(format!(
            "substrate semantic_surface.check_receipt_schema mismatch: found '{}'",
            receipt.semantic_surface.check_receipt_schema
        ));
    }
    if !receipt.semantic_surface.requires_source_digest {
        return Err("substrate semantic_surface.requires_source_digest must be true".to_string());
    }
    if !receipt.semantic_surface.requires_input_graph_digest {
        return Err(
            "substrate semantic_surface.requires_input_graph_digest must be true".to_string(),
        );
    }
    for required in [
        "declared_effects",
        "observed_capabilities",
        "propagated_effects",
    ] {
        if !receipt
            .semantic_surface
            .effect_surfaces
            .iter()
            .any(|surface| surface == required)
        {
            return Err(format!(
                "substrate semantic_surface.effect_surfaces missing {required}"
            ));
        }
    }

    if receipt.execution_surface.is_empty() {
        return Err("substrate execution_surface must not be empty".to_string());
    }
    for required in ["c", "rust", "spirv"] {
        if !receipt.execution_surface.contains_key(required) {
            return Err(format!(
                "substrate execution_surface missing required target {required}"
            ));
        }
    }
    for (label, target) in &receipt.execution_surface {
        validate_non_empty(&target.target, &format!("execution_surface.{label}.target"))?;
        validate_non_empty(
            &target.maturity,
            &format!("execution_surface.{label}.maturity"),
        )?;
        validate_non_empty(
            &target.evidence_class,
            &format!("execution_surface.{label}.evidence_class"),
        )?;
        let unsupported_mir_policy = target
            .unsupported_mir_policy
            .as_deref()
            .map(str::trim)
            .unwrap_or_default();

        match label.as_str() {
            "c" => {
                if target.target != "c" {
                    return Err(format!(
                        "substrate execution_surface.c target mismatch: expected 'c', found '{}'",
                        target.target
                    ));
                }
                if target.maturity != "production-anchor" {
                    return Err(format!(
                        "substrate execution_surface.c maturity mismatch: expected 'production-anchor', found '{}'",
                        target.maturity
                    ));
                }
                let Some(relative_receipt) = target.receipt.as_deref() else {
                    return Err(
                        "substrate execution_surface.c is production-anchor but receipt is missing"
                            .to_string(),
                    );
                };
                let execution_receipt_path = validate_substrate_path(
                    corpus_root,
                    relative_receipt,
                    "execution_surface.c.receipt",
                )?;
                let execution_receipt: CorpusExecutionReceipt =
                    read_json_quiet(&execution_receipt_path)?;
                if execution_receipt.backend != "c" {
                    return Err(format!(
                        "substrate execution_surface.c.receipt backend mismatch: expected 'c', found '{}'",
                        execution_receipt.backend
                    ));
                }
                if !receipt_has_stdout_validator(&execution_receipt) {
                    return Err(
                        "substrate execution_surface.c production-anchor requires stdout assertion evidence"
                            .to_string(),
                    );
                }
                continue;
            }
            "rust" => {
                if target.target != "rust" {
                    return Err(format!(
                        "substrate execution_surface.rust target mismatch: expected 'rust', found '{}'",
                        target.target
                    ));
                }
                if target.maturity != "experimental-subset" {
                    return Err(format!(
                        "substrate execution_surface.rust maturity mismatch: expected 'experimental-subset', found '{}'",
                        target.maturity
                    ));
                }
                let Some(relative_receipt) = target.receipt.as_deref() else {
                    return Err(
                        "substrate execution_surface.rust experimental-subset requires receipt evidence"
                            .to_string(),
                    );
                };
                let execution_receipt_path = validate_substrate_path(
                    corpus_root,
                    relative_receipt,
                    "execution_surface.rust.receipt",
                )?;
                let execution_receipt: CorpusExecutionReceipt =
                    read_json_quiet(&execution_receipt_path)?;
                if execution_receipt.backend != "rust" {
                    return Err(format!(
                        "substrate execution_surface.rust.receipt backend mismatch: expected 'rust', found '{}'",
                        execution_receipt.backend
                    ));
                }
                if unsupported_mir_policy.is_empty() {
                    return Err(
                        "substrate execution_surface.rust unsupported_mir_policy must not be empty"
                            .to_string(),
                    );
                }
                continue;
            }
            "spirv" => {
                if target.target != "spirv" {
                    return Err(format!(
                        "substrate execution_surface.spirv target mismatch: expected 'spirv', found '{}'",
                        target.target
                    ));
                }
                if !target.maturity.starts_with("experimental") {
                    return Err(format!(
                        "substrate execution_surface.spirv maturity mismatch: expected experimental*, found '{}'",
                        target.maturity
                    ));
                }
                if target.status.as_deref() != Some("unverified")
                    && unsupported_mir_policy.is_empty()
                {
                    return Err(
                        "substrate execution_surface.spirv experimental target requires status=unverified or unsupported_mir_policy"
                            .to_string(),
                    );
                }
                continue;
            }
            _ => {}
        }

        match target.maturity.as_str() {
            "production-anchor" => {
                let Some(relative_receipt) = target.receipt.as_deref() else {
                    return Err(format!(
                        "substrate execution_surface.{label} is production-anchor but receipt is missing"
                    ));
                };
                let execution_receipt_path = validate_substrate_path(
                    corpus_root,
                    relative_receipt,
                    &format!("execution_surface.{label}.receipt"),
                )?;
                let execution_receipt: CorpusExecutionReceipt =
                    read_json_quiet(&execution_receipt_path)?;
                if execution_receipt.backend != target.target {
                    return Err(format!(
                        "substrate execution_surface.{label}.receipt backend mismatch: expected '{}', found '{}'",
                        target.target, execution_receipt.backend
                    ));
                }
                if !receipt_has_stdout_validator(&execution_receipt) {
                    return Err(format!(
                        "substrate execution_surface.{label} production-anchor requires stdout assertion evidence"
                    ));
                }
            }
            "experimental-subset" => {
                if target.receipt.is_none() && unsupported_mir_policy.is_empty() {
                    return Err(format!(
                        "substrate execution_surface.{label} experimental-subset requires receipt or unsupported_mir_policy"
                    ));
                }
                if let Some(relative_receipt) = target.receipt.as_deref() {
                    let execution_receipt_path = validate_substrate_path(
                        corpus_root,
                        relative_receipt,
                        &format!("execution_surface.{label}.receipt"),
                    )?;
                    let execution_receipt: CorpusExecutionReceipt =
                        read_json_quiet(&execution_receipt_path)?;
                    if execution_receipt.backend != target.target {
                        return Err(format!(
                            "substrate execution_surface.{label}.receipt backend mismatch: expected '{}', found '{}'",
                            target.target, execution_receipt.backend
                        ));
                    }
                }
            }
            maturity if maturity.starts_with("experimental") => {
                if target.status.as_deref() != Some("unverified")
                    && unsupported_mir_policy.is_empty()
                {
                    return Err(format!(
                        "substrate execution_surface.{label} experimental target requires status=unverified or unsupported_mir_policy"
                    ));
                }
            }
            other => {
                return Err(format!(
                    "substrate execution_surface.{label} has unknown maturity '{other}'"
                ));
            }
        }
    }

    validate_non_empty(
        &receipt.memory_surface.ownership_model,
        "memory_surface.ownership_model",
    )?;
    if receipt.memory_surface.known_gaps.is_empty() {
        return Err("substrate memory_surface.known_gaps must not be empty".to_string());
    }
    if receipt.memory_surface.verified_surfaces.is_empty() {
        return Err("substrate memory_surface.verified_surfaces must not be empty".to_string());
    }
    let memory_receipt_path = validate_substrate_path(
        corpus_root,
        &receipt.memory_surface.memory_receipt,
        "memory_surface.memory_receipt",
    )?;
    if memory_receipt_path
        != corpus_root
            .join("receipts")
            .join(MEMORY_LAYOUT_RECEIPT)
            .canonicalize()
            .map_err(|err| {
                format!(
                    "substrate memory_surface.memory_receipt failed to canonicalize expected receipt {}: {err}",
                    corpus_root
                        .join("receipts")
                        .join(MEMORY_LAYOUT_RECEIPT)
                        .display()
                )
            })?
    {
        return Err(format!(
            "substrate memory_surface.memory_receipt must point at receipts/{}, found {}",
            MEMORY_LAYOUT_RECEIPT, receipt.memory_surface.memory_receipt
        ));
    }

    if receipt.representation_surface.ir != "MIR" {
        return Err(format!(
            "substrate representation_surface.ir mismatch: expected 'MIR', found '{}'",
            receipt.representation_surface.ir
        ));
    }
    validate_non_empty(
        &receipt.representation_surface.fallback_policy,
        "representation_surface.fallback_policy",
    )?;
    validate_non_empty(
        &receipt.representation_surface.backend_maturity_descriptor,
        "representation_surface.backend_maturity_descriptor",
    )?;
    let representation_receipt_path = validate_substrate_path(
        corpus_root,
        &receipt.representation_surface.representation_receipt,
        "representation_surface.representation_receipt",
    )?;
    if representation_receipt_path
        != corpus_root
            .join("receipts")
            .join(MIR_REPRESENTATION_RECEIPT)
            .canonicalize()
            .map_err(|err| {
                format!(
                    "substrate representation_surface.representation_receipt failed to canonicalize expected receipt {}: {err}",
                    corpus_root
                        .join("receipts")
                        .join(MIR_REPRESENTATION_RECEIPT)
                        .display()
                )
            })?
    {
        return Err(format!(
            "substrate representation_surface.representation_receipt must point at receipts/{}, found {}",
            MIR_REPRESENTATION_RECEIPT,
            receipt.representation_surface.representation_receipt
        ));
    }

    if receipt.module_surface.resolver != "buildc source input resolver" {
        return Err(format!(
            "substrate module_surface.resolver mismatch: expected 'buildc source input resolver', found '{}'",
            receipt.module_surface.resolver
        ));
    }
    if receipt.module_surface.digest_anchor != "buildlang-check-receipt/v1 input_graph_digest" {
        return Err(format!(
            "substrate module_surface.digest_anchor mismatch: expected 'buildlang-check-receipt/v1 input_graph_digest', found '{}'",
            receipt.module_surface.digest_anchor
        ));
    }
    if receipt.module_surface.known_gaps.is_empty() {
        return Err("substrate module_surface.known_gaps must not be empty".to_string());
    }
    let module_receipt_path = validate_substrate_path(
        corpus_root,
        &receipt.module_surface.module_receipt,
        "module_surface.module_receipt",
    )?;
    if module_receipt_path
        != corpus_root
            .join("receipts")
            .join(MODULE_GRAPH_RECEIPT)
            .canonicalize()
            .map_err(|err| {
                format!(
                    "substrate module_surface.module_receipt failed to canonicalize expected receipt {}: {err}",
                    corpus_root
                        .join("receipts")
                        .join(MODULE_GRAPH_RECEIPT)
                        .display()
                )
            })?
    {
        return Err(format!(
            "substrate module_surface.module_receipt must point at receipts/{}, found {}",
            MODULE_GRAPH_RECEIPT, receipt.module_surface.module_receipt
        ));
    }

    if receipt.symbol_surface.source != "AST" {
        return Err(format!(
            "substrate symbol_surface.source mismatch: expected 'AST', found '{}'",
            receipt.symbol_surface.source
        ));
    }
    if receipt.symbol_surface.representation != "MIR" {
        return Err(format!(
            "substrate symbol_surface.representation mismatch: expected 'MIR', found '{}'",
            receipt.symbol_surface.representation
        ));
    }
    if receipt.symbol_surface.effect_anchor != "buildlang-check-receipt/v1" {
        return Err(format!(
            "substrate symbol_surface.effect_anchor mismatch: expected 'buildlang-check-receipt/v1', found '{}'",
            receipt.symbol_surface.effect_anchor
        ));
    }
    if receipt.symbol_surface.known_gaps.is_empty() {
        return Err("substrate symbol_surface.known_gaps must not be empty".to_string());
    }
    let symbol_receipt_path = validate_substrate_path(
        corpus_root,
        &receipt.symbol_surface.symbol_receipt,
        "symbol_surface.symbol_receipt",
    )?;
    if symbol_receipt_path
        != corpus_root
            .join("receipts")
            .join(SYMBOL_GRAPH_RECEIPT)
            .canonicalize()
            .map_err(|err| {
                format!(
                    "substrate symbol_surface.symbol_receipt failed to canonicalize expected receipt {}: {err}",
                    corpus_root
                        .join("receipts")
                        .join(SYMBOL_GRAPH_RECEIPT)
                        .display()
                )
            })?
    {
        return Err(format!(
            "substrate symbol_surface.symbol_receipt must point at receipts/{}, found {}",
            SYMBOL_GRAPH_RECEIPT, receipt.symbol_surface.symbol_receipt
        ));
    }

    if receipt.lsp_surface.protocol != "LSP JSON-RPC over stdio" {
        return Err(format!(
            "substrate lsp_surface.protocol mismatch: expected 'LSP JSON-RPC over stdio', found '{}'",
            receipt.lsp_surface.protocol
        ));
    }
    if receipt.lsp_surface.dispatch != "buildc lsp raw message dispatch" {
        return Err(format!(
            "substrate lsp_surface.dispatch mismatch: expected 'buildc lsp raw message dispatch', found '{}'",
            receipt.lsp_surface.dispatch
        ));
    }
    if receipt.lsp_surface.request_parser != "serde_json structural JSON-RPC parser" {
        return Err(format!(
            "substrate lsp_surface.request_parser mismatch: expected 'serde_json structural JSON-RPC parser', found '{}'",
            receipt.lsp_surface.request_parser
        ));
    }
    if receipt.lsp_surface.known_gaps.is_empty() {
        return Err("substrate lsp_surface.known_gaps must not be empty".to_string());
    }
    let lsp_receipt_path = validate_substrate_path(
        corpus_root,
        &receipt.lsp_surface.lsp_receipt,
        "lsp_surface.lsp_receipt",
    )?;
    if lsp_receipt_path
        != corpus_root
            .join("receipts")
            .join(LSP_DISPATCH_RECEIPT)
            .canonicalize()
            .map_err(|err| {
                format!(
                    "substrate lsp_surface.lsp_receipt failed to canonicalize expected receipt {}: {err}",
                    corpus_root
                        .join("receipts")
                        .join(LSP_DISPATCH_RECEIPT)
                        .display()
                )
            })?
    {
        return Err(format!(
            "substrate lsp_surface.lsp_receipt must point at receipts/{}, found {}",
            LSP_DISPATCH_RECEIPT, receipt.lsp_surface.lsp_receipt
        ));
    }

    if receipt.evidence_surface.commands.is_empty() {
        return Err("substrate evidence_surface.commands must not be empty".to_string());
    }
    if !receipt
        .evidence_surface
        .commands
        .iter()
        .all(|command| !command.trim().is_empty())
    {
        return Err(
            "substrate evidence_surface.commands must contain only non-empty commands".to_string(),
        );
    }

    Ok(())
}

fn verify_substrate_receipt(
    corpus_root: &Path,
    receipt: &SubstrateReceipt,
    manifest: &SemanticCorpusManifest,
) -> Result<(), i32> {
    validate_substrate_receipt(corpus_root, receipt, manifest).map_err(|message| {
        eprintln!("{message}");
        1
    })
}

fn substrate_invalid_rows() -> Vec<String> {
    vec!["  receipt   invalid  run buildc corpus verify for details".to_string()]
}

fn substrate_missing_rows() -> Vec<String> {
    vec!["  receipt   missing  run buildc corpus verify from a repository checkout".to_string()]
}

fn substrate_target<'a>(
    receipt: &'a SubstrateReceipt,
    target: &str,
) -> Result<&'a SubstrateExecutionTarget, ()> {
    receipt.execution_surface.get(target).ok_or(())
}

fn substrate_evidence_rows(corpus_root: Option<&Path>) -> Vec<String> {
    let Some(corpus_root) = corpus_root else {
        return substrate_missing_rows();
    };
    let manifest_path = corpus_root.join("manifest.json");
    let substrate_receipt_path = corpus_root
        .join("receipts")
        .join("substrate-semantic-corpus-2026-06-18.json");

    if !manifest_path.is_file() || !substrate_receipt_path.is_file() {
        return substrate_missing_rows();
    }

    let manifest: SemanticCorpusManifest = match read_json_quiet(&manifest_path) {
        Ok(manifest) => manifest,
        Err(_) => return substrate_invalid_rows(),
    };
    let receipt: SubstrateReceipt = match read_json_quiet(&substrate_receipt_path) {
        Ok(receipt) => receipt,
        Err(_) => return substrate_invalid_rows(),
    };

    if validate_substrate_receipt(corpus_root, &receipt, &manifest).is_err() {
        return substrate_invalid_rows();
    }

    let Ok(c_target) = substrate_target(&receipt, "c") else {
        return substrate_invalid_rows();
    };
    let Ok(rust_target) = substrate_target(&receipt, "rust") else {
        return substrate_invalid_rows();
    };
    let Ok(spirv_target) = substrate_target(&receipt, "spirv") else {
        return substrate_invalid_rows();
    };

    let c_status = match c_target.maturity.as_str() {
        "production-anchor" => "anchor",
        _ => return substrate_invalid_rows(),
    };
    let rust_status = match rust_target.maturity.as_str() {
        "experimental-subset" => "subset",
        _ => return substrate_invalid_rows(),
    };
    let spirv_status = if spirv_target.status.as_deref() == Some("unverified")
        || !spirv_target
            .unsupported_mir_policy
            .as_deref()
            .map(str::trim)
            .unwrap_or_default()
            .is_empty()
    {
        "unverified"
    } else {
        return substrate_invalid_rows();
    };

    vec![
        format!("  receipt   ok       {}", receipt.schema),
        format!(
            "  corpus    ok       {} semantic program(s)",
            manifest.programs.len()
        ),
        format!("  c         {c_status}   production execution evidence"),
        format!("  rust      {rust_status}   experimental executable subset"),
        format!("  spirv     {spirv_status} explicit unsupported-MIR posture"),
        format!(
            "  memory    partial  {} verified surface(s), {} known gap(s)",
            receipt.memory_surface.verified_surfaces.len(),
            receipt.memory_surface.known_gaps.len()
        ),
        format!(
            "  repr      {}      fallback policy recorded",
            receipt.representation_surface.ir
        ),
    ]
}

fn verify_c_corpus_stdout(
    corpus_root: &Path,
    manifest: &SemanticCorpusManifest,
) -> Result<usize, i32> {
    let buildc = std::env::current_exe().map_err(|err| {
        eprintln!("failed to locate current buildc executable: {}", err);
        1
    })?;

    for program in &manifest.programs {
        let program_path = corpus_root.join(&program.path);
        let output = std::process::Command::new(&buildc)
            .arg("run")
            .arg(&program_path)
            .output()
            .map_err(|err| {
                eprintln!(
                    "failed to run semantic corpus program {}: {}",
                    program.id, err
                );
                1
            })?;

        if !output.status.success() {
            eprintln!(
                "semantic corpus program {} failed\nstdout:\n{}\nstderr:\n{}",
                program.id,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return Err(1);
        }

        let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
        if stdout != program.expected_stdout {
            eprintln!(
                "semantic corpus stdout drift for {}\nexpected:\n{:?}\nactual:\n{:?}",
                program.id, program.expected_stdout, stdout
            );
            return Err(1);
        }
    }

    Ok(manifest.programs.len())
}

fn cmd_lex(file: &PathBuf, verbose: bool) -> Result<(), i32> {
    let source = std::fs::read_to_string(file).map_err(|e| {
        eprintln!("Error reading file '{}': {}", file.display(), e);
        1
    })?;

    // Expand `include!("path")` directives
    let lex_base = file.parent().unwrap_or(Path::new("."));
    let source = preprocess_includes(&source, lex_base)?;

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
                start.line,
                start.column,
                end.line,
                end.column,
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

fn cmd_mir(command: MirCommands) -> Result<(), i32> {
    match command {
        MirCommands::Emit { file, output } => cmd_mir_emit(&file, output.as_deref()),
        MirCommands::Load { file } => cmd_mir_load(&file),
    }
}

fn cmd_mir_emit(file: &Path, output: Option<&Path>) -> Result<(), i32> {
    let lowered = mir_representation::lower_program_to_mir(file).map_err(|message| {
        eprintln!("{message}");
        1
    })?;
    let envelope = buildlang::codegen::MirModuleEnvelope::wrap(&lowered.module);

    if let Some(path) = output {
        write_json(path, &envelope)?;
        println!("Wrote {} MIR to {}", envelope.schema, path.display());
    } else {
        let json = envelope.to_json_pretty().map_err(|err| {
            eprintln!("failed to serialize MIR: {err}");
            1
        })?;
        println!("{json}");
    }
    Ok(())
}

fn cmd_mir_load(file: &Path) -> Result<(), i32> {
    let json = std::fs::read_to_string(file).map_err(|err| {
        eprintln!("Error reading file '{}': {}", file.display(), err);
        1
    })?;
    let envelope = buildlang::codegen::MirModuleEnvelope::from_json(&json).map_err(|message| {
        eprintln!("{message}");
        1
    })?;

    let module = &envelope.module;
    let digest = mir_representation::digest_mir_module(module);
    let defined = module
        .functions
        .iter()
        .filter(|function| !function.is_declaration())
        .count();
    println!("schema: {}", envelope.schema);
    println!("module: {}", module.name);
    println!("mir_digest: {}:{}", digest.algorithm, digest.hex);
    println!(
        "functions: {} ({} defined, {} declarations)",
        module.functions.len(),
        defined,
        module.functions.len() - defined,
    );
    println!("types: {}", module.types.len());
    println!("globals: {}", module.globals.len());
    println!("strings: {}", module.strings.len());
    println!("externals: {}", module.externals.len());
    Ok(())
}

fn cmd_bdf(command: BdfCommands) -> Result<(), i32> {
    match command {
        BdfCommands::Encode { input, output } => cmd_bdf_encode(&input, output.as_deref()),
        BdfCommands::Decode { input, output } => cmd_bdf_decode(&input, output.as_deref()),
        BdfCommands::Validate { file } => cmd_bdf_validate(&file),
        BdfCommands::FromFlagshipAction { input, output } => {
            cmd_bdf_from_flagship_action(&input, output.as_deref())
        }
        BdfCommands::ToFlagshipAction { input, output } => {
            cmd_bdf_to_flagship_action(&input, output.as_deref())
        }
    }
}

fn cmd_bdf_encode(input: &Path, output: Option<&Path>) -> Result<(), i32> {
    use buildlang::bdf::BdfValue;

    let json = std::fs::read_to_string(input).map_err(|err| {
        eprintln!("Error reading file '{}': {}", input.display(), err);
        1
    })?;
    let value = BdfValue::from_json(&json).map_err(|err| {
        eprintln!("Error parsing BDF JSON '{}': {}", input.display(), err);
        1
    })?;
    let bytes = value.to_bytes();

    if let Some(path) = output {
        std::fs::write(path, &bytes).map_err(|err| {
            eprintln!("Error writing '{}': {}", path.display(), err);
            1
        })?;
        let digest = buildlang::bdf::payload_digest_hex(&value);
        println!(
            "Wrote {} byte(s) of {} to {} (sha256:{})",
            bytes.len(),
            buildlang::bdf::BDF_VALUE_SCHEMA,
            path.display(),
            digest
        );
    } else {
        write_stdout_bytes(&bytes)?;
    }
    Ok(())
}

fn cmd_bdf_decode(input: &Path, output: Option<&Path>) -> Result<(), i32> {
    use buildlang::bdf::BdfValue;

    let bytes = std::fs::read(input).map_err(|err| {
        eprintln!("Error reading file '{}': {}", input.display(), err);
        1
    })?;
    let value = BdfValue::from_bytes(&bytes).map_err(|err| {
        eprintln!("Error decoding BDF binary '{}': {}", input.display(), err);
        1
    })?;
    let json = value.to_json_pretty().map_err(|err| {
        eprintln!("Error serializing BDF JSON: {}", err);
        1
    })?;

    if let Some(path) = output {
        std::fs::write(path, format!("{json}\n")).map_err(|err| {
            eprintln!("Error writing '{}': {}", path.display(), err);
            1
        })?;
        println!("Wrote BDF JSON projection to {}", path.display());
    } else {
        println!("{json}");
    }
    Ok(())
}

fn cmd_bdf_validate(file: &Path) -> Result<(), i32> {
    use buildlang::bdf::{BdfValue, BDF_MAGIC, BDF_VALUE_SCHEMA};

    let bytes = std::fs::read(file).map_err(|err| {
        eprintln!("Error reading file '{}': {}", file.display(), err);
        1
    })?;

    // Auto-detect: a binary value stream begins with the BDF magic; otherwise
    // treat the file as the JSON projection.
    let is_binary = bytes.len() >= BDF_MAGIC.len() && bytes[..BDF_MAGIC.len()] == BDF_MAGIC;
    let (form, value) = if is_binary {
        let value = BdfValue::from_bytes(&bytes).map_err(|err| {
            eprintln!(
                "Error: '{}' is not valid BDF binary: {}",
                file.display(),
                err
            );
            1
        })?;
        ("binary", value)
    } else {
        let text = std::str::from_utf8(&bytes).map_err(|_| {
            eprintln!(
                "Error: '{}' is neither BDF binary nor UTF-8 JSON",
                file.display()
            );
            1
        })?;
        let value = BdfValue::from_json(text).map_err(|err| {
            eprintln!("Error: '{}' is not valid BDF JSON: {}", file.display(), err);
            1
        })?;
        ("json", value)
    };

    let digest = buildlang::bdf::payload_digest_hex(&value);
    println!("schema: {BDF_VALUE_SCHEMA}");
    println!("form: {form}");
    println!("kind: {}", bdf_value_kind(&value));
    println!("canonical_bytes: {}", value.to_bytes().len());
    println!("payload_digest: sha256:{digest}");
    println!("status: valid");
    Ok(())
}

fn cmd_bdf_from_flagship_action(input: &Path, output: Option<&Path>) -> Result<(), i32> {
    let json = std::fs::read_to_string(input).map_err(|err| {
        eprintln!("Error reading file '{}': {}", input.display(), err);
        1
    })?;
    let message = buildlang::bdf::flagship_action_to_bdf(&json).map_err(|err| {
        eprintln!(
            "Error bridging flagship-action '{}': {}",
            input.display(),
            err
        );
        1
    })?;
    let bytes = message.to_bytes();

    if let Some(path) = output {
        std::fs::write(path, &bytes).map_err(|err| {
            eprintln!("Error writing '{}': {}", path.display(), err);
            1
        })?;
        println!(
            "Wrote {} byte(s) of {} to {} (payload sha256:{})",
            bytes.len(),
            buildlang::bdf::BDF_MESSAGE_SCHEMA,
            path.display(),
            message.receipt.sha256
        );
    } else {
        write_stdout_bytes(&bytes)?;
    }
    Ok(())
}

fn cmd_bdf_to_flagship_action(input: &Path, output: Option<&Path>) -> Result<(), i32> {
    let bytes = std::fs::read(input).map_err(|err| {
        eprintln!("Error reading file '{}': {}", input.display(), err);
        1
    })?;
    let message = buildlang::bdf::BdfMessage::from_bytes(&bytes).map_err(|err| {
        eprintln!("Error decoding BDF message '{}': {}", input.display(), err);
        1
    })?;
    let json = buildlang::bdf::bdf_to_flagship_action_pretty(&message).map_err(|err| {
        eprintln!(
            "Error reconstructing flagship-action JSON from '{}': {}",
            input.display(),
            err
        );
        1
    })?;

    if let Some(path) = output {
        std::fs::write(path, format!("{json}\n")).map_err(|err| {
            eprintln!("Error writing '{}': {}", path.display(), err);
            1
        })?;
        println!("Wrote flagship-action/v1 JSON to {}", path.display());
    } else {
        println!("{json}");
    }
    Ok(())
}

fn bdf_value_kind(value: &buildlang::bdf::BdfValue) -> &'static str {
    use buildlang::bdf::BdfValue;
    match value {
        BdfValue::Null => "null",
        BdfValue::Bool(_) => "bool",
        BdfValue::Int(_) => "int",
        BdfValue::Float(_) => "float",
        BdfValue::Str(_) => "str",
        BdfValue::Bytes(_) => "bytes",
        BdfValue::Array(_) => "array",
        BdfValue::Map(_) => "map",
    }
}

fn write_stdout_bytes(bytes: &[u8]) -> Result<(), i32> {
    use std::io::Write as _;
    std::io::stdout().write_all(bytes).map_err(|err| {
        eprintln!("Error writing to stdout: {}", err);
        1
    })?;
    Ok(())
}

fn cmd_parse(file: &PathBuf, json: bool) -> Result<(), i32> {
    // Read source file
    let source = std::fs::read_to_string(file).map_err(|e| {
        eprintln!("Error reading file '{}': {}", file.display(), e);
        1
    })?;

    // Expand `include!("path")` directives
    let parse_base = file.parent().unwrap_or(Path::new("."));
    let source = preprocess_includes(&source, parse_base)?;

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

fn item_kind_name(kind: &buildlang::ast::ItemKind) -> &'static str {
    match kind {
        buildlang::ast::ItemKind::Function(_) => "Function",
        buildlang::ast::ItemKind::Struct(_) => "Struct",
        buildlang::ast::ItemKind::Enum(_) => "Enum",
        buildlang::ast::ItemKind::Trait(_) => "Trait",
        buildlang::ast::ItemKind::Impl(_) => "Impl",
        buildlang::ast::ItemKind::TypeAlias(_) => "TypeAlias",
        buildlang::ast::ItemKind::Const(_) => "Const",
        buildlang::ast::ItemKind::Static(_) => "Static",
        buildlang::ast::ItemKind::Mod(_) => "Mod",
        buildlang::ast::ItemKind::Use(_) => "Use",
        buildlang::ast::ItemKind::ExternCrate(_) => "ExternCrate",
        buildlang::ast::ItemKind::ExternBlock(_) => "ExternBlock",
        buildlang::ast::ItemKind::Macro(_) => "Macro",
        buildlang::ast::ItemKind::MacroRules(_) => "MacroRules",
        buildlang::ast::ItemKind::Effect(_) => "Effect",
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
        output.push_str(&format!(
            "    \"{}\"{}\n",
            item_kind_name(&item.kind),
            comma
        ));
    }
    output.push_str("  ]\n");
    output.push_str("}\n");
    output
}

fn struct_field_count(fields: &buildlang::ast::StructFields) -> usize {
    match fields {
        buildlang::ast::StructFields::Named(f) => f.len(),
        buildlang::ast::StructFields::Tuple(f) => f.len(),
        buildlang::ast::StructFields::Unit => 0,
    }
}

fn print_item_summary(item: &buildlang::ast::Item, indent: usize) {
    let prefix = "  ".repeat(indent);
    match &item.kind {
        buildlang::ast::ItemKind::Function(f) => {
            println!("{}fn {}()", prefix, f.name.name);
            if let Some(ret) = &f.sig.return_ty {
                println!("{}  -> {:?}", prefix, ret);
            }
        }
        buildlang::ast::ItemKind::Struct(s) => {
            println!(
                "{}struct {} ({} fields)",
                prefix,
                s.name.name,
                struct_field_count(&s.fields)
            );
        }
        buildlang::ast::ItemKind::Enum(e) => {
            println!(
                "{}enum {} ({} variants)",
                prefix,
                e.name.name,
                e.variants.len()
            );
        }
        buildlang::ast::ItemKind::Trait(t) => {
            println!("{}trait {} ({} items)", prefix, t.name.name, t.items.len());
        }
        buildlang::ast::ItemKind::Impl(i) => {
            println!("{}impl ({} items)", prefix, i.items.len());
        }
        buildlang::ast::ItemKind::TypeAlias(t) => {
            println!("{}type {}", prefix, t.name.name);
        }
        buildlang::ast::ItemKind::Const(c) => {
            println!("{}const {}", prefix, c.name.name);
        }
        buildlang::ast::ItemKind::Static(s) => {
            println!("{}static {}", prefix, s.name.name);
        }
        buildlang::ast::ItemKind::Mod(m) => {
            println!("{}mod {}", prefix, m.name.name);
        }
        buildlang::ast::ItemKind::Use(u) => {
            println!("{}use {:?}", prefix, u.tree);
        }
        buildlang::ast::ItemKind::ExternCrate(e) => {
            println!("{}extern crate {}", prefix, e.name.name);
        }
        buildlang::ast::ItemKind::ExternBlock(e) => {
            println!(
                "{}extern \"{}\" ({} items)",
                prefix,
                e.abi.as_deref().unwrap_or("C"),
                e.items.len()
            );
        }
        buildlang::ast::ItemKind::Macro(m) => {
            println!("{}macro {:?}!", prefix, m.name.as_ref().map(|n| &n.name));
        }
        buildlang::ast::ItemKind::MacroRules(m) => {
            println!("{}macro_rules! {}", prefix, m.name.name);
        }
        buildlang::ast::ItemKind::Effect(e) => {
            println!("{}effect {}", prefix, e.name.name);
        }
    }
}

// =============================================================================
// INCLUDE PREPROCESSING (textual `include!("path")` expansion)
// =============================================================================

/// Maximum recursion depth for nested includes to prevent infinite loops.
const MAX_INCLUDE_DEPTH: usize = 10;

/// Preprocess `include!("path")` directives in source code.
///
/// This is a textual inclusion mechanism (like C's `#include`): the referenced
/// file's contents replace the `include!()` line.  Paths are resolved relative
/// to `base_dir` (typically the directory containing the current source file).
///
/// Features:
/// - Nested includes up to `MAX_INCLUDE_DEPTH` levels
/// - Double-inclusion guard: each canonical path is included at most once
/// - Graceful error reporting on missing files or depth overflow
fn preprocess_includes(source: &str, base_dir: &Path) -> Result<String, i32> {
    let mut included: HashSet<PathBuf> = HashSet::new();
    let mut ledger = None;
    preprocess_includes_inner(source, base_dir, 0, &mut included, &mut ledger)
}

fn preprocess_includes_recording_inputs(
    source: &str,
    base_dir: &Path,
    ledger: &mut InputDigestLedger,
) -> Result<String, i32> {
    let mut included: HashSet<PathBuf> = HashSet::new();
    let mut ledger = Some(ledger);
    preprocess_includes_inner(source, base_dir, 0, &mut included, &mut ledger)
}

fn preprocess_includes_inner(
    source: &str,
    base_dir: &Path,
    depth: usize,
    included: &mut HashSet<PathBuf>,
    ledger: &mut Option<&mut InputDigestLedger>,
) -> Result<String, i32> {
    if depth > MAX_INCLUDE_DEPTH {
        eprintln!(
            "Error: include depth exceeds {} - possible circular inclusion",
            MAX_INCLUDE_DEPTH
        );
        return Err(1);
    }

    let mut result = String::with_capacity(source.len());

    for line in source.lines() {
        let trimmed = line.trim();

        // Match: include!("some/path.bld");
        if let Some(path_str) = trimmed
            .strip_prefix("include!(\"")
            .and_then(|s| s.strip_suffix("\");"))
        {
            let full_path = base_dir.join(path_str);
            let canonical = full_path
                .canonicalize()
                .unwrap_or_else(|_| full_path.clone());

            // Double-inclusion guard
            if included.contains(&canonical) {
                // Already included - skip silently
                result.push_str("// [include already loaded: ");
                result.push_str(path_str);
                result.push_str("]\n");
                continue;
            }

            if full_path.exists() {
                let bytes = std::fs::read(&full_path).map_err(|e| {
                    eprintln!("Error reading include '{}': {}", full_path.display(), e);
                    1
                })?;
                if let Some(ledger) = ledger.as_deref_mut() {
                    ledger.record("include", &full_path, &bytes);
                }
                let contents = String::from_utf8(bytes).map_err(|e| {
                    eprintln!("Error reading include '{}': {}", full_path.display(), e);
                    1
                })?;

                included.insert(canonical);

                // Recursively expand includes in the included file
                let inc_dir = full_path.parent().unwrap_or(base_dir);
                let expanded =
                    preprocess_includes_inner(&contents, inc_dir, depth + 1, included, ledger)?;

                result.push_str("// === include: ");
                result.push_str(path_str);
                result.push_str(" ===\n");
                result.push_str(&expanded);
                if !expanded.ends_with('\n') {
                    result.push('\n');
                }
                result.push_str("// === end include: ");
                result.push_str(path_str);
                result.push_str(" ===\n");
            } else {
                eprintln!(
                    "Error: include file not found: '{}' (resolved to '{}')",
                    path_str,
                    full_path.display()
                );
                return Err(1);
            }
        } else {
            result.push_str(line);
            result.push('\n');
        }
    }

    Ok(result)
}

// =============================================================================
// IMPORT RESOLUTION (simple `// import <pkg>` and `use <pkg>;` directives)
// =============================================================================

/// Scan `source` for lines matching `// import <name>` or `use <name>;`.
/// For each match, look for `registry/packages/<name>/src/lib.bld` relative
/// to the repo root (derived from `input_file`).  If found, prepend its contents
/// to the source so the combined text can be parsed as a single compilation unit.
///
/// Name normalisation: underscores in the import name are converted to hyphens
/// when looking up the package directory (e.g. `use std_math;` maps to
/// `registry/packages/std-math/src/lib.bld`).
fn resolve_imports(source: &str, input_file: &Path) -> Result<String, i32> {
    let mut ledger = None;
    resolve_imports_inner(source, input_file, &mut ledger)
}

fn resolve_imports_recording_inputs(
    source: &str,
    input_file: &Path,
    ledger: &mut InputDigestLedger,
) -> Result<String, i32> {
    let mut ledger = Some(ledger);
    resolve_imports_inner(source, input_file, &mut ledger)
}

fn resolve_imports_inner(
    source: &str,
    input_file: &Path,
    ledger: &mut Option<&mut InputDigestLedger>,
) -> Result<String, i32> {
    // Try to locate the registry directory.
    // Walk up from the input file looking for a directory that contains
    // `registry/packages`.
    let registry_dir = {
        let mut dir = input_file.parent();
        let mut found: Option<PathBuf> = None;
        while let Some(d) = dir {
            let candidate = d.join("registry").join("packages");
            if candidate.is_dir() {
                found = Some(candidate);
                break;
            }
            dir = d.parent();
        }
        found
    };

    let mut prepended = String::new();
    let mut found_any = false;

    for line in source.lines() {
        let trimmed = line.trim();

        // Match `// import <name>`
        let import_name = if let Some(rest) = trimmed.strip_prefix("// import ") {
            Some(rest.trim().to_string())
        }
        // Match `use <name>;`
        else if let Some(rest) = trimmed.strip_prefix("use ") {
            let rest = rest.trim();
            if let Some(name) = rest.strip_suffix(';') {
                let name = name.trim();
                // Skip complex use paths like `std::collections::HashMap` - we
                // only handle bare package names (no `::` separators).
                if !name.contains("::") && !name.contains('{') {
                    Some(name.to_string())
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        if let Some(name) = import_name {
            if let Some(ref reg) = registry_dir {
                // Normalise: underscores -> hyphens for the directory name.
                let pkg_dir_name = name.replace('_', "-");
                let lib_path = reg.join(&pkg_dir_name).join("src").join("lib.bld");
                if lib_path.exists() {
                    let bytes = std::fs::read(&lib_path).map_err(|e| {
                        eprintln!(
                            "Error reading import '{}' from '{}': {}",
                            name,
                            lib_path.display(),
                            e
                        );
                        1
                    })?;
                    if let Some(ledger) = ledger.as_deref_mut() {
                        ledger.record("import", &lib_path, &bytes);
                    }
                    let contents = String::from_utf8(bytes).map_err(|e| {
                        eprintln!(
                            "Error reading import '{}' from '{}': {}",
                            name,
                            lib_path.display(),
                            e
                        );
                        1
                    })?;
                    // Prepend with a separator comment for clarity.
                    prepended.push_str(&format!(
                        "// === imported from registry: {} ===\n{}\n// === end import: {} ===\n\n",
                        name, contents, name
                    ));
                    found_any = true;
                } else {
                    eprintln!(
                        "Warning: import '{}' not found at '{}'",
                        name,
                        lib_path.display()
                    );
                }
            } else {
                eprintln!(
                    "Warning: import '{}' requested but no registry directory found",
                    name
                );
            }
        }
    }

    if found_any {
        prepended.push_str(source);
        Ok(prepended)
    } else {
        Ok(source.to_string())
    }
}

fn type_error_kind(error: &TypeError) -> &'static str {
    match error {
        TypeError::TypeMismatch { .. } => "TypeMismatch",
        TypeError::InfiniteType { .. } => "InfiniteType",
        TypeError::MutabilityMismatch { .. } => "MutabilityMismatch",
        TypeError::UnknownEffect { .. } => "UnknownEffect",
        TypeError::UnhandledEffect { .. } => "UnhandledEffect",
        TypeError::UndeclaredEffect { .. } => "UndeclaredEffect",
        TypeError::UnknownEffectOperation { .. } => "UnknownEffectOperation",
        TypeError::MissingHandlerClause { .. } => "MissingHandlerClause",
        TypeError::NotTryable { .. } => "NotTryable",
        TypeError::NotAwaitable { .. } => "NotAwaitable",
        TypeError::UnitMismatch { .. } => "UnitMismatch",
        TypeError::UnitOperationMismatch { .. } => "UnitOperationMismatch",
        TypeError::ModuleImportCycle { .. } => "ModuleImportCycle",
        _ => "TypeError",
    }
}

fn language_version_string() -> String {
    format!(
        "{}.{}.{}",
        buildlang::LANGUAGE_VERSION.0,
        buildlang::LANGUAGE_VERSION.1,
        buildlang::LANGUAGE_VERSION.2
    )
}

fn source_digest_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut hex, "{byte:02x}").expect("write to string");
    }
    hex
}

fn source_text_digest_hex(bytes: &[u8]) -> String {
    let mut normalized = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\r' {
            normalized.push(b'\n');
            if bytes.get(index + 1) == Some(&b'\n') {
                index += 2;
            } else {
                index += 1;
            }
        } else {
            normalized.push(bytes[index]);
            index += 1;
        }
    }
    source_digest_hex(&normalized)
}

fn input_graph_digest(records: &[CheckReceiptInputDigest]) -> CheckReceiptSourceDigest {
    let mut hasher = Sha256::new();
    for record in records {
        hasher.update(record.role.as_bytes());
        hasher.update([0]);
        hasher.update(record.digest.algorithm.as_bytes());
        hasher.update([0]);
        hasher.update(record.digest.hex.as_bytes());
        hasher.update([10]);
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut hex, "{byte:02x}").expect("write to string");
    }
    CheckReceiptSourceDigest {
        algorithm: "sha256",
        hex,
    }
}

fn load_check_policy(path: &Path) -> Result<LoadedCheckPolicy, i32> {
    let bytes = std::fs::read(path).map_err(|err| {
        eprintln!("Error reading policy '{}': {}", path.display(), err);
        1
    })?;
    let source_digest = CheckReceiptSourceDigest {
        algorithm: "sha256",
        hex: source_digest_hex(&bytes),
    };
    // The policy's source_digest seals the RAW bytes while the profile is the
    // PARSED view; with a last-duplicate-wins parser those can disagree (two
    // allowlist keys where the hasher sees both and the reader keeps one), so
    // policies get the same strict duplicate-key rejection as receipts.
    let text = String::from_utf8_lossy(&bytes);
    assert_no_duplicate_json_keys(&text).map_err(|err| {
        eprintln!("Error parsing policy '{}': {}", path.display(), err);
        1
    })?;
    let profile: CheckPolicyProfile = serde_json::from_slice(&bytes).map_err(|err| {
        eprintln!("Error parsing policy '{}': {}", path.display(), err);
        1
    })?;
    if profile.schema != "buildlang-check-policy/v1" {
        eprintln!("Unsupported check policy schema '{}'", profile.schema);
        return Err(1);
    }

    Ok(LoadedCheckPolicy {
        source: path.to_string_lossy().to_string(),
        source_digest,
        builtin_profile: None,
        builtin_profile_digest: None,
        profile,
    })
}

fn load_builtin_check_policy(name: &str) -> Result<LoadedCheckPolicy, i32> {
    let json = builtin_policy_json(name).ok_or_else(|| {
        eprintln!(
            "Unknown built-in policy profile '{}'. Available: {}",
            name,
            builtin_policy_names()
        );
        1
    })?;
    let source_digest = CheckReceiptSourceDigest {
        algorithm: "sha256",
        hex: source_digest_hex(json.as_bytes()),
    };
    let profile: CheckPolicyProfile = serde_json::from_str(&json).map_err(|err| {
        eprintln!("Error parsing built-in policy profile '{}': {}", name, err);
        1
    })?;
    if profile.schema != "buildlang-check-policy/v1" {
        eprintln!("Unsupported check policy schema '{}'", profile.schema);
        return Err(1);
    }

    Ok(LoadedCheckPolicy {
        source: format!("builtin:{name}"),
        source_digest: source_digest.clone(),
        builtin_profile: Some(name.to_string()),
        builtin_profile_digest: Some(source_digest),
        profile,
    })
}

fn check_policy_status(decision: &CheckPolicyDecision) -> &'static str {
    if decision.violations.is_empty() {
        "passed"
    } else {
        "failed"
    }
}

fn allowlist_allows(
    allowlist: &BTreeMap<String, Vec<String>>,
    effect: &str,
    function: &str,
    require_entry: bool,
) -> bool {
    match allowlist.get(effect) {
        Some(functions) => functions.iter().any(|allowed| allowed == function),
        None => !require_entry,
    }
}

fn source_allowlist_allows(
    allowlist: &BTreeMap<String, BTreeMap<String, Vec<String>>>,
    effect: &str,
    function: &str,
    source: &str,
    require_entry: bool,
) -> bool {
    allowlist
        .get(effect)
        .and_then(|functions| functions.get(function))
        .map_or(!require_entry, |sources| {
            sources.iter().any(|allowed| allowed == source)
        })
}

fn digest_is_sha256_hex(digest: &CheckReceiptSourceDigest) -> bool {
    digest.algorithm == "sha256"
        && digest.hex.len() == 64
        && digest.hex.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn collect_check_policy_evidence(outcome: &CheckOutcome) -> BTreeSet<CheckPolicyEvidence> {
    let mut evidence = BTreeSet::new();
    for summary in &outcome.function_summaries {
        for effect in &summary.declared_effects {
            evidence.insert(CheckPolicyEvidence {
                function: summary.function.clone(),
                effect: effect.clone(),
                surface: "declared_effects",
                source: String::new(),
            });
        }
        for (effect, sources) in &summary.observed_capabilities {
            for source in sources {
                evidence.insert(CheckPolicyEvidence {
                    function: summary.function.clone(),
                    effect: effect.clone(),
                    surface: "observed_capabilities",
                    source: source.clone(),
                });
            }
        }
        for (effect, sources) in &summary.propagated_effects {
            for source in sources {
                evidence.insert(CheckPolicyEvidence {
                    function: summary.function.clone(),
                    effect: effect.clone(),
                    surface: "propagated_effects",
                    source: source.clone(),
                });
            }
        }
    }
    evidence
}

fn known_policy_effect_names(outcome: &CheckOutcome) -> BTreeSet<String> {
    let mut names = capability_effect_names()
        .iter()
        .map(|name| (*name).to_string())
        .collect::<BTreeSet<_>>();
    for summary in &outcome.function_summaries {
        names.extend(summary.declared_effects.iter().cloned());
        names.extend(summary.observed_capabilities.keys().cloned());
        names.extend(summary.propagated_effects.keys().cloned());
    }
    names
}

fn insert_unknown_policy_effect_violations<'a>(
    violations: &mut BTreeSet<CheckPolicyViolation>,
    known_effects: &BTreeSet<String>,
    surface: &'static str,
    effects: impl Iterator<Item = &'a String>,
) {
    for effect in effects {
        if !known_effects.contains(effect) {
            violations.insert(CheckPolicyViolation {
                kind: "UnknownPolicyEffect",
                effect: effect.clone(),
                function: String::new(),
                surface,
                source: String::new(),
                message: format!("policy references unknown effect `{effect}` in {surface}"),
            });
        }
    }
}

fn evidence_contains_policy_pair(
    evidence: &BTreeSet<CheckPolicyEvidence>,
    surface: &'static str,
    effect: &str,
    function: &str,
) -> bool {
    evidence
        .iter()
        .any(|item| item.surface == surface && item.effect == effect && item.function == function)
}

fn evidence_contains_policy_source(
    evidence: &BTreeSet<CheckPolicyEvidence>,
    surface: &'static str,
    effect: &str,
    function: &str,
    source: &str,
) -> bool {
    evidence.iter().any(|item| {
        item.surface == surface
            && item.effect == effect
            && item.function == function
            && item.source == source
    })
}

fn insert_unused_allowlist_violations(
    violations: &mut BTreeSet<CheckPolicyViolation>,
    evidence: &BTreeSet<CheckPolicyEvidence>,
    allowlist: &BTreeMap<String, Vec<String>>,
    allowlist_surface: &'static str,
    evidence_surface: &'static str,
    kind: &'static str,
) {
    for (effect, functions) in allowlist {
        for function in functions {
            if !evidence_contains_policy_pair(evidence, evidence_surface, effect, function) {
                violations.insert(CheckPolicyViolation {
                    kind,
                    effect: effect.clone(),
                    function: function.clone(),
                    surface: allowlist_surface,
                    source: String::new(),
                    message: format!(
                        "{allowlist_surface} entry `{effect}`/`{function}` was not matched by current receipt evidence"
                    ),
                });
            }
        }
    }
}

fn insert_unused_source_allowlist_violations(
    violations: &mut BTreeSet<CheckPolicyViolation>,
    evidence: &BTreeSet<CheckPolicyEvidence>,
    allowlist: &BTreeMap<String, BTreeMap<String, Vec<String>>>,
    allowlist_surface: &'static str,
    evidence_surface: &'static str,
    kind: &'static str,
) {
    for (effect, functions) in allowlist {
        for (function, sources) in functions {
            for source in sources {
                if !evidence_contains_policy_source(
                    evidence,
                    evidence_surface,
                    effect,
                    function,
                    source,
                ) {
                    violations.insert(CheckPolicyViolation {
                        kind,
                        effect: effect.clone(),
                        function: function.clone(),
                        surface: allowlist_surface,
                        source: source.clone(),
                        message: format!(
                            "{allowlist_surface} entry `{effect}`/`{function}`/`{source}` was not matched by current receipt evidence"
                        ),
                    });
                }
            }
        }
    }
}

fn evaluate_check_policy(
    policy: &LoadedCheckPolicy,
    outcome: &CheckOutcome,
) -> CheckPolicyDecision {
    let allowed: BTreeSet<&str> = policy
        .profile
        .allowed_effects
        .iter()
        .map(String::as_str)
        .collect();
    let denied: BTreeSet<&str> = policy
        .profile
        .denied_effects
        .iter()
        .map(String::as_str)
        .collect();
    let mut violations = BTreeSet::new();
    let known_effects = known_policy_effect_names(outcome);
    let evidence = collect_check_policy_evidence(outcome);

    insert_unknown_policy_effect_violations(
        &mut violations,
        &known_effects,
        "allowed_effects",
        policy.profile.allowed_effects.iter(),
    );
    insert_unknown_policy_effect_violations(
        &mut violations,
        &known_effects,
        "denied_effects",
        policy.profile.denied_effects.iter(),
    );
    insert_unknown_policy_effect_violations(
        &mut violations,
        &known_effects,
        "direct_effect_allowlist",
        policy.profile.direct_effect_allowlist.keys(),
    );
    insert_unknown_policy_effect_violations(
        &mut violations,
        &known_effects,
        "direct_capability_source_allowlist",
        policy.profile.direct_capability_source_allowlist.keys(),
    );
    insert_unknown_policy_effect_violations(
        &mut violations,
        &known_effects,
        "propagated_effect_allowlist",
        policy.profile.propagated_effect_allowlist.keys(),
    );
    insert_unknown_policy_effect_violations(
        &mut violations,
        &known_effects,
        "propagated_effect_source_allowlist",
        policy.profile.propagated_effect_source_allowlist.keys(),
    );
    if policy.profile.require_allowlist_coverage {
        insert_unused_allowlist_violations(
            &mut violations,
            &evidence,
            &policy.profile.direct_effect_allowlist,
            "direct_effect_allowlist",
            "observed_capabilities",
            "UnusedDirectEffectAllowlist",
        );
        insert_unused_allowlist_violations(
            &mut violations,
            &evidence,
            &policy.profile.propagated_effect_allowlist,
            "propagated_effect_allowlist",
            "propagated_effects",
            "UnusedPropagatedEffectAllowlist",
        );
        insert_unused_source_allowlist_violations(
            &mut violations,
            &evidence,
            &policy.profile.direct_capability_source_allowlist,
            "direct_capability_source_allowlist",
            "observed_capabilities",
            "UnusedDirectCapabilitySourceAllowlist",
        );
        insert_unused_source_allowlist_violations(
            &mut violations,
            &evidence,
            &policy.profile.propagated_effect_source_allowlist,
            "propagated_effect_source_allowlist",
            "propagated_effects",
            "UnusedPropagatedEffectSourceAllowlist",
        );
    }

    if policy.profile.require_source_digest && outcome.source_digest.algorithm != "sha256" {
        violations.insert(CheckPolicyViolation {
            kind: "MissingSourceDigest",
            effect: String::new(),
            function: String::new(),
            surface: "source_digest",
            source: String::new(),
            message: "policy requires sha256 source digest".to_string(),
        });
    }

    if policy.profile.require_input_graph_digest
        && !digest_is_sha256_hex(&outcome.input_graph_digest)
    {
        violations.insert(CheckPolicyViolation {
            kind: "MissingInputGraphDigest",
            effect: String::new(),
            function: outcome.source.clone(),
            surface: "input_graph_digest",
            source: String::new(),
            message: "policy requires a valid sha256 input graph digest".to_string(),
        });
    }

    for item in evidence {
        if denied.contains(item.effect.as_str()) {
            violations.insert(CheckPolicyViolation {
                kind: "DeniedEffect",
                effect: item.effect.clone(),
                function: item.function.clone(),
                surface: item.surface,
                source: item.source.clone(),
                message: format!("policy denies effect `{}`", item.effect),
            });
        } else if (policy.profile.require_effect_allowlist || !allowed.is_empty())
            && !allowed.contains(item.effect.as_str())
        {
            violations.insert(CheckPolicyViolation {
                kind: "DisallowedEffect",
                effect: item.effect.clone(),
                function: item.function.clone(),
                surface: item.surface,
                source: item.source.clone(),
                message: format!("policy does not allow effect `{}`", item.effect),
            });
        } else if item.surface == "observed_capabilities"
            && !allowlist_allows(
                &policy.profile.direct_effect_allowlist,
                &item.effect,
                &item.function,
                policy.profile.require_provenance_allowlists,
            )
        {
            violations.insert(CheckPolicyViolation {
                kind: "DirectEffectNotAllowed",
                effect: item.effect.clone(),
                function: item.function.clone(),
                surface: item.surface,
                source: item.source.clone(),
                message: format!(
                    "effect `{}` is directly used by `{}` via `{}` but policy does not allow that boundary",
                    item.effect, item.function, item.source
                ),
            });
        } else if item.surface == "observed_capabilities"
            && !source_allowlist_allows(
                &policy.profile.direct_capability_source_allowlist,
                &item.effect,
                &item.function,
                &item.source,
                policy.profile.require_source_allowlists,
            )
        {
            violations.insert(CheckPolicyViolation {
                kind: "DirectCapabilitySourceNotAllowed",
                effect: item.effect.clone(),
                function: item.function.clone(),
                surface: item.surface,
                source: item.source.clone(),
                message: format!(
                    "effect `{}` is directly used by `{}` via `{}` but policy does not allow that capability source",
                    item.effect, item.function, item.source
                ),
            });
        } else if item.surface == "propagated_effects"
            && !allowlist_allows(
                &policy.profile.propagated_effect_allowlist,
                &item.effect,
                &item.function,
                policy.profile.require_provenance_allowlists,
            )
        {
            violations.insert(CheckPolicyViolation {
                kind: "PropagatedEffectNotAllowed",
                effect: item.effect.clone(),
                function: item.function.clone(),
                surface: item.surface,
                source: item.source.clone(),
                message: format!(
                    "effect `{}` is propagated into `{}` via `{}` but policy does not allow that caller",
                    item.effect, item.function, item.source
                ),
            });
        } else if item.surface == "propagated_effects"
            && !source_allowlist_allows(
                &policy.profile.propagated_effect_source_allowlist,
                &item.effect,
                &item.function,
                &item.source,
                policy.profile.require_source_allowlists,
            )
        {
            violations.insert(CheckPolicyViolation {
                kind: "PropagatedEffectSourceNotAllowed",
                effect: item.effect.clone(),
                function: item.function.clone(),
                surface: item.surface,
                source: item.source.clone(),
                message: format!(
                    "effect `{}` is propagated into `{}` via `{}` but policy does not allow that callee source",
                    item.effect, item.function, item.source
                ),
            });
        }
    }

    CheckPolicyDecision {
        schema: policy.profile.schema.clone(),
        source: policy.source.clone(),
        source_digest: policy.source_digest.clone(),
        builtin_profile: policy.builtin_profile.clone(),
        builtin_profile_digest: policy.builtin_profile_digest.clone(),
        violations: violations.into_iter().collect(),
    }
}

fn build_check_receipt(
    outcome: &CheckOutcome,
    policy: Option<&CheckPolicyDecision>,
) -> CheckReceipt {
    let mut declared_effects = BTreeMap::new();
    let mut observed_capabilities = BTreeMap::new();
    let mut propagated_effects = BTreeMap::new();

    for summary in &outcome.function_summaries {
        declared_effects.insert(summary.function.clone(), summary.declared_effects.clone());
        let mut capabilities = BTreeMap::new();
        for (effect, sources) in &summary.observed_capabilities {
            capabilities.insert(effect.clone(), sources.iter().cloned().collect::<Vec<_>>());
        }
        observed_capabilities.insert(summary.function.clone(), capabilities);

        let mut propagated = BTreeMap::new();
        for (effect, sources) in &summary.propagated_effects {
            propagated.insert(effect.clone(), sources.iter().cloned().collect::<Vec<_>>());
        }
        propagated_effects.insert(summary.function.clone(), propagated);
    }

    let mut diagnostics = Vec::new();
    diagnostics.extend(
        outcome
            .parse_errors
            .iter()
            .map(|diag| CheckReceiptDiagnostic {
                stage: "parse",
                kind: "ParseError".to_string(),
                message: diag.message.clone(),
                line: Some(diag.line),
                col: Some(diag.col),
                help: diag.help.clone(),
                notes: diag.notes.clone(),
            }),
    );
    diagnostics.extend(outcome.type_errors.iter().enumerate().map(|(i, err)| {
        let loc = outcome.type_error_locations.get(i).copied().flatten();
        CheckReceiptDiagnostic {
            stage: "type",
            kind: type_error_kind(&err.error).to_string(),
            message: err.error.to_string(),
            line: loc.map(|(line, _)| line),
            col: loc.map(|(_, col)| col),
            help: err.help.clone(),
            notes: err.notes.clone(),
        }
    }));

    let policy_failed = policy
        .map(|decision| !decision.violations.is_empty())
        .unwrap_or(false);
    let receipt_policy = policy.map(|decision| CheckReceiptPolicy {
        schema: decision.schema.clone(),
        source: decision.source.clone(),
        source_digest: decision.source_digest.clone(),
        profile: decision.builtin_profile.clone(),
        profile_digest: decision.builtin_profile_digest.clone(),
        status: check_policy_status(decision),
        violations: decision.violations.clone(),
    });

    CheckReceipt {
        schema: "buildlang-check-receipt/v1",
        compiler: "buildc",
        compiler_version: outcome.compiler_version,
        language_version: outcome.language_version.clone(),
        source: outcome.source.clone(),
        source_digest: outcome.source_digest.clone(),
        input_graph_digest: outcome.input_graph_digest.clone(),
        input_digests: outcome.input_digests.clone(),
        status: if diagnostics.is_empty() && !policy_failed {
            "passed"
        } else {
            "failed"
        },
        items: outcome.items,
        tokens: outcome.tokens,
        declared_effects,
        observed_capabilities,
        propagated_effects,
        diagnostics,
        policy: receipt_policy,
    }
}

fn run_check(file: &Path) -> Result<CheckOutcome, i32> {
    let mut input_digest_ledger = InputDigestLedger::default();
    let source_bytes = std::fs::read(file).map_err(|e| {
        eprintln!("Error reading file '{}': {}", file.display(), e);
        1
    })?;
    input_digest_ledger.record("entry", file, &source_bytes);
    let source_digest = CheckReceiptSourceDigest {
        algorithm: "sha256",
        hex: source_digest_hex(&source_bytes),
    };
    let source = String::from_utf8(source_bytes).map_err(|e| {
        eprintln!("Error reading file '{}': {}", file.display(), e);
        1
    })?;

    let source = resolve_imports_recording_inputs(&source, file, &mut input_digest_ledger)?;
    let chk_base = file.parent().unwrap_or(Path::new("."));
    let source = preprocess_includes_recording_inputs(&source, chk_base, &mut input_digest_ledger)?;
    let source_file = SourceFile::new(file.to_string_lossy(), source);

    let mut lexer = Lexer::new(&source_file);
    let tokens = lexer.tokenize().map_err(|e| {
        eprintln!("Lexer error: {}", e);
        1
    })?;
    let token_count = tokens.len();

    let mut parser = Parser::new(&source_file, tokens);
    let mut ast = parser.parse().unwrap();
    // Resolve each parse error's byte span to `line:col` and grab its source
    // line now, while `source_file` is borrowable. Same arithmetic as
    // `report_parse_errors` (the `build`/`run` renderer), so `check` reports
    // the identical location for the identical error.
    let parse_errors = parser
        .errors()
        .iter()
        .map(|err| {
            let line = source_file.lookup_line(err.span.start);
            let line_start = source_file.line_start(line).unwrap_or(err.span.start);
            let col = err.span.start.0.saturating_sub(line_start.0) as usize;
            let snippet = source_file.source().lines().nth(line).map(str::to_string);
            let underline = (err.span.end.0.saturating_sub(err.span.start.0) as usize).max(1);
            ParseDiagnostic {
                message: err.message(),
                line: line + 1,
                col: col + 1,
                snippet,
                underline,
                help: err.help.clone(),
                notes: err.notes.clone(),
            }
        })
        .collect::<Vec<_>>();
    let item_count = ast.items.len();

    resolve_modules_recording_inputs(&mut ast, chk_base, &mut input_digest_ledger)?;

    let mut ctx = TypeContext::new();
    let mut checker = TypeChecker::new(&mut ctx);
    checker.set_source_file(&source_file);
    checker.set_source_dir(chk_base.to_path_buf());
    checker.check_module(&ast);

    let mut type_errors = checker.errors().to_vec();
    let function_summaries = checker.function_effect_summaries().to_vec();

    // MIR `#[linear]` checker (2b-wire): only lower + run it when the AST
    // tracker (and parsing) found nothing. The AST tracker blocks lowering
    // on its own errors -- lowering a linear-invalid-per-AST-tracker program
    // is not guaranteed sound -- so the MIR checker only ever sees programs
    // the AST tracker already PASSED. That is exactly the "no double
    // reporting" interaction the design spec calls for: this pass's job is
    // to catch what the AST tracker's name-keyed tracking cannot (the open
    // classes: move-out-of-borrow, borrow-after-move, dataflow joins), never
    // to re-report what the AST tracker already rejected.
    if parse_errors.is_empty() && type_errors.is_empty() {
        let codegen_source: Arc<str> = Arc::from(source_file.source());
        let mut codegen = CodeGenerator::with_source(&ctx, Target::C, codegen_source);
        // `generate()`'s `Err` here means a lowering/backend bug unrelated to
        // linearity (e.g. invalid MIR); `buildc check` treats that the same
        // way it always has -- surfaced separately, not folded into
        // `type_errors` -- by simply not adding linear diagnostics.
        if let Ok(()) = codegen.generate(&ast).map(|_| ()) {
            type_errors.extend(codegen.linear_errors().to_vec());
        }
    }

    // Resolve each type error's byte span to 1-based `line:col` now, while
    // `source_file` is still borrowable. Same arithmetic as the parse-error
    // path above, so a type error reports the identical location shape. Done
    // AFTER the codegen linear errors were appended, so the vec stays
    // index-aligned with the final `type_errors`. A dummy span (synthetic
    // node) has `end == 0`; it resolves to `None` so the receipt omits the
    // field instead of reporting a false `1:1`.
    let type_error_locations = type_errors
        .iter()
        .map(|err| {
            if err.span.end.0 == 0 {
                return None;
            }
            let line = source_file.lookup_line(err.span.start);
            let line_start = source_file.line_start(line).unwrap_or(err.span.start);
            let col = err.span.start.0.saturating_sub(line_start.0) as usize;
            Some((line + 1, col + 1))
        })
        .collect::<Vec<_>>();

    let input_digests = input_digest_ledger.into_sorted_records();
    let input_graph_digest = input_graph_digest(&input_digests);

    Ok(CheckOutcome {
        source: file.to_string_lossy().to_string(),
        compiler_version: buildlang::VERSION,
        language_version: language_version_string(),
        source_digest,
        input_graph_digest,
        input_digests,
        items: item_count,
        tokens: token_count,
        parse_errors,
        type_errors,
        type_error_locations,
        function_summaries,
    })
}

fn render_check_line(receipt_to_stdout: bool, message: impl AsRef<str>) {
    if receipt_to_stdout {
        eprintln!("{}", message.as_ref());
    } else {
        println!("{}", message.as_ref());
    }
}

/// Render one parse diagnostic to stderr as `error[path:line:col]: message`
/// with the source line and a caret underline. Mirrors `report_parse_errors`
/// (the `build`/`run` renderer) so a parse error reads the same whether it is
/// found by `check` or by `build`; the location was resolved in `run_check`.
fn render_parse_diagnostic(source_path: &str, diag: &ParseDiagnostic) {
    eprintln!(
        "error[{}:{}:{}]: {}",
        source_path, diag.line, diag.col, diag.message
    );
    if let Some(src_line) = &diag.snippet {
        eprintln!("  {} | {}", diag.line, src_line);
        let padding = format!("{}", diag.line).len();
        let col0 = diag.col.saturating_sub(1);
        eprintln!(
            "  {} | {}{}",
            " ".repeat(padding),
            " ".repeat(col0),
            "^".repeat(diag.underline.min(src_line.len().saturating_sub(col0)))
        );
    }
    if let Some(help) = &diag.help {
        eprintln!("  help: {}", help);
    }
    for note in &diag.notes {
        eprintln!("  note: {}", note);
    }
}

fn render_check_human_output(outcome: &CheckOutcome, receipt_to_stdout: bool) {
    render_check_line(
        receipt_to_stdout,
        format!("Lexing... OK ({} tokens)", outcome.tokens),
    );
    if outcome.parse_errors.is_empty() {
        render_check_line(
            receipt_to_stdout,
            format!("Parsing... OK ({} items)", outcome.items),
        );
    } else {
        render_check_line(
            receipt_to_stdout,
            format!(
                "Parsing... {} items ({} parse errors)",
                outcome.items,
                outcome.parse_errors.len()
            ),
        );
    }

    if !outcome.parse_errors.is_empty() {
        for diag in &outcome.parse_errors {
            render_parse_diagnostic(&outcome.source, diag);
        }
    }
    if !outcome.type_errors.is_empty() {
        eprintln!("Type errors found:");
        for (i, err) in outcome.type_errors.iter().enumerate() {
            match outcome.type_error_locations.get(i).copied().flatten() {
                Some((line, col)) => eprintln!("  {}:{}: {}", line, col, err),
                None => eprintln!("  {}", err),
            }
        }
    }

    if outcome.parse_errors.is_empty() && outcome.type_errors.is_empty() {
        render_check_line(receipt_to_stdout, "Type checking... OK");
        render_check_line(receipt_to_stdout, "");
        render_check_line(
            receipt_to_stdout,
            format!("No errors found in '{}'", outcome.source),
        );
    }
}

fn write_check_receipt(path: &Path, receipt: &CheckReceipt) -> Result<(), i32> {
    let json = serde_json::to_string_pretty(receipt).map_err(|err| {
        eprintln!("Error serializing check receipt: {}", err);
        1
    })?;
    if path == Path::new("-") {
        println!("{}", json);
        Ok(())
    } else {
        std::fs::write(path, format!("{}\n", json)).map_err(|err| {
            eprintln!("Error writing check receipt '{}': {}", path.display(), err);
            1
        })
    }
}

fn render_check_policy_output(policy: Option<&CheckPolicyDecision>) {
    let Some(policy) = policy else {
        return;
    };
    for violation in &policy.violations {
        let target = if violation.function.is_empty() {
            violation.surface.to_string()
        } else {
            format!("{} in {}", violation.surface, violation.function)
        };
        eprintln!("Policy violation: {} ({})", violation.message, target);
    }
}

fn cmd_check(
    file: &Path,
    receipt: Option<&Path>,
    policy: Option<&Path>,
    profile: Option<&str>,
    expect_profile_digest: Option<&str>,
) -> Result<(), i32> {
    let receipt_to_stdout = receipt == Some(Path::new("-"));
    if policy.is_some() && profile.is_some() {
        eprintln!("Error: --policy and --profile cannot be used together");
        return Err(1);
    }
    if expect_profile_digest.is_some() && profile.is_none() {
        eprintln!("Error: --expect-profile-digest requires --profile");
        return Err(1);
    }
    let loaded_policy = if let Some(policy) = policy {
        Some(load_check_policy(policy)?)
    } else if let Some(profile) = profile {
        Some(load_builtin_check_policy(profile)?)
    } else {
        None
    };
    if let Some(expected_digest) = expect_profile_digest {
        let profile_name = profile.expect("profile is required for digest pinning");
        let actual_digest = loaded_policy
            .as_ref()
            .and_then(|policy| policy.builtin_profile_digest.as_ref())
            .expect("built-in profile digest is present");
        let expected_hex = normalize_digest_pin(expected_digest);
        if !actual_digest.hex.eq_ignore_ascii_case(expected_hex) {
            eprintln!(
                "Error: Built-in policy profile digest mismatch for '{}': expected sha256:{}, actual sha256:{}",
                profile_name, expected_hex, actual_digest.hex
            );
            return Err(1);
        }
    }
    let outcome = run_check(file)?;
    let policy_decision = loaded_policy
        .as_ref()
        .map(|policy| evaluate_check_policy(policy, &outcome));
    let receipt_value = receipt.map(|_| build_check_receipt(&outcome, policy_decision.as_ref()));

    render_check_human_output(&outcome, receipt_to_stdout);
    render_check_policy_output(policy_decision.as_ref());
    if let Some(receipt_value) = receipt_value {
        write_check_receipt(receipt.expect("receipt path is present"), &receipt_value)?;
    }

    let policy_passed = policy_decision
        .as_ref()
        .map(|decision| decision.violations.is_empty())
        .unwrap_or(true);
    if outcome.parse_errors.is_empty() && outcome.type_errors.is_empty() && policy_passed {
        Ok(())
    } else {
        Err(1)
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
    // First: try compilers already in PATH
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

        let ok = match probe {
            // cl.exe has no `--version`: it prints its banner and exits 2, so
            // for the cl family a successful SPAWN proves presence (requiring
            // exit 0 wrongly skipped a functional PATH cl and fell through to
            // gcc/clang or the hardcoded VS-root fallback).
            Ok(status) => status.success() || compiler.starts_with("cl"),
            Err(_) if compiler.starts_with("cl") => std::process::Command::new(compiler)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|_| true)
                .unwrap_or(false),
            Err(_) => false,
        };

        if ok {
            return Some(compiler.to_string());
        }
    }

    // Second (Windows only): auto-discover MSVC from Visual Studio BuildTools
    #[cfg(windows)]
    {
        if let Some(cl_path) = find_msvc_cl() {
            return Some(cl_path);
        }
    }

    None
}

/// Locate `spirv-val` (from the Vulkan SDK / SPIRV-Tools). Returns the program
/// to invoke, preferring one already on PATH; falls back to a known Vulkan SDK
/// bin path on Windows. `None` means the tool is absent and validation is
/// skipped gracefully (mirrors `find_c_compiler`'s absence-is-graceful policy).
fn find_spirv_val() -> Option<String> {
    let candidates: &[&str] = if cfg!(windows) {
        &["spirv-val", "spirv-val.exe"]
    } else {
        &["spirv-val"]
    };
    for &tool in candidates {
        let ok = std::process::Command::new(tool)
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            return Some(tool.to_string());
        }
    }
    #[cfg(windows)]
    {
        let sdk = r"C:\VulkanSDK\1.4.341.1\Bin\spirv-val.exe";
        if std::path::Path::new(sdk).is_file() {
            return Some(sdk.to_string());
        }
    }
    None
}

/// Run `spirv-val` on an emitted module. Returns `Ok(())` on validation success,
/// `Err(stderr)` on a non-zero exit (the caller fails the build), or `Ok(())`
/// with a printed skip if the tool is absent.
fn validate_spirv_module(spv_path: &std::path::Path) -> Result<(), String> {
    let Some(tool) = find_spirv_val() else {
        println!("  spirv-val: not found on PATH; skipping validation");
        return Ok(());
    };
    match std::process::Command::new(&tool)
        .arg("--target-env")
        .arg("vulkan1.0")
        .arg(spv_path)
        .output()
    {
        Ok(result) if result.status.success() => {
            println!("  spirv-val: PASSED (Vulkan 1.0)");
            Ok(())
        }
        Ok(result) => {
            let stderr = String::from_utf8_lossy(&result.stderr);
            Err(stderr.trim().to_string())
        }
        Err(e) => Err(format!("failed to invoke spirv-val: {}", e)),
    }
}

/// Find vcvarsall.bat from Visual Studio installation.
#[cfg(windows)]
#[allow(dead_code)]
fn find_vcvars_bat() -> Option<String> {
    let vs_roots = [
        r"C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools",
        r"C:\Program Files\Microsoft Visual Studio\2022\BuildTools",
        r"C:\Program Files (x86)\Microsoft Visual Studio\2022\Community",
        r"C:\Program Files\Microsoft Visual Studio\2022\Community",
        r"C:\Program Files (x86)\Microsoft Visual Studio\2022\Professional",
        r"C:\Program Files (x86)\Microsoft Visual Studio\2022\Enterprise",
    ];

    for vs_root in &vs_roots {
        let vcvars = std::path::PathBuf::from(vs_root).join(r"VC\Auxiliary\Build\vcvarsall.bat");
        if vcvars.is_file() {
            return Some(vcvars.to_string_lossy().to_string());
        }
    }
    None
}

/// Auto-discover MSVC cl.exe from Visual Studio BuildTools installation.
/// Searches common install paths and sets INCLUDE/LIB/PATH environment
/// variables so cl.exe can find headers and libraries.
#[cfg(windows)]
fn find_msvc_cl() -> Option<String> {
    use std::path::PathBuf;

    let vs_roots = [
        r"C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools",
        r"C:\Program Files\Microsoft Visual Studio\2022\BuildTools",
        r"C:\Program Files (x86)\Microsoft Visual Studio\2022\Community",
        r"C:\Program Files\Microsoft Visual Studio\2022\Community",
        r"C:\Program Files (x86)\Microsoft Visual Studio\2022\Professional",
        r"C:\Program Files (x86)\Microsoft Visual Studio\2022\Enterprise",
    ];

    for vs_root in &vs_roots {
        let vc_tools = PathBuf::from(vs_root).join(r"VC\Tools\MSVC");
        if !vc_tools.is_dir() {
            continue;
        }

        // Find the latest MSVC version directory
        let mut versions: Vec<_> = std::fs::read_dir(&vc_tools)
            .ok()?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        versions.sort();

        let msvc_ver = versions.last()?;
        let msvc_dir = vc_tools.join(msvc_ver);
        let cl_exe = msvc_dir.join(r"bin\Hostx64\x64\cl.exe");

        if !cl_exe.is_file() {
            continue;
        }

        // Find Windows SDK
        let sdk_root = PathBuf::from(r"C:\Program Files (x86)\Windows Kits\10");
        let sdk_include = sdk_root.join("Include");
        let sdk_lib = sdk_root.join("Lib");

        // Find latest SDK version
        let sdk_ver = if sdk_include.is_dir() {
            let mut sdk_versions: Vec<_> = std::fs::read_dir(&sdk_include)
                .ok()
                .map(|rd| {
                    rd.filter_map(|e| e.ok())
                        .filter(|e| e.path().is_dir())
                        .map(|e| e.file_name().to_string_lossy().to_string())
                        .collect()
                })
                .unwrap_or_default();
            sdk_versions.sort();
            sdk_versions.last().cloned().unwrap_or_default()
        } else {
            String::new()
        };

        // Set INCLUDE
        let msvc_include = msvc_dir.join("include");
        let ucrt_include = sdk_include.join(&sdk_ver).join("ucrt");
        let um_include = sdk_include.join(&sdk_ver).join("um");
        let shared_include = sdk_include.join(&sdk_ver).join("shared");

        let include_path = format!(
            "{};{};{};{}",
            msvc_include.display(),
            ucrt_include.display(),
            um_include.display(),
            shared_include.display(),
        );

        // Set LIB
        let msvc_lib = msvc_dir.join(r"lib\x64");
        let ucrt_lib = sdk_lib.join(&sdk_ver).join(r"ucrt\x64");
        let um_lib = sdk_lib.join(&sdk_ver).join(r"um\x64");

        let lib_path = format!(
            "{};{};{}",
            msvc_lib.display(),
            ucrt_lib.display(),
            um_lib.display(),
        );

        // Set PATH to include the bin directory
        let bin_dir = msvc_dir.join(r"bin\Hostx64\x64");
        let current_path = std::env::var("PATH").unwrap_or_default();
        let new_path = format!("{};{}", bin_dir.display(), current_path);

        // Apply environment variables globally for this process.
        // This ensures cl.exe can find headers and libraries when invoked.
        std::env::set_var("INCLUDE", &include_path);
        std::env::set_var("LIB", &lib_path);
        std::env::set_var("PATH", &new_path);

        // Also store the paths for explicit use by invoke_c_compiler
        std::env::set_var("BUILDLANG_MSVC_INCLUDE", &include_path);
        std::env::set_var("BUILDLANG_MSVC_LIB", &lib_path);
        std::env::set_var("BUILDLANG_MSVC_BIN", bin_dir.to_string_lossy().as_ref());

        return Some(cl_exe.to_string_lossy().to_string());
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
    user_libs: &[String],
) -> Result<(), i32> {
    let is_msvc =
        compiler.starts_with("cl") || compiler.ends_with("cl.exe") || compiler.ends_with("cl");

    let mut cmd = std::process::Command::new(compiler);

    if is_msvc {
        // On Windows, write a temporary .bat file that sets the MSVC
        // environment and calls cl.exe. This avoids quoting issues
        // with PowerShell and cmd.exe invocations.
        let c_path = c_file.to_string_lossy().replace('/', "\\");
        let _exe_path = exe_file.to_string_lossy().replace('/', "\\");
        let opt_flag = if release { "/O2" } else { "/Zi" };

        if let (Ok(inc), Ok(lib), Ok(bin)) = (
            std::env::var("BUILDLANG_MSVC_INCLUDE"),
            std::env::var("BUILDLANG_MSVC_LIB"),
            std::env::var("BUILDLANG_MSVC_BIN"),
        ) {
            let bat_path = c_file.with_extension("bat");
            let exe_path = exe_file.to_string_lossy().replace('/', "\\");
            // Write bat file with MSVC env setup and compilation
            let mut all_libs: Vec<String> = host_c_link_libraries(true)
                .iter()
                .map(|s| s.to_string())
                .collect();
            all_libs.extend(user_link_flags(user_libs, true));
            let bat_content = format!(
                "set \"INCLUDE={}\"\r\nset \"LIB={}\"\r\nset \"PATH={};%PATH%\"\r\ncl.exe /nologo /W0 /std:c11 {} \"{}\" /Fe\"{}\" {} 1>&2\r\n",
                inc,
                lib,
                bin,
                opt_flag,
                c_path,
                exe_path,
                all_libs.join(" ")
            );
            std::fs::write(&bat_path, &bat_content).map_err(|e| {
                eprintln!("Failed to write build script: {}", e);
                1
            })?;

            cmd = std::process::Command::new("cmd.exe");
            cmd.args(&["/C", &bat_path.to_string_lossy().replace('/', "\\")]);
            if let Some(parent) = c_file.parent() {
                cmd.current_dir(parent);
            }
        } else {
            // Direct invocation fallback
            cmd.arg(c_file);
            cmd.arg(format!("/Fe:{}", exe_file.display()));
            cmd.arg("/std:c11");
            if release {
                cmd.arg("/O2");
            } else {
                cmd.arg("/Zi");
            }
            cmd.arg("/nologo");
            cmd.arg("/W0");
            cmd.args(host_c_link_libraries(true));
            cmd.args(user_link_flags(user_libs, true));
        }
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
        cmd.args(host_c_link_libraries(false));
        cmd.args(user_link_flags(user_libs, false));
    }

    let output = cmd.output().map_err(|e| {
        eprintln!("Failed to invoke C compiler '{}': {}", compiler, e);
        1
    })?;

    if output.status.success() {
        if !exe_file.exists() {
            eprintln!(
                "Warning: C compiler succeeded but executable not found at {}",
                exe_file.display()
            );
        }
        Ok(())
    } else {
        eprintln!(
            "C compilation failed (exit code: {:?}):",
            output.status.code()
        );
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

fn host_c_link_libraries(is_msvc: bool) -> &'static [&'static str] {
    c_link_libraries(std::env::consts::OS, is_msvc)
}

fn c_link_libraries(target_os: &str, is_msvc: bool) -> &'static [&'static str] {
    match (target_os, is_msvc) {
        ("windows", true) => &["ws2_32.lib"],
        ("windows", false) => &["-lws2_32"],
        (_, false) => &["-lm"],
        _ => &[],
    }
}

/// Format user-declared FFI libraries (from extern blocks' `link "..."`
/// clauses) as C compiler arguments. MSVC takes `name.lib`; gcc/clang/cc take
/// `-lname`. The actual library resolution is handled by the C toolchain.
fn user_link_flags(libs: &[String], is_msvc: bool) -> Vec<String> {
    libs.iter()
        .map(|lib| {
            if is_msvc {
                format!("{lib}.lib")
            } else {
                format!("-l{lib}")
            }
        })
        .collect()
}

// =============================================================================
// BUILD COMMAND
// =============================================================================

/// Fail closed on recovered parse errors before an artifact is produced.
///
/// `Parser::parse` returns `Ok` even when it recovered from errors, so the type
/// checker can still see the file's valid items. That is right for `check`,
/// `lint`, and the LSP, but a code-producing path must never emit from a
/// recovered (truncated) AST: a statement that fails to parse is dropped from
/// its block, and the code that remains type-checks and compiles to a silently
/// wrong result. Every artifact path calls this after `parse()` and returns its
/// `Err` so the failure is located, not silent. Returns `Ok(())` when the
/// parser recovered nothing (no errors), so a clean parse is unaffected.
fn report_parse_errors(
    path: &Path,
    source_file: &SourceFile,
    errors: &[ParseError],
) -> Result<(), i32> {
    if errors.is_empty() {
        return Ok(());
    }
    for err in errors {
        let line = source_file.lookup_line(err.span.start);
        let line_start = source_file.line_start(line).unwrap_or(err.span.start);
        let col = err.span.start.0.saturating_sub(line_start.0) as usize;
        eprintln!(
            "error[{}:{}:{}]: {}",
            path.display(),
            line + 1,
            col + 1,
            err.message()
        );
        if let Some(src_line) = source_file.source().lines().nth(line) {
            eprintln!("  {} | {}", line + 1, src_line);
            let padding = format!("{}", line + 1).len();
            let underline_len = (err.span.end.0.saturating_sub(err.span.start.0) as usize).max(1);
            eprintln!(
                "  {} | {}{}",
                " ".repeat(padding),
                " ".repeat(col),
                "^".repeat(underline_len.min(src_line.len().saturating_sub(col)))
            );
        }
        if let Some(help) = &err.help {
            eprintln!("  help: {}", help);
        }
        for note in &err.notes {
            eprintln!("  note: {}", note);
        }
    }
    Err(1)
}

fn cmd_build(
    path: &PathBuf,
    release: bool,
    emit: &str,
    keep_c: bool,
    target_str: &str,
) -> Result<(), i32> {
    // Look for Build.toml or main.bld in the project directory
    let manifest_path = path.join("Build.toml");
    let main_path = if manifest_path.exists() {
        // Read manifest to find entry point
        path.join("src").join("main.bld")
    } else {
        // Look for main.bld directly
        let main_file = path.join("main.bld");
        if main_file.exists() {
            main_file
        } else {
            path.join("src").join("main.bld")
        }
    };

    if !main_path.exists() {
        eprintln!("Could not find entry point. Expected one of:");
        eprintln!("  - {}/main.bld", path.display());
        eprintln!("  - {}/src/main.bld", path.display());
        return Err(1);
    }

    let emit_c_only = emit == "c";
    let emit_header = emit == "header";

    // Resolve the code generation target.
    let target = parse_codegen_target(target_str).map_err(|err| {
        eprintln!("{}", err);
        1
    })?;
    let use_llvm = target == Target::LlvmIr;
    let use_spirv = target == Target::SpirV;
    let use_native = target == Target::X86_64 || target == Target::Arm64;
    let use_wasm = target == Target::Wasm;
    let use_shader = target == Target::Hlsl || target == Target::Glsl;
    let use_rust = target == Target::Rust;

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

    // Resolve `// import <pkg>` and `use <pkg>;` directives
    let source = resolve_imports(&source, &main_path)?;

    // Expand `include!("path")` directives
    let inc_base = main_path.parent().unwrap_or(Path::new("."));
    let source = preprocess_includes(&source, inc_base)?;

    let source_file = SourceFile::new(main_path.to_string_lossy(), source);

    // Tokenize
    let mut lexer = Lexer::new(&source_file);
    let tokens = lexer.tokenize().map_err(|e| {
        eprintln!("Lexer error: {}", e);
        1
    })?;

    let total_steps = if emit_c_only
        || emit_header
        || use_llvm
        || use_native
        || use_wasm
        || use_spirv
        || use_shader
        || use_rust
    {
        4
    } else {
        5
    };
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
    report_parse_errors(&main_path, &source_file, parser.errors())?;
    println!(
        "[2/{}] Parsing... OK ({} items)",
        total_steps,
        ast.items.len()
    );

    // Resolve `mod foo;` declarations - load and merge external module files
    let source_dir = main_path.parent().unwrap_or(Path::new("."));
    resolve_modules(&mut ast, source_dir)?;

    // Type check
    let mut ctx = TypeContext::new();
    let mut checker = TypeChecker::new(&mut ctx);
    checker.set_source_file(&source_file);
    checker.set_source_dir(source_dir.to_path_buf());
    checker.check_module(&ast);

    if checker.has_errors() {
        eprintln!("Type errors found:");
        for err in checker.errors() {
            eprintln!("  {}", err);
        }
        return Err(1);
    }
    println!("[3/{}] Type checking... OK", total_steps);

    // Code generation - pass source for macro string extraction
    let mut codegen = CodeGenerator::with_source(&ctx, target, Arc::from(source_file.source()));
    let output = codegen.generate(&ast).map_err(|e| {
        eprintln!("Code generation error: {}", e);
        1
    })?;
    if !codegen.linear_errors().is_empty() {
        eprintln!("Linear type errors found:");
        for err in codegen.linear_errors() {
            eprintln!("  {}", err);
        }
        return Err(1);
    }
    println!(
        "[4/{}] Code generation ({})... OK ({} bytes)",
        total_steps,
        target,
        output.data.len()
    );

    // Write output
    let output_dir = path
        .join("target")
        .join(if release { "release" } else { "debug" });
    std::fs::create_dir_all(&output_dir).map_err(|e| {
        eprintln!("Failed to create output directory: {}", e);
        1
    })?;

    // --emit=header: write a C header declaring the `extern "C"` exports so
    // other languages can call into the compiled BuildLang code.
    if emit_header {
        let header = codegen.c_export_header().unwrap_or_default();
        let header_file = output_dir.join("main.h");
        std::fs::write(&header_file, header.as_bytes()).map_err(|e| {
            eprintln!("Failed to write header file: {}", e);
            1
        })?;
        println!("\nHeader generated!");
        println!("Output: {}", header_file.display());
        return Ok(());
    }

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
    } else if use_native {
        // x86-64 / ARM64 target: write assembly file
        let ext = if target == Target::X86_64 {
            "x86_64.s"
        } else {
            "aarch64.s"
        };
        let asm_output_file = output_dir.join(format!("main.{}", ext));
        std::fs::write(&asm_output_file, &output.data).map_err(|e| {
            eprintln!("Failed to write assembly output: {}", e);
            1
        })?;

        if !emit_c_only {
            // Try to assemble + link with system tools
            let assembler = if target == Target::X86_64 {
                if cfg!(windows) {
                    "ml64"
                } else {
                    "as"
                }
            } else {
                "aarch64-linux-gnu-as"
            };

            let asm_ok = std::process::Command::new(assembler)
                .arg("--version")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);

            if asm_ok {
                println!("[5/5] Assembling {} -> executable...", ext);
                // For now, output the assembly; full linking requires platform-specific logic
                println!();
                println!("Build successful! (assembly output)");
                println!("Output: {}", asm_output_file.display());
                println!();
                if target == Target::X86_64 {
                    if cfg!(windows) {
                        println!("To link: ml64 /Fe:main.exe {}", asm_output_file.display());
                    } else {
                        println!("To assemble and link:");
                        println!(
                            "  as {} -o main.o && ld main.o -o main -lc",
                            asm_output_file.display()
                        );
                    }
                } else {
                    println!("To cross-compile:");
                    println!("  aarch64-linux-gnu-as {} -o main.o && aarch64-linux-gnu-ld main.o -o main -lc", asm_output_file.display());
                }
                return Ok(());
            }

            println!();
            println!("Build successful! (assembly only - no assembler found)");
            println!("Output: {}", asm_output_file.display());
            return Ok(());
        }

        println!();
        println!("Build successful!");
        println!("Output: {}", asm_output_file.display());
        return Ok(());
    } else if use_shader {
        // HLSL/GLSL target: write shader source file
        let (ext, label) = if target == Target::Hlsl {
            ("hlsl", "HLSL")
        } else {
            ("glsl", "GLSL")
        };
        let shader_output_file = output_dir.join(format!("main.{}", ext));
        std::fs::write(&shader_output_file, &output.data).map_err(|e| {
            eprintln!("Failed to write {} output: {}", label, e);
            1
        })?;
        println!();
        println!("Build successful!");
        println!("Output: {} ({})", shader_output_file.display(), label);
        return Ok(());
    } else if use_wasm {
        // WebAssembly target: write .wasm binary
        let wasm_output_file = output_dir.join("main.wasm");
        std::fs::write(&wasm_output_file, &output.data).map_err(|e| {
            eprintln!("Failed to write WebAssembly output: {}", e);
            1
        })?;

        // Try running with wasmtime if available
        if !emit_c_only {
            let wt_ok = std::process::Command::new("wasmtime")
                .arg("--version")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);

            if wt_ok {
                println!("[5/5] WebAssembly module ready (wasmtime available)");
                println!();
                println!("Build successful!");
                println!("Output: {}", wasm_output_file.display());
                println!();
                println!("Run with: wasmtime {}", wasm_output_file.display());
                return Ok(());
            }
        }

        println!();
        println!("Build successful!");
        println!("Output: {}", wasm_output_file.display());
        println!();
        println!("Run with: wasmtime {}", wasm_output_file.display());
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
                    println!(
                        "  clang {} -o {}",
                        ll_output_file.display(),
                        output_dir.join("main.exe").display()
                    );
                } else {
                    println!("To compile to executable, install clang and run:");
                    println!(
                        "  clang {} -o {} -lm",
                        ll_output_file.display(),
                        output_dir.join("main").display()
                    );
                }
                return Ok(());
            }
        }

        println!();
        println!("Build successful!");
        println!("Output: {}", ll_output_file.display());
        return Ok(());
    } else if use_rust {
        let rs_output_file = output_dir.join("main.rs");
        std::fs::write(&rs_output_file, &output.data).map_err(|e| {
            eprintln!("Failed to write Rust output: {}", e);
            1
        })?;
        println!();
        println!("Build successful!");
        println!("Output: {} (Rust source)", rs_output_file.display());
        println!(
            "Validate with: rustc --emit=metadata {}",
            rs_output_file.display()
        );
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
        eprintln!("BuildLang needs a C compiler to produce executables.");
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

    println!(
        "[5/{}] Compiling C -> executable (using {})...",
        total_steps, compiler
    );

    invoke_c_compiler(
        &compiler,
        &c_output_file,
        &exe_output_file,
        release,
        &output.link_libraries,
    )?;

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

static RUN_TEMP_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn sanitize_temp_component(component: &str) -> String {
    let sanitized: String = component
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect();

    if sanitized.is_empty() {
        "program".to_string()
    } else {
        sanitized
    }
}

fn run_temp_build_dir(file: &Path) -> PathBuf {
    let stem = file
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(sanitize_temp_component)
        .unwrap_or_else(|| "program".to_string());
    let counter = RUN_TEMP_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);

    std::env::temp_dir().join(format!(
        "buildlang_run_{}_{}_{}_{}",
        stem,
        std::process::id(),
        nanos,
        counter
    ))
}

#[allow(clippy::too_many_arguments)]
/// A compiled BuildLang program: the temp build directory and the produced
/// executable. The caller owns the temp dir and is responsible for removing it
/// once the program has been run.
struct CompiledProgram {
    temp_dir: PathBuf,
    exe_file: PathBuf,
}

/// Captured result of running a compiled program with `.output()`.
///
/// Used by both `run --emit-receipt` and `receipt verify` for the
/// scientific-runtime schema: both need the program's numeric stdout and its
/// exit code, and both re-run through the exact same compile+run path so the
/// verifier re-derives (never trusts) the emitted receipt.
struct CapturedRun {
    /// Program stdout decoded lossily as UTF-8 (the numeric measurement series).
    stdout: String,
    /// Raw stdout bytes, for byte-faithful echoing by `run --emit-receipt`.
    stdout_bytes: Vec<u8>,
    /// Raw stderr bytes, for echoing by `run --emit-receipt`.
    stderr_bytes: Vec<u8>,
    /// The program's process exit code (`-1` if terminated by a signal).
    exit_code: i32,
    /// sha256 of the compiled executable, hashed BEFORE the program ran (the
    /// temp dir is deleted afterwards). Sealed into the receipt's
    /// compiler_branch block at emit; REPORTED (never required) at verify.
    executable_digest: ScientificDigest,
    /// Wall-clock duration of the child process run, measured with
    /// `std::time::Instant` around the `.output()` call and rounded to 3
    /// decimal places (millisecond precision keeps the JSON tidy and the
    /// fact honest). The receipt's first EXECUTED time fact.
    wall_seconds: f64,
}

/// Derive the scientific receipt's effect_policy block from a check outcome:
/// a canonical, sorted rendering of every function's declared effects and
/// observed capability facts, hashed, plus the observed capability union.
/// Deterministic by construction (BTree iteration order + explicit sorts), so
/// emit and verify derive identical facts from identical source.
fn derive_effect_policy(outcome: &CheckOutcome) -> ScientificEffectPolicy {
    let mut lines: Vec<String> = Vec::new();
    let mut union: BTreeSet<String> = BTreeSet::new();
    // `Console` covers both stdout writes and stdin reads; the witnessed-field
    // derivation needs the stdin distinction the capability NAME loses. This
    // reads from the same sealed source set (the sources are already hashed
    // into `facts_digest`), so a tampered flag also drifts the digest.
    let mut reads_stdin = false;
    for summary in &outcome.function_summaries {
        let mut declared = summary.declared_effects.clone();
        declared.sort();
        let observed: Vec<String> = summary
            .observed_capabilities
            .iter()
            .map(|(capability, sources)| {
                union.insert(capability.clone());
                if sources.iter().any(|s| buildlang::types::is_stdin_source(s)) {
                    reads_stdin = true;
                }
                let sources: Vec<&str> = sources.iter().map(String::as_str).collect();
                format!("{}[{}]", capability, sources.join(","))
            })
            .collect();
        lines.push(format!(
            "fn {} declared({}) observed({})",
            summary.function,
            declared.join(","),
            observed.join(";")
        ));
    }
    lines.sort();
    let canonical = lines.join(
        "
",
    );
    ScientificEffectPolicy {
        facts_digest: ScientificDigest {
            algorithm: "sha256".to_string(),
            hex: source_digest_hex(canonical.as_bytes()),
        },
        observed_capabilities: union.into_iter().collect(),
        reads_stdin,
    }
}

/// Probe the local C toolchain for the scientific receipt's compiler_branch
/// block (the pass-0122 contract). Returns `None` when no C compiler is
/// available.
///
/// `hash_own_binary` is true on the EMIT path only: the buildc binary digest
/// is sealed into the receipt, so a failed self-read FAILS CLOSED (None; no
/// receipt with fabricated toolchain facts). Verify passes false: its probe
/// exists to establish availability and identity for `toolchain_matched`
/// (which compares only c_compiler + version digest + target), so hashing the
/// verifier's own multi-megabyte binary would be pure waste; the local
/// buildc_binary_digest stays empty and is never compared or sealed.
///
/// The `program_executable_digest` field is a placeholder here (filled per
/// compiled artifact by the emit caller). The `target` field records the
/// os/arch the buildc binary was BUILT FOR (compile-time constants), which
/// equals the host for every supported configuration; it is not a runtime
/// host probe.
fn probe_c_toolchain(hash_own_binary: bool) -> Option<ScientificToolchain> {
    let compiler = find_c_compiler()?;
    // Capture the version banner. `cl` has no `--version`; it prints its
    // banner (to stderr) on the attempt anyway, so one probe serves both
    // families. Spawn failure means the compiler vanished since discovery.
    let output = std::process::Command::new(&compiler)
        .arg("--version")
        .output()
        .ok()?;
    let mut banner = output.stdout.clone();
    banner.extend_from_slice(&output.stderr);
    let version_line = String::from_utf8_lossy(&banner)
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("unknown")
        .trim()
        .to_string();
    let buildc_binary_hex = if hash_own_binary {
        // Sealed into the receipt: fail closed rather than seal an absent hash.
        let bytes = std::env::current_exe()
            .ok()
            .and_then(|path| std::fs::read(&path).ok())?;
        source_digest_hex(&bytes)
    } else {
        String::new()
    };
    Some(ScientificToolchain {
        c_compiler: compiler,
        c_compiler_version: version_line,
        version_output_digest: ScientificDigest {
            algorithm: "sha256".to_string(),
            hex: source_digest_hex(&banner),
        },
        target: format!("{}/{}", std::env::consts::OS, std::env::consts::ARCH),
        buildc_binary_digest: ScientificDigest {
            algorithm: "sha256".to_string(),
            hex: buildc_binary_hex,
        },
        program_executable_digest: ScientificDigest {
            algorithm: "sha256".to_string(),
            hex: String::new(),
        },
    })
}

/// Compile a `.bld` program to a native executable via the C backend.
///
/// This is the compile pipeline shared by every `run` variant
/// (read/resolve-imports/preprocess/lex/parse/resolve-modules/type-check/
/// codegen/C-compile). It is byte-identical to the pipeline that previously
/// lived inline in `cmd_run`; factoring it lets `run --emit-receipt` and
/// `receipt verify` (scientific schema) compile through the exact same path.
///
/// Returns the temp build dir and the produced exe path. The caller MUST remove
/// the temp dir after running (both call sites do).
fn compile_program_to_exe(
    file: &Path,
    compiler_override: Option<&str>,
) -> Result<CompiledProgram, i32> {
    // Read source file
    let source = std::fs::read_to_string(file).map_err(|e| {
        eprintln!("Error reading file '{}': {}", file.display(), e);
        1
    })?;

    // Resolve `// import <pkg>` and `use <pkg>;` directives
    let source = resolve_imports(&source, file)?;

    // Expand `include!("path")` directives
    let run_base = file.parent().unwrap_or(Path::new("."));
    let source = preprocess_includes(&source, run_base)?;

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
    report_parse_errors(file, &source_file, parser.errors())?;

    // Resolve `mod foo;` declarations - load and merge external module files
    let source_dir = file.parent().unwrap_or(Path::new("."));
    resolve_modules(&mut ast, source_dir)?;

    // Type check
    let mut ctx = TypeContext::new();
    let mut checker = TypeChecker::new(&mut ctx);
    checker.set_source_file(&source_file);
    checker.set_source_dir(source_dir.to_path_buf());
    checker.check_module(&ast);

    if checker.has_errors() {
        for err in checker.errors() {
            eprintln!("Type error: {}", err);
        }
        return Err(1);
    }

    // Generate C code - pass source for macro string extraction
    let mut codegen = CodeGenerator::with_source(&ctx, Target::C, Arc::from(source_file.source()));
    let output = codegen.generate(&ast).map_err(|e| {
        eprintln!("Code generation error: {}", e);
        1
    })?;
    if !codegen.linear_errors().is_empty() {
        eprintln!("Linear type errors found:");
        for err in codegen.linear_errors() {
            eprintln!("  {}", err);
        }
        return Err(1);
    }

    // Write to temp file
    let temp_dir = run_temp_build_dir(file);
    std::fs::create_dir_all(&temp_dir).map_err(|e| {
        eprintln!("Failed to create temp directory: {}", e);
        1
    })?;

    let c_file = temp_dir.join("main.c");
    let exe_file = if cfg!(windows) {
        temp_dir.join("main.exe")
    } else {
        temp_dir.join("main")
    };

    std::fs::write(&c_file, &output.data).map_err(|e| {
        eprintln!("Failed to write temp file: {}", e);
        1
    })?;

    // Invoke the C compiler: the caller's already-resolved compiler when one
    // was probed (the receipt paths, so the SEALED toolchain identity is the
    // compiler that actually built the executable, no re-resolution TOCTOU),
    // else resolve here (the plain `run` path).
    let compiler = match compiler_override {
        Some(compiler) => compiler.to_string(),
        None => find_c_compiler().ok_or_else(|| {
            eprintln!("Error: No C compiler found on the system.");
            eprintln!("BuildLang needs a C compiler to compile and run programs.");
            eprintln!();
            if cfg!(windows) {
                eprintln!("Install one of: cl.exe (MSVC), gcc (MinGW), or clang");
            } else {
                eprintln!("Install one of: cc, gcc, or clang");
            }
            1
        })?,
    };

    invoke_c_compiler(&compiler, &c_file, &exe_file, false, &output.link_libraries)?;

    // Verify the executable was created
    if !exe_file.exists() {
        eprintln!(
            "Error: C compilation reported success but executable not found at '{}'",
            exe_file.display()
        );
        // Check if MSVC put it somewhere else (current directory)
        let alt_name = temp_dir.join("temp.exe");
        if alt_name.exists() {
            eprintln!("Found executable in current directory instead - moving it");
            let _ = std::fs::rename(alt_name, &exe_file);
        } else {
            return Err(1);
        }
    }

    Ok(CompiledProgram { temp_dir, exe_file })
}

/// Compile `file`, run the produced executable with `args`, and capture its
/// stdout/stderr and exit code with `.output()`. Cleans up the temp build dir.
///
/// This is the single compile+run+capture path shared by `run --emit-receipt`
/// (which parses the captured stdout into a measurement series) and
/// `receipt verify` for the scientific-runtime schema (which RE-RUNS the program
/// and re-parses the series to re-check the invariant, rather than trusting the
/// stored values). Keeping one path means verify observes exactly what emit
/// observed.
fn compile_and_capture_run(
    file: &Path,
    args: &[String],
    compiler_override: Option<&str>,
    seed: Option<u64>,
) -> Result<CapturedRun, i32> {
    let CompiledProgram { temp_dir, exe_file } = compile_program_to_exe(file, compiler_override)?;

    // Hash the produced executable BEFORE running (the temp dir is removed
    // after the run). FAIL CLOSED on a read failure: a receipt must never
    // seal an absent hash as if it were witnessed provenance (an empty-hex
    // digest that "matches" another empty-hex digest would fabricate the
    // strongest reproduction signal).
    let executable_digest = ScientificDigest {
        algorithm: "sha256".to_string(),
        hex: match std::fs::read(&exe_file) {
            Ok(bytes) => source_digest_hex(&bytes),
            Err(err) => {
                eprintln!(
                    "Error: could not hash the compiled executable '{}': {}",
                    exe_file.display(),
                    err
                );
                let _ = std::fs::remove_dir_all(&temp_dir);
                return Err(1);
            }
        },
    };

    // Run the compiled program, capturing stdout so we can parse the series.
    // The wall clock brackets ONLY the child process's `.output()` call (not
    // the compile above it or the cleanup below): this is the receipt's
    // EXECUTED wall-clock fact, so it must measure exactly the witnessed run.
    let run_start = std::time::Instant::now();
    let run_output = {
        let mut run_cmd = std::process::Command::new(&exe_file);
        run_cmd.args(args);
        // The seed transport: ALWAYS strip any ambient BUILD_RANDOM_SEED first
        // (a stale shell export must never silently seed a run the receipt
        // would then claim as unseeded or differently seeded), then set the
        // explicit seed when one was given.
        run_cmd.env_remove("BUILD_RANDOM_SEED");
        if let Some(seed) = seed {
            run_cmd.env("BUILD_RANDOM_SEED", seed.to_string());
        }
        run_cmd.output().map_err(|e| {
            eprintln!("Failed to run program: {}", e);
            let _ = std::fs::remove_dir_all(&temp_dir);
            1i32
        })?
    };
    // Rounded to 3 decimal places (millisecond precision keeps the JSON tidy
    // and the fact honest: buildc cannot claim sub-millisecond fidelity over
    // a `.output()` call that itself pipes through the OS).
    let wall_seconds = (run_start.elapsed().as_secs_f64() * 1000.0).round() / 1000.0;

    // Clean up temp files.
    let _ = std::fs::remove_dir_all(&temp_dir);

    let stdout = String::from_utf8_lossy(&run_output.stdout).into_owned();
    let exit_code = run_output.status.code().unwrap_or(-1);
    Ok(CapturedRun {
        stdout,
        stdout_bytes: run_output.stdout,
        stderr_bytes: run_output.stderr,
        exit_code,
        executable_digest,
        wall_seconds,
    })
}

// =============================================================================
// CROSS-BACKEND SECONDARY LANE (--cross-backend rust)
// =============================================================================

/// The rustc toolchain facts probed for a `--cross-backend rust` run: the
/// resolved command (honoring a `RUSTC` env override, mirroring the existing
/// `rustc_available`/`rustc_compile_and_run` test convention), the first line
/// of its version banner, and a sha256 over the full version-probe output.
struct RustcProbe {
    path: String,
    version_line: String,
    version_output_digest: ScientificDigest,
}

/// Probe `rustc --version` for the secondary lane's toolchain facts. Returns
/// `None` when rustc is unreachable: at emit this refuses with an install
/// hint (exit 1) BEFORE any work is wasted; at verify the caller maps the
/// absence to the `RERUN_FAILED`/exit-4 pairing that matches how the primary
/// C toolchain's absence is classed (`TOOL_UNAVAILABLE`).
fn probe_rustc_toolchain() -> Option<RustcProbe> {
    let path = std::env::var_os("RUSTC")
        .map(|v| v.to_string_lossy().into_owned())
        .unwrap_or_else(|| "rustc".to_string());
    let output = std::process::Command::new(&path)
        .arg("--version")
        .output()
        .ok()?;
    let mut banner = output.stdout.clone();
    banner.extend_from_slice(&output.stderr);
    let version_line = String::from_utf8_lossy(&banner)
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("unknown")
        .trim()
        .to_string();
    Some(RustcProbe {
        path,
        version_line,
        version_output_digest: ScientificDigest {
            algorithm: "sha256".to_string(),
            hex: source_digest_hex(&banner),
        },
    })
}

/// Compile a `.bld` program to a native executable via the RUST backend: the
/// same internal codegen path `cmd_build --target rust` dispatches
/// (`CodeGenerator::with_source(&ctx, Target::Rust, ...)`), reached directly
/// (mirroring `compile_program_to_exe`'s structure) rather than shelling out
/// to `cmd_build`, since the pipeline through type-checking is byte-identical
/// to the C path above it and only the codegen target and the invoked
/// compiler differ.
///
/// A lowering failure (the kernel is outside the Rust subset) refuses with a
/// message naming the limitation; a `rustc` compile failure refuses with its
/// stderr forwarded. Returns the temp build dir and the produced exe path;
/// the caller MUST remove the temp dir.
fn compile_program_to_rust_exe(file: &Path, rustc_path: &str) -> Result<CompiledProgram, i32> {
    let source = std::fs::read_to_string(file).map_err(|e| {
        eprintln!("Error reading file '{}': {}", file.display(), e);
        1
    })?;
    let source = resolve_imports(&source, file)?;
    let run_base = file.parent().unwrap_or(Path::new("."));
    let source = preprocess_includes(&source, run_base)?;
    let source_file = SourceFile::new(file.to_string_lossy(), source);

    let mut lexer = Lexer::new(&source_file);
    let tokens = lexer.tokenize().map_err(|e| {
        eprintln!("Lexer error: {}", e);
        1
    })?;

    let mut parser = Parser::new(&source_file, tokens);
    let mut ast = parser.parse().map_err(|e| {
        eprintln!("Parse error: {}", e);
        for err in parser.errors() {
            eprintln!("  {}", err);
        }
        1
    })?;
    report_parse_errors(file, &source_file, parser.errors())?;

    let source_dir = file.parent().unwrap_or(Path::new("."));
    resolve_modules(&mut ast, source_dir)?;

    let mut ctx = TypeContext::new();
    let mut checker = TypeChecker::new(&mut ctx);
    checker.set_source_file(&source_file);
    checker.set_source_dir(source_dir.to_path_buf());
    checker.check_module(&ast);
    if checker.has_errors() {
        for err in checker.errors() {
            eprintln!("Type error: {}", err);
        }
        return Err(1);
    }

    let mut codegen =
        CodeGenerator::with_source(&ctx, Target::Rust, Arc::from(source_file.source()));
    let output = codegen.generate(&ast).map_err(|e| {
        eprintln!(
            "Error: kernel could not be lowered to the Rust backend (Rust subset limitation): {}",
            e
        );
        1
    })?;
    if !codegen.linear_errors().is_empty() {
        eprintln!("Linear type errors found (Rust backend):");
        for err in codegen.linear_errors() {
            eprintln!("  {}", err);
        }
        return Err(1);
    }

    let temp_dir = run_temp_build_dir(file);
    std::fs::create_dir_all(&temp_dir).map_err(|e| {
        eprintln!("Failed to create temp directory: {}", e);
        1
    })?;

    let rs_file = temp_dir.join("main.rs");
    std::fs::write(&rs_file, &output.data).map_err(|e| {
        eprintln!("Failed to write temp Rust source: {}", e);
        1
    })?;

    let exe_file = temp_dir.join("main_rust.exe");
    let compile_output = std::process::Command::new(rustc_path)
        .arg("-O")
        .arg("-o")
        .arg(&exe_file)
        .arg(&rs_file)
        .output()
        .map_err(|e| {
            eprintln!("Failed to invoke rustc: {}", e);
            let _ = std::fs::remove_dir_all(&temp_dir);
            1
        })?;
    if !compile_output.status.success() {
        eprintln!(
            "Error: rustc failed to compile the secondary (cross-backend) lane:\n{}",
            String::from_utf8_lossy(&compile_output.stderr)
        );
        let _ = std::fs::remove_dir_all(&temp_dir);
        return Err(1);
    }
    if !exe_file.exists() {
        eprintln!(
            "Error: rustc reported success but the secondary executable was not found at '{}'",
            exe_file.display()
        );
        let _ = std::fs::remove_dir_all(&temp_dir);
        return Err(1);
    }

    Ok(CompiledProgram { temp_dir, exe_file })
}

/// Compile `file` through the Rust backend, run the produced executable with
/// `args`, and capture its stdout/stderr and exit code with `.output()`.
/// Mirrors `compile_and_capture_run`: no seed is ever set (a cross-backend
/// request already refused a Random-observing kernel), but the same
/// `BUILD_RANDOM_SEED` scrub applies defensively. Cleans up the temp build
/// dir.
fn compile_and_capture_rust_run(
    file: &Path,
    args: &[String],
    rustc_path: &str,
) -> Result<CapturedRun, i32> {
    let CompiledProgram { temp_dir, exe_file } = compile_program_to_rust_exe(file, rustc_path)?;

    let executable_digest = ScientificDigest {
        algorithm: "sha256".to_string(),
        hex: match std::fs::read(&exe_file) {
            Ok(bytes) => source_digest_hex(&bytes),
            Err(err) => {
                eprintln!(
                    "Error: could not hash the compiled secondary executable '{}': {}",
                    exe_file.display(),
                    err
                );
                let _ = std::fs::remove_dir_all(&temp_dir);
                return Err(1);
            }
        },
    };

    // Measured for the same reason as the primary lane (a shared
    // `CapturedRun` shape), though the secondary's wall time is not sealed
    // anywhere in v0: the cross-backend block carries no wall_seconds field.
    let run_start = std::time::Instant::now();
    let run_output = {
        let mut run_cmd = std::process::Command::new(&exe_file);
        run_cmd.args(args);
        run_cmd.env_remove("BUILD_RANDOM_SEED");
        run_cmd.output().map_err(|e| {
            eprintln!("Failed to run the secondary (Rust) executable: {}", e);
            let _ = std::fs::remove_dir_all(&temp_dir);
            1i32
        })?
    };
    let wall_seconds = (run_start.elapsed().as_secs_f64() * 1000.0).round() / 1000.0;

    let _ = std::fs::remove_dir_all(&temp_dir);

    let stdout = String::from_utf8_lossy(&run_output.stdout).into_owned();
    let exit_code = run_output.status.code().unwrap_or(-1);
    Ok(CapturedRun {
        stdout,
        stdout_bytes: run_output.stdout,
        stderr_bytes: run_output.stderr,
        exit_code,
        executable_digest,
        wall_seconds,
    })
}

/// Shared `rerun_series` body for BOTH scientific-runtime verify dispatch
/// call sites (`receipt export` and `receipt verify`): re-runs the primary
/// (C) program through the exact path `run --emit-receipt` used, and, when
/// `secondary_target` names a lane (from the receipt's sealed
/// `cross_backend` block), re-runs the secondary too and fills
/// `RerunObservation.secondary`. `None` leaves it `None`.
///
/// rustc absence AT VERIFY is TOOL_UNAVAILABLE semantics: `probe_rustc_toolchain`
/// returning `None` maps to `Err(4)`, matching the exit code the primary C
/// toolchain's absence gets (checked by the caller before this is ever
/// invoked). A lowering or compile failure propagates as `Err(1)` (or
/// whatever `compile_and_capture_rust_run` returns), which the evaluator
/// maps to `RERUN_FAILED`.
fn rerun_scientific_receipt(
    source_path: &Path,
    args: &[String],
    seed: Option<u64>,
    secondary_target: Option<&str>,
    probed_toolchain: Option<&ScientificToolchain>,
) -> Result<RerunObservation, i32> {
    let captured = compile_and_capture_run(
        source_path,
        args,
        probed_toolchain.map(|t| t.c_compiler.as_str()),
        seed,
    )?;
    let raw_stdout_digest = ScientificDigest {
        algorithm: "sha256".to_string(),
        hex: source_digest_hex(&captured.stdout_bytes),
    };
    let secondary = match secondary_target {
        Some("rust") => {
            let rustc = probe_rustc_toolchain().ok_or(4)?;
            let secondary_captured = compile_and_capture_rust_run(source_path, args, &rustc.path)?;
            Some(SecondaryObservation {
                parsed: parse_numeric_series(&secondary_captured.stdout),
                exit_code: secondary_captured.exit_code,
                raw_stdout_digest: ScientificDigest {
                    algorithm: "sha256".to_string(),
                    hex: source_digest_hex(&secondary_captured.stdout_bytes),
                },
                executable_digest: secondary_captured.executable_digest,
                // The SAME probe that resolved `rustc.path` above (honoring
                // a RUSTC env override): carried into the observation so the
                // evaluator can compare it against the sealed
                // cross_backend.secondary_toolchain_digest and warn on drift,
                // giving the secondary lane the same toolchain visibility
                // the primary C lane already has at verify.
                probed_toolchain_version: rustc.version_line,
                probed_toolchain_digest: rustc.version_output_digest,
            })
        }
        Some(other) => {
            eprintln!(
                "Error: unsupported secondary target `{other}` in sealed cross_backend block"
            );
            return Err(1);
        }
        None => None,
    };
    Ok(RerunObservation {
        parsed: parse_numeric_series(&captured.stdout),
        exit_code: captured.exit_code,
        secondary,
        raw_stdout_digest,
        executable_digest: captured.executable_digest,
        wall_seconds: captured.wall_seconds,
    })
}

/// `buildc run <kernel> --gpu`: execute a `#[compute]` kernel on the physical
/// Vulkan device and cross-check the readback against the CPU-C scalar loop over
/// the same grid within tolerance (default 1e-6). Exits non-zero on mismatch.
///
/// Without the `gpu` feature this prints a rebuild hint and exits non-zero (the
/// default build carries no Vulkan dependency).
#[cfg(not(feature = "gpu"))]
fn cmd_run_gpu(_file: &Path, _emit_receipt: Option<&Path>) -> Result<(), i32> {
    eprintln!(
        "buildc run --gpu requires a build with the `gpu` feature.\n\
         Rebuild with: cargo build --features gpu --manifest-path compiler/Cargo.toml"
    );
    Err(1)
}

#[cfg(feature = "gpu")]
fn cmd_run_gpu(file: &Path, emit_receipt: Option<&Path>) -> Result<(), i32> {
    gpu::run_gpu_cross_check(file, emit_receipt)
}

#[allow(clippy::too_many_arguments)]
fn cmd_run(
    file: &PathBuf,
    args: &[String],
    emit_receipt: Option<&Path>,
    invariant: &str,
    metric: &str,
    units: Option<&str>,
    columns: usize,
    problem: Option<&str>,
    method: Option<&str>,
    negative_fixture: bool,
    seed: Option<u64>,
    mc_estimator: Option<&str>,
    mc_samples: Option<u64>,
    mc_interval: Option<&str>,
    mc_executed: bool,
    budget_steps: Option<u64>,
    budget_consumed: Option<u64>,
    budget_wall_seconds: Option<f64>,
    cross_backend: Option<&str>,
) -> Result<(), i32> {
    // The Monte Carlo declaration is all-or-nothing, validated whenever ANY
    // mc flag is present (like --units: a typo is never silently accepted):
    // an estimator whose interval method is undeclared is refused, and so is
    // every other partial combination. A zero sample count is an unpriceable
    // denominator, refused just as early.
    let mc_flag_count = [
        mc_estimator.is_some(),
        mc_samples.is_some(),
        mc_interval.is_some(),
    ]
    .iter()
    .filter(|present| **present)
    .count();
    let monte_carlo = match (mc_flag_count, mc_estimator, mc_samples, mc_interval) {
        (0, ..) => None,
        (3, Some(estimator), Some(samples), Some(interval_method)) => {
            if samples == 0 {
                eprintln!(
                    "Error: --mc-samples 0: an MC claim without its denominator is unpriceable"
                );
                return Err(1);
            }
            if estimator.trim().is_empty() || interval_method.trim().is_empty() {
                eprintln!(
                    "Error: --mc-estimator and --mc-interval must be non-empty: the claim is the interval, never the point, so both must be named"
                );
                return Err(1);
            }
            // Only the DECLARED shape can be built here: whether the block
            // is EXECUTED-and-coherent is unknown until the real series
            // exists. The finalization below (after capture) upgrades this
            // to EXECUTED when --mc-executed was passed, fail closed.
            Some(ScientificMonteCarlo {
                estimator: estimator.to_string(),
                samples,
                interval_method: interval_method.to_string(),
                status: "DECLARED".to_string(),
                estimate: None,
                interval_low: None,
                interval_high: None,
                n_effective: None,
                successes: None,
            })
        }
        _ => {
            eprintln!(
                "Error: a Monte Carlo declaration states estimator, samples, AND interval method together (--mc-estimator, --mc-samples, --mc-interval); an estimator whose interval method is undeclared is refused, and so is every other partial declaration"
            );
            return Err(1);
        }
    };
    // --mc-executed requires the full declaration (all three --mc-* flags),
    // and its estimator must be in the v1 executable vocabulary (`proportion`);
    // DECLARED blocks may still use free text. The interval-method vocabulary
    // check is NOT duplicated here: it is fail-closed inside
    // `compute_mc_executed`, called once real data exists, below.
    if mc_executed && mc_flag_count < 3 {
        eprintln!(
            "Error: --mc-executed requires the full Monte Carlo declaration (--mc-estimator, --mc-samples, --mc-interval)"
        );
        return Err(1);
    }
    if let Some(estimator) = mc_estimator {
        if mc_executed && estimator != MC_EXECUTED_ESTIMATOR_PROPORTION {
            eprintln!(
                "Error: --mc-executed requires --mc-estimator proportion (v1 executable vocabulary); DECLARED blocks may still use free text"
            );
            return Err(1);
        }
    }
    // The budgeted-search declaration is all-or-nothing, exactly like the MC
    // declaration: a result without its budget ceiling hides whether it
    // stopped at the limit, so neither flag alone is accepted. Deterministic
    // (no Random needed): a budget block is not coupled to the seed pairing.
    let mut budget: Option<ScientificBudget> = match (budget_steps, budget_consumed) {
        (None, None) => None,
        (Some(steps_limit), Some(steps_consumed)) => {
            if steps_limit == 0 {
                eprintln!("Error: --budget-steps 0: a zero ceiling is not a budget");
                return Err(1);
            }
            if steps_consumed > steps_limit {
                eprintln!(
                    "Error: --budget-consumed {steps_consumed} exceeds --budget-steps {steps_limit}: a consumption above its ceiling is incoherent"
                );
                return Err(1);
            }
            Some(ScientificBudget {
                steps_limit,
                steps_consumed,
                exhausted: steps_consumed == steps_limit,
                status: "DECLARED".to_string(),
                wall_seconds_limit: None,
                wall_exceeded: None,
            })
        }
        _ => {
            eprintln!(
                "Error: a budget declares its ceiling AND its consumption together (--budget-steps, --budget-consumed); a result without its budget ceiling hides whether it stopped at the limit"
            );
            return Err(1);
        }
    };
    // The wall ceiling is a MEMBER of the budget declaration, not a
    // freestanding knob: it requires the steps pair and a positive, finite
    // value. `wall_exceeded` is set below, once the primary run's wall time
    // has actually been measured (it is DERIVED from the sealed
    // measurement, never a hand-set or pre-run guess).
    if let Some(limit) = budget_wall_seconds {
        if budget.is_none() {
            eprintln!(
                "Error: --budget-wall-seconds requires the budget declaration (--budget-steps, --budget-consumed); the wall ceiling is a member of the budget block, not a freestanding one"
            );
            return Err(1);
        }
        if limit <= 0.0 || !limit.is_finite() {
            eprintln!(
                "Error: --budget-wall-seconds {limit}: must be a positive, finite number of seconds"
            );
            return Err(1);
        }
    }
    // The claim-language emit gate, beside the budget declaration: a
    // budgeted search reports its incumbent, never optimality, so
    // --method / --problem free text may not contradict NOT_PROVES_OPTIMALITY.
    if budget.is_some() {
        let problem_claims_optimal = problem
            .map(|s| s.to_lowercase().contains("optimal"))
            .unwrap_or(false);
        let method_claims_optimal = method
            .map(|s| s.to_lowercase().contains("optimal"))
            .unwrap_or(false);
        if problem_claims_optimal || method_claims_optimal {
            eprintln!(
                "Error: a budgeted search reports its incumbent, never optimality; the free text may not contradict NOT_PROVES_OPTIMALITY (the check is a plain case-insensitive substring: even a word like `suboptimal` trips it, so reword the label rather than weakening the gate)"
            );
            return Err(1);
        }
    }
    // Canonicalize the declared unit through the dimensional-analysis core
    // BEFORE any compilation work: a malformed or unknown unit is an operator
    // error we report immediately, and the receipt records the CHECKED
    // canonical form rather than an arbitrary free-text string. Only meaningful
    // when emitting a receipt, but validate whenever `--units` is present so a
    // typo is never silently accepted.
    let canonical_units: Option<String> = match units {
        Some(raw) => match buildlang::units::canonicalize_unit(raw) {
            Ok(canon) => Some(canon),
            Err(err) => {
                eprintln!("Invalid --units `{raw}`: {err}");
                return Err(1);
            }
        },
        None => None,
    };
    // Map the CLI invariant name (kebab-case) to the registry invariant the
    // receipt seals and verify re-checks. Reject unknown names early (before
    // compiling) so the error is clear and no work is wasted.
    let invariant_name = match invariant {
        "energy-monotone" => ENERGY_MONOTONE_INVARIANT,
        "conservation" => CONSERVATION_INVARIANT,
        "bounded" => BOUNDED_INVARIANT,
        "energy-identity" => ENERGY_IDENTITY_INVARIANT,
        "relation" => RELATION_INVARIANT,
        "conserved-band" => CONSERVED_BAND_INVARIANT,
        "non-negative" => NON_NEGATIVE_INVARIANT,
        "cross-backend" => CROSS_BACKEND_INVARIANT,
        other => {
            if emit_receipt.is_some() {
                eprintln!(
                    "Unknown --invariant '{other}'. Supported: energy-monotone, conservation, bounded, energy-identity, relation, conserved-band, non-negative, cross-backend"
                );
                return Err(1);
            }
            // Without --emit-receipt the invariant is unused; keep a harmless
            // default so the no-receipt run path is unaffected.
            ENERGY_MONOTONE_INVARIANT
        }
    };

    // `--invariant cross-backend` defines its own column structure (2: the C
    // anchor and the secondary lane), so an unset `--columns` (the CLI
    // default, 1) is silently upgraded; `--mc-executed` similarly forces 3
    // (the invariant scalar plus the witnessed successes/trials counters);
    // anything else is left for the existing column-count gate below to
    // refuse.
    let columns = if invariant_name == CROSS_BACKEND_INVARIANT && columns == 1 {
        2
    } else if mc_executed && columns == 1 {
        3
    } else {
        columns
    };

    // Column structure validation (only meaningful when emitting a receipt),
    // gated on the SAME contract verify re-checks (column_count_matches_invariant)
    // so the two can never drift: the `relation` invariant reads across columns
    // and needs at least two; every single-scalar invariant reads one value per
    // step and rejects a multi-column request rather than silently ignoring it.
    if emit_receipt.is_some()
        && !column_count_matches_invariant(invariant_name, columns, mc_executed)
    {
        if invariant_name == RELATION_INVARIANT {
            eprintln!(
                "--invariant relation needs --columns >= 2 (each row must hold the columns to compare)"
            );
        } else if invariant_name == CROSS_BACKEND_INVARIANT {
            eprintln!(
                "--invariant cross-backend needs --columns 2 (the C anchor and the secondary lane); the invariant defines its own column structure"
            );
        } else if mc_executed {
            eprintln!(
                "--mc-executed needs --columns 3 (the invariant scalar plus the witnessed successes/trials counters); the invariant defines its own column structure"
            );
        } else {
            eprintln!(
                "--columns {columns} is only valid with --invariant relation; the single-scalar invariants read one value per step"
            );
        }
        return Err(1);
    }

    // Default path (no --emit-receipt): inherit stdout via `.status()`, exactly
    // as before -- byte-identical output and exit-code semantics. Receipt path:
    // capture stdout via `.output()` (shared `compile_and_capture_run` helper),
    // echo it (so `run` still shows output), parse the numeric series, check the
    // invariant, seal, and write.
    let Some(receipt_path) = emit_receipt else {
        // No receipt: compile, then run with inherited stdout via `.status()`.
        let CompiledProgram { temp_dir, exe_file } = compile_program_to_exe(file, None)?;
        let status = {
            let mut run_cmd = std::process::Command::new(&exe_file);
            run_cmd.args(args);
            // Same seed transport as the captured path: strip any ambient
            // BUILD_RANDOM_SEED, then set the explicit one. A Random-using
            // program run seedless aborts at its first draw (fail closed in
            // the runtime), so the plain-run path needs no policy re-check.
            run_cmd.env_remove("BUILD_RANDOM_SEED");
            if let Some(seed) = seed {
                run_cmd.env("BUILD_RANDOM_SEED", seed.to_string());
            }
            run_cmd.status().map_err(|e| {
                eprintln!("Failed to run program: {}", e);
                1i32
            })?
        };

        // Clean up temp files
        let _ = std::fs::remove_dir_all(&temp_dir);

        return if status.success() {
            Ok(())
        } else {
            Err(status.code().unwrap_or(1))
        };
    };

    // --cross-backend gates, cheap and CLI-shape-only, checked before any
    // toolchain probing so a malformed invocation fails fast: the value must
    // be one v0 supports, the pairing with the invariant is a strict
    // biconditional (both directions refused), and --seed / --mc-* can never
    // combine with it (Monte Carlo requires Random, which cross-backend
    // refuses anyway a few lines down once the effect policy is derived).
    if let Some(target) = cross_backend {
        if target != "rust" {
            eprintln!("Unsupported --cross-backend target '{target}'. v0 supports: rust");
            return Err(1);
        }
    }
    let cross_backend_paired =
        cross_backend.is_some() == (invariant_name == CROSS_BACKEND_INVARIANT);
    if !cross_backend_paired {
        if cross_backend.is_some() {
            eprintln!(
                "--cross-backend requires --invariant cross-backend (the cross-backend receipt IS the pairing)"
            );
        } else {
            eprintln!(
                "--invariant cross-backend requires --cross-backend <TARGET> (v0 supports rust)"
            );
        }
        return Err(1);
    }
    if cross_backend.is_some() {
        if seed.is_some() {
            eprintln!(
                "--cross-backend does not support --seed (the Rust validation lane has no seeded PRNG builtin, so a Random-observing stream could not agree across backends)"
            );
            return Err(1);
        }
        // This transitively refuses --cross-backend --mc-executed too:
        // --mc-executed requires mc_flag_count == 3 (checked above), which
        // this arm already refuses whenever any mc flag is present.
        if mc_flag_count > 0 {
            eprintln!(
                "--cross-backend does not support --mc-* (Monte Carlo requires the Random capability, which --cross-backend already refuses)"
            );
            return Err(1);
        }
    }

    // --emit-receipt path: probe the toolchain FIRST (its identity is sealed
    // into the receipt's compiler_branch block; without a C compiler the
    // compile below would fail anyway, but the receipt must not be emitted
    // with fabricated toolchain facts), then compile + run + capture via the
    // shared helper so the emitted series is observed through the exact path
    // `receipt verify` re-runs.
    let Some(mut toolchain) = probe_c_toolchain(true) else {
        eprintln!(
            "Error: could not establish toolchain facts (no C compiler available, or the buildc binary could not be hashed); cannot emit a scientific-runtime receipt"
        );
        return Err(1);
    };

    // Probe rustc beside the C toolchain probe, BEFORE the primary run: a
    // missing rustc refuses here so no compile/run work is wasted on a
    // cross-backend request that cannot complete.
    let rustc_probe = if cross_backend.is_some() {
        match probe_rustc_toolchain() {
            Some(probe) => Some(probe),
            None => {
                eprintln!(
                    "Error: rustc not found; install the Rust toolchain (https://rustup.rs) to use --cross-backend rust"
                );
                return Err(1);
            }
        }
    } else {
        None
    };

    // Derive the effect/capability facts BEFORE running: the seed pairing is
    // an operator-level contract checked up front (fail closed), not a
    // runtime trap discovered mid-capture. A Random-using program requires a
    // sealed seed (an unseeded stream cannot be re-derived by verify), and a
    // seed on a program with no Random capability is a knob nothing reads,
    // which a receipt must not seal as if it were witnessed.
    let outcome = run_check(file)?;
    let effect_policy = derive_effect_policy(&outcome);

    // The propose/dispose admission rule, checked before any other gate: a
    // Model-observing program cannot emit a scientific receipt outright.
    // Models propose; oracles dispose. This is not a seed-style pairing
    // (there is no flag that would make a Model program admissible), so it
    // short-circuits ahead of the seed gates rather than joining them.
    let uses_model = effect_policy
        .observed_capabilities
        .iter()
        .any(|cap| cap == "Model");
    if uses_model {
        eprintln!(
            "Error: this program observes the Model capability and cannot emit a scientific receipt: models propose, oracles dispose. Run the model as a proposer and verify its output with a model-free oracled kernel."
        );
        return Err(1);
    }

    let uses_random = effect_policy
        .observed_capabilities
        .iter()
        .any(|cap| cap == "Random");
    if cross_backend.is_some() && uses_random {
        eprintln!(
            "Error: --cross-backend refuses a Random-observing kernel: the Rust lane has no seeded PRNG builtin, so the streams could not agree"
        );
        return Err(1);
    }
    if uses_random && seed.is_none() {
        eprintln!(
            "Error: this program observes the Random capability; a receipt requires an explicit seed (`--seed N`), which is sealed so `receipt verify` re-runs the same stream"
        );
        return Err(1);
    }
    if !uses_random && seed.is_some() {
        eprintln!(
            "Error: --seed was given but the program observes no Random capability (nothing draws from a seed; refusing to seal an unconsumed knob)"
        );
        return Err(1);
    }
    if monte_carlo.is_some() && !uses_random {
        eprintln!(
            "Error: --mc-* flags declare a Monte Carlo estimate, but the program observes no Random capability (nothing samples; refusing to seal an MC block no stream backs)"
        );
        return Err(1);
    }

    let captured = compile_and_capture_run(file, args, Some(&toolchain.c_compiler), seed)?;
    toolchain.program_executable_digest = captured.executable_digest.clone();

    // The wall ceiling's exceeded flag is DERIVED here, from the SEALED
    // measurement just captured, never from a re-run's re-measured time (see
    // `evaluate_scientific_runtime_receipt`'s wall contract, which re-checks
    // exactly this derivation from the two sealed numbers).
    if let Some(limit) = budget_wall_seconds {
        if let Some(b) = budget.as_mut() {
            b.wall_seconds_limit = Some(limit);
            b.wall_exceeded = Some(captured.wall_seconds > limit);
        }
    }

    // Seal the raw stdout bytes (the pass-0122 runtime_branch output hash):
    // computed over the EXACT captured bytes, before any parsing.
    let raw_stdout_digest = ScientificDigest {
        algorithm: "sha256".to_string(),
        hex: source_digest_hex(&captured.stdout_bytes),
    };

    // Echo the captured streams so `run --emit-receipt` still shows program
    // output. stdout goes to the real stdout ONLY when the receipt is written
    // to a file; when the receipt itself is written to stdout ('-') we route
    // the echoed program output to stderr to keep stdout pure JSON.
    let captured_stdout = captured.stdout;
    let receipt_to_stdout = receipt_path == Path::new("-");
    {
        use std::io::Write as _;
        if receipt_to_stdout {
            let _ = std::io::stderr().write_all(&captured.stdout_bytes);
        } else {
            let _ = std::io::stdout().write_all(&captured.stdout_bytes);
        }
        let _ = std::io::stderr().write_all(&captured.stderr_bytes);
    }

    let exit_code = captured.exit_code;

    // Parse the captured stdout into an f64 series. `token.parse::<f64>()`
    // accepts BOTH the C `%g` plain-decimal (`0.530827`) and scientific
    // (`1.59908e+28`) forms the backend emits. A non-finite (inf/NaN) value
    // marks the run as diverged -> UNVERIFIABLE, and only the finite prefix is
    // retained so the receipt always serializes cleanly.
    let parsed = parse_numeric_series(&captured_stdout);
    let series_parsed = parsed.any_parsed;
    let diverged = parsed.diverged;
    let primary_series = parsed.series;

    // The secondary (cross-backend) pipeline: emit the SAME resolved source
    // through the Rust backend, compile it with rustc, run it with the same
    // trailing args (no seed is ever set here, since --cross-backend already
    // refused a Random-observing kernel), and capture its stdout. The
    // secondary's stdout is never echoed (primary-only echo, above).
    let (series, column_count, cross_backend_block) = if let Some(target) = cross_backend {
        let rustc = rustc_probe
            .as_ref()
            .expect("rustc_probe is Some whenever cross_backend is Some");
        let secondary = compile_and_capture_rust_run(file, args, &rustc.path)?;
        let secondary_parsed = parse_numeric_series(&secondary.stdout);
        if diverged
            || secondary_parsed.diverged
            || primary_series.is_empty()
            || secondary_parsed.series.is_empty()
            || primary_series.len() != secondary_parsed.series.len()
        {
            eprintln!(
                "Error: cross-backend series mismatch: the C anchor produced {} values (diverged={}), the Rust lane produced {} values (diverged={})",
                primary_series.len(),
                diverged,
                secondary_parsed.series.len(),
                secondary_parsed.diverged
            );
            return Err(1);
        }
        let mut interleaved = Vec::with_capacity(primary_series.len() * 2);
        for (c, r) in primary_series.iter().zip(secondary_parsed.series.iter()) {
            interleaved.push(*c);
            interleaved.push(*r);
        }
        let block = ScientificCrossBackend {
            secondary_target: target.to_string(),
            secondary_toolchain_version: rustc.version_line.clone(),
            secondary_toolchain_digest: rustc.version_output_digest.clone(),
            secondary_executable_digest: secondary.executable_digest.clone(),
            secondary_raw_stdout_digest: ScientificDigest {
                algorithm: "sha256".to_string(),
                hex: source_digest_hex(&secondary.stdout_bytes),
            },
            secondary_exit_code: secondary.exit_code,
            status: "EXECUTED".to_string(),
        };
        (interleaved, 2usize, Some(block))
    } else {
        (primary_series, columns, None)
    };

    // --mc-executed finalization: the early `monte_carlo` block could only
    // build the DECLARED shape (whether it is EXECUTED-and-coherent was
    // unknown until the real series existed). Recompute now, fail closed:
    // an incoherent EXECUTED block never reaches
    // `build_scientific_runtime_receipt` (never gets sealed).
    let monte_carlo = if mc_executed {
        let mc = monte_carlo.expect("mc_executed implies mc_flag_count == 3, checked above");
        let computed = compute_mc_executed(&series, column_count, mc.samples, &mc.interval_method)
            .map_err(|reason| {
                eprintln!(
                    "Error: --mc-executed refuses to seal an incoherent EXECUTED block: {reason}"
                );
                1i32
            })?;
        Some(ScientificMonteCarlo {
            status: "EXECUTED".to_string(),
            estimate: Some(computed.estimate),
            interval_low: Some(computed.interval_low),
            interval_high: Some(computed.interval_high),
            n_effective: Some(computed.n_effective),
            successes: Some(computed.successes),
            ..mc
        })
    } else {
        monte_carlo
    };

    let os = std::env::consts::OS.to_string();
    let mut flags = vec![format!("invariant={invariant}"), format!("metric={metric}")];
    if negative_fixture {
        flags.push("negative-fixture".to_string());
    }

    let inputs = ScientificReceiptInputs {
        source_path: file,
        compiler_version: buildlang::VERSION,
        language_version: outcome.language_version.clone(),
        source_digest: ScientificDigest {
            algorithm: outcome.source_digest.algorithm.to_string(),
            hex: outcome.source_digest.hex.clone(),
        },
        input_graph_digest: ScientificDigest {
            algorithm: outcome.input_graph_digest.algorithm.to_string(),
            hex: outcome.input_graph_digest.hex.clone(),
        },
        target: "c",
        os: &os,
        exit_code,
        wall_seconds: Some(captured.wall_seconds),
        toolchain,
        effect_policy,
        method_description: method.map(str::to_string),
        raw_stdout_digest,
        series,
        series_parsed,
        diverged,
        args: args.to_vec(),
        seed_value: seed,
        monte_carlo,
        budget,
        cross_backend: cross_backend_block,
        invariant_name: invariant_name.to_string(),
        metric: metric.to_string(),
        units: canonical_units.clone(),
        column_count,
        problem_label: problem.map(str::to_string),
        negative_fixture,
        flags,
    };

    let receipt = build_scientific_runtime_receipt(inputs);
    write_scientific_runtime_receipt(receipt_path, &receipt)?;

    // Exit-code semantics: emitting the receipt is the success signal. A
    // program may exit nonzero (or, as a negative fixture, exit 0 while
    // printing an increasing series); either way the receipt is written and
    // records the observed exit_code and verdict. We return Ok so the emit
    // succeeds; the receipt's receipt_status carries the PASS/FAIL verdict.
    Ok(())
}

fn write_scientific_runtime_receipt(
    path: &Path,
    receipt: &ScientificRuntimeReceipt,
) -> Result<(), i32> {
    let json = serde_json::to_string_pretty(receipt).map_err(|err| {
        eprintln!("Error serializing scientific-runtime receipt: {}", err);
        1
    })?;
    if path == Path::new("-") {
        println!("{}", json);
        Ok(())
    } else {
        std::fs::write(path, format!("{}\n", json)).map_err(|err| {
            eprintln!(
                "Error writing scientific-runtime receipt '{}': {}",
                path.display(),
                err
            );
            1
        })
    }
}

fn cmd_test(
    directory: &PathBuf,
    filter: Option<&str>,
    verbose: bool,
    no_fail_fast: bool,
) -> Result<(), i32> {
    // Discover .bld test files
    let entries: Vec<_> = match std::fs::read_dir(directory) {
        Ok(dir) => dir
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .map(|ext| ext == "bld")
                    .unwrap_or(false)
            })
            .collect(),
        Err(e) => {
            eprintln!(
                "Error reading test directory '{}': {}",
                directory.display(),
                e
            );
            return Err(1);
        }
    };

    let mut tests: Vec<PathBuf> = entries.iter().map(|e| e.path()).collect();
    tests.sort();

    // Apply filter
    if let Some(pattern) = filter {
        tests.retain(|t| {
            t.file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.contains(pattern))
                .unwrap_or(false)
        });
    }

    // Only include tests that have .expected files
    let test_pairs: Vec<(PathBuf, PathBuf)> = tests
        .iter()
        .filter_map(|build_file| {
            let expected = build_file.with_extension("expected");
            if expected.exists() {
                Some((build_file.clone(), expected))
            } else {
                None
            }
        })
        .collect();

    let total = test_pairs.len();
    let skipped = tests.len() - total;
    if total == 0 {
        println!(
            "No tests found with .expected files in '{}'",
            directory.display()
        );
        return Ok(());
    }

    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut errors = 0usize;
    let mut failures: Vec<String> = Vec::new();

    println!("running {} tests\n", total);

    for (build_file, expected_file) in &test_pairs {
        let name = build_file
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("???");

        // --- Compile and capture output ---
        let result = (|| -> Result<String, String> {
            let source = std::fs::read_to_string(build_file).map_err(|e| format!("read: {}", e))?;
            let source = resolve_imports(&source, build_file).map_err(|_| "import".to_string())?;
            let run_base = build_file.parent().unwrap_or(Path::new("."));
            let source =
                preprocess_includes(&source, run_base).map_err(|_| "include".to_string())?;

            let source_file = SourceFile::new(build_file.to_string_lossy(), source);
            let mut lexer = Lexer::new(&source_file);
            let tokens = lexer.tokenize().map_err(|e| format!("lex: {}", e))?;
            let mut parser = Parser::new(&source_file, tokens);
            let mut ast = parser.parse().map_err(|e| format!("parse: {}", e))?;
            // A recovered (truncated) AST would run tests against dropped code and
            // could report a false pass, so treat any recovered parse error as a
            // test error rather than compile the remainder.
            if !parser.errors().is_empty() {
                return Err(format!("parse: {} error(s)", parser.errors().len()));
            }

            let source_dir = build_file.parent().unwrap_or(Path::new("."));
            let _ = resolve_modules(&mut ast, source_dir);

            let mut ctx = TypeContext::new();
            let mut checker = TypeChecker::new(&mut ctx);
            checker.set_source_file(&source_file);
            checker.set_source_dir(source_dir.to_path_buf());
            checker.check_module(&ast);
            if checker.has_errors() {
                let errs: Vec<_> = checker.errors().iter().map(|e| e.to_string()).collect();
                return Err(format!("type: {}", errs.join("; ")));
            }

            let mut codegen =
                CodeGenerator::with_source(&ctx, Target::C, Arc::from(source_file.source()));
            let output = codegen
                .generate(&ast)
                .map_err(|e| format!("codegen: {}", e))?;
            if !codegen.linear_errors().is_empty() {
                let errs: Vec<_> = codegen
                    .linear_errors()
                    .iter()
                    .map(|e| e.to_string())
                    .collect();
                return Err(format!("linear: {}", errs.join("; ")));
            }

            // Use a unique temp directory per test to avoid MSVC bat conflicts
            let test_dir = std::env::temp_dir().join(format!("buildtest_{}", name));
            let _ = std::fs::create_dir_all(&test_dir);
            let c_file = test_dir.join("main.c");
            let exe_file = test_dir.join(if cfg!(windows) { "main.exe" } else { "main" });

            std::fs::write(&c_file, &output.data).map_err(|e| format!("write: {}", e))?;

            let compiler = find_c_compiler().ok_or_else(|| "no C compiler".to_string())?;
            invoke_c_compiler(&compiler, &c_file, &exe_file, false, &output.link_libraries)
                .map_err(|_| "cc".to_string())?;

            // MSVC bat outputs temp.exe in the c_file directory
            if !exe_file.exists() {
                let alt = test_dir.join("temp.exe");
                if alt.exists() {
                    let _ = std::fs::rename(&alt, &exe_file);
                }
            }
            if !exe_file.exists() {
                return Err("exe not created (link failed)".to_string());
            }

            let run_output = std::process::Command::new(&exe_file)
                .output()
                .map_err(|e| format!("run: {}", e))?;

            let _ = std::fs::remove_dir_all(&test_dir);

            let stdout = String::from_utf8_lossy(&run_output.stdout).replace("\r\n", "\n");
            Ok(stdout)
        })();

        match result {
            Ok(actual) => {
                let expected = std::fs::read_to_string(expected_file)
                    .unwrap_or_default()
                    .replace("\r\n", "\n");

                if actual.trim_end() == expected.trim_end() {
                    passed += 1;
                    println!("test {} ... \x1b[32mok\x1b[0m", name);
                    if verbose {
                        for line in actual.lines() {
                            println!("  {}", line);
                        }
                    }
                } else {
                    failed += 1;
                    println!("test {} ... \x1b[31mFAILED\x1b[0m", name);
                    failures.push(format!(
                        "---- {} ----\nexpected:\n{}\nactual:\n{}\n",
                        name,
                        expected.trim_end(),
                        actual.trim_end()
                    ));
                    if !no_fail_fast {
                        break;
                    }
                }
            }
            Err(stage) => {
                errors += 1;
                println!("test {} ... \x1b[33mERROR\x1b[0m ({})", name, stage);
                if !no_fail_fast {
                    break;
                }
            }
        }
    }

    // Summary
    println!();
    if !failures.is_empty() {
        println!("failures:\n");
        for f in &failures {
            println!("{}", f);
        }
    }

    let status = if failed == 0 && errors == 0 {
        "\x1b[32mok\x1b[0m"
    } else {
        "\x1b[31mFAILED\x1b[0m"
    };
    println!(
        "test result: {}. {} passed; {} failed; {} errors; {} skipped\n",
        status, passed, failed, errors, skipped
    );

    if failed > 0 || errors > 0 {
        Err(1)
    } else {
        Ok(())
    }
}

fn cmd_lint(file: &PathBuf) -> Result<(), i32> {
    let source = std::fs::read_to_string(file).map_err(|e| {
        eprintln!("Error reading file '{}': {}", file.display(), e);
        1
    })?;

    let source = resolve_imports(&source, file)?;
    let base = file.parent().unwrap_or(Path::new("."));
    let source = preprocess_includes(&source, base)?;

    let source_file = SourceFile::new(file.to_string_lossy(), source.clone());

    // Lex
    let mut lexer = Lexer::new(&source_file);
    let tokens = lexer.tokenize().map_err(|e| {
        eprintln!("Lexer error: {}", e);
        1
    })?;

    // Parse
    let mut parser = Parser::new(&source_file, tokens);
    let mut ast = parser.parse().map_err(|e| {
        eprintln!("Parse error: {}", e);
        1
    })?;

    resolve_modules(&mut ast, base)?;

    // Type check
    let mut ctx = TypeContext::new();
    let mut checker = TypeChecker::new(&mut ctx);
    checker.set_source_file(&source_file);
    checker.set_source_dir(base.to_path_buf());
    checker.check_module(&ast);

    let mut warnings = 0u32;
    let mut errors = 0u32;

    // Report type errors
    for err in checker.errors() {
        let span = err.span;
        let pos = source_file.lookup_position(span.start);
        eprintln!(
            "\x1b[31merror\x1b[0m: {} ({}:{}:{})",
            err,
            file.display(),
            pos.line,
            pos.column
        );
        errors += 1;
    }

    // Report parse errors
    for err in parser.errors() {
        eprintln!("\x1b[31merror\x1b[0m: {} ({})", err, file.display());
        errors += 1;
    }

    // Lint checks: style warnings
    for (line_num, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        let line_num = line_num + 1;

        // Trailing whitespace
        if line.len() > trimmed.len() + (line.len() - line.trim_end().len())
            && line.trim_end().len() < line.len()
        {
            eprintln!(
                "\x1b[33mwarning\x1b[0m: trailing whitespace ({}:{})",
                file.display(),
                line_num
            );
            warnings += 1;
        }

        // TODO/FIXME markers
        if trimmed.contains("TODO") || trimmed.contains("FIXME") || trimmed.contains("HACK") {
            eprintln!(
                "\x1b[33mwarning\x1b[0m: {} ({}:{})",
                if trimmed.contains("TODO") {
                    "TODO marker"
                } else if trimmed.contains("FIXME") {
                    "FIXME marker"
                } else {
                    "HACK marker"
                },
                file.display(),
                line_num
            );
            warnings += 1;
        }

        // Lines > 120 chars
        if line.len() > 120 {
            eprintln!(
                "\x1b[33mwarning\x1b[0m: line exceeds 120 characters ({} chars) ({}:{})",
                line.len(),
                file.display(),
                line_num
            );
            warnings += 1;
        }
    }

    // Summary
    if errors == 0 && warnings == 0 {
        println!("No issues found in '{}'", file.display());
    } else {
        println!(
            "{} error(s), {} warning(s) in '{}'",
            errors,
            warnings,
            file.display()
        );
    }

    if errors > 0 {
        Err(1)
    } else {
        Ok(())
    }
}

fn cmd_repl() -> Result<(), i32> {
    println!("BuildLang REPL v{}", buildlang::VERSION);
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
                    println!("Or enter BuildLang code to parse and analyze.");
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
                                    checker.set_source_file(&file);
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
                        checker.set_source_file(&file);
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
    eprintln!(
        "BuildLang LSP server v{} starting on stdio...",
        buildlang::VERSION
    );

    match buildlang::lsp::run_server() {
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

    let formatter = buildlang::fmt::Formatter::default_formatter();
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
            let manifest_path = path.join("Build.toml");
            if manifest_path.exists() {
                eprintln!("Build.toml already exists in {}", path.display());
                return Err(1);
            }
            let dir_name = path
                .canonicalize()
                .ok()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                .unwrap_or_else(|| "my-project".to_string());
            let manifest = format!(
                "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2026\"\n\n[dependencies]\n",
                dir_name
            );
            std::fs::write(&manifest_path, &manifest).map_err(|e| {
                eprintln!("Error creating Build.toml: {}", e);
                1
            })?;
            println!("Created {}", manifest_path.display());
            Ok(())
        }
        PkgCommands::Add { name, version } => {
            let manifest_path = Path::new("Build.toml");
            if !manifest_path.exists() {
                eprintln!("No Build.toml found. Run `buildc pkg init` first.");
                return Err(1);
            }
            let mut content = std::fs::read_to_string(manifest_path).map_err(|e| {
                eprintln!("Error reading Build.toml: {}", e);
                1
            })?;
            let ver = version.unwrap_or_else(|| "*".to_string());
            content.push_str(&format!("{} = \"{}\"\n", name, ver));
            std::fs::write(manifest_path, &content).map_err(|e| {
                eprintln!("Error writing Build.toml: {}", e);
                1
            })?;
            println!("Added {} = \"{}\"", name, ver);
            Ok(())
        }
        PkgCommands::Resolve { path } => {
            let manifest_path = path.join("Build.toml");
            if !manifest_path.exists() {
                eprintln!("No Build.toml found in {}", path.display());
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
                        if dep_name.is_empty() {
                            continue;
                        }
                        if let Some(entry) = index.get(dep_name) {
                            println!(
                                "  {} = {} ... found ({})",
                                dep_name, entry.version, entry.description
                            );
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
/// 1. Looks for `foo.bld` in the same directory, or `foo/mod.bld`
/// 2. Parses that file
/// 3. Recursively resolves sub-module declarations
/// 4. Collects all item names defined in the module
/// 5. Prefixes each definition with `foo_` (functions, structs, enums)
/// 6. Renames intra-module references in function bodies
/// 7. Appends the prefixed items into the main AST
///
/// Multi-segment paths like `foo::bar::baz()` resolve to `foo_bar_baz`
/// during lowering since lower_path joins segments with `_`.
/// Find the stdlib directory. Searches:
/// 1. `stdlib/` relative to the compiler executable
/// 2. `../stdlib/` relative to the compiler executable (for dev builds)
/// 3. `BUILDLANG_STDLIB` environment variable
fn find_stdlib_path() -> Option<PathBuf> {
    // Check env var first
    if let Ok(path) = std::env::var("BUILDLANG_STDLIB") {
        let p = PathBuf::from(path);
        if p.is_dir() {
            return Some(p);
        }
    }
    // Relative to the compiler executable
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            // stdlib/ next to the executable
            let candidate = exe_dir.join("stdlib");
            if candidate.is_dir() {
                return Some(candidate);
            }
            // ../stdlib/ (dev layout: compiler/target/release/buildc → ../../stdlib)
            for ancestor in exe_dir.ancestors().skip(1).take(4) {
                let candidate = ancestor.join("stdlib");
                if candidate.is_dir() {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

fn resolve_modules(ast: &mut Module, source_dir: &Path) -> Result<(), i32> {
    let mut ledger = None;
    let mut visiting = HashSet::new();
    resolve_modules_with_prefix(ast, source_dir, "", &mut ledger, &mut visiting)
}

fn resolve_modules_recording_inputs(
    ast: &mut Module,
    source_dir: &Path,
    ledger: &mut InputDigestLedger,
) -> Result<(), i32> {
    let mut ledger = Some(ledger);
    let mut visiting = HashSet::new();
    resolve_modules_with_prefix(ast, source_dir, "", &mut ledger, &mut visiting)
}

/// Resolve modules with a prefix for nested module support.
/// The prefix is prepended to all mangled names (e.g., "utils_" for sub-modules of utils).
///
/// `visiting` holds the canonical paths of the module files on the current
/// resolution stack. It turns an import cycle -- including a `mod NAME;` in a
/// file that resolves back to a file already being resolved -- into a
/// fail-closed diagnostic instead of unbounded recursion.
fn resolve_modules_with_prefix(
    ast: &mut Module,
    source_dir: &Path,
    prefix: &str,
    ledger: &mut Option<&mut InputDigestLedger>,
    visiting: &mut HashSet<PathBuf>,
) -> Result<(), i32> {
    // Collect module names from `mod foo;` declarations (content == None).
    //
    // A file-level `module NAME` header is skipped here: it names the module
    // the file provides, it is not a request to load `NAME.bld`. Loading it
    // would read the declaring file itself whenever the file is named after
    // its module (e.g. `benchmarks.bld` declaring `module benchmarks`), which
    // recurses forever.
    let mod_names: Vec<String> = ast
        .items
        .iter()
        .filter_map(|item| {
            if let ItemKind::Mod(ref m) = item.kind {
                if m.content.is_none() && !m.is_file_module {
                    return Some(m.name.name.to_string());
                }
            }
            None
        })
        .collect();

    if mod_names.is_empty() {
        return Ok(());
    }

    let mut new_items: Vec<ast::Item> = Vec::new();

    for mod_name in &mod_names {
        // Look for foo.bld or foo/mod.bld
        let mod_file = source_dir.join(format!("{}.bld", mod_name));
        let mod_dir_file = source_dir.join(mod_name).join("mod.bld");

        // Search order: source directory → stdlib directory → skip
        let stdlib_file = find_stdlib_path().map(|p| p.join(format!("{}.bld", mod_name)));

        let (actual_file, sub_source_dir) = if mod_file.exists() {
            (mod_file, source_dir.to_path_buf())
        } else if mod_dir_file.exists() {
            (mod_dir_file, source_dir.join(mod_name))
        } else if let Some(ref sf) = stdlib_file {
            if sf.exists() {
                (
                    sf.clone(),
                    sf.parent().unwrap_or(Path::new(".")).to_path_buf(),
                )
            } else {
                continue;
            }
        } else {
            continue;
        };

        // Fail-closed cycle guard: if this module file is already on the
        // resolution stack, a `mod` import cycle (a->b->a, or a file importing
        // itself) would recurse forever. Emit a diagnostic instead of hanging.
        // Canonicalize so the same file reached by two spellings compares equal.
        let module_key =
            std::fs::canonicalize(&actual_file).unwrap_or_else(|_| actual_file.clone());
        if !visiting.insert(module_key.clone()) {
            eprintln!(
                "error: module import cycle detected at '{}' (module '{}' is already being resolved)",
                actual_file.display(),
                mod_name
            );
            return Err(1);
        }

        // Read and parse the module file
        let mod_bytes = std::fs::read(&actual_file).map_err(|e| {
            eprintln!(
                "Error reading module file '{}': {}",
                actual_file.display(),
                e
            );
            1
        })?;
        if let Some(ledger) = ledger.as_deref_mut() {
            ledger.record("module", &actual_file, &mod_bytes);
        }
        let mod_source = String::from_utf8(mod_bytes).map_err(|e| {
            eprintln!(
                "Error reading module file '{}': {}",
                actual_file.display(),
                e
            );
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
        report_parse_errors(&actual_file, &mod_source_file, mod_parser.errors())?;

        // The full prefix for this module's items
        let full_prefix = if prefix.is_empty() {
            mod_name.clone()
        } else {
            format!("{}_{}", prefix, mod_name)
        };

        // Recursively resolve sub-modules within this module
        resolve_modules_with_prefix(&mut mod_ast, &sub_source_dir, &full_prefix, ledger, visiting)?;
        // Done with this module's subtree; drop it from the stack so a sibling
        // branch may legitimately include the same module again (a diamond is
        // not a cycle).
        visiting.remove(&module_key);

        // Collect names defined in this module (for intra-module rewriting)
        let mod_defined: std::collections::HashSet<String> = mod_ast
            .items
            .iter()
            .filter_map(|item| match &item.kind {
                ItemKind::Function(f) => Some(f.name.name.to_string()),
                _ => None,
            })
            .collect();

        // Merge module items with name prefixing.
        // Functions are prefixed: `add` → `math_helpers_add`
        // This matches how lower_path joins path segments with `_`:
        // `math_helpers::add(...)` emits a call to `math_helpers_add`.
        for item in mod_ast.items {
            match item.kind {
                ItemKind::Function(f) => {
                    let mut prefixed_fn = *f;
                    let original_name = prefixed_fn.name.name.to_string();
                    prefixed_fn.name = ast::Ident {
                        name: Arc::from(format!("{}_{}", full_prefix, original_name)),
                        span: prefixed_fn.name.span,
                    };
                    // Rewrite intra-module calls in the function body:
                    // if this function calls `helper()` and `helper` is defined
                    // in the same module, rewrite to `math_helpers_helper()`.
                    // Seed the shadow scope with this function's parameter names
                    // first so a parameter shadowing a sibling module function is
                    // not rewritten.
                    let mut param_names = HashSet::new();
                    for p in &prefixed_fn.sig.params {
                        collect_pattern_names(&p.pattern, &mut param_names);
                    }
                    if let Some(ref mut body) = prefixed_fn.body {
                        rewrite_intra_module_calls(body, &mod_defined, &full_prefix, param_names);
                    }
                    new_items.push(ast::Item::new(
                        ItemKind::Function(Box::new(prefixed_fn)),
                        Visibility::default(),
                        Vec::new(),
                        Span::dummy(),
                    ));
                }
                ItemKind::Struct(_) | ItemKind::Enum(_) | ItemKind::Impl(_) => {
                    new_items.push(item);
                }
                _ => {
                    new_items.push(item);
                }
            }
        }
    }

    // Build a map of all imported function names: bare_name → prefixed_name
    let mut imported_fns: HashMap<String, String> = HashMap::new();
    for item in &new_items {
        if let ItemKind::Function(f) = &item.kind {
            let prefixed = f.name.name.to_string();
            // Extract the bare name by stripping the module prefix
            // e.g., "core_i32_min" → "i32_min", "math_lerp_f64" → "lerp_f64"
            for mod_name in &mod_names {
                let module_prefix = if prefix.is_empty() {
                    mod_name.clone()
                } else {
                    format!("{}_{}", prefix, mod_name)
                };
                let prefix_with_sep = format!("{}_", module_prefix);
                if let Some(bare) = prefixed.strip_prefix(&prefix_with_sep) {
                    imported_fns.insert(bare.to_string(), prefixed.clone());
                }
            }
        }
    }

    // Append module items to the main AST
    ast.items.extend(new_items);

    // Rewrite calls in the main program's existing functions to use prefixed names
    if !imported_fns.is_empty() {
        for item in &mut ast.items {
            if let ItemKind::Function(f) = &mut item.kind {
                let mut param_names = HashSet::new();
                for p in &f.sig.params {
                    collect_pattern_names(&p.pattern, &mut param_names);
                }
                if let Some(ref mut body) = f.body {
                    rewrite_imported_calls(body, &imported_fns, param_names);
                }
            }
        }
    }

    Ok(())
}

/// Names bound by a pattern (function params, `let`, closure params, loop and
/// match patterns). Used by the module-call rewriter to detect when a callee
/// identifier is actually a shadowing local/parameter rather than a free
/// reference to a module function. Over-collecting is the safe direction (it
/// only ever suppresses a rewrite, turning a would-be silent miscompile into
/// either correct resolution or a loud name error); under-collecting is the
/// dangerous direction, so every binding pattern kind is covered.
fn collect_pattern_names(pat: &ast::Pattern, out: &mut HashSet<Arc<str>>) {
    use ast::PatternKind as P;
    match &pat.kind {
        P::Ident {
            name, subpattern, ..
        } => {
            out.insert(name.name.clone());
            if let Some(sub) = subpattern {
                collect_pattern_names(sub, out);
            }
        }
        P::Tuple(pats) | P::Slice(pats) | P::Or(pats) => {
            for p in pats {
                collect_pattern_names(p, out);
            }
        }
        P::TupleStruct { patterns, .. } => {
            for p in patterns {
                collect_pattern_names(p, out);
            }
        }
        P::Struct { fields, .. } => {
            for f in fields {
                collect_pattern_names(&f.pattern, out);
            }
        }
        P::Ref { pattern, .. } | P::Box(pattern) | P::Paren(pattern) => {
            collect_pattern_names(pattern, out);
        }
        P::Wildcard
        | P::Rest
        | P::Literal(_)
        | P::Path(_)
        | P::Range { .. }
        | P::Macro { .. }
        | P::Error => {}
    }
}

/// A lexical shadow tracker for the module-call rewriter: a stack of scope
/// frames, each holding the names bound in that scope. A callee identifier that
/// is bound in ANY live frame is a local/parameter shadowing a module function
/// of the same name, so it must NOT be prefix-rewritten. Without this, the
/// rewriter renamed a call purely by name-set membership and silently
/// retargeted a fn-pointer parameter (e.g. `fn run(square: fn(i32)->i32) {
/// square(x) }`) to a same-named module function -- a wrong-callee miscompile.
#[derive(Default)]
struct ShadowScope {
    frames: Vec<HashSet<Arc<str>>>,
}

impl ShadowScope {
    fn push(&mut self, names: HashSet<Arc<str>>) {
        self.frames.push(names);
    }

    fn push_empty(&mut self) {
        self.frames.push(HashSet::new());
    }

    fn pop(&mut self) {
        self.frames.pop();
    }

    /// Add a name to the current (innermost) frame, e.g. for a `let` binding as
    /// the walk passes it.
    fn bind(&mut self, name: Arc<str>) {
        if let Some(top) = self.frames.last_mut() {
            top.insert(name);
        }
    }

    fn is_bound(&self, name: &str) -> bool {
        self.frames.iter().any(|frame| frame.contains(name))
    }
}

/// Collect the names a pattern binds into a fresh scope frame.
fn scope_frame_for_pattern(pat: &ast::Pattern) -> HashSet<Arc<str>> {
    let mut names = HashSet::new();
    collect_pattern_names(pat, &mut names);
    names
}

/// Rewrite calls to module-local functions within a function body. `param_names`
/// seeds the shadow scope with the function's parameter names so a parameter
/// that shadows a sibling module function is left as the parameter reference.
fn rewrite_intra_module_calls(
    body: &mut ast::Block,
    mod_defined: &HashSet<String>,
    prefix: &str,
    param_names: HashSet<Arc<str>>,
) {
    let mut scope = ShadowScope::default();
    scope.push(param_names);
    walk_calls_in_block(body, &mut scope, &mut |ident: &mut ast::Ident| {
        if mod_defined.contains(ident.name.as_ref()) {
            ident.name = Arc::from(format!("{}_{}", prefix, ident.name));
        }
    });
}

/// Walk EVERY call expression reachable from a block and apply `rename` to each
/// call's callee identifier that is NOT shadowed by a local/parameter binding.
/// This is the single, EXHAUSTIVE structural walk shared by both intra-module
/// prefixing and main-program imported-call rewriting. Using one exhaustive
/// walker (rather than two ad-hoc walkers that each descended into only a
/// handful of contexts and drifted apart) is what fixes bare module calls
/// failing to resolve inside loop bodies, match arms, method-call arguments,
/// and other previously un-walked contexts. The match is exhaustive on purpose:
/// a new `ExprKind` variant forces this walker to be updated instead of
/// silently regressing.
///
/// `scope` tracks the lexical bindings in force at each call site (function
/// params, `let`, closure params, and loop/match patterns), so a callee that
/// names a parameter or local is never mistaken for a free module-function
/// reference and mis-rewritten. NOTE: calls inside a `macro!(...)` invocation's
/// argument tokens are NOT rewritten - macro args are unstructured tokens, so
/// `println!("{}", vec_dot(a, b))` still needs the let-bind idiom.
fn walk_calls_in_block(
    block: &mut ast::Block,
    scope: &mut ShadowScope,
    rename: &mut dyn FnMut(&mut ast::Ident),
) {
    scope.push_empty();
    for stmt in &mut block.stmts {
        match &mut stmt.kind {
            ast::StmtKind::Expr(expr) | ast::StmtKind::Semi(expr) => {
                walk_calls_in_expr(expr, scope, rename);
            }
            ast::StmtKind::Local(local) => {
                // The RHS is evaluated in the OUTER scope (the binding is not yet
                // in force), so walk it before binding the pattern's names. The
                // let-else diverge block is also walked here, also pre-binding
                // (the let-else bindings are not in force in the else block).
                // NOTE: let-else is currently REJECTED by the type checker
                // (UnsupportedConstruct) because codegen cannot lower it; the
                // walk keeps the rewriter exhaustive so module calls in the
                // else block resolve correctly once lowering lands.
                if let Some(ref mut init) = local.init {
                    walk_calls_in_expr(&mut init.expr, scope, rename);
                    if let Some(ref mut diverge) = init.diverge {
                        walk_calls_in_expr(diverge, scope, rename);
                    }
                }
                let mut names = HashSet::new();
                collect_pattern_names(&local.pattern, &mut names);
                for n in names {
                    scope.bind(n);
                }
            }
            ast::StmtKind::Item(_) | ast::StmtKind::Empty | ast::StmtKind::Macro { .. } => {}
        }
    }
    scope.pop();
}

fn walk_calls_in_expr(
    expr: &mut ast::Expr,
    scope: &mut ShadowScope,
    rename: &mut dyn FnMut(&mut ast::Ident),
) {
    use ast::ExprKind as E;
    match &mut expr.kind {
        E::Call { func, args } => {
            if let E::Ident(ref mut ident) = func.kind {
                // Only rewrite a free reference to a module function. A callee
                // that names a bound local/parameter shadows the module function
                // and must be left alone (else a fn-pointer param call silently
                // retargets to the module function).
                if !scope.is_bound(ident.name.as_ref()) {
                    rename(ident);
                }
            }
            walk_calls_in_expr(func, scope, rename);
            for arg in args {
                walk_calls_in_expr(arg, scope, rename);
            }
        }
        E::MethodCall { receiver, args, .. } => {
            walk_calls_in_expr(receiver, scope, rename);
            for arg in args {
                walk_calls_in_expr(arg, scope, rename);
            }
        }
        E::Binary { left, right, .. } => {
            walk_calls_in_expr(left, scope, rename);
            walk_calls_in_expr(right, scope, rename);
        }
        E::Assign { target, value, .. } => {
            walk_calls_in_expr(target, scope, rename);
            walk_calls_in_expr(value, scope, rename);
        }
        E::Index { expr: e, index } => {
            walk_calls_in_expr(e, scope, rename);
            walk_calls_in_expr(index, scope, rename);
        }
        E::Unary { expr: e, .. }
        | E::Deref(e)
        | E::Ref { expr: e, .. }
        | E::Field { expr: e, .. }
        | E::TupleField { expr: e, .. }
        | E::Cast { expr: e, .. }
        | E::TypeAscription { expr: e, .. }
        | E::AIInfer { expr: e, .. }
        | E::Try(e)
        | E::Await(e)
        | E::Paren(e) => {
            walk_calls_in_expr(e, scope, rename);
        }
        E::Closure { params, body, .. } => {
            // Closure parameters shadow within the closure body only.
            let mut names = HashSet::new();
            for p in params {
                collect_pattern_names(&p.pattern, &mut names);
            }
            scope.push(names);
            walk_calls_in_expr(body, scope, rename);
            scope.pop();
        }
        E::Array(items) | E::Tuple(items) => {
            for it in items {
                walk_calls_in_expr(it, scope, rename);
            }
        }
        E::ArrayRepeat { element, count } => {
            walk_calls_in_expr(element, scope, rename);
            walk_calls_in_expr(count, scope, rename);
        }
        E::Struct { fields, rest, .. } => {
            for f in fields {
                if let Some(ref mut v) = f.value {
                    walk_calls_in_expr(v, scope, rename);
                }
            }
            if let Some(ref mut r) = rest {
                walk_calls_in_expr(r, scope, rename);
            }
        }
        E::If {
            condition,
            then_branch,
            else_branch,
        } => {
            walk_calls_in_expr(condition, scope, rename);
            walk_calls_in_block(then_branch, scope, rename);
            if let Some(ref mut eb) = else_branch {
                walk_calls_in_expr(eb, scope, rename);
            }
        }
        E::IfLet {
            pattern,
            expr: e,
            then_branch,
            else_branch,
        } => {
            walk_calls_in_expr(e, scope, rename);
            // Pattern bindings are in force only in the `then` branch.
            scope.push(scope_frame_for_pattern(pattern));
            walk_calls_in_block(then_branch, scope, rename);
            scope.pop();
            if let Some(ref mut eb) = else_branch {
                walk_calls_in_expr(eb, scope, rename);
            }
        }
        E::Match { scrutinee, arms } => {
            walk_calls_in_expr(scrutinee, scope, rename);
            for arm in arms {
                // Arm-pattern bindings are in force in the guard and body.
                scope.push(scope_frame_for_pattern(&arm.pattern));
                if let Some(ref mut g) = arm.guard {
                    walk_calls_in_expr(g, scope, rename);
                }
                walk_calls_in_expr(&mut arm.body, scope, rename);
                scope.pop();
            }
        }
        E::While {
            condition, body, ..
        } => {
            walk_calls_in_expr(condition, scope, rename);
            walk_calls_in_block(body, scope, rename);
        }
        E::WhileLet {
            pattern,
            expr: e,
            body,
            ..
        } => {
            walk_calls_in_expr(e, scope, rename);
            scope.push(scope_frame_for_pattern(pattern));
            walk_calls_in_block(body, scope, rename);
            scope.pop();
        }
        E::For {
            pattern,
            iter,
            body,
            ..
        } => {
            walk_calls_in_expr(iter, scope, rename);
            scope.push(scope_frame_for_pattern(pattern));
            walk_calls_in_block(body, scope, rename);
            scope.pop();
        }
        E::Loop { body, .. } | E::Unsafe(body) | E::Async { body, .. } => {
            walk_calls_in_block(body, scope, rename);
        }
        E::Handle { handlers, body, .. } => {
            for h in handlers {
                // Handler operation params shadow within that handler body.
                let mut names = HashSet::new();
                for p in &h.params {
                    collect_pattern_names(&p.pattern, &mut names);
                }
                scope.push(names);
                walk_calls_in_expr(&mut h.body, scope, rename);
                scope.pop();
            }
            walk_calls_in_block(body, scope, rename);
        }
        E::Block(block) => {
            walk_calls_in_block(block, scope, rename);
        }
        E::Return(opt) | E::Resume(opt) => {
            if let Some(ref mut e) = opt {
                walk_calls_in_expr(e, scope, rename);
            }
        }
        E::Break { value, .. } => {
            if let Some(ref mut e) = value {
                walk_calls_in_expr(e, scope, rename);
            }
        }
        E::Range { start, end, .. } => {
            if let Some(ref mut s) = start {
                walk_calls_in_expr(s, scope, rename);
            }
            if let Some(ref mut en) = end {
                walk_calls_in_expr(en, scope, rename);
            }
        }
        E::AIQuery { prompt, options } => {
            walk_calls_in_expr(prompt, scope, rename);
            for (_, e) in options {
                walk_calls_in_expr(e, scope, rename);
            }
        }
        E::Perform { args, .. } => {
            for arg in args {
                walk_calls_in_expr(arg, scope, rename);
            }
        }
        // Leaves and un-rewritable contexts: identifiers/paths/literals hold no
        // nested calls; `break`/`continue` labels are not exprs; macro argument
        // tokens are unstructured and cannot be structurally rewritten here.
        E::Literal(_)
        | E::Ident(_)
        | E::Path(_)
        | E::Continue { .. }
        | E::Macro { .. }
        | E::Error => {}
    }
}

/// Rewrite bare function calls in the main program to use module-prefixed names.
/// E.g., `i32_min(a, b)` → `core_i32_min(a, b)` when `core.bld` defines `i32_min`.
/// `param_names` seeds the shadow scope so a fn-pointer parameter that shadows an
/// imported function name stays a reference to the parameter.
fn rewrite_imported_calls(
    body: &mut ast::Block,
    imported: &HashMap<String, String>,
    param_names: HashSet<Arc<str>>,
) {
    let mut scope = ShadowScope::default();
    scope.push(param_names);
    walk_calls_in_block(body, &mut scope, &mut |ident: &mut ast::Ident| {
        if let Some(prefixed) = imported.get(ident.name.as_ref()) {
            ident.name = Arc::from(prefixed.as_str());
        }
    });
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

    // Resolve `// import <pkg>` and `use <pkg>;` directives
    let source = resolve_imports(&source, input)?;

    // Expand `include!("path")` directives
    let base_dir = input.parent().unwrap_or(Path::new("."));
    let source = preprocess_includes(&source, base_dir)?;

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

    report_parse_errors(input, &source_file, parser.errors())?;

    // Resolve `mod foo;` declarations - load and merge external module files
    let source_dir = input.parent().unwrap_or(Path::new("."));
    resolve_modules(&mut ast, source_dir)?;

    // Type check
    let mut ctx = TypeContext::new();
    let mut checker = TypeChecker::new(&mut ctx);
    checker.set_source_file(&source_file);
    checker.set_source_dir(source_dir.to_path_buf());
    checker.check_module(&ast);

    if checker.has_errors() {
        for err in checker.errors() {
            // Show error with source location: file:line:col
            let line = source_file.lookup_line(err.span.start);
            let line_start = source_file.line_start(line).unwrap_or(err.span.start);
            let col = err.span.start.0.saturating_sub(line_start.0) as usize;
            eprintln!(
                "error[{}:{}:{}]: {}",
                input.display(),
                line + 1,
                col + 1,
                err.error
            );

            // Show the source line with an underline
            if let Some(src_line) = source_file.source().lines().nth(line) {
                eprintln!("  {} | {}", line + 1, src_line);
                let padding = format!("{}", line + 1).len();
                let underline_pos = col;
                let underline_len =
                    (err.span.end.0.saturating_sub(err.span.start.0) as usize).max(1);
                eprintln!(
                    "  {} | {}{}",
                    " ".repeat(padding),
                    " ".repeat(underline_pos),
                    "^".repeat(underline_len.min(src_line.len().saturating_sub(underline_pos)))
                );
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
        parse_codegen_target(t).map_err(|err| {
            eprintln!("{}", err);
            1
        })?
    } else if let Some(ext) = output.and_then(|p| p.extension()).and_then(|e| e.to_str()) {
        target_from_extension(ext).unwrap_or(Target::C)
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
    if !codegen.linear_errors().is_empty() {
        eprintln!("Linear type errors found:");
        for err in codegen.linear_errors() {
            eprintln!("  {}", err);
        }
        return Err(1);
    }

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

    // SPIR-V: validate the emitted module with spirv-val (if present) and FAIL
    // the build on a non-zero exit. Absence of the tool is a graceful skip. This
    // makes `buildc kernel.bld --target spirv` an honest gate: what it emits is
    // valid dispatchable SPIR-V, checked by an external validator.
    if target == Target::SpirV {
        match validate_spirv_module(&output_path) {
            Ok(()) => {}
            Err(msg) => {
                eprintln!("spirv-val: FAILED");
                eprintln!("{}", msg);
                return Err(1);
            }
        }
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
                    eprintln!(
                        "LLVM IR file is still available at: {}",
                        output_path.display()
                    );
                }
                Err(e) => {
                    eprintln!("Failed to invoke clang: {}", e);
                    eprintln!(
                        "LLVM IR file is still available at: {}",
                        output_path.display()
                    );
                }
            }
        } else {
            println!();
            println!("LLVM IR generated at {}", output_path.display());
            if cfg!(windows) {
                println!(
                    "To compile: clang {} -o {}",
                    output_path.display(),
                    exe_path.display()
                );
            } else {
                println!(
                    "To compile: clang {} -o {} -lm",
                    output_path.display(),
                    exe_path.display()
                );
            }
        }
    }

    // x86-64: try nasm → ld pipeline for native executable
    if target == Target::X86_64 {
        let obj_path = input.with_extension("o");
        let exe_path = input.with_extension(if cfg!(windows) { "exe" } else { "" });
        let nasm_ok = std::process::Command::new("nasm")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if nasm_ok {
            let fmt = if cfg!(windows) { "win64" } else { "elf64" };
            if let Ok(r) = std::process::Command::new("nasm")
                .args(["-f", fmt])
                .arg(&output_path)
                .arg("-o")
                .arg(&obj_path)
                .output()
            {
                if r.status.success() {
                    println!("Assembled -> {}", obj_path.display());
                    let lr = if cfg!(windows) {
                        std::process::Command::new("link.exe")
                            .args(["/entry:main", "/subsystem:console"])
                            .arg(&obj_path)
                            .arg(&format!("/out:{}", exe_path.display()))
                            .output()
                    } else {
                        std::process::Command::new("ld")
                            .arg(&obj_path)
                            .arg("-o")
                            .arg(&exe_path)
                            .arg("-lc")
                            .output()
                    };
                    if let Ok(r) = lr {
                        if r.status.success() {
                            println!("Linked -> {}", exe_path.display());
                        }
                    }
                }
            }
        } else {
            println!(
                "\nx86-64 assembly at {}. Install nasm to build native.",
                output_path.display()
            );
        }
    }

    // WASM: detect wasmtime/wasmer and show run instructions
    if target == Target::Wasm {
        let wt = std::process::Command::new("wasmtime")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if wt {
            println!("Run: wasmtime {}", output_path.display());
        } else {
            println!(
                "\nWASM at {}. Install wasmtime to run.",
                output_path.display()
            );
        }
    }

    Ok(())
}

/// Watch shader files for changes and recompile automatically.
///
/// Usage:
///   buildc watch shaders/ --target=spirv
///   buildc watch shader.bld --target=spirv
fn cmd_watch(path: &PathBuf, target_str: &str) -> Result<(), i32> {
    use std::collections::HashMap;
    use std::time::{Duration, SystemTime};

    let target = parse_codegen_target(target_str).map_err(|err| {
        eprintln!("{}", err);
        1
    })?;
    let target_ext = match target {
        Target::SpirV => "spv",
        Target::C => "c",
        Target::LlvmIr => "ll",
        Target::Rust => "rs",
        _ => {
            eprintln!(
                "Watch target '{}' is not supported. Supported: spirv, c, llvm, rust",
                target_str
            );
            return Err(1);
        }
    };

    // Collect .bld files to watch
    let files_to_watch: Vec<PathBuf> = if path.is_dir() {
        std::fs::read_dir(path)
            .map_err(|e| {
                eprintln!("Failed to read directory '{}': {}", path.display(), e);
                1
            })?
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let p = entry.path();
                if p.extension().and_then(|e| e.to_str()) == Some("bld") {
                    Some(p)
                } else {
                    None
                }
            })
            .collect()
    } else if path.extension().and_then(|e| e.to_str()) == Some("bld") {
        vec![path.clone()]
    } else {
        eprintln!("Expected a .bld file or directory");
        return Err(1);
    };

    if files_to_watch.is_empty() {
        eprintln!("No .bld files found in '{}'", path.display());
        return Err(1);
    }

    println!(
        "Watching {} file(s) for changes (target: {})...",
        files_to_watch.len(),
        target_str
    );
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
                            println!(
                                "[OK] {} -> {} ({:.1}ms)",
                                f.file_name().unwrap().to_string_lossy(),
                                output.file_name().unwrap().to_string_lossy(),
                                elapsed.as_secs_f64() * 1000.0
                            );

                            // Auto-validate SPIR-V if spirv-val is available
                            if target_ext == "spv" {
                                let spirv_val_paths =
                                    ["C:\\VulkanSDK\\1.4.341.1\\Bin\\spirv-val.exe", "spirv-val"];
                                for val_path in &spirv_val_paths {
                                    if let Ok(result) = std::process::Command::new(val_path)
                                        .arg("--target-env")
                                        .arg("vulkan1.0")
                                        .arg(&output)
                                        .output()
                                    {
                                        if result.status.success() {
                                            println!("     spirv-val: PASSED (Vulkan 1.0)");
                                        } else {
                                            let stderr = String::from_utf8_lossy(&result.stderr);
                                            eprintln!(
                                                "     spirv-val: FAILED\n     {}",
                                                stderr.trim()
                                            );
                                        }
                                        break;
                                    }
                                }
                            }
                        }
                        Err(msg) => eprintln!(
                            "[ERR] {}: {}",
                            f.file_name().unwrap().to_string_lossy(),
                            msg
                        ),
                    }
                }
            }
        }
    }
}

/// Compile a single .bld file to the given output path.
fn compile_single_file(input: &Path, output: &Path) -> Result<(), String> {
    let source = std::fs::read_to_string(input).map_err(|e| format!("read error: {}", e))?;

    // Resolve `// import <pkg>` and `use <pkg>;` directives
    let source = resolve_imports(&source, input)
        .map_err(|code| format!("import resolution failed (exit {})", code))?;

    let source_file = SourceFile::new(input.to_string_lossy(), source);

    let mut lexer = Lexer::new(&source_file);
    let tokens = lexer
        .tokenize()
        .map_err(|e| format!("lexer error: {}", e))?;

    let mut parser = Parser::new(&source_file, tokens);
    let ast = parser.parse().map_err(|e| format!("parse error: {}", e))?;

    if !parser.errors().is_empty() {
        return Err(format!("parse errors: {}", parser.errors().len()));
    }

    let mut ctx = TypeContext::new();
    let mut checker = TypeChecker::new(&mut ctx);
    checker.set_source_file(&source_file);
    checker.check_module(&ast);

    if checker.has_errors() {
        let errs: Vec<String> = checker.errors().iter().map(|e| format!("{}", e)).collect();
        return Err(format!("type errors:\n  {}", errs.join("\n  ")));
    }

    let target = output
        .extension()
        .and_then(|e| e.to_str())
        .and_then(target_from_extension)
        .unwrap_or(Target::C);

    let mut codegen = CodeGenerator::with_source(&ctx, target, source_file.source().into());
    let generated = codegen
        .generate(&ast)
        .map_err(|e| format!("codegen error: {}", e))?;
    if !codegen.linear_errors().is_empty() {
        let errs: Vec<String> = codegen
            .linear_errors()
            .iter()
            .map(|e| format!("{}", e))
            .collect();
        return Err(format!("linear type errors:\n  {}", errs.join("\n  ")));
    }

    std::fs::write(output, &generated.data).map_err(|e| format!("write error: {}", e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The corpus capability gate must be LIVE: the derivation runs the real
    /// type checker over program source, so a program whose capability surface
    /// changes changes the derived set. Without this, the gate is an
    /// author-supplied stamp that verify merely string-compares, i.e. a
    /// verifier that cannot fail.
    #[test]
    fn corpus_capability_derivation_observes_source_not_manifest() {
        let dir = std::env::temp_dir().join(format!(
            "buildlang_capability_derivation_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("programs")).expect("create mini corpus");

        // One Console program and one program that ALSO touches the Clock
        // capability: the derived union must include both, regardless of what
        // any manifest surface strings claim.
        std::fs::write(
            dir.join("programs/console.bld"),
            "fn main() ~ Console {\n    println(\"{}\", 1);\n}\n",
        )
        .expect("write console program");
        std::fs::write(
            dir.join("programs/clock.bld"),
            "fn main() ~ Console + Clock {\n    let t = clock_ms();\n    println(\"{}\", t);\n}\n",
        )
        .expect("write clock program");

        let manifest = SemanticCorpusManifest {
            schema: "test".to_string(),
            programs: vec![
                SemanticCorpusProgram {
                    id: "console".to_string(),
                    path: "programs/console.bld".to_string(),
                    surfaces: vec!["stdout".to_string()],
                    expected_stdout: "1\n".to_string(),
                },
                SemanticCorpusProgram {
                    id: "clock".to_string(),
                    path: "programs/clock.bld".to_string(),
                    surfaces: vec!["stdout".to_string()],
                    expected_stdout: String::new(),
                },
            ],
        };

        let derived =
            derive_corpus_capabilities(&dir, &manifest).expect("derivation over checking programs");
        let _ = std::fs::remove_dir_all(&dir);

        assert!(
            derived.observed.contains(&"Console".to_string()),
            "derived observed capabilities must include Console: {:?}",
            derived.observed
        );
        assert!(
            derived.observed.contains(&"Clock".to_string()),
            "derived observed capabilities must include Clock (proving the \
             derivation reads SOURCE, not manifest surfaces): {:?}",
            derived.observed
        );
        assert!(
            derived.declared.contains(&"Clock".to_string()),
            "derived declared effects must include Clock: {:?}",
            derived.declared
        );
    }

    #[test]
    fn duplicate_json_keys_are_rejected_at_any_depth() {
        // Top-level duplicate.
        assert!(
            assert_no_duplicate_json_keys(r#"{"a": 1, "a": 2}"#).is_err(),
            "top-level duplicate key must be rejected"
        );
        // Nested duplicate (the seal-forgery shape: two verdict keys where the
        // hasher sees one and a permissive reader the other).
        assert!(
            assert_no_duplicate_json_keys(
                r#"{"receipt": {"receipt_status": "PASS", "receipt_status": "FAIL_UNEXPECTED"}}"#
            )
            .is_err(),
            "nested duplicate key must be rejected"
        );
        // Duplicate inside an array element.
        assert!(
            assert_no_duplicate_json_keys(r#"[{"k": 1}, {"x": 1, "x": 2}]"#).is_err(),
            "duplicate key inside an array element must be rejected"
        );
        // Clean documents pass, including repeated keys in DIFFERENT objects.
        assert!(assert_no_duplicate_json_keys(r#"{"a": {"k": 1}, "b": {"k": 2}}"#).is_ok());
        assert!(assert_no_duplicate_json_keys(r#"[1, "x", null, true, 2.5]"#).is_ok());
    }

    #[test]
    fn nonfinite_json_literals_are_rejected_by_the_parser() {
        // serde_json's parser treats bare NaN/Infinity as invalid JSON; this
        // pins that assumption (the strict loader relies on it).
        assert!(serde_json::from_str::<serde_json::Value>(r#"{"v": NaN}"#).is_err());
        assert!(serde_json::from_str::<serde_json::Value>(r#"{"v": Infinity}"#).is_err());
        assert!(serde_json::from_str::<serde_json::Value>(r#"{"v": -Infinity}"#).is_err());
    }

    #[test]
    fn parses_rust_codegen_target_aliases() {
        assert_eq!(parse_codegen_target("rust"), Ok(Target::Rust));
        assert_eq!(parse_codegen_target("rs"), Ok(Target::Rust));
    }

    #[test]
    fn infers_rust_target_from_rs_extension() {
        assert_eq!(target_from_extension("rs"), Some(Target::Rust));
    }

    #[test]
    fn c_link_libraries_cover_host_runtime_dependencies() {
        assert_eq!(c_link_libraries("windows", false), &["-lws2_32"]);
        assert_eq!(c_link_libraries("windows", true), &["ws2_32.lib"]);
        assert_eq!(c_link_libraries("linux", false), &["-lm"]);
        assert_eq!(c_link_libraries("macos", true), &[] as &[&str]);
    }

    #[test]
    fn user_link_flags_format_per_toolchain() {
        // gcc / clang / cc style.
        assert_eq!(
            user_link_flags(&["sqlite3".to_string(), "z".to_string()], false),
            vec!["-lsqlite3".to_string(), "-lz".to_string()]
        );
        // MSVC style.
        assert_eq!(
            user_link_flags(&["sqlite3".to_string()], true),
            vec!["sqlite3.lib".to_string()]
        );
        // No libraries declared -> no extra flags.
        assert!(user_link_flags(&[], false).is_empty());
    }

    #[test]
    fn source_digest_hex_returns_known_sha256() {
        assert_eq!(
            source_digest_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn language_version_string_matches_public_tuple() {
        assert_eq!(language_version_string(), "1.0.0");
    }

    #[test]
    fn doctor_substrate_rows_report_missing_when_root_is_absent() {
        assert_eq!(
            substrate_evidence_rows(None),
            vec![
                "  receipt   missing  run buildc corpus verify from a repository checkout"
                    .to_string()
            ]
        );
    }

    #[test]
    fn doctor_substrate_rows_report_invalid_when_receipt_is_malformed() {
        let root = std::env::temp_dir().join(format!(
            "buildlang_doctor_substrate_invalid_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("receipts")).expect("create substrate fixture");
        std::fs::write(
            root.join("manifest.json"),
            r#"{
  "schema": "buildlang-semantic-corpus/v1",
  "programs": []
}
"#,
        )
        .expect("write malformed-doctor manifest");
        std::fs::write(
            root.join("receipts")
                .join("substrate-semantic-corpus-2026-06-18.json"),
            r#"{
  "schema": "buildlang-substrate-receipt/v9"
}
"#,
        )
        .expect("write malformed-doctor substrate receipt");

        assert_eq!(
            substrate_evidence_rows(Some(&root)),
            vec!["  receipt   invalid  run buildc corpus verify for details".to_string()]
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn substrate_path_lexical_windows_rejection_is_host_independent() {
        assert!(is_lexically_invalid_substrate_relative_path(
            "\\receipts\\mir-representation-2026-06-18.json"
        ));
        assert!(is_lexically_invalid_substrate_relative_path(
            "\\\\server\\share\\outside.json"
        ));
        assert!(is_lexically_invalid_substrate_relative_path(
            "\\\\?\\C:\\outside.json"
        ));
        assert!(is_lexically_invalid_substrate_relative_path(
            "C:\\outside.json"
        ));
        assert!(is_lexically_invalid_substrate_relative_path(
            "C:outside.json"
        ));
        assert!(!is_lexically_invalid_substrate_relative_path(
            "receipts/mir-representation-2026-06-18.json"
        ));
    }

    #[test]
    fn check_policy_evaluation_sorts_and_deduplicates_violations() {
        let policy = LoadedCheckPolicy {
            source: "policy.json".to_string(),
            source_digest: CheckReceiptSourceDigest {
                algorithm: "sha256",
                hex: source_digest_hex(b"policy"),
            },
            builtin_profile: None,
            builtin_profile_digest: None,
            profile: CheckPolicyProfile {
                schema: "buildlang-check-policy/v1".to_string(),
                allowed_effects: vec!["Console".to_string()],
                denied_effects: vec!["Network".to_string()],
                direct_effect_allowlist: BTreeMap::new(),
                direct_capability_source_allowlist: BTreeMap::new(),
                propagated_effect_allowlist: BTreeMap::new(),
                propagated_effect_source_allowlist: BTreeMap::new(),
                require_source_digest: true,
                require_input_graph_digest: false,
                require_effect_allowlist: false,
                require_provenance_allowlists: false,
                require_source_allowlists: false,
                require_allowlist_coverage: false,
            },
        };
        let outcome = CheckOutcome {
            source: "source.bld".to_string(),
            compiler_version: buildlang::VERSION,
            language_version: language_version_string(),
            source_digest: CheckReceiptSourceDigest {
                algorithm: "sha256",
                hex: source_digest_hex(b"source"),
            },
            input_graph_digest: input_graph_digest(&[]),
            input_digests: Vec::new(),
            items: 1,
            tokens: 1,
            parse_errors: Vec::new(),
            type_errors: Vec::new(),
            type_error_locations: Vec::new(),
            function_summaries: vec![
                FunctionEffectSummary {
                    function: "b".to_string(),
                    declared_effects: vec!["Network".to_string(), "Network".to_string()],
                    observed_capabilities: BTreeMap::new(),
                    propagated_effects: BTreeMap::new(),
                },
                FunctionEffectSummary {
                    function: "a".to_string(),
                    declared_effects: vec!["FileSystem".to_string()],
                    observed_capabilities: BTreeMap::new(),
                    propagated_effects: BTreeMap::new(),
                },
            ],
        };

        let decision = evaluate_check_policy(&policy, &outcome);
        assert_eq!(decision.schema, "buildlang-check-policy/v1");
        assert_eq!(decision.source, "policy.json");
        assert_eq!(decision.source_digest.algorithm, "sha256");
        assert_eq!(check_policy_status(&decision), "failed");
        let keys = decision
            .violations
            .iter()
            .map(|violation| {
                (
                    violation.function.as_str(),
                    violation.effect.as_str(),
                    violation.surface,
                    violation.kind,
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(
            keys,
            vec![
                ("a", "FileSystem", "declared_effects", "DisallowedEffect"),
                ("b", "Network", "declared_effects", "DeniedEffect"),
            ]
        );
    }

    #[test]
    fn check_policy_requires_valid_input_graph_digest() {
        let policy = LoadedCheckPolicy {
            source: "policy.json".to_string(),
            source_digest: CheckReceiptSourceDigest {
                algorithm: "sha256",
                hex: source_digest_hex(b"policy"),
            },
            builtin_profile: None,
            builtin_profile_digest: None,
            profile: CheckPolicyProfile {
                schema: "buildlang-check-policy/v1".to_string(),
                allowed_effects: Vec::new(),
                denied_effects: Vec::new(),
                direct_effect_allowlist: BTreeMap::new(),
                direct_capability_source_allowlist: BTreeMap::new(),
                propagated_effect_allowlist: BTreeMap::new(),
                propagated_effect_source_allowlist: BTreeMap::new(),
                require_source_digest: false,
                require_input_graph_digest: true,
                require_effect_allowlist: false,
                require_provenance_allowlists: false,
                require_source_allowlists: false,
                require_allowlist_coverage: false,
            },
        };
        let outcome = CheckOutcome {
            source: "source.bld".to_string(),
            compiler_version: buildlang::VERSION,
            language_version: language_version_string(),
            source_digest: CheckReceiptSourceDigest {
                algorithm: "sha256",
                hex: source_digest_hex(b"source"),
            },
            input_graph_digest: CheckReceiptSourceDigest {
                algorithm: "sha1",
                hex: "abc".to_string(),
            },
            input_digests: Vec::new(),
            items: 1,
            tokens: 1,
            parse_errors: Vec::new(),
            type_errors: Vec::new(),
            type_error_locations: Vec::new(),
            function_summaries: Vec::new(),
        };

        let decision = evaluate_check_policy(&policy, &outcome);
        assert_eq!(check_policy_status(&decision), "failed");
        assert_eq!(decision.violations.len(), 1);
        assert_eq!(decision.violations[0].kind, "MissingInputGraphDigest");
        assert_eq!(decision.violations[0].surface, "input_graph_digest");
    }

    #[test]
    fn check_policy_loads_profile_and_digest() {
        let path = std::env::temp_dir().join(format!(
            "buildlang_check_policy_load_{}.json",
            std::process::id()
        ));
        std::fs::write(
            &path,
            r#"{
              "schema": "buildlang-check-policy/v1",
              "allowed_effects": ["Console"],
              "unknown_future_field": true
            }"#,
        )
        .expect("write policy load fixture");

        let loaded = load_check_policy(&path).expect("policy should load");
        let _ = std::fs::remove_file(&path);

        assert_eq!(loaded.profile.schema, "buildlang-check-policy/v1");
        assert_eq!(loaded.profile.allowed_effects, vec!["Console"]);
        assert!(loaded.profile.direct_effect_allowlist.is_empty());
        assert!(loaded.profile.direct_capability_source_allowlist.is_empty());
        assert!(loaded.profile.propagated_effect_allowlist.is_empty());
        assert!(loaded.profile.propagated_effect_source_allowlist.is_empty());
        assert!(!loaded.profile.require_input_graph_digest);
        assert!(!loaded.profile.require_effect_allowlist);
        assert!(!loaded.profile.require_provenance_allowlists);
        assert!(!loaded.profile.require_source_allowlists);
        assert!(!loaded.profile.require_allowlist_coverage);
        assert_eq!(loaded.source_digest.algorithm, "sha256");
        assert_eq!(loaded.source_digest.hex.len(), 64);
    }

    #[test]
    fn run_temp_build_dirs_are_unique_for_same_source() {
        let source = PathBuf::from("semantic-corpus/programs/scalar_branch.bld");

        let first = run_temp_build_dir(&source);
        let second = run_temp_build_dir(&source);

        assert_ne!(first, second);
        assert!(first
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .starts_with("buildlang_run_scalar_branch_"));
    }

    #[test]
    fn run_temp_build_dirs_sanitize_source_stems() {
        let source = PathBuf::from("semantic-corpus/programs/weird file!.bld");
        let dir = run_temp_build_dir(&source);
        let name = dir
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();

        assert!(name.starts_with("buildlang_run_weird_file__"));
        assert!(!name.contains(' '));
        assert!(!name.contains('!'));
    }
}
