use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use sha2::{Digest, Sha256};

fn buildc() -> Command {
    Command::new(env!("CARGO_BIN_EXE_buildc"))
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler manifest should have a repository parent")
        .to_path_buf()
}

fn quickstart_example(name: &str) -> PathBuf {
    repo_root().join("examples").join("quickstart").join(name)
}

fn c_backend_ready() -> bool {
    let output = buildc().arg("doctor").output().expect("run buildc doctor");
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.contains("Ready for practical C-backend examples: yes")
}

fn receipt_from_stdout(output: &std::process::Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "stdout should be JSON receipt: {err}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut hex, "{byte:02x}").expect("write digest hex");
    }
    hex
}

fn write_temp_policy(label: &str, json: &str) -> PathBuf {
    let policy = std::env::temp_dir().join(format!(
        "buildlang_check_policy_{}_{}.json",
        label,
        std::process::id()
    ));
    fs::write(&policy, json)
        .unwrap_or_else(|err| panic!("write policy fixture {}: {}", policy.display(), err));
    policy
}

fn input_digest_hex(receipt: &serde_json::Value, role: &str, source_suffix: &str) -> String {
    let records = receipt["input_digests"]
        .as_array()
        .expect("input_digests should be an array");
    let record = records
        .iter()
        .find(|record| {
            record["role"] == role
                && record["source"]
                    .as_str()
                    .is_some_and(|source| source.ends_with(source_suffix))
        })
        .unwrap_or_else(|| {
            panic!("missing input digest role={role:?} suffix={source_suffix:?} in {records:#?}")
        });
    assert_eq!(record["digest"]["algorithm"], "sha256");
    let hex = record["digest"]["hex"]
        .as_str()
        .expect("input digest hex should be a string");
    assert_eq!(hex.len(), 64);
    hex.to_string()
}

fn input_graph_digest_hex(receipt: &serde_json::Value) -> String {
    assert_eq!(receipt["input_graph_digest"]["algorithm"], "sha256");
    let hex = receipt["input_graph_digest"]["hex"]
        .as_str()
        .expect("input graph digest hex should be a string");
    assert_eq!(hex.len(), 64);
    hex.to_string()
}

fn verification_check<'a>(report: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    report["checks"]
        .as_array()
        .expect("verification checks should be an array")
        .iter()
        .find(|check| check["name"] == name)
        .unwrap_or_else(|| panic!("missing verification check {name:?} in {report:#?}"))
}

fn copy_dir_recursive(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap_or_else(|err| {
        panic!(
            "create destination directory {}: {}",
            destination.display(),
            err
        )
    });

    for entry in fs::read_dir(source)
        .unwrap_or_else(|err| panic!("read source directory {}: {}", source.display(), err))
    {
        let entry = entry.expect("read directory entry");
        let entry_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if entry
            .file_type()
            .unwrap_or_else(|err| panic!("read file type for {}: {}", entry_path.display(), err))
            .is_dir()
        {
            copy_dir_recursive(&entry_path, &destination_path);
        } else {
            fs::copy(&entry_path, &destination_path).unwrap_or_else(|err| {
                panic!(
                    "copy {} to {}: {}",
                    entry_path.display(),
                    destination_path.display(),
                    err
                )
            });
        }
    }
}

fn temp_semantic_corpus(label: &str) -> PathBuf {
    let destination = std::env::temp_dir().join(format!(
        "buildlang_semantic_corpus_{}_{}",
        label,
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&destination);
    copy_dir_recursive(&repo_root().join("semantic-corpus"), &destination);
    destination
}

fn write_substrate_receipt_copy(
    corpus_root: &Path,
    transform: impl FnOnce(serde_json::Value) -> serde_json::Value,
) {
    let receipt_path = corpus_root
        .join("receipts")
        .join("substrate-semantic-corpus-2026-06-18.json");
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipt_path).expect("read substrate receipt"))
            .expect("parse substrate receipt");
    let receipt = transform(receipt);
    let rendered =
        serde_json::to_string_pretty(&receipt).expect("render modified substrate receipt");
    fs::write(&receipt_path, format!("{rendered}\n")).expect("write modified substrate receipt");
}

fn write_mir_representation_receipt_copy(
    corpus_root: &Path,
    transform: impl FnOnce(serde_json::Value) -> serde_json::Value,
) {
    let receipt_path = corpus_root
        .join("receipts")
        .join("mir-representation-2026-06-18.json");
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipt_path).expect("read MIR representation receipt"))
            .expect("parse MIR representation receipt");
    let receipt = transform(receipt);
    let rendered =
        serde_json::to_string_pretty(&receipt).expect("render modified MIR representation receipt");
    fs::write(&receipt_path, format!("{rendered}\n"))
        .expect("write modified MIR representation receipt");
}

fn write_memory_layout_receipt_copy(
    corpus_root: &Path,
    transform: impl FnOnce(serde_json::Value) -> serde_json::Value,
) {
    let receipt_path = corpus_root
        .join("receipts")
        .join("memory-layout-2026-06-18.json");
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipt_path).expect("read memory layout receipt"))
            .expect("parse memory layout receipt");
    let receipt = transform(receipt);
    let rendered =
        serde_json::to_string_pretty(&receipt).expect("render modified memory layout receipt");
    fs::write(&receipt_path, format!("{rendered}\n"))
        .expect("write modified memory layout receipt");
}

fn write_symbol_graph_receipt_copy(
    corpus_root: &Path,
    transform: impl FnOnce(serde_json::Value) -> serde_json::Value,
) {
    let receipt_path = corpus_root
        .join("receipts")
        .join("symbol-graph-2026-06-18.json");
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipt_path).expect("read symbol graph receipt"))
            .expect("parse symbol graph receipt");
    let rendered = serde_json::to_string_pretty(&transform(receipt))
        .expect("render modified symbol graph receipt");
    fs::write(&receipt_path, format!("{rendered}\n")).expect("write modified symbol graph receipt");
}

fn write_module_graph_receipt_copy(
    corpus_root: &Path,
    transform: impl FnOnce(serde_json::Value) -> serde_json::Value,
) {
    let receipt_path = corpus_root
        .join("receipts")
        .join("module-graph-2026-06-18.json");
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipt_path).expect("read module graph receipt"))
            .expect("parse module graph receipt");
    let rendered = serde_json::to_string_pretty(&transform(receipt))
        .expect("render modified module graph receipt");
    fs::write(&receipt_path, format!("{rendered}\n")).expect("write modified module graph receipt");
}

fn write_lsp_dispatch_receipt_copy(
    corpus_root: &Path,
    transform: impl FnOnce(serde_json::Value) -> serde_json::Value,
) {
    let receipt_path = corpus_root
        .join("receipts")
        .join("lsp-dispatch-2026-06-18.json");
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipt_path).expect("read LSP dispatch receipt"))
            .expect("parse LSP dispatch receipt");
    let rendered = serde_json::to_string_pretty(&transform(receipt))
        .expect("render modified LSP dispatch receipt");
    fs::write(&receipt_path, format!("{rendered}\n")).expect("write modified LSP receipt");
}

fn assert_corpus_verify_rejects(corpus_root: &Path, expected_stderr: &str) {
    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(corpus_root)
        .output()
        .expect("run buildc corpus verify against symbol graph fixture");
    let _ = fs::remove_dir_all(corpus_root);

    assert!(!output.status.success(), "fixture should fail");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(expected_stderr),
        "stderr should contain {expected_stderr:?}:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn help_lists_doctor_command() {
    let output = buildc().arg("--help").output().expect("run buildc --help");

    assert!(
        output.status.success(),
        "buildc --help should exit successfully"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("doctor"),
        "help should list doctor command:\n{}",
        stdout
    );
}

#[test]
fn doctor_reports_adoption_readiness_summary() {
    let output = buildc().arg("doctor").output().expect("run buildc doctor");

    assert!(
        output.status.success(),
        "buildc doctor should exit successfully; stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "buildc doctor should report diagnostics on stdout only:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    for expected in [
        "BuildLang Doctor",
        "buildc:",
        "C backend:",
        "stdlib:",
        "registry:",
        "Backend maturity:",
        "Substrate evidence:",
        "receipt   ok",
        "buildlang-substrate-receipt/v0",
        "corpus    ok",
        "8 semantic program(s)",
        "c         anchor",
        "rust      subset",
        "spirv     unverified",
        "memory    partial",
        "6 verified surface(s), 3 known gap(s)",
        "repr      MIR",
        "c        primary",
        "rust     experimental",
    ] {
        assert!(
            stdout.contains(expected),
            "doctor output should contain {expected:?}:\n{}",
            stdout
        );
    }
}

#[test]
fn help_lists_corpus_command() {
    let output = buildc().arg("--help").output().expect("run buildc --help");

    assert!(
        output.status.success(),
        "buildc --help should exit successfully"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("corpus"),
        "help should list corpus command:\n{}",
        stdout
    );
}

#[test]
fn check_reports_capability_effect_for_ambient_file_call() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_capability_gate_{}.bld",
        std::process::id()
    ));
    fs::write(&fixture, r#"fn main() { read_file("ops.txt"); }"#)
        .expect("write capability fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run buildc check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "ambient file call should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("read_file"),
        "diagnostic should name triggering ambient call:\n{}",
        stderr
    );
}

#[test]
fn check_reports_capability_effect_for_qualified_ambient_call() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_qualified_capability_gate_{}.bld",
        std::process::id()
    ));
    fs::write(&fixture, r#"fn main() { io::read_file("ops.txt"); }"#)
        .expect("write qualified capability fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run buildc check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "qualified ambient file call should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("io::read_file"),
        "diagnostic should name qualified ambient call:\n{}",
        stderr
    );
}

#[test]
fn check_reports_capability_effect_for_gpu_runtime_call() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_gpu_capability_gate_{}.bld",
        std::process::id()
    ));
    fs::write(&fixture, r#"fn main() { build_vk_init(); }"#).expect("write gpu capability fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run buildc check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "GPU runtime call should fail without Gpu effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Gpu"),
        "diagnostic should name Gpu effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("build_vk_init"),
        "diagnostic should name triggering GPU helper:\n{}",
        stderr
    );
}

/// Phase 4a effect-gating negative: `workgroupBarrier()` carries the `Gpu`
/// effect (registered in `types/capabilities.rs`), so calling it inside a `fn`
/// that does NOT declare `~Gpu` MUST be rejected by the effect checker, with a
/// diagnostic that names both the `Gpu` effect and the triggering
/// `workgroupBarrier` intrinsic. This is the specific gating the Phase 4a work
/// claimed but had never committed as a test.
#[test]
fn check_rejects_workgroup_barrier_without_gpu_effect() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_workgroup_barrier_gate_{}.bld",
        std::process::id()
    ));
    // No `~Gpu` on `main`, yet it performs the Gpu-effecting workgroupBarrier().
    fs::write(&fixture, r#"fn main() { workgroupBarrier(); }"#)
        .expect("write workgroup barrier fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run buildc check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "workgroupBarrier() should fail the effect check without a ~Gpu declaration"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Gpu"),
        "diagnostic should name the Gpu effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("workgroupBarrier"),
        "diagnostic should name the triggering workgroupBarrier intrinsic:\n{}",
        stderr
    );
}

/// Companion negative for the other Phase 4a intrinsic: `workgroupArray(N)` is
/// also `Gpu`-effecting, so binding it in a non-`~Gpu` function must be rejected
/// naming the `Gpu` effect and `workgroupArray`.
#[test]
fn check_rejects_workgroup_array_without_gpu_effect() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_workgroup_array_gate_{}.bld",
        std::process::id()
    ));
    fs::write(&fixture, r#"fn main() { let s = workgroupArray(64); }"#)
        .expect("write workgroup array fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run buildc check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "workgroupArray() should fail the effect check without a ~Gpu declaration"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Gpu"),
        "diagnostic should name the Gpu effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("workgroupArray"),
        "diagnostic should name the triggering workgroupArray intrinsic:\n{}",
        stderr
    );
}

#[test]
fn check_receipt_stdout_records_passing_capabilities() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_pass_{}.bld",
        std::process::id()
    ));
    fs::write(&fixture, r#"fn main() ~ Console { println!("ok"); }"#)
        .expect("write passing receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "passing receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be JSON receipt");
    assert_eq!(receipt["schema"], "buildlang-check-receipt/v1");
    assert_eq!(receipt["status"], "passed");
    assert_eq!(receipt["compiler"], "buildc");
    assert_eq!(receipt["compiler_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(receipt["language_version"], "1.0.0");
    assert_eq!(receipt["source_digest"]["algorithm"], "sha256");
    let digest = receipt["source_digest"]["hex"]
        .as_str()
        .expect("source digest hex string");
    assert_eq!(digest.len(), 64);
    assert!(
        digest
            .chars()
            .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase()),
        "digest should be lowercase hex: {digest}"
    );
    assert_eq!(
        receipt["declared_effects"]["main"],
        serde_json::json!(["Console"])
    );
    assert_eq!(
        receipt["observed_capabilities"]["main"]["Console"],
        serde_json::json!(["println!"])
    );
    assert!(receipt["diagnostics"].as_array().unwrap().is_empty());
}

#[test]
fn check_reports_capability_effect_for_include_str_macro() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_include_str_capability_gate_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"fn main() { let embedded = include_str!("ops.txt"); }"#,
    )
    .expect("write include_str macro capability fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run buildc check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "include_str! should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("include_str!"),
        "diagnostic should name triggering include_str macro:\n{}",
        stderr
    );
}

#[test]
fn check_reports_capability_effect_for_ambient_call_inside_macro_argument() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_macro_arg_capability_gate_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"fn main() ~ Console { println!(read_file("ops.txt")); }"#,
    )
    .expect("write macro argument capability fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run buildc check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "macro argument ambient file call should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("read_file"),
        "diagnostic should name triggering macro argument ambient call:\n{}",
        stderr
    );
}

#[test]
fn check_reports_capability_effect_for_macro_argument_call_in_external_module() {
    let dir = std::env::temp_dir().join(format!(
        "buildlang_module_macro_arg_capability_gate_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create module macro argument fixture dir");
    let entry = dir.join("main.bld");
    let module = dir.join("ops.bld");

    fs::write(
        &entry,
        r#"mod ops;
fn main() {}
"#,
    )
    .expect("write module macro argument entry fixture");
    fs::write(
        &module,
        r#"fn leak() ~ Console {
    println!(read_file("ops.txt"));
}
"#,
    )
    .expect("write module macro argument module fixture");

    let output = buildc()
        .arg("check")
        .arg(&entry)
        .output()
        .expect("run buildc check");

    let _ = fs::remove_dir_all(&dir);

    assert!(
        !output.status.success(),
        "external module macro argument ambient file call should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("read_file"),
        "diagnostic should name triggering module macro argument ambient call:\n{}",
        stderr
    );
}

#[test]
fn check_receipt_records_env_macro_capability_source() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_env_macro_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"fn main() ~ Environment { let token_name = env!("TOKEN"); }"#,
    )
    .expect("write env macro receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "env macro receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["declared_effects"]["main"],
        serde_json::json!(["Environment"])
    );
    assert_eq!(
        receipt["observed_capabilities"]["main"]["Environment"],
        serde_json::json!(["env!"])
    );
}

#[test]
fn check_receipt_records_macro_argument_capability_source() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_macro_arg_capability_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"fn main() ~ Console + FileSystem { println!(read_file("ops.txt")); }"#,
    )
    .expect("write macro argument capability receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "macro argument capability receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["declared_effects"]["main"],
        serde_json::json!(["Console", "FileSystem"])
    );
    assert_eq!(
        receipt["observed_capabilities"]["main"]["Console"],
        serde_json::json!(["println!"])
    );
    assert_eq!(
        receipt["observed_capabilities"]["main"]["FileSystem"],
        serde_json::json!(["read_file"])
    );
}

#[test]
fn check_receipt_records_macro_argument_capability_source_in_external_module() {
    let dir = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_module_macro_arg_capability_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create module macro argument receipt fixture dir");
    let entry = dir.join("main.bld");
    let module = dir.join("ops.bld");

    fs::write(
        &entry,
        r#"mod ops;
fn main() {}
"#,
    )
    .expect("write module macro argument receipt entry fixture");
    fs::write(
        &module,
        r#"fn leak() ~ Console + FileSystem {
    println!(read_file("ops.txt"));
}
"#,
    )
    .expect("write module macro argument receipt module fixture");

    let output = buildc()
        .arg("check")
        .arg(&entry)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_dir_all(&dir);

    assert!(
        output.status.success(),
        "external module macro argument capability receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["declared_effects"]["ops_leak"],
        serde_json::json!(["Console", "FileSystem"])
    );
    assert_eq!(
        receipt["observed_capabilities"]["ops_leak"]["Console"],
        serde_json::json!(["println!"])
    );
    assert_eq!(
        receipt["observed_capabilities"]["ops_leak"]["FileSystem"],
        serde_json::json!(["read_file"])
    );
}

#[test]
fn check_receipt_records_gpu_capability_source() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_gpu_{}.bld",
        std::process::id()
    ));
    fs::write(&fixture, r#"fn main() ~ Gpu { build_vk_init(); }"#)
        .expect("write gpu receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "GPU receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["declared_effects"]["main"],
        serde_json::json!(["Gpu"])
    );
    assert_eq!(
        receipt["observed_capabilities"]["main"]["Gpu"],
        serde_json::json!(["build_vk_init"])
    );
}

#[test]
fn check_receipt_records_graphics_runtime_capability_source() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_graphics_runtime_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
extern "C" {
    fn build_gfx_init(width: i32, height: i32, title: &str) -> i32;
}

fn main() ~ Gpu {
    build_gfx_init(800, 600, "BuildLang Triangle");
}
"#,
    )
    .expect("write graphics runtime receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "graphics runtime receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["declared_effects"]["main"],
        serde_json::json!(["Gpu"])
    );
    assert_eq!(
        receipt["observed_capabilities"]["main"]["Gpu"],
        serde_json::json!(["build_gfx_init"])
    );
}

#[test]
fn check_receipt_records_qualified_capability_source() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_qualified_capability_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"fn main() ~ FileSystem { io::read_file("ops.txt"); }"#,
    )
    .expect("write qualified capability receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "qualified capability receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]["FileSystem"],
        serde_json::json!(["io::read_file"])
    );
}

#[test]
fn check_receipt_records_foreign_call_as_direct_capability_source() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_foreign_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
extern "C" { fn touch(); }

fn main() ~ Foreign {
    touch();
}
"#,
    )
    .expect("write foreign receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "foreign receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["declared_effects"]["main"],
        serde_json::json!(["Foreign"])
    );
    assert_eq!(
        receipt["observed_capabilities"]["main"]["Foreign"],
        serde_json::json!(["touch"])
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]
            .as_object()
            .expect("main propagated effects")
            .len(),
        0
    );
}

#[test]
fn check_receipt_records_foreign_static_as_direct_capability_source() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_foreign_static_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
extern "C" { static BUILD_ERRNO: i32; }

fn main() ~ Foreign {
    let code = BUILD_ERRNO;
}
"#,
    )
    .expect("write foreign static receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "foreign static receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["declared_effects"]["main"],
        serde_json::json!(["Foreign"])
    );
    assert_eq!(
        receipt["observed_capabilities"]["main"]["Foreign"],
        serde_json::json!(["BUILD_ERRNO"])
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]
            .as_object()
            .expect("main propagated effects")
            .len(),
        0
    );
}

#[test]
fn check_reports_foreign_call_inside_macro_argument() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_macro_arg_foreign_call_gate_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
extern "C" { fn touch(); }

fn main() ~ Console {
    println!(touch());
}
"#,
    )
    .expect("write macro argument foreign call fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run buildc check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "macro argument foreign call should fail without Foreign effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Foreign"),
        "diagnostic should name Foreign effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("touch"),
        "diagnostic should name triggering foreign call:\n{}",
        stderr
    );
}

#[test]
fn check_receipt_records_foreign_static_inside_macro_argument() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_macro_arg_foreign_static_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
extern "C" { static BUILD_ERRNO: i32; }

fn main() ~ Console + Foreign {
    println!("{}", BUILD_ERRNO);
}
"#,
    )
    .expect("write macro argument foreign static receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "macro argument foreign static receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["declared_effects"]["main"],
        serde_json::json!(["Console", "Foreign"])
    );
    assert_eq!(
        receipt["observed_capabilities"]["main"]["Console"],
        serde_json::json!(["println!"])
    );
    assert_eq!(
        receipt["observed_capabilities"]["main"]["Foreign"],
        serde_json::json!(["BUILD_ERRNO"])
    );
}

#[test]
fn check_receipt_records_propagated_effects_separately() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_propagated_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config() ~ FileSystem {
    read_file("ops.txt");
}

fn main() ~ FileSystem {
    load_config();
}
"#,
    )
    .expect("write propagated receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["load_config"]["FileSystem"],
        serde_json::json!(["read_file"])
    );
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["load_config"]
            .as_object()
            .expect("load_config propagated effects")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config"])
    );
}

#[test]
fn check_reports_effect_for_effectful_method_call() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_effectful_method_gate_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Config;

impl Config {
    fn load(self) ~ FileSystem {
        read_file("ops.txt");
    }
}

fn main() {
    let config = Config;
    config.load();
}
"#,
    )
    .expect("write effectful method fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run buildc check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "effectful method call should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("load"),
        "diagnostic should name triggering method:\n{}",
        stderr
    );
}

#[test]
fn check_receipt_records_effectful_method_call_source() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_effectful_method_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Config;

impl Config {
    fn load(self) ~ FileSystem {
        read_file("ops.txt");
    }
}

fn main() ~ FileSystem {
    let config = Config;
    config.load();
}
"#,
    )
    .expect("write effectful method receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "effectful method receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["load"]["FileSystem"],
        serde_json::json!(["read_file"])
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["Config.load"])
    );
}

#[test]
fn check_reports_effect_for_effectful_associated_function_call() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_effectful_associated_function_gate_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Config;

impl Config {
    fn load() ~ FileSystem {
        read_file("ops.txt");
    }
}

fn main() {
    Config::load();
}
"#,
    )
    .expect("write effectful associated function fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run buildc check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "effectful associated function call should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("Config::load"),
        "diagnostic should name triggering associated function:\n{}",
        stderr
    );
}

#[test]
fn check_receipt_records_effectful_associated_function_call_source() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_effectful_associated_function_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Config;

impl Config {
    fn load() ~ FileSystem {
        read_file("ops.txt");
    }
}

fn main() ~ FileSystem {
    Config::load();
}
"#,
    )
    .expect("write effectful associated function receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "effectful associated function receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["load"]["FileSystem"],
        serde_json::json!(["read_file"])
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["Config::load"])
    );
}

#[test]
fn check_reports_effect_for_effectful_trait_object_method_call() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_effectful_trait_object_method_gate_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
trait Loader {
    fn load(self) ~ FileSystem;
}

fn run(loader: dyn Loader) {
    loader.load();
}
"#,
    )
    .expect("write effectful trait object method fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run buildc check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "effectful dyn trait method call should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("Loader.load"),
        "diagnostic should name triggering trait object method:\n{}",
        stderr
    );
}

#[test]
fn check_reports_effect_for_effectful_callback_parameter() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_effectful_callback_gate_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn run(loader: fn() with FileSystem) {
    loader();
}
"#,
    )
    .expect("write effectful callback fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run buildc check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "effectful callback should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("loader"),
        "diagnostic should name callback source:\n{}",
        stderr
    );
}

#[test]
fn check_receipt_records_effectful_callback_parameter_source() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_effectful_callback_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn run(loader: fn() with FileSystem) ~ FileSystem {
    loader();
}
"#,
    )
    .expect("write effectful callback receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "effectful callback receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["run"]
            .as_object()
            .expect("run observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["run"]["FileSystem"],
        serde_json::json!(["loader"])
    );
}

#[test]
fn check_receipt_records_effectful_returning_callback_source() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_effectful_returning_callback_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn run(loader: (fn() -> str) with FileSystem) -> str ~ FileSystem {
    loader()
}
"#,
    )
    .expect("write returning effectful callback receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "returning effectful callback receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["propagated_effects"]["run"]["FileSystem"],
        serde_json::json!(["loader"])
    );
}

#[test]
fn check_receipt_records_effectful_callback_argument_source() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_effectful_callback_arg_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn run(loader: fn() with FileSystem) ~ FileSystem {
    loader();
}

fn load_config() ~ FileSystem {
    read_file("config.toml");
}

fn main() ~ FileSystem {
    run(load_config);
}
"#,
    )
    .expect("write effectful callback argument receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "effectful callback argument receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["run"]["FileSystem"],
        serde_json::json!(["loader"])
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "run"])
    );
}

#[test]
fn check_reports_effect_for_effectful_callback_argument_source() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_effectful_callback_arg_gate_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn run(loader: fn() with FileSystem) ~ FileSystem {
    loader();
}

fn load_config() ~ FileSystem {
    read_file("config.toml");
}

fn main() {
    run(load_config);
}
"#,
    )
    .expect("write effectful callback argument gate fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run buildc check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "effectful callback argument should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("run"),
        "diagnostic should name wrapper source:\n{}",
        stderr
    );
    assert!(
        stderr.contains("load_config"),
        "diagnostic should name callback argument source:\n{}",
        stderr
    );
}

#[test]
fn check_rejects_effectful_callback_erasure_into_pure_signature() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_effectful_callback_erasure_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn run(loader: fn(str) -> str) {
    loader("ops.txt");
}

fn main() {
    run(read_file);
}
"#,
    )
    .expect("write effectful callback erasure fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run buildc check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "effectful callback should not be accepted by a pure callback signature"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name erased FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("fn(str) -> str"),
        "diagnostic should show the pure callback boundary:\n{}",
        stderr
    );
}

#[test]
fn check_reports_effect_for_ambient_function_alias_call() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_ambient_alias_gate_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn main() {
    let loader = read_file;
    loader("ops.txt");
}
"#,
    )
    .expect("write ambient alias fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run buildc check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "ambient helper alias call should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("loader"),
        "diagnostic should name alias call source:\n{}",
        stderr
    );
}

#[test]
fn check_receipt_records_ambient_function_alias_as_propagated_source() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_ambient_alias_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn main() ~ FileSystem {
    let loader = read_file;
    loader("ops.txt");
}
"#,
    )
    .expect("write ambient alias receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "ambient alias receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["loader"])
    );
}

#[test]
fn check_receipt_clears_stale_sources_for_shadowed_ambient_alias() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_shadowed_ambient_alias_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn main() ~ FileSystem {
    let loader = load_config;
    let loader = read_file;
    loader("ops.txt");
}
"#,
    )
    .expect("write shadowed ambient alias receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "shadowed ambient alias receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["loader"])
    );
}

#[test]
fn check_receipt_rebinds_assigned_effectful_function_alias_source() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_assigned_effectful_alias_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config() ~ FileSystem {
    read_file("config.toml");
}

fn load_secret() ~ FileSystem {
    read_file("secret.toml");
}

fn main() ~ FileSystem {
    let mut loader = load_config;
    loader = load_secret;
    loader();
}
"#,
    )
    .expect("write assigned effectful alias receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "assigned effectful alias receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_secret", "loader"])
    );
}

#[test]
fn check_receipt_rebinds_assigned_effectful_struct_field_source() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_assigned_effectful_field_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let mut ops = Ops { loader: load_config };
    ops.loader = load_secret;
    (ops.loader)("ops.txt");
}
"#,
    )
    .expect("write assigned effectful field receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "assigned effectful field receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_secret", "ops.loader"])
    );
}

#[test]
fn check_receipt_rebinds_assigned_effectful_struct_object_source() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_assigned_effectful_struct_object_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let mut ops = Ops { loader: load_config };
    let defaults = Ops { loader: load_secret };
    ops = defaults;
    (ops.loader)("ops.txt");
}
"#,
    )
    .expect("write assigned effectful struct object receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "assigned effectful struct object receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_secret", "ops.loader"])
    );
}

#[test]
fn check_receipt_rebinds_assigned_effectful_tuple_field_source() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_assigned_effectful_tuple_field_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let mut loaders = (load_config,);
    loaders.0 = load_secret;
    (loaders.0)("ops.txt");
}
"#,
    )
    .expect("write assigned effectful tuple field receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "assigned effectful tuple field receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_secret", "loaders.0"])
    );
}

#[test]
fn check_receipt_rebinds_assigned_effectful_index_source() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_assigned_effectful_index_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let mut loaders = [load_config];
    loaders[0] = load_secret;
    (loaders[0])("ops.txt");
}
"#,
    )
    .expect("write assigned effectful index receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "assigned effectful index receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_secret", "loaders[0]"])
    );
}

#[test]
fn check_receipt_records_repeated_effectful_index_source_origin() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_repeated_effectful_index_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let loaders = [load_config; 2];
    (loaders[1])("ops.txt");
}
"#,
    )
    .expect("write repeated effectful index receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "repeated indexed effect receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "loaders[1]"])
    );
}

#[test]
fn check_receipt_clears_stale_sources_for_assigned_ambient_alias() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_assigned_ambient_alias_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let mut loader = load_config;
    loader = read_file;
    loader("ops.txt");
}
"#,
    )
    .expect("write assigned ambient alias receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "assigned ambient alias receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["loader"])
    );
}

#[test]
fn check_effectful_closure_literal_is_pure_until_called() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_effectful_closure_literal_pure_until_called_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn main() {
    let loader = |path: str| read_file(path);
}
"#,
    )
    .expect("write effectful closure literal fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run buildc check");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "defining an effectful closure should not perform its effect\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_effectful_tuple_struct_constructor_is_pure_until_called() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_effectful_tuple_struct_constructor_pure_until_called_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Slot((fn() -> str) with FileSystem);

fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn main() {
    let slot = Slot(load_config);
}
"#,
    )
    .expect("write effectful tuple struct constructor fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "storing an effectful callback in a tuple struct should stay pure until call\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]
            .as_object()
            .expect("main propagated effects")
            .len(),
        0
    );
}

#[test]
fn check_emits_distinct_argv_slots_for_multi_arg_effect() {
    // Regression: a multi-parameter effect operation must marshal every
    // argument, and each handler parameter must read its OWN argument. The
    // earlier lowering dropped arguments 2..N at the perform site and made
    // every handler parameter alias the first argument, so `Coord.point(10,
    // 20, 30)` handled as `(x, y, z)` silently produced `x=10 y=10 z=10`.
    // The perform site now builds a `void*[N]` argv array and each parameter
    // reads slot `i` via `((void**)handler_data)[i]`; assert the emitted C
    // reads three DISTINCT slots rather than the same one three times.
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_multi_arg_effect_marshal_{}.bld",
        std::process::id()
    ));
    let out_c = std::env::temp_dir().join(format!(
        "buildlang_multi_arg_effect_marshal_{}.c",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
effect Coord {
    fn point(x: i32, y: i32, z: i32) -> (),
}

fn emit() ~ Coord {
    perform Coord.point(10, 20, 30);
}

fn main() ~ Console {
    handle {
        emit()
    } with {
        Coord.point(x, y, z) => {
            println!("x={} y={} z={}", x, y, z);
        },
    }
}
"#,
    )
    .expect("write multi-arg effect fixture");

    let output = buildc()
        .arg(&fixture)
        .arg("-o")
        .arg(&out_c)
        .output()
        .expect("run buildc to emit C");

    assert!(
        output.status.success(),
        "multi-arg effect program should compile to C\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let code = fs::read_to_string(&out_c).expect("read emitted C");
    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&out_c);

    for idx in 0..3 {
        let needle = format!("handler_data)[{}]", idx);
        assert!(
            code.contains(&needle),
            "handler parameter {} should read its own argv slot `{}`; \
             emitted C:\n{}",
            idx,
            needle,
            code
        );
    }
}

#[test]
fn check_vec_string_literal_uses_str_push_helper() {
    // Regression: `vec!["a", "b"]` must build a str-typed handle. The macro
    // lowering picked the vec helper from a type -> name table that had only
    // f64/i64/i32 arms, so string elements fell to the i32 default and the
    // emitted C called `build_hvec_push_i32(handle, <BuildString>)` -- passing a
    // BuildString where an int32_t is expected, which does not compile. Assert
    // the emitted C pushes through the str helper and never through the i32 one.
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_vec_str_literal_{}.bld",
        std::process::id()
    ));
    let out_c = std::env::temp_dir().join(format!(
        "buildlang_vec_str_literal_{}.c",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn main() ~ Console {
    let words = vec!["alpha", "beta", "gamma"];
    println!("count={}", words.len());
    println!("a={}", words.get(0));
}
"#,
    )
    .expect("write vec string-literal fixture");

    let output = buildc()
        .arg(&fixture)
        .arg("-o")
        .arg(&out_c)
        .output()
        .expect("run buildc to emit C");

    assert!(
        output.status.success(),
        "vec string-literal program should compile to C\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let code = fs::read_to_string(&out_c).expect("read emitted C");
    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&out_c);

    // The runtime prelude DEFINES every helper, so a bare substring check cannot
    // tell a call from a definition; match the call form `name(_` (temporary
    // arguments), which the definitions `name(BuildVecHandle ..)` never produce.
    assert!(
        code.contains("build_hvec_push_str(_"),
        "vec![\"..\"] should push string elements through the str helper; \
         emitted C:\n{}",
        code
    );
    assert!(
        !code.contains("build_hvec_push_i32(_"),
        "vec![\"..\"] must not push string elements through the i32 helper; \
         emitted C:\n{}",
        code
    );
}

#[test]
fn check_reports_effect_for_effectful_closure_alias_call() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_effectful_closure_alias_gate_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn main() {
    let loader = |path: str| read_file(path);
    loader("ops.txt");
}
"#,
    )
    .expect("write effectful closure alias fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run buildc check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "calling an effectful closure should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("loader"),
        "diagnostic should name closure alias source:\n{}",
        stderr
    );
}

#[test]
fn check_receipt_records_effectful_closure_alias_as_propagated_source() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_effectful_closure_alias_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn main() ~ FileSystem {
    let loader = |path: str| read_file(path);
    loader("ops.txt");
}
"#,
    )
    .expect("write effectful closure alias receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "effectful closure alias receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["loader"])
    );
}

#[test]
fn check_reports_effect_for_immediate_effectful_closure_call() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_immediate_effectful_closure_gate_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn main() {
    (|path: str| read_file(path))("ops.txt");
}
"#,
    )
    .expect("write immediate effectful closure fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run buildc check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "immediate effectful closure call should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("<closure>"),
        "diagnostic should name anonymous closure source:\n{}",
        stderr
    );
}

#[test]
fn check_receipt_records_immediate_effectful_closure_call_source() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_immediate_effectful_closure_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn main() ~ FileSystem {
    (|path: str| read_file(path))("ops.txt");
}
"#,
    )
    .expect("write immediate effectful closure receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "immediate effectful closure receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["<closure>"])
    );
}

#[test]
fn check_effectful_async_block_is_pure_until_awaited() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_effectful_async_block_pure_until_awaited_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn main() {
    let task = async { read_file("ops.txt") };
}
"#,
    )
    .expect("write effectful async block fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run buildc check");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "constructing an effectful async block should not perform its effect\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_reports_effect_for_awaited_effectful_async_block() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_awaited_effectful_async_block_gate_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn main() {
    let task = async { read_file("ops.txt") };
    task.await;
}
"#,
    )
    .expect("write awaited effectful async block fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run buildc check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "awaited effectful async block should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("task"),
        "diagnostic should name awaited async source:\n{}",
        stderr
    );
}

#[test]
fn check_receipt_records_awaited_async_block_source() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_awaited_async_block_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn main() ~ FileSystem {
    let task = async { read_file("ops.txt") };
    task.await;
}
"#,
    )
    .expect("write awaited async block receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "awaited async block receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["task", "task <- read_file"])
    );
}

#[test]
fn check_rejects_await_operator_on_selected_effectful_function_value() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_await_selected_effectful_function_gate_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn load_secret() -> str ~ FileSystem {
    read_file("secret.toml")
}

fn main() {
    let use_secret = true;
    let loader = if use_secret { load_secret } else { load_config };
    loader.await;
}
"#,
    )
    .expect("write await selected effectful function fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run buildc check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "await operator on selected effectful function value should be rejected"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("await"),
        "diagnostic should name the await operator:\n{}",
        stderr
    );
    assert!(
        stderr.contains("fn()"),
        "diagnostic should name the invalid function operand:\n{}",
        stderr
    );
}

#[test]
fn check_selected_async_block_is_pure_until_awaited() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_selected_async_block_pure_until_awaited_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn main() {
    let use_secret = true;
    let task = if use_secret {
        async { read_file("ops.txt") }
    } else {
        async { getenv("TOKEN") }
    };
}
"#,
    )
    .expect("write selected async block fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run buildc check");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "selecting an effectful async block should not perform its effect\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_reports_effects_for_awaited_selected_async_block() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_awaited_selected_async_block_gate_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn main() {
    let use_secret = true;
    let task = if use_secret {
        async { read_file("ops.txt") }
    } else {
        async { getenv("TOKEN") }
    };
    task.await;
}
"#,
    )
    .expect("write awaited selected async block fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run buildc check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "awaited selected async block should fail without declared effects"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("Environment"),
        "diagnostic should name Environment effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("task"),
        "diagnostic should name awaited selected async source:\n{}",
        stderr
    );
    assert!(
        stderr.contains("read_file"),
        "diagnostic should name selected async FileSystem origin:\n{}",
        stderr
    );
    assert!(
        stderr.contains("getenv"),
        "diagnostic should name selected async Environment origin:\n{}",
        stderr
    );
}

#[test]
fn check_receipt_records_awaited_selected_async_block_sources() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_awaited_selected_async_block_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn main() ~ FileSystem + Environment {
    let use_secret = true;
    let task = if use_secret {
        async { read_file("ops.txt") }
    } else {
        async { getenv("TOKEN") }
    };
    task.await;
}
"#,
    )
    .expect("write awaited selected async block receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "awaited selected async block receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["task", "task <- read_file"])
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["Environment"],
        serde_json::json!(["task", "task <- getenv"])
    );
}

#[test]
fn check_reports_effects_for_awaited_match_selected_async_block() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_awaited_match_selected_async_block_gate_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn main() {
    let mode = 1;
    let task = match mode {
        0 => async { read_file("ops.txt") },
        _ => async { getenv("TOKEN") },
    };
    task.await;
}
"#,
    )
    .expect("write awaited match selected async block fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run buildc check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "awaited match selected async block should fail without declared effects"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("Environment"),
        "diagnostic should name Environment effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("task"),
        "diagnostic should name awaited match selected async source:\n{}",
        stderr
    );
    assert!(
        stderr.contains("read_file"),
        "diagnostic should name match-selected async FileSystem origin:\n{}",
        stderr
    );
    assert!(
        stderr.contains("getenv"),
        "diagnostic should name match-selected async Environment origin:\n{}",
        stderr
    );
}

#[test]
fn check_receipt_records_awaited_match_selected_async_block_sources() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_awaited_match_selected_async_block_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn main() ~ FileSystem + Environment {
    let mode = 1;
    let task = match mode {
        0 => async { read_file("ops.txt") },
        _ => async { getenv("TOKEN") },
    };
    task.await;
}
"#,
    )
    .expect("write awaited match selected async block receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "awaited match selected async block receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["task", "task <- read_file"])
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["Environment"],
        serde_json::json!(["task", "task <- getenv"])
    );
}

#[test]
fn check_reports_effect_for_effectful_struct_field_call() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_struct_field_effect_gate_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

fn main() {
    let ops = Ops { loader: read_file };
    (ops.loader)("ops.txt");
}
"#,
    )
    .expect("write struct field effect fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run buildc check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "effectful struct field call should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("ops.loader"),
        "diagnostic should name field call source:\n{}",
        stderr
    );
}

#[test]
fn check_receipt_records_effectful_struct_field_call_source() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_struct_field_effect_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

fn main() ~ FileSystem {
    let ops = Ops { loader: read_file };
    (ops.loader)("ops.txt");
}
"#,
    )
    .expect("write struct field effect receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "struct field effect receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["ops.loader"])
    );
}

#[test]
fn check_receipt_records_struct_update_effectful_field_origin() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_struct_update_effectful_field_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let defaults = Ops { loader: load_config };
    let ops = Ops { ..defaults };
    (ops.loader)("ops.txt");
}
"#,
    )
    .expect("write struct update effectful field receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "struct update field effect receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "ops.loader"])
    );
}

#[test]
fn check_receipt_records_nested_struct_update_effectful_field_origin() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_nested_struct_update_effectful_field_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

struct Outer {
    ops: Ops
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let defaults = Outer { ops: Ops { loader: load_config } };
    let outer = Outer { ..defaults };
    (outer.ops.loader)("ops.txt");
}
"#,
    )
    .expect("write nested struct update effectful field receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "nested struct update effect receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "outer.ops.loader"])
    );
}

#[test]
fn check_receipt_records_destructured_nested_struct_update_effectful_field_origin() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_destructured_nested_struct_update_effectful_field_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

struct Outer {
    ops: Ops
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let defaults = Outer { ops: Ops { loader: load_config } };
    let outer = Outer { ..defaults };
    let Outer { ops } = outer;
    (ops.loader)("ops.txt");
}
"#,
    )
    .expect("write destructured nested struct update effectful field receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "destructured nested struct update effect receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "ops.loader"])
    );
}

#[test]
fn check_receipt_records_struct_update_expression_destructured_effectful_field_origin() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_struct_update_expression_destructured_effectful_field_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

struct Outer {
    ops: Ops
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let defaults = Outer { ops: Ops { loader: load_config } };
    let Outer { ops } = Outer { ..defaults };
    (ops.loader)("ops.txt");
}
"#,
    )
    .expect("write struct update expression destructured effectful field receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "struct update expression destructured effect receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "ops.loader"])
    );
}

#[test]
fn check_receipt_records_struct_update_expression_explicit_field_destructured_origin() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_struct_update_expression_explicit_field_destructured_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

struct Outer {
    ops: Ops
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let defaults = Outer { ops: Ops { loader: load_config } };
    let replacement = Ops { loader: load_config };
    let Outer { ops } = Outer { ops: replacement, ..defaults };
    (ops.loader)("ops.txt");
}
"#,
    )
    .expect("write struct update expression explicit field destructured receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "struct update expression explicit field destructured receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "ops.loader"])
    );
}

#[test]
fn check_receipt_records_tuple_literal_destructured_aggregate_origin_without_stale_alias() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_tuple_literal_destructured_aggregate_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let replacement = Ops { loader: load_config };
    let (ops,) = (replacement,);
    (ops.loader)("ops.txt");
}
"#,
    )
    .expect("write tuple literal destructured aggregate receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "tuple literal destructured aggregate receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "ops.loader"])
    );
}

#[test]
fn check_receipt_records_stored_variant_aggregate_origin_without_stale_alias() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_stored_variant_aggregate_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

enum Slot {
    Ready(Ops),
    Empty
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let replacement = Ops { loader: load_config };
    let slot = Slot::Ready(replacement);
    match slot {
        Slot::Ready(ops) => { (ops.loader)("ops.txt"); }
        Slot::Empty => { }
    }
}
"#,
    )
    .expect("write stored variant aggregate receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "stored variant aggregate receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "ops.loader"])
    );
}

#[test]
fn check_receipt_records_selected_aggregate_origin_without_stale_branch_aliases() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_selected_aggregate_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let use_secret = true;
    let config = Ops { loader: load_config };
    let secret = Ops { loader: load_secret };
    let ops = if use_secret { secret } else { config };
    (ops.loader)("ops.txt");
}
"#,
    )
    .expect("write selected aggregate receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "selected aggregate receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "ops.loader"])
    );
}

#[test]
fn check_receipt_records_tuple_destructured_selected_aggregate_origins() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_tuple_selected_aggregate_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let use_secret = true;
    let config = Ops { loader: load_config };
    let secret = Ops { loader: load_secret };
    let (ops,) = (if use_secret { secret } else { config },);
    (ops.loader)("ops.txt");
}
"#,
    )
    .expect("write tuple destructured selected aggregate receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "tuple destructured selected aggregate receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "ops.loader"])
    );
}

#[test]
fn check_receipt_records_nested_if_let_selected_aggregate_origins() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_nested_if_let_selected_aggregate_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

struct Outer {
    ops: Ops
}

enum Slot {
    Ready(i32),
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let slot = Slot::Ready(0);
    let config = Ops { loader: load_config };
    let secret = Ops { loader: load_secret };
    let outer = Outer { ops: if let Slot::Ready(version) = slot { secret } else { config } };
    (outer.ops.loader)("ops.txt");
}
"#,
    )
    .expect("write nested if-let selected aggregate receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "nested if-let selected aggregate receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "outer.ops.loader"])
    );
}

#[test]
fn check_receipt_records_tuple_destructured_nested_if_let_selected_aggregate_origins() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_tuple_nested_if_let_selected_aggregate_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

struct Outer {
    ops: Ops
}

enum Slot {
    Ready(i32),
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let slot = Slot::Ready(0);
    let config = Ops { loader: load_config };
    let secret = Ops { loader: load_secret };
    let Outer { ops } = Outer { ops: if let Slot::Ready(version) = slot { secret } else { config } };
    (ops.loader)("ops.txt");
}
"#,
    )
    .expect("write destructured nested if-let selected aggregate receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "destructured nested if-let selected aggregate receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "ops.loader"])
    );
}

#[test]
fn check_receipt_records_shorthand_aggregate_field_origin_without_stale_alias() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_shorthand_aggregate_field_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

struct Outer {
    ops: Ops
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let ops = Ops { loader: load_config };
    let outer = Outer { ops };
    (outer.ops.loader)("ops.txt");
}
"#,
    )
    .expect("write shorthand aggregate field receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "shorthand aggregate field receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "outer.ops.loader"])
    );
}

#[test]
fn check_receipt_records_direct_shorthand_aggregate_destructuring_origin_without_stale_alias() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_direct_shorthand_aggregate_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

struct Outer {
    ops: Ops
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let ops = Ops { loader: load_config };
    let Outer { ops: unpacked } = Outer { ops };
    (unpacked.loader)("ops.txt");
}
"#,
    )
    .expect("write direct shorthand aggregate destructuring receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "direct shorthand aggregate destructuring receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "unpacked.loader"])
    );
}

#[test]
fn check_receipt_clears_stale_aggregate_sources_for_shadowed_opaque_binding() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_shadowed_opaque_aggregate_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn make_ops() -> Ops {
    Ops { loader: read_file }
}

fn main() ~ FileSystem {
    let ops = Ops { loader: load_config };
    let ops = make_ops();
    (ops.loader)("ops.txt");
}
"#,
    )
    .expect("write shadowed opaque aggregate receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "shadowed opaque aggregate receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["make_ops().loader", "ops.loader"])
    );
}

#[test]
fn check_receipt_clears_outer_aggregate_sources_for_inner_shadowed_opaque_binding() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_inner_shadowed_opaque_aggregate_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn make_ops() -> Ops {
    Ops { loader: read_file }
}

fn main() ~ FileSystem {
    let ops = Ops { loader: load_config };
    {
        let ops = make_ops();
        (ops.loader)("ops.txt");
    }
}
"#,
    )
    .expect("write inner shadowed opaque aggregate receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "inner shadowed opaque aggregate receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["make_ops().loader", "ops.loader"])
    );
}

#[test]
fn check_receipt_clears_outer_aggregate_sources_when_inner_shadow_is_copied() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_inner_shadowed_aggregate_copy_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn make_ops() -> Ops {
    Ops { loader: read_file }
}

fn main() ~ FileSystem {
    let ops = Ops { loader: load_config };
    {
        let ops = make_ops();
        let copied = ops;
        (copied.loader)("ops.txt");
    }
}
"#,
    )
    .expect("write inner shadowed aggregate copy receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "inner shadowed aggregate copy receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["copied.loader", "make_ops().loader", "ops.loader"])
    );
}

#[test]
fn check_reports_effect_for_effectful_tuple_field_call() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_tuple_field_effect_gate_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn main() {
    let loaders = (read_file,);
    (loaders.0)("ops.txt");
}
"#,
    )
    .expect("write tuple field effect fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run buildc check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "effectful tuple field call should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("loaders.0"),
        "diagnostic should name tuple field call source:\n{}",
        stderr
    );
}

#[test]
fn check_receipt_refreshes_outer_alias_source_after_inner_block_assignment() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_inner_assignment_alias_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let mut loader = load_config;
    {
        loader = load_secret;
    };
    loader("ops.txt");
}
"#,
    )
    .expect("write inner assignment alias receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "inner assignment alias receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_secret", "loader"])
    );
}

#[test]
fn check_receipt_refreshes_outer_aggregate_source_after_inner_block_assignment() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_inner_assignment_aggregate_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let mut ops = Ops { loader: load_config };
    {
        ops.loader = load_secret;
    };
    (ops.loader)("ops.txt");
}
"#,
    )
    .expect("write inner assignment aggregate receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "inner assignment aggregate receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_secret", "ops.loader"])
    );
}

#[test]
fn check_receipt_merges_outer_alias_sources_after_conditional_assignment() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_conditional_assignment_alias_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let use_secret = true;
    let mut loader = load_config;
    if use_secret {
        loader = load_secret;
    };
    loader("ops.txt");
}
"#,
    )
    .expect("write conditional assignment alias receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "conditional assignment alias receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "loader"])
    );
}

#[test]
fn check_receipt_merges_outer_aggregate_sources_after_conditional_assignment() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_conditional_assignment_aggregate_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let use_secret = true;
    let mut ops = Ops { loader: load_config };
    if use_secret {
        ops.loader = load_secret;
    };
    (ops.loader)("ops.txt");
}
"#,
    )
    .expect("write conditional assignment aggregate receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "conditional assignment aggregate receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "ops.loader"])
    );
}

#[test]
fn check_receipt_merges_outer_alias_sources_after_if_let_assignment() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_if_let_assignment_alias_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
enum Slot {
    Ready(i32),
    Empty,
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let slot = Slot::Ready(0);
    let mut loader = load_config;
    if let Slot::Ready(version) = slot {
        loader = load_secret;
    };
    loader("ops.txt");
}
"#,
    )
    .expect("write if-let assignment alias receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "if-let assignment alias receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "loader"])
    );
}

#[test]
fn check_receipt_merges_outer_aggregate_sources_after_if_let_assignment() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_if_let_assignment_aggregate_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

enum Slot {
    Ready(i32),
    Empty,
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let slot = Slot::Ready(0);
    let mut ops = Ops { loader: load_config };
    if let Slot::Ready(version) = slot {
        ops.loader = load_secret;
    };
    (ops.loader)("ops.txt");
}
"#,
    )
    .expect("write if-let assignment aggregate receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "if-let assignment aggregate receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "ops.loader"])
    );
}

#[test]
fn check_receipt_merges_outer_alias_sources_after_if_else_assignment() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_if_else_assignment_alias_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_backup(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let use_secret = true;
    let mut loader = load_config;
    if use_secret {
        loader = load_secret;
    } else {
        loader = load_backup;
    };
    loader("ops.txt");
}
"#,
    )
    .expect("write if-else assignment alias receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "if-else assignment alias receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_backup", "load_secret", "loader"])
    );
}

#[test]
fn check_receipt_merges_outer_alias_sources_after_match_assignment() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_match_assignment_alias_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_backup(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let mode = 0;
    let mut loader = load_config;
    match mode {
        0 => { loader = load_secret; }
        _ => { loader = load_backup; }
    };
    loader("ops.txt");
}
"#,
    )
    .expect("write match assignment alias receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "match assignment alias receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_backup", "load_secret", "loader"])
    );
}

#[test]
fn check_receipt_keeps_pre_match_alias_source_after_guarded_match_assignment() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_guarded_match_assignment_alias_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_backup(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let mode = true;
    let allow_secret = false;
    let mut loader = load_config;
    match mode {
        true if allow_secret => { loader = load_secret; },
        false => { loader = load_backup; }
    };
    loader("ops.txt");
}
"#,
    )
    .expect("write guarded match assignment alias receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "guarded match assignment alias receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_backup", "load_config", "load_secret", "loader"])
    );
}

#[test]
fn check_receipt_merges_outer_aggregate_sources_after_match_assignment() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_match_assignment_aggregate_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_backup(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let mode = 0;
    let mut ops = Ops { loader: load_config };
    match mode {
        0 => { ops.loader = load_secret; }
        _ => { ops.loader = load_backup; }
    };
    (ops.loader)("ops.txt");
}
"#,
    )
    .expect("write match assignment aggregate receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "match assignment aggregate receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_backup", "load_secret", "ops.loader"])
    );
}

#[test]
fn check_receipt_merges_outer_alias_sources_after_while_assignment() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_while_assignment_alias_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let should_reload = false;
    let mut loader = load_config;
    while should_reload {
        loader = load_secret;
    };
    loader("ops.txt");
}
"#,
    )
    .expect("write while assignment alias receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "while assignment alias receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "loader"])
    );
}

#[test]
fn check_receipt_merges_outer_aggregate_sources_after_while_assignment() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_while_assignment_aggregate_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let should_reload = false;
    let mut ops = Ops { loader: load_config };
    while should_reload {
        ops.loader = load_secret;
    };
    (ops.loader)("ops.txt");
}
"#,
    )
    .expect("write while assignment aggregate receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "while assignment aggregate receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "ops.loader"])
    );
}

#[test]
fn check_receipt_merges_outer_alias_sources_after_for_assignment() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_for_assignment_alias_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let mut loader = load_config;
    for item in [0] {
        loader = load_secret;
    };
    loader("ops.txt");
}
"#,
    )
    .expect("write for assignment alias receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "for assignment alias receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "loader"])
    );
}

#[test]
fn check_receipt_merges_outer_alias_sources_after_while_let_assignment() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_while_let_assignment_alias_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
enum Slot {
    Ready(i32),
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let slot = Slot::Ready(0);
    let mut loader = load_config;
    while let Slot::Ready(version) = slot {
        loader = load_secret;
        break;
    };
    loader("ops.txt");
}
"#,
    )
    .expect("write while-let assignment alias receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "while-let assignment alias receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "loader"])
    );
}

#[test]
fn check_receipt_merges_outer_aggregate_sources_after_while_let_assignment() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_while_let_assignment_aggregate_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

enum Slot {
    Ready(i32),
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let slot = Slot::Ready(0);
    let mut ops = Ops { loader: load_config };
    while let Slot::Ready(version) = slot {
        ops.loader = load_secret;
        break;
    };
    (ops.loader)("ops.txt");
}
"#,
    )
    .expect("write while-let assignment aggregate receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "while-let assignment aggregate receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "ops.loader"])
    );
}

#[test]
fn check_receipt_merges_outer_alias_sources_after_loop_break_assignment() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_loop_break_assignment_alias_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let stop_now = true;
    let mut loader = load_config;
    loop {
        if stop_now {
            break;
        };
        loader = load_secret;
        break;
    };
    loader("ops.txt");
}
"#,
    )
    .expect("write loop break assignment alias receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "loop break assignment alias receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "loader"])
    );
}

#[test]
fn check_receipt_merges_outer_aggregate_sources_after_loop_break_assignment() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_loop_break_assignment_aggregate_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let stop_now = true;
    let mut ops = Ops { loader: load_config };
    loop {
        if stop_now {
            break;
        };
        ops.loader = load_secret;
        break;
    };
    (ops.loader)("ops.txt");
}
"#,
    )
    .expect("write loop break assignment aggregate receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "loop break assignment aggregate receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "ops.loader"])
    );
}

#[test]
fn check_receipt_records_effectful_tuple_field_call_source() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_tuple_field_effect_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn main() ~ FileSystem {
    let loaders = (read_file,);
    (loaders.0)("ops.txt");
}
"#,
    )
    .expect("write tuple field effect receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "tuple field effect receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["loaders.0"])
    );
}

#[test]
fn check_reports_effect_for_effectful_index_call() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_index_effect_gate_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn main() {
    let loaders = [read_file];
    (loaders[0])("ops.txt");
}
"#,
    )
    .expect("write index effect fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run buildc check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "effectful index call should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("loaders[0]"),
        "diagnostic should name indexed call source:\n{}",
        stderr
    );
}

#[test]
fn check_receipt_records_effectful_index_call_source() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_index_effect_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn main() ~ FileSystem {
    let loaders = [read_file];
    (loaders[0])("ops.txt");
}
"#,
    )
    .expect("write index effect receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "indexed effect receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["loaders[0]"])
    );
}

#[test]
fn check_reports_effect_for_immediate_returned_effectful_function_call() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_returned_effectful_function_gate_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn make_loader() -> (fn(str) -> str) with FileSystem {
    read_file
}

fn main() {
    make_loader()("ops.txt");
}
"#,
    )
    .expect("write returned effectful function fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run buildc check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "immediate returned effectful function call should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("make_loader()"),
        "diagnostic should name returned function call source:\n{}",
        stderr
    );
}

#[test]
fn check_receipt_records_immediate_returned_effectful_function_call_source() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_returned_effectful_function_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn make_loader() -> (fn(str) -> str) with FileSystem {
    read_file
}

fn main() ~ FileSystem {
    make_loader()("ops.txt");
}
"#,
    )
    .expect("write returned effectful function receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "returned effectful function receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["make_loader()"])
    );
}

#[test]
fn check_reports_effect_for_pipe_effectful_function_call() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_pipe_effectful_function_gate_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() {
    "ops.toml" |> load_config;
}
"#,
    )
    .expect("write pipe effectful function fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "pipe effectful function call should fail without FileSystem effect\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["status"], "failed");
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config"])
    );
    let diagnostics = receipt["diagnostics"]
        .as_array()
        .expect("diagnostics array");
    assert!(
        diagnostics.iter().any(|diag| {
            let message = diag["message"].as_str().unwrap_or_default();
            message.contains("FileSystem") && message.contains("main")
        }),
        "missing FileSystem diagnostic for pipe effectful function call\nreceipt:\n{}",
        serde_json::to_string_pretty(&receipt).expect("receipt should serialize")
    );
}

#[test]
fn check_receipt_records_pipe_effectful_function_call_source() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_pipe_effectful_function_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    "ops.toml" |> load_config;
}
"#,
    )
    .expect("write pipe effectful function receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "pipe effectful function receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config"])
    );
}

#[test]
fn check_rejects_function_values_with_shift_operator() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_function_shift_gate_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() {
    let pipeline = load_config >> load_secret;
}
"#,
    )
    .expect("write function shift fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "function values used with >> should be rejected\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    let diagnostics = receipt["diagnostics"]
        .as_array()
        .expect("diagnostics array");
    assert!(
        diagnostics.iter().any(|diag| {
            let message = diag["message"].as_str().unwrap_or_default();
            message.contains("binary operator `>>`") && message.contains("FileSystem")
        }),
        "missing invalid function shift diagnostic\nreceipt:\n{}",
        serde_json::to_string_pretty(&receipt).expect("receipt should serialize")
    );
}

#[test]
fn check_reports_effect_for_control_flow_selected_effectful_function_call() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_control_flow_selected_effectful_function_gate_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn load_secret() -> str ~ FileSystem {
    read_file("secret.toml")
}

fn main() {
    let use_secret = true;
    (if use_secret { load_secret } else { load_config })();
}
"#,
    )
    .expect("write control-flow selected effectful function fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run buildc check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "control-flow selected effectful function call should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("load_config"),
        "diagnostic should name one possible branch source:\n{}",
        stderr
    );
    assert!(
        stderr.contains("load_secret"),
        "diagnostic should name the other possible branch source:\n{}",
        stderr
    );
}

#[test]
fn check_receipt_records_control_flow_selected_effectful_function_sources() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_control_flow_effectful_function_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn load_secret() -> str ~ FileSystem {
    read_file("secret.toml")
}

fn main() ~ FileSystem {
    let use_secret = true;
    (if use_secret { load_secret } else { load_config })();
}
"#,
    )
    .expect("write control-flow selected effectful function receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "control-flow selected effectful function receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret"])
    );
}

#[test]
fn check_reports_effect_for_match_selected_effectful_function_call() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_match_selected_effectful_function_gate_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn load_secret() -> str ~ FileSystem {
    read_file("secret.toml")
}

fn main() {
    let use_secret = true;
    (match use_secret {
        true => load_secret,
        false => load_config,
    })();
}
"#,
    )
    .expect("write match selected effectful function fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run buildc check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "match selected effectful function call should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("load_config"),
        "diagnostic should name one possible match source:\n{}",
        stderr
    );
    assert!(
        stderr.contains("load_secret"),
        "diagnostic should name the other possible match source:\n{}",
        stderr
    );
}

#[test]
fn check_receipt_records_match_selected_effectful_function_sources() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_match_effectful_function_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn load_secret() -> str ~ FileSystem {
    read_file("secret.toml")
}

fn main() ~ FileSystem {
    let use_secret = true;
    (match use_secret {
        true => load_secret,
        false => load_config,
    })();
}
"#,
    )
    .expect("write match selected effectful function receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "match selected effectful function receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret"])
    );
}

#[test]
fn check_reports_effect_for_let_bound_selected_effectful_function_call() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_let_bound_selected_effectful_function_gate_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn load_secret() -> str ~ FileSystem {
    read_file("secret.toml")
}

fn main() {
    let use_secret = true;
    let loader = if use_secret { load_secret } else { load_config };
    loader();
}
"#,
    )
    .expect("write let-bound selected effectful function fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run buildc check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "let-bound selected effectful function call should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("loader"),
        "diagnostic should name selected binding source:\n{}",
        stderr
    );
    assert!(
        stderr.contains("load_config"),
        "diagnostic should name one possible selected source:\n{}",
        stderr
    );
    assert!(
        stderr.contains("load_secret"),
        "diagnostic should name the other possible selected source:\n{}",
        stderr
    );
}

#[test]
fn check_reports_effect_for_if_let_bound_selected_effectful_function_call() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_if_let_bound_selected_effectful_function_gate_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
enum Slot {
    Ready(i32),
}

fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn load_secret() -> str ~ FileSystem {
    read_file("secret.toml")
}

fn main() {
    let slot = Slot::Ready(0);
    let loader = if let Slot::Ready(version) = slot { load_secret } else { load_config };
    loader();
}
"#,
    )
    .expect("write if-let-bound selected effectful function fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run buildc check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "if-let-bound selected effectful function call should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("loader"),
        "diagnostic should name selected binding source:\n{}",
        stderr
    );
    assert!(
        stderr.contains("load_config"),
        "diagnostic should name if-let fallback source:\n{}",
        stderr
    );
    assert!(
        stderr.contains("load_secret"),
        "diagnostic should name if-let matched source:\n{}",
        stderr
    );
}

#[test]
fn check_rejects_try_operator_on_selected_effectful_function_value() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_try_selected_effectful_function_gate_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn load_secret() -> str ~ FileSystem {
    read_file("secret.toml")
}

fn main() {
    let use_secret = true;
    let loader = if use_secret { load_secret } else { load_config };
    loader?();
}
"#,
    )
    .expect("write try selected effectful function fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run buildc check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "try operator on selected effectful function value should be rejected"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("`?`"),
        "diagnostic should name the try operator:\n{}",
        stderr
    );
    assert!(
        stderr.contains("fn()"),
        "diagnostic should name the invalid function operand:\n{}",
        stderr
    );
}

#[test]
fn check_receipt_records_let_bound_selected_effectful_function_sources() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_let_bound_selected_effectful_function_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn load_secret() -> str ~ FileSystem {
    read_file("secret.toml")
}

fn main() ~ FileSystem {
    let use_secret = true;
    let loader = if use_secret { load_secret } else { load_config };
    loader();
}
"#,
    )
    .expect("write let-bound selected effectful function receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "let-bound selected effectful function receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "loader"])
    );
}

#[test]
fn check_receipt_records_if_let_bound_selected_effectful_function_sources() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_if_let_bound_selected_effectful_function_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
enum Slot {
    Ready(i32),
}

fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn load_secret() -> str ~ FileSystem {
    read_file("secret.toml")
}

fn main() ~ FileSystem {
    let slot = Slot::Ready(0);
    let loader = if let Slot::Ready(version) = slot { load_secret } else { load_config };
    loader();
}
"#,
    )
    .expect("write if-let-bound selected effectful function receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "if-let-bound selected effectful function receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "loader"])
    );
}

#[test]
fn check_receipt_records_cast_selected_effectful_function_sources() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_cast_selected_effectful_function_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn load_secret() -> str ~ FileSystem {
    read_file("secret.toml")
}

fn main() ~ FileSystem {
    let use_secret = true;
    let loader = (if use_secret { load_secret } else { load_config }) as (fn() -> str) with FileSystem;
    loader();
}
"#,
    )
    .expect("write cast selected effectful function receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "cast selected effectful function receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "loader"])
    );
}

#[test]
fn check_rejects_cast_laundering_selected_effectful_function_to_pure_callback() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_cast_selected_effectful_function_to_pure_gate_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn load_secret() -> str ~ FileSystem {
    read_file("secret.toml")
}

fn main() {
    let use_secret = true;
    let loader = (if use_secret { load_secret } else { load_config }) as fn() -> str;
    loader();
}
"#,
    )
    .expect("write pure cast selected effectful function fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run buildc check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "casting selected effectful function to pure callback should be rejected"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should retain the erased effect row:\n{}",
        stderr
    );
}

#[test]
fn check_receipt_records_ref_deref_selected_effectful_function_sources() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_ref_deref_selected_effectful_function_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn load_secret() -> str ~ FileSystem {
    read_file("secret.toml")
}

fn main() ~ FileSystem {
    let use_secret = true;
    let loader = if use_secret { load_secret } else { load_config };
    let loader_ref = &loader;
    (*loader_ref)();
}
"#,
    )
    .expect("write ref-deref selected effectful function receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "ref-deref selected effectful function receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "loader", "loader_ref"])
    );
}

#[test]
fn check_receipt_records_tuple_destructured_selected_effectful_function_sources() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_tuple_destructured_selected_effectful_function_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn load_secret() -> str ~ FileSystem {
    read_file("secret.toml")
}

fn main() ~ FileSystem {
    let use_secret = true;
    let (loader,) = (if use_secret { load_secret } else { load_config },);
    loader();
}
"#,
    )
    .expect("write tuple-destructured effectful function receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "tuple-destructured effectful function receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "loader"])
    );
}

#[test]
fn check_receipt_records_struct_destructured_selected_effectful_function_sources() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_struct_destructured_selected_effectful_function_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn() -> str) with FileSystem
}

fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn load_secret() -> str ~ FileSystem {
    read_file("secret.toml")
}

fn main() ~ FileSystem {
    let use_secret = true;
    let ops = Ops { loader: if use_secret { load_secret } else { load_config } };
    let Ops { loader } = ops;
    loader();
}
"#,
    )
    .expect("write struct-destructured effectful function receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "struct-destructured effectful function receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "loader"])
    );
}

#[test]
fn check_receipt_records_slice_destructured_selected_effectful_function_sources() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_slice_destructured_selected_effectful_function_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn load_secret() -> str ~ FileSystem {
    read_file("secret.toml")
}

fn main() ~ FileSystem {
    let use_secret = true;
    let loaders = [if use_secret { load_secret } else { load_config }];
    let [loader] = loaders;
    loader();
}
"#,
    )
    .expect("write slice-destructured effectful function receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "slice-destructured effectful function receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "loader"])
    );
}

#[test]
fn check_receipt_records_tuple_struct_destructured_selected_effectful_function_sources() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_tuple_struct_destructured_selected_effectful_function_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Slot((fn() -> str) with FileSystem);

fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn load_secret() -> str ~ FileSystem {
    read_file("secret.toml")
}

fn main() ~ FileSystem {
    let use_secret = true;
    let slot = Slot(if use_secret { load_secret } else { load_config });
    let Slot(loader) = slot;
    loader();
}
"#,
    )
    .expect("write tuple-struct destructured selected effectful function receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "tuple-struct destructured selected effectful function receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "loader"])
    );
}

#[test]
fn check_receipt_records_enum_variant_destructured_selected_effectful_function_sources() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_enum_variant_destructured_selected_effectful_function_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
enum Slot {
    Ready((fn() -> str) with FileSystem),
}

fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn load_secret() -> str ~ FileSystem {
    read_file("secret.toml")
}

fn main() ~ FileSystem {
    let use_secret = true;
    let slot = if use_secret {
        Slot::Ready(load_secret)
    } else {
        Slot::Ready(load_config)
    };
    match slot {
        Slot::Ready(loader) => loader(),
    };
}
"#,
    )
    .expect("write enum-variant destructured selected effectful function receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "enum-variant destructured selected effectful function receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "loader"])
    );
}

#[test]
fn check_struct_enum_variant_destructured_callback_requires_declared_effect() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_struct_enum_variant_callback_requires_effect_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
enum Slot {
    Ready { loader: (fn() -> str) with FileSystem },
}

fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn load_secret() -> str ~ FileSystem {
    read_file("secret.toml")
}

fn main() {
    let use_secret = true;
    let slot = if use_secret {
        Slot::Ready { loader: load_secret }
    } else {
        Slot::Ready { loader: load_config }
    };
    match slot {
        Slot::Ready { loader } => loader(),
    };
}
"#,
    )
    .expect("write struct enum variant callback effect fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "struct enum variant callback without declared effect should fail\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["status"], "failed");
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "loader"])
    );
    let diagnostics = receipt["diagnostics"]
        .as_array()
        .expect("diagnostics array");
    assert!(
        diagnostics.iter().any(|diag| {
            let message = diag["message"].as_str().unwrap_or_default();
            message.contains("FileSystem") && message.contains("main")
        }),
        "missing FileSystem diagnostic for struct enum variant callback\nreceipt:\n{}",
        serde_json::to_string_pretty(&receipt).expect("receipt should serialize")
    );
}

#[test]
fn check_if_let_destructured_enum_variant_callback_requires_declared_effect() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_if_let_enum_variant_callback_requires_effect_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
enum Slot {
    Ready((fn() -> str) with FileSystem),
}

fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn load_secret() -> str ~ FileSystem {
    read_file("secret.toml")
}

fn main() {
    let use_secret = true;
    let slot = if use_secret {
        Slot::Ready(load_secret)
    } else {
        Slot::Ready(load_config)
    };
    if let Slot::Ready(loader) = slot {
        loader();
    };
}
"#,
    )
    .expect("write if-let enum variant callback effect fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "if-let destructured callback without declared effect should fail\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["status"], "failed");
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "loader"])
    );
    let diagnostics = receipt["diagnostics"]
        .as_array()
        .expect("diagnostics array");
    assert!(
        diagnostics.iter().any(|diag| {
            let message = diag["message"].as_str().unwrap_or_default();
            message.contains("FileSystem") && message.contains("main")
        }),
        "missing FileSystem diagnostic for if-let callback\nreceipt:\n{}",
        serde_json::to_string_pretty(&receipt).expect("receipt should serialize")
    );
}

#[test]
fn check_while_let_destructured_enum_variant_callback_requires_declared_effect() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_while_let_enum_variant_callback_requires_effect_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
enum Slot {
    Ready((fn() -> str) with FileSystem),
}

fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn load_secret() -> str ~ FileSystem {
    read_file("secret.toml")
}

fn main() {
    let use_secret = true;
    let slot = if use_secret {
        Slot::Ready(load_secret)
    } else {
        Slot::Ready(load_config)
    };
    while let Slot::Ready(loader) = slot {
        loader();
        break;
    };
}
"#,
    )
    .expect("write while-let enum variant callback effect fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "while-let destructured callback without declared effect should fail\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["status"], "failed");
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "loader"])
    );
    let diagnostics = receipt["diagnostics"]
        .as_array()
        .expect("diagnostics array");
    assert!(
        diagnostics.iter().any(|diag| {
            let message = diag["message"].as_str().unwrap_or_default();
            message.contains("FileSystem") && message.contains("main")
        }),
        "missing FileSystem diagnostic for while-let callback\nreceipt:\n{}",
        serde_json::to_string_pretty(&receipt).expect("receipt should serialize")
    );
}

#[test]
fn check_receipt_file_records_failing_capability_diagnostic() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_fail_{}.bld",
        std::process::id()
    ));
    let receipt_path = fixture.with_extension("receipt.json");
    fs::write(&fixture, r#"fn main() { read_file("ops.txt"); }"#)
        .expect("write failing receipt fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg(&receipt_path)
        .output()
        .expect("run buildc check --receipt file");

    let receipt_text = fs::read_to_string(&receipt_path).expect("read receipt file");
    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&receipt_path);

    assert!(
        !output.status.success(),
        "failing capability check should return nonzero"
    );
    let receipt: serde_json::Value =
        serde_json::from_str(&receipt_text).expect("receipt file should be JSON");
    assert_eq!(receipt["schema"], "buildlang-check-receipt/v1");
    assert_eq!(receipt["status"], "failed");
    assert_eq!(receipt["compiler_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(receipt["language_version"], "1.0.0");
    assert_eq!(receipt["source_digest"]["algorithm"], "sha256");
    assert_eq!(
        receipt["source_digest"]["hex"]
            .as_str()
            .expect("failing receipt digest")
            .len(),
        64
    );
    assert_eq!(
        receipt["observed_capabilities"]["main"]["FileSystem"],
        serde_json::json!(["read_file"])
    );
    let diagnostics = receipt["diagnostics"]
        .as_array()
        .expect("diagnostics array");
    assert!(
        diagnostics.iter().any(|diag| {
            diag["stage"] == "type"
                && diag["kind"] == "UnhandledEffect"
                && diag["message"]
                    .as_str()
                    .unwrap_or("")
                    .contains("FileSystem")
        }),
        "expected FileSystem UnhandledEffect diagnostic in {diagnostics:#?}"
    );
}

#[test]
fn check_receipt_source_digest_ignores_path_for_identical_content() {
    let id = std::process::id();
    let left = std::env::temp_dir().join(format!("buildlang_check_receipt_digest_left_{id}.bld"));
    let right = std::env::temp_dir().join(format!("buildlang_check_receipt_digest_right_{id}.bld"));
    let source = r#"fn main() ~ Console { println!("same"); }"#;
    fs::write(&left, source).expect("write left digest fixture");
    fs::write(&right, source).expect("write right digest fixture");

    let left_output = buildc()
        .arg("check")
        .arg(&left)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run left digest receipt");
    let right_output = buildc()
        .arg("check")
        .arg(&right)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run right digest receipt");

    let _ = fs::remove_file(&left);
    let _ = fs::remove_file(&right);

    assert!(left_output.status.success(), "left check should pass");
    assert!(right_output.status.success(), "right check should pass");
    let left_receipt = receipt_from_stdout(&left_output);
    let right_receipt = receipt_from_stdout(&right_output);
    assert_ne!(left_receipt["source"], right_receipt["source"]);
    let left_digest = left_receipt["source_digest"]["hex"]
        .as_str()
        .expect("left source digest hex string");
    let right_digest = right_receipt["source_digest"]["hex"]
        .as_str()
        .expect("right source digest hex string");
    assert_eq!(left_digest.len(), 64);
    assert_eq!(right_digest.len(), 64);
    assert_eq!(
        left_receipt["source_digest"]["hex"],
        right_receipt["source_digest"]["hex"]
    );
}

#[test]
fn check_receipt_source_digest_changes_when_source_changes() {
    let id = std::process::id();
    let first = std::env::temp_dir().join(format!("buildlang_check_receipt_digest_first_{id}.bld"));
    let second =
        std::env::temp_dir().join(format!("buildlang_check_receipt_digest_second_{id}.bld"));
    fs::write(&first, r#"fn main() ~ Console { println!("first"); }"#)
        .expect("write first digest fixture");
    fs::write(&second, r#"fn main() ~ Console { println!("second"); }"#)
        .expect("write second digest fixture");

    let first_output = buildc()
        .arg("check")
        .arg(&first)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run first digest receipt");
    let second_output = buildc()
        .arg("check")
        .arg(&second)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run second digest receipt");

    let _ = fs::remove_file(&first);
    let _ = fs::remove_file(&second);

    assert!(first_output.status.success(), "first check should pass");
    assert!(second_output.status.success(), "second check should pass");
    let first_receipt = receipt_from_stdout(&first_output);
    let second_receipt = receipt_from_stdout(&second_output);
    assert_ne!(
        first_receipt["source_digest"]["hex"],
        second_receipt["source_digest"]["hex"]
    );
}

#[test]
fn check_receipt_input_digests_track_included_source_changes() {
    let dir = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_inputs_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create input digest fixture dir");
    let entry = dir.join("entry.bld");
    let shared = dir.join("shared.bld");
    fs::write(
        &entry,
        r#"include!("shared.bld");
fn main() ~ Console { println!("{}", value()); }
"#,
    )
    .expect("write entry fixture");
    fs::write(&shared, "fn value() -> i32 { 7 }\n").expect("write first include fixture");

    let first_output = buildc()
        .arg("check")
        .arg(&entry)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run first input digest receipt");
    assert!(
        first_output.status.success(),
        "first check should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&first_output.stdout),
        String::from_utf8_lossy(&first_output.stderr)
    );
    let first_receipt = receipt_from_stdout(&first_output);
    let first_entry_digest = first_receipt["source_digest"]["hex"]
        .as_str()
        .expect("entry source digest")
        .to_string();
    let first_input_entry = input_digest_hex(&first_receipt, "entry", "entry.bld");
    let first_input_include = input_digest_hex(&first_receipt, "include", "shared.bld");
    let first_graph_digest = input_graph_digest_hex(&first_receipt);
    assert_eq!(first_entry_digest, first_input_entry);

    fs::write(&shared, "fn value() -> i32 { 8 }\n").expect("write changed include fixture");
    let second_output = buildc()
        .arg("check")
        .arg(&entry)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run second input digest receipt");

    let _ = fs::remove_dir_all(&dir);

    assert!(
        second_output.status.success(),
        "second check should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&second_output.stdout),
        String::from_utf8_lossy(&second_output.stderr)
    );
    let second_receipt = receipt_from_stdout(&second_output);
    assert_eq!(second_receipt["source_digest"]["hex"], first_entry_digest);
    assert_eq!(
        input_digest_hex(&second_receipt, "entry", "entry.bld"),
        first_input_entry
    );
    assert_ne!(input_graph_digest_hex(&second_receipt), first_graph_digest);
    assert_ne!(
        input_digest_hex(&second_receipt, "include", "shared.bld"),
        first_input_include
    );
}

#[test]
fn check_receipt_input_digests_record_imports_and_modules() {
    let dir = std::env::temp_dir().join(format!(
        "buildlang_check_receipt_graph_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    let package_dir = dir.join("registry/packages/std-math/src");
    fs::create_dir_all(&package_dir).expect("create import package dir");
    fs::create_dir_all(dir.join("helpers")).expect("create helper module dir");
    let entry = dir.join("entry.bld");
    let imported = package_dir.join("lib.bld");
    let module = dir.join("helpers/mod.bld");

    fs::write(&imported, "fn imported_value() -> i32 { 2 }\n").expect("write import fixture");
    fs::write(&module, "fn module_value() -> i32 { 5 }\n").expect("write module fixture");
    fs::write(
        &entry,
        r#"use std_math;
mod helpers;
fn main() ~ Console {
    println!("{}", imported_value() + helpers::module_value());
}
"#,
    )
    .expect("write graph entry fixture");

    let output = buildc()
        .arg("check")
        .arg(&entry)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run graph input digest receipt");

    let _ = fs::remove_dir_all(&dir);

    assert!(
        output.status.success(),
        "graph check should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    input_digest_hex(&receipt, "entry", "entry.bld");
    input_digest_hex(&receipt, "import", "lib.bld");
    input_digest_hex(&receipt, "module", "mod.bld");
}

#[test]
fn check_receipt_input_graph_digest_is_path_portable() {
    let mut graph_digests = Vec::new();
    let mut entry_sources = Vec::new();

    for label in ["left", "right"] {
        let dir = std::env::temp_dir().join(format!(
            "buildlang_check_receipt_graph_digest_{}_{}",
            label,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create graph digest fixture dir");
        let entry = dir.join("entry.bld");
        let shared = dir.join("shared.bld");
        fs::write(
            &entry,
            r#"include!("shared.bld");
fn main() ~ Console { println!("{}", value()); }
"#,
        )
        .expect("write graph digest entry fixture");
        fs::write(&shared, "fn value() -> i32 { 11 }\n")
            .expect("write graph digest include fixture");

        let output = buildc()
            .arg("check")
            .arg(&entry)
            .arg("--receipt")
            .arg("-")
            .output()
            .expect("run graph digest receipt");

        let _ = fs::remove_dir_all(&dir);

        assert!(
            output.status.success(),
            "graph digest check should pass for {label}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let receipt = receipt_from_stdout(&output);
        entry_sources.push(receipt["source"].as_str().unwrap_or("").to_string());
        graph_digests.push(input_graph_digest_hex(&receipt));
    }

    assert_ne!(entry_sources[0], entry_sources[1]);
    assert_eq!(graph_digests[0], graph_digests[1]);
}

#[test]
fn receipt_verify_accepts_fresh_check_receipt() {
    let dir = std::env::temp_dir().join(format!(
        "buildlang_receipt_verify_fresh_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create receipt verify fixture dir");
    let entry = dir.join("entry.bld");
    let receipt = dir.join("receipt.json");
    fs::write(&entry, r#"fn main() {}"#).expect("write receipt verify entry");

    let check_output = buildc()
        .arg("check")
        .arg(&entry)
        .arg("--profile")
        .arg("pure")
        .arg("--receipt")
        .arg(&receipt)
        .output()
        .expect("write fresh check receipt");
    assert!(
        check_output.status.success(),
        "check should pass before receipt verify\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check_output.stdout),
        String::from_utf8_lossy(&check_output.stderr)
    );

    let verify_output = buildc()
        .args(["receipt", "verify"])
        .arg(&receipt)
        .output()
        .expect("verify fresh check receipt");

    let _ = fs::remove_dir_all(&dir);

    assert!(
        verify_output.status.success(),
        "fresh receipt should verify\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verify_output.stdout),
        String::from_utf8_lossy(&verify_output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&verify_output.stdout).contains("Receipt verified"),
        "stdout should confirm verification:\n{}",
        String::from_utf8_lossy(&verify_output.stdout)
    );
}

#[test]
fn receipt_verify_json_reports_passed_checks() {
    let dir = std::env::temp_dir().join(format!(
        "buildlang_receipt_verify_json_pass_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create receipt json fixture dir");
    let entry = dir.join("entry.bld");
    let receipt = dir.join("receipt.json");
    fs::write(&entry, r#"fn main() {}"#).expect("write receipt json entry");

    let check_output = buildc()
        .arg("check")
        .arg(&entry)
        .arg("--profile")
        .arg("pure")
        .arg("--receipt")
        .arg(&receipt)
        .output()
        .expect("write check receipt for json verify");
    assert!(
        check_output.status.success(),
        "check should pass before json receipt verify\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check_output.stdout),
        String::from_utf8_lossy(&check_output.stderr)
    );

    let verify_output = buildc()
        .args(["receipt", "verify"])
        .arg(&receipt)
        .arg("--json")
        .output()
        .expect("verify receipt as json");

    let _ = fs::remove_dir_all(&dir);

    assert!(
        verify_output.status.success(),
        "json receipt verification should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verify_output.stdout),
        String::from_utf8_lossy(&verify_output.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&verify_output.stdout).expect("verification report should be JSON");
    assert_eq!(report["schema"], "buildlang-receipt-verification/v1");
    assert_eq!(report["status"], "passed");
    assert_eq!(
        verification_check(&report, "source_digest")["status"],
        "passed"
    );
    assert_eq!(
        verification_check(&report, "input_graph_digest")["status"],
        "passed"
    );
    assert_eq!(
        verification_check(&report, "policy_profile_digest")["status"],
        "passed"
    );
}

#[test]
fn receipt_verify_expect_profile_rejects_stripped_policy() {
    let dir = std::env::temp_dir().join(format!(
        "buildlang_receipt_verify_expect_profile_stripped_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create expect profile fixture dir");
    let entry = dir.join("entry.bld");
    let receipt = dir.join("receipt.json");
    fs::write(&entry, r#"fn main() {}"#).expect("write expect profile entry");

    let check_output = buildc()
        .arg("check")
        .arg(&entry)
        .arg("--profile")
        .arg("ci-review")
        .arg("--receipt")
        .arg(&receipt)
        .output()
        .expect("write profile-backed receipt");
    assert!(
        check_output.status.success(),
        "check should pass before stripping policy\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check_output.stdout),
        String::from_utf8_lossy(&check_output.stderr)
    );

    let mut saved: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipt).expect("read profile receipt"))
            .expect("profile receipt should parse");
    saved
        .as_object_mut()
        .expect("receipt should be an object")
        .remove("policy");
    fs::write(
        &receipt,
        serde_json::to_string_pretty(&saved).expect("serialize stripped receipt"),
    )
    .expect("write stripped profile receipt");

    let verify_output = buildc()
        .args(["receipt", "verify"])
        .arg(&receipt)
        .arg("--expect-profile")
        .arg("ci-review")
        .output()
        .expect("verify stripped receipt with expected profile");

    let _ = fs::remove_dir_all(&dir);

    assert!(
        !verify_output.status.success(),
        "stripped policy receipt should fail expected-profile verification"
    );
    assert!(
        String::from_utf8_lossy(&verify_output.stderr)
            .contains("receipt built-in profile mismatch"),
        "stderr should report expected profile mismatch:\n{}",
        String::from_utf8_lossy(&verify_output.stderr)
    );
}

#[test]
fn receipt_verify_json_reports_expected_profile_mismatch() {
    let dir = std::env::temp_dir().join(format!(
        "buildlang_receipt_verify_expect_profile_json_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create expect profile json fixture dir");
    let entry = dir.join("entry.bld");
    let receipt = dir.join("receipt.json");
    fs::write(&entry, r#"fn main() {}"#).expect("write expect profile json entry");

    let check_output = buildc()
        .arg("check")
        .arg(&entry)
        .arg("--profile")
        .arg("pure")
        .arg("--receipt")
        .arg(&receipt)
        .output()
        .expect("write pure profile receipt");
    assert!(
        check_output.status.success(),
        "check should pass before expected-profile mismatch\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check_output.stdout),
        String::from_utf8_lossy(&check_output.stderr)
    );

    let verify_output = buildc()
        .args(["receipt", "verify"])
        .arg(&receipt)
        .arg("--expect-profile")
        .arg("ci-review")
        .arg("--json")
        .output()
        .expect("verify mismatched profile receipt as json");

    let _ = fs::remove_dir_all(&dir);

    assert!(
        !verify_output.status.success(),
        "mismatched profile receipt should fail json verification"
    );
    let report: serde_json::Value = serde_json::from_slice(&verify_output.stdout)
        .expect("profile mismatch report should be JSON");
    assert_eq!(report["status"], "failed");
    let profile_check = verification_check(&report, "expected_profile");
    assert_eq!(profile_check["status"], "failed");
    assert_eq!(profile_check["expected"], "builtin:ci-review");
    assert_eq!(profile_check["actual"], "builtin:pure");
    assert!(
        profile_check["message"]
            .as_str()
            .expect("expected profile failure message")
            .contains("receipt built-in profile mismatch"),
        "profile failure should explain mismatch: {profile_check:#?}"
    );
}

#[test]
fn receipt_verify_json_reports_failed_input_graph() {
    let dir = std::env::temp_dir().join(format!(
        "buildlang_receipt_verify_json_fail_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create receipt json failure fixture dir");
    let entry = dir.join("entry.bld");
    let shared = dir.join("shared.bld");
    let receipt = dir.join("receipt.json");
    fs::write(
        &entry,
        r#"include!("shared.bld");
fn main() ~ Console { println!("{}", value()); }
"#,
    )
    .expect("write receipt json failure entry");
    fs::write(&shared, "fn value() -> i32 { 7 }\n").expect("write first shared source");

    let check_output = buildc()
        .arg("check")
        .arg(&entry)
        .arg("--receipt")
        .arg(&receipt)
        .output()
        .expect("write staleable check receipt for json verify");
    assert!(
        check_output.status.success(),
        "check should pass before json graph mutation\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check_output.stdout),
        String::from_utf8_lossy(&check_output.stderr)
    );

    fs::write(&shared, "fn value() -> i32 { 8 }\n").expect("mutate shared source for json verify");
    let verify_output = buildc()
        .args(["receipt", "verify"])
        .arg(&receipt)
        .arg("--json")
        .output()
        .expect("verify stale receipt as json");

    let _ = fs::remove_dir_all(&dir);

    assert!(
        !verify_output.status.success(),
        "stale json receipt verification should fail"
    );
    let report: serde_json::Value =
        serde_json::from_slice(&verify_output.stdout).expect("failure report should be JSON");
    assert_eq!(report["schema"], "buildlang-receipt-verification/v1");
    assert_eq!(report["status"], "failed");
    let graph_check = verification_check(&report, "input_graph_digest");
    assert_eq!(graph_check["status"], "failed");
    assert!(
        graph_check["message"]
            .as_str()
            .expect("failure check message")
            .contains("input graph digest mismatch"),
        "graph failure should explain mismatch: {graph_check:#?}"
    );
}

#[test]
fn receipt_verify_rejects_changed_policy_file_digest() {
    let dir = std::env::temp_dir().join(format!(
        "buildlang_receipt_verify_policy_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create receipt policy fixture dir");
    let entry = dir.join("entry.bld");
    let policy = dir.join("policy.json");
    let receipt = dir.join("receipt.json");
    fs::write(&entry, r#"fn main() ~ Console { println!("ok"); }"#)
        .expect("write policy receipt entry");
    fs::write(
        &policy,
        r#"{
  "schema": "buildlang-check-policy/v1",
  "allowed_effects": ["Console"],
  "require_source_digest": true
}
"#,
    )
    .expect("write initial policy");

    let check_output = buildc()
        .arg("check")
        .arg(&entry)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg(&receipt)
        .output()
        .expect("write file-backed policy receipt");
    assert!(
        check_output.status.success(),
        "check should pass before policy mutation\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check_output.stdout),
        String::from_utf8_lossy(&check_output.stderr)
    );

    fs::write(
        &policy,
        r#"{
  "schema": "buildlang-check-policy/v1",
  "allowed_effects": ["FileSystem"],
  "require_source_digest": true
}
"#,
    )
    .expect("mutate policy file");
    let verify_output = buildc()
        .args(["receipt", "verify"])
        .arg(&receipt)
        .output()
        .expect("verify stale policy receipt");

    let _ = fs::remove_dir_all(&dir);

    assert!(
        !verify_output.status.success(),
        "stale policy digest receipt should fail"
    );
    assert!(
        String::from_utf8_lossy(&verify_output.stderr).contains("policy source digest mismatch"),
        "stderr should report policy digest mismatch:\n{}",
        String::from_utf8_lossy(&verify_output.stderr)
    );
}

#[test]
fn receipt_verify_json_reports_failed_policy_file_digest() {
    let dir = std::env::temp_dir().join(format!(
        "buildlang_receipt_verify_policy_json_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create receipt policy json fixture dir");
    let entry = dir.join("entry.bld");
    let policy = dir.join("policy.json");
    let receipt = dir.join("receipt.json");
    fs::write(&entry, r#"fn main() ~ Console { println!("ok"); }"#)
        .expect("write policy json receipt entry");
    fs::write(
        &policy,
        r#"{
  "schema": "buildlang-check-policy/v1",
  "allowed_effects": ["Console"],
  "require_source_digest": true
}
"#,
    )
    .expect("write initial json policy");

    let check_output = buildc()
        .arg("check")
        .arg(&entry)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg(&receipt)
        .output()
        .expect("write file-backed policy receipt for json verify");
    assert!(
        check_output.status.success(),
        "check should pass before json policy mutation\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check_output.stdout),
        String::from_utf8_lossy(&check_output.stderr)
    );

    fs::write(
        &policy,
        r#"{
  "schema": "buildlang-check-policy/v1",
  "allowed_effects": ["FileSystem"],
  "require_source_digest": true
}
"#,
    )
    .expect("mutate json policy file");
    let verify_output = buildc()
        .args(["receipt", "verify"])
        .arg(&receipt)
        .arg("--json")
        .output()
        .expect("verify stale policy receipt as json");

    let _ = fs::remove_dir_all(&dir);

    assert!(
        !verify_output.status.success(),
        "stale policy json receipt should fail"
    );
    let report: serde_json::Value = serde_json::from_slice(&verify_output.stdout)
        .expect("policy failure report should be JSON");
    assert_eq!(report["status"], "failed");
    let policy_check = verification_check(&report, "policy_source_digest");
    assert_eq!(policy_check["status"], "failed");
    assert!(
        policy_check["message"]
            .as_str()
            .expect("policy failure message")
            .contains("policy source digest mismatch"),
        "policy failure should explain mismatch: {policy_check:#?}"
    );
}

#[test]
fn receipt_verify_expect_policy_digest_rejects_stripped_policy() {
    let dir = std::env::temp_dir().join(format!(
        "buildlang_receipt_verify_expect_policy_stripped_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create expect policy digest fixture dir");
    let entry = dir.join("entry.bld");
    let policy = dir.join("policy.json");
    let receipt = dir.join("receipt.json");
    fs::write(&entry, r#"fn main() ~ Console { println!("ok"); }"#)
        .expect("write expect policy digest entry");
    fs::write(
        &policy,
        r#"{
  "schema": "buildlang-check-policy/v1",
  "allowed_effects": ["Console"],
  "require_source_digest": true
}
"#,
    )
    .expect("write expected policy");

    let check_output = buildc()
        .arg("check")
        .arg(&entry)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg(&receipt)
        .output()
        .expect("write policy-backed receipt");
    assert!(
        check_output.status.success(),
        "check should pass before stripping policy\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check_output.stdout),
        String::from_utf8_lossy(&check_output.stderr)
    );

    let mut saved: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipt).expect("read policy receipt"))
            .expect("policy receipt should parse");
    let expected_digest = saved["policy"]["source_digest"]["hex"]
        .as_str()
        .expect("policy digest")
        .to_string();
    saved
        .as_object_mut()
        .expect("receipt should be an object")
        .remove("policy");
    fs::write(
        &receipt,
        serde_json::to_string_pretty(&saved).expect("serialize stripped policy receipt"),
    )
    .expect("write stripped policy receipt");

    let verify_output = buildc()
        .args(["receipt", "verify"])
        .arg(&receipt)
        .arg("--expect-policy-digest")
        .arg(format!("sha256:{expected_digest}"))
        .output()
        .expect("verify stripped policy with expected digest");

    let _ = fs::remove_dir_all(&dir);

    assert!(
        !verify_output.status.success(),
        "stripped policy receipt should fail policy digest expectation"
    );
    assert!(
        String::from_utf8_lossy(&verify_output.stderr).contains("receipt policy digest mismatch"),
        "stderr should report expected policy digest mismatch:\n{}",
        String::from_utf8_lossy(&verify_output.stderr)
    );
}

#[test]
fn receipt_verify_expect_policy_digest_rejects_algorithm_tamper() {
    let dir = std::env::temp_dir().join(format!(
        "buildlang_receipt_verify_expect_policy_algorithm_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create expect policy digest algorithm fixture dir");
    let entry = dir.join("entry.bld");
    let policy = dir.join("policy.json");
    let receipt = dir.join("receipt.json");
    fs::write(&entry, r#"fn main() ~ Console { println!("ok"); }"#)
        .expect("write expect policy digest algorithm entry");
    fs::write(
        &policy,
        r#"{
  "schema": "buildlang-check-policy/v1",
  "allowed_effects": ["Console"],
  "require_source_digest": true
}
"#,
    )
    .expect("write expected algorithm policy");

    let check_output = buildc()
        .arg("check")
        .arg(&entry)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg(&receipt)
        .output()
        .expect("write policy-backed receipt for algorithm tamper");
    assert!(
        check_output.status.success(),
        "check should pass before tampering policy digest algorithm\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check_output.stdout),
        String::from_utf8_lossy(&check_output.stderr)
    );

    let mut saved: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipt).expect("read policy algorithm receipt"))
            .expect("policy algorithm receipt should parse");
    let expected_digest = saved["policy"]["source_digest"]["hex"]
        .as_str()
        .expect("policy digest")
        .to_string();
    saved["policy"]
        .as_object_mut()
        .expect("policy should be an object")
        .remove("source");
    saved["policy"]["source_digest"]["algorithm"] = serde_json::Value::String("sha512".into());
    fs::write(
        &receipt,
        serde_json::to_string_pretty(&saved).expect("serialize algorithm-tampered receipt"),
    )
    .expect("write algorithm-tampered policy receipt");

    let verify_output = buildc()
        .args(["receipt", "verify"])
        .arg(&receipt)
        .arg("--expect-policy-digest")
        .arg(format!("sha256:{expected_digest}"))
        .arg("--json")
        .output()
        .expect("verify algorithm-tampered policy digest");

    let _ = fs::remove_dir_all(&dir);

    assert!(
        !verify_output.status.success(),
        "algorithm-tampered policy digest receipt should fail policy digest expectation"
    );
    let report: serde_json::Value = serde_json::from_slice(&verify_output.stdout)
        .expect("algorithm-tampered policy digest report should be JSON");
    let policy_check = verification_check(&report, "expected_policy_digest");
    assert_eq!(policy_check["status"], "failed");
    assert_eq!(
        policy_check["expected"],
        format!("sha256:{expected_digest}")
    );
    assert_eq!(policy_check["actual"], format!("sha512:{expected_digest}"));
}

#[test]
fn receipt_verify_json_reports_expected_policy_digest_mismatch() {
    let dir = std::env::temp_dir().join(format!(
        "buildlang_receipt_verify_expect_policy_json_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create expect policy digest json fixture dir");
    let entry = dir.join("entry.bld");
    let policy = dir.join("policy.json");
    let receipt = dir.join("receipt.json");
    fs::write(&entry, r#"fn main() ~ Console { println!("ok"); }"#)
        .expect("write expect policy digest json entry");
    fs::write(
        &policy,
        r#"{
  "schema": "buildlang-check-policy/v1",
  "allowed_effects": ["Console"],
  "require_source_digest": true
}
"#,
    )
    .expect("write expected json policy");

    let check_output = buildc()
        .arg("check")
        .arg(&entry)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg(&receipt)
        .output()
        .expect("write policy-backed receipt for json expectation");
    assert!(
        check_output.status.success(),
        "check should pass before policy digest mismatch\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check_output.stdout),
        String::from_utf8_lossy(&check_output.stderr)
    );
    let saved: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipt).expect("read policy digest receipt"))
            .expect("policy digest receipt should parse");
    let actual_digest = saved["policy"]["source_digest"]["hex"]
        .as_str()
        .expect("policy digest")
        .to_string();
    let wrong_digest = "0".repeat(64);

    let verify_output = buildc()
        .args(["receipt", "verify"])
        .arg(&receipt)
        .arg("--expect-policy-digest")
        .arg(format!("sha256:{wrong_digest}"))
        .arg("--json")
        .output()
        .expect("verify policy digest mismatch as json");

    let _ = fs::remove_dir_all(&dir);

    assert!(
        !verify_output.status.success(),
        "mismatched policy digest receipt should fail json verification"
    );
    let report: serde_json::Value = serde_json::from_slice(&verify_output.stdout)
        .expect("policy digest mismatch report should be JSON");
    assert_eq!(report["status"], "failed");
    let policy_check = verification_check(&report, "expected_policy_digest");
    assert_eq!(policy_check["status"], "failed");
    assert_eq!(policy_check["expected"], format!("sha256:{wrong_digest}"));
    assert_eq!(policy_check["actual"], format!("sha256:{actual_digest}"));
    assert!(
        policy_check["message"]
            .as_str()
            .expect("expected policy digest failure message")
            .contains("receipt policy digest mismatch"),
        "policy digest failure should explain mismatch: {policy_check:#?}"
    );
}

#[test]
fn receipt_verify_rejects_tampered_observed_capabilities() {
    let dir = std::env::temp_dir().join(format!(
        "buildlang_receipt_verify_tampered_capabilities_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create tampered receipt fixture dir");
    let entry = dir.join("entry.bld");
    let receipt = dir.join("receipt.json");
    fs::write(&entry, r#"fn main() ~ Console { println!("ok"); }"#)
        .expect("write tampered receipt entry");

    let check_output = buildc()
        .arg("check")
        .arg(&entry)
        .arg("--receipt")
        .arg(&receipt)
        .output()
        .expect("write receipt before capability tamper");
    assert!(
        check_output.status.success(),
        "check should pass before receipt tamper\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check_output.stdout),
        String::from_utf8_lossy(&check_output.stderr)
    );

    let mut saved: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipt).expect("read saved receipt"))
            .expect("saved receipt should parse");
    saved["observed_capabilities"]["main"]["Console"] = serde_json::json!(["forged_console"]);
    fs::write(
        &receipt,
        serde_json::to_string_pretty(&saved).expect("serialize tampered receipt"),
    )
    .expect("write tampered receipt");

    let verify_output = buildc()
        .args(["receipt", "verify"])
        .arg(&receipt)
        .output()
        .expect("verify tampered receipt");

    let _ = fs::remove_dir_all(&dir);

    assert!(
        !verify_output.status.success(),
        "tampered capability receipt should fail verification"
    );
    assert!(
        String::from_utf8_lossy(&verify_output.stderr)
            .contains("receipt observed_capabilities mismatch"),
        "stderr should report observed capability mismatch:\n{}",
        String::from_utf8_lossy(&verify_output.stderr)
    );
}

#[test]
fn receipt_verify_json_reports_tampered_propagated_effects() {
    let dir = std::env::temp_dir().join(format!(
        "buildlang_receipt_verify_tampered_propagated_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create tampered propagated receipt fixture dir");
    let entry = dir.join("entry.bld");
    let receipt = dir.join("receipt.json");
    fs::write(
        &entry,
        r#"
fn load_config() ~ FileSystem {
    read_file("ops.txt");
}

fn main() ~ FileSystem {
    load_config();
}
"#,
    )
    .expect("write propagated receipt entry");

    let check_output = buildc()
        .arg("check")
        .arg(&entry)
        .arg("--receipt")
        .arg(&receipt)
        .output()
        .expect("write receipt before propagated tamper");
    assert!(
        check_output.status.success(),
        "check should pass before propagated tamper\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check_output.stdout),
        String::from_utf8_lossy(&check_output.stderr)
    );

    let mut saved: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipt).expect("read saved propagated receipt"))
            .expect("saved propagated receipt should parse");
    saved["propagated_effects"]["main"]["FileSystem"] = serde_json::json!(["forged_boundary"]);
    fs::write(
        &receipt,
        serde_json::to_string_pretty(&saved).expect("serialize tampered propagated receipt"),
    )
    .expect("write tampered propagated receipt");

    let verify_output = buildc()
        .args(["receipt", "verify"])
        .arg(&receipt)
        .arg("--json")
        .output()
        .expect("verify tampered propagated receipt as json");

    let _ = fs::remove_dir_all(&dir);

    assert!(
        !verify_output.status.success(),
        "tampered propagated receipt should fail verification"
    );
    let report: serde_json::Value = serde_json::from_slice(&verify_output.stdout)
        .expect("tampered propagated failure report should be JSON");
    assert_eq!(report["status"], "failed");
    let replay_check = verification_check(&report, "propagated_effects");
    assert_eq!(replay_check["status"], "failed");
    assert!(
        replay_check["message"]
            .as_str()
            .expect("propagated replay failure message")
            .contains("receipt propagated_effects mismatch"),
        "propagated failure should explain mismatch: {replay_check:#?}"
    );
}

#[test]
fn receipt_verify_rejects_changed_input_graph() {
    let dir = std::env::temp_dir().join(format!(
        "buildlang_receipt_verify_graph_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create receipt graph fixture dir");
    let entry = dir.join("entry.bld");
    let shared = dir.join("shared.bld");
    let receipt = dir.join("receipt.json");
    fs::write(
        &entry,
        r#"include!("shared.bld");
fn main() ~ Console { println!("{}", value()); }
"#,
    )
    .expect("write receipt graph entry");
    fs::write(&shared, "fn value() -> i32 { 7 }\n").expect("write first shared source");

    let check_output = buildc()
        .arg("check")
        .arg(&entry)
        .arg("--receipt")
        .arg(&receipt)
        .output()
        .expect("write graph receipt");
    assert!(
        check_output.status.success(),
        "check should pass before graph mutation\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check_output.stdout),
        String::from_utf8_lossy(&check_output.stderr)
    );

    fs::write(&shared, "fn value() -> i32 { 8 }\n").expect("mutate shared source");
    let verify_output = buildc()
        .args(["receipt", "verify"])
        .arg(&receipt)
        .output()
        .expect("verify stale graph receipt");

    let _ = fs::remove_dir_all(&dir);

    assert!(
        !verify_output.status.success(),
        "stale input graph receipt should fail"
    );
    assert!(
        String::from_utf8_lossy(&verify_output.stderr).contains("input graph digest mismatch"),
        "stderr should report graph mismatch:\n{}",
        String::from_utf8_lossy(&verify_output.stderr)
    );
}

#[test]
fn receipt_verify_rejects_tampered_builtin_profile_digest() {
    let dir = std::env::temp_dir().join(format!(
        "buildlang_receipt_verify_profile_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create receipt profile fixture dir");
    let entry = dir.join("entry.bld");
    let receipt_path = dir.join("receipt.json");
    fs::write(&entry, r#"fn main() {}"#).expect("write pure entry");

    let check_output = buildc()
        .arg("check")
        .arg(&entry)
        .arg("--profile")
        .arg("pure")
        .arg("--receipt")
        .arg(&receipt_path)
        .output()
        .expect("write built-in profile receipt");
    assert!(
        check_output.status.success(),
        "check should pass before profile digest tamper\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check_output.stdout),
        String::from_utf8_lossy(&check_output.stderr)
    );

    let receipt_text = fs::read_to_string(&receipt_path).expect("read receipt json");
    let mut receipt_json: serde_json::Value =
        serde_json::from_str(&receipt_text).expect("receipt should be JSON");
    receipt_json["policy"]["profile_digest"]["hex"] = serde_json::Value::String("0".repeat(64));
    fs::write(
        &receipt_path,
        serde_json::to_string_pretty(&receipt_json).expect("serialize tampered receipt"),
    )
    .expect("write tampered receipt");

    let verify_output = buildc()
        .args(["receipt", "verify"])
        .arg(&receipt_path)
        .output()
        .expect("verify tampered profile receipt");

    let _ = fs::remove_dir_all(&dir);

    assert!(
        !verify_output.status.success(),
        "tampered profile digest receipt should fail"
    );
    let stderr = String::from_utf8_lossy(&verify_output.stderr);
    assert!(
        stderr.contains("built-in policy profile digest mismatch"),
        "stderr should report profile digest mismatch:\n{stderr}"
    );
    assert!(
        stderr.contains("pure"),
        "stderr should name the profile:\n{stderr}"
    );
}

#[test]
fn check_policy_allows_console_receipt() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_policy_console_{}.bld",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "console_allow",
        r#"{
          "schema": "buildlang-check-policy/v1",
          "allowed_effects": ["Console"],
          "require_source_digest": true
        }"#,
    );
    fs::write(&fixture, r#"fn main() ~ Console { println!("ok"); }"#)
        .expect("write policy console fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check with passing policy");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        output.status.success(),
        "console policy check should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["status"], "passed");
    assert_eq!(receipt["policy"]["schema"], "buildlang-check-policy/v1");
    assert_eq!(receipt["policy"]["status"], "passed");
    assert_eq!(receipt["policy"]["source_digest"]["algorithm"], "sha256");
    assert_eq!(
        receipt["policy"]["source_digest"]["hex"]
            .as_str()
            .expect("policy digest")
            .len(),
        64
    );
    assert!(receipt["policy"]["violations"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn check_policy_denies_filesystem_even_when_typecheck_passes() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_policy_deny_fs_{}.bld",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "deny_fs",
        r#"{
          "schema": "buildlang-check-policy/v1",
          "denied_effects": ["FileSystem"]
        }"#,
    );
    fs::write(
        &fixture,
        r#"fn main() ~ FileSystem { read_file("ops.txt"); }"#,
    )
    .expect("write denied filesystem fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check with denied filesystem policy");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(!output.status.success(), "policy denial should fail check");
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["status"], "failed");
    assert_eq!(receipt["policy"]["status"], "failed");
    let violations = receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations");
    assert!(
        violations.iter().any(|violation| {
            violation["kind"] == "DeniedEffect"
                && violation["effect"] == "FileSystem"
                && violation["function"] == "main"
                && violation["message"]
                    .as_str()
                    .unwrap_or("")
                    .contains("policy denies effect `FileSystem`")
        }),
        "expected FileSystem denied violation in {violations:#?}"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Policy violation"),
        "stderr should include policy diagnostic:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_policy_denies_gpu_even_when_typecheck_passes() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_policy_deny_gpu_{}.bld",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "deny_gpu",
        r#"{
          "schema": "buildlang-check-policy/v1",
          "denied_effects": ["Gpu"]
        }"#,
    );
    fs::write(&fixture, r#"fn main() ~ Gpu { build_vk_init(); }"#)
        .expect("write denied gpu fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check with denied gpu policy");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(!output.status.success(), "policy denial should fail check");
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["status"], "failed");
    assert_eq!(receipt["policy"]["status"], "failed");
    let violations = receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations");
    assert!(
        violations.iter().any(|violation| {
            violation["kind"] == "DeniedEffect"
                && violation["effect"] == "Gpu"
                && violation["function"] == "main"
                && violation["source"] == "build_vk_init"
        }),
        "expected Gpu denied violation in {violations:#?}"
    );
}

#[test]
fn check_policy_rejects_unknown_effect_name() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_policy_unknown_effect_{}.bld",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "unknown_effect",
        r#"{
          "schema": "buildlang-check-policy/v1",
          "denied_effects": ["Netwrok"]
        }"#,
    );
    fs::write(&fixture, r#"fn main() {}"#).expect("write unknown policy effect fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check with unknown policy effect");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        !output.status.success(),
        "unknown policy effect should fail check\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["status"], "failed");
    assert_eq!(receipt["policy"]["status"], "failed");
    let violations = receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations");
    assert!(
        violations.iter().any(|violation| {
            violation["kind"] == "UnknownPolicyEffect"
                && violation["effect"] == "Netwrok"
                && violation["surface"] == "denied_effects"
                && violation["message"]
                    .as_str()
                    .unwrap_or("")
                    .contains("unknown effect")
        }),
        "expected unknown policy effect violation in {violations:#?}"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("unknown effect"),
        "stderr should report unknown policy effect:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_policy_allows_source_defined_effect_name() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_policy_user_effect_{}.bld",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "user_effect",
        r#"{
          "schema": "buildlang-check-policy/v1",
          "allowed_effects": ["Audit"]
        }"#,
    );
    fs::write(
        &fixture,
        r#"
effect Audit {
    fn record();
}

fn main() ~ Audit {}
"#,
    )
    .expect("write source-defined effect fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check with source-defined effect policy");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        output.status.success(),
        "source-defined effect should be accepted by policy validator\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["policy"]["status"], "passed");
    assert_eq!(
        receipt["declared_effects"]["main"],
        serde_json::json!(["Audit"])
    );
    assert!(receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations")
        .is_empty());
}

#[test]
fn check_policy_allow_list_rejects_unlisted_effect() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_policy_allow_list_{}.bld",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "allow_console_only",
        r#"{
          "schema": "buildlang-check-policy/v1",
          "allowed_effects": ["Console"]
        }"#,
    );
    fs::write(
        &fixture,
        r#"fn main() ~ FileSystem { read_file("ops.txt"); }"#,
    )
    .expect("write allow-list filesystem fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check with allow-list policy");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        !output.status.success(),
        "unlisted effect should fail policy"
    );
    let receipt = receipt_from_stdout(&output);
    let violations = receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations");
    assert!(
        violations.iter().any(|violation| {
            violation["kind"] == "DisallowedEffect"
                && violation["effect"] == "FileSystem"
                && violation["function"] == "main"
        }),
        "expected FileSystem disallowed violation in {violations:#?}"
    );
}

#[test]
fn check_policy_required_effect_allowlist_rejects_unlisted_declared_effect() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_policy_require_effect_allowlist_{}.bld",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "require_effect_allowlist",
        r#"{
          "schema": "buildlang-check-policy/v1",
          "allowed_effects": [],
          "require_effect_allowlist": true
        }"#,
    );
    fs::write(
        &fixture,
        r#"
effect Audit {
    fn record();
}

fn main() ~ Audit {}
"#,
    )
    .expect("write required effect allowlist fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check with required effect allowlist");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        !output.status.success(),
        "required empty effect allowlist should reject declared effect\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    let violations = receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations");
    assert!(
        violations.iter().any(|violation| {
            violation["kind"] == "DisallowedEffect"
                && violation["effect"] == "Audit"
                && violation["function"] == "main"
        }),
        "expected Audit disallowed violation in {violations:#?}"
    );
}

#[test]
fn check_policy_direct_allowlist_rejects_unapproved_direct_effect() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_policy_direct_allowlist_{}.bld",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "direct_allowlist",
        r#"{
          "schema": "buildlang-check-policy/v1",
          "allowed_effects": ["FileSystem"],
          "direct_effect_allowlist": {
            "FileSystem": ["load_config"]
          }
        }"#,
    );
    fs::write(
        &fixture,
        r#"fn main() ~ FileSystem { read_file("ops.txt"); }"#,
    )
    .expect("write direct allowlist fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check with direct allowlist policy");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        !output.status.success(),
        "policy should reject direct helper\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["policy"]["status"], "failed");
    let violations = receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations");
    assert!(
        violations.iter().any(|violation| {
            violation["kind"] == "DirectEffectNotAllowed"
                && violation["effect"] == "FileSystem"
                && violation["function"] == "main"
                && violation["surface"] == "observed_capabilities"
                && violation["source"] == "read_file"
        }),
        "expected direct effect allowlist violation in {violations:#?}"
    );
}

#[test]
fn check_policy_provenance_allowlists_accept_boundary_and_caller() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_policy_provenance_accept_{}.bld",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "provenance_accept",
        r#"{
          "schema": "buildlang-check-policy/v1",
          "allowed_effects": ["FileSystem"],
          "direct_effect_allowlist": {
            "FileSystem": ["load_config"]
          },
          "propagated_effect_allowlist": {
            "FileSystem": ["main"]
          }
        }"#,
    );
    fs::write(
        &fixture,
        r#"
fn load_config() ~ FileSystem {
    read_file("ops.txt");
}

fn main() ~ FileSystem {
    load_config();
}
"#,
    )
    .expect("write provenance allowlist fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check with provenance allowlists");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        output.status.success(),
        "policy should accept allowlisted provenance\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["policy"]["status"], "passed");
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config"])
    );
}

#[test]
fn check_policy_strict_allowlist_coverage_rejects_unused_direct_entry() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_policy_unused_direct_allowlist_{}.bld",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "unused_direct_allowlist",
        r#"{
          "schema": "buildlang-check-policy/v1",
          "allowed_effects": ["FileSystem"],
          "direct_effect_allowlist": {
            "FileSystem": ["load_config", "legacy_loader"]
          },
          "propagated_effect_allowlist": {
            "FileSystem": ["main"]
          },
          "require_allowlist_coverage": true
        }"#,
    );
    fs::write(
        &fixture,
        r#"
fn load_config() ~ FileSystem {
    read_file("ops.txt");
}

fn main() ~ FileSystem {
    load_config();
}
"#,
    )
    .expect("write unused direct allowlist fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check with strict allowlist coverage");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        !output.status.success(),
        "strict policy should reject unused direct allowlist entry\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["policy"]["status"], "failed");
    let violations = receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations");
    assert!(
        violations.iter().any(|violation| {
            violation["kind"] == "UnusedDirectEffectAllowlist"
                && violation["effect"] == "FileSystem"
                && violation["function"] == "legacy_loader"
                && violation["surface"] == "direct_effect_allowlist"
                && violation["message"]
                    .as_str()
                    .unwrap_or("")
                    .contains("not matched")
        }),
        "expected unused direct allowlist violation in {violations:#?}"
    );
}

#[test]
fn check_policy_strict_allowlist_coverage_rejects_unused_propagated_entry() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_policy_unused_propagated_allowlist_{}.bld",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "unused_propagated_allowlist",
        r#"{
          "schema": "buildlang-check-policy/v1",
          "allowed_effects": ["FileSystem"],
          "direct_effect_allowlist": {
            "FileSystem": ["load_config"]
          },
          "propagated_effect_allowlist": {
            "FileSystem": ["main", "legacy_workflow"]
          },
          "require_allowlist_coverage": true
        }"#,
    );
    fs::write(
        &fixture,
        r#"
fn load_config() ~ FileSystem {
    read_file("ops.txt");
}

fn main() ~ FileSystem {
    load_config();
}
"#,
    )
    .expect("write unused propagated allowlist fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check with strict propagated allowlist coverage");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        !output.status.success(),
        "strict policy should reject unused propagated allowlist entry\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["policy"]["status"], "failed");
    let violations = receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations");
    assert!(
        violations.iter().any(|violation| {
            violation["kind"] == "UnusedPropagatedEffectAllowlist"
                && violation["effect"] == "FileSystem"
                && violation["function"] == "legacy_workflow"
                && violation["surface"] == "propagated_effect_allowlist"
                && violation["message"]
                    .as_str()
                    .unwrap_or("")
                    .contains("not matched")
        }),
        "expected unused propagated allowlist violation in {violations:#?}"
    );
}

#[test]
fn check_policy_strict_allowlist_coverage_accepts_used_entries() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_policy_used_allowlist_{}.bld",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "used_allowlist",
        r#"{
          "schema": "buildlang-check-policy/v1",
          "allowed_effects": ["FileSystem"],
          "direct_effect_allowlist": {
            "FileSystem": ["load_config"]
          },
          "propagated_effect_allowlist": {
            "FileSystem": ["main"]
          },
          "require_allowlist_coverage": true
        }"#,
    );
    fs::write(
        &fixture,
        r#"
fn load_config() ~ FileSystem {
    read_file("ops.txt");
}

fn main() ~ FileSystem {
    load_config();
}
"#,
    )
    .expect("write used allowlist fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check with used strict allowlists");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        output.status.success(),
        "strict policy should accept fully-used allowlist entries\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["policy"]["status"], "passed");
    assert!(receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations")
        .is_empty());
}

#[test]
fn check_policy_requires_direct_provenance_allowlist_entry() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_policy_require_direct_allowlist_{}.bld",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "require_direct_allowlist",
        r#"{
          "schema": "buildlang-check-policy/v1",
          "allowed_effects": ["FileSystem"],
          "require_provenance_allowlists": true
        }"#,
    );
    fs::write(
        &fixture,
        r#"fn main() ~ FileSystem { read_file("ops.txt"); }"#,
    )
    .expect("write required direct allowlist fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check with required direct allowlist");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        !output.status.success(),
        "policy should require explicit direct provenance allowlist entry\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    let violations = receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations");
    assert!(
        violations.iter().any(|violation| {
            violation["kind"] == "DirectEffectNotAllowed"
                && violation["effect"] == "FileSystem"
                && violation["function"] == "main"
                && violation["surface"] == "observed_capabilities"
                && violation["source"] == "read_file"
        }),
        "expected required direct provenance violation in {violations:#?}"
    );
}

#[test]
fn check_policy_requires_propagated_provenance_allowlist_entry() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_policy_require_propagated_allowlist_{}.bld",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "require_propagated_allowlist",
        r#"{
          "schema": "buildlang-check-policy/v1",
          "allowed_effects": ["FileSystem"],
          "direct_effect_allowlist": {
            "FileSystem": ["load_config"]
          },
          "require_provenance_allowlists": true
        }"#,
    );
    fs::write(
        &fixture,
        r#"
fn load_config() ~ FileSystem {
    read_file("ops.txt");
}

fn main() ~ FileSystem {
    load_config();
}
"#,
    )
    .expect("write required propagated allowlist fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check with required propagated allowlist");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        !output.status.success(),
        "policy should require explicit propagated provenance allowlist entry\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    let violations = receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations");
    assert!(
        violations.iter().any(|violation| {
            violation["kind"] == "PropagatedEffectNotAllowed"
                && violation["effect"] == "FileSystem"
                && violation["function"] == "main"
                && violation["surface"] == "propagated_effects"
                && violation["source"] == "load_config"
        }),
        "expected required propagated provenance violation in {violations:#?}"
    );
}

#[test]
fn check_policy_required_provenance_allowlists_accept_explicit_entries() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_policy_required_allowlists_accept_{}.bld",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "required_allowlists_accept",
        r#"{
          "schema": "buildlang-check-policy/v1",
          "allowed_effects": ["FileSystem"],
          "direct_effect_allowlist": {
            "FileSystem": ["load_config"]
          },
          "propagated_effect_allowlist": {
            "FileSystem": ["main"]
          },
          "require_provenance_allowlists": true
        }"#,
    );
    fs::write(
        &fixture,
        r#"
fn load_config() ~ FileSystem {
    read_file("ops.txt");
}

fn main() ~ FileSystem {
    load_config();
}
"#,
    )
    .expect("write required allowlists accept fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check with required explicit allowlists");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        output.status.success(),
        "policy should accept explicit direct and propagated allowlists\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["policy"]["status"], "passed");
    assert!(receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations")
        .is_empty());
}

#[test]
fn check_policy_direct_capability_source_allowlist_rejects_unapproved_source() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_policy_direct_source_reject_{}.bld",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "direct_source_reject",
        r#"{
          "schema": "buildlang-check-policy/v1",
          "allowed_effects": ["FileSystem"],
          "direct_effect_allowlist": {
            "FileSystem": ["load_config"]
          },
          "direct_capability_source_allowlist": {
            "FileSystem": {
              "load_config": ["read_file"]
            }
          }
        }"#,
    );
    fs::write(
        &fixture,
        r#"
fn load_config() ~ FileSystem {
    write_file("ops.txt", "x");
}

fn main() ~ FileSystem {
    load_config();
}
"#,
    )
    .expect("write direct source allowlist rejection fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check with direct source allowlist");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        !output.status.success(),
        "policy should reject unapproved direct capability source\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    let violations = receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations");
    assert!(
        violations.iter().any(|violation| {
            violation["kind"] == "DirectCapabilitySourceNotAllowed"
                && violation["effect"] == "FileSystem"
                && violation["function"] == "load_config"
                && violation["surface"] == "observed_capabilities"
                && violation["source"] == "write_file"
        }),
        "expected direct capability source allowlist violation in {violations:#?}"
    );
}

#[test]
fn check_policy_direct_capability_source_allowlist_accepts_approved_source() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_policy_direct_source_accept_{}.bld",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "direct_source_accept",
        r#"{
          "schema": "buildlang-check-policy/v1",
          "allowed_effects": ["FileSystem"],
          "direct_effect_allowlist": {
            "FileSystem": ["load_config"]
          },
          "direct_capability_source_allowlist": {
            "FileSystem": {
              "load_config": ["read_file"]
            }
          }
        }"#,
    );
    fs::write(
        &fixture,
        r#"
fn load_config() ~ FileSystem {
    read_file("ops.txt");
}

fn main() ~ FileSystem {
    load_config();
}
"#,
    )
    .expect("write direct source allowlist acceptance fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check with approved direct source allowlist");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        output.status.success(),
        "policy should accept approved direct capability source\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["policy"]["status"], "passed");
    assert!(receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations")
        .is_empty());
}

#[test]
fn check_policy_strict_allowlist_coverage_rejects_unused_direct_capability_source_entry() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_policy_unused_direct_source_allowlist_{}.bld",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "unused_direct_source_allowlist",
        r#"{
          "schema": "buildlang-check-policy/v1",
          "allowed_effects": ["FileSystem"],
          "direct_effect_allowlist": {
            "FileSystem": ["load_config"]
          },
          "propagated_effect_allowlist": {
            "FileSystem": ["main"]
          },
          "direct_capability_source_allowlist": {
            "FileSystem": {
              "load_config": ["read_file", "legacy_reader"]
            }
          },
          "require_allowlist_coverage": true
        }"#,
    );
    fs::write(
        &fixture,
        r#"
fn load_config() ~ FileSystem {
    read_file("ops.txt");
}

fn main() ~ FileSystem {
    load_config();
}
"#,
    )
    .expect("write unused direct source allowlist fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check with strict direct source coverage");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        !output.status.success(),
        "strict policy should reject unused direct capability source entry\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    let violations = receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations");
    assert!(
        violations.iter().any(|violation| {
            violation["kind"] == "UnusedDirectCapabilitySourceAllowlist"
                && violation["effect"] == "FileSystem"
                && violation["function"] == "load_config"
                && violation["surface"] == "direct_capability_source_allowlist"
                && violation["source"] == "legacy_reader"
        }),
        "expected unused direct capability source allowlist violation in {violations:#?}"
    );
}

#[test]
fn check_policy_propagated_effect_source_allowlist_rejects_unapproved_callee() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_policy_propagated_source_reject_{}.bld",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "propagated_source_reject",
        r#"{
          "schema": "buildlang-check-policy/v1",
          "allowed_effects": ["FileSystem"],
          "direct_effect_allowlist": {
            "FileSystem": ["load_config", "load_secret"]
          },
          "propagated_effect_allowlist": {
            "FileSystem": ["main"]
          },
          "propagated_effect_source_allowlist": {
            "FileSystem": {
              "main": ["load_config"]
            }
          }
        }"#,
    );
    fs::write(
        &fixture,
        r#"
fn load_config() ~ FileSystem {
    read_file("ops.txt");
}

fn load_secret() ~ FileSystem {
    read_file("secret.txt");
}

fn main() ~ FileSystem {
    load_secret();
}
"#,
    )
    .expect("write propagated source allowlist rejection fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check with propagated source allowlist");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        !output.status.success(),
        "policy should reject unapproved propagated effect source\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    let violations = receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations");
    assert!(
        violations.iter().any(|violation| {
            violation["kind"] == "PropagatedEffectSourceNotAllowed"
                && violation["effect"] == "FileSystem"
                && violation["function"] == "main"
                && violation["surface"] == "propagated_effects"
                && violation["source"] == "load_secret"
        }),
        "expected propagated effect source allowlist violation in {violations:#?}"
    );
}

#[test]
fn check_policy_propagated_effect_source_allowlist_accepts_approved_callee() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_policy_propagated_source_accept_{}.bld",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "propagated_source_accept",
        r#"{
          "schema": "buildlang-check-policy/v1",
          "allowed_effects": ["FileSystem"],
          "direct_effect_allowlist": {
            "FileSystem": ["load_config"]
          },
          "propagated_effect_allowlist": {
            "FileSystem": ["main"]
          },
          "propagated_effect_source_allowlist": {
            "FileSystem": {
              "main": ["load_config"]
            }
          }
        }"#,
    );
    fs::write(
        &fixture,
        r#"
fn load_config() ~ FileSystem {
    read_file("ops.txt");
}

fn main() ~ FileSystem {
    load_config();
}
"#,
    )
    .expect("write propagated source allowlist acceptance fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check with approved propagated source allowlist");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        output.status.success(),
        "policy should accept approved propagated effect source\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["policy"]["status"], "passed");
    assert!(receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations")
        .is_empty());
}

#[test]
fn check_policy_propagated_effect_source_allowlist_accepts_approved_method() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_policy_propagated_method_source_accept_{}.bld",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "propagated_method_source_accept",
        r#"{
          "schema": "buildlang-check-policy/v1",
          "allowed_effects": ["FileSystem"],
          "direct_effect_allowlist": {
            "FileSystem": ["load"]
          },
          "propagated_effect_allowlist": {
            "FileSystem": ["main"]
          },
          "propagated_effect_source_allowlist": {
            "FileSystem": {
              "main": ["Config.load"]
            }
          }
        }"#,
    );
    fs::write(
        &fixture,
        r#"
struct Config;

impl Config {
    fn load(self) ~ FileSystem {
        read_file("ops.txt");
    }
}

fn main() ~ FileSystem {
    let config = Config;
    config.load();
}
"#,
    )
    .expect("write propagated method source allowlist acceptance fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check with approved propagated method source allowlist");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        output.status.success(),
        "policy should accept approved propagated method source\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["Config.load"])
    );
    assert_eq!(receipt["policy"]["status"], "passed");
    assert!(receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations")
        .is_empty());
}

#[test]
fn check_policy_strict_allowlist_coverage_rejects_unused_propagated_effect_source_entry() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_policy_unused_propagated_source_allowlist_{}.bld",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "unused_propagated_source_allowlist",
        r#"{
          "schema": "buildlang-check-policy/v1",
          "allowed_effects": ["FileSystem"],
          "direct_effect_allowlist": {
            "FileSystem": ["load_config"]
          },
          "propagated_effect_allowlist": {
            "FileSystem": ["main"]
          },
          "propagated_effect_source_allowlist": {
            "FileSystem": {
              "main": ["load_config", "legacy_loader"]
            }
          },
          "require_allowlist_coverage": true
        }"#,
    );
    fs::write(
        &fixture,
        r#"
fn load_config() ~ FileSystem {
    read_file("ops.txt");
}

fn main() ~ FileSystem {
    load_config();
}
"#,
    )
    .expect("write unused propagated source allowlist fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check with strict propagated source coverage");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        !output.status.success(),
        "strict policy should reject unused propagated effect source entry\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    let violations = receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations");
    assert!(
        violations.iter().any(|violation| {
            violation["kind"] == "UnusedPropagatedEffectSourceAllowlist"
                && violation["effect"] == "FileSystem"
                && violation["function"] == "main"
                && violation["surface"] == "propagated_effect_source_allowlist"
                && violation["source"] == "legacy_loader"
        }),
        "expected unused propagated effect source allowlist violation in {violations:#?}"
    );
}

#[test]
fn check_policy_require_source_allowlists_rejects_missing_direct_source_entry() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_policy_require_direct_source_{}.bld",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "require_direct_source",
        r#"{
          "schema": "buildlang-check-policy/v1",
          "allowed_effects": ["FileSystem"],
          "direct_effect_allowlist": {
            "FileSystem": ["load_config"]
          },
          "require_source_allowlists": true
        }"#,
    );
    fs::write(
        &fixture,
        r#"
fn load_config() ~ FileSystem {
    read_file("ops.txt");
}
"#,
    )
    .expect("write required direct source allowlist fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check with required direct source allowlist");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        !output.status.success(),
        "policy should require direct capability source allowlist entry\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    let violations = receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations");
    assert!(
        violations.iter().any(|violation| {
            violation["kind"] == "DirectCapabilitySourceNotAllowed"
                && violation["effect"] == "FileSystem"
                && violation["function"] == "load_config"
                && violation["surface"] == "observed_capabilities"
                && violation["source"] == "read_file"
        }),
        "expected required direct source allowlist violation in {violations:#?}"
    );
}

#[test]
fn check_policy_require_source_allowlists_rejects_missing_propagated_source_entry() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_policy_require_propagated_source_{}.bld",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "require_propagated_source",
        r#"{
          "schema": "buildlang-check-policy/v1",
          "allowed_effects": ["FileSystem"],
          "direct_effect_allowlist": {
            "FileSystem": ["load_config"]
          },
          "direct_capability_source_allowlist": {
            "FileSystem": {
              "load_config": ["read_file"]
            }
          },
          "propagated_effect_allowlist": {
            "FileSystem": ["main"]
          },
          "require_source_allowlists": true
        }"#,
    );
    fs::write(
        &fixture,
        r#"
fn load_config() ~ FileSystem {
    read_file("ops.txt");
}

fn main() ~ FileSystem {
    load_config();
}
"#,
    )
    .expect("write required propagated source allowlist fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check with required propagated source allowlist");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        !output.status.success(),
        "policy should require propagated effect source allowlist entry\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    let violations = receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations");
    assert!(
        violations.iter().any(|violation| {
            violation["kind"] == "PropagatedEffectSourceNotAllowed"
                && violation["effect"] == "FileSystem"
                && violation["function"] == "main"
                && violation["surface"] == "propagated_effects"
                && violation["source"] == "load_config"
        }),
        "expected required propagated source allowlist violation in {violations:#?}"
    );
}

#[test]
fn check_policy_require_source_allowlists_accepts_explicit_source_entries() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_policy_require_sources_accept_{}.bld",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "require_sources_accept",
        r#"{
          "schema": "buildlang-check-policy/v1",
          "allowed_effects": ["FileSystem"],
          "direct_effect_allowlist": {
            "FileSystem": ["load_config"]
          },
          "direct_capability_source_allowlist": {
            "FileSystem": {
              "load_config": ["read_file"]
            }
          },
          "propagated_effect_allowlist": {
            "FileSystem": ["main"]
          },
          "propagated_effect_source_allowlist": {
            "FileSystem": {
              "main": ["load_config"]
            }
          },
          "require_source_allowlists": true
        }"#,
    );
    fs::write(
        &fixture,
        r#"
fn load_config() ~ FileSystem {
    read_file("ops.txt");
}

fn main() ~ FileSystem {
    load_config();
}
"#,
    )
    .expect("write required source allowlists acceptance fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check with required explicit source allowlists");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        output.status.success(),
        "policy should accept explicit source allowlist entries\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["policy"]["status"], "passed");
    assert!(receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations")
        .is_empty());
}

#[test]
fn check_policy_propagated_allowlist_rejects_unlisted_caller() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_policy_propagated_reject_{}.bld",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "propagated_reject",
        r#"{
          "schema": "buildlang-check-policy/v1",
          "allowed_effects": ["FileSystem"],
          "direct_effect_allowlist": {
            "FileSystem": ["load_config"]
          },
          "propagated_effect_allowlist": {
            "FileSystem": []
          }
        }"#,
    );
    fs::write(
        &fixture,
        r#"
fn load_config() ~ FileSystem {
    read_file("ops.txt");
}

fn main() ~ FileSystem {
    load_config();
}
"#,
    )
    .expect("write propagated allowlist fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check with propagated allowlist policy");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        !output.status.success(),
        "policy should reject propagated caller\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["policy"]["status"], "failed");
    let violations = receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations");
    assert!(
        violations.iter().any(|violation| {
            violation["kind"] == "PropagatedEffectNotAllowed"
                && violation["effect"] == "FileSystem"
                && violation["function"] == "main"
                && violation["surface"] == "propagated_effects"
                && violation["source"] == "load_config"
        }),
        "expected propagated effect allowlist violation in {violations:#?}"
    );
}

#[test]
fn check_policy_rejects_unsupported_schema() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_policy_bad_schema_{}.bld",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "bad_schema",
        r#"{
          "schema": "buildlang-check-policy/v0",
          "allowed_effects": ["Console"]
        }"#,
    );
    fs::write(&fixture, r#"fn main() ~ Console { println!("ok"); }"#)
        .expect("write bad schema fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .output()
        .expect("run buildc check with bad policy schema");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        !output.status.success(),
        "unsupported policy schema should fail"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("Unsupported check policy schema 'buildlang-check-policy/v0'"),
        "stderr should report unsupported schema:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn policy_list_includes_builtin_security_profiles() {
    let output = buildc()
        .args(["policy", "list"])
        .output()
        .expect("run buildc policy list");

    assert!(
        output.status.success(),
        "policy list should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("pure"), "missing pure profile:\n{stdout}");
    assert!(
        stdout.contains("console-only"),
        "missing console-only profile:\n{stdout}"
    );
    assert!(
        stdout.contains("offline"),
        "missing offline profile:\n{stdout}"
    );
    assert!(
        stdout.contains("ci-review"),
        "missing ci-review profile:\n{stdout}"
    );
    assert!(
        stdout.contains("strict-accountability"),
        "missing strict-accountability profile:\n{stdout}"
    );
}

#[test]
fn policy_list_json_emits_catalog_with_profile_digests() {
    let output = buildc()
        .args(["policy", "list", "--json"])
        .output()
        .expect("run buildc policy list --json");

    assert!(
        output.status.success(),
        "policy list --json should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let catalog: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("catalog should be JSON");
    assert_eq!(catalog["schema"], "buildlang-policy-catalog/v1");
    let profiles = catalog["profiles"]
        .as_array()
        .expect("profiles should be an array");
    let names = profiles
        .iter()
        .map(|profile| profile["name"].as_str().unwrap_or(""))
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec![
            "pure",
            "console-only",
            "offline",
            "ci-review",
            "strict-accountability"
        ]
    );
    for profile in profiles {
        assert_eq!(
            profile["policy_schema"], "buildlang-check-policy/v1",
            "profile should name the policy schema: {profile:#?}"
        );
        assert!(
            profile["summary"]
                .as_str()
                .is_some_and(|summary| !summary.is_empty()),
            "profile should include a summary: {profile:#?}"
        );
        assert_eq!(profile["digest"]["algorithm"], "sha256");
        assert_eq!(
            profile["digest"]["hex"]
                .as_str()
                .expect("profile digest hex")
                .len(),
            64
        );
    }
}

#[test]
fn policy_list_json_digest_matches_printed_profile() {
    let dir = std::env::temp_dir().join(format!(
        "buildlang_policy_catalog_digest_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create policy catalog digest directory");
    let profile_path = dir.join("ci-review.json");

    let catalog_output = buildc()
        .args(["policy", "list", "--json"])
        .output()
        .expect("run buildc policy list --json");
    let print_output = buildc()
        .args(["policy", "print", "ci-review", "--output"])
        .arg(&profile_path)
        .output()
        .expect("write ci-review policy");

    assert!(
        catalog_output.status.success(),
        "policy catalog should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&catalog_output.stdout),
        String::from_utf8_lossy(&catalog_output.stderr)
    );
    assert!(
        print_output.status.success(),
        "policy print should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&print_output.stdout),
        String::from_utf8_lossy(&print_output.stderr)
    );

    let catalog: serde_json::Value =
        serde_json::from_slice(&catalog_output.stdout).expect("catalog should be JSON");
    let ci_review_digest = catalog["profiles"]
        .as_array()
        .expect("profiles should be an array")
        .iter()
        .find(|profile| profile["name"] == "ci-review")
        .expect("ci-review profile")["digest"]
        .clone();
    let printed_text = fs::read(&profile_path).expect("read printed policy");
    let expected_hex = sha256_hex(&printed_text);

    assert_eq!(ci_review_digest["algorithm"], "sha256");
    assert_eq!(ci_review_digest["hex"], expected_hex);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn policy_print_emits_valid_pure_profile() {
    let output = buildc()
        .args(["policy", "print", "pure"])
        .output()
        .expect("run buildc policy print pure");

    assert!(
        output.status.success(),
        "policy print should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let profile: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("policy profile should be JSON");
    assert_eq!(profile["schema"], "buildlang-check-policy/v1");
    assert_eq!(profile["require_source_digest"], true);
    assert_eq!(profile["require_input_graph_digest"], true);
    let denied = profile["denied_effects"]
        .as_array()
        .expect("denied_effects should be an array");
    assert!(
        denied.iter().any(|effect| effect == "Network"),
        "pure profile should deny Network: {denied:#?}"
    );
    assert!(
        denied.iter().any(|effect| effect == "Foreign"),
        "pure profile should deny Foreign: {denied:#?}"
    );
}

#[test]
fn policy_print_emits_strict_accountability_profile_with_source_gates() {
    let output = buildc()
        .args(["policy", "print", "strict-accountability"])
        .output()
        .expect("run buildc policy print strict-accountability");

    assert!(
        output.status.success(),
        "policy print should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let profile: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("policy profile should be JSON");
    assert_eq!(profile["schema"], "buildlang-check-policy/v1");
    assert_eq!(profile["require_source_digest"], true);
    assert_eq!(profile["require_input_graph_digest"], true);
    assert_eq!(profile["require_effect_allowlist"], true);
    assert_eq!(profile["require_provenance_allowlists"], true);
    assert_eq!(profile["require_source_allowlists"], true);
    assert_eq!(profile["require_allowlist_coverage"], true);
    let denied = profile["denied_effects"]
        .as_array()
        .expect("denied_effects should be an array");
    for effect in ["Network", "Process", "Foreign", "Gpu"] {
        assert!(
            denied.iter().any(|denied| denied == effect),
            "strict-accountability profile should deny {effect}: {denied:#?}"
        );
    }
}

#[test]
fn policy_scaffold_from_receipt_emits_exact_source_allowlists() {
    let dir = std::env::temp_dir().join(format!(
        "buildlang_policy_scaffold_receipt_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create policy scaffold fixture directory");
    let input = dir.join("app.bld");
    let receipt = dir.join("receipt.json");
    let policy = dir.join("policy.json");
    fs::write(
        &input,
        r#"
fn load_config() ~ FileSystem {
    read_file("ops.txt");
}

fn main() ~ FileSystem {
    load_config();
}
"#,
    )
    .expect("write policy scaffold input");

    let check = buildc()
        .arg("check")
        .arg(&input)
        .arg("--receipt")
        .arg(&receipt)
        .output()
        .expect("write policy scaffold receipt");
    assert!(
        check.status.success(),
        "check should produce a receipt\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    let scaffold = buildc()
        .arg("policy")
        .arg("scaffold")
        .arg(&receipt)
        .arg("--output")
        .arg(&policy)
        .output()
        .expect("scaffold policy from receipt");
    assert!(
        scaffold.status.success(),
        "policy scaffold should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&scaffold.stdout),
        String::from_utf8_lossy(&scaffold.stderr)
    );

    let scaffolded: serde_json::Value =
        serde_json::from_slice(&fs::read(&policy).expect("read scaffolded policy"))
            .expect("scaffolded policy should be JSON");
    assert_eq!(scaffolded["schema"], "buildlang-check-policy/v1");
    assert_eq!(
        scaffolded["allowed_effects"],
        serde_json::json!(["FileSystem"])
    );
    assert_eq!(
        scaffolded["direct_effect_allowlist"]["FileSystem"],
        serde_json::json!(["load_config"])
    );
    assert_eq!(
        scaffolded["direct_capability_source_allowlist"]["FileSystem"]["load_config"],
        serde_json::json!(["read_file"])
    );
    assert_eq!(
        scaffolded["propagated_effect_allowlist"]["FileSystem"],
        serde_json::json!(["main"])
    );
    assert_eq!(
        scaffolded["propagated_effect_source_allowlist"]["FileSystem"]["main"],
        serde_json::json!(["load_config"])
    );
    assert_eq!(scaffolded["require_source_digest"], true);
    assert_eq!(scaffolded["require_input_graph_digest"], true);
    assert_eq!(scaffolded["require_effect_allowlist"], true);
    assert_eq!(scaffolded["require_provenance_allowlists"], true);
    assert_eq!(scaffolded["require_source_allowlists"], true);
    assert_eq!(scaffolded["require_allowlist_coverage"], true);

    let verify_scaffold = buildc()
        .arg("check")
        .arg(&input)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("check source with scaffolded policy");
    assert!(
        verify_scaffold.status.success(),
        "scaffolded policy should accept the exact receipt evidence\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verify_scaffold.stdout),
        String::from_utf8_lossy(&verify_scaffold.stderr)
    );
    let verified_receipt = receipt_from_stdout(&verify_scaffold);
    assert_eq!(verified_receipt["policy"]["status"], "passed");
    assert!(verified_receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations")
        .is_empty());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn policy_scaffold_from_pure_receipt_rejects_later_effect_drift() {
    let dir = std::env::temp_dir().join(format!(
        "buildlang_policy_scaffold_pure_drift_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create pure scaffold drift fixture directory");
    let input = dir.join("app.bld");
    let receipt = dir.join("receipt.json");
    let policy = dir.join("policy.json");
    fs::write(&input, "fn main() {}\n").expect("write pure policy scaffold input");

    let check = buildc()
        .arg("check")
        .arg(&input)
        .arg("--receipt")
        .arg(&receipt)
        .output()
        .expect("write pure policy scaffold receipt");
    assert!(
        check.status.success(),
        "pure check should produce a receipt\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    let scaffold = buildc()
        .arg("policy")
        .arg("scaffold")
        .arg(&receipt)
        .arg("--output")
        .arg(&policy)
        .output()
        .expect("scaffold policy from pure receipt");
    assert!(
        scaffold.status.success(),
        "pure policy scaffold should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&scaffold.stdout),
        String::from_utf8_lossy(&scaffold.stderr)
    );
    let scaffolded: serde_json::Value =
        serde_json::from_slice(&fs::read(&policy).expect("read pure scaffolded policy"))
            .expect("pure scaffolded policy should be JSON");
    assert_eq!(scaffolded["allowed_effects"], serde_json::json!([]));
    assert_eq!(scaffolded["require_effect_allowlist"], true);

    fs::write(
        &input,
        r#"
effect Audit {
    fn record();
}

fn main() ~ Audit {}
"#,
    )
    .expect("write drifted effect input");
    let drift = buildc()
        .arg("check")
        .arg(&input)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("check drifted source with pure scaffolded policy");
    assert!(
        !drift.status.success(),
        "pure scaffolded policy should reject later declared effect drift\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&drift.stdout),
        String::from_utf8_lossy(&drift.stderr)
    );
    let receipt = receipt_from_stdout(&drift);
    let violations = receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations");
    assert!(
        violations.iter().any(|violation| {
            violation["kind"] == "DisallowedEffect"
                && violation["effect"] == "Audit"
                && violation["function"] == "main"
        }),
        "expected declared effect drift violation in {violations:#?}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn policy_scaffold_from_foreign_receipt_preserves_direct_ffi_boundary() {
    let dir = std::env::temp_dir().join(format!(
        "buildlang_policy_scaffold_foreign_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create foreign scaffold fixture directory");
    let input = dir.join("app.bld");
    let receipt = dir.join("receipt.json");
    fs::write(
        &input,
        r#"
extern "C" { fn touch(); }

fn main() ~ Foreign {
    touch();
}
"#,
    )
    .expect("write foreign policy scaffold input");

    let check = buildc()
        .arg("check")
        .arg(&input)
        .arg("--receipt")
        .arg(&receipt)
        .output()
        .expect("write foreign policy scaffold receipt");
    assert!(
        check.status.success(),
        "foreign check should produce a receipt\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    let scaffold = buildc()
        .arg("policy")
        .arg("scaffold")
        .arg(&receipt)
        .output()
        .expect("scaffold policy from foreign receipt");
    assert!(
        scaffold.status.success(),
        "foreign policy scaffold should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&scaffold.stdout),
        String::from_utf8_lossy(&scaffold.stderr)
    );
    let scaffolded: serde_json::Value =
        serde_json::from_slice(&scaffold.stdout).expect("foreign scaffold should be JSON");
    assert_eq!(
        scaffolded["allowed_effects"],
        serde_json::json!(["Foreign"])
    );
    assert_eq!(
        scaffolded["direct_effect_allowlist"]["Foreign"],
        serde_json::json!(["main"])
    );
    assert_eq!(
        scaffolded["direct_capability_source_allowlist"]["Foreign"]["main"],
        serde_json::json!(["touch"])
    );
    assert!(
        scaffolded["propagated_effect_allowlist"]
            .as_object()
            .expect("propagated effect allowlist")
            .is_empty(),
        "direct FFI boundary should not be scaffolded as propagated: {scaffolded:#?}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn policy_scaffold_from_qualified_capability_receipt_preserves_source_path() {
    let dir = std::env::temp_dir().join(format!(
        "buildlang_policy_scaffold_qualified_capability_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create qualified scaffold fixture directory");
    let input = dir.join("app.bld");
    let receipt = dir.join("receipt.json");
    fs::write(
        &input,
        r#"fn main() ~ FileSystem { io::read_file("ops.txt"); }"#,
    )
    .expect("write qualified policy scaffold input");

    let check = buildc()
        .arg("check")
        .arg(&input)
        .arg("--receipt")
        .arg(&receipt)
        .output()
        .expect("write qualified policy scaffold receipt");
    assert!(
        check.status.success(),
        "qualified check should produce a receipt\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    let scaffold = buildc()
        .arg("policy")
        .arg("scaffold")
        .arg(&receipt)
        .output()
        .expect("scaffold policy from qualified receipt");
    assert!(
        scaffold.status.success(),
        "qualified policy scaffold should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&scaffold.stdout),
        String::from_utf8_lossy(&scaffold.stderr)
    );
    let scaffolded: serde_json::Value =
        serde_json::from_slice(&scaffold.stdout).expect("qualified scaffold should be JSON");
    assert_eq!(
        scaffolded["direct_effect_allowlist"]["FileSystem"],
        serde_json::json!(["main"])
    );
    assert_eq!(
        scaffolded["direct_capability_source_allowlist"]["FileSystem"]["main"],
        serde_json::json!(["io::read_file"])
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn printed_pure_policy_rejects_console_program() {
    let dir = std::env::temp_dir().join(format!(
        "buildlang_printed_pure_policy_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create pure policy fixture directory");
    let policy_path = dir.join("pure-policy.json");
    let fixture = dir.join("console.bld");

    let print = buildc()
        .args(["policy", "print", "pure", "--output"])
        .arg(&policy_path)
        .output()
        .expect("write pure policy");
    assert!(
        print.status.success(),
        "policy print --output should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&print.stdout),
        String::from_utf8_lossy(&print.stderr)
    );

    fs::write(&fixture, r#"fn main() ~ Console { println!("blocked"); }"#)
        .expect("write console fixture");
    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy_path)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check with pure policy");

    assert!(
        !output.status.success(),
        "pure policy should reject console program\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["policy"]["status"], "failed");
    let violations = receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations");
    assert!(
        violations.iter().any(|violation| {
            violation["kind"] == "DeniedEffect"
                && violation["effect"] == "Console"
                && violation["function"] == "main"
        }),
        "expected Console denial in {violations:#?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn check_profile_strict_accountability_rejects_ambient_console_without_allowlists() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_profile_strict_accountability_{}.bld",
        std::process::id()
    ));
    fs::write(&fixture, r#"fn main() ~ Console { println!("blocked"); }"#)
        .expect("write console fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--profile")
        .arg("strict-accountability")
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check with strict-accountability profile");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "strict-accountability profile should reject console without allowlists\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["policy"]["source"], "builtin:strict-accountability");
    assert_eq!(receipt["policy"]["profile"], "strict-accountability");
    assert_eq!(receipt["policy"]["status"], "failed");
    let violations = receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations");
    assert!(
        violations.iter().any(|violation| {
            violation["kind"] == "DisallowedEffect"
                && violation["effect"] == "Console"
                && violation["function"] == "main"
        }),
        "expected strict Console effect allowlist denial in {violations:#?}"
    );
}

#[test]
fn check_profile_pure_rejects_console_program_without_policy_file() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_profile_pure_{}.bld",
        std::process::id()
    ));
    fs::write(&fixture, r#"fn main() ~ Console { println!("blocked"); }"#)
        .expect("write console fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--profile")
        .arg("pure")
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check with pure profile");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "pure profile should reject console program\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["policy"]["source"], "builtin:pure");
    assert_eq!(receipt["policy"]["profile"], "pure");
    assert_eq!(receipt["policy"]["profile_digest"]["algorithm"], "sha256");
    assert_eq!(
        receipt["policy"]["profile_digest"]["hex"]
            .as_str()
            .expect("profile digest")
            .len(),
        64
    );
    assert_eq!(receipt["policy"]["status"], "failed");
    let violations = receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations");
    assert!(
        violations.iter().any(|violation| {
            violation["kind"] == "DeniedEffect"
                && violation["effect"] == "Console"
                && violation["function"] == "main"
        }),
        "expected Console denial in {violations:#?}"
    );
}

#[test]
fn check_profile_receipt_digest_matches_printed_builtin_profile() {
    let dir = std::env::temp_dir().join(format!("buildlang_profile_digest_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create profile digest fixture directory");
    let profile_path = dir.join("pure.json");
    let input = dir.join("pure.bld");

    let print = buildc()
        .args(["policy", "print", "pure", "--output"])
        .arg(&profile_path)
        .output()
        .expect("write pure profile");
    assert!(
        print.status.success(),
        "policy print should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&print.stdout),
        String::from_utf8_lossy(&print.stderr)
    );
    fs::write(&input, r#"fn main() {}"#).expect("write pure input");

    let via_profile = buildc()
        .arg("check")
        .arg(&input)
        .arg("--profile")
        .arg("pure")
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run check with built-in profile");
    let via_policy = buildc()
        .arg("check")
        .arg(&input)
        .arg("--policy")
        .arg(&profile_path)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run check with printed profile");

    assert!(
        via_profile.status.success(),
        "built-in profile check should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&via_profile.stdout),
        String::from_utf8_lossy(&via_profile.stderr)
    );
    assert!(
        via_policy.status.success(),
        "printed policy check should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&via_policy.stdout),
        String::from_utf8_lossy(&via_policy.stderr)
    );

    let profile_receipt = receipt_from_stdout(&via_profile);
    let policy_receipt = receipt_from_stdout(&via_policy);
    assert_eq!(profile_receipt["policy"]["profile"], "pure");
    assert_eq!(
        profile_receipt["policy"]["profile_digest"],
        policy_receipt["policy"]["source_digest"]
    );
    assert_eq!(policy_receipt["policy"]["profile"], serde_json::Value::Null);
    assert_eq!(
        policy_receipt["policy"]["profile_digest"],
        serde_json::Value::Null
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn check_profile_expect_digest_accepts_matching_builtin_digest() {
    let dir = std::env::temp_dir().join(format!(
        "buildlang_profile_expect_digest_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create profile digest fixture directory");
    let input = dir.join("pure.bld");
    fs::write(&input, r#"fn main() {}"#).expect("write pure input");

    let catalog_output = buildc()
        .args(["policy", "list", "--json"])
        .output()
        .expect("run buildc policy list --json");
    assert!(
        catalog_output.status.success(),
        "policy catalog should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&catalog_output.stdout),
        String::from_utf8_lossy(&catalog_output.stderr)
    );
    let catalog: serde_json::Value =
        serde_json::from_slice(&catalog_output.stdout).expect("catalog should be JSON");
    let pure_digest = catalog["profiles"]
        .as_array()
        .expect("profiles should be an array")
        .iter()
        .find(|profile| profile["name"] == "pure")
        .expect("pure profile")["digest"]["hex"]
        .as_str()
        .expect("pure digest")
        .to_string();

    let output = buildc()
        .arg("check")
        .arg(&input)
        .arg("--profile")
        .arg("pure")
        .arg("--expect-profile-digest")
        .arg(&pure_digest)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run check with matching profile digest");

    assert!(
        output.status.success(),
        "matching profile digest should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["policy"]["profile"], "pure");
    assert_eq!(receipt["policy"]["profile_digest"]["hex"], pure_digest);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn check_profile_expect_digest_rejects_mismatch() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_profile_digest_mismatch_{}.bld",
        std::process::id()
    ));
    fs::write(&fixture, r#"fn main() {}"#).expect("write pure fixture");
    let wrong_digest = "0".repeat(64);

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--profile")
        .arg("pure")
        .arg("--expect-profile-digest")
        .arg(&wrong_digest)
        .output()
        .expect("run check with mismatched profile digest");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "mismatched profile digest should fail"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Built-in policy profile digest mismatch"),
        "stderr should report digest mismatch:\n{stderr}"
    );
    assert!(
        stderr.contains("pure"),
        "stderr should name the profile:\n{stderr}"
    );
}

#[test]
fn check_expect_profile_digest_requires_builtin_profile() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_profile_digest_without_profile_{}.bld",
        std::process::id()
    ));
    fs::write(&fixture, r#"fn main() {}"#).expect("write pure fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--expect-profile-digest")
        .arg("0".repeat(64))
        .output()
        .expect("run check with profile digest but no profile");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "profile digest pin without profile should fail"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("requires --profile"),
        "stderr should report missing --profile:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_profile_rejects_unknown_builtin_profile() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_profile_unknown_{}.bld",
        std::process::id()
    ));
    fs::write(&fixture, r#"fn main() {}"#).expect("write pure fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--profile")
        .arg("missing")
        .output()
        .expect("run buildc check with missing profile");

    let _ = fs::remove_file(&fixture);

    assert!(!output.status.success(), "unknown profile should fail");
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("Unknown built-in policy profile 'missing'"),
        "stderr should name the unknown profile:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_rejects_policy_file_and_builtin_profile_together() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_check_profile_conflict_{}.bld",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "profile_conflict",
        r#"{
          "schema": "buildlang-check-policy/v1",
          "allowed_effects": ["Console"]
        }"#,
    );
    fs::write(&fixture, r#"fn main() ~ Console { println!("ok"); }"#)
        .expect("write conflict fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--profile")
        .arg("pure")
        .output()
        .expect("run buildc check with conflicting policy inputs");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        !output.status.success(),
        "policy file and profile should conflict"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("cannot be used with"),
        "stderr should report the argument conflict:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_accepts_explicit_root() {
    if !c_backend_ready() {
        eprintln!("skipping semantic corpus root verification because no C backend is available");
        return;
    }

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(repo_root().join("semantic-corpus"))
        .output()
        .expect("run buildc corpus verify with explicit root");

    assert!(
        output.status.success(),
        "corpus verify --root should exit successfully\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    assert!(
        stdout.contains("c execution: 8 passed"),
        "corpus verify --root should run the manifest programs:\n{}",
        stdout
    );
}

#[test]
fn corpus_verify_write_repairs_receipt_program_drift_in_copy() {
    if !c_backend_ready() {
        eprintln!("skipping semantic corpus write verification because no C backend is available");
        return;
    }

    let corpus_root = temp_semantic_corpus("write");
    let c_receipt_path = corpus_root
        .join("receipts")
        .join("c-execution-2026-06-13.json");
    let original_receipt = fs::read_to_string(&c_receipt_path).expect("read copied C receipt");
    let drifted_receipt = original_receipt.replacen(
        r#""expected_stdout": "4\n""#,
        r#""expected_stdout": "999\n""#,
        1,
    );
    assert_ne!(
        original_receipt, drifted_receipt,
        "copied C receipt should contain the scalar stdout fixture"
    );
    fs::write(&c_receipt_path, drifted_receipt).expect("write drifted copied C receipt");

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .arg("--write")
        .output()
        .expect("run buildc corpus verify --write");

    assert!(
        output.status.success(),
        "corpus verify --write should repair copied receipt drift\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    assert!(
        stdout.contains("c receipt: written"),
        "corpus verify --write should report the C receipt write:\n{}",
        stdout
    );

    let repaired_receipt = fs::read_to_string(&c_receipt_path).expect("read repaired C receipt");
    assert!(
        repaired_receipt.contains(r#""expected_stdout": "4\n""#),
        "repaired C receipt should restore manifest stdout:\n{}",
        repaired_receipt
    );
    assert!(
        !repaired_receipt.contains(r#""expected_stdout": "999\n""#),
        "repaired C receipt should remove drifted stdout:\n{}",
        repaired_receipt
    );

    let _ = fs::remove_dir_all(&corpus_root);
}

#[test]
fn corpus_verify_checks_manifest_receipts_and_c_execution() {
    if !c_backend_ready() {
        eprintln!("skipping semantic corpus verification because no C backend is available");
        return;
    }

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .output()
        .expect("run buildc corpus verify");

    assert!(
        output.status.success(),
        "corpus verify should exit successfully\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    for expected in [
        "Semantic Corpus Verify",
        "manifest: 8 program(s)",
        "c receipt: ok",
        "rust receipt: ok",
        "c execution: 8 passed",
    ] {
        assert!(
            stdout.contains(expected),
            "corpus verify output should contain {expected:?}:\n{}",
            stdout
        );
    }
}

#[test]
fn corpus_verify_checks_substrate_receipt() {
    if !c_backend_ready() {
        eprintln!("skipping substrate receipt verification because no C backend is available");
        return;
    }

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(repo_root().join("semantic-corpus"))
        .output()
        .expect("run buildc corpus verify with substrate receipt");

    assert!(
        output.status.success(),
        "corpus verify should accept substrate receipt\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    assert!(
        stdout.contains("substrate receipt: ok"),
        "corpus verify should report substrate receipt status:\n{}",
        stdout
    );
}

#[test]
fn corpus_verify_checks_mir_representation_receipt() {
    if !c_backend_ready() {
        eprintln!(
            "skipping MIR representation receipt verification because no C backend is available"
        );
        return;
    }

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(repo_root().join("semantic-corpus"))
        .output()
        .expect("run buildc corpus verify with MIR representation receipt");

    assert!(
        output.status.success(),
        "corpus verify should accept MIR representation receipt\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    assert!(
        stdout.contains("mir representation receipt: ok"),
        "corpus verify should report MIR representation receipt status:\n{}",
        stdout
    );
}

#[test]
fn corpus_verify_checks_memory_layout_receipt() {
    if !c_backend_ready() {
        eprintln!("skipping memory layout receipt verification because no C backend is available");
        return;
    }

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(repo_root().join("semantic-corpus"))
        .output()
        .expect("run buildc corpus verify with memory layout receipt");

    assert!(
        output.status.success(),
        "corpus verify should accept memory layout receipt\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    assert!(
        stdout.contains("memory layout receipt: ok"),
        "corpus verify should report memory layout receipt status:\n{}",
        stdout
    );
}

#[test]
fn corpus_verify_checks_symbol_graph_receipt() {
    if !c_backend_ready() {
        eprintln!("skipping symbol graph receipt verification because no C backend is available");
        return;
    }

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(repo_root().join("semantic-corpus"))
        .output()
        .expect("run buildc corpus verify with symbol graph receipt");

    assert!(
        output.status.success(),
        "corpus verify should accept symbol graph receipt\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    assert!(
        stdout.contains("symbol graph receipt: ok"),
        "corpus verify should report symbol graph receipt status:\n{}",
        stdout
    );
}

#[test]
fn corpus_verify_checks_module_graph_receipt() {
    if !c_backend_ready() {
        eprintln!("skipping module graph receipt verification because no C backend is available");
        return;
    }

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(repo_root().join("semantic-corpus"))
        .output()
        .expect("run buildc corpus verify with module graph receipt");

    assert!(
        output.status.success(),
        "corpus verify should accept module graph receipt\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    assert!(
        stdout.contains("module graph receipt: ok"),
        "corpus verify should report module graph receipt status:\n{}",
        stdout
    );
}

#[test]
fn corpus_verify_checks_lsp_dispatch_receipt() {
    if !c_backend_ready() {
        eprintln!("skipping LSP dispatch receipt verification because no C backend is available");
        return;
    }

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(repo_root().join("semantic-corpus"))
        .output()
        .expect("run buildc corpus verify with LSP dispatch receipt");

    assert!(
        output.status.success(),
        "corpus verify should accept LSP dispatch receipt\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    assert!(
        stdout.contains("lsp dispatch receipt: ok"),
        "corpus verify should report LSP dispatch receipt status:\n{}",
        stdout
    );
}

// -- C/Rust execution receipts: negative fixtures. These were the ONLY corpus
//    receipt family with zero tamper coverage; a verifier never demonstrated
//    failing is indistinguishable from a decorative one. --

/// Transform the C execution receipt in a corpus copy.
fn write_c_execution_receipt_copy(
    corpus_root: &Path,
    transform: impl FnOnce(serde_json::Value) -> serde_json::Value,
) {
    let receipt_path = corpus_root
        .join("receipts")
        .join("c-execution-2026-06-13.json");
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipt_path).expect("read c execution receipt"))
            .expect("parse c execution receipt");
    let rendered = serde_json::to_string_pretty(&transform(receipt))
        .expect("render modified c execution receipt");
    fs::write(&receipt_path, format!("{rendered}\n")).expect("write modified c execution receipt");
}

#[test]
fn corpus_verify_rejects_manifest_stdout_tamper() {
    // Tamper the expected stdout CONSISTENTLY in both the manifest and the C
    // execution receipt, so the receipt/manifest cross-check passes and the
    // failure can only come from the LIVE re-run comparing real program
    // output. This proves verify_c_corpus_stdout is a verifier that can fail,
    // not a bookkeeping echo.
    let corpus_root = temp_semantic_corpus("manifest_stdout_tamper");
    let manifest_path = corpus_root.join("manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).expect("read manifest"))
            .expect("parse manifest");
    manifest["programs"][0]["expected_stdout"] =
        serde_json::Value::String("tampered-stdout\n".to_string());
    fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).expect("render manifest"),
    )
    .expect("write tampered manifest");
    // BOTH execution receipts bind expected_stdout to the manifest, so both
    // must carry the same tampered value for the bookkeeping cross-checks to
    // pass and the live re-run to be the check that fires.
    write_c_execution_receipt_copy(&corpus_root, |mut receipt| {
        receipt["programs"][0]["expected_stdout"] =
            serde_json::Value::String("tampered-stdout\n".to_string());
        receipt
    });
    let rust_receipt_path = corpus_root
        .join("receipts")
        .join("rust-execution-2026-06-13.json");
    let mut rust_receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(&rust_receipt_path).expect("read rust execution receipt"))
            .expect("parse rust execution receipt");
    rust_receipt["programs"][0]["expected_stdout"] =
        serde_json::Value::String("tampered-stdout\n".to_string());
    fs::write(
        &rust_receipt_path,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&rust_receipt).expect("render rust execution receipt")
        ),
    )
    .expect("write tampered rust execution receipt");

    assert_corpus_verify_rejects(&corpus_root, "semantic corpus stdout drift");
}

#[test]
fn corpus_verify_rejects_c_execution_pass_count_tamper() {
    let corpus_root = temp_semantic_corpus("c_exec_pass_count");
    write_c_execution_receipt_copy(&corpus_root, |mut receipt| {
        receipt["result"]["passed"] = serde_json::Value::from(3);
        receipt
    });

    assert_corpus_verify_rejects(&corpus_root, "c receipt pass count mismatch");
}

#[test]
fn corpus_verify_rejects_c_execution_program_list_truncation() {
    let corpus_root = temp_semantic_corpus("c_exec_program_truncation");
    write_c_execution_receipt_copy(&corpus_root, |mut receipt| {
        let programs = receipt["programs"].as_array_mut().expect("programs array");
        programs.pop();
        receipt
    });

    assert_corpus_verify_rejects(&corpus_root, "c receipt program count mismatch");
}

#[test]
fn corpus_verify_rejects_capability_metadata_tamper() {
    // The stored capability facts must match a FRESH derivation from program
    // source through the type checker. A receipt claiming an extra capability
    // (or a missing one) must be rejected, not string-compared into a pass.
    let corpus_root = temp_semantic_corpus("capability_metadata_tamper");
    write_c_execution_receipt_copy(&corpus_root, |mut receipt| {
        receipt["observed_capabilities"]
            .as_array_mut()
            .expect("observed_capabilities array")
            .push(serde_json::Value::String("Network".to_string()));
        receipt
    });

    assert_corpus_verify_rejects(&corpus_root, "c receipt capability metadata drift");
}

#[test]
fn corpus_verify_rejects_capability_gate_stamp_tamper() {
    // The gate verdict itself is checked; a receipt with the gate blanked out
    // (or altered) must fail even when all counts and program lists match.
    let corpus_root = temp_semantic_corpus("capability_gate_tamper");
    write_c_execution_receipt_copy(&corpus_root, |mut receipt| {
        receipt["capability_gate"] = serde_json::Value::String("skipped".to_string());
        receipt
    });

    assert_corpus_verify_rejects(&corpus_root, "c receipt capability metadata drift");
}

#[test]
fn corpus_verify_rejects_per_program_surface_tamper() {
    // Deleting ONE program's stdout surface while seven other programs still
    // contribute Console defeated a union-level cross-check; the per-program
    // check must catch it (the review's confirmed escape).
    let corpus_root = temp_semantic_corpus("per_program_surface_tamper");
    let manifest_path = corpus_root.join("manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).expect("read manifest"))
            .expect("parse manifest");
    manifest["programs"][0]["surfaces"] = serde_json::Value::Array(Vec::new());
    fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).expect("render manifest"),
    )
    .expect("write tampered manifest");

    assert_corpus_verify_rejects(&corpus_root, "corpus manifest surface drift for program");
}

#[test]
fn corpus_verify_rejects_module_graph_schema_drift() {
    let corpus_root = temp_semantic_corpus("module_graph_schema");
    write_module_graph_receipt_copy(&corpus_root, |mut receipt| {
        receipt["schema"] = serde_json::Value::String("buildlang-module-graph-receipt/v9".into());
        receipt
    });

    assert_corpus_verify_rejects(&corpus_root, "module graph receipt has unsupported schema");
}

#[test]
fn corpus_verify_rejects_module_graph_program_count_drift() {
    let corpus_root = temp_semantic_corpus("module_graph_program_count");
    write_module_graph_receipt_copy(&corpus_root, |mut receipt| {
        receipt["source_set"]["program_count"] = serde_json::Value::from(7);
        receipt
    });

    assert_corpus_verify_rejects(
        &corpus_root,
        "module graph source_set.program_count mismatch",
    );
}

#[test]
fn corpus_verify_rejects_module_graph_path_escape() {
    let corpus_root = temp_semantic_corpus("module_graph_path_escape");
    write_module_graph_receipt_copy(&corpus_root, |mut receipt| {
        receipt["programs"][0]["path"] = serde_json::Value::String("../outside.bld".into());
        receipt
    });

    assert_corpus_verify_rejects(
        &corpus_root,
        "module graph program.path must stay within corpus root",
    );
}

#[test]
fn corpus_verify_rejects_module_graph_source_digest_drift() {
    let corpus_root = temp_semantic_corpus("module_graph_source_digest");
    write_module_graph_receipt_copy(&corpus_root, |mut receipt| {
        receipt["programs"][0]["source_digest"]["hex"] = serde_json::Value::String("0".repeat(64));
        receipt
    });

    assert_corpus_verify_rejects(
        &corpus_root,
        "module graph program scalar_branch source_digest mismatch",
    );
}

#[test]
fn corpus_verify_rejects_module_graph_input_graph_digest_drift() {
    let corpus_root = temp_semantic_corpus("module_graph_input_graph_digest");
    write_module_graph_receipt_copy(&corpus_root, |mut receipt| {
        receipt["programs"][0]["input_graph_digest"]["hex"] =
            serde_json::Value::String("1".repeat(64));
        receipt
    });

    assert_corpus_verify_rejects(
        &corpus_root,
        "module graph program scalar_branch input_graph_digest mismatch",
    );
}

#[test]
fn corpus_verify_rejects_module_graph_inputs_drift() {
    let corpus_root = temp_semantic_corpus("module_graph_inputs");
    write_module_graph_receipt_copy(&corpus_root, |mut receipt| {
        receipt["programs"][0]["inputs"][0]["role"] = serde_json::Value::String("forged".into());
        receipt
    });

    assert_corpus_verify_rejects(
        &corpus_root,
        "module graph program scalar_branch inputs drift",
    );
}

#[test]
fn corpus_verify_rejects_module_graph_edges_drift() {
    let corpus_root = temp_semantic_corpus("module_graph_edges");
    write_module_graph_receipt_copy(&corpus_root, |mut receipt| {
        receipt["programs"][0]["edges"][0]["kind"] =
            serde_json::Value::String("forged_edge".into());
        receipt
    });

    assert_corpus_verify_rejects(
        &corpus_root,
        "module graph program scalar_branch edges drift",
    );
}

#[test]
fn corpus_verify_rejects_module_graph_summary_drift() {
    let corpus_root = temp_semantic_corpus("module_graph_summary");
    write_module_graph_receipt_copy(&corpus_root, |mut receipt| {
        receipt["summary"]["input_count"] = serde_json::Value::from(999);
        receipt
    });

    assert_corpus_verify_rejects(&corpus_root, "module graph summary drift");
}

#[test]
fn corpus_verify_rejects_lsp_dispatch_schema_drift() {
    let corpus_root = temp_semantic_corpus("lsp_dispatch_schema");
    write_lsp_dispatch_receipt_copy(&corpus_root, |mut receipt| {
        receipt["schema"] = serde_json::Value::String("buildlang-lsp-dispatch-receipt/v9".into());
        receipt
    });

    assert_corpus_verify_rejects(&corpus_root, "lsp dispatch receipt has unsupported schema");
}

#[test]
fn corpus_verify_rejects_lsp_dispatch_fixture_digest_drift() {
    let corpus_root = temp_semantic_corpus("lsp_dispatch_digest");
    write_lsp_dispatch_receipt_copy(&corpus_root, |mut receipt| {
        let fixture = receipt["fixtures"]
            .as_array_mut()
            .expect("fixtures should be an array")
            .iter_mut()
            .find(|fixture| fixture["id"] == "document-symbol")
            .expect("document-symbol fixture should exist");
        fixture["result_digest"]["hex"] = serde_json::Value::String("0".repeat(64));
        receipt
    });

    assert_corpus_verify_rejects(
        &corpus_root,
        "lsp dispatch fixture document-symbol result_digest mismatch",
    );
}

#[test]
fn corpus_verify_rejects_lsp_dispatch_observed_drift() {
    let corpus_root = temp_semantic_corpus("lsp_dispatch_observed");
    write_lsp_dispatch_receipt_copy(&corpus_root, |mut receipt| {
        let fixture = receipt["fixtures"]
            .as_array_mut()
            .expect("fixtures should be an array")
            .iter_mut()
            .find(|fixture| fixture["id"] == "document-symbol")
            .expect("document-symbol fixture should exist");
        fixture["observed"]["document_symbols"] = serde_json::Value::from(0);
        receipt
    });

    assert_corpus_verify_rejects(
        &corpus_root,
        "lsp dispatch fixture document-symbol observed drift",
    );
}

#[test]
fn corpus_verify_rejects_lsp_dispatch_semantic_tokens_observed_drift() {
    let corpus_root = temp_semantic_corpus("lsp_dispatch_semantic_tokens_observed");
    write_lsp_dispatch_receipt_copy(&corpus_root, |mut receipt| {
        let fixture = receipt["fixtures"]
            .as_array_mut()
            .expect("fixtures should be an array")
            .iter_mut()
            .find(|fixture| fixture["id"] == "semantic-tokens")
            .expect("semantic token fixture should exist");
        fixture["observed"]["semantic_tokens"] = serde_json::Value::from(0);
        receipt
    });

    assert_corpus_verify_rejects(
        &corpus_root,
        "lsp dispatch fixture semantic-tokens observed drift",
    );
}

#[test]
fn corpus_verify_rejects_lsp_dispatch_workspace_symbol_observed_drift() {
    let corpus_root = temp_semantic_corpus("lsp_dispatch_workspace_symbol_observed");
    write_lsp_dispatch_receipt_copy(&corpus_root, |mut receipt| {
        let fixture = receipt["fixtures"]
            .as_array_mut()
            .expect("fixtures should be an array")
            .iter_mut()
            .find(|fixture| fixture["id"] == "workspace-symbol")
            .expect("workspace symbol fixture should exist");
        fixture["observed"]["workspace_symbols"] = serde_json::Value::from(0);
        receipt
    });

    assert_corpus_verify_rejects(
        &corpus_root,
        "lsp dispatch fixture workspace-symbol observed drift",
    );
}

#[test]
fn corpus_verify_rejects_lsp_dispatch_code_action_observed_drift() {
    let corpus_root = temp_semantic_corpus("lsp_dispatch_code_action_observed");
    write_lsp_dispatch_receipt_copy(&corpus_root, |mut receipt| {
        let fixture = receipt["fixtures"]
            .as_array_mut()
            .expect("fixtures should be an array")
            .iter_mut()
            .find(|fixture| fixture["method"] == "textDocument/codeAction")
            .expect("codeAction fixture should exist");
        fixture["observed"]["code_actions"] = serde_json::Value::from(0);
        receipt
    });

    assert_corpus_verify_rejects(
        &corpus_root,
        "lsp dispatch fixture code-action observed drift",
    );
}

#[test]
fn corpus_verify_rejects_lsp_dispatch_rename_observed_drift() {
    let corpus_root = temp_semantic_corpus("lsp_dispatch_rename_observed");
    write_lsp_dispatch_receipt_copy(&corpus_root, |mut receipt| {
        let fixture = receipt["fixtures"]
            .as_array_mut()
            .expect("fixtures should be an array")
            .iter_mut()
            .find(|fixture| fixture["method"] == "textDocument/rename")
            .expect("rename fixture should exist");
        fixture["observed"]["workspace_edits"] = serde_json::Value::from(0);
        receipt
    });

    assert_corpus_verify_rejects(&corpus_root, "lsp dispatch fixture rename observed drift");
}

#[test]
fn corpus_verify_rejects_lsp_dispatch_compiler_diagnostic_observed_drift() {
    let corpus_root = temp_semantic_corpus("lsp_dispatch_compiler_diagnostic_observed");
    write_lsp_dispatch_receipt_copy(&corpus_root, |mut receipt| {
        let fixture = receipt["fixtures"]
            .as_array_mut()
            .expect("fixtures should be an array")
            .iter_mut()
            .find(|fixture| fixture["id"] == "did-change-type-error")
            .expect("type-error fixture should exist");
        fixture["observed"]["compiler_diagnostics"] = serde_json::Value::from(0);
        receipt
    });

    assert_corpus_verify_rejects(
        &corpus_root,
        "lsp dispatch fixture did-change-type-error observed drift",
    );
}

#[test]
fn corpus_verify_rejects_lsp_dispatch_stale_compiler_diagnostics_gap() {
    let corpus_root = temp_semantic_corpus("lsp_dispatch_compiler_diagnostics_gap");
    write_lsp_dispatch_receipt_copy(&corpus_root, |mut receipt| {
        receipt["summary"]["known_gaps"] = serde_json::json!([
            "compiler type-checker diagnostics in LSP",
            "full VS Code extension readiness"
        ]);
        receipt
    });

    assert_corpus_verify_rejects(&corpus_root, "lsp dispatch summary drift");
}

#[test]
fn corpus_verify_rejects_lsp_dispatch_summary_drift() {
    let corpus_root = temp_semantic_corpus("lsp_dispatch_summary");
    write_lsp_dispatch_receipt_copy(&corpus_root, |mut receipt| {
        receipt["summary"]["known_gaps"]
            .as_array_mut()
            .expect("known_gaps should be an array")
            .push(serde_json::Value::String("untracked gap".into()));
        receipt
    });

    assert_corpus_verify_rejects(&corpus_root, "lsp dispatch summary drift");
}

#[test]
fn corpus_verify_rejects_lsp_dispatch_stale_parser_metadata() {
    let corpus_root = temp_semantic_corpus("lsp_dispatch_parser_metadata");
    write_lsp_dispatch_receipt_copy(&corpus_root, |mut receipt| {
        receipt["lsp_model"]["request_parser"] =
            serde_json::Value::String("simplified string extraction".into());
        receipt
    });

    assert_corpus_verify_rejects(&corpus_root, "lsp dispatch lsp_model drift");
}

#[test]
fn corpus_verify_rejects_lsp_dispatch_stale_json_rpc_gap() {
    let corpus_root = temp_semantic_corpus("lsp_dispatch_json_rpc_gap");
    write_lsp_dispatch_receipt_copy(&corpus_root, |mut receipt| {
        receipt["summary"]["known_gaps"] = serde_json::json!([
            "compiler type-checker diagnostics in LSP",
            "full JSON-RPC deserialization",
            "full VS Code extension readiness"
        ]);
        receipt
    });

    assert_corpus_verify_rejects(&corpus_root, "lsp dispatch summary drift");
}

#[test]
fn corpus_verify_rejects_symbol_graph_schema_drift() {
    let corpus_root = temp_semantic_corpus("symbol_graph_schema");
    write_symbol_graph_receipt_copy(&corpus_root, |mut receipt| {
        receipt["schema"] = serde_json::Value::String("buildlang-symbol-graph-receipt/v9".into());
        receipt
    });

    assert_corpus_verify_rejects(&corpus_root, "symbol graph receipt has unsupported schema");
}

#[test]
fn corpus_verify_rejects_symbol_graph_program_count_drift() {
    let corpus_root = temp_semantic_corpus("symbol_graph_program_count");
    write_symbol_graph_receipt_copy(&corpus_root, |mut receipt| {
        receipt["source_set"]["program_count"] = serde_json::Value::from(7);
        receipt
    });

    assert_corpus_verify_rejects(
        &corpus_root,
        "symbol graph source_set.program_count mismatch",
    );
}

#[test]
fn corpus_verify_rejects_symbol_graph_path_escape() {
    let corpus_root = temp_semantic_corpus("symbol_graph_path_escape");
    write_symbol_graph_receipt_copy(&corpus_root, |mut receipt| {
        receipt["programs"][0]["path"] = serde_json::Value::String("../outside.bld".into());
        receipt
    });

    assert_corpus_verify_rejects(
        &corpus_root,
        "symbol graph program.path must stay within corpus root",
    );
}

#[test]
fn corpus_verify_rejects_symbol_graph_source_digest_drift() {
    let corpus_root = temp_semantic_corpus("symbol_graph_source_digest");
    write_symbol_graph_receipt_copy(&corpus_root, |mut receipt| {
        receipt["programs"][0]["source_digest"]["hex"] = serde_json::Value::String("0".repeat(64));
        receipt
    });

    assert_corpus_verify_rejects(
        &corpus_root,
        "symbol graph program scalar_branch source_digest mismatch",
    );
}

#[test]
fn corpus_verify_rejects_symbol_graph_source_symbol_drift() {
    let corpus_root = temp_semantic_corpus("symbol_graph_source_symbol");
    write_symbol_graph_receipt_copy(&corpus_root, |mut receipt| {
        receipt["programs"][0]["source_symbols"] = serde_json::json!([]);
        receipt
    });

    assert_corpus_verify_rejects(
        &corpus_root,
        "symbol graph program scalar_branch source_symbols drift",
    );
}

#[test]
fn corpus_verify_rejects_symbol_graph_mir_symbol_drift() {
    let corpus_root = temp_semantic_corpus("symbol_graph_mir_symbol");
    write_symbol_graph_receipt_copy(&corpus_root, |mut receipt| {
        receipt["programs"][0]["mir_symbols"]["functions"] = serde_json::json!(["forged"]);
        receipt
    });

    assert_corpus_verify_rejects(
        &corpus_root,
        "symbol graph program scalar_branch mir_symbols drift",
    );
}

#[test]
fn corpus_verify_rejects_symbol_graph_edge_drift() {
    let corpus_root = temp_semantic_corpus("symbol_graph_edge");
    write_symbol_graph_receipt_copy(&corpus_root, |mut receipt| {
        receipt["programs"][0]["edges"] = serde_json::json!([]);
        receipt
    });

    assert_corpus_verify_rejects(
        &corpus_root,
        "symbol graph program scalar_branch edges drift",
    );
}

#[test]
fn corpus_verify_rejects_symbol_graph_lsp_overclaim() {
    let corpus_root = temp_semantic_corpus("symbol_graph_lsp_overclaim");
    write_symbol_graph_receipt_copy(&corpus_root, |mut receipt| {
        receipt["symbol_model"]["semantic_anchor"] =
            serde_json::Value::String("LSP request dispatch verified".into());
        receipt
    });

    assert_corpus_verify_rejects(&corpus_root, "symbol graph symbol_model drift");
}

#[test]
fn corpus_verify_rejects_memory_layout_schema_drift() {
    let corpus_root = temp_semantic_corpus("memory_layout_schema");
    write_memory_layout_receipt_copy(&corpus_root, |mut receipt| {
        receipt["schema"] = serde_json::Value::String("buildlang-memory-layout-receipt/v9".into());
        receipt
    });

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run buildc corpus verify against bad memory layout schema");
    let _ = fs::remove_dir_all(&corpus_root);

    assert!(!output.status.success(), "schema drift should fail");
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("memory layout receipt has unsupported schema"),
        "stderr should name memory layout schema drift:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_memory_layout_program_count_drift() {
    let corpus_root = temp_semantic_corpus("memory_layout_program_count");
    write_memory_layout_receipt_copy(&corpus_root, |mut receipt| {
        receipt["source_set"]["program_count"] = serde_json::Value::from(7);
        receipt
    });

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run buildc corpus verify against memory layout program count drift");
    let _ = fs::remove_dir_all(&corpus_root);

    assert!(!output.status.success(), "program count drift should fail");
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("memory layout source_set.program_count mismatch"),
        "stderr should name memory layout program count drift:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_memory_layout_path_escape() {
    let corpus_root = temp_semantic_corpus("memory_layout_path_escape");
    write_memory_layout_receipt_copy(&corpus_root, |mut receipt| {
        receipt["programs"][0]["path"] = serde_json::Value::String("../outside.bld".into());
        receipt
    });

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run buildc corpus verify against memory layout path escape");
    let _ = fs::remove_dir_all(&corpus_root);

    assert!(!output.status.success(), "path escape should fail");
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("memory layout program.path must stay within corpus root"),
        "stderr should name memory layout path containment:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_memory_layout_source_digest_drift() {
    let corpus_root = temp_semantic_corpus("memory_layout_source_digest");
    write_memory_layout_receipt_copy(&corpus_root, |mut receipt| {
        receipt["programs"][0]["source_digest"]["hex"] = serde_json::Value::String("0".repeat(64));
        receipt
    });

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run buildc corpus verify against memory layout source digest drift");
    let _ = fs::remove_dir_all(&corpus_root);

    assert!(!output.status.success(), "source digest drift should fail");
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("memory layout program scalar_branch source_digest mismatch"),
        "stderr should name memory layout source digest drift:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_memory_layout_observed_surface_drift() {
    let corpus_root = temp_semantic_corpus("memory_layout_observed_surface");
    write_memory_layout_receipt_copy(&corpus_root, |mut receipt| {
        receipt["programs"][1]["observed_memory_surfaces"]["references"] =
            serde_json::Value::Bool(false);
        receipt
    });

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run buildc corpus verify against memory layout observed surface drift");
    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "observed memory surface drift should fail"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("memory layout program references_mutation observed_memory_surfaces drift"),
        "stderr should name memory layout observed surface drift:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_memory_layout_known_gap_drift() {
    let corpus_root = temp_semantic_corpus("memory_layout_known_gap");
    write_memory_layout_receipt_copy(&corpus_root, |mut receipt| {
        receipt["summary"]["known_gaps"] = serde_json::json!(["none"]);
        receipt
    });

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run buildc corpus verify against memory layout known gap drift");
    let _ = fs::remove_dir_all(&corpus_root);

    assert!(!output.status.success(), "known gap drift should fail");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("memory layout summary drift"),
        "stderr should name memory layout summary drift:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_memory_layout_byte_layout_overclaim() {
    let corpus_root = temp_semantic_corpus("memory_layout_overclaim");
    write_memory_layout_receipt_copy(&corpus_root, |mut receipt| {
        receipt["memory_model"]["layout_claim"] =
            serde_json::Value::String("byte-offset ABI layout verified".into());
        receipt
    });

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run buildc corpus verify against memory layout overclaim");
    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "byte layout overclaim should fail"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("memory layout memory_model drift"),
        "stderr should name memory layout overclaim:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_mir_representation_receipt_schema_drift() {
    let corpus_root = temp_semantic_corpus("mir_repr_schema");
    write_mir_representation_receipt_copy(&corpus_root, |mut receipt| {
        receipt["schema"] =
            serde_json::Value::String("buildlang-mir-representation-receipt/v9".into());
        receipt
    });

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run buildc corpus verify against bad MIR representation schema");

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject MIR representation schema drift"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("mir representation receipt has unsupported schema"),
        "stderr should name MIR representation schema drift:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_mir_representation_program_count_drift() {
    let corpus_root = temp_semantic_corpus("mir_repr_program_count");
    write_mir_representation_receipt_copy(&corpus_root, |mut receipt| {
        receipt["source_set"]["program_count"] = serde_json::Value::from(7);
        receipt
    });

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run buildc corpus verify against MIR representation program count drift");

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject MIR representation program count drift"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("mir representation source_set.program_count mismatch"),
        "stderr should name MIR representation program count drift:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_mir_representation_path_escape() {
    let corpus_root = temp_semantic_corpus("mir_repr_path_escape");
    write_mir_representation_receipt_copy(&corpus_root, |mut receipt| {
        receipt["programs"][0]["path"] = serde_json::Value::String("../outside.bld".into());
        receipt
    });

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run buildc corpus verify against MIR representation path escape");

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject MIR representation path escape"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("must stay within corpus root"),
        "stderr should name MIR representation path containment failure:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_mir_representation_path_escape_absolute() {
    let corpus_root = temp_semantic_corpus("mir_repr_path_escape_absolute");
    let absolute_path = std::env::temp_dir().join("outside.bld");
    write_mir_representation_receipt_copy(&corpus_root, |mut receipt| {
        receipt["programs"][0]["path"] =
            serde_json::Value::String(absolute_path.to_string_lossy().into_owned());
        receipt
    });

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run buildc corpus verify against MIR representation absolute path");

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject MIR representation absolute path"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("must stay within corpus root"),
        "stderr should name MIR representation path containment failure:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_mir_representation_source_digest_drift() {
    let corpus_root = temp_semantic_corpus("mir_repr_source_digest");
    write_mir_representation_receipt_copy(&corpus_root, |mut receipt| {
        receipt["programs"][0]["source_digest"]["hex"] = serde_json::Value::String("0".repeat(64));
        receipt
    });

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run buildc corpus verify against MIR representation source digest drift");

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject MIR representation source digest drift"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("mir representation program scalar_branch source_digest mismatch"),
        "stderr should name MIR representation source digest drift:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_mir_representation_operation_drift() {
    let corpus_root = temp_semantic_corpus("mir_repr_operation_drift");
    write_mir_representation_receipt_copy(&corpus_root, |mut receipt| {
        receipt["programs"][0]["operations"]["rvalues"] = serde_json::json!(["ForgedRValue"]);
        receipt
    });

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run buildc corpus verify against MIR representation operation drift");

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject MIR representation operation drift"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("mir representation program scalar_branch operations.rvalues drift"),
        "stderr should name MIR representation operation drift:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_mir_representation_input_graph_digest_drift() {
    let corpus_root = temp_semantic_corpus("mir_repr_input_graph_digest");
    write_mir_representation_receipt_copy(&corpus_root, |mut receipt| {
        receipt["programs"][0]["input_graph_digest"]["hex"] =
            serde_json::Value::String("f".repeat(64));
        receipt
    });

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run buildc corpus verify against MIR representation input graph digest drift");

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject MIR representation input graph digest drift"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("mir representation program scalar_branch input_graph_digest mismatch"),
        "stderr should name MIR representation input graph digest drift:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_mir_representation_mir_digest_drift() {
    let corpus_root = temp_semantic_corpus("mir_repr_mir_digest");
    write_mir_representation_receipt_copy(&corpus_root, |mut receipt| {
        receipt["programs"][0]["mir_digest"]["hex"] = serde_json::Value::String("e".repeat(64));
        receipt
    });

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run buildc corpus verify against MIR representation MIR digest drift");

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject MIR representation MIR digest drift"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("mir representation program scalar_branch mir_digest mismatch"),
        "stderr should name MIR representation MIR digest drift:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_substrate_receipt_schema_drift() {
    let corpus_root = temp_semantic_corpus("substrate_schema");
    write_substrate_receipt_copy(&corpus_root, |mut receipt| {
        receipt["schema"] = serde_json::Value::String("buildlang-substrate-receipt/v9".into());
        receipt
    });

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run buildc corpus verify against bad substrate schema");

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject bad substrate schema"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("substrate receipt has unsupported schema"),
        "stderr should name schema drift:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_substrate_program_count_drift() {
    let corpus_root = temp_semantic_corpus("substrate_program_count");
    write_substrate_receipt_copy(&corpus_root, |mut receipt| {
        receipt["source_set"]["program_count"] = serde_json::Value::from(7);
        receipt
    });

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run buildc corpus verify against bad substrate program count");

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject substrate program count drift"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("substrate source_set.program_count mismatch"),
        "stderr should name program-count drift:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_production_substrate_backend_without_receipt() {
    let corpus_root = temp_semantic_corpus("substrate_missing_receipt");
    write_substrate_receipt_copy(&corpus_root, |mut receipt| {
        if let Some(c) = receipt["execution_surface"]["c"].as_object_mut() {
            c.remove("receipt");
        }
        receipt
    });

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run buildc corpus verify against missing production receipt");

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject production backend without receipt"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("substrate execution_surface.c is production-anchor but receipt is missing"),
        "stderr should name missing production receipt:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_substrate_rust_subset_without_receipt() {
    let corpus_root = temp_semantic_corpus("substrate_rust_subset_missing_receipt");
    write_substrate_receipt_copy(&corpus_root, |mut receipt| {
        if let Some(rust) = receipt["execution_surface"]["rust"].as_object_mut() {
            rust.remove("receipt");
        }
        receipt
    });

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run buildc corpus verify against missing rust subset receipt");

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject rust subset without receipt evidence"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(
            "substrate execution_surface.rust experimental-subset requires receipt evidence"
        ),
        "stderr should name missing rust subset receipt:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_substrate_missing_required_spirv_target() {
    let corpus_root = temp_semantic_corpus("substrate_missing_spirv");
    write_substrate_receipt_copy(&corpus_root, |mut receipt| {
        if let Some(execution_surface) = receipt["execution_surface"].as_object_mut() {
            execution_surface.remove("spirv");
        }
        receipt
    });

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run buildc corpus verify against missing spirv target");

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject missing required spirv target"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("substrate execution_surface missing required target spirv"),
        "stderr should name missing required spirv target:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_substrate_receipt_path_escape() {
    let corpus_root = temp_semantic_corpus("substrate_path_escape");
    write_substrate_receipt_copy(&corpus_root, |mut receipt| {
        receipt["execution_surface"]["c"]["receipt"] =
            serde_json::Value::String("../outside.json".into());
        receipt
    });

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run buildc corpus verify against substrate path escape");

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject substrate receipt path escape"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("must stay within corpus root"),
        "stderr should name substrate path containment failure:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_substrate_representation_receipt_path_escape() {
    let corpus_root = temp_semantic_corpus("substrate_repr_path_escape");
    write_substrate_receipt_copy(&corpus_root, |mut receipt| {
        receipt["representation_surface"]["representation_receipt"] =
            serde_json::Value::String("../outside.json".into());
        receipt
    });

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run buildc corpus verify against substrate representation receipt path escape");

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject substrate representation receipt path escape"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("must stay within corpus root"),
        "stderr should name substrate representation receipt path containment failure:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_substrate_memory_receipt_path_escape() {
    let corpus_root = temp_semantic_corpus("substrate_memory_path_escape");
    write_substrate_receipt_copy(&corpus_root, |mut receipt| {
        receipt["memory_surface"]["memory_receipt"] =
            serde_json::Value::String("../memory-layout-2026-06-18.json".into());
        receipt
    });

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run buildc corpus verify against substrate memory receipt path escape");
    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "substrate memory receipt path escape should fail"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("substrate memory_surface.memory_receipt must stay within corpus root"),
        "stderr should name substrate memory receipt path containment:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_substrate_module_receipt_missing_path() {
    let corpus_root = temp_semantic_corpus("substrate_module_path_missing");
    write_substrate_receipt_copy(&corpus_root, |mut receipt| {
        receipt["module_surface"]["module_receipt"] =
            serde_json::Value::String("receipts/missing-module-graph.json".into());
        receipt
    });

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run buildc corpus verify against substrate module receipt missing path");
    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "substrate module receipt missing path should fail"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("substrate module_surface.module_receipt path not found"),
        "stderr should name substrate module receipt missing path:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_substrate_module_receipt_path_escape() {
    let corpus_root = temp_semantic_corpus("substrate_module_path_escape");
    write_substrate_receipt_copy(&corpus_root, |mut receipt| {
        receipt["module_surface"]["module_receipt"] =
            serde_json::Value::String("../module-graph-2026-06-18.json".into());
        receipt
    });

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run buildc corpus verify against substrate module receipt path escape");
    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "substrate module receipt path escape should fail"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("substrate module_surface.module_receipt must stay within corpus root"),
        "stderr should name substrate module receipt path containment:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_substrate_symbol_receipt_path_escape() {
    let corpus_root = temp_semantic_corpus("substrate_symbol_path_escape");
    write_substrate_receipt_copy(&corpus_root, |mut receipt| {
        receipt["symbol_surface"]["symbol_receipt"] =
            serde_json::Value::String("../symbol.json".into());
        receipt
    });

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run buildc corpus verify against substrate symbol receipt path escape");
    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "substrate symbol receipt path escape should fail"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("substrate symbol_surface.symbol_receipt must stay within corpus root"),
        "stderr should name substrate symbol receipt path containment:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_substrate_lsp_receipt_missing_path() {
    let corpus_root = temp_semantic_corpus("substrate_lsp_path_missing");
    write_substrate_receipt_copy(&corpus_root, |mut receipt| {
        receipt["lsp_surface"]["lsp_receipt"] =
            serde_json::Value::String("receipts/missing-lsp-dispatch.json".into());
        receipt
    });

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run buildc corpus verify against substrate LSP receipt missing path");
    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "substrate LSP receipt missing path should fail"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("substrate lsp_surface.lsp_receipt path not found"),
        "stderr should name substrate LSP receipt missing path:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_substrate_lsp_receipt_path_escape() {
    let corpus_root = temp_semantic_corpus("substrate_lsp_path_escape");
    write_substrate_receipt_copy(&corpus_root, |mut receipt| {
        receipt["lsp_surface"]["lsp_receipt"] =
            serde_json::Value::String("../lsp-dispatch-2026-06-18.json".into());
        receipt
    });

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run buildc corpus verify against substrate LSP receipt path escape");
    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "substrate LSP receipt path escape should fail"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("substrate lsp_surface.lsp_receipt must stay within corpus root"),
        "stderr should name substrate LSP receipt path containment:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_substrate_representation_receipt_root_qualified_path() {
    let corpus_root = temp_semantic_corpus("substrate_repr_path_root");
    let rooted = if cfg!(windows) {
        "\\receipts\\mir-representation-2026-06-18.json"
    } else {
        "/receipts/mir-representation-2026-06-18.json"
    };
    write_substrate_receipt_copy(&corpus_root, |mut receipt| {
        receipt["representation_surface"]["representation_receipt"] =
            serde_json::Value::String(rooted.into());
        receipt
    });

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect(
            "run buildc corpus verify against substrate representation receipt root-qualified path",
        );

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject substrate representation receipt root-qualified path"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("must stay within corpus root"),
        "stderr should name substrate representation receipt path containment failure:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_substrate_representation_receipt_absolute_path() {
    let corpus_root = temp_semantic_corpus("substrate_repr_path_absolute");
    let absolute_path = std::env::temp_dir().join("outside.json");
    write_substrate_receipt_copy(&corpus_root, |mut receipt| {
        receipt["representation_surface"]["representation_receipt"] =
            serde_json::Value::String(absolute_path.to_string_lossy().into_owned());
        receipt
    });

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run buildc corpus verify against substrate representation receipt absolute path");

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject substrate representation receipt absolute path"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("must stay within corpus root"),
        "stderr should name substrate representation receipt path containment failure:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_substrate_representation_receipt_windows_drive_path() {
    let corpus_root = temp_semantic_corpus("substrate_repr_windows_drive_path");
    write_substrate_receipt_copy(&corpus_root, |mut receipt| {
        receipt["representation_surface"]["representation_receipt"] =
            serde_json::Value::String("C:\\outside.json".into());
        receipt
    });

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect(
            "run buildc corpus verify against substrate representation receipt windows drive path",
        );

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject substrate representation receipt windows drive path"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("must stay within corpus root"),
        "stderr should name substrate representation receipt path containment failure:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_substrate_representation_receipt_windows_drive_relative_path() {
    let corpus_root = temp_semantic_corpus("substrate_repr_windows_drive_relative_path");
    write_substrate_receipt_copy(&corpus_root, |mut receipt| {
        receipt["representation_surface"]["representation_receipt"] =
            serde_json::Value::String("C:outside.json".into());
        receipt
    });

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect(
            "run buildc corpus verify against substrate representation receipt windows drive-relative path",
        );

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject substrate representation receipt windows drive-relative path"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("must stay within corpus root"),
        "stderr should name substrate representation receipt path containment failure:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_empty_substrate_evidence_commands() {
    let corpus_root = temp_semantic_corpus("substrate_empty_commands");
    write_substrate_receipt_copy(&corpus_root, |mut receipt| {
        receipt["evidence_surface"]["commands"] = serde_json::Value::Array(Vec::new());
        receipt
    });

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run buildc corpus verify against empty substrate commands");

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject empty evidence commands"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("substrate evidence_surface.commands must not be empty"),
        "stderr should name empty evidence commands:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn quickstart_examples_are_typechecked() {
    for name in [
        "hello.bld",
        "ledger.bld",
        "effects_greeting.bld",
        "vignette_shader.bld",
    ] {
        let path = quickstart_example(name);
        let output = buildc()
            .arg("check")
            .arg(&path)
            .output()
            .unwrap_or_else(|err| panic!("run buildc check for {name}: {err}"));

        assert!(
            output.status.success(),
            "quickstart example {name} should typecheck\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn quickstart_cpu_examples_run_when_c_backend_is_ready() {
    if !c_backend_ready() {
        eprintln!("skipping quickstart run smoke test because no C backend is available");
        return;
    }

    for (name, expected_stdout) in [
        ("hello.bld", "Hello from BuildLang!\n"),
        ("ledger.bld", "balance: 115\n"),
        ("effects_greeting.bld", "Hello, teammate!\n"),
    ] {
        let output = buildc()
            .arg("run")
            .arg(quickstart_example(name))
            .output()
            .unwrap_or_else(|err| panic!("run buildc run for {name}: {err}"));

        assert!(
            output.status.success(),
            "quickstart example {name} should run\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
        assert_eq!(stdout, expected_stdout);
    }
}

#[test]
fn quickstart_shader_example_compiles_to_hlsl() {
    let out_dir = std::env::temp_dir().join(format!(
        "buildlang_quickstart_shader_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&out_dir);
    fs::create_dir_all(&out_dir).expect("create quickstart shader temp dir");
    let output_path = out_dir.join("vignette_shader.hlsl");

    let output = buildc()
        .arg(quickstart_example("vignette_shader.bld"))
        .arg("--target")
        .arg("hlsl")
        .arg("-o")
        .arg(&output_path)
        .output()
        .expect("compile quickstart shader to HLSL");

    assert!(
        output.status.success(),
        "quickstart shader should compile to HLSL\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let hlsl = fs::read_to_string(&output_path).expect("read generated HLSL");
    assert!(
        hlsl.contains("PS_Vignette"),
        "generated HLSL should contain the fragment entry point:\n{}",
        hlsl
    );

    let _ = fs::remove_dir_all(&out_dir);
}

// =============================================================================
// Transpile-preservation criterion (Phase 1, brick 2)
//
// Criterion: when the SAME program is lowered to the MIR interlingua and then
// emitted through two different backend target languages, the observable
// contract that must be preserved is byte-identical stdout AND equal process
// exit status, independent of the target language. The test below asserts that
// the C and Rust backends AGREE WITH EACH OTHER (not merely that each matches
// its own pre-recorded expected stdout). Honest scope: only the Rust-supported
// corpus subset can be cross-checked; the C path covers the whole corpus.
//
// See docs/superpowers/specs/transpile-preservation-criterion.md.
// =============================================================================

#[derive(serde::Deserialize)]
struct PreservationCorpusManifest {
    programs: Vec<PreservationCorpusProgram>,
}

#[derive(serde::Deserialize)]
struct PreservationCorpusProgram {
    id: String,
    path: String,
    rust_execution_test: String,
}

fn semantic_corpus_root() -> PathBuf {
    repo_root().join("semantic-corpus")
}

fn load_preservation_corpus() -> PreservationCorpusManifest {
    let manifest_path = semantic_corpus_root().join("manifest.json");
    let manifest = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|err| panic!("read {}: {}", manifest_path.display(), err));
    serde_json::from_str(&manifest)
        .unwrap_or_else(|err| panic!("parse {}: {}", manifest_path.display(), err))
}

/// True when a `rustc` invocation is available to compile the Rust backend's
/// output. Honors the `RUSTC` override the existing rust.rs tests use.
fn rustc_available() -> bool {
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    Command::new(rustc)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Lower a BuildLang source string through the front end and the Rust backend,
/// returning the generated Rust source. Mirrors `compile_build_to_rust` in the
/// rust backend's unit tests, but reached through the public library API.
fn lower_source_to_rust(source: &str) -> String {
    use buildlang::lexer::{Lexer, SourceFile};
    use buildlang::parser::Parser;
    use buildlang::types::{TypeChecker, TypeContext};
    use buildlang::{CodeGenerator, Target};

    let source_file = SourceFile::new("transpile_preservation_test.bld", source);
    let mut lexer = Lexer::new(&source_file);
    let tokens = lexer.tokenize().expect("lexing should succeed");
    let mut parser = Parser::new(&source_file, tokens);
    let ast = parser.parse().expect("parsing should succeed");
    assert!(
        parser.errors().is_empty(),
        "unexpected parser errors: {:?}",
        parser.errors()
    );

    let mut ctx = TypeContext::new();
    let mut checker = TypeChecker::new(&mut ctx);
    checker.set_source_file(&source_file);
    checker.check_module(&ast);
    assert!(
        !checker.has_errors(),
        "unexpected type errors: {:?}",
        checker.errors()
    );

    let mut codegen = CodeGenerator::with_source(&ctx, Target::Rust, source_file.source().into());
    codegen
        .generate(&ast)
        .expect("rust codegen should succeed")
        .as_string()
        .expect("generated Rust should be UTF-8")
}

struct RunResult {
    stdout: String,
    exit_code: Option<i32>,
}

/// Compile generated Rust with `rustc`, run the executable, and capture stdout
/// (CRLF-normalized) plus the process exit code.
fn rustc_compile_and_run(name: &str, rust_source: &str) -> RunResult {
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let dir = std::env::temp_dir().join(format!(
        "buildlang_transpile_preservation_{}_{}",
        name,
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("create temp dir");
    let source_path = dir.join("generated.rs");
    let exe_path = dir.join(format!("generated{}", std::env::consts::EXE_SUFFIX));
    fs::write(&source_path, rust_source).expect("write generated Rust");

    let compile = Command::new(&rustc)
        .arg(&source_path)
        .arg("-o")
        .arg(&exe_path)
        .output()
        .expect("invoke rustc");
    assert!(
        compile.status.success(),
        "rustc failed for {name}\nstdout:\n{}\nstderr:\n{}\nsource:\n{}",
        String::from_utf8_lossy(&compile.stdout),
        String::from_utf8_lossy(&compile.stderr),
        rust_source
    );

    let run = Command::new(&exe_path)
        .output()
        .expect("run generated Rust executable");
    let stdout = String::from_utf8_lossy(&run.stdout).replace("\r\n", "\n");
    let result = RunResult {
        stdout,
        exit_code: run.status.code(),
    };
    let _ = fs::remove_dir_all(&dir);
    result
}

/// Run a corpus program through the production C path (`buildc run`) and capture
/// stdout (CRLF-normalized) plus the process exit code.
fn c_backend_run(program_path: &Path) -> RunResult {
    let output = buildc()
        .arg("run")
        .arg(program_path)
        .output()
        .unwrap_or_else(|err| panic!("run buildc run for {}: {}", program_path.display(), err));
    assert!(
        output.status.success(),
        "C backend run failed for {}\nstdout:\n{}\nstderr:\n{}",
        program_path.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    RunResult {
        stdout: String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n"),
        exit_code: output.status.code(),
    }
}

/// Foundation: overflow-safe checked + saturating integer arithmetic
/// (examples/finance/safe_math.bld) runs end-to-end. Exercises the i64-literal,
/// `Option<i64>`-return, `match`-with-if-arms, `&&`/`else if`, and `0 - MAX - 1`
/// (i64::MIN) paths together. `saturating_add` must CLAMP to i64::MAX, not wrap.
#[test]
fn safe_math_checked_and_saturating_arithmetic_runs() {
    if !c_backend_ready() {
        eprintln!("skipping safe_math e2e: no C backend available (buildc doctor)");
        return;
    }
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/finance/safe_math.bld");
    let result = c_backend_run(&path);
    assert_eq!(
        result.stdout,
        "add ok 102599\nadd overflow 1\nsub ok 60\nsat_add 9223372036854775807\nsat_sub 60\n",
        "checked/saturating arithmetic must detect overflow and clamp, not wrap"
    );
}

/// Regression: an unsuffixed integer literal exceeding i32 range must keep its
/// full 64-bit value end-to-end. Previously it was silently truncated to 32 bits
/// in both the type checker (defaulted to an i32 inference var) and the MIR
/// lowering (`unwrap_or(i32)`), so `9223372036854775000` printed as `-808`.
#[test]
fn large_unsuffixed_int_literal_not_truncated_end_to_end() {
    if !c_backend_ready() {
        eprintln!("skipping large-literal e2e: no C backend available (buildc doctor)");
        return;
    }
    let src = "fn main() ~ Console {\n\
               let big = 9223372036854775000;\n\
               let diff = big - 100000;\n\
               println(\"{}\", big);\n\
               println(\"{}\", diff);\n\
               }\n";
    let dir = std::env::temp_dir().join("buildlang_large_literal_regress");
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join("large_literal.bld");
    std::fs::write(&path, src).expect("write large_literal.bld");
    let result = c_backend_run(&path);
    assert_eq!(
        result.stdout, "9223372036854775000\n9223372036854675000\n",
        "a 64-bit literal must not be truncated to 32 bits"
    );
}

/// Regression: unary complement must pick the C operator from the operand type.
/// BuildLang follows Rust: `!` is logical complement on `bool` and bitwise
/// complement on integers, and `~` is always bitwise complement. MIR lowering
/// previously collapsed both `~` and integer `!` onto a single logical-not node,
/// so the C backend emitted `!` for every case. That is a silent wrong answer:
/// `~5` and `!5` printed `0` instead of `-6`, and a `u8` complement lost its
/// width. Each line below pins one path; `!bool` must stay logical.
///   `~x`  (i32 5)  -> bitwise complement -> -6
///   `!x`  (i32 5)  -> bitwise complement -> -6
///   `~y`  (u8 5)   -> width-correct complement -> 250
///   `!b`  (bool)   -> logical complement, else branch -> f
#[test]
fn unary_complement_uses_type_directed_c_operator_end_to_end() {
    if !c_backend_ready() {
        eprintln!("skipping unary-complement e2e: no C backend available (buildc doctor)");
        return;
    }
    let src = "fn main() ~ Console {\n\
               let x: i32 = 5;\n\
               println!(\"{}\", ~x);\n\
               println!(\"{}\", !x);\n\
               let y: u8 = 5;\n\
               println!(\"{}\", ~y);\n\
               let b: bool = true;\n\
               if !b { println!(\"t\"); } else { println!(\"f\"); }\n\
               }\n";
    let dir = std::env::temp_dir().join("buildlang_unary_complement_regress");
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join("unary_complement.bld");
    std::fs::write(&path, src).expect("write unary_complement.bld");
    let result = c_backend_run(&path);
    assert_eq!(
        result.stdout, "-6\n-6\n250\nf\n",
        "`~` and integer `!` must lower to bitwise complement, `!bool` to logical not"
    );
}

/// Regression: the remainder operator on floats must lower to `fmod`/`fmodf`,
/// not to C's `%`. C's `%` rejects floating-point operands, so the previous
/// lowering emitted `(a % b)` and gcc failed to compile the program. That is a
/// fail-closed crash rather than a wrong answer, but a supported operation on a
/// supported type must simply work. Both an f64 and an f32 remainder are pinned
/// so the f32 path (`fmodf`) is covered too; each yields 1.5.
#[test]
fn float_remainder_uses_fmod_end_to_end() {
    if !c_backend_ready() {
        eprintln!("skipping float-remainder e2e: no C backend available (buildc doctor)");
        return;
    }
    let src = "fn main() ~ Console {\n\
               let a: f64 = 5.5;\n\
               let b: f64 = 2.0;\n\
               println!(\"{}\", a % b);\n\
               let c: f32 = 7.5;\n\
               let d: f32 = 2.0;\n\
               println!(\"{}\", c % d);\n\
               }\n";
    let dir = std::env::temp_dir().join("buildlang_float_remainder_regress");
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join("float_remainder.bld");
    std::fs::write(&path, src).expect("write float_remainder.bld");
    let result = c_backend_run(&path);
    assert_eq!(
        result.stdout, "1.5\n1.5\n",
        "float remainder must lower to fmod/fmodf and compute correctly"
    );
}

/// Regression: `break 'label` / `continue 'label` must target the named
/// enclosing loop, not the innermost one. Labeled loops parse (parser support),
/// but MIR lowering previously stored only the innermost loop's blocks and
/// ignored the label, so `break 'outer` from an inner loop compiled to a plain
/// innermost `break`. That is a silent wrong answer: the four programs below
/// print 3/6/3/... instead of 1/0/3/2. Each line pins one path:
///   a: `break 'outer` from inner-for escapes both loops on the first hit -> 1
///   b: `continue 'outer2` restarts the outer loop before any inner count -> 0
///   c: bare `break` still targets the innermost loop (no regression) -> 3
///   d: `break 'w` from an inner-for to an outer *while* (cross loop-kind) -> 2
#[test]
fn labeled_break_continue_target_named_loop_end_to_end() {
    if !c_backend_ready() {
        eprintln!("skipping labeled-loop e2e: no C backend available (buildc doctor)");
        return;
    }
    let src = "fn main() ~ Console {\n\
               let mut a = 0;\n\
               'outer: for i in 0..3 {\n\
               for j in 0..3 { if j == 1 { break 'outer; } a = a + 1; }\n\
               }\n\
               println(\"{}\", a);\n\
               let mut b = 0;\n\
               'outer2: for i in 0..3 {\n\
               for j in 0..3 { if j == 0 { continue 'outer2; } b = b + 1; }\n\
               }\n\
               println(\"{}\", b);\n\
               let mut c = 0;\n\
               for i in 0..3 { for j in 0..3 { if j == 1 { break; } c = c + 1; } }\n\
               println(\"{}\", c);\n\
               let mut d = 0;\n\
               let mut k = 0;\n\
               'w: while k < 3 {\n\
               for j in 0..3 { if j == 2 { break 'w; } d = d + 1; }\n\
               k = k + 1;\n\
               }\n\
               println(\"{}\", d);\n\
               }\n";
    let dir = std::env::temp_dir().join("buildlang_labeled_loops_regress");
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join("labeled_loops.bld");
    std::fs::write(&path, src).expect("write labeled_loops.bld");
    let result = c_backend_run(&path);
    assert_eq!(
        result.stdout, "1\n0\n3\n2\n",
        "break/continue with a label must target the named loop, not the innermost"
    );
}

/// Check-the-check: `break 'x` / `continue 'x` against a label that names no
/// enclosing loop cannot compile, so `check` must reject it at the type stage
/// rather than reporting "no errors" and deferring the failure to codegen. A
/// valid labeled loop must still pass. Runs without a C backend.
#[test]
fn check_rejects_undefined_loop_label() {
    let id = std::process::id();

    // Undefined break label: `'nope` is never declared.
    let bad = std::env::temp_dir().join(format!("buildlang_undef_break_label_{id}.bld"));
    fs::write(&bad, "fn main() {\n    loop {\n        break 'nope;\n    }\n}\n")
        .expect("write undefined-label fixture");
    let output = buildc()
        .arg("check")
        .arg(&bad)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");
    let stdout = output.stdout.clone();
    let _ = fs::remove_file(&bad);
    assert!(
        !output.status.success(),
        "check must fail for an undefined loop label\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt: serde_json::Value =
        serde_json::from_slice(&stdout).expect("stdout should be JSON receipt");
    assert_eq!(receipt["status"], "failed");
    let diagnostics = receipt["diagnostics"]
        .as_array()
        .expect("diagnostics array");
    assert!(
        diagnostics.iter().any(|diag| {
            diag["stage"] == "type"
                && diag["kind"] == "UndefinedLoopLabel"
                && diag["message"]
                    .as_str()
                    .unwrap_or("")
                    .contains("'nope")
        }),
        "expected an UndefinedLoopLabel type diagnostic naming 'nope in {diagnostics:#?}"
    );

    // Undefined continue label, where a different label is in scope.
    let bad_cont = std::env::temp_dir().join(format!("buildlang_undef_cont_label_{id}.bld"));
    fs::write(
        &bad_cont,
        "fn main() {\n    'a: loop {\n        continue 'b;\n    }\n}\n",
    )
    .expect("write undefined-continue-label fixture");
    let cont_out = buildc()
        .arg("check")
        .arg(&bad_cont)
        .output()
        .expect("run buildc check");
    let _ = fs::remove_file(&bad_cont);
    assert!(
        !cont_out.status.success(),
        "check must fail for `continue` to an undefined label"
    );

    // A valid labeled loop must still pass `check`.
    let good = std::env::temp_dir().join(format!("buildlang_valid_label_{id}.bld"));
    fs::write(
        &good,
        "fn main() {\n    'outer: loop {\n        loop {\n            break 'outer;\n        }\n    }\n}\n",
    )
    .expect("write valid-label fixture");
    let good_out = buildc()
        .arg("check")
        .arg(&good)
        .output()
        .expect("run buildc check");
    let _ = fs::remove_file(&good);
    assert!(
        good_out.status.success(),
        "a labeled loop with a resolvable `break` target must pass check\nstderr:\n{}",
        String::from_utf8_lossy(&good_out.stderr)
    );
}

/// Regression: an `Option<i64>`-returning function whose result is matched must
/// compile and run end-to-end. Previously the if-expression result local was
/// typed `int32_t` (the `None` branch defaulted to i32), so the 64-bit Option
/// payload miscompiled (MSVC C2440 "cannot convert from 'Option' to int32_t").
/// `lower_if` now retypes the result local to the aggregate branch type.
#[test]
fn option_i64_return_and_match_runs_end_to_end() {
    if !c_backend_ready() {
        eprintln!("skipping option_i64 e2e: no C backend available (buildc doctor)");
        return;
    }
    let src = "fn checked_add(a: i64, b: i64) -> Option<i64> {\n\
               let max = 9223372036854775807;\n\
               if b > max - a { None } else { Some(a + b) }\n\
               }\n\
               fn main() ~ Console {\n\
               match checked_add(100000, 2599) { Some(v) => println(\"ok {}\", v), None => println(\"of {}\", 0) }\n\
               let big = 9223372036854775000;\n\
               match checked_add(big, 1000) { Some(v) => println(\"ok {}\", v), None => println(\"rej {}\", 1) }\n\
               }\n";
    let dir = std::env::temp_dir().join("buildlang_option_i64_regress");
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join("option_i64.bld");
    std::fs::write(&path, src).expect("write option_i64.bld");
    let result = c_backend_run(&path);
    assert_eq!(
        result.stdout, "ok 102599\nrej 1\n",
        "Option<i64> return + match must run end-to-end with the 64-bit payload intact"
    );
}

/// Regression (silent-wrong-answer): a tuple pattern with literal elements must
/// select the arm whose elements ALL equal the scrutinee. Before the fix,
/// `lower_pattern_test` fell through to a `_ => Ok(bool(true))` catch-all for
/// tuple patterns, so `(1, 2)` "matched" the scrutinee `(3, 4)`, the first arm
/// was taken unconditionally, and this printed "a". A compiler that silently
/// selects the wrong arm is the exact failure the fail-closed rule forbids.
#[test]
fn match_tuple_literal_pattern_selects_equal_arm() {
    if !c_backend_ready() {
        eprintln!("skipping tuple-literal-arm e2e: no C backend available (buildc doctor)");
        return;
    }
    let src = "fn main() ~ Console {\n\
               let pair = (3, 4);\n\
               let r = match pair {\n\
               (1, 2) => \"a\",\n\
               (3, 4) => \"b\",\n\
               _ => \"other\",\n\
               };\n\
               println(\"{}\", r);\n\
               }\n";
    let dir = std::env::temp_dir().join("buildlang_match_tuple_literal");
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join("tuple_literal.bld");
    std::fs::write(&path, src).expect("write tuple_literal.bld");
    let result = c_backend_run(&path);
    assert_eq!(
        result.stdout, "b\n",
        "a tuple pattern must test every element for equality, not silently always match"
    );
}

/// Regression (invalid C): scalar-bodied arms over a tuple scrutinee must yield
/// a scalar result local. Before the fix, `lower_match` fell back to the
/// scrutinee type (a tuple) for the result, emitting C that assigned an int
/// (`100`/`200`) to a `Tuple_i32_i32` local, which gcc rejects as incompatible
/// types. The program must compile and print the selected scalar.
#[test]
fn match_tuple_scalar_result_over_tuple_scrutinee() {
    if !c_backend_ready() {
        eprintln!("skipping tuple-scalar-result e2e: no C backend available (buildc doctor)");
        return;
    }
    let src = "fn main() ~ Console {\n\
               let pair = (3, 4);\n\
               let r = match pair {\n\
               (1, 2) => 100,\n\
               (3, 4) => 200,\n\
               _ => 999,\n\
               };\n\
               println(\"{}\", r);\n\
               }\n";
    let dir = std::env::temp_dir().join("buildlang_match_tuple_scalar");
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join("tuple_scalar.bld");
    std::fs::write(&path, src).expect("write tuple_scalar.bld");
    let result = c_backend_run(&path);
    assert_eq!(
        result.stdout, "200\n",
        "scalar match arms over a tuple scrutinee must produce a scalar result, not an aggregate"
    );
}

/// Regression: a nested tuple pattern tests every leaf. `((1, 2), 9)` must not
/// match `((1, 3), 9)` (the inner `2 != 3` fails that arm). Before the fix the
/// tuple arm always matched, so the first (wrong) arm was taken.
#[test]
fn match_nested_tuple_pattern_tests_inner_elements() {
    if !c_backend_ready() {
        eprintln!("skipping nested-tuple e2e: no C backend available (buildc doctor)");
        return;
    }
    let src = "fn main() ~ Console {\n\
               let p = ((1, 2), 9);\n\
               let r = match p {\n\
               ((1, 3), 9) => \"wrongA\",\n\
               ((1, 2), 9) => \"right\",\n\
               _ => \"other\",\n\
               };\n\
               println(\"{}\", r);\n\
               }\n";
    let dir = std::env::temp_dir().join("buildlang_match_nested_tuple");
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join("nested_tuple.bld");
    std::fs::write(&path, src).expect("write nested_tuple.bld");
    let result = c_backend_run(&path);
    assert_eq!(
        result.stdout, "right\n",
        "a nested tuple pattern must test inner elements, not silently always match"
    );
}

/// Regression (undeclared C variables): tuple-pattern element bindings must be
/// emitted. `(a, b) => a + b` prints 15; the nested `((a, b), c)` binds all
/// three (prints 12); the mixed `(1, x)` tests the literal `1` and binds `x`
/// (prints 20). Before the binder existed, `lower_match` had no tuple arm in
/// its binding section, so the emitted C referenced undeclared `a`/`b`.
#[test]
fn match_tuple_pattern_binds_elements() {
    if !c_backend_ready() {
        eprintln!("skipping tuple-binding e2e: no C backend available (buildc doctor)");
        return;
    }
    let src = "fn main() ~ Console {\n\
               let p = (7, 8);\n\
               match p { (a, b) => println(\"{}\", a + b), }\n\
               let q = ((3, 4), 5);\n\
               match q { ((a, b), c) => println(\"{}\", a + b + c), }\n\
               let m = (1, 20);\n\
               match m { (1, x) => println(\"{}\", x), _ => println(\"{}\", 0), }\n\
               }\n";
    let dir = std::env::temp_dir().join("buildlang_match_tuple_binds");
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join("tuple_binds.bld");
    std::fs::write(&path, src).expect("write tuple_binds.bld");
    let result = c_backend_run(&path);
    assert_eq!(
        result.stdout, "15\n12\n20\n",
        "tuple, nested-tuple, and mixed literal/binding patterns must bind their element variables"
    );
}

/// Regression: a match guard can reference tuple-pattern bindings. The binder
/// runs in the guard block before the guard expression, so `(a, b) if a < b`
/// evaluates `a < b` with `a`/`b` in scope. `(3, 4)` takes the guarded arm and
/// prints 34 (`a * 10 + b`); if the binder did not run before the guard the
/// guard would reference undeclared variables.
#[test]
fn match_tuple_guard_references_bindings() {
    if !c_backend_ready() {
        eprintln!("skipping tuple-guard e2e: no C backend available (buildc doctor)");
        return;
    }
    let src = "fn main() ~ Console {\n\
               let p = (3, 4);\n\
               let r = match p {\n\
               (a, b) if a < b => a * 10 + b,\n\
               (a, b) => a + b,\n\
               };\n\
               println(\"{}\", r);\n\
               }\n";
    let dir = std::env::temp_dir().join("buildlang_match_tuple_guard");
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join("tuple_guard.bld");
    std::fs::write(&path, src).expect("write tuple_guard.bld");
    let result = c_backend_run(&path);
    assert_eq!(
        result.stdout, "34\n",
        "a guard clause must see the tuple pattern's bound elements"
    );
}

/// Regression (fail-closed): a slice pattern in `match` is not yet supported in
/// codegen. It must be REJECTED with a diagnostic, never compiled as an
/// always-true test. Before the fix the `_ => Ok(bool(true))` catch-all made
/// `[1, 2, 3]` silently always match, so a non-matching array would take the
/// wrong arm. The `--target c` codegen path needs no C compiler, so this proves
/// the fail-closed diagnostic directly.
#[test]
fn match_slice_pattern_fails_closed() {
    let dir = std::env::temp_dir().join(format!(
        "buildlang_match_slice_closed_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let fixture = dir.join("slice_pattern.bld");
    fs::write(
        &fixture,
        "fn main() ~ Console {\n\
         let a = [1, 2, 3];\n\
         let r = match a {\n\
         [1, 2, 3] => \"a\",\n\
         _ => \"other\",\n\
         };\n\
         println(\"{}\", r);\n\
         }\n",
    )
    .expect("write slice-pattern fixture");
    let out_c = dir.join("slice_pattern.c");
    let output = buildc()
        .arg(&fixture)
        .args(["--target", "c", "-o"])
        .arg(&out_c)
        .output()
        .expect("run buildc --target c");
    let _ = fs::remove_file(&fixture);
    assert!(
        !output.status.success(),
        "a slice pattern in match must fail closed, not compile as always-true\nstderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.to_lowercase().contains("slice pattern"),
        "diagnostic should name the unsupported slice pattern:\n{}",
        stderr
    );
}

/// I1: Unicode arithmetic-operator aliases lex to their ASCII counterparts and
/// run end-to-end. `×` (U+00D7), `·` (U+00B7), `∙` (U+2219) alias `*`; `÷`
/// (U+00F7) aliases `/`; `−` (U+2212, the minus sign, NOT ASCII hyphen) aliases
/// `-`. Previously these raised `LexerErrorKind::UnexpectedChar`. Computes
/// `6 × 7 = 42`, `84 ÷ 2 = 42`, `10 − 3 = 7`.
#[test]
fn unicode_math_operators_run_end_to_end() {
    if !c_backend_ready() {
        eprintln!("skipping unicode-math e2e: no C backend available (buildc doctor)");
        return;
    }
    let src = "fn main() ~ Console {\n\
               let a = 6 \u{00D7} 7;\n\
               let b = 84 \u{00F7} 2;\n\
               let c = 10 \u{2212} 3;\n\
               println(\"{} {} {}\", a, b, c);\n\
               }\n";
    let dir = std::env::temp_dir().join("buildlang_unicode_math_ops");
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join("unicode_math.bld");
    std::fs::write(&path, src).expect("write unicode_math.bld");
    let result = c_backend_run(&path);
    assert_eq!(
        result.stdout, "42 42 7\n",
        "Unicode math operators must alias to * / - and run end-to-end"
    );
}

/// I2: the `**` power operator runs end-to-end through the C backend.
/// `2 ** 10 == 1024` exercises the new `StarStar` token -> `BinOp::Pow` wiring
/// (Pow was already typed and lowered to `pow(l, r)`; I2 only wires lexer +
/// parser). `2 ** 3 ** 2` must be `512` (right-associative `2 ** (3 ** 2)`),
/// NOT `64` (`(2 ** 3) ** 2`), proving right-associativity end-to-end.
#[test]
fn power_operator_runs_end_to_end() {
    if !c_backend_ready() {
        eprintln!("skipping power-operator e2e: no C backend available (buildc doctor)");
        return;
    }
    let src = "fn main() ~ Console {\n\
               println(\"{}\", 2 ** 10);\n\
               println(\"{}\", 2 ** 3 ** 2);\n\
               }\n";
    let dir = std::env::temp_dir().join("buildlang_power_operator");
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join("power.bld");
    std::fs::write(&path, src).expect("write power.bld");
    let result = c_backend_run(&path);
    assert_eq!(
        result.stdout, "1024\n512\n",
        "`**` must compute power and be right-associative (2 ** 3 ** 2 == 512)"
    );
}

/// I2 documented semantics: unary minus binds LOOSER than `**`, so `-2 ** 2`
/// means `-(2 ** 2) == -4`, NOT `(-2) ** 2 == 4`. This matches the
/// Julia/Python convention `-a**b == -(a**b)`. The binding power `bp::POWER`
/// is set equal to `bp::PREFIX` so the power operator binds inside a leading
/// unary minus; the parser-level test `neg_double_star_binds_power_inside_neg`
/// asserts the corresponding AST shape (`Neg(Pow(2, 2))`).
#[test]
fn neg_power_semantics() {
    if !c_backend_ready() {
        eprintln!("skipping neg-power e2e: no C backend available (buildc doctor)");
        return;
    }
    let src = "fn main() ~ Console {\n\
               println(\"{}\", -2 ** 2);\n\
               }\n";
    let dir = std::env::temp_dir().join("buildlang_neg_power");
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join("neg_power.bld");
    std::fs::write(&path, src).expect("write neg_power.bld");
    let result = c_backend_run(&path);
    assert_eq!(
        result.stdout, "-4\n",
        "-2 ** 2 must be -(2 ** 2) == -4 (unary minus binds looser than **)"
    );
}

/// I4: elementwise broadcasting operators `.+ .- .* ./` over fixed-size
/// `Array<T,N>` run end-to-end through the C backend. Array-array `a .+ b`
/// desugars to per-element scalar adds; scalar broadcast on the right
/// (`a .* 2.0`) and the left (`2.0 .+ a`) reuse the scalar for every element.
/// f64 prints via C `%g`, so `11.0` prints as `11`.
#[test]
fn array_broadcast_runs_end_to_end() {
    if !c_backend_ready() {
        eprintln!("skipping array-broadcast e2e: no C backend available (buildc doctor)");
        return;
    }
    let src = "fn main() ~ Console {\n\
               let a = [1.0, 2.0, 3.0];\n\
               let b = [10.0, 20.0, 30.0];\n\
               let sum = a .+ b;\n\
               println(\"{} {} {}\", sum[0], sum[1], sum[2]);\n\
               let scaled = a .* 2.0;\n\
               println(\"{} {} {}\", scaled[0], scaled[1], scaled[2]);\n\
               let shifted = 2.0 .+ a;\n\
               println(\"{} {} {}\", shifted[0], shifted[1], shifted[2]);\n\
               }\n";
    let dir = std::env::temp_dir().join("buildlang_array_broadcast");
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join("array_broadcast.bld");
    std::fs::write(&path, src).expect("write array_broadcast.bld");
    let result = c_backend_run(&path);
    assert_eq!(
        result.stdout, "11 22 33\n2 4 6\n3 4 5\n",
        "`.+` must add elementwise, `.* 2.0` scale each element, `2.0 .+ a` broadcast scalar-left"
    );
}

/// I4: broadcasting two arrays of DIFFERENT compile-time lengths is a
/// compile-time type error. `[1.0, 2.0] .+ [1.0, 2.0, 3.0]` (lengths 2 vs 3)
/// must be REJECTED by `buildc check` with a length-related diagnostic; the
/// length is carried in the `Array<T,N>` type so the mismatch is caught before
/// codegen.
#[test]
fn array_broadcast_length_mismatch_is_rejected() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_array_broadcast_mismatch_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        "fn main() ~ Console {\n\
         let a = [1.0, 2.0];\n\
         let b = [1.0, 2.0, 3.0];\n\
         let c = a .+ b;\n\
         println(\"{}\", c[0]);\n\
         }\n",
    )
    .expect("write array-broadcast mismatch fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run buildc check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "broadcasting arrays of different lengths must fail buildc check"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.to_lowercase().contains("length"),
        "diagnostic should mention the length mismatch:\n{}",
        stderr
    );
}

/// I4: the subtraction and division broadcasting operators `.-` and `./` run
/// end-to-end alongside the array-array and scalar-broadcast forms already
/// covered by `array_broadcast_runs_end_to_end`. `[10,20,30] .- [1,2,3]` yields
/// `9 18 27`; `[10,20] ./ [2,4]` yields `5 5`; scalar-right `[10,20] ./ 2.0`
/// yields `5 10`. f64 prints via C `%g`, so whole numbers render without `.0`.
#[test]
fn array_broadcast_sub_div_run_end_to_end() {
    if !c_backend_ready() {
        eprintln!("skipping array-broadcast sub/div e2e: no C backend available (buildc doctor)");
        return;
    }
    // Each result is printed from its own helper: buildlang's println
    // format-arg checker fixes the placeholder count from the first println in
    // a function body, so a 3-placeholder and a 2-placeholder println cannot
    // share one function. Separate helpers keep the counts independent and also
    // exercise passing a broadcast-result array into a function by value.
    let src = "fn print3(a: [f64; 3]) ~ Console {\n\
               println(\"{} {} {}\", a[0], a[1], a[2]);\n\
               }\n\
               fn print2(p: [f64; 2]) ~ Console {\n\
               println(\"{} {}\", p[0], p[1]);\n\
               }\n\
               fn main() ~ Console {\n\
               let a = [10.0, 20.0, 30.0];\n\
               let b = [1.0, 2.0, 3.0];\n\
               let diff = a .- b;\n\
               print3(diff);\n\
               let p = [10.0, 20.0];\n\
               let q = [2.0, 4.0];\n\
               let quot = p ./ q;\n\
               print2(quot);\n\
               let half = p ./ 2.0;\n\
               print2(half);\n\
               }\n";
    let dir = std::env::temp_dir().join("buildlang_array_broadcast_subdiv");
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join("array_broadcast_subdiv.bld");
    std::fs::write(&path, src).expect("write array_broadcast_subdiv.bld");
    let result = c_backend_run(&path);
    assert_eq!(
        result.stdout, "9 18 27\n5 5\n5 10\n",
        "`.-` must subtract elementwise, `./` divide elementwise, `./ 2.0` broadcast scalar-right"
    );
}

/// Typed `Array<T,N>` end-to-end kernel: a real numerical routine written with
/// fixed-size array types in the FUNCTION SIGNATURE (params AND return), not just
/// inline literals. This is the "write real array math naturally" surface — an
/// axpy (`a*x + y`) kernel whose shapes are declared `[f64; 3]` and whose body
/// composes scalar-left broadcast (`a .* x`) with an array-array add (`.+ y`).
/// The compile-time size flows through the call boundary: the caller's arrays,
/// the parameter types, the body's broadcast result, and the declared return
/// type must all agree at `[f64; 3]`. Proves the size-tracked type survives being
/// passed into and returned out of a function, then lowers and runs.
#[test]
fn typed_array_kernel_signature_runs_end_to_end() {
    if !c_backend_ready() {
        eprintln!("skipping typed-array kernel e2e: no C backend available (buildc doctor)");
        return;
    }
    let src = "fn axpy(a: f64, x: [f64; 3], y: [f64; 3]) -> [f64; 3] {\n\
               let scaled = a .* x;\n\
               scaled .+ y\n\
               }\n\
               fn main() ~ Console {\n\
               let x = [1.0, 2.0, 3.0];\n\
               let y = [10.0, 20.0, 30.0];\n\
               let r = axpy(2.0, x, y);\n\
               println(\"{} {} {}\", r[0], r[1], r[2]);\n\
               }\n";
    let dir = std::env::temp_dir().join("buildlang_typed_array_kernel");
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join("typed_array_kernel.bld");
    std::fs::write(&path, src).expect("write typed_array_kernel.bld");
    let result = c_backend_run(&path);
    assert_eq!(
        result.stdout, "12 24 36\n",
        "axpy(2, [1,2,3], [10,20,30]) must broadcast through a `[f64; 3]` signature: 2*1+10, 2*2+20, 2*3+30"
    );
}

/// Compile-time size tracking through a FUNCTION CALL boundary: passing a
/// `[f64; 4]` argument where the parameter is declared `[f64; 3]` is a COMPILE
/// ERROR, not a runtime panic or a silent truncation. This is the can-it-FAIL
/// proof for the kernel-writing surface: a shape mismatch discovered at the call
/// site (not just in a literal `.+` expression) must be rejected by `buildc
/// check` with a length diagnostic before any code is generated.
#[test]
fn typed_array_kernel_arg_length_mismatch_is_rejected() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_typed_array_arg_mismatch_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        "fn scale(x: [f64; 3]) -> [f64; 3] {\n\
         x .* 2.0\n\
         }\n\
         fn main() ~ Console {\n\
         let wide = [1.0, 2.0, 3.0, 4.0];\n\
         let r = scale(wide);\n\
         println(\"{}\", r[0]);\n\
         }\n",
    )
    .expect("write typed-array arg-mismatch fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run buildc check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "passing a `[f64; 4]` where a `[f64; 3]` parameter is declared must fail buildc check"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.to_lowercase().contains("length"),
        "diagnostic should name the array length mismatch at the call site:\n{}",
        stderr
    );
    assert!(
        !stderr.to_lowercase().contains("panic"),
        "shape mismatch must be a compile-time diagnostic, never a runtime panic:\n{}",
        stderr
    );
}

/// Compile-time size tracking through a RETURN type: a body that produces a
/// `[f64; 3]` value declared to return `[f64; 2]` is a COMPILE ERROR. The
/// returned array's compile-time N is checked against the declared return
/// shape, so a kernel that accidentally changes length is caught by `buildc
/// check` rather than emitting a mis-sized array at codegen.
#[test]
fn typed_array_kernel_return_length_mismatch_is_rejected() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_typed_array_ret_mismatch_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        "fn widen(x: [f64; 3]) -> [f64; 2] {\n\
         x .+ 1.0\n\
         }\n\
         fn main() ~ Console {\n\
         let r = widen([1.0, 2.0, 3.0]);\n\
         println(\"{}\", r[0]);\n\
         }\n",
    )
    .expect("write typed-array return-mismatch fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run buildc check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "returning a `[f64; 3]` body from a `-> [f64; 2]` signature must fail buildc check"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("[f64; 3]") && stderr.contains("[f64; 2]"),
        "diagnostic should name both the produced `[f64; 3]` and expected `[f64; 2]` shapes:\n{}",
        stderr
    );
}

/// Regression: function-style `println(...)` had its argument count fixed by the
/// FIRST `println` in a function body (its shared type-var binding was
/// monomorphized to that call's arity), so a later `println` with a different
/// placeholder count failed to type-check with a spurious `ArityMismatch`
/// ("expected 4 arguments, found 3"). All three differing counts must now compile
/// and run in one function body, and the Console effect stays required.
#[test]
fn println_varying_placeholder_counts_run_end_to_end() {
    if !c_backend_ready() {
        eprintln!("skipping println-arity e2e: no C backend available (buildc doctor)");
        return;
    }
    let src = "fn main() ~ Console {\n\
               let a = 1;\n\
               let b = 2;\n\
               let c = 3;\n\
               println(\"{} {} {}\", a, b, c);\n\
               println(\"{} {}\", a, b);\n\
               println(\"{}\", c);\n\
               }\n";
    let dir = std::env::temp_dir().join("buildlang_println_arity");
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join("println_arity.bld");
    std::fs::write(&path, src).expect("write println_arity.bld");
    let result = c_backend_run(&path);
    assert_eq!(
        result.stdout, "1 2 3\n1 2\n3\n",
        "function-style println must accept any placeholder count across one function body"
    );
}

/// Regression: the type checker loaded and registered an external `mod foo;`
/// module in BOTH its collect pass and its check pass, so a self-recursive (or
/// intra-module-calling) function in a LOCAL module registered duplicate dispatch
/// candidates and multiple dispatch reported a false "equally-specific candidates"
/// ambiguity on every call to it. Registering an identical parameter-type
/// signature is now deduped in the multi-method registry. This program has a
/// local `helper.bld` with a self-recursive `fact`; it must compile and run.
#[test]
fn local_module_self_recursive_fn_no_false_ambiguity() {
    if !c_backend_ready() {
        eprintln!("skipping local-module self-recursion e2e: no C backend available");
        return;
    }
    let dir = std::env::temp_dir().join("buildlang_local_mod_selfrec");
    std::fs::create_dir_all(&dir).expect("create temp dir");
    std::fs::write(
        dir.join("helper.bld"),
        "fn fact(n: i32) -> i32 { if n <= 1 { 1 } else { n * fact(n - 1) } }\n",
    )
    .expect("write helper.bld");
    let main_path = dir.join("main.bld");
    std::fs::write(
        &main_path,
        "mod helper;\nfn main() ~ Console { let x = fact(5); println(\"{}\", x); }\n",
    )
    .expect("write main.bld");
    let result = c_backend_run(&main_path);
    assert_eq!(
        result.stdout, "120\n",
        "a self-recursive fn in a local module must not be a false dispatch ambiguity"
    );
}

/// Regression: a function imported via `mod foo;` failed to resolve with
/// "undefined variable" when CALLED inside a loop body (while/for), match arm,
/// method-call argument, or other context the import call-rewriter did not
/// descend into. The rewriter now walks ALL expression contexts, so a bare
/// imported call is rewritten to its prefixed name everywhere.
#[test]
fn imported_module_fn_resolves_inside_loop_body() {
    if !c_backend_ready() {
        eprintln!("skipping imported-fn-in-loop e2e: no C backend available");
        return;
    }
    let dir = std::env::temp_dir().join("buildlang_imported_fn_in_loop");
    std::fs::create_dir_all(&dir).expect("create temp dir");
    std::fs::write(
        dir.join("helper.bld"),
        "fn square(n: i32) -> i32 { n * n }\n",
    )
    .expect("write helper.bld");
    let main_path = dir.join("main.bld");
    std::fs::write(
        &main_path,
        "mod helper;\nfn main() ~ Console {\n    \
         let mut i: i32 = 1;\n    let mut acc: i32 = 0;\n    \
         while i <= 3 {\n        acc = acc + square(i);\n        i = i + 1;\n    }\n    \
         println(\"{}\", acc);\n}\n",
    )
    .expect("write main.bld");
    let result = c_backend_run(&main_path);
    assert_eq!(
        result.stdout, "14\n",
        "an imported module fn must resolve when called inside a loop body"
    );
}

/// Regression: the module-call rewriter must be scope-aware. A fn-pointer
/// PARAMETER named identically to an imported module function, called inside a
/// loop body, must resolve to the PARAMETER -- not be silently rewritten to the
/// module function (a wrong-callee miscompile). Here `square` is a fn-pointer
/// param bound to `triple`; the module `helper` also exports `square`. Applying
/// the param twice to 2 gives triple(triple(2)) = 18; the pre-fix bug rewrote
/// `square(acc)` to `helper_square(acc)` and produced 16.
#[test]
fn shadowing_fn_pointer_param_is_not_rewritten_to_module_fn() {
    if !c_backend_ready() {
        eprintln!("skipping shadow-rewrite e2e: no C backend available");
        return;
    }
    let dir = std::env::temp_dir().join("buildlang_shadow_fn_ptr_param");
    std::fs::create_dir_all(&dir).expect("create temp dir");
    std::fs::write(
        dir.join("helper.bld"),
        "pub fn square(x: i32) -> i32 { x * x }\n",
    )
    .expect("write helper.bld");
    let main_path = dir.join("main.bld");
    std::fs::write(
        &main_path,
        "mod helper;\n\
         fn triple(x: i32) -> i32 { x + x + x }\n\
         fn run(square: fn(i32) -> i32, x: i32) -> i32 {\n    \
         let mut acc = x;\n    let mut i = 0;\n    \
         while i < 2 {\n        acc = square(acc);\n        i = i + 1;\n    }\n    \
         acc\n}\n\
         fn main() ~ Console {\n    println(\"{}\", run(triple, 2));\n}\n",
    )
    .expect("write main.bld");
    let result = c_backend_run(&main_path);
    assert_eq!(
        result.stdout, "18\n",
        "a fn-pointer parameter shadowing an imported fn must call the parameter, not the module fn"
    );
}

/// I4 (review FIX A): broadcasting arrays whose element type is NOT numeric is a
/// compile-time type error. Broadcasting is defined only for integer/float
/// elements; `["a", "b"] .+ ["c", "d"]` (string elements) must be REJECTED by
/// `buildc check` with a type diagnostic, not accepted and then leaked as a raw
/// C backend error (`error C2088: ... operator '+' cannot be applied to ...
/// BuildString`) at codegen.
#[test]
fn array_broadcast_nonnumeric_element_is_rejected() {
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_array_broadcast_nonnumeric_{}.bld",
        std::process::id()
    ));
    fs::write(
        &fixture,
        "fn main() ~ Console {\n\
         let a = [\"a\", \"b\"];\n\
         let b = [\"c\", \"d\"];\n\
         let c = a .+ b;\n\
         println(\"{}\", c[0]);\n\
         }\n",
    )
    .expect("write array-broadcast non-numeric fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run buildc check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "broadcasting non-numeric (string) element arrays must fail buildc check"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("C2088") && !stderr.to_lowercase().contains("c compilation"),
        "rejection must be a clean type diagnostic, not a leaked C compiler error:\n{}",
        stderr
    );
}

/// Executable witness of the transpile-preservation criterion: for every
/// Rust-supported corpus program, lowering the same source through the C
/// backend and through the Rust backend must produce byte-identical stdout and
/// equal exit status. This asserts the two backends AGREE WITH EACH OTHER, not
/// merely that each reproduces its own expected stdout.
#[test]
fn transpile_preservation_c_and_rust_backends_agree_on_stdout() {
    if !c_backend_ready() {
        eprintln!(
            "skipping transpile-preservation cross-check: no C backend available (buildc doctor)"
        );
        return;
    }
    if !rustc_available() {
        eprintln!("skipping transpile-preservation cross-check: rustc not available");
        return;
    }

    let corpus = load_preservation_corpus();
    let mut cross_checked = 0usize;

    for program in &corpus.programs {
        // Honest scope: only programs the Rust backend can execute are
        // cross-checked. Programs without a `generated_rust_runs_for_*`
        // execution test are covered by the C path alone.
        if !program
            .rust_execution_test
            .starts_with("generated_rust_runs_for_")
        {
            continue;
        }

        let program_path = semantic_corpus_root().join(&program.path);
        let source = fs::read_to_string(&program_path)
            .unwrap_or_else(|err| panic!("read corpus program {}: {}", program.id, err));

        let c_result = c_backend_run(&program_path);
        let rust_source = lower_source_to_rust(&source);
        let rust_result = rustc_compile_and_run(&program.id, &rust_source);

        assert_eq!(
            c_result.stdout, rust_result.stdout,
            "transpile-preservation DIVERGENCE for {}: C and Rust backends disagree on stdout\n\
             C stdout:    {:?}\n\
             Rust stdout: {:?}",
            program.id, c_result.stdout, rust_result.stdout
        );
        assert_eq!(
            c_result.exit_code, rust_result.exit_code,
            "transpile-preservation DIVERGENCE for {}: C and Rust backends disagree on exit status\n\
             C exit:    {:?}\n\
             Rust exit: {:?}",
            program.id, c_result.exit_code, rust_result.exit_code
        );

        cross_checked += 1;
    }

    assert!(
        cross_checked > 0,
        "transpile-preservation harness cross-checked zero programs; \
         the Rust-supported corpus subset should be non-empty"
    );
    eprintln!(
        "transpile-preservation: C and Rust backends agree on stdout + exit status \
         for {cross_checked} corpus program(s)"
    );
}

// =============================================================================
// STATIC MULTIPLE DISPATCH (Pillar A)
// =============================================================================

/// Write a `.bld` fixture to a unique temp path (labelled) and return it.
fn write_dispatch_fixture(label: &str, src: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "buildlang_dispatch_{}_{}.bld",
        label,
        std::process::id()
    ));
    fs::write(&path, src).unwrap_or_else(|e| panic!("write dispatch fixture {label}: {e}"));
    path
}

#[test]
fn dispatch_selects_overload_by_argument_type_tuple() {
    if !c_backend_ready() {
        eprintln!("skipping multiple-dispatch run test because no C backend is available");
        return;
    }

    // Three same-name `add` overloads whose bodies differ so the printed
    // result reveals which method was selected: i32 adds, f64 multiplies.
    // Plus a two-argument `combo` that dispatches on the SECOND argument's
    // type (i32,i32) vs (i32,f64), proving dispatch is not receiver-only.
    let src = r#"
fn add(a: i32, b: i32) -> i32 { a + b }
fn add(a: f64, b: f64) -> f64 { a * b }

fn combo(a: i32, b: i32) -> i32 { a + b }
fn combo(a: i32, b: f64) -> i32 { a * 100 }

fn main() ~ Console {
    let w: i32 = add(3, 5);
    let x: f64 = add(2.5, 4.0);
    let y: i32 = combo(7, 9);
    let z: i32 = combo(7, 9.0);
    println!("{}", w);
    println!("{}", x);
    println!("{}", y);
    println!("{}", z);
}
"#;
    let fixture = write_dispatch_fixture("run_select", src);

    let output = buildc()
        .arg("run")
        .arg(&fixture)
        .output()
        .expect("run buildc run on dispatch fixture");
    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "dispatch program should run successfully\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    // add(3,5) -> i32 (a+b) = 8 ; add(2.5,4.0) -> f64 (a*b) = 10 ;
    // combo(7,9) -> (i32,i32) (a+b) = 16 ; combo(7,9.0) -> (i32,f64) (a*100) = 700
    let want = "8\n10\n16\n700\n";
    assert!(
        stdout.contains(want),
        "dispatch stdout should show each overload's result; want to contain {want:?}, got:\n{}",
        stdout
    );
}

#[test]
fn dispatch_reports_no_matching_method() {
    // add(i32,i32) and add(f64,f64) but the call passes strings: no candidate
    // matches the argument tuple, so `check` fails with NoMatchingMethod.
    let src = r#"
fn add(a: i32, b: i32) -> i32 { a + b }
fn add(a: f64, b: f64) -> f64 { a * b }

fn main() {
    let s: str = "x";
    add(s, s);
}
"#;
    let fixture = write_dispatch_fixture("no_match", src);

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run buildc check on no-match fixture");
    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "a call with no matching overload should fail type checking"
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("no method `add` matches"),
        "check should report NoMatchingMethod:\n{}",
        combined
    );
}

#[test]
fn dispatch_reports_ambiguous_method() {
    // add(i32, T) vs add(T, i32) with a call add(i32, i32): both candidates are
    // equally specific (one exact + one generic each) and incomparable, so the
    // call is ambiguous.
    let src = r#"
fn add<T>(a: i32, b: T) -> i32 { a }
fn add<T>(a: T, b: i32) -> i32 { b }

fn main() {
    add(1, 2);
}
"#;
    let fixture = write_dispatch_fixture("ambiguous", src);

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run buildc check on ambiguous fixture");
    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "an equally-specific overload pair should fail type checking as ambiguous"
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("is ambiguous"),
        "check should report AmbiguousMethod:\n{}",
        combined
    );
}

#[test]
fn dispatch_single_definition_names_are_unaffected() {
    // A non-overloaded program still checks cleanly: the single-candidate fast
    // path must behave exactly as before (backward compatibility).
    let src = r#"
fn double(x: i32) -> i32 { x * 2 }

fn main() ~ Console {
    println!("{}", double(21));
}
"#;
    let fixture = write_dispatch_fixture("single_def", src);

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run buildc check on single-def fixture");
    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "a non-overloaded program should check cleanly\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn dispatch_generic_and_concrete_overload_compose() {
    // Regression: a name that mixes a GENERIC def and a CONCRETE def of the same
    // name must have the checker and codegen agree on the selected overload.
    // Previously codegen skipped generic siblings when counting overloads, so the
    // multi-dispatch path was dead and it emitted the concrete C symbol for BOTH
    // calls -> a `h(i32)` vs `h(BuildString)` name collision (invalid C, C2440).
    //
    //   h(3)       -> concrete `h(i32)` wins (concrete beats generic) -> "3"
    //   h("hello") -> only the generic `h<T>` matches -> monomorphized -> "hello"
    if !c_backend_ready() {
        eprintln!("skipping generic+concrete dispatch run test: no C backend available");
        return;
    }
    let src = r#"
fn h<T>(a: T) -> T { a }
fn h(a: i32) -> i32 { a }

fn main() ~ Console {
    let x: i32 = h(3);
    let y: str = h("hello");
    println!("{}", x);
    println!("{}", y);
}
"#;
    let fixture = write_dispatch_fixture("generic_concrete", src);

    let output = buildc()
        .arg("run")
        .arg(&fixture)
        .output()
        .expect("run buildc run on generic+concrete dispatch fixture");
    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "generic+concrete overload program should compile and run\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    // Concrete `h(i32)` returns 3; generic `h<T>` monomorphized for str returns
    // "hello". Both must print, proving each call selected the correct sibling.
    assert!(
        stdout.contains("3\n") && stdout.contains("hello"),
        "each call should select the correct overload (concrete `3`, generic `hello`); got:\n{}",
        stdout
    );
}

#[test]
fn dispatch_generic_only_call_still_monomorphizes() {
    // Guard the reordered call-lowering path: a generic-ONLY name (no concrete
    // sibling, so NOT overloaded) must still monomorphize normally. This exercises
    // the fall-through from the multi-dispatch check to the plain generic path.
    if !c_backend_ready() {
        eprintln!("skipping generic-only dispatch run test: no C backend available");
        return;
    }
    let src = r#"
fn identity<T>(a: T) -> T { a }

fn main() ~ Console {
    let x: i32 = identity(42);
    let z: i32 = identity(7);
    println!("{}", x);
    println!("{}", z);
}
"#;
    let fixture = write_dispatch_fixture("generic_only", src);

    let output = buildc()
        .arg("run")
        .arg(&fixture)
        .output()
        .expect("run buildc run on generic-only fixture");
    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "generic-only program should compile and run\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    assert!(
        stdout.contains("42\n") && stdout.contains("7\n"),
        "generic-only monomorphization should print both results; got:\n{}",
        stdout
    );
}

#[test]
fn linalg_module_runs_end_to_end() {
    // I3: the `linalg` stdlib module provides free functions over the dynamic
    // Vec<f64> (vec_dot / vec_sum / vec_norm). This exercises the full pipeline:
    // `mod linalg;` resolves from the repo-root stdlib, its free functions are
    // prefix-mangled and callable by bare name, and the f64 vector builtins plus
    // `sqrt` lower to the C backend. Floats print via C `printf("%g", ...)`, so
    // whole-number f64 values render without a decimal point (32.0 -> "32").
    if !c_backend_ready() {
        eprintln!("skipping linalg module e2e test because no C backend is available");
        return;
    }
    // Bind each result to a `let` before printing: this is the standard
    // buildlang idiom for stdlib-imported calls (see 100_inline_modules.bld /
    // 101_calibrate_pipeline.bld). The import rewriter mangles bare imported
    // calls in statement/binding position, and `println!("{}", var)` prints
    // the already-computed value.
    let src = r#"
mod core;
mod math;
mod linalg;

fn main() ~ Console {
    let mut a = vec_new_f64();
    vec_push_f64(a, 1.0);
    vec_push_f64(a, 2.0);
    vec_push_f64(a, 3.0);

    let mut b = vec_new_f64();
    vec_push_f64(b, 4.0);
    vec_push_f64(b, 5.0);
    vec_push_f64(b, 6.0);

    let dot = vec_dot(a, b);
    let sum = vec_sum(a);

    let mut c = vec_new_f64();
    vec_push_f64(c, 3.0);
    vec_push_f64(c, 4.0);
    let norm = vec_norm(c);

    println!("{}", dot);
    println!("{}", sum);
    println!("{}", norm);
}
"#;
    let fixture = write_dispatch_fixture("linalg_module", src);

    let output = buildc()
        .arg("run")
        .arg(&fixture)
        .output()
        .expect("run buildc run on linalg module fixture");
    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "linalg module program should compile and run\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    // vec_dot([1,2,3],[4,5,6]) = 4+10+18 = 32; vec_sum([1,2,3]) = 6;
    // vec_norm([3,4]) = sqrt(9+16) = sqrt(25) = 5. `%g` drops the trailing `.0`.
    assert_eq!(
        stdout, "32\n6\n5\n",
        "linalg dot/sum/norm should print 32, 6, 5; got:\n{}",
        stdout
    );
}

#[test]
fn linalg_elementwise_and_scalar_builders_run_end_to_end() {
    // I3 (review FIX C): the `linalg` stdlib module's elementwise builder
    // (vec_add) and scalar-broadcast builder (vec_scale) return a Vec<f64>
    // whose elements are read back with vec_get_f64. vec_add([1,2],[3,4]) ->
    // [4, 6]; vec_scale([1,2], 10.0) -> [10, 20]. Results are bound to a `let`
    // before printing because the import rewriter does not descend into
    // `println!` macro args. Floats print via `%g`, so whole numbers render
    // without a trailing `.0`.
    if !c_backend_ready() {
        eprintln!("skipping linalg elementwise/scalar builders e2e: no C backend available");
        return;
    }
    let src = r#"
mod core;
mod math;
mod linalg;

fn main() ~ Console {
    let mut a = vec_new_f64();
    vec_push_f64(a, 1.0);
    vec_push_f64(a, 2.0);

    let mut b = vec_new_f64();
    vec_push_f64(b, 3.0);
    vec_push_f64(b, 4.0);

    let sum = vec_add(a, b);
    let sum0 = vec_get_f64(sum, 0);
    let sum1 = vec_get_f64(sum, 1);

    let scaled = vec_scale(a, 10.0);
    let scaled0 = vec_get_f64(scaled, 0);
    let scaled1 = vec_get_f64(scaled, 1);

    println!("{} {}", sum0, sum1);
    println!("{} {}", scaled0, scaled1);
}
"#;
    let fixture = write_dispatch_fixture("linalg_builders", src);

    let output = buildc()
        .arg("run")
        .arg(&fixture)
        .output()
        .expect("run buildc run on linalg builders fixture");
    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "linalg elementwise/scalar builders should compile and run\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    // vec_add([1,2],[3,4]) = [4,6]; vec_scale([1,2],10) = [10,20]. `%g` drops `.0`.
    assert_eq!(
        stdout, "4 6\n10 20\n",
        "linalg vec_add/vec_scale should print `4 6` then `10 20`; got:\n{}",
        stdout
    );
}

// =============================================================================
// SCIENTIFIC-RUNTIME RECEIPT (buildlang-scientific-runtime-receipt/v0)
// =============================================================================

fn repo_example(name: &str) -> PathBuf {
    repo_root().join("examples").join(name)
}

#[test]
fn conservation_invariant_round_trips_positive_and_negative() {
    if !c_backend_ready() {
        eprintln!("skipping conservation_invariant_round_trips_positive_and_negative: C backend not ready");
        return;
    }
    let dir =
        std::env::temp_dir().join(format!("buildlang_sci_conservation_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create conservation fixture dir");

    // POSITIVE: the rotation kernel conserves r^2, so `--invariant conservation`
    // yields a PASS receipt that re-runs and re-checks clean.
    let pass_receipt = dir.join("rotation.json");
    let emit_pass = buildc()
        .arg("run")
        .arg(repo_example("conservation_rotation.bld"))
        .args(["--emit-receipt"])
        .arg(&pass_receipt)
        .args([
            "--invariant",
            "conservation",
            "--metric",
            "r2",
            "--problem",
            "rotational-radius",
        ])
        .output()
        .expect("emit conservation PASS receipt");
    assert!(
        emit_pass.status.success(),
        "emitting the conservation PASS receipt should succeed\nstderr:\n{}",
        String::from_utf8_lossy(&emit_pass.stderr)
    );
    let pass: serde_json::Value =
        serde_json::from_slice(&fs::read(&pass_receipt).expect("read PASS receipt")).unwrap();
    assert_eq!(pass["invariant"]["name"], "conserved_quantity_constant");
    assert_eq!(pass["oracle"]["name"], "conserved_quantity_constant");
    assert_eq!(pass["receipt_status"], "PASS");
    assert_eq!(pass["invariant"]["observed"]["violation_count"], 0);

    let verify_pass = buildc()
        .args(["receipt", "verify"])
        .arg(&pass_receipt)
        .output()
        .expect("verify conservation PASS receipt");
    assert!(
        verify_pass.status.success(),
        "the conservation PASS receipt must verify\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verify_pass.stdout),
        String::from_utf8_lossy(&verify_pass.stderr)
    );

    // NEGATIVE fixture: the decay kernel leaks, so with `--negative-fixture` it
    // is a FAIL_EXPECTED receipt that STILL verifies (it faithfully reproduces
    // its declared, expected failure -> exit 0).
    let fail_receipt = dir.join("decay.json");
    let emit_fail = buildc()
        .arg("run")
        .arg(repo_example("conservation_decay.bld"))
        .args(["--emit-receipt"])
        .arg(&fail_receipt)
        .args([
            "--invariant",
            "conservation",
            "--negative-fixture",
            "--metric",
            "q",
            "--problem",
            "leak",
        ])
        .output()
        .expect("emit conservation negative fixture");
    assert!(
        emit_fail.status.success(),
        "emitting the negative fixture should succeed"
    );
    let fail: serde_json::Value =
        serde_json::from_slice(&fs::read(&fail_receipt).expect("read FAIL receipt")).unwrap();
    assert_eq!(fail["receipt_status"], "FAIL_EXPECTED");
    assert!(
        fail["invariant"]["observed"]["violation_count"]
            .as_u64()
            .unwrap()
            > 0
    );

    let verify_fail = buildc()
        .args(["receipt", "verify"])
        .arg(&fail_receipt)
        .output()
        .expect("verify conservation negative fixture");
    assert!(
        verify_fail.status.success(),
        "a faithfully reproduced FAIL_EXPECTED must verify (exit 0)\nstderr:\n{}",
        String::from_utf8_lossy(&verify_fail.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn bounded_invariant_round_trips_positive_negative_and_is_distinct() {
    if !c_backend_ready() {
        eprintln!(
            "skipping bounded_invariant_round_trips_positive_negative_and_is_distinct: C backend not ready"
        );
        return;
    }
    let dir = std::env::temp_dir().join(format!("buildlang_sci_bounded_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create bounded fixture dir");

    // POSITIVE: the undamped oscillator's x^2 never rises above its initial
    // 1.0, so `--invariant bounded` yields a PASS receipt that re-checks clean.
    let pass_receipt = dir.join("oscillation.json");
    let emit_pass = buildc()
        .arg("run")
        .arg(repo_example("bounded_oscillation.bld"))
        .args(["--emit-receipt"])
        .arg(&pass_receipt)
        .args([
            "--invariant",
            "bounded",
            "--metric",
            "x2",
            "--problem",
            "undamped-oscillator",
        ])
        .output()
        .expect("emit bounded PASS receipt");
    assert!(
        emit_pass.status.success(),
        "emitting the bounded PASS receipt should succeed\nstderr:\n{}",
        String::from_utf8_lossy(&emit_pass.stderr)
    );
    let pass: serde_json::Value =
        serde_json::from_slice(&fs::read(&pass_receipt).expect("read PASS receipt")).unwrap();
    assert_eq!(pass["invariant"]["name"], "bounded_by_initial_maximum");
    assert_eq!(pass["oracle"]["name"], "bounded_by_initial_maximum");
    assert_eq!(pass["receipt_status"], "PASS");
    assert_eq!(pass["invariant"]["observed"]["violation_count"], 0);

    let verify_pass = buildc()
        .args(["receipt", "verify"])
        .arg(&pass_receipt)
        .output()
        .expect("verify bounded PASS receipt");
    assert!(
        verify_pass.status.success(),
        "the bounded PASS receipt must verify\nstderr:\n{}",
        String::from_utf8_lossy(&verify_pass.stderr)
    );

    // DISTINCTNESS end-to-end: the SAME oscillation kernel, checked under
    // `--invariant energy-monotone`, FAILs (x^2 rises after each dip). This is
    // the binary-level witness that bounded is not an alias of monotone.
    let mono_receipt = dir.join("oscillation_mono.json");
    let emit_mono = buildc()
        .arg("run")
        .arg(repo_example("bounded_oscillation.bld"))
        .args(["--emit-receipt"])
        .arg(&mono_receipt)
        .args([
            "--invariant",
            "energy-monotone",
            "--metric",
            "x2",
            "--problem",
            "undamped-oscillator",
        ])
        .output()
        .expect("emit monotone receipt from the oscillation kernel");
    assert!(emit_mono.status.success());
    let mono: serde_json::Value =
        serde_json::from_slice(&fs::read(&mono_receipt).expect("read monotone receipt")).unwrap();
    assert_eq!(
        mono["receipt_status"], "FAIL_UNEXPECTED",
        "the oscillation that PASSES bounded must FAIL monotone"
    );

    // NEGATIVE fixture: the explicit-Euler oscillator injects energy, so its
    // tracked quantity overshoots. With `--negative-fixture` it is a
    // FAIL_EXPECTED receipt that STILL verifies (exit 0).
    let fail_receipt = dir.join("overshoot.json");
    let emit_fail = buildc()
        .arg("run")
        .arg(repo_example("bounded_overshoot.bld"))
        .args(["--emit-receipt"])
        .arg(&fail_receipt)
        .args([
            "--invariant",
            "bounded",
            "--negative-fixture",
            "--metric",
            "energy",
            "--problem",
            "explicit-euler-oscillator",
        ])
        .output()
        .expect("emit bounded negative fixture");
    assert!(
        emit_fail.status.success(),
        "emitting the negative fixture should succeed"
    );
    let fail: serde_json::Value =
        serde_json::from_slice(&fs::read(&fail_receipt).expect("read FAIL receipt")).unwrap();
    assert_eq!(fail["receipt_status"], "FAIL_EXPECTED");
    assert!(
        fail["invariant"]["observed"]["violation_count"]
            .as_u64()
            .unwrap()
            > 0
    );

    let verify_fail = buildc()
        .args(["receipt", "verify"])
        .arg(&fail_receipt)
        .output()
        .expect("verify bounded negative fixture");
    assert!(
        verify_fail.status.success(),
        "a faithfully reproduced FAIL_EXPECTED must verify (exit 0)\nstderr:\n{}",
        String::from_utf8_lossy(&verify_fail.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn energy_identity_invariant_round_trips_positive_and_negative() {
    if !c_backend_ready() {
        eprintln!(
            "skipping energy_identity_invariant_round_trips_positive_and_negative: C backend not ready"
        );
        return;
    }
    let dir = std::env::temp_dir().join(format!(
        "buildlang_sci_energy_identity_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create energy-identity fixture dir");

    // POSITIVE: the FTCS kernel computes the EXACT discrete energy balance, so
    // its per-step residual stays at roundoff and `--invariant energy-identity`
    // PASSes.
    let pass_receipt = dir.join("balance.json");
    let emit_pass = buildc()
        .arg("run")
        .arg(repo_example("energy_identity.bld"))
        .args(["--emit-receipt"])
        .arg(&pass_receipt)
        .args([
            "--invariant",
            "energy-identity",
            "--metric",
            "residual",
            "--problem",
            "heat-energy-balance",
        ])
        .output()
        .expect("emit energy-identity PASS receipt");
    assert!(
        emit_pass.status.success(),
        "emitting the energy-identity PASS receipt should succeed\nstderr:\n{}",
        String::from_utf8_lossy(&emit_pass.stderr)
    );
    let pass: serde_json::Value =
        serde_json::from_slice(&fs::read(&pass_receipt).expect("read PASS receipt")).unwrap();
    assert_eq!(pass["invariant"]["name"], "energy_identity_residual");
    assert_eq!(pass["oracle"]["name"], "energy_identity_residual");
    assert_eq!(pass["receipt_status"], "PASS");
    assert_eq!(pass["invariant"]["observed"]["violation_count"], 0);

    let verify_pass = buildc()
        .args(["receipt", "verify"])
        .arg(&pass_receipt)
        .output()
        .expect("verify energy-identity PASS receipt");
    assert!(
        verify_pass.status.success(),
        "the energy-identity PASS receipt must verify\nstderr:\n{}",
        String::from_utf8_lossy(&verify_pass.stderr)
    );

    // NEGATIVE fixture: the broken kernel drops the r^2 correction term, so its
    // residual is O(r^2) (~1e-5), far above the 1e-9 tolerance. With
    // `--negative-fixture` it is a FAIL_EXPECTED receipt that STILL verifies.
    let fail_receipt = dir.join("broken.json");
    let emit_fail = buildc()
        .arg("run")
        .arg(repo_example("energy_identity_broken.bld"))
        .args(["--emit-receipt"])
        .arg(&fail_receipt)
        .args([
            "--invariant",
            "energy-identity",
            "--negative-fixture",
            "--metric",
            "residual",
            "--problem",
            "broken-balance",
        ])
        .output()
        .expect("emit energy-identity negative fixture");
    assert!(
        emit_fail.status.success(),
        "emitting the negative fixture should succeed"
    );
    let fail: serde_json::Value =
        serde_json::from_slice(&fs::read(&fail_receipt).expect("read FAIL receipt")).unwrap();
    assert_eq!(fail["receipt_status"], "FAIL_EXPECTED");
    // The reference is zero, so the very first step already violates.
    assert_eq!(fail["invariant"]["observed"]["first_violation_step"], 0);
    assert!(
        fail["invariant"]["observed"]["violation_count"]
            .as_u64()
            .unwrap()
            > 0
    );

    let verify_fail = buildc()
        .args(["receipt", "verify"])
        .arg(&fail_receipt)
        .output()
        .expect("verify energy-identity negative fixture");
    assert!(
        verify_fail.status.success(),
        "a faithfully reproduced FAIL_EXPECTED must verify (exit 0)\nstderr:\n{}",
        String::from_utf8_lossy(&verify_fail.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn relation_invariant_round_trips_positive_negative_and_validates_columns() {
    if !c_backend_ready() {
        eprintln!(
            "skipping relation_invariant_round_trips_positive_negative_and_validates_columns: C backend not ready"
        );
        return;
    }
    let dir = std::env::temp_dir().join(format!("buildlang_sci_relation_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create relation fixture dir");

    // POSITIVE: sin(2t) computed two ways (direct vs the double-angle identity)
    // agree, so `--invariant relation --columns 2` PASSes. The VERIFIER computes
    // the agreement across the two columns.
    let pass_receipt = dir.join("double_angle.json");
    let emit_pass = buildc()
        .arg("run")
        .arg(repo_example("relation_double_angle.bld"))
        .args(["--emit-receipt"])
        .arg(&pass_receipt)
        .args([
            "--invariant",
            "relation",
            "--columns",
            "2",
            "--metric",
            "sin2t",
            "--problem",
            "double-angle",
        ])
        .output()
        .expect("emit relation PASS receipt");
    assert!(
        emit_pass.status.success(),
        "emitting the relation PASS receipt should succeed\nstderr:\n{}",
        String::from_utf8_lossy(&emit_pass.stderr)
    );
    let pass: serde_json::Value =
        serde_json::from_slice(&fs::read(&pass_receipt).expect("read PASS receipt")).unwrap();
    assert_eq!(pass["invariant"]["name"], "relation_columns_agree");
    assert_eq!(pass["oracle"]["name"], "relation_columns_agree");
    assert_eq!(pass["measurement"]["column_count"], 2);
    assert_eq!(pass["receipt_status"], "PASS");
    assert_eq!(pass["invariant"]["observed"]["violation_count"], 0);

    let verify_pass = buildc()
        .args(["receipt", "verify"])
        .arg(&pass_receipt)
        .output()
        .expect("verify relation PASS receipt");
    assert!(
        verify_pass.status.success(),
        "the relation PASS receipt must verify\nstderr:\n{}",
        String::from_utf8_lossy(&verify_pass.stderr)
    );

    // NEGATIVE fixture: the broken kernel drops the factor of 2, so the two
    // columns disagree by |col0|/2. With `--negative-fixture` it is a
    // FAIL_EXPECTED receipt that STILL verifies.
    let fail_receipt = dir.join("broken.json");
    let emit_fail = buildc()
        .arg("run")
        .arg(repo_example("relation_double_angle_broken.bld"))
        .args(["--emit-receipt"])
        .arg(&fail_receipt)
        .args([
            "--invariant",
            "relation",
            "--columns",
            "2",
            "--negative-fixture",
            "--metric",
            "sin2t",
            "--problem",
            "broken",
        ])
        .output()
        .expect("emit relation negative fixture");
    assert!(
        emit_fail.status.success(),
        "emitting the negative fixture should succeed"
    );
    let fail: serde_json::Value =
        serde_json::from_slice(&fs::read(&fail_receipt).expect("read FAIL receipt")).unwrap();
    assert_eq!(fail["receipt_status"], "FAIL_EXPECTED");
    assert!(
        fail["invariant"]["observed"]["violation_count"]
            .as_u64()
            .unwrap()
            > 0
    );

    let verify_fail = buildc()
        .args(["receipt", "verify"])
        .arg(&fail_receipt)
        .output()
        .expect("verify relation negative fixture");
    assert!(
        verify_fail.status.success(),
        "a faithfully reproduced FAIL_EXPECTED must verify (exit 0)\nstderr:\n{}",
        String::from_utf8_lossy(&verify_fail.stderr)
    );

    // VALIDATION: `--invariant relation` without `--columns >= 2` is rejected
    // before compiling (each row must hold the columns to compare).
    let bad = buildc()
        .arg("run")
        .arg(repo_example("relation_double_angle.bld"))
        .args(["--emit-receipt"])
        .arg(dir.join("bad.json"))
        .args(["--invariant", "relation"])
        .output()
        .expect("emit relation without columns");
    assert!(
        !bad.status.success(),
        "relation without --columns >= 2 must be rejected"
    );
    assert!(String::from_utf8_lossy(&bad.stderr).contains("--columns >= 2"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn reaction_atom_balance_round_trips_positive_and_negative() {
    if !c_backend_ready() {
        eprintln!(
            "skipping reaction_atom_balance_round_trips_positive_and_negative: C backend not ready"
        );
        return;
    }
    let dir = std::env::temp_dir().join(format!("buildlang_sci_reaction_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create reaction fixture dir");

    // POSITIVE: the reaction A + B <=> C conserves the atom count [A] + [C]
    // exactly under a balanced update, so `--invariant conservation` PASSes even
    // as the reaction proceeds. The family applied to a reaction network.
    let pass_receipt = dir.join("reaction.json");
    let emit_pass = buildc()
        .arg("run")
        .arg(repo_example("reaction_atom_balance.bld"))
        .args(["--emit-receipt"])
        .arg(&pass_receipt)
        .args([
            "--invariant",
            "conservation",
            "--metric",
            "atom-balance",
            "--problem",
            "reaction-A-B-C",
        ])
        .output()
        .expect("emit reaction PASS receipt");
    assert!(
        emit_pass.status.success(),
        "emitting the reaction PASS receipt should succeed\nstderr:\n{}",
        String::from_utf8_lossy(&emit_pass.stderr)
    );
    let pass: serde_json::Value =
        serde_json::from_slice(&fs::read(&pass_receipt).expect("read PASS receipt")).unwrap();
    assert_eq!(pass["invariant"]["name"], "conserved_quantity_constant");
    assert_eq!(pass["receipt_status"], "PASS");
    assert_eq!(pass["invariant"]["observed"]["violation_count"], 0);

    let verify_pass = buildc()
        .args(["receipt", "verify"])
        .arg(&pass_receipt)
        .output()
        .expect("verify reaction PASS receipt");
    assert!(
        verify_pass.status.success(),
        "the reaction PASS receipt must verify\nstderr:\n{}",
        String::from_utf8_lossy(&verify_pass.stderr)
    );

    // NEGATIVE fixture: a stoichiometry bug (two C produced per event) drifts the
    // atom balance. With `--negative-fixture` it is a FAIL_EXPECTED receipt that
    // STILL verifies.
    let fail_receipt = dir.join("reaction_broken.json");
    let emit_fail = buildc()
        .arg("run")
        .arg(repo_example("reaction_atom_balance_broken.bld"))
        .args(["--emit-receipt"])
        .arg(&fail_receipt)
        .args([
            "--invariant",
            "conservation",
            "--negative-fixture",
            "--metric",
            "atom-balance",
            "--problem",
            "reaction-broken",
        ])
        .output()
        .expect("emit reaction negative fixture");
    assert!(
        emit_fail.status.success(),
        "emitting the negative fixture should succeed"
    );
    let fail: serde_json::Value =
        serde_json::from_slice(&fs::read(&fail_receipt).expect("read FAIL receipt")).unwrap();
    assert_eq!(fail["receipt_status"], "FAIL_EXPECTED");
    assert!(
        fail["invariant"]["observed"]["violation_count"]
            .as_u64()
            .unwrap()
            > 0
    );

    let verify_fail = buildc()
        .args(["receipt", "verify"])
        .arg(&fail_receipt)
        .output()
        .expect("verify reaction negative fixture");
    assert!(
        verify_fail.status.success(),
        "a faithfully reproduced FAIL_EXPECTED must verify (exit 0)\nstderr:\n{}",
        String::from_utf8_lossy(&verify_fail.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn non_negative_invariant_round_trips_a_result_bearing_bound() {
    if !c_backend_ready() {
        eprintln!("skipping non_negative_invariant_round_trips_a_result_bearing_bound: C backend not ready");
        return;
    }
    let dir =
        std::env::temp_dir().join(format!("buildlang_sci_non_negative_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create non-negative fixture dir");

    // POSITIVE: binary search's measured probe count never exceeds its proven
    // bound, so the printed slack (bound - probes) stays non-negative and
    // `--invariant non-negative` PASSes. This is the family's ALGORITHMIC
    // accountability member: the receipt witnesses a computation's measured
    // cost, not a physical quantity.
    let pass_receipt = dir.join("binary.json");
    let emit_pass = buildc()
        .arg("run")
        .arg(repo_example("search_bound_binary.bld"))
        .args(["--emit-receipt"])
        .arg(&pass_receipt)
        .args([
            "--invariant",
            "non-negative",
            "--metric",
            "slack",
            "--problem",
            "binary-search-probe-bound",
        ])
        .output()
        .expect("emit non-negative PASS receipt");
    assert!(
        emit_pass.status.success(),
        "emitting the non-negative PASS receipt should succeed\nstderr:\n{}",
        String::from_utf8_lossy(&emit_pass.stderr)
    );
    let pass: serde_json::Value =
        serde_json::from_slice(&fs::read(&pass_receipt).expect("read PASS receipt")).unwrap();
    assert_eq!(pass["invariant"]["name"], "non_negative");
    assert_eq!(pass["receipt_status"], "PASS");
    assert_eq!(pass["invariant"]["observed"]["violation_count"], 0);

    let verify_pass = buildc()
        .args(["receipt", "verify"])
        .arg(&pass_receipt)
        .output()
        .expect("verify non-negative PASS receipt");
    assert!(
        verify_pass.status.success(),
        "the non-negative PASS receipt must verify\nstderr:\n{}",
        String::from_utf8_lossy(&verify_pass.stderr)
    );

    // NEGATIVE fixture: linear search exceeds the same bound, so the slack goes
    // negative. With `--negative-fixture` it is a FAIL_EXPECTED receipt that
    // STILL verifies.
    let fail_receipt = dir.join("linear.json");
    let emit_fail = buildc()
        .arg("run")
        .arg(repo_example("search_bound_linear.bld"))
        .args(["--emit-receipt"])
        .arg(&fail_receipt)
        .args([
            "--invariant",
            "non-negative",
            "--negative-fixture",
            "--metric",
            "slack",
            "--problem",
            "linear-search-probe-bound",
        ])
        .output()
        .expect("emit non-negative negative fixture");
    assert!(
        emit_fail.status.success(),
        "emitting the negative fixture should succeed"
    );
    let fail: serde_json::Value =
        serde_json::from_slice(&fs::read(&fail_receipt).expect("read FAIL receipt")).unwrap();
    assert_eq!(fail["receipt_status"], "FAIL_EXPECTED");
    assert!(
        fail["invariant"]["observed"]["violation_count"]
            .as_u64()
            .unwrap()
            > 0
    );

    let verify_fail = buildc()
        .args(["receipt", "verify"])
        .arg(&fail_receipt)
        .output()
        .expect("verify non-negative negative fixture");
    assert!(
        verify_fail.status.success(),
        "a faithfully reproduced FAIL_EXPECTED must verify (exit 0)\nstderr:\n{}",
        String::from_utf8_lossy(&verify_fail.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn born_rule_normalization_round_trips_conservation() {
    if !c_backend_ready() {
        eprintln!("skipping born_rule_normalization_round_trips_conservation: C backend not ready");
        return;
    }
    let dir = std::env::temp_dir().join(format!("buildlang_sci_born_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create born-rule fixture dir");

    // POSITIVE: a single qubit evolved by a unitary X-rotation keeps its total
    // Born probability at 1, so `--invariant conservation` PASSes.
    let pass_receipt = dir.join("unitary.json");
    let emit_pass = buildc()
        .arg("run")
        .arg(repo_example("born_rule_normalization.bld"))
        .args(["--emit-receipt"])
        .arg(&pass_receipt)
        .args([
            "--invariant",
            "conservation",
            "--problem",
            "born-rule-normalization",
        ])
        .output()
        .expect("emit born-rule PASS receipt");
    assert!(
        emit_pass.status.success(),
        "emitting the born-rule PASS receipt should succeed\nstderr:\n{}",
        String::from_utf8_lossy(&emit_pass.stderr)
    );
    let pass: serde_json::Value =
        serde_json::from_slice(&fs::read(&pass_receipt).expect("read PASS receipt")).unwrap();
    assert_eq!(pass["invariant"]["name"], "conserved_quantity_constant");
    assert_eq!(pass["receipt_status"], "PASS");
    assert_eq!(pass["invariant"]["observed"]["violation_count"], 0);

    let verify_pass = buildc()
        .args(["receipt", "verify"])
        .arg(&pass_receipt)
        .output()
        .expect("verify born-rule PASS receipt");
    assert!(
        verify_pass.status.success(),
        "the born-rule PASS receipt must verify\nstderr:\n{}",
        String::from_utf8_lossy(&verify_pass.stderr)
    );

    // NEGATIVE fixture: a non-unitary gain (g = 1.001 on every amplitude each
    // step) inflates the total probability past 1, so with `--negative-fixture`
    // it is a FAIL_EXPECTED receipt that STILL verifies.
    let fail_receipt = dir.join("leaky.json");
    let emit_fail = buildc()
        .arg("run")
        .arg(repo_example("born_rule_leaky.bld"))
        .args(["--emit-receipt"])
        .arg(&fail_receipt)
        .args([
            "--invariant",
            "conservation",
            "--negative-fixture",
            "--problem",
            "born-rule-leaky",
        ])
        .output()
        .expect("emit born-rule negative fixture");
    assert!(
        emit_fail.status.success(),
        "emitting the negative fixture should succeed"
    );
    let fail: serde_json::Value =
        serde_json::from_slice(&fs::read(&fail_receipt).expect("read FAIL receipt")).unwrap();
    assert_eq!(fail["receipt_status"], "FAIL_EXPECTED");
    assert!(
        fail["invariant"]["observed"]["violation_count"]
            .as_u64()
            .unwrap()
            > 0
    );

    let verify_fail = buildc()
        .args(["receipt", "verify"])
        .arg(&fail_receipt)
        .output()
        .expect("verify born-rule negative fixture");
    assert!(
        verify_fail.status.success(),
        "a faithfully reproduced FAIL_EXPECTED must verify (exit 0)\nstderr:\n{}",
        String::from_utf8_lossy(&verify_fail.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn funnel_hashing_round_trips_a_probe_bound() {
    if !c_backend_ready() {
        eprintln!("skipping funnel_hashing_round_trips_a_probe_bound: C backend not ready");
        return;
    }
    let dir = std::env::temp_dir().join(format!("buildlang_sci_funnel_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create funnel fixture dir");

    // POSITIVE: the funnel's measured probe count stays under its calibrated
    // bound, so the printed slack (bound - probes) stays non-negative and
    // `--invariant non-negative` PASSes.
    let pass_receipt = dir.join("funnel.json");
    let emit_pass = buildc()
        .arg("run")
        .arg(repo_example("funnel_probe.bld"))
        .args(["--emit-receipt"])
        .arg(&pass_receipt)
        .args([
            "--invariant",
            "non-negative",
            "--metric",
            "slack",
            "--problem",
            "funnel-hashing-probe-bound",
        ])
        .output()
        .expect("emit funnel PASS receipt");
    assert!(
        emit_pass.status.success(),
        "emitting the funnel PASS receipt should succeed\nstderr:\n{}",
        String::from_utf8_lossy(&emit_pass.stderr)
    );
    let pass: serde_json::Value =
        serde_json::from_slice(&fs::read(&pass_receipt).expect("read PASS receipt")).unwrap();
    assert_eq!(pass["invariant"]["name"], "non_negative");
    assert_eq!(pass["receipt_status"], "PASS");
    assert_eq!(pass["invariant"]["observed"]["violation_count"], 0);

    let verify_pass = buildc()
        .args(["receipt", "verify"])
        .arg(&pass_receipt)
        .output()
        .expect("verify funnel PASS receipt");
    assert!(
        verify_pass.status.success(),
        "the funnel PASS receipt must verify\nstderr:\n{}",
        String::from_utf8_lossy(&verify_pass.stderr)
    );

    // NEGATIVE fixture: single-level linear probing exceeds the same bound, so
    // the slack goes negative. With `--negative-fixture` it is a FAIL_EXPECTED
    // receipt that STILL verifies.
    let fail_receipt = dir.join("linear.json");
    let emit_fail = buildc()
        .arg("run")
        .arg(repo_example("funnel_probe_linear.bld"))
        .args(["--emit-receipt"])
        .arg(&fail_receipt)
        .args([
            "--invariant",
            "non-negative",
            "--negative-fixture",
            "--metric",
            "slack",
            "--problem",
            "linear-probing-probe-bound",
        ])
        .output()
        .expect("emit funnel negative fixture");
    assert!(
        emit_fail.status.success(),
        "emitting the negative fixture should succeed"
    );
    let fail: serde_json::Value =
        serde_json::from_slice(&fs::read(&fail_receipt).expect("read FAIL receipt")).unwrap();
    assert_eq!(fail["receipt_status"], "FAIL_EXPECTED");
    assert!(
        fail["invariant"]["observed"]["violation_count"]
            .as_u64()
            .unwrap()
            > 0
    );

    let verify_fail = buildc()
        .args(["receipt", "verify"])
        .arg(&fail_receipt)
        .output()
        .expect("verify funnel negative fixture");
    assert!(
        verify_fail.status.success(),
        "a faithfully reproduced FAIL_EXPECTED must verify (exit 0)\nstderr:\n{}",
        String::from_utf8_lossy(&verify_fail.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn seeded_random_walk_round_trips_and_pins_the_seed_contract() {
    if !c_backend_ready() {
        eprintln!("skipping seeded_random_walk_round_trips_and_pins_the_seed_contract: C backend not ready");
        return;
    }
    let dir = std::env::temp_dir().join(format!("buildlang_sci_random_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create random fixture dir");

    // FAIL CLOSED, operator level: a Random-using kernel with no --seed is
    // refused before any run (an unseeded stream cannot be re-derived, so a
    // receipt over it would be unverifiable by construction).
    let refused = buildc()
        .arg("run")
        .arg(repo_example("random_walk_bound.bld"))
        .args(["--emit-receipt"])
        .arg(dir.join("no-seed.json"))
        .args(["--invariant", "non-negative"])
        .output()
        .expect("run seedless random kernel");
    assert!(
        !refused.status.success(),
        "a Random-using kernel with no --seed must be refused"
    );
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("requires an explicit seed"),
        "the refusal must name the missing seed\nstderr:\n{}",
        String::from_utf8_lossy(&refused.stderr)
    );

    // FAIL CLOSED, the other direction: a seed on a kernel with no Random
    // capability is an unconsumed knob and must not be sealed as witnessed.
    let unconsumed = buildc()
        .arg("run")
        .arg(repo_example("search_bound_binary.bld"))
        .args(["--emit-receipt"])
        .arg(dir.join("unconsumed.json"))
        .args(["--invariant", "non-negative", "--seed", "7"])
        .output()
        .expect("run seedless program with a seed");
    assert!(
        !unconsumed.status.success(),
        "a seed on a program with no Random capability must be refused"
    );
    assert!(
        String::from_utf8_lossy(&unconsumed.stderr).contains("no Random capability"),
        "the refusal must name the unconsumed seed\nstderr:\n{}",
        String::from_utf8_lossy(&unconsumed.stderr)
    );

    // POSITIVE: the seeded walk stays inside its worst-case envelope, the
    // receipt seals the seed and the derived claims, and verify re-runs the
    // exact stream.
    let emit = |source: &str, receipt: &std::path::Path, seed: &str, negative: bool| {
        let mut cmd = buildc();
        cmd.arg("run")
            .arg(repo_example(source))
            .args(["--emit-receipt"])
            .arg(receipt)
            .args([
                "--invariant",
                "non-negative",
                "--metric",
                "slack",
                "--problem",
                "seeded-random-walk-envelope",
                "--seed",
                seed,
            ]);
        if negative {
            cmd.arg("--negative-fixture");
        }
        cmd.output().expect("emit seeded receipt")
    };
    let pass_receipt = dir.join("walk.json");
    let emit_pass = emit("random_walk_bound.bld", &pass_receipt, "42", false);
    assert!(
        emit_pass.status.success(),
        "emitting the seeded PASS receipt should succeed\nstderr:\n{}",
        String::from_utf8_lossy(&emit_pass.stderr)
    );
    let pass: serde_json::Value =
        serde_json::from_slice(&fs::read(&pass_receipt).expect("read PASS receipt")).unwrap();
    assert_eq!(pass["receipt_status"], "PASS");
    assert_eq!(pass["seed_value"], 42);
    assert_eq!(pass["seed"]["status"], "SEALED");
    assert!(
        pass["effect_policy"]["observed_capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c == "Random"),
        "the effect policy must observe the Random capability"
    );
    // The receipt's honest third determinism state: deterministic GIVEN the
    // sealed seed, with the grounds saying so.
    assert_eq!(pass["determinism"]["deterministic_modulo_args"], true);
    assert!(
        pass["determinism"]["grounds"]
            .as_str()
            .unwrap()
            .contains("seeded"),
        "determinism grounds must name the seeded qualification"
    );
    let verify_pass = buildc()
        .args(["receipt", "verify"])
        .arg(&pass_receipt)
        .output()
        .expect("verify seeded PASS receipt");
    assert!(
        verify_pass.status.success(),
        "the seeded PASS receipt must verify\nstderr:\n{}",
        String::from_utf8_lossy(&verify_pass.stderr)
    );

    // The seed genuinely drives the stream: the same seed reproduces the raw
    // stdout byte-for-byte, a different seed does not. This is what makes the
    // stochastic run as re-derivable as a deterministic one, and it is the
    // check that would catch a runtime whose "seeded" PRNG ignored its seed.
    let again_receipt = dir.join("walk-again.json");
    let other_receipt = dir.join("walk-other.json");
    assert!(emit("random_walk_bound.bld", &again_receipt, "42", false)
        .status
        .success());
    assert!(emit("random_walk_bound.bld", &other_receipt, "43", false)
        .status
        .success());
    let digest = |path: &std::path::Path| -> String {
        let v: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        v["measurement"]["raw_stdout_digest"]["hex"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert_eq!(
        digest(&pass_receipt),
        digest(&again_receipt),
        "the same seed must reproduce the stream byte-for-byte"
    );
    assert_ne!(
        digest(&pass_receipt),
        digest(&other_receipt),
        "a different seed must produce a different stream"
    );

    // NEGATIVE fixture under the same seed: the walk leaves the claimed
    // tighter envelope, the invariant FAILs as declared, and the receipt
    // still verifies (the harness catches a false stochastic claim, it does
    // not just bless a true one).
    let fail_receipt = dir.join("walk-broken.json");
    let emit_fail = emit("random_walk_bound_broken.bld", &fail_receipt, "42", true);
    assert!(
        emit_fail.status.success(),
        "emitting the negative fixture should succeed\nstderr:\n{}",
        String::from_utf8_lossy(&emit_fail.stderr)
    );
    let fail: serde_json::Value =
        serde_json::from_slice(&fs::read(&fail_receipt).expect("read FAIL receipt")).unwrap();
    assert_eq!(fail["receipt_status"], "FAIL_EXPECTED");
    assert!(
        fail["invariant"]["observed"]["violation_count"]
            .as_u64()
            .unwrap()
            > 0
    );
    let verify_fail = buildc()
        .args(["receipt", "verify"])
        .arg(&fail_receipt)
        .output()
        .expect("verify seeded negative fixture");
    assert!(
        verify_fail.status.success(),
        "a faithfully reproduced FAIL_EXPECTED must verify (exit 0)\nstderr:\n{}",
        String::from_utf8_lossy(&verify_fail.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn monte_carlo_declaration_round_trips_and_pins_the_admission_contract() {
    if !c_backend_ready() {
        eprintln!("skipping monte_carlo_declaration_round_trips_and_pins_the_admission_contract: C backend not ready");
        return;
    }
    let dir = std::env::temp_dir().join(format!("buildlang_sci_mc_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create mc fixture dir");

    let mc_flags = [
        "--mc-estimator",
        "mean",
        "--mc-samples",
        "2000",
        "--mc-interval",
        "normal-approx-95",
    ];

    // POSITIVE: the pi kernel under the full declaration PASSes, the block is
    // sealed with the declared facts, and the receipt verifies.
    let pass_receipt = dir.join("mcpi.json");
    let emit_pass = buildc()
        .arg("run")
        .arg(repo_example("mc_pi_rejection.bld"))
        .args(["--emit-receipt"])
        .arg(&pass_receipt)
        .args([
            "--invariant",
            "non-negative",
            "--metric",
            "slack",
            "--problem",
            "mc-pi-rejection",
            "--seed",
            "42",
        ])
        .args(mc_flags)
        .output()
        .expect("emit mc PASS receipt");
    assert!(
        emit_pass.status.success(),
        "emitting the mc PASS receipt should succeed\nstderr:\n{}",
        String::from_utf8_lossy(&emit_pass.stderr)
    );
    let pass: serde_json::Value =
        serde_json::from_slice(&fs::read(&pass_receipt).expect("read PASS receipt")).unwrap();
    assert_eq!(pass["receipt_status"], "PASS");
    assert_eq!(pass["monte_carlo"]["estimator"], "mean");
    assert_eq!(pass["monte_carlo"]["samples"], 2000);
    assert_eq!(pass["monte_carlo"]["interval_method"], "normal-approx-95");
    assert_eq!(pass["monte_carlo"]["status"], "DECLARED");
    assert_eq!(pass["seed_value"], 42);
    let verify_pass = buildc()
        .args(["receipt", "verify"])
        .arg(&pass_receipt)
        .output()
        .expect("verify mc PASS receipt");
    assert!(
        verify_pass.status.success(),
        "the mc PASS receipt must verify\nstderr:\n{}",
        String::from_utf8_lossy(&verify_pass.stderr)
    );

    // NEGATIVE fixture: the wrong-area estimator misses pi by ~0.785, blows
    // through the calibrated band, and FAILs as declared.
    let fail_receipt = dir.join("mcpi-broken.json");
    let emit_fail = buildc()
        .arg("run")
        .arg(repo_example("mc_pi_rejection_broken.bld"))
        .args(["--emit-receipt"])
        .arg(&fail_receipt)
        .args([
            "--invariant",
            "non-negative",
            "--negative-fixture",
            "--metric",
            "slack",
            "--problem",
            "mc-pi-rejection",
            "--seed",
            "42",
        ])
        .args(mc_flags)
        .output()
        .expect("emit mc negative fixture");
    assert!(
        emit_fail.status.success(),
        "emitting the mc negative fixture should succeed\nstderr:\n{}",
        String::from_utf8_lossy(&emit_fail.stderr)
    );
    let fail: serde_json::Value =
        serde_json::from_slice(&fs::read(&fail_receipt).expect("read FAIL receipt")).unwrap();
    assert_eq!(fail["receipt_status"], "FAIL_EXPECTED");
    assert!(
        fail["invariant"]["observed"]["violation_count"]
            .as_u64()
            .unwrap()
            > 0
    );
    let verify_fail = buildc()
        .args(["receipt", "verify"])
        .arg(&fail_receipt)
        .output()
        .expect("verify mc negative fixture");
    assert!(
        verify_fail.status.success(),
        "a faithfully reproduced FAIL_EXPECTED must verify (exit 0)\nstderr:\n{}",
        String::from_utf8_lossy(&verify_fail.stderr)
    );

    // FAIL CLOSED: every partial declaration is refused (an estimator whose
    // interval method is undeclared, and each other missing-one combination).
    for partial in [
        &["--mc-estimator", "mean", "--mc-samples", "2000"][..],
        &[
            "--mc-estimator",
            "mean",
            "--mc-interval",
            "normal-approx-95",
        ][..],
        &["--mc-samples", "2000", "--mc-interval", "normal-approx-95"][..],
        &["--mc-estimator", "mean"][..],
    ] {
        let refused = buildc()
            .arg("run")
            .arg(repo_example("mc_pi_rejection.bld"))
            .args(["--emit-receipt"])
            .arg(dir.join("partial.json"))
            .args(["--invariant", "non-negative", "--seed", "42"])
            .args(partial)
            .output()
            .expect("run partial mc declaration");
        assert!(
            !refused.status.success(),
            "a partial mc declaration must be refused: {partial:?}"
        );
        assert!(
            String::from_utf8_lossy(&refused.stderr).contains("together"),
            "the refusal must name the all-or-nothing contract\nstderr:\n{}",
            String::from_utf8_lossy(&refused.stderr)
        );
    }

    // FAIL CLOSED: a zero denominator is refused before anything runs.
    let zero = buildc()
        .arg("run")
        .arg(repo_example("mc_pi_rejection.bld"))
        .args(["--emit-receipt"])
        .arg(dir.join("zero.json"))
        .args([
            "--invariant",
            "non-negative",
            "--seed",
            "42",
            "--mc-estimator",
            "mean",
            "--mc-samples",
            "0",
            "--mc-interval",
            "normal-approx-95",
        ])
        .output()
        .expect("run zero-sample mc declaration");
    assert!(
        !zero.status.success(),
        "a zero mc denominator must be refused"
    );
    assert!(
        String::from_utf8_lossy(&zero.stderr).contains("unpriceable"),
        "the refusal must name the unpriceable denominator\nstderr:\n{}",
        String::from_utf8_lossy(&zero.stderr)
    );

    // FAIL CLOSED: an mc declaration on a program with no Random capability
    // is refused (nothing samples).
    let unbacked = buildc()
        .arg("run")
        .arg(repo_example("search_bound_binary.bld"))
        .args(["--emit-receipt"])
        .arg(dir.join("unbacked.json"))
        .args(["--invariant", "non-negative"])
        .args(mc_flags)
        .output()
        .expect("run mc declaration on a seedless program");
    assert!(
        !unbacked.status.success(),
        "an mc declaration on a non-Random program must be refused"
    );
    assert!(
        String::from_utf8_lossy(&unbacked.stderr).contains("no Random capability"),
        "the refusal must name the missing capability\nstderr:\n{}",
        String::from_utf8_lossy(&unbacked.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn mc_pi_rejection_executed_round_trip_and_negative_fixture() {
    if !c_backend_ready() {
        eprintln!(
            "skipping mc_pi_rejection_executed_round_trip_and_negative_fixture: C backend not ready"
        );
        return;
    }
    let dir =
        std::env::temp_dir().join(format!("buildlang_sci_mc_executed_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create mc-executed fixture dir");

    let mc_executed_flags = [
        "--mc-executed",
        "--mc-estimator",
        "proportion",
        "--mc-samples",
        "2000",
        "--mc-interval",
        "wilson-95",
    ];

    // POSITIVE: the executed pi kernel PASSes, the block is sealed EXECUTED
    // with a re-deriving interval and a witnessed denominator, and the
    // receipt verifies (both Stage A and Stage B).
    let pass_receipt = dir.join("mcpi_executed.json");
    let emit_pass = buildc()
        .arg("run")
        .arg(repo_example("mc_pi_rejection_executed.bld"))
        .args(["--emit-receipt"])
        .arg(&pass_receipt)
        .args([
            "--invariant",
            "non-negative",
            "--metric",
            "slack",
            "--problem",
            "mc-pi-rejection-executed",
            "--seed",
            "42",
        ])
        .args(mc_executed_flags)
        .output()
        .expect("emit mc-executed PASS receipt");
    assert!(
        emit_pass.status.success(),
        "emitting the mc-executed PASS receipt should succeed\nstderr:\n{}",
        String::from_utf8_lossy(&emit_pass.stderr)
    );
    let pass: serde_json::Value =
        serde_json::from_slice(&fs::read(&pass_receipt).expect("read PASS receipt")).unwrap();
    assert_eq!(pass["receipt_status"], "PASS");
    assert_eq!(pass["monte_carlo"]["status"], "EXECUTED");
    assert_eq!(pass["monte_carlo"]["n_effective"], 2000);
    let successes = pass["monte_carlo"]["successes"]
        .as_u64()
        .expect("successes must be present");
    assert!(successes <= 2000);
    let estimate = pass["monte_carlo"]["estimate"].as_f64().unwrap();
    let low = pass["monte_carlo"]["interval_low"].as_f64().unwrap();
    let high = pass["monte_carlo"]["interval_high"].as_f64().unwrap();
    assert!(
        low < estimate && estimate < high,
        "interval_low < estimate < interval_high must hold: {low} < {estimate} < {high}"
    );
    let verify_pass = buildc()
        .args(["receipt", "verify"])
        .arg(&pass_receipt)
        .output()
        .expect("verify mc-executed PASS receipt");
    assert!(
        verify_pass.status.success(),
        "the mc-executed PASS receipt must verify\nstderr:\n{}",
        String::from_utf8_lossy(&verify_pass.stderr)
    );

    // NEGATIVE fixture: the wrong-area estimator still executes and
    // re-derives a coherent interval (the raw successes/trials counters are
    // untouched by the wrong-area factor), while the slack column blows the
    // truth band and FAILs as declared. The interval claim and the
    // truth-band claim fail independently.
    let fail_receipt = dir.join("mcpi_executed_broken.json");
    let emit_fail = buildc()
        .arg("run")
        .arg(repo_example("mc_pi_rejection_executed_broken.bld"))
        .args(["--emit-receipt"])
        .arg(&fail_receipt)
        .args([
            "--invariant",
            "non-negative",
            "--negative-fixture",
            "--metric",
            "slack",
            "--problem",
            "mc-pi-rejection-executed",
            "--seed",
            "42",
        ])
        .args(mc_executed_flags)
        .output()
        .expect("emit mc-executed negative fixture");
    assert!(
        emit_fail.status.success(),
        "emitting the mc-executed negative fixture should succeed\nstderr:\n{}",
        String::from_utf8_lossy(&emit_fail.stderr)
    );
    let fail: serde_json::Value =
        serde_json::from_slice(&fs::read(&fail_receipt).expect("read FAIL receipt")).unwrap();
    assert_eq!(fail["receipt_status"], "FAIL_EXPECTED");
    assert_eq!(
        fail["monte_carlo"]["status"], "EXECUTED",
        "the wrong-area estimator still yields a coherent, re-deriving EXECUTED interval"
    );
    let verify_fail = buildc()
        .args(["receipt", "verify"])
        .arg(&fail_receipt)
        .output()
        .expect("verify mc-executed negative fixture");
    assert!(
        verify_fail.status.success(),
        "a faithfully reproduced FAIL_EXPECTED must verify (exit 0)\nstderr:\n{}",
        String::from_utf8_lossy(&verify_fail.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn mc_executed_without_full_declaration_is_refused() {
    if !c_backend_ready() {
        eprintln!("skipping mc_executed_without_full_declaration_is_refused: C backend not ready");
        return;
    }
    let dir = std::env::temp_dir().join(format!(
        "buildlang_sci_mc_executed_refuse_partial_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create fixture dir");

    let refused = buildc()
        .arg("run")
        .arg(repo_example("mc_pi_rejection_executed.bld"))
        .args(["--emit-receipt"])
        .arg(dir.join("partial.json"))
        .args([
            "--invariant",
            "non-negative",
            "--seed",
            "42",
            "--mc-executed",
        ])
        .output()
        .expect("run --mc-executed with no other mc flags");
    assert!(
        !refused.status.success(),
        "--mc-executed alone must be refused"
    );
    assert!(
        String::from_utf8_lossy(&refused.stderr)
            .contains("requires the full Monte Carlo declaration"),
        "the refusal must name the all-or-nothing contract\nstderr:\n{}",
        String::from_utf8_lossy(&refused.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn mc_executed_clopper_pearson_is_refused() {
    if !c_backend_ready() {
        eprintln!("skipping mc_executed_clopper_pearson_is_refused: C backend not ready");
        return;
    }
    let dir = std::env::temp_dir().join(format!(
        "buildlang_sci_mc_executed_refuse_clopper_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create fixture dir");

    let refused = buildc()
        .arg("run")
        .arg(repo_example("mc_pi_rejection_executed.bld"))
        .args(["--emit-receipt"])
        .arg(dir.join("clopper.json"))
        .args(["--invariant", "non-negative", "--seed", "42"])
        .args([
            "--mc-executed",
            "--mc-estimator",
            "proportion",
            "--mc-samples",
            "2000",
            "--mc-interval",
            "clopper-pearson-95",
        ])
        .output()
        .expect("run --mc-executed with clopper-pearson-95");
    assert!(
        !refused.status.success(),
        "clopper-pearson-95 must be refused (not executable in v1)"
    );
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(
        stderr.contains("not in the EXECUTED executable vocabulary")
            || stderr.contains("inverse incomplete beta"),
        "the refusal must name the unexecutable method\nstderr:\n{}",
        stderr
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn mc_executed_declared_samples_mismatching_witnessed_trials_is_refused() {
    if !c_backend_ready() {
        eprintln!(
            "skipping mc_executed_declared_samples_mismatching_witnessed_trials_is_refused: C backend not ready"
        );
        return;
    }
    let dir = std::env::temp_dir().join(format!(
        "buildlang_sci_mc_executed_refuse_denominator_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create fixture dir");

    // A tiny kernel that prints exactly 5 post-burn rows (trials_final = 5),
    // declared under --mc-samples 2000: the witnessed denominator does not
    // equal the declared one.
    let src = "fn main() ~ Console + Random {\n\
               let n: i32 = 5;\n\
               let mut inside: i32 = 0;\n\
               let mut k: i32 = 0;\n\
               while k < n {\n\
               let x: f64 = random_f64();\n\
               if x < 0.5 { inside = inside + 1; }\n\
               k = k + 1;\n\
               println!(\"{} {} {}\", 1.0, inside, k);\n\
               }\n\
               }\n";
    let path = dir.join("mc_five_rows.bld");
    fs::write(&path, src).expect("write mc_five_rows.bld");

    let refused = buildc()
        .arg("run")
        .arg(&path)
        .args(["--emit-receipt"])
        .arg(dir.join("mismatch.json"))
        .args(["--invariant", "non-negative", "--seed", "42"])
        .args([
            "--mc-executed",
            "--mc-estimator",
            "proportion",
            "--mc-samples",
            "2000",
            "--mc-interval",
            "wilson-95",
        ])
        .output()
        .expect("run --mc-executed on a kernel with fewer draws than declared");
    assert!(
        !refused.status.success(),
        "a witnessed/declared denominator mismatch must be refused"
    );
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("does not equal the declared samples"),
        "the refusal must name the denominator mismatch\nstderr:\n{}",
        String::from_utf8_lossy(&refused.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn mc_executed_incoherent_successes_jump_is_refused() {
    if !c_backend_ready() {
        eprintln!("skipping mc_executed_incoherent_successes_jump_is_refused: C backend not ready");
        return;
    }
    let dir = std::env::temp_dir().join(format!(
        "buildlang_sci_mc_executed_refuse_jump_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create fixture dir");

    // A tiny kernel whose "successes" column jumps by 2 between row 0 (k=1,
    // successes=0) and row 1 (k=2, successes=2): never exceeds trials at any
    // row, but is not a coherent cumulative Bernoulli count (a delta of 2
    // is neither 0 nor 1).
    let src = "fn main() ~ Console + Random {\n\
               let mut k: i32 = 0;\n\
               while k < 3 {\n\
               let x: f64 = random_f64();\n\
               let _unused: f64 = x;\n\
               k = k + 1;\n\
               let mut succ: i32 = 0;\n\
               if k >= 2 { succ = k; }\n\
               println!(\"{} {} {}\", 1.0, succ, k);\n\
               }\n\
               }\n";
    let path = dir.join("mc_jump.bld");
    fs::write(&path, src).expect("write mc_jump.bld");

    let refused = buildc()
        .arg("run")
        .arg(&path)
        .args(["--emit-receipt"])
        .arg(dir.join("jump.json"))
        .args(["--invariant", "non-negative", "--seed", "42"])
        .args([
            "--mc-executed",
            "--mc-estimator",
            "proportion",
            "--mc-samples",
            "3",
            "--mc-interval",
            "wilson-95",
        ])
        .output()
        .expect("run --mc-executed on a kernel with an incoherent successes jump");
    assert!(
        !refused.status.success(),
        "a successes column jumping by 2 must be refused"
    );
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(
        stderr.contains("does not follow") && stderr.contains("by 0 or 1"),
        "the refusal must name the incoherent jump\nstderr:\n{}",
        stderr
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn mc_executed_explicit_columns_other_than_three_is_refused() {
    if !c_backend_ready() {
        eprintln!(
            "skipping mc_executed_explicit_columns_other_than_three_is_refused: C backend not ready"
        );
        return;
    }
    let dir = std::env::temp_dir().join(format!(
        "buildlang_sci_mc_executed_refuse_columns_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create fixture dir");

    let refused = buildc()
        .arg("run")
        .arg(repo_example("mc_pi_rejection_executed.bld"))
        .args(["--emit-receipt"])
        .arg(dir.join("columns.json"))
        .args([
            "--invariant",
            "non-negative",
            "--seed",
            "42",
            "--columns",
            "2",
        ])
        .args([
            "--mc-executed",
            "--mc-estimator",
            "proportion",
            "--mc-samples",
            "2000",
            "--mc-interval",
            "wilson-95",
        ])
        .output()
        .expect("run --mc-executed with an explicit --columns 2");
    assert!(
        !refused.status.success(),
        "an explicit --columns other than 3 must be refused under --mc-executed"
    );
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("needs --columns 3"),
        "the refusal must name the required column count\nstderr:\n{}",
        String::from_utf8_lossy(&refused.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn mc_executed_normal_approx_degenerate_boundary_is_refused() {
    if !c_backend_ready() {
        eprintln!(
            "skipping mc_executed_normal_approx_degenerate_boundary_is_refused: C backend not ready"
        );
        return;
    }
    let dir = std::env::temp_dir().join(format!(
        "buildlang_sci_mc_executed_refuse_degenerate_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create fixture dir");

    // A tiny kernel whose counter never fails a single draw: successes ==
    // trials on every row, so the final proportion sits at the 1.0 boundary.
    let src = "fn main() ~ Console + Random {\n\
               let n: i32 = 5;\n\
               let mut k: i32 = 0;\n\
               while k < n {\n\
               let x: f64 = random_f64();\n\
               let _unused: f64 = x;\n\
               k = k + 1;\n\
               println!(\"{} {} {}\", 1.0, k, k);\n\
               }\n\
               }\n";
    let path = dir.join("mc_always_succeeds.bld");
    fs::write(&path, src).expect("write mc_always_succeeds.bld");

    let refused = buildc()
        .arg("run")
        .arg(&path)
        .args(["--emit-receipt"])
        .arg(dir.join("degenerate.json"))
        .args(["--invariant", "non-negative", "--seed", "42"])
        .args([
            "--mc-executed",
            "--mc-estimator",
            "proportion",
            "--mc-samples",
            "5",
            "--mc-interval",
            "normal-approx-95",
        ])
        .output()
        .expect("run --mc-executed on a boundary-proportion kernel with normal-approx-95");
    assert!(
        !refused.status.success(),
        "a boundary proportion under normal-approx-95 must be refused"
    );
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(
        stderr.contains("degenerate") && stderr.contains("wilson-95"),
        "the refusal must name the degeneracy and point at wilson-95\nstderr:\n{}",
        stderr
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn mc_executed_refused_with_gpu() {
    // No `c_backend_ready()` gate needed: this refusal fires on CLI shape
    // before any GPU device probe or receipt work (mirroring the existing
    // --gpu/--budget-wall-seconds composition test).
    let dir = std::env::temp_dir().join(format!(
        "buildlang_sci_mc_executed_refuse_gpu_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create fixture dir");

    let refused = buildc()
        .arg("run")
        .arg(repo_example("decay_cross_backend.bld"))
        .args([
            "--gpu",
            "--mc-executed",
            "--mc-estimator",
            "proportion",
            "--mc-samples",
            "2000",
            "--mc-interval",
            "wilson-95",
        ])
        .output()
        .expect("run --gpu --mc-executed together");
    assert!(
        !refused.status.success(),
        "--gpu combined with --mc-executed must be refused"
    );
    assert!(
        String::from_utf8_lossy(&refused.stderr)
            .contains("--mc-* flags are not supported with --gpu"),
        "the refusal must name the gpu/mc composition\nstderr:\n{}",
        String::from_utf8_lossy(&refused.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn mc_executed_refused_with_cross_backend() {
    if !c_backend_ready() {
        eprintln!("skipping mc_executed_refused_with_cross_backend: C backend not ready");
        return;
    }
    let dir = std::env::temp_dir().join(format!(
        "buildlang_sci_mc_executed_refuse_cross_backend_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create fixture dir");

    let refused = buildc()
        .arg("run")
        .arg(repo_example("decay_cross_backend.bld"))
        .args(["--emit-receipt"])
        .arg(dir.join("cb_mc_executed.json"))
        .args(["--cross-backend", "rust", "--invariant", "cross-backend"])
        .args([
            "--mc-executed",
            "--mc-estimator",
            "proportion",
            "--mc-samples",
            "2000",
            "--mc-interval",
            "wilson-95",
        ])
        .output()
        .expect("run --cross-backend --mc-executed together");
    assert!(
        !refused.status.success(),
        "--cross-backend combined with --mc-executed must be refused"
    );
    // The column-count gate is the FIRST refusal to fire here (Task 3's
    // `column_count_matches_invariant` update makes the cross-backend arm
    // require `!mc_executed`, so it refuses for ANY --columns value once
    // mc_executed is true), ahead of the later `mc_flag_count > 0` check
    // inside the `--cross-backend` block. Both gates independently cover
    // this composition; the column gate simply wins the race.
    assert!(
        String::from_utf8_lossy(&refused.stderr)
            .contains("--invariant cross-backend needs --columns 2"),
        "the refusal must name the cross-backend column contract\nstderr:\n{}",
        String::from_utf8_lossy(&refused.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn budget_declaration_round_trips_and_pins_the_admission_contract() {
    if !c_backend_ready() {
        eprintln!("skipping budget_declaration_round_trips_and_pins_the_admission_contract: C backend not ready");
        return;
    }
    let dir = std::env::temp_dir().join(format!("buildlang_sci_budget_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create budget fixture dir");

    let budget_flags = ["--budget-steps", "60000", "--budget-consumed", "495"];

    // POSITIVE: the greedy kernel under the full declaration PASSes, the
    // block is sealed with the declared facts plus the boundary rule
    // (NOT_PROVES_OPTIMALITY label, optimality not_claimed entry), and the
    // receipt verifies.
    let pass_receipt = dir.join("greedy.json");
    let emit_pass = buildc()
        .arg("run")
        .arg(repo_example("greedy_change_budget.bld"))
        .args(["--emit-receipt"])
        .arg(&pass_receipt)
        .args([
            "--invariant",
            "non-negative",
            "--metric",
            "slack",
            "--problem",
            "greedy-change-budget",
        ])
        .args(budget_flags)
        .output()
        .expect("emit budget PASS receipt");
    assert!(
        emit_pass.status.success(),
        "emitting the budget PASS receipt should succeed\nstderr:\n{}",
        String::from_utf8_lossy(&emit_pass.stderr)
    );
    let pass: serde_json::Value =
        serde_json::from_slice(&fs::read(&pass_receipt).expect("read PASS receipt")).unwrap();
    assert_eq!(pass["receipt_status"], "PASS");
    assert_eq!(pass["budget"]["steps_limit"], 60000);
    assert_eq!(pass["budget"]["steps_consumed"], 495);
    assert_eq!(pass["budget"]["exhausted"], false);
    assert_eq!(pass["budget"]["status"], "DECLARED");
    assert!(pass["labels"]
        .as_array()
        .unwrap()
        .iter()
        .any(|l| l == "NOT_PROVES_OPTIMALITY"));
    assert!(pass["not_claimed"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c == "optimality"));
    let verify_pass = buildc()
        .args(["receipt", "verify"])
        .arg(&pass_receipt)
        .output()
        .expect("verify budget PASS receipt");
    assert!(
        verify_pass.status.success(),
        "the budget PASS receipt must verify\nstderr:\n{}",
        String::from_utf8_lossy(&verify_pass.stderr)
    );

    // NEGATIVE fixture: the under-budgeted kernel overruns its own declared
    // step budget for the worst amounts and FAILs as declared.
    let fail_receipt = dir.join("greedy-broken.json");
    let emit_fail = buildc()
        .arg("run")
        .arg(repo_example("greedy_change_budget_broken.bld"))
        .args(["--emit-receipt"])
        .arg(&fail_receipt)
        .args([
            "--invariant",
            "non-negative",
            "--negative-fixture",
            "--metric",
            "slack",
            "--problem",
            "greedy-change-budget",
        ])
        .args(budget_flags)
        .output()
        .expect("emit budget negative fixture");
    assert!(
        emit_fail.status.success(),
        "emitting the budget negative fixture should succeed\nstderr:\n{}",
        String::from_utf8_lossy(&emit_fail.stderr)
    );
    let fail: serde_json::Value =
        serde_json::from_slice(&fs::read(&fail_receipt).expect("read FAIL receipt")).unwrap();
    assert_eq!(fail["receipt_status"], "FAIL_EXPECTED");
    assert!(
        fail["invariant"]["observed"]["violation_count"]
            .as_u64()
            .unwrap()
            > 0
    );
    let verify_fail = buildc()
        .args(["receipt", "verify"])
        .arg(&fail_receipt)
        .output()
        .expect("verify budget negative fixture");
    assert!(
        verify_fail.status.success(),
        "a faithfully reproduced FAIL_EXPECTED must verify (exit 0)\nstderr:\n{}",
        String::from_utf8_lossy(&verify_fail.stderr)
    );

    // FAIL CLOSED: each partial declaration (either flag alone) is refused.
    for partial in [
        &["--budget-steps", "60000"][..],
        &["--budget-consumed", "495"][..],
    ] {
        let refused = buildc()
            .arg("run")
            .arg(repo_example("greedy_change_budget.bld"))
            .args(["--emit-receipt"])
            .arg(dir.join("partial.json"))
            .args(["--invariant", "non-negative"])
            .args(partial)
            .output()
            .expect("run partial budget declaration");
        assert!(
            !refused.status.success(),
            "a partial budget declaration must be refused: {partial:?}"
        );
        assert!(
            String::from_utf8_lossy(&refused.stderr).contains("together"),
            "the refusal must name the all-or-nothing contract\nstderr:\n{}",
            String::from_utf8_lossy(&refused.stderr)
        );
    }

    // FAIL CLOSED: a zero ceiling is refused before anything runs.
    let zero = buildc()
        .arg("run")
        .arg(repo_example("greedy_change_budget.bld"))
        .args(["--emit-receipt"])
        .arg(dir.join("zero.json"))
        .args(["--invariant", "non-negative"])
        .args(["--budget-steps", "0", "--budget-consumed", "0"])
        .output()
        .expect("run zero-ceiling budget declaration");
    assert!(
        !zero.status.success(),
        "a zero budget ceiling must be refused"
    );
    assert!(
        String::from_utf8_lossy(&zero.stderr).contains("ceiling"),
        "the refusal must name the ceiling\nstderr:\n{}",
        String::from_utf8_lossy(&zero.stderr)
    );

    // FAIL CLOSED: consumption above the ceiling is incoherent and refused.
    let over = buildc()
        .arg("run")
        .arg(repo_example("greedy_change_budget.bld"))
        .args(["--emit-receipt"])
        .arg(dir.join("over.json"))
        .args(["--invariant", "non-negative"])
        .args(["--budget-steps", "10", "--budget-consumed", "11"])
        .output()
        .expect("run over-consumed budget declaration");
    assert!(
        !over.status.success(),
        "a consumption above the ceiling must be refused"
    );
    assert!(
        String::from_utf8_lossy(&over.stderr).contains("incoherent"),
        "the refusal must name the incoherence\nstderr:\n{}",
        String::from_utf8_lossy(&over.stderr)
    );

    // FAIL CLOSED: the claim-language rule refuses free text claiming
    // optimality on a budgeted run.
    let claims_optimal = buildc()
        .arg("run")
        .arg(repo_example("greedy_change_budget.bld"))
        .args(["--emit-receipt"])
        .arg(dir.join("optimal.json"))
        .args(["--invariant", "non-negative"])
        .args(budget_flags)
        .args(["--problem", "greedy-optimal-change"])
        .output()
        .expect("run budget declaration with optimal-claiming problem text");
    assert!(
        !claims_optimal.status.success(),
        "free text claiming optimality on a budgeted run must be refused"
    );
    assert!(
        String::from_utf8_lossy(&claims_optimal.stderr).contains("optimality"),
        "the refusal must name the optimality claim\nstderr:\n{}",
        String::from_utf8_lossy(&claims_optimal.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn wall_metering_seals_and_reports() {
    if !c_backend_ready() {
        eprintln!("skipping wall_metering_seals_and_reports: C backend not ready");
        return;
    }
    let dir = std::env::temp_dir().join(format!("buildlang_sci_wall_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create wall-metering fixture dir");

    // POSITIVE: the greedy kernel under the full budget declaration plus a
    // generous wall ceiling (300s, far above any real run) PASSes, and the
    // receipt seals both the EXECUTED wall time and the DECLARED ceiling
    // with a correctly-derived wall_exceeded.
    let receipt_path = dir.join("greedy-wall.json");
    let emit = buildc()
        .arg("run")
        .arg(repo_example("greedy_change_budget.bld"))
        .args(["--emit-receipt"])
        .arg(&receipt_path)
        .args([
            "--invariant",
            "non-negative",
            "--metric",
            "slack",
            "--problem",
            "greedy-change-budget",
        ])
        .args(["--budget-steps", "60000", "--budget-consumed", "495"])
        .args(["--budget-wall-seconds", "300"])
        .output()
        .expect("emit wall-metering receipt");
    assert!(
        emit.status.success(),
        "emitting the wall-metering receipt should succeed\nstderr:\n{}",
        String::from_utf8_lossy(&emit.stderr)
    );
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipt_path).expect("read wall-metering receipt"))
            .unwrap();
    let wall_seconds = receipt["runtime_state"]["wall_seconds"]
        .as_f64()
        .expect("runtime_state.wall_seconds must be a number");
    // wall_seconds is rounded to 3 decimals at emit, so a sub-0.5ms run
    // legitimately rounds to exactly 0.0; assert presence, finiteness, and
    // non-negativity instead of a strict positivity bound a fast-enough run
    // could round below (the 0.5ms rounding floor).
    assert!(
        wall_seconds.is_finite() && wall_seconds >= 0.0,
        "runtime_state.wall_seconds must be a finite, non-negative number, got {wall_seconds}"
    );
    assert_eq!(receipt["budget"]["wall_seconds_limit"], 300.0);
    assert_eq!(receipt["budget"]["wall_exceeded"], false);

    let verify = buildc()
        .args(["receipt", "verify"])
        .arg(&receipt_path)
        .output()
        .expect("verify wall-metering receipt");
    assert!(
        verify.status.success(),
        "the wall-metering receipt must verify\nstderr:\n{}",
        String::from_utf8_lossy(&verify.stderr)
    );
    assert!(
        String::from_utf8_lossy(&verify.stdout).contains("wall_seconds"),
        "verify stdout must mention wall_seconds\nstdout:\n{}",
        String::from_utf8_lossy(&verify.stdout)
    );

    // FAIL CLOSED: --budget-wall-seconds without the steps pair is refused
    // (the wall ceiling is a member of the budget declaration, not a
    // freestanding one).
    let unpaired = buildc()
        .arg("run")
        .arg(repo_example("greedy_change_budget.bld"))
        .args(["--emit-receipt"])
        .arg(dir.join("unpaired.json"))
        .args(["--invariant", "non-negative"])
        .args(["--budget-wall-seconds", "300"])
        .output()
        .expect("run wall ceiling without the steps pair");
    assert!(
        !unpaired.status.success(),
        "--budget-wall-seconds without the steps pair must be refused"
    );
    assert!(
        String::from_utf8_lossy(&unpaired.stderr).contains("budget declaration"),
        "the refusal must name the missing budget declaration\nstderr:\n{}",
        String::from_utf8_lossy(&unpaired.stderr)
    );

    // FAIL CLOSED: a zero wall ceiling is refused before anything runs.
    let zero = buildc()
        .arg("run")
        .arg(repo_example("greedy_change_budget.bld"))
        .args(["--emit-receipt"])
        .arg(dir.join("zero-wall.json"))
        .args(["--invariant", "non-negative"])
        .args(["--budget-steps", "60000", "--budget-consumed", "495"])
        .args(["--budget-wall-seconds", "0"])
        .output()
        .expect("run zero wall ceiling");
    assert!(
        !zero.status.success(),
        "a zero wall ceiling must be refused"
    );
    assert!(
        String::from_utf8_lossy(&zero.stderr).contains("positive"),
        "the refusal must name the positive/finite contract\nstderr:\n{}",
        String::from_utf8_lossy(&zero.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn cross_backend_receipt_round_trips_and_pins_the_pairing() {
    if !c_backend_ready() {
        eprintln!(
            "skipping cross_backend_receipt_round_trips_and_pins_the_pairing: C backend not ready"
        );
        return;
    }
    if !rustc_available() {
        eprintln!(
            "skipping cross_backend_receipt_round_trips_and_pins_the_pairing: rustc not available"
        );
        return;
    }
    let dir = std::env::temp_dir().join(format!(
        "buildlang_sci_cross_backend_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create cross-backend fixture dir");

    // POSITIVE: the decay kernel through both backends agrees, sealed as a
    // 2-column relation, and verifies.
    let receipt_path = dir.join("decay_cross_backend.json");
    let emit = buildc()
        .arg("run")
        .arg(repo_example("decay_cross_backend.bld"))
        .args(["--emit-receipt"])
        .arg(&receipt_path)
        .args(["--cross-backend", "rust", "--invariant", "cross-backend"])
        .args(["--problem", "decay-cross-backend"])
        .output()
        .expect("emit cross-backend receipt");
    assert!(
        emit.status.success(),
        "emitting the cross-backend receipt should succeed\nstderr:\n{}",
        String::from_utf8_lossy(&emit.stderr)
    );
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipt_path).expect("read cross-backend receipt"))
            .unwrap();
    assert_eq!(receipt["receipt_status"], "PASS");
    assert_eq!(receipt["invariant"]["name"], "cross_backend_columns_agree");
    assert_eq!(receipt["measurement"]["column_count"], 2);
    assert_eq!(receipt["measurement"]["count"], 80);
    assert_eq!(receipt["cross_backend"]["secondary_target"], "rust");
    assert_eq!(receipt["cross_backend"]["status"], "EXECUTED");
    assert_eq!(receipt["cross_backend"]["secondary_exit_code"], 0);
    for digest_field in [
        "secondary_toolchain_digest",
        "secondary_executable_digest",
        "secondary_raw_stdout_digest",
    ] {
        let digest = &receipt["cross_backend"][digest_field];
        assert_eq!(digest["algorithm"], "sha256");
        assert_eq!(
            digest["hex"].as_str().map(|h| h.len()),
            Some(64),
            "{digest_field} must be a well-formed sha256"
        );
    }

    let verify = buildc()
        .args(["receipt", "verify"])
        .arg(&receipt_path)
        .output()
        .expect("verify cross-backend receipt");
    assert!(
        verify.status.success(),
        "the cross-backend receipt must verify\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verify.stdout),
        String::from_utf8_lossy(&verify.stderr)
    );

    // REFUSAL: --cross-backend without --invariant cross-backend.
    let missing_invariant = buildc()
        .arg("run")
        .arg(repo_example("decay_cross_backend.bld"))
        .args(["--emit-receipt"])
        .arg(dir.join("missing_invariant.json"))
        .args(["--cross-backend", "rust"])
        .output()
        .expect("run cross-backend without the paired invariant");
    assert!(
        !missing_invariant.status.success(),
        "--cross-backend without --invariant cross-backend must be refused"
    );
    assert!(
        String::from_utf8_lossy(&missing_invariant.stderr).contains("--invariant cross-backend"),
        "the refusal must name the pairing\nstderr:\n{}",
        String::from_utf8_lossy(&missing_invariant.stderr)
    );

    // REFUSAL: --invariant cross-backend without --cross-backend (vice versa).
    let missing_flag = buildc()
        .arg("run")
        .arg(repo_example("decay_cross_backend.bld"))
        .args(["--emit-receipt"])
        .arg(dir.join("missing_flag.json"))
        .args(["--invariant", "cross-backend"])
        .output()
        .expect("run cross-backend invariant without the flag");
    assert!(
        !missing_flag.status.success(),
        "--invariant cross-backend without --cross-backend must be refused"
    );
    assert!(
        String::from_utf8_lossy(&missing_flag.stderr).contains("--cross-backend"),
        "the refusal must name the pairing\nstderr:\n{}",
        String::from_utf8_lossy(&missing_flag.stderr)
    );

    // REFUSAL: --seed on a cross-backend run.
    let with_seed = buildc()
        .arg("run")
        .arg(repo_example("decay_cross_backend.bld"))
        .args(["--emit-receipt"])
        .arg(dir.join("with_seed.json"))
        .args([
            "--cross-backend",
            "rust",
            "--invariant",
            "cross-backend",
            "--seed",
            "7",
        ])
        .output()
        .expect("run cross-backend with --seed");
    assert!(
        !with_seed.status.success(),
        "--cross-backend with --seed must be refused"
    );
    assert!(
        String::from_utf8_lossy(&with_seed.stderr).contains("--seed"),
        "the refusal must name --seed\nstderr:\n{}",
        String::from_utf8_lossy(&with_seed.stderr)
    );

    // REFUSAL: a Random-observing kernel.
    let random_kernel = buildc()
        .arg("run")
        .arg(repo_example("random_walk_bound.bld"))
        .args(["--emit-receipt"])
        .arg(dir.join("random_kernel.json"))
        .args(["--cross-backend", "rust", "--invariant", "cross-backend"])
        .output()
        .expect("run cross-backend on a Random-observing kernel");
    assert!(
        !random_kernel.status.success(),
        "--cross-backend on a Random-observing kernel must be refused"
    );
    assert!(
        String::from_utf8_lossy(&random_kernel.stderr).contains("Random"),
        "the refusal must name the Random capability\nstderr:\n{}",
        String::from_utf8_lossy(&random_kernel.stderr)
    );

    // REFUSAL: an unsupported target.
    let bad_target = buildc()
        .arg("run")
        .arg(repo_example("decay_cross_backend.bld"))
        .args(["--emit-receipt"])
        .arg(dir.join("bad_target.json"))
        .args(["--cross-backend", "vulkan", "--invariant", "cross-backend"])
        .output()
        .expect("run cross-backend with an unsupported target");
    assert!(
        !bad_target.status.success(),
        "--cross-backend vulkan must be refused"
    );
    assert!(
        String::from_utf8_lossy(&bad_target.stderr).contains("vulkan"),
        "the refusal must name the unsupported target\nstderr:\n{}",
        String::from_utf8_lossy(&bad_target.stderr)
    );

    // REFUSAL: --gpu composed with --cross-backend (finding 2, previously
    // untested composition gate). The two flags name two different secondary
    // lanes (GPU cross-check vs. the rust cross-backend lane), so combining
    // them is refused up front, before any GPU device probe or receipt work.
    let gpu_composition = buildc()
        .arg("run")
        .arg(repo_example("decay_cross_backend.bld"))
        .args(["--emit-receipt"])
        .arg(dir.join("gpu_composition.json"))
        .args([
            "--gpu",
            "--cross-backend",
            "rust",
            "--invariant",
            "cross-backend",
        ])
        .output()
        .expect("run --gpu --cross-backend together");
    assert!(
        !gpu_composition.status.success(),
        "--gpu combined with --cross-backend must be refused"
    );
    assert!(
        String::from_utf8_lossy(&gpu_composition.stderr)
            .contains("--cross-backend is not supported with --gpu"),
        "the refusal must name the gpu/cross-backend composition\nstderr:\n{}",
        String::from_utf8_lossy(&gpu_composition.stderr)
    );

    // REFUSAL: --gpu composed with --budget-wall-seconds (review finding:
    // the gpu/budget composition refusal at main.rs:694-701 had zero test
    // coverage). The GPU cross-check produces no budget block, so the flag
    // is refused up front, before any GPU device probe or receipt work.
    let gpu_budget_wall = buildc()
        .arg("run")
        .arg(repo_example("decay_cross_backend.bld"))
        .args(["--gpu", "--budget-wall-seconds", "300"])
        .output()
        .expect("run --gpu --budget-wall-seconds together");
    assert!(
        !gpu_budget_wall.status.success(),
        "--gpu combined with --budget-wall-seconds must be refused"
    );
    assert!(
        String::from_utf8_lossy(&gpu_budget_wall.stderr)
            .contains("--budget-* flags are not supported with --gpu"),
        "the refusal must name the gpu/budget composition\nstderr:\n{}",
        String::from_utf8_lossy(&gpu_budget_wall.stderr)
    );

    // REFUSAL: rustc unreachable, simulated via RUSTC pointing at a
    // nonexistent binary (the same override `rustc_available` /
    // `rustc_compile_and_run` honor), so this is deterministic rather than
    // depending on PATH manipulation.
    let missing_rustc = buildc()
        .arg("run")
        .arg(repo_example("decay_cross_backend.bld"))
        .args(["--emit-receipt"])
        .arg(dir.join("missing_rustc.json"))
        .args(["--cross-backend", "rust", "--invariant", "cross-backend"])
        .env("RUSTC", "buildlang-cross-backend-missing-rustc-probe")
        .output()
        .expect("run cross-backend with rustc unreachable");
    assert!(
        !missing_rustc.status.success(),
        "a missing rustc must be refused"
    );
    assert!(
        String::from_utf8_lossy(&missing_rustc.stderr).contains("rustc"),
        "the refusal must name rustc\nstderr:\n{}",
        String::from_utf8_lossy(&missing_rustc.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn permitted_secondary_block_compositions_mc_plus_budget_and_budget_plus_cross_backend() {
    // Minor (a) from the final-stack review: mc+budget and budget+cross-backend
    // are both permitted by design (each is orthogonal to the others) and were
    // verified working by hand during the review, but neither combination had
    // a test. One test closes both gaps so a later slice cannot regress either
    // composition silently.
    if !c_backend_ready() {
        eprintln!(
            "skipping permitted_secondary_block_compositions_mc_plus_budget_and_budget_plus_cross_backend: C backend not ready"
        );
        return;
    }
    let dir = std::env::temp_dir().join(format!(
        "buildlang_sci_permitted_combos_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create permitted-combos fixture dir");

    let budget_flags = ["--budget-steps", "60000", "--budget-consumed", "495"];

    // COMBINATION 1: --mc-* declared together with --budget-* (plus --seed,
    // required by the mc/Random pairing). Both blocks are independent
    // admission contracts; nothing in either gate excludes the other.
    let mc_flags = [
        "--mc-estimator",
        "mean",
        "--mc-samples",
        "2000",
        "--mc-interval",
        "normal-approx-95",
    ];
    let mc_budget_receipt = dir.join("mc_plus_budget.json");
    let emit_mc_budget = buildc()
        .arg("run")
        .arg(repo_example("mc_pi_rejection.bld"))
        .args(["--emit-receipt"])
        .arg(&mc_budget_receipt)
        .args([
            "--invariant",
            "non-negative",
            "--metric",
            "slack",
            "--problem",
            "mc-pi-rejection",
            "--seed",
            "42",
        ])
        .args(mc_flags)
        .args(budget_flags)
        .output()
        .expect("emit mc + budget receipt");
    assert!(
        emit_mc_budget.status.success(),
        "emitting mc + budget together should succeed\nstderr:\n{}",
        String::from_utf8_lossy(&emit_mc_budget.stderr)
    );
    let mc_budget: serde_json::Value =
        serde_json::from_slice(&fs::read(&mc_budget_receipt).expect("read mc + budget receipt"))
            .unwrap();
    assert_eq!(mc_budget["receipt_status"], "PASS");
    assert_eq!(mc_budget["monte_carlo"]["estimator"], "mean");
    assert_eq!(mc_budget["monte_carlo"]["samples"], 2000);
    assert_eq!(mc_budget["monte_carlo"]["status"], "DECLARED");
    assert_eq!(mc_budget["budget"]["steps_limit"], 60000);
    assert_eq!(mc_budget["budget"]["steps_consumed"], 495);
    assert_eq!(mc_budget["budget"]["status"], "DECLARED");
    assert_eq!(mc_budget["seed_value"], 42);
    let verify_mc_budget = buildc()
        .args(["receipt", "verify"])
        .arg(&mc_budget_receipt)
        .output()
        .expect("verify mc + budget receipt");
    assert!(
        verify_mc_budget.status.success(),
        "the mc + budget receipt must verify (exit 0)\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verify_mc_budget.stdout),
        String::from_utf8_lossy(&verify_mc_budget.stderr)
    );

    // COMBINATION 2: --budget-* declared together with --cross-backend
    // (requires rustc; skip that half rather than the whole test on an
    // environment gap, matching how the primary cross-backend test handles
    // the same missing-rustc case).
    if !rustc_available() {
        eprintln!(
            "skipping the budget + cross-backend half of permitted_secondary_block_compositions_mc_plus_budget_and_budget_plus_cross_backend: rustc not available"
        );
        let _ = fs::remove_dir_all(&dir);
        return;
    }
    let budget_cross_backend_receipt = dir.join("budget_plus_cross_backend.json");
    let emit_budget_cross_backend = buildc()
        .arg("run")
        .arg(repo_example("decay_cross_backend.bld"))
        .args(["--emit-receipt"])
        .arg(&budget_cross_backend_receipt)
        .args(["--cross-backend", "rust", "--invariant", "cross-backend"])
        .args(["--problem", "decay-cross-backend"])
        .args(budget_flags)
        .output()
        .expect("emit budget + cross-backend receipt");
    assert!(
        emit_budget_cross_backend.status.success(),
        "emitting budget + cross-backend together should succeed\nstderr:\n{}",
        String::from_utf8_lossy(&emit_budget_cross_backend.stderr)
    );
    let budget_cross_backend: serde_json::Value = serde_json::from_slice(
        &fs::read(&budget_cross_backend_receipt).expect("read budget + cross-backend receipt"),
    )
    .unwrap();
    assert_eq!(budget_cross_backend["receipt_status"], "PASS");
    assert_eq!(budget_cross_backend["budget"]["steps_limit"], 60000);
    assert_eq!(budget_cross_backend["budget"]["steps_consumed"], 495);
    assert_eq!(budget_cross_backend["budget"]["status"], "DECLARED");
    assert_eq!(
        budget_cross_backend["cross_backend"]["secondary_target"],
        "rust"
    );
    assert_eq!(budget_cross_backend["cross_backend"]["status"], "EXECUTED");
    assert!(
        budget_cross_backend["labels"]
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l == "NOT_PROVES_OPTIMALITY"),
        "the budget block's NOT_PROVES_OPTIMALITY label must be present"
    );
    let verify_budget_cross_backend = buildc()
        .args(["receipt", "verify"])
        .arg(&budget_cross_backend_receipt)
        .output()
        .expect("verify budget + cross-backend receipt");
    assert!(
        verify_budget_cross_backend.status.success(),
        "the budget + cross-backend receipt must verify (exit 0)\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verify_budget_cross_backend.stdout),
        String::from_utf8_lossy(&verify_budget_cross_backend.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn receipt_verify_self_test_proves_the_verifier_can_fail() {
    if !c_backend_ready() {
        eprintln!(
            "skipping receipt_verify_self_test_proves_the_verifier_can_fail: C backend not ready"
        );
        return;
    }
    let dir = std::env::temp_dir().join(format!("buildlang_sci_selftest_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create self-test fixture dir");

    // Emit a valid receipt, then run the verifier's own can-it-FAIL check: each
    // sealed-field tamper must be rejected with its expected failure_class.
    let receipt = dir.join("funnel.json");
    let emit = buildc()
        .arg("run")
        .arg(repo_example("funnel_probe.bld"))
        .args(["--emit-receipt"])
        .arg(&receipt)
        .args([
            "--invariant",
            "non-negative",
            "--metric",
            "slack",
            "--problem",
            "p",
        ])
        .output()
        .expect("emit receipt for self-test");
    assert!(emit.status.success(), "emitting the receipt should succeed");

    let self_test = buildc()
        .args(["receipt", "verify"])
        .arg(&receipt)
        .arg("--self-test")
        .output()
        .expect("run receipt verify --self-test");
    assert!(
        self_test.status.success(),
        "self-test must pass (every tamper rejected with its expected class)\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&self_test.stdout),
        String::from_utf8_lossy(&self_test.stderr)
    );
    let stdout = String::from_utf8_lossy(&self_test.stdout);
    assert!(
        stdout.contains("10/10 tampers rejected with the expected failure_class"),
        "self-test should report all ten tampers rejected\nstdout:\n{}",
        stdout
    );
    // The taxonomy arms actually exercised must appear in the report.
    for class in [
        "COMPILER_MISMATCH",
        "SEAL_MISMATCH",
        "MALFORMED",
        "FIELD_CONTRACT_VIOLATION",
        "INVARIANT_UNSUPPORTED",
    ] {
        assert!(
            stdout.contains(class),
            "self-test report must exercise {class}\nstdout:\n{stdout}"
        );
    }

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn receipt_chain_build_and_verify_is_tamper_evident() {
    if !c_backend_ready() {
        eprintln!("skipping receipt_chain_build_and_verify_is_tamper_evident: C backend not ready");
        return;
    }
    let dir = std::env::temp_dir().join(format!("buildlang_sci_chain_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create chain fixture dir");

    // Two member receipts from different kernels.
    let emit = |src: &str, out: &std::path::Path, problem: &str| {
        let status = buildc()
            .arg("run")
            .arg(repo_example(src))
            .args(["--emit-receipt"])
            .arg(out)
            .args([
                "--invariant",
                "non-negative",
                "--metric",
                "slack",
                "--problem",
                problem,
            ])
            .output()
            .expect("emit member receipt");
        assert!(status.status.success(), "emitting {src} should succeed");
    };
    let a = dir.join("a.json");
    let b = dir.join("b.json");
    emit("funnel_probe.bld", &a, "a");
    emit("search_bound_binary.bld", &b, "b");
    // Snapshot member b's exact bytes so negative scenarios can restore it
    // (receipt emit is not byte-reproducible: it seals the compiled program's
    // executable digest, which varies across builds, so a re-emit would not
    // reproduce the pinned seal).
    let b_original = fs::read(&b).expect("snapshot member b");

    // Build the chain and verify the clean chain.
    let chain = dir.join("chain.json");
    let build = buildc()
        .args(["receipt", "chain", "build"])
        .arg(&a)
        .arg(&b)
        .arg("-o")
        .arg(&chain)
        .output()
        .expect("build chain");
    assert!(build.status.success(), "chain build should succeed");

    let verify = buildc()
        .args(["receipt", "chain", "verify"])
        .arg(&chain)
        .output()
        .expect("verify chain");
    assert!(
        verify.status.success(),
        "the clean chain must verify\nstderr:\n{}",
        String::from_utf8_lossy(&verify.stderr)
    );

    // Reordering the members breaks the chain seal.
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&chain).expect("read chain")).unwrap();
    let links = manifest["links"].as_array().unwrap().clone();
    manifest["links"] = serde_json::Value::Array(vec![links[1].clone(), links[0].clone()]);
    let reordered = dir.join("reorder.json");
    fs::write(&reordered, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
    let verify_reorder = buildc()
        .args(["receipt", "chain", "verify"])
        .arg(&reordered)
        .output()
        .expect("verify reordered chain");
    assert!(
        !verify_reorder.status.success(),
        "a reordered chain must fail"
    );
    assert!(
        String::from_utf8_lossy(&verify_reorder.stderr).contains("CHAIN_SEAL_MISMATCH"),
        "reordering must report CHAIN_SEAL_MISMATCH\nstderr:\n{}",
        String::from_utf8_lossy(&verify_reorder.stderr)
    );

    // A member whose seal still matches the pin but whose body no longer
    // re-verifies is caught as CHAIN_LINK_UNVERIFIED. Edit a witnessed field
    // WITHOUT touching seal.hex: step 2 (pinned seal) passes, but `receipt verify`
    // on the member recomputes the seal over the edited body and rejects it.
    let mut body: serde_json::Value = serde_json::from_slice(&b_original).unwrap();
    let vc = body["invariant"]["observed"]["violation_count"]
        .as_i64()
        .unwrap_or(0);
    body["invariant"]["observed"]["violation_count"] = serde_json::Value::from(vc + 1);
    fs::write(&b, serde_json::to_vec_pretty(&body).unwrap()).unwrap();
    let verify_unverified = buildc()
        .args(["receipt", "chain", "verify"])
        .arg(&chain)
        .output()
        .expect("verify unverifiable-member chain");
    assert!(
        !verify_unverified.status.success(),
        "an unverifiable member must fail"
    );
    assert!(
        String::from_utf8_lossy(&verify_unverified.stderr).contains("CHAIN_LINK_UNVERIFIED"),
        "a member that does not re-verify must report CHAIN_LINK_UNVERIFIED\nstderr:\n{}",
        String::from_utf8_lossy(&verify_unverified.stderr)
    );

    // Substituting a member (its seal no longer matches the pinned seal) is
    // caught as CHAIN_LINK_TAMPERED. Restore b from its snapshot, then edit its
    // seal.hex so it no longer matches the pin.
    let mut member: serde_json::Value = serde_json::from_slice(&b_original).unwrap();
    member["seal"]["hex"] = serde_json::Value::String("0".repeat(64));
    fs::write(&b, serde_json::to_vec_pretty(&member).unwrap()).unwrap();
    let verify_tampered = buildc()
        .args(["receipt", "chain", "verify"])
        .arg(&chain)
        .output()
        .expect("verify tampered-member chain");
    assert!(
        !verify_tampered.status.success(),
        "a tampered member must fail"
    );
    assert!(
        String::from_utf8_lossy(&verify_tampered.stderr).contains("CHAIN_LINK_TAMPERED"),
        "a substituted member must report CHAIN_LINK_TAMPERED\nstderr:\n{}",
        String::from_utf8_lossy(&verify_tampered.stderr)
    );

    // A missing member file is caught as CHAIN_LINK_MISSING.
    fs::remove_file(&b).expect("delete member");
    let verify_missing = buildc()
        .args(["receipt", "chain", "verify"])
        .arg(&chain)
        .output()
        .expect("verify missing-member chain");
    assert!(
        !verify_missing.status.success(),
        "a missing member must fail"
    );
    assert!(
        String::from_utf8_lossy(&verify_missing.stderr).contains("CHAIN_LINK_MISSING"),
        "a deleted member must report CHAIN_LINK_MISSING\nstderr:\n{}",
        String::from_utf8_lossy(&verify_missing.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn receipt_corpus_asserts_declared_classifications() {
    if !c_backend_ready() {
        eprintln!("skipping receipt_corpus_asserts_declared_classifications: C backend not ready");
        return;
    }
    let dir = std::env::temp_dir().join(format!("buildlang_sci_corpus_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create corpus fixture dir");

    // The shipped corpus manifest stays well-formed and covers every kernel pair.
    let shipped: serde_json::Value = serde_json::from_slice(
        &fs::read(repo_example("scientific-corpus.json")).expect("read shipped corpus"),
    )
    .expect("shipped corpus parses");
    assert_eq!(shipped["schema"], "buildlang-scientific-receipt-corpus/v0");
    assert_eq!(
        shipped["members"].as_array().unwrap().len(),
        29,
        "the shipped corpus should cover all fourteen kernel pairs plus the cross-backend singleton"
    );

    // A small correct manifest: the command emits, verifies, and confirms each
    // member classifies as declared, exiting 0.
    let bin = repo_example("search_bound_binary.bld");
    let lin = repo_example("search_bound_linear.bld");
    let manifest = serde_json::json!({
        "schema": "buildlang-scientific-receipt-corpus/v0",
        "members": [
            {"source": bin.to_string_lossy(), "invariant": "non-negative", "expected_status": "PASS"},
            {"source": lin.to_string_lossy(), "invariant": "non-negative", "negative_fixture": true, "expected_status": "FAIL_EXPECTED"}
        ]
    });
    let good = dir.join("good.json");
    fs::write(&good, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
    let run_good = buildc()
        .args(["receipt", "corpus"])
        .arg(&good)
        .output()
        .expect("run corpus (good)");
    assert!(
        run_good.status.success(),
        "a correct corpus must pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run_good.stdout),
        String::from_utf8_lossy(&run_good.stderr)
    );
    assert!(
        String::from_utf8_lossy(&run_good.stdout).contains("2/2 members classified"),
        "corpus should report 2/2\nstdout:\n{}",
        String::from_utf8_lossy(&run_good.stdout)
    );

    // A mis-declared member (a PASS kernel declared FAIL_EXPECTED) must fail: the
    // corpus is a gate that can fail, not a rubber stamp.
    let mut wrong = manifest.clone();
    wrong["members"][0]["expected_status"] = serde_json::Value::String("FAIL_EXPECTED".to_string());
    let wrong_path = dir.join("wrong.json");
    fs::write(&wrong_path, serde_json::to_vec_pretty(&wrong).unwrap()).unwrap();
    let run_wrong = buildc()
        .args(["receipt", "corpus"])
        .arg(&wrong_path)
        .output()
        .expect("run corpus (wrong)");
    assert!(
        !run_wrong.status.success(),
        "a mis-declared corpus must fail"
    );
    assert!(
        String::from_utf8_lossy(&run_wrong.stderr)
            .contains("did NOT match their declared classification"),
        "a mis-declared corpus must report the mismatch\nstderr:\n{}",
        String::from_utf8_lossy(&run_wrong.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn conserved_band_invariant_round_trips_and_is_distinct() {
    if !c_backend_ready() {
        eprintln!(
            "skipping conserved_band_invariant_round_trips_and_is_distinct: C backend not ready"
        );
        return;
    }
    let dir = std::env::temp_dir().join(format!(
        "buildlang_sci_conserved_band_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create conserved-band fixture dir");

    // POSITIVE: the symplectic (leapfrog) oscillator holds its energy within an
    // O(dt^2) band forever, so `--invariant conserved-band` PASSes.
    let pass_receipt = dir.join("symplectic.json");
    let emit_pass = buildc()
        .arg("run")
        .arg(repo_example("symplectic_oscillator.bld"))
        .args(["--emit-receipt"])
        .arg(&pass_receipt)
        .args([
            "--invariant",
            "conserved-band",
            "--metric",
            "energy",
            "--problem",
            "leapfrog",
        ])
        .output()
        .expect("emit conserved-band PASS receipt");
    assert!(
        emit_pass.status.success(),
        "emitting the conserved-band PASS receipt should succeed\nstderr:\n{}",
        String::from_utf8_lossy(&emit_pass.stderr)
    );
    let pass: serde_json::Value =
        serde_json::from_slice(&fs::read(&pass_receipt).expect("read PASS receipt")).unwrap();
    assert_eq!(pass["invariant"]["name"], "conserved_within_band");
    assert_eq!(pass["receipt_status"], "PASS");
    assert_eq!(pass["invariant"]["observed"]["violation_count"], 0);

    let verify_pass = buildc()
        .args(["receipt", "verify"])
        .arg(&pass_receipt)
        .output()
        .expect("verify conserved-band PASS receipt");
    assert!(
        verify_pass.status.success(),
        "the conserved-band PASS receipt must verify\nstderr:\n{}",
        String::from_utf8_lossy(&verify_pass.stderr)
    );

    // DISTINCTNESS end-to-end: the SAME symplectic energy series FAILS both
    // `bounded` (energy rises above H_0) and `conservation` (energy deviates
    // beyond roundoff). Only conserved-band accepts the O(dt^2) band.
    for (inv, why) in [
        ("bounded", "energy rises above H_0"),
        ("conservation", "energy deviates"),
    ] {
        let r = dir.join(format!("symp_{inv}.json"));
        let emit = buildc()
            .arg("run")
            .arg(repo_example("symplectic_oscillator.bld"))
            .args(["--emit-receipt"])
            .arg(&r)
            .args([
                "--invariant",
                inv,
                "--metric",
                "energy",
                "--problem",
                "leapfrog",
            ])
            .output()
            .expect("emit symplectic under a tighter invariant");
        assert!(emit.status.success());
        let v: serde_json::Value =
            serde_json::from_slice(&fs::read(&r).expect("read receipt")).unwrap();
        assert_eq!(
            v["receipt_status"], "FAIL_UNEXPECTED",
            "the symplectic series must FAIL {inv} ({why}), so conserved-band is distinct"
        );
    }

    // NEGATIVE fixture: explicit Euler injects energy, so it drifts out of the
    // band. With `--negative-fixture` it is a FAIL_EXPECTED receipt that STILL
    // verifies.
    let fail_receipt = dir.join("euler.json");
    let emit_fail = buildc()
        .arg("run")
        .arg(repo_example("euler_oscillator.bld"))
        .args(["--emit-receipt"])
        .arg(&fail_receipt)
        .args([
            "--invariant",
            "conserved-band",
            "--negative-fixture",
            "--metric",
            "energy",
            "--problem",
            "euler",
        ])
        .output()
        .expect("emit conserved-band negative fixture");
    assert!(
        emit_fail.status.success(),
        "emitting the negative fixture should succeed"
    );
    let fail: serde_json::Value =
        serde_json::from_slice(&fs::read(&fail_receipt).expect("read FAIL receipt")).unwrap();
    assert_eq!(fail["receipt_status"], "FAIL_EXPECTED");
    assert!(
        fail["invariant"]["observed"]["violation_count"]
            .as_u64()
            .unwrap()
            > 0
    );

    let verify_fail = buildc()
        .args(["receipt", "verify"])
        .arg(&fail_receipt)
        .output()
        .expect("verify conserved-band negative fixture");
    assert!(
        verify_fail.status.success(),
        "a faithfully reproduced FAIL_EXPECTED must verify (exit 0)\nstderr:\n{}",
        String::from_utf8_lossy(&verify_fail.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn run_emit_receipt_stable_kernel_is_pass() {
    if !c_backend_ready() {
        eprintln!("skipping run_emit_receipt_stable_kernel_is_pass: C backend not ready");
        return;
    }

    let output = buildc()
        .arg("run")
        .arg(repo_example("heat_equation_energy.bld"))
        .arg("--emit-receipt")
        .arg("-")
        .arg("--problem")
        .arg("1d-heat-equation-energy")
        .output()
        .expect("run buildc run --emit-receipt - on stable kernel");

    assert!(
        output.status.success(),
        "emitting a receipt for the stable kernel should succeed\nstderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["schema"], "buildlang-scientific-runtime-receipt/v0");
    assert_eq!(receipt["compiler"], "buildc");
    assert_eq!(receipt["receipt_status"], "PASS");
    assert_eq!(receipt["invariant"]["status"], "PASS");
    assert_eq!(receipt["invariant"]["observed"]["violation_count"], 0);
    assert_eq!(receipt["problem"]["label"], "1d-heat-equation-energy");
    assert_eq!(receipt["measurement"]["count"], 400);

    let seal = receipt["seal"]["hex"].as_str().expect("seal hex string");
    assert_eq!(seal.len(), 64, "seal must be 64 hex chars");
    assert!(
        seal.chars()
            .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase()),
        "seal must be lowercase hex: {seal}"
    );

    let labels = receipt["labels"].as_array().expect("labels array");
    assert!(
        labels.iter().any(|label| label == "NOT_A_NEW_PHYSICAL_LAW"),
        "every receipt must carry NOT_A_NEW_PHYSICAL_LAW; got {labels:?}"
    );

    // The source digest is a 64-hex sha256.
    let digest = receipt["source_digest"]["hex"]
        .as_str()
        .expect("source digest hex");
    assert_eq!(digest.len(), 64);
}

#[test]
fn run_emit_receipt_unstable_negative_fixture_is_fail_expected() {
    if !c_backend_ready() {
        eprintln!(
            "skipping run_emit_receipt_unstable_negative_fixture_is_fail_expected: C backend not ready"
        );
        return;
    }

    let output = buildc()
        .arg("run")
        .arg(repo_example("heat_equation_energy_unstable.bld"))
        .arg("--emit-receipt")
        .arg("-")
        .arg("--negative-fixture")
        .output()
        .expect("run buildc run --emit-receipt - --negative-fixture on unstable kernel");

    assert!(
        output.status.success(),
        "emitting a negative-fixture receipt should succeed\nstderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["receipt_status"], "FAIL_EXPECTED");
    assert_eq!(receipt["invariant"]["status"], "FAIL");
    assert_eq!(receipt["negative_fixture"], true);
    assert!(
        receipt["invariant"]["observed"]["violation_count"]
            .as_u64()
            .unwrap_or(0)
            > 0,
        "the unstable kernel must record at least one energy increase"
    );

    let labels = receipt["labels"].as_array().expect("labels array");
    assert!(labels.iter().any(|label| label == "NOT_A_NEW_PHYSICAL_LAW"));
    assert!(labels.iter().any(|label| label == "NEGATIVE_FIXTURE"));
}

#[test]
fn run_emit_receipt_unstable_without_fixture_is_fail_unexpected() {
    if !c_backend_ready() {
        eprintln!(
            "skipping run_emit_receipt_unstable_without_fixture_is_fail_unexpected: C backend not ready"
        );
        return;
    }

    let output = buildc()
        .arg("run")
        .arg(repo_example("heat_equation_energy_unstable.bld"))
        .arg("--emit-receipt")
        .arg("-")
        .output()
        .expect("run buildc run --emit-receipt - on unstable kernel (no fixture)");

    assert!(output.status.success());
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["receipt_status"], "FAIL_UNEXPECTED");
    assert_eq!(receipt["invariant"]["status"], "FAIL");
    assert_eq!(receipt["negative_fixture"], false);
}

#[test]
fn run_emit_receipt_unknown_invariant_errors() {
    let output = buildc()
        .arg("run")
        .arg(repo_example("heat_equation_energy.bld"))
        .arg("--emit-receipt")
        .arg("-")
        .arg("--invariant")
        .arg("does-not-exist")
        .output()
        .expect("run buildc run --emit-receipt - --invariant does-not-exist");

    assert!(
        !output.status.success(),
        "an unknown --invariant must fail before compiling"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Unknown --invariant"),
        "error should name the unknown invariant; got:\n{stderr}"
    );
}

#[test]
fn run_emit_receipt_to_file_still_echoes_program_stdout() {
    if !c_backend_ready() {
        eprintln!(
            "skipping run_emit_receipt_to_file_still_echoes_program_stdout: C backend not ready"
        );
        return;
    }

    let kernel = repo_example("heat_equation_energy.bld");

    // Baseline: plain `run` stdout with no --emit-receipt.
    let baseline = buildc()
        .arg("run")
        .arg(&kernel)
        .output()
        .expect("baseline buildc run");
    assert!(baseline.status.success());
    let baseline_stdout = String::from_utf8_lossy(&baseline.stdout).replace("\r\n", "\n");

    // With --emit-receipt to a FILE, the program's stdout must still appear on
    // real stdout byte-for-byte (the receipt goes to the file, not stdout).
    let receipt_path =
        std::env::temp_dir().join(format!("buildlang_sci_receipt_{}.json", std::process::id()));
    let emitted = buildc()
        .arg("run")
        .arg(&kernel)
        .arg("--emit-receipt")
        .arg(&receipt_path)
        .output()
        .expect("buildc run --emit-receipt <file>");
    assert!(
        emitted.status.success(),
        "emit-to-file run should succeed\nstderr:\n{}",
        String::from_utf8_lossy(&emitted.stderr)
    );
    let emitted_stdout = String::from_utf8_lossy(&emitted.stdout).replace("\r\n", "\n");

    assert_eq!(
        emitted_stdout, baseline_stdout,
        "emit-to-file must echo the program stdout identically to plain run"
    );

    // The receipt file parses and re-verifies its own seal shape.
    let bytes = fs::read(&receipt_path).expect("read emitted receipt file");
    let _ = fs::remove_file(&receipt_path);
    let receipt: serde_json::Value =
        serde_json::from_slice(&bytes).expect("receipt file is valid JSON");
    assert_eq!(receipt["receipt_status"], "PASS");
    assert_eq!(receipt["seal"]["hex"].as_str().map(str::len), Some(64));
}

// =============================================================================
// SCIENTIFIC-RUNTIME RECEIPT VERIFY (T3): re-run + re-check round trip
// =============================================================================

/// Copy the stable heat-equation kernel into a fresh temp dir so a test can
/// tamper the source file without disturbing the shared example. Returns the
/// temp dir and the copied `.bld` path.
fn stage_stable_kernel(label: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "buildlang_sci_verify_{}_{}",
        label,
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create scientific verify fixture dir");
    let src = repo_example("heat_equation_energy.bld");
    let staged = dir.join("heat_equation_energy.bld");
    fs::copy(&src, &staged).expect("copy stable kernel into fixture dir");
    (dir, staged)
}

/// Emit a stable scientific-runtime receipt for the staged kernel to `receipt`.
fn emit_stable_receipt(staged: &Path, receipt: &Path) {
    let emitted = buildc()
        .arg("run")
        .arg(staged)
        .arg("--emit-receipt")
        .arg(receipt)
        .arg("--problem")
        .arg("1d-heat-equation-energy")
        .output()
        .expect("emit stable scientific receipt");
    assert!(
        emitted.status.success(),
        "emitting the stable receipt should succeed\nstderr:\n{}",
        String::from_utf8_lossy(&emitted.stderr)
    );
}

#[test]
fn receipt_verify_scientific_stable_receipt_matches() {
    if !c_backend_ready() {
        eprintln!("skipping receipt_verify_scientific_stable_receipt_matches: C backend not ready");
        return;
    }

    let (dir, staged) = stage_stable_kernel("match");
    let receipt = dir.join("receipt.json");
    emit_stable_receipt(&staged, &receipt);

    let verify = buildc()
        .args(["receipt", "verify"])
        .arg(&receipt)
        .output()
        .expect("verify stable scientific receipt");

    let _ = fs::remove_dir_all(&dir);

    assert!(
        verify.status.success(),
        "a valid stable scientific receipt must verify\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verify.stdout),
        String::from_utf8_lossy(&verify.stderr)
    );
    let stdout = String::from_utf8_lossy(&verify.stdout);
    assert!(
        stdout.contains("MATCH"),
        "human output should report MATCH:\n{stdout}"
    );
}

#[test]
fn receipt_verify_scientific_json_reports_match() {
    if !c_backend_ready() {
        eprintln!("skipping receipt_verify_scientific_json_reports_match: C backend not ready");
        return;
    }

    let (dir, staged) = stage_stable_kernel("json");
    let receipt = dir.join("receipt.json");
    emit_stable_receipt(&staged, &receipt);

    let verify = buildc()
        .args(["receipt", "verify", "--json"])
        .arg(&receipt)
        .output()
        .expect("verify stable scientific receipt (--json)");

    let _ = fs::remove_dir_all(&dir);

    assert!(
        verify.status.success(),
        "valid scientific receipt must verify (--json)\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verify.stdout),
        String::from_utf8_lossy(&verify.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&verify.stdout).expect("json verify output is JSON");
    assert_eq!(report["status"], "match");
    assert_eq!(report["schema"], "buildlang-scientific-runtime-receipt/v0");
}

#[test]
fn receipt_verify_scientific_detects_invariant_status_tamper() {
    if !c_backend_ready() {
        eprintln!(
            "skipping receipt_verify_scientific_detects_invariant_status_tamper: C backend not ready"
        );
        return;
    }

    let (dir, staged) = stage_stable_kernel("invtamper");
    let receipt = dir.join("receipt.json");
    emit_stable_receipt(&staged, &receipt);

    // Tamper the stored invariant verdict: flip PASS -> FAIL. The re-run will
    // recompute PASS and must reject the disagreement.
    let raw = fs::read_to_string(&receipt).expect("read emitted receipt");
    let mut value: serde_json::Value = serde_json::from_str(&raw).expect("parse receipt");
    assert_eq!(value["invariant"]["status"], "PASS");
    value["invariant"]["status"] = serde_json::Value::String("FAIL".to_string());
    fs::write(&receipt, serde_json::to_string_pretty(&value).unwrap())
        .expect("write tampered receipt");

    let verify = buildc()
        .args(["receipt", "verify"])
        .arg(&receipt)
        .output()
        .expect("verify tampered scientific receipt");

    let _ = fs::remove_dir_all(&dir);

    assert!(
        !verify.status.success(),
        "an invariant-status tamper must fail verification\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verify.stdout),
        String::from_utf8_lossy(&verify.stderr)
    );
}

#[test]
fn receipt_verify_scientific_detects_source_tamper() {
    if !c_backend_ready() {
        eprintln!("skipping receipt_verify_scientific_detects_source_tamper: C backend not ready");
        return;
    }

    let (dir, staged) = stage_stable_kernel("srctamper");
    let receipt = dir.join("receipt.json");
    emit_stable_receipt(&staged, &receipt);

    // Tamper the SOURCE file after sealing. The re-derived source digest must
    // no longer match the stored one, so verification fails.
    let original = fs::read_to_string(&staged).expect("read staged kernel");
    let tampered = format!("{original}\n// tamper: appended after sealing\n");
    fs::write(&staged, tampered).expect("write tampered source");

    let verify = buildc()
        .args(["receipt", "verify"])
        .arg(&receipt)
        .output()
        .expect("verify receipt against tampered source");

    let _ = fs::remove_dir_all(&dir);

    assert!(
        !verify.status.success(),
        "a source-file tamper must fail verification\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verify.stdout),
        String::from_utf8_lossy(&verify.stderr)
    );
}

// =============================================================================
// SCIENTIFIC-RUNTIME RECEIPT: consolidated emit -> verify -> tamper ->
// negative-fixture round trip (T4). One runnable witness of the whole arc.
// The individual legs are also covered by the focused tests above; this test
// exists so the end-to-end story is a single readable proof, not scattered.
// =============================================================================

#[test]
fn scientific_runtime_receipt_emit_and_verify_round_trip() {
    if !c_backend_ready() {
        eprintln!(
            "skipping scientific_runtime_receipt_emit_and_verify_round_trip: C backend not ready"
        );
        return;
    }

    // 1) Emit a receipt from the STABLE kernel. The invariant holds, so the
    //    receipt is PASS, carries a valid 64-hex seal, and is labelled
    //    NOT_A_NEW_PHYSICAL_LAW (honest scope: an observed-series invariant,
    //    never a physical law).
    let (dir, staged) = stage_stable_kernel("roundtrip");
    let receipt = dir.join("receipt.json");
    emit_stable_receipt(&staged, &receipt);

    let emitted: serde_json::Value = {
        let bytes = fs::read(&receipt).expect("read emitted round-trip receipt");
        serde_json::from_slice(&bytes).expect("emitted receipt is valid JSON")
    };
    assert_eq!(emitted["schema"], "buildlang-scientific-runtime-receipt/v0");
    assert_eq!(emitted["receipt_status"], "PASS");
    assert_eq!(emitted["invariant"]["status"], "PASS");
    assert_eq!(emitted["invariant"]["observed"]["violation_count"], 0);
    let seal = emitted["seal"]["hex"].as_str().expect("seal hex string");
    assert_eq!(seal.len(), 64, "seal must be 64 hex chars");
    assert!(
        seal.chars()
            .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase()),
        "seal must be lowercase hex: {seal}"
    );
    assert!(
        emitted["labels"]
            .as_array()
            .expect("labels array")
            .iter()
            .any(|label| label == "NOT_A_NEW_PHYSICAL_LAW"),
        "every receipt must carry NOT_A_NEW_PHYSICAL_LAW"
    );

    // 2) `receipt verify` on the untouched receipt re-runs the program, re-derives
    //    the source digest, re-checks the invariant, and confirms a clean MATCH.
    let verify_ok = buildc()
        .args(["receipt", "verify"])
        .arg(&receipt)
        .output()
        .expect("verify untouched round-trip receipt");
    assert!(
        verify_ok.status.success(),
        "the untouched receipt must verify\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verify_ok.stdout),
        String::from_utf8_lossy(&verify_ok.stderr)
    );
    assert!(
        String::from_utf8_lossy(&verify_ok.stdout).contains("MATCH"),
        "human verify output should report MATCH"
    );

    // 3) Tamper the sealed receipt (flip the stored invariant verdict PASS -> FAIL)
    //    WITHOUT resealing. Integrity is checked before any sealed field is
    //    interpreted, so a hand-forged receipt is rejected as tampering
    //    (SEAL_MISMATCH) rather than misreported as a verdict "drift": the
    //    re-run never even happens because the body no longer re-seals. (Genuine
    //    non-reproduction of a VALIDLY-sealed receipt is what raises
    //    INVARIANT_STATUS_DRIFT; that path is covered by the unit tests, which
    //    inject a divergent re-run on an untouched receipt.)
    let mut tampered: serde_json::Value = emitted.clone();
    tampered["invariant"]["status"] = serde_json::Value::String("FAIL".to_string());
    fs::write(&receipt, serde_json::to_string_pretty(&tampered).unwrap())
        .expect("write tampered round-trip receipt");
    let verify_tampered = buildc()
        .args(["receipt", "verify"])
        .arg(&receipt)
        .output()
        .expect("verify tampered round-trip receipt");
    assert!(
        !verify_tampered.status.success(),
        "a tampered receipt must fail verification\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verify_tampered.stdout),
        String::from_utf8_lossy(&verify_tampered.stderr)
    );
    assert!(
        String::from_utf8_lossy(&verify_tampered.stderr).contains("failure_class: SEAL_MISMATCH"),
        "an unsealed hand-edit must be caught by the integrity gate\nstderr:\n{}",
        String::from_utf8_lossy(&verify_tampered.stderr)
    );

    // 3b) A receipt with a DUPLICATED key is rejected at load: serde_json is
    //     last-duplicate-wins, so a duplicated verdict key is a seal-forgery
    //     vector (hasher sees one value, permissive reader the other). The
    //     strict loader must refuse it outright.
    let pretty = serde_json::to_string_pretty(&emitted).unwrap();
    let with_dup = pretty.replacen(
        "\"receipt_status\":",
        "\"receipt_status\": \"PASS\",\n  \"receipt_status\":",
        1,
    );
    assert_ne!(pretty, with_dup, "duplication must have been injected");
    fs::write(&receipt, with_dup).expect("write duplicate-key receipt");
    let verify_dup = buildc()
        .args(["receipt", "verify"])
        .arg(&receipt)
        .output()
        .expect("verify duplicate-key receipt");
    assert!(
        !verify_dup.status.success(),
        "a duplicate-key receipt must be rejected"
    );
    assert!(
        String::from_utf8_lossy(&verify_dup.stderr).contains("duplicate object key"),
        "rejection must name the duplicate key\nstderr:\n{}",
        String::from_utf8_lossy(&verify_dup.stderr)
    );

    let _ = fs::remove_dir_all(&dir);

    // 4) The UNSTABLE kernel with --negative-fixture: the invariant is expected to
    //    fail, so the receipt is FAIL_EXPECTED (an expected violation, still an
    //    honest witness), and is additionally labelled NEGATIVE_FIXTURE.
    let negative = buildc()
        .arg("run")
        .arg(repo_example("heat_equation_energy_unstable.bld"))
        .arg("--emit-receipt")
        .arg("-")
        .arg("--negative-fixture")
        .output()
        .expect("emit negative-fixture receipt for the unstable kernel");
    assert!(
        negative.status.success(),
        "emitting a negative-fixture receipt should succeed\nstderr:\n{}",
        String::from_utf8_lossy(&negative.stderr)
    );
    let negative_receipt = receipt_from_stdout(&negative);
    assert_eq!(negative_receipt["receipt_status"], "FAIL_EXPECTED");
    assert_eq!(negative_receipt["invariant"]["status"], "FAIL");
    assert_eq!(negative_receipt["negative_fixture"], true);
    let negative_labels = negative_receipt["labels"].as_array().expect("labels array");
    assert!(
        negative_labels
            .iter()
            .any(|label| label == "NOT_A_NEW_PHYSICAL_LAW"),
        "the negative fixture is still labelled NOT_A_NEW_PHYSICAL_LAW"
    );
    assert!(
        negative_labels
            .iter()
            .any(|label| label == "NEGATIVE_FIXTURE"),
        "the negative fixture must be labelled NEGATIVE_FIXTURE"
    );
}

/// The Telos bridge: `receipt export` re-verifies and emits ONE witnessed
/// Crucible measurement row (deviation derived from the fresh re-run, recheck
/// descriptor sealing the replay command). A receipt that does not reproduce
/// exports nothing.
#[test]
fn receipt_export_emits_a_witnessed_crucible_measurement() {
    if !c_backend_ready() {
        eprintln!(
            "skipping receipt_export_emits_a_witnessed_crucible_measurement: C backend not ready"
        );
        return;
    }
    let (dir, staged) = stage_stable_kernel("export_bridge");
    let receipt = dir.join("receipt.json");
    emit_stable_receipt(&staged, &receipt);

    let out = dir.join("measurement.json");
    let export = buildc()
        .args(["receipt", "export"])
        .arg(&receipt)
        .arg("-o")
        .arg(&out)
        .args(["--claim-id", "heat-energy-monotone"])
        .args(["--claim-sha256", &"a".repeat(64)])
        .output()
        .expect("run receipt export");
    assert!(
        export.status.success(),
        "export of a faithful receipt must succeed\nstderr:\n{}",
        String::from_utf8_lossy(&export.stderr)
    );

    let envelope: serde_json::Value =
        serde_json::from_slice(&fs::read(&out).expect("read export")).expect("parse export");
    assert_eq!(
        envelope["schema"],
        "buildlang-crucible-measurement-export/v0"
    );
    assert_eq!(envelope["invariant_held"], true);
    let row = &envelope["measurements"][0];
    // Witnessed shape: deviation DERIVED (0.0 for the stable kernel), the
    // Crucible ingestion fields all present, and the recheck descriptor
    // sealing the replay command + expected verdict.
    assert_eq!(row["deviation"], 0.0);
    assert_eq!(row["tolerance"], 0.5);
    assert_eq!(row["claim_id"], "heat-energy-monotone");
    assert_eq!(row["method"], "buildc-receipt-verify/reexecuted-v1");
    assert_eq!(row["recheck"]["oracle"], "buildc.receipt.verify");
    assert_eq!(row["recheck"]["expected"]["receipt_status"], "PASS");
    assert!(
        row["recheck"]["receipt_sha256"].as_str().unwrap().len() == 64,
        "the replayed artifact must be hash-bound"
    );

    // Tamper the SOURCE: the receipt no longer reproduces, so export must
    // fail and write nothing (a non-reproducing receipt cannot become a
    // witnessed measurement).
    let source_text = fs::read_to_string(&staged).expect("read staged kernel");
    fs::write(&staged, format!("{source_text}\n// tamper\n")).expect("tamper staged kernel");
    let out2 = dir.join("measurement2.json");
    let export_tampered = buildc()
        .args(["receipt", "export"])
        .arg(&receipt)
        .arg("-o")
        .arg(&out2)
        .output()
        .expect("run receipt export on tampered source");
    assert!(
        !export_tampered.status.success(),
        "a non-reproducing receipt must export nothing"
    );
    assert!(
        !out2.exists(),
        "no measurement file may be written on failure"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Regression: a large-magnitude series must RE-SEAL after a disk round-trip.
/// The unstable kernel's energy blows up to ~1e28; without `float_roundtrip`
/// serde_json re-parsed those f64 values ~1 ULP off, so the receipt (sealed over
/// its in-memory series) failed its own seal re-check at verify. A legitimately
/// emitted FAIL_EXPECTED receipt must verify clean.
#[test]
fn scientific_receipt_large_value_series_reseals_and_verifies() {
    if !c_backend_ready() {
        eprintln!("skipping scientific_receipt_large_value_series_reseals: C backend not ready");
        return;
    }
    let dir = std::env::temp_dir().join(format!("buildlang_sci_largeval_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create large-value fixture dir");
    let staged = dir.join("heat_equation_energy_unstable.bld");
    fs::copy(repo_example("heat_equation_energy_unstable.bld"), &staged)
        .expect("copy unstable kernel");
    let receipt = dir.join("receipt.json");
    let emit = buildc()
        .arg("run")
        .arg(&staged)
        .arg("--emit-receipt")
        .arg(&receipt)
        .arg("--negative-fixture")
        .output()
        .expect("emit large-value receipt");
    assert!(
        emit.status.success(),
        "emitting the unstable receipt should succeed\nstderr:\n{}",
        String::from_utf8_lossy(&emit.stderr)
    );
    let verify = buildc()
        .args(["receipt", "verify"])
        .arg(&receipt)
        .output()
        .expect("verify large-value receipt");
    let _ = fs::remove_dir_all(&dir);
    assert!(
        verify.status.success(),
        "a large-value FAIL_EXPECTED receipt must re-seal and verify clean\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verify.stdout),
        String::from_utf8_lossy(&verify.stderr)
    );
}

/// Regression: `receipt verify` on a faithful receipt that RECORDS an unexpected
/// invariant violation (FAIL_UNEXPECTED) must exit NONZERO, so `verify && deploy`
/// does not deploy on a recorded failure. The receipt reproduces exactly (it is
/// faithful); the exit code reflects the verdict, not just faithfulness.
#[test]
fn scientific_receipt_verify_fails_on_unexpected_invariant_violation() {
    if !c_backend_ready() {
        eprintln!("skipping scientific_receipt_verify_fails_unexpected: C backend not ready");
        return;
    }
    let dir = std::env::temp_dir().join(format!("buildlang_sci_unexpected_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create unexpected fixture dir");
    let staged = dir.join("heat_equation_energy_unstable.bld");
    fs::copy(repo_example("heat_equation_energy_unstable.bld"), &staged)
        .expect("copy unstable kernel");
    let receipt = dir.join("receipt.json");
    // NO --negative-fixture: an unstable run is an UNEXPECTED violation.
    let emit = buildc()
        .arg("run")
        .arg(&staged)
        .arg("--emit-receipt")
        .arg(&receipt)
        .output()
        .expect("emit FAIL_UNEXPECTED receipt");
    assert!(emit.status.success(), "emit should succeed");
    let value: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipt).expect("read receipt")).expect("parse receipt");
    assert_eq!(value["receipt_status"], "FAIL_UNEXPECTED");
    let verify = buildc()
        .args(["receipt", "verify"])
        .arg(&receipt)
        .output()
        .expect("verify FAIL_UNEXPECTED receipt");
    let _ = fs::remove_dir_all(&dir);
    assert!(
        !verify.status.success(),
        "verify must exit nonzero on a faithful FAIL_UNEXPECTED receipt\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verify.stdout),
        String::from_utf8_lossy(&verify.stderr)
    );
}

/// Regression: a program that diverges to a non-finite value must NOT be sealed
/// as a false PASS. It is UNVERIFIABLE (labelled NONFINITE_OBSERVED), only the
/// finite prefix is stored (no JSON `null`), and verify does not exit 0.
#[test]
fn scientific_receipt_nonfinite_run_is_unverifiable() {
    if !c_backend_ready() {
        eprintln!("skipping scientific_receipt_nonfinite_run_is_unverifiable: C backend not ready");
        return;
    }
    let dir = std::env::temp_dir().join(format!("buildlang_sci_nonfinite_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create nonfinite fixture dir");
    let src = dir.join("diverge.bld");
    // Prints a monotone finite prefix then a NaN (0.0/0.0) -> divergence.
    fs::write(
        &src,
        "fn main() ~ Console {\n    \
         println(\"{}\", 4.0);\n    println(\"{}\", 3.0);\n    \
         let z = 0.0;\n    let bad = z / z;\n    println(\"{}\", bad);\n}\n",
    )
    .expect("write diverging kernel");
    let receipt = dir.join("receipt.json");
    let emit = buildc()
        .arg("run")
        .arg(&src)
        .arg("--emit-receipt")
        .arg(&receipt)
        .output()
        .expect("emit diverging receipt");
    assert!(emit.status.success(), "emit should succeed");
    let bytes = fs::read(&receipt).expect("read receipt");
    let text = String::from_utf8_lossy(&bytes);
    assert!(
        !text.contains("null"),
        "observed_values must be finite (no JSON null):\n{text}"
    );
    let value: serde_json::Value = serde_json::from_slice(&bytes).expect("parse receipt");
    assert_eq!(
        value["receipt_status"], "UNVERIFIABLE",
        "a diverged run must be UNVERIFIABLE, not a false PASS"
    );
    assert!(
        value["labels"]
            .as_array()
            .expect("labels array")
            .iter()
            .any(|label| label == "NONFINITE_OBSERVED"),
        "a diverged receipt must be labelled NONFINITE_OBSERVED"
    );
    let verify = buildc()
        .args(["receipt", "verify"])
        .arg(&receipt)
        .output()
        .expect("verify diverging receipt");
    let _ = fs::remove_dir_all(&dir);
    assert!(
        !verify.status.success(),
        "a diverged (UNVERIFIABLE) receipt must not verify as a pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verify.stdout),
        String::from_utf8_lossy(&verify.stderr)
    );
}

#[test]
fn run_units_flag_seals_canonicalized_unit_in_receipt() {
    // The dimensional-analysis core canonicalizes the declared `--units`
    // annotation before it enters the sealed scientific-runtime receipt: an
    // equivalent spelling (`m*s^-1`) must be recorded as the canonical `m/s`,
    // and the sealed receipt must still verify (the unit rides on the existing
    // seal; it does not bypass the accountability layer).
    if !c_backend_ready() {
        eprintln!(
            "skipping run_units_flag_seals_canonicalized_unit_in_receipt: C backend not ready"
        );
        return;
    }
    let dir = std::env::temp_dir().join(format!("buildlang_sci_units_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create units fixture dir");

    let receipt = dir.join("with_units.json");
    let emit = buildc()
        .arg("run")
        .arg(repo_example("energy_identity.bld"))
        .args(["--emit-receipt"])
        .arg(&receipt)
        .args([
            "--invariant",
            "energy-identity",
            "--metric",
            "residual",
            "--units",
            // Deliberately a non-canonical spelling of metres-per-second.
            "m*s^-1",
        ])
        .output()
        .expect("emit receipt with --units");
    assert!(
        emit.status.success(),
        "emitting a receipt with a valid --units should succeed\nstderr:\n{}",
        String::from_utf8_lossy(&emit.stderr)
    );

    let value: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipt).expect("read units receipt")).unwrap();
    assert_eq!(
        value["measurement"]["units"], "m/s",
        "the receipt must seal the CANONICAL unit `m/s`, not the raw `m*s^-1`"
    );

    let verify = buildc()
        .args(["receipt", "verify"])
        .arg(&receipt)
        .output()
        .expect("verify units receipt");
    assert!(
        verify.status.success(),
        "a receipt carrying a canonicalized unit must still verify\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verify.stdout),
        String::from_utf8_lossy(&verify.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn run_units_flag_rejects_unknown_unit_before_compile() {
    // An unknown or malformed unit is an operator error reported immediately,
    // before any compilation work. This needs no C backend: the rejection
    // happens during argument validation. The command must fail and name the
    // offending unit, and it must NOT write a receipt.
    let dir = std::env::temp_dir().join(format!("buildlang_sci_bad_units_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create bad-units fixture dir");
    let receipt = dir.join("should_not_exist.json");

    let output = buildc()
        .arg("run")
        .arg(repo_example("energy_identity.bld"))
        .args(["--emit-receipt"])
        .arg(&receipt)
        .args(["--invariant", "energy-identity", "--units", "furlong"])
        .output()
        .expect("run with an unknown --units");

    assert!(
        !output.status.success(),
        "an unknown unit must fail the command\nstdout:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("furlong"),
        "the error must name the offending unit `furlong`, got:\n{stderr}"
    );
    assert!(
        !receipt.exists(),
        "no receipt should be written when the unit is rejected"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn model_capability_is_fail_closed_and_inadmissible_on_the_receipt_path() {
    if !c_backend_ready() {
        eprintln!("skipping model_capability_is_fail_closed_and_inadmissible_on_the_receipt_path: C backend not ready");
        return;
    }

    // FAIL CLOSED: with no BUILD_MODEL_ENDPOINT in the child env, the first
    // model_complete() call aborts rather than fabricating a completion.
    let no_endpoint = buildc()
        .arg("run")
        .arg(repo_example("model_propose.bld"))
        .env_remove("BUILD_MODEL_ENDPOINT")
        .output()
        .expect("run model kernel with no endpoint");
    assert!(
        !no_endpoint.status.success(),
        "a Model-observing kernel with no endpoint must be refused"
    );
    assert!(
        String::from_utf8_lossy(&no_endpoint.stderr).contains("no endpoint provided"),
        "the refusal must name the missing endpoint\nstderr:\n{}",
        String::from_utf8_lossy(&no_endpoint.stderr)
    );

    // THE WIRE FORMAT: a bare TcpListener stands in for the harness-side
    // shim. It accepts one connection, reads until it sees the '\n'
    // terminator, asserts the received line equals the kernel's EXACT
    // prompt (this is the live smoke test for the wire protocol, not just an
    // exit-code check), writes one completion line, and closes.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral TCP port");
    listener
        .set_nonblocking(true)
        .expect("set listener nonblocking so the accept loop can honor a deadline");
    let port = listener.local_addr().expect("listener local_addr").port();
    let server = std::thread::spawn(move || {
        use std::io::{BufRead, BufReader, Write};
        // Bounded accept, Minor (b) from the final-stack review: a blocking
        // `accept()` here means a regression where the kernel never connects
        // hangs this thread forever, and `server.join()` below hangs with it
        // (this repo has a documented CI-hang failure class, so a
        // hang-shaped test carries real cost). Poll against a deadline
        // instead, so that failure mode becomes a clean panic-and-fail
        // within ~30s rather than a stuck suite.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let (stream, _) = loop {
            match listener.accept() {
                Ok(accepted) => break accepted,
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "timed out after 30s waiting for the model kernel to connect to the TCP shim"
                    );
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(err) => panic!("accept model kernel connection: {err}"),
            }
        };
        stream
            .set_nonblocking(false)
            .expect("set accepted stream back to blocking for the read/write below");
        let mut received_line = String::new();
        BufReader::new(&stream)
            .read_line(&mut received_line)
            .expect("read prompt line up to the newline terminator");
        (&stream)
            .write_all(b"four\n")
            .expect("write completion line");
        drop(stream);
        received_line
    });

    let shimmed = buildc()
        .arg("run")
        .arg(repo_example("model_propose.bld"))
        .env("BUILD_MODEL_ENDPOINT", format!("127.0.0.1:{port}"))
        .output()
        .expect("run model kernel against the TCP shim");
    let received_line = server.join().expect("shim thread should not panic");
    assert_eq!(
        received_line, "propose: 2 + 2 =\n",
        "the shim must receive the kernel's EXACT prompt line, newline-terminated"
    );
    assert!(
        shimmed.status.success(),
        "a shimmed model kernel should exit 0\nstderr:\n{}",
        String::from_utf8_lossy(&shimmed.stderr)
    );
    assert!(
        String::from_utf8_lossy(&shimmed.stdout).contains("four"),
        "stdout should contain the shim's completion\nstdout:\n{}",
        String::from_utf8_lossy(&shimmed.stdout)
    );

    // THE RECEIPT BOUNDARY: --emit-receipt on the same kernel is refused
    // outright, before the program is even compiled (the capability check
    // runs on the checked source, independent of whether an endpoint is
    // reachable), because models propose and oracles dispose.
    let dir = std::env::temp_dir().join(format!("buildlang_model_receipt_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create model receipt fixture dir");
    let receipt = dir.join("model.json");
    let refused = buildc()
        .arg("run")
        .arg(repo_example("model_propose.bld"))
        .args(["--emit-receipt"])
        .arg(&receipt)
        .args(["--invariant", "non-negative"])
        .env_remove("BUILD_MODEL_ENDPOINT")
        .output()
        .expect("run model kernel with --emit-receipt");
    assert!(
        !refused.status.success(),
        "--emit-receipt on a Model-observing kernel must be refused"
    );
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("models propose, oracles dispose"),
        "the refusal must state the propose/dispose rule\nstderr:\n{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert!(
        !receipt.exists(),
        "no receipt should be written when the Model capability is refused"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn check_reports_capability_effect_for_model_call() {
    // Effect-gate coverage for the Model capability, mirroring
    // check_reports_capability_effect_for_gpu_runtime_call: a fixture whose
    // main lacks `~ Model` but calls model_complete() must be rejected by
    // `buildc check`, naming both the effect and the triggering call.
    let fixture = std::env::temp_dir().join(format!(
        "buildlang_model_capability_gate_{}.bld",
        std::process::id()
    ));
    fs::write(&fixture, r#"fn main() { model_complete("x"); }"#)
        .expect("write model capability fixture");

    let output = buildc()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run buildc check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "model_complete call should fail without Model effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Model"),
        "diagnostic should name Model effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("model_complete"),
        "diagnostic should name triggering ambient call:\n{}",
        stderr
    );
}

#[test]
fn five_modes_bind_into_one_chain() {
    if !c_backend_ready() {
        eprintln!("skipping five_modes_bind_into_one_chain: C backend not ready");
        return;
    }
    let dir =
        std::env::temp_dir().join(format!("buildlang_five_modes_chain_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create five-modes chain fixture dir");

    // One receipt per computation mode, each under its shipped exemplar
    // kernel and PASS-declared invariant from examples/scientific-corpus.json.
    let emit_member = |label: &str, src: &str, out: &Path, metric: Option<&str>, extra: &[&str]| {
        let mut cmd = buildc();
        cmd.arg("run")
            .arg(repo_example(src))
            .args(["--emit-receipt"])
            .arg(out);
        if let Some(metric) = metric {
            cmd.args(["--metric", metric]);
        }
        cmd.args(extra);
        let output = cmd
            .output()
            .unwrap_or_else(|err| panic!("emit {label} receipt: {err}"));
        assert!(
            output.status.success(),
            "emitting the {label} receipt should succeed\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let receipt: serde_json::Value = serde_json::from_slice(
            &fs::read(out).unwrap_or_else(|err| panic!("read {label} receipt: {err}")),
        )
        .unwrap_or_else(|err| panic!("parse {label} receipt: {err}"));
        assert_eq!(
            receipt["receipt_status"], "PASS",
            "the {label} receipt must PASS\nreceipt:\n{receipt}"
        );
    };

    let deterministic = dir.join("01-deterministic.json");
    emit_member(
        "deterministic",
        "heat_equation_energy.bld",
        &deterministic,
        Some("energy"),
        &[
            "--invariant",
            "energy-monotone",
            "--problem",
            "1d-heat-equation-energy",
        ],
    );

    let probabilistic_exact = dir.join("02-probabilistic-exact.json");
    emit_member(
        "probabilistic-exact",
        "born_rule_normalization.bld",
        &probabilistic_exact,
        None,
        &[
            "--invariant",
            "conservation",
            "--problem",
            "born-rule-normalization",
        ],
    );

    let stochastic = dir.join("03-stochastic.json");
    emit_member(
        "stochastic",
        "random_walk_bound.bld",
        &stochastic,
        Some("slack"),
        &[
            "--invariant",
            "non-negative",
            "--problem",
            "seeded-random-walk-envelope",
            "--seed",
            "42",
        ],
    );

    let monte_carlo = dir.join("04-monte-carlo.json");
    emit_member(
        "monte-carlo",
        "mc_pi_rejection.bld",
        &monte_carlo,
        Some("slack"),
        &[
            "--invariant",
            "non-negative",
            "--problem",
            "mc-pi-rejection",
            "--seed",
            "42",
            "--mc-estimator",
            "mean",
            "--mc-samples",
            "2000",
            "--mc-interval",
            "normal-approx-95",
        ],
    );

    let heuristic = dir.join("05-heuristic.json");
    emit_member(
        "heuristic",
        "greedy_change_budget.bld",
        &heuristic,
        Some("slack"),
        &[
            "--invariant",
            "non-negative",
            "--problem",
            "greedy-change-budget",
            "--budget-steps",
            "60000",
            "--budget-consumed",
            "495",
        ],
    );

    let mut members = vec![
        deterministic.clone(),
        probabilistic_exact.clone(),
        stochastic.clone(),
        monte_carlo.clone(),
        heuristic.clone(),
    ];

    // The cross-backend bonus link rides only on a local rustc; if it is
    // unavailable, proceed with the five-member chain instead of six,
    // mirroring c_backend_ready's skip idiom for the affected link only.
    let cross_backend_included = rustc_available();
    if cross_backend_included {
        let cross_backend = dir.join("06-cross-backend.json");
        emit_member(
            "cross-backend",
            "decay_cross_backend.bld",
            &cross_backend,
            None,
            &[
                "--cross-backend",
                "rust",
                "--invariant",
                "cross-backend",
                "--problem",
                "decay-cross-backend",
            ],
        );
        members.push(cross_backend);
    } else {
        eprintln!(
            "five_modes_bind_into_one_chain: skipping the cross-backend link, rustc not available; chaining five members instead of six"
        );
    }

    // Bind every member into one tamper-evident chain with the EXISTING
    // chain machinery, and the clean chain verifies.
    let chain = dir.join("chain.json");
    let mut build_cmd = buildc();
    build_cmd
        .args(["receipt", "chain", "build"])
        .args(&members)
        .arg("-o")
        .arg(&chain);
    let build = build_cmd.output().expect("build the five-modes chain");
    assert!(
        build.status.success(),
        "chain build should succeed\nstderr:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );

    let verify = buildc()
        .args(["receipt", "chain", "verify"])
        .arg(&chain)
        .output()
        .expect("verify the clean five-modes chain");
    assert!(
        verify.status.success(),
        "the clean five-modes chain must verify\nstderr:\n{}",
        String::from_utf8_lossy(&verify.stderr)
    );

    // Tamper the stochastic member's stored violation_count WITHOUT
    // re-sealing: seal.hex still matches the pin (step 2 of chain verify
    // passes), but the member's own `receipt verify` recomputes the seal
    // over the edited body and rejects it, so the exact failure class is
    // CHAIN_LINK_UNVERIFIED, not CHAIN_LINK_TAMPERED (that class is reserved
    // for a seal.hex edit / member substitution; see
    // receipt_chain_build_and_verify_is_tamper_evident).
    let mut body: serde_json::Value =
        serde_json::from_slice(&fs::read(&stochastic).expect("read stochastic receipt"))
            .expect("parse stochastic receipt");
    let violation_count = body["invariant"]["observed"]["violation_count"]
        .as_i64()
        .unwrap_or(0);
    body["invariant"]["observed"]["violation_count"] = serde_json::Value::from(violation_count + 1);
    fs::write(&stochastic, serde_json::to_vec_pretty(&body).unwrap())
        .expect("write tampered stochastic receipt");

    let verify_tampered = buildc()
        .args(["receipt", "chain", "verify"])
        .arg(&chain)
        .output()
        .expect("verify the tampered five-modes chain");
    assert!(
        !verify_tampered.status.success(),
        "a chain with a tampered member must fail"
    );
    assert!(
        String::from_utf8_lossy(&verify_tampered.stderr).contains("CHAIN_LINK_UNVERIFIED"),
        "editing a witnessed field without re-sealing must report CHAIN_LINK_UNVERIFIED\nstderr:\n{}",
        String::from_utf8_lossy(&verify_tampered.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}

// =============================================================================
// UNIT-ANNOTATED NUMERIC TYPES (experimental, W6 checker slice one)
//
// `f64<m/s>`-style annotations (`compiler/src/units.rs` grammar) are checked
// at unification boundaries but have ZERO codegen impact: MIR lowers types
// from the AST, and the `WithUnit` node delegates to its base type
// (`codegen/lower/types.rs`). These tests are the mechanical proof: emitted C
// is byte-identical to the same program with the annotations stripped, and
// the check receipt surfaces the new checker rule through the EXISTING
// open-string `diagnostics` field with no schema change.
// =============================================================================

fn units_fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("units")
        .join(name)
}

#[test]
fn units_annotation_erases_to_byte_identical_c() {
    let dir = std::env::temp_dir().join(format!("buildlang_units_byte_id_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create units byte-identity fixture dir");

    let annotated_c = dir.join("annotated.c");
    let plain_c = dir.join("plain.c");

    // `buildc <file> --target c -o <out>` writes raw generated C bytes
    // directly (cmd_compile), no C compiler required to prove erasure.
    let annotated = buildc()
        .arg(units_fixture("units_velocity.bld"))
        .args(["--target", "c", "-o"])
        .arg(&annotated_c)
        .output()
        .expect("compile the unit-annotated fixture to C");
    assert!(
        annotated.status.success(),
        "the unit-annotated fixture must compile to C\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&annotated.stdout),
        String::from_utf8_lossy(&annotated.stderr)
    );

    let plain = buildc()
        .arg(units_fixture("units_velocity_plain.bld"))
        .args(["--target", "c", "-o"])
        .arg(&plain_c)
        .output()
        .expect("compile the plain fixture to C");
    assert!(
        plain.status.success(),
        "the plain fixture must compile to C\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&plain.stdout),
        String::from_utf8_lossy(&plain.stderr)
    );

    let annotated_bytes = fs::read(&annotated_c).expect("read annotated C output");
    let plain_bytes = fs::read(&plain_c).expect("read plain C output");
    assert_eq!(
        annotated_bytes, plain_bytes,
        "a unit annotation must have ZERO codegen impact: the emitted C for \
         `units_velocity.bld` and `units_velocity_plain.bld` must be byte-identical"
    );

    if c_backend_ready() {
        let run_annotated = buildc()
            .arg("run")
            .arg(units_fixture("units_velocity.bld"))
            .output()
            .expect("run the unit-annotated fixture");
        let run_plain = buildc()
            .arg("run")
            .arg(units_fixture("units_velocity_plain.bld"))
            .output()
            .expect("run the plain fixture");
        assert!(
            run_annotated.status.success() && run_plain.status.success(),
            "both fixtures must run successfully\nannotated stderr:\n{}\nplain stderr:\n{}",
            String::from_utf8_lossy(&run_annotated.stderr),
            String::from_utf8_lossy(&run_plain.stderr)
        );
        assert_eq!(
            run_annotated.stdout, run_plain.stdout,
            "both fixtures must run with identical stdout"
        );
    } else {
        eprintln!(
            "skipping the run-output half of units_annotation_erases_to_byte_identical_c: \
             C backend not ready"
        );
    }

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn units_check_receipt_clean_and_mismatch() {
    // Clean fixture: receipt schema string unchanged, diagnostics empty.
    let clean = buildc()
        .arg("check")
        .arg(units_fixture("units_velocity.bld"))
        .args(["--receipt", "-"])
        .output()
        .expect("check the unit-annotated fixture with a receipt");
    assert!(
        clean.status.success(),
        "the unit-annotated fixture must check clean\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&clean.stdout),
        String::from_utf8_lossy(&clean.stderr)
    );
    let clean_receipt = receipt_from_stdout(&clean);
    assert_eq!(clean_receipt["schema"], "buildlang-check-receipt/v1");
    assert_eq!(clean_receipt["status"], "passed");
    assert_eq!(
        clean_receipt["diagnostics"].as_array().map(Vec::len),
        Some(0),
        "a program using the new syntax that checks OK must have no \
         diagnostics: {clean_receipt:#?}"
    );

    // Mismatched fixture: one diagnostic, stage "type", kind
    // "UnitOperationMismatch" -- the new checker rule surfaces through the
    // EXISTING open-string `diagnostics` field, no schema change.
    let mismatch = buildc()
        .arg("check")
        .arg(units_fixture("units_mismatch.bld"))
        .args(["--receipt", "-"])
        .output()
        .expect("check the mismatched-add fixture with a receipt");
    assert!(
        !mismatch.status.success(),
        "a mismatched-dimension add must fail the check"
    );
    let mismatch_receipt = receipt_from_stdout(&mismatch);
    assert_eq!(mismatch_receipt["schema"], "buildlang-check-receipt/v1");
    assert_eq!(mismatch_receipt["status"], "failed");
    let diagnostics = mismatch_receipt["diagnostics"]
        .as_array()
        .expect("diagnostics array");
    assert_eq!(
        diagnostics.len(),
        1,
        "expected exactly one diagnostic: {mismatch_receipt:#?}"
    );
    assert_eq!(diagnostics[0]["stage"], "type");
    assert_eq!(diagnostics[0]["kind"], "UnitOperationMismatch");
    assert_eq!(
        diagnostics[0]["message"],
        "unit mismatch: cannot add `m` and `s` (dimensions differ)"
    );
}

// =============================================================================
// MODEL BOUNDARY RECEIPT (buildlang-model-boundary-receipt/v0)
//
// Design: docs/superpowers/specs/2026-07-29-model-boundary-receipts-design.md
// Emission belongs to the harness-side shim (local-model), never to buildc;
// these tests exercise the READ side only -- `receipt verify`'s model arm and
// the chain-build allowlist widening -- against hand-built and golden-fixture
// JSON. No C backend is required for any test in this section.
// =============================================================================

fn model_receipt_golden_fixture_path() -> PathBuf {
    repo_root()
        .join("compiler")
        .join("tests")
        .join("fixtures")
        .join("model-receipt-golden.json")
}

#[test]
fn model_receipt_golden_fixture_verifies_byte_identically() {
    // The cross-repo pin (design section 2, "Seal and the cross-language
    // canonicalization contract"): this exact file, with this exact seal, is
    // ALSO checked into the local-model / _wshim repo, sealed there by the
    // Python shim. buildc's verify arm accepting it here is the buildlang
    // half of the byte-identity claim.
    let verify = buildc()
        .args(["receipt", "verify"])
        .arg(model_receipt_golden_fixture_path())
        .output()
        .expect("verify golden model receipt");
    assert!(
        verify.status.success(),
        "the golden model receipt must verify\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verify.stdout),
        String::from_utf8_lossy(&verify.stderr)
    );
    let stdout = String::from_utf8_lossy(&verify.stdout);
    assert!(
        stdout.contains("VERIFIED"),
        "human output should report VERIFIED\nstdout:\n{stdout}"
    );

    let verify_json = buildc()
        .args(["receipt", "verify"])
        .arg(model_receipt_golden_fixture_path())
        .arg("--json")
        .output()
        .expect("verify golden model receipt --json");
    assert!(
        verify_json.status.success(),
        "--json verify must also succeed"
    );
    let report = receipt_from_stdout(&verify_json);
    assert_eq!(report["schema"], "buildlang-model-boundary-receipt/v0");
    assert_eq!(report["status"], "verified");
    assert_eq!(
        report["seal"]["hex"],
        "6bb2a09c47f5eaa2e3208a5eadcd6d57d1faffa74a567e024e920571c3794035"
    );
}

#[test]
fn model_receipt_seal_mismatch_is_rejected() {
    // A resealed-looking edit that was NOT actually resealed: the integrity
    // gate must catch it before any field-contract check runs.
    let mut receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(model_receipt_golden_fixture_path()).unwrap()).unwrap();
    receipt["session"]["nonce"] = serde_json::Value::String("ffffffff".to_string());
    let dir = std::env::temp_dir().join(format!("buildlang_model_seal_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create fixture dir");
    let path = dir.join("tampered.json");
    fs::write(&path, serde_json::to_vec_pretty(&receipt).unwrap()).unwrap();

    let verify = buildc()
        .args(["receipt", "verify"])
        .arg(&path)
        .output()
        .expect("verify tampered model receipt");
    assert!(
        !verify.status.success(),
        "an unsealed edit must fail verify"
    );
    assert!(
        String::from_utf8_lossy(&verify.stderr).contains("failure_class: SEAL_MISMATCH"),
        "an unsealed edit must report SEAL_MISMATCH\nstderr:\n{}",
        String::from_utf8_lossy(&verify.stderr)
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn model_receipt_resealed_field_shape_violation_is_rejected() {
    // A COMPLETED outcome with a null reply, re-sealed so the tamper reaches
    // the field-contract gate instead of tripping SEAL_MISMATCH first.
    //
    // The reseal is computed over a HAND-BUILT canonical string, not by
    // round-tripping through `serde_json::Value` (this crate's `Value` is a
    // `BTreeMap`, so re-serializing a `Value` sorts keys ALPHABETICALLY --
    // it would NOT reproduce the struct-field-order canonicalization the
    // real seal uses). The string below mirrors the golden fixture's field
    // order with `reply` set to `null`, exactly as `model_receipt.rs`'s
    // typed struct would serialize it.
    let mut receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(model_receipt_golden_fixture_path()).unwrap()).unwrap();
    receipt["reply"] = serde_json::Value::Null;
    let canonical = concat!(
        r#"{"schema":"buildlang-model-boundary-receipt/v0","source":"model:echo:echo/v1","#,
        r#""shim":{"name":"model_shim.py","version":"0.1.0","mode":"echo"},"#,
        r#""session":{"listen":"127.0.0.1:8931","nonce":"a1b2c3d4","#,
        r#""request_received_utc":"2026-07-29T00:00:00Z","reply_written_utc":"2026-07-29T00:00:00Z"},"#,
        r#""prompt":{"sha256":"758d61f26a44448384e5c4468a0dcb7a2abe456067b0f7b505bc28b9411fe931","bytes":4},"#,
        r#""reply":null,"model":{"name":"echo/v1"},"seed":{"status":"NOT_SENT"},"#,
        r#""outcome":"COMPLETED","seal":{"algorithm":"sha256","hex":""}}"#,
    );
    receipt["seal"]["hex"] = serde_json::Value::String(sha256_hex(canonical.as_bytes()));

    let dir = std::env::temp_dir().join(format!("buildlang_model_fcv_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create fixture dir");
    let path = dir.join("resealed_bad_shape.json");
    fs::write(&path, serde_json::to_vec_pretty(&receipt).unwrap()).unwrap();

    let verify = buildc()
        .args(["receipt", "verify"])
        .arg(&path)
        .output()
        .expect("verify resealed field-shape violation");
    assert!(
        !verify.status.success(),
        "a re-sealed COMPLETED-with-null-reply receipt must still fail verify"
    );
    assert!(
        String::from_utf8_lossy(&verify.stderr).contains("failure_class: FIELD_CONTRACT_VIOLATION"),
        "must report FIELD_CONTRACT_VIOLATION\nstderr:\n{}",
        String::from_utf8_lossy(&verify.stderr)
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn model_receipt_chain_build_refuses_before_the_allowlist_widening_check() {
    // Sanity: an unrelated schema (neither scientific-runtime nor
    // model-boundary-receipt) is still refused by chain build. This pins the
    // allowlist is exactly two members, not "anything with a seal".
    let dir = std::env::temp_dir().join(format!(
        "buildlang_model_chain_refuse_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create fixture dir");
    let bogus = serde_json::json!({
        "schema": "not-a-real-schema/v0",
        "source": "nowhere",
        "seal": { "algorithm": "sha256", "hex": "0".repeat(64) },
    });
    let a = dir.join("bogus.json");
    fs::write(&a, serde_json::to_vec_pretty(&bogus).unwrap()).unwrap();
    let b = model_receipt_golden_fixture_path();
    let out = dir.join("chain.json");
    let build = buildc()
        .args(["receipt", "chain", "build"])
        .arg(&a)
        .arg(&b)
        .arg("-o")
        .arg(&out)
        .output()
        .expect("attempt chain build with a bogus schema member");
    assert!(
        !build.status.success(),
        "chain build must refuse a member whose schema is neither allowlisted schema"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn model_receipt_chains_beside_a_scientific_receipt_and_tamper_breaks_it() {
    // The propose/dispose demo (design section 6): a model receipt (the
    // proposal crossing) chained beside a scientific-runtime receipt (a
    // Model-FREE disposer kernel), bound in order. Tampering the model member
    // must break the chain even though the scientific member is untouched --
    // this is the chain-build allowlist widening plus the verify arm
    // composing with ZERO changes to the schema-agnostic chain machinery.
    if !c_backend_ready() {
        eprintln!(
            "skipping model_receipt_chains_beside_a_scientific_receipt_and_tamper_breaks_it: C backend not ready"
        );
        return;
    }
    let dir = std::env::temp_dir().join(format!("buildlang_model_chain_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create fixture dir");

    // Member 0: the model receipt (the proposer). Copy the golden fixture
    // byte-for-byte so its seal stays intact.
    let model_member = dir.join("model.json");
    fs::copy(model_receipt_golden_fixture_path(), &model_member)
        .expect("copy golden model receipt as chain member 0");
    let model_member_original = fs::read(&model_member).expect("snapshot model member");

    // Member 1: a Model-free scientific-runtime receipt (the disposer). Any
    // corpus kernel works; CAPABILITY_INADMISSIBLE never fires because this
    // kernel does not observe Model -- the rule is demonstrated by what each
    // chain member IS, not bent.
    let sci_member = dir.join("sci.json");
    let emit = buildc()
        .arg("run")
        .arg(repo_example("funnel_probe.bld"))
        .args(["--emit-receipt"])
        .arg(&sci_member)
        .args([
            "--invariant",
            "non-negative",
            "--metric",
            "slack",
            "--problem",
            "disposer",
        ])
        .output()
        .expect("emit disposer receipt");
    assert!(
        emit.status.success(),
        "emitting the disposer receipt should succeed"
    );

    let chain = dir.join("chain.json");
    let build = buildc()
        .args(["receipt", "chain", "build"])
        .arg(&model_member)
        .arg(&sci_member)
        .arg("-o")
        .arg(&chain)
        .output()
        .expect("build propose/dispose chain");
    assert!(
        build.status.success(),
        "chaining a model receipt beside a scientific receipt must succeed\nstderr:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );

    let verify = buildc()
        .args(["receipt", "chain", "verify"])
        .arg(&chain)
        .output()
        .expect("verify propose/dispose chain");
    assert!(
        verify.status.success(),
        "the clean propose/dispose chain must verify\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verify.stdout),
        String::from_utf8_lossy(&verify.stderr)
    );

    // Tamper the model member's body WITHOUT touching its seal.hex text
    // field: the chain's pinned-seal check (step 2) still matches, so the
    // break is caught only when `receipt verify` re-derives the seal over
    // the now-inconsistent body (step 3), reported as CHAIN_LINK_UNVERIFIED.
    let mut tampered: serde_json::Value = serde_json::from_slice(&model_member_original).unwrap();
    tampered["prompt"]["bytes"] = serde_json::Value::from(999);
    fs::write(&model_member, serde_json::to_vec_pretty(&tampered).unwrap()).unwrap();

    let verify_tampered = buildc()
        .args(["receipt", "chain", "verify"])
        .arg(&chain)
        .output()
        .expect("verify chain with a tampered model member");
    assert!(
        !verify_tampered.status.success(),
        "a tampered model member must break the chain"
    );
    assert!(
        String::from_utf8_lossy(&verify_tampered.stderr).contains("CHAIN_LINK_UNVERIFIED"),
        "a model member whose body no longer re-seals must report CHAIN_LINK_UNVERIFIED\nstderr:\n{}",
        String::from_utf8_lossy(&verify_tampered.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Spawn `buildc check <file>` and wait up to `timeout_secs`, killing the
/// process and failing the test if it does not exit. A module-resolution
/// regression would otherwise hang the whole suite (this repo has a documented
/// CI-hang failure class), so the wait is always bounded. Assumes the command's
/// output is small enough not to fill the OS pipe buffer while we poll, which
/// holds for `check` on the tiny fixtures below.
fn check_within(file: &Path, timeout_secs: u64) -> (std::process::ExitStatus, String, String) {
    use std::io::Read;
    let mut child = buildc()
        .arg("check")
        .arg(file)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn buildc check");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll buildc check") {
            break status;
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "buildc check '{}' did not terminate within {timeout_secs}s -- \
                 module resolution likely regressed into unbounded recursion",
                file.display()
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    let mut stdout = String::new();
    let mut stderr = String::new();
    if let Some(mut out) = child.stdout.take() {
        let _ = out.read_to_string(&mut stdout);
    }
    if let Some(mut err) = child.stderr.take() {
        let _ = err.read_to_string(&mut stderr);
    }
    (status, stdout, stderr)
}

#[test]
fn check_file_module_named_after_its_file_terminates() {
    // A file `benchmarks.bld` that declares `module benchmarks` must NOT be
    // read back as a `mod benchmarks;` submodule import of itself. This is the
    // regression for the corpus hang: the file-level `module` header used to be
    // indistinguishable from a bodyless `mod`, so the resolver loaded the
    // declaring file again and recursed forever.
    let dir = std::env::temp_dir().join(format!("buildlang_filemod_{}", std::process::id()));
    let _ = fs::create_dir_all(&dir);
    let file = dir.join("benchmarks.bld");
    fs::write(
        &file,
        "module benchmarks\n\npub fn main() -> i32 {\n    return 0\n}\n",
    )
    .expect("write file-module fixture");

    let (status, stdout, stderr) = check_within(&file, 30);
    assert!(
        status.success(),
        "a file declaring `module <own filename>` must type-check, not hang or error\n\
         stdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn check_mod_self_import_cycle_fails_closed() {
    // A `mod selfref;` inside `selfref.bld` asks the resolver to load the very
    // file that declares it -- an import cycle. The resolver must emit a
    // diagnostic and exit non-zero, never recurse forever.
    let dir = std::env::temp_dir().join(format!("buildlang_modcycle_{}", std::process::id()));
    let _ = fs::create_dir_all(&dir);
    let file = dir.join("selfref.bld");
    fs::write(
        &file,
        "mod selfref;\n\npub fn main() -> i32 {\n    return 0\n}\n",
    )
    .expect("write mod-cycle fixture");

    let (status, _stdout, stderr) = check_within(&file, 30);
    assert!(
        !status.success(),
        "a self-importing `mod` must fail closed, not succeed"
    );
    assert!(
        stderr.contains("cycle detected"),
        "a module import cycle must report a diagnostic\nstderr:\n{stderr}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn check_parse_error_reports_line_and_column() {
    // `check` used to print parse errors with no location, while `build`/`run`
    // printed `error[path:line:col]` with a caret underline. This locks in that
    // `check` now resolves the same location -- in both the human output and
    // the receipt -- so a parse failure is as locatable from `check` as from a
    // build, and the corpus can be triaged by exact site.
    let dir = std::env::temp_dir().join(format!("buildlang_parseloc_{}", std::process::id()));
    let _ = fs::create_dir_all(&dir);
    let file = dir.join("parseloc.bld");
    // The missing `;` after `let x = 5` is reported at the next token,
    // `println!` on line 3, which starts at column 5 (a four-space indent).
    fs::write(
        &file,
        "fn main() ~ Console {\n    let x = 5\n    println!(\"{}\", x);\n}\n",
    )
    .expect("write parse-location fixture");

    // Human output: `error[<path>:3:5]: expected `;`` plus a caret line.
    let (status, _stdout, stderr) = check_within(&file, 30);
    assert!(
        !status.success(),
        "a missing semicolon must fail closed\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains(":3:5]:"),
        "check must report the parse error at line 3, column 5\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("expected `;`"),
        "the diagnostic must name the missing semicolon\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains('^'),
        "check must underline the offending token with a caret\nstderr:\n{stderr}"
    );

    // Receipt: the parse diagnostic carries structured line/col, so machine
    // consumers get the location too (type diagnostics still omit it).
    let output = buildc()
        .arg("check")
        .arg(&file)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run buildc check --receipt -");
    let receipt: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be JSON receipt");
    let parse_diag = receipt["diagnostics"]
        .as_array()
        .expect("diagnostics array")
        .iter()
        .find(|d| d["stage"] == "parse")
        .expect("a parse diagnostic in the receipt");
    assert_eq!(parse_diag["line"], 3, "receipt parse diagnostic line");
    assert_eq!(parse_diag["col"], 5, "receipt parse diagnostic column");
    assert_eq!(parse_diag["kind"], "ParseError");
    assert!(
        parse_diag["message"]
            .as_str()
            .unwrap_or_default()
            .contains("expected `;`"),
        "receipt message must name the missing semicolon: {parse_diag}"
    );

    let _ = fs::remove_dir_all(&dir);
}
