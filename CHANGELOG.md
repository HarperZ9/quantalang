# Changelog

All notable changes to BuildLang will be documented in this file.

Current status note (2026-06-15): entries below preserve historical release
claims as they were recorded at the time. Current release-shaped evidence is
tracked in `STATUS.md`, `README.md`, and
`docs/COMPILER_WIND_DOWN_ASSESSMENT_2026-06-15.md`; historical counts such as
`108/108` or `132/132` are not the current release gate.

## Unreleased

- **Float `{}` Display matches Rust's shortest round-trip**: the
  `println!`/`format!` lowering baked C's `%g` specifier into the format string
  for a default `{}` float placeholder. `%g` caps output at six significant
  digits and switches to exponent form, so `0.1 + 0.2` printed `0.3`,
  `1234567.0` printed `1.23457e+06`, and `3.14159265358979` lost every digit
  past the sixth. Each was a silent wrong answer. A plain `{}` on an `f32` or
  `f64` now routes through a shortest-decimal formatter (`bl_fmt_f32` /
  `bl_fmt_f64`) that searches for the fewest digits whose value round-trips
  through `strtof`/`strtod` and emits plain positional decimal, so Display
  reproduces Rust byte for byte: whole-number floats print without a trailing
  `.0`, and `inf` / `-inf` / `NaN` / signed zero carry through. An explicit spec
  such as `{:.3}` keeps its `%.3f` path. The f32 case uses its own formatter so
  the shortest f32 round-trip is printed rather than the wider f64 value. One
  end-to-end control in `compiler/tests/cli.rs` pins the f64 and f32 cases and
  prints the `%g` values on the pre-fix binary.
- **Float-to-int casts saturate like Rust's `as`**: the C backend lowered a
  float-to-int cast to a raw C cast, which is undefined behaviour when the value
  is out of the target integer's range or is NaN. On x86 an out-of-range
  magnitude wrapped to the target `INT_MIN` and NaN produced a garbage integer,
  both silent wrong answers. A `FloatToInt` cast now calls a per-width
  saturating helper (`bl_f2i_i8` through `bl_f2i_i64`, their unsigned forms, and
  `bl_f2i_i128`) that returns 0 for NaN, clamps to the target MIN/MAX outside
  the representable range, and truncates toward zero inside it, matching Rust's
  saturating `as`. The range thresholds use exact power-of-two hex-float
  constants so the boundary comparison itself never rounds. One end-to-end
  control in `compiler/tests/cli.rs` pins the positive-overflow,
  negative-overflow, NaN, unsigned, and 64-bit paths and prints the wrapped UB
  values on the pre-fix binary.
- **Shifts mask the count to the operand width like Rust's release shift**: the
  C backend emitted a bare `a << b` / `a >> b`. C promotes a sub-`int` operand to
  a 32-bit `int` before shifting, so a narrow type shifted by a count at or past
  its own width used the full 32-bit count instead of Rust's wrapped count
  (`b % bits`). `255u8 >> 9` printed 0 instead of 127, `1u8 << 8` printed 0
  instead of 1, `-128i8 >> 9` printed -1 instead of -64, and `1u16 << 16` printed
  0 instead of 1. Each was a silent wrong answer. The backend now masks the shift
  count to the left operand's bit width for every integer width, emitting
  `(a << ((b) & (bits - 1)))`. At i32 and i64 this equals the count masking x86
  already performs, so those widths stay correct and the emitted C no longer
  relies on a target-specific shift rule. One end-to-end control in
  `compiler/tests/cli.rs` reads the shift amounts from mutable locals so the value
  flows through codegen, pins the six narrow-width cases plus two already-correct
  wide controls, and prints the un-masked `0 0 -1 0 0 0` values on the pre-fix
  binary.
- **`match` pattern tests and result type**: five codegen defects in match
  lowering are fixed, four of which produced a silent wrong answer. An
  open-ended range pattern (`100..`) compared the scrutinee against an
  `i32::MAX` sentinel for the absent bound, so a value past that sentinel
  failed to match; the lowering now emits only the comparisons for bounds that
  are present, and a fully open `..` is always true. An `@`-binding tested
  nothing and matched unconditionally, so `n @ 1..=5` accepted an out-of-range
  value and returned it; the binder now tests the subpattern. An enum
  or-pattern arm (`Green | Blue`) was unhandled in the enum matcher and fell
  through a match-anything catch-all, so the first arm always won; the matcher
  now folds the alternatives with a bitwise-or of their tests, tests an
  `@`-binding's variant subpattern, and looks through parentheses. The
  remaining catch-all in the enum matcher returns an `unsupported pattern`
  error rather than silently matching, so an unhandled pattern kind fails
  closed. Last, a match on a non-integer scrutinee took the scrutinee type as
  the result type, so `match b { true => 42, false => 7 }` stored 42 into a
  `_Bool` and printed `true`; the result now takes the arm type for a bool,
  float, or aggregate scrutinee and keeps the scrutinee width for an integer
  one. One end-to-end control in `compiler/tests/cli.rs` pins all five paths and
  fails on the pre-fix binary.
- **Unary complement and float remainder in the C backend**: two codegen
  defects that produced silent wrong answers or a failed compile are fixed.
  Unary complement lowered `~` and integer `!` onto a single logical-not node,
  so the C backend emitted `!` for both; BuildLang follows Rust, where `!` is
  logical complement on `bool` and bitwise complement on integers and `~` is
  always bitwise complement. MIR gains a `BitNot` unary op, and lowering now
  chooses the operator from the operand type, so `~5` and `!5` produce `-6` and
  a `u8` complement keeps its width (`~5u8` is `250`), while `!bool` stays
  logical. The remainder operator on floats emitted C's `%`, which rejects
  floating-point operands, so `5.5 % 2.0` failed to compile; float remainder
  now lowers to `fmod`/`fmodf` and computes `1.5`. Every non-C backend's unary
  match gains the `BitNot` arm so the enum stays exhaustive. Two end-to-end
  controls in `compiler/tests/cli.rs` each fail on the pre-fix binary (wrong
  value, or gcc rejecting the emitted `%`).
- **Tuple patterns in `match`: correct selection, scalar results, and
  bindings**: `match` over a tuple scrutinee had three codegen defects, now
  fixed. A literal tuple pattern fell through `lower_pattern_test`'s
  always-true catch-all, so `(1, 2)` matched any pair and the first arm won
  regardless of the value; it now tests every element and recurses into nested
  tuples. Scalar arms over a tuple scrutinee took the tuple as the result type
  and emitted C that assigned an int to a tuple struct; the result type is now
  the scalar. Tuple elements bound no variables, since the binding section had
  no tuple arm, so `(a, b) => a + b` emitted undeclared C names; a recursive
  binder now binds elements in both the body and guard blocks, covering nested
  `((a, b), c)` and mixed literal/binding `(1, x)`. Pattern kinds the backend
  cannot yet lower (slice, struct with a refutable field, refutable
  tuple-struct/ref/box) now fail closed with an `unsupported feature`
  diagnostic instead of silently always matching. Six end-to-end controls in
  `compiler/tests/cli.rs` each fail on the pre-fix binary (wrong arm, invalid
  C, or a false always-match); cli suite 359 pass, lib suite 1037 pass, no
  regression.
- **Tool-call receipt verify arm**: `buildc receipt verify` gains a third
  schema dispatch for `flywheel.tool-call-receipt/v1`, the sealed per-tool-call
  receipt emitted by Flywheel's agent loop. Mirrors `model_receipt.rs` in
  structure (fixed-order seal, shared failure taxonomy, golden fixture pinned
  across Python and Rust). Chain allowlist widened to admit tool-call receipts
  alongside scientific and model-boundary receipts.
- **Model boundary receipts, the verify arm and chain admission**: a new
  artifact kind, `buildlang-model-boundary-receipt/v0`, documented in the new
  `docs/MODEL-RECEIPT.md` (SCIENTIFIC-RECEIPT.md gains a pointer section).
  Emission is harness-side (`harness/model_shim.py`'s `--receipt-dir` flag,
  local-model repo), never buildc; this slice ships the buildc-side READ path
  only. `receipt verify` gains a fourth schema arm (beside gpu,
  scientific-runtime, and check) in the new `compiler/src/model_receipt.rs`:
  offline seal recompute, digest well-formedness (`DIGEST_MALFORMED`), and
  field-shape contracts (`FIELD_CONTRACT_VIOLATION` for a `daemon_digest.hex`
  present alongside `UNAVAILABLE`, a `COMPLETED` outcome with a null `reply`,
  or a `PROTOCOL_VIOLATION` with a present `prompt`) -- no new failure
  classes, the shared taxonomy is reused whole. `receipt chain build`'s
  member-schema gate widens from a single-schema equality to a two-schema
  allowlist (scientific-runtime + model-boundary-receipt); chain verify needed
  zero changes, since pinned seals and subprocess re-verification already
  dispatch through the new arm. The scientific verifier's
  `CAPABILITY_INADMISSIBLE` refusal of any `Model`-observing program is
  untouched: a model receipt is a different artifact kind by construction, it
  cannot masquerade as scientific evidence. A byte-identical GOLDEN FIXTURE
  (`compiler/tests/fixtures/model-receipt-golden.json`, an echo-mode
  `COMPLETED` receipt) is checked into both this repo and local-model's
  `_wshim` worktree with the same pinned seal, proving the Rust
  (`serde_json::to_vec`) and Python (`json.dumps(..., separators=(",", ":"))`)
  canonicalizations agree byte-for-byte; the no-floats schema is what makes
  that agreement stable across the two serializers. Tamper coverage: a
  resealed field-shape violation, a seal mismatch, and a chain binding a
  model receipt beside a scientific receipt that breaks
  (`CHAIN_LINK_UNVERIFIED`) when only the model member is tampered, all
  exercised both as `compiler/src/model_receipt.rs` unit tests and as
  `compiler/tests/cli.rs` CLI-level tests against the real `buildc` binary.
  The model receipt is **not** a corpus member (it has no invariant to
  classify PASS/FAIL_EXPECTED against, and is emitted by a different program
  entirely) and not a `--self-test` case (that table is
  scientific-runtime-only): corpus 29/29 and self-test 10/10 stay unchanged,
  re-run and recorded. Full suite: 1,698 passed, 0 failed; `cargo fmt` clean.
- **Executed Monte Carlo intervals with a witnessed denominator**: `monte_carlo`
  gains a two-arm `DECLARED | EXECUTED` status. Under the new `--mc-executed`
  flag (opt-in; requires the full `--mc-*` declaration and forces `--columns`
  to 3), the kernel prints a three-column row per post-burn-in step
  (`<invariant_scalar> <successes> <trials>`), and `receipt verify`
  RE-DERIVES the Wilson or normal-approx-95 interval from those raw
  sufficient-statistic columns, entirely in verifier-owned code, at two
  stages: Stage A over the sealed series before any re-run (a
  tampered-and-resealed interval is a pure data contradiction, rejectable
  with no C compiler), and Stage B over the re-run series (a new failure
  class, `MC_INTERVAL_DRIFT`, for a receipt that stays internally coherent
  while no longer describing the run it names). The declared sample count
  becomes a WITNESSED denominator: the final row's `trials` must equal
  `monte_carlo.samples`. Coherence is checked as a cumulative Bernoulli
  count (integers below 2^53, `trials` incrementing by exactly 1, `successes`
  non-decreasing in `{0, 1}`, `successes <= trials`). An EXECUTED block adds
  three `not_claimed` entries -- `sample_independence`, `interval_coverage`,
  `estimator_semantics` -- present if and only if the block is EXECUTED:
  EXECUTED hardens the interval arithmetic and the denominator, never the
  estimator's semantics or independence. Backward compatible: `DECLARED`
  receipts stay valid forever, the five new fields are `Option` with
  `skip_serializing_if`, and a receipt sealed before this slice re-serializes
  to its exact bytes (pinned by test). New kernel pair
  `examples/mc_pi_rejection_executed.bld` /
  `examples/mc_pi_rejection_executed_broken.bld`; corpus 29/29; self-test
  10/10; full suite 1,644 passed, 0 failed.
- **Split-frontier drop flags (memory pillar increment 5, opt-in)**: behind the
  same `BUILDLANG_EXPERIMENTAL_FREE` flag (default off, flag-off output
  byte-identical, verified mechanically), the C backend now reclaims heap
  `BuildString` buffers whose death frontier is split across conditional
  edges and buffers whose allocation is itself conditional, the two shapes
  increments 1-4 decline. Mechanism: a per-buffer runtime `uint8_t` drop
  flag, set immediately after the owner's unique allocation or move-acquire,
  tested and cleared at every free (`analysis::flags::split_frontier_flag_frees`,
  wired into `backend::c`). Additive and disjoint from increments 1-4: every
  buffer is freed by exactly one mechanism. Verified ASan-clean on two
  1,000,000-total-iteration real-program fixtures and a six-lens adversarial
  pass in an isolated worktree; `buildc corpus verify` 8/8 flag on and off;
  full suite 1,613 passed. Honest scope kept: re-entrant frontier sites
  (loop headers, self-loops) still decline (only the Return backstop
  reclaims those, leaking safely per-iteration); allocations outside the
  closed 6-name runtime list, escaping/reassigned/multi-move-tainted owners,
  and `BuildVec`/`BuildMap` buffers are unchanged declines. The memory
  pillar is NOT done and the flag is NOT default-on.
- **Unit-annotated numeric types, checker slice one (EXPERIMENTAL, opt-in)**:
  `f64<m/s>`-style dimension annotations (sibling to the shipped
  `dimensional analysis, first slice` unit-core entry below) parse through
  the shared `units::parse_unit` grammar and are now enforced by the
  Hindley-Milner checker, not just the receipt label. `+`/`-`/`%`/comparisons
  require equal dimensions (`+`/`-`/compare get an operation-worded message);
  `*`/`/` derive dimensions via `Dimension::multiply`/`divide`; `**` on a
  unit-carrying operand is a loud `UnsupportedConstruct` refusal, never a
  silently wrong dimension; `.sqrt()`/`.cbrt()`/`.powi()`/`.powf()` on a
  unit-carrying receiver get the identical refusal (a review pass found they
  had silently kept returning the receiver's unchanged, wrong dimension;
  `.abs()`/`.floor()`/`.ceil()`/`.round()`/`.trunc()`/`.fract()` stay
  identity-shaped and correct, unchanged; the REMAINING float methods in
  that dispatch arm, `.recip()`, `.signum()`, trig/log/exp, `.hypot()`,
  `.clamp()` and siblings, are UNAUDITED for dimensional correctness in
  this slice and may propagate a dimension a method does not actually
  preserve, so annotate receivers of those methods with care until the
  audit lands as a follow-up); unification is the backstop at
  every let, assign, argument, and return boundary. Weak mode: an
  unannotated float
  stays UNCONSTRAINED (compatible with any unit) rather than a full
  dimension variable, so a bug through an unannotated intermediate binding
  is caught only if a later boundary is annotated -- full dimension
  variables are a specced follow-up. Zero codegen impact, mechanically
  verified: `MirType` has no unit slot, the `WithUnit` AST node erases to
  its base type before MIR, and a fixture pair
  (`compiler/tests/units/units_velocity.bld` / `_plain.bld`, identical minus
  annotations) proves byte-identical emitted C and stdout; a mutation check
  confirmed removing the erasure arm goes red. Backward compatible: a
  repo-wide grep found zero existing sources using `f64<`/`f32<` syntax, so
  the only semantic change is that `f64<zebra>` (previously silently
  accepted, unit ignored) now correctly fails to parse. No receipt schema
  change: the new checker errors are ordinary `diagnostics` entries; the
  scientific-runtime receipt's `measurement.units` keeps its existing
  `--units` source (Pass D receipt flow-through stays specced). Detail:
  `docs/DIMENSIONAL-ANALYSIS.md`.
- **Wall-clock metering, the first EXECUTED budget fact**: `runtime_state` gains
  `wall_seconds`, the receipt's first EXECUTED time fact, measured with
  `std::time::Instant` around the primary run's `.output()` call, rounded to
  3 decimal places, and sealed at emit. `receipt verify` re-measures its own
  re-run and REPORTS the fresh number beside the sealed one (the human MATCH
  line appends `wall_seconds=<sealed>~<remeasured>`, and `--json` carries a
  `wall_seconds` object), never requiring agreement: timing is environmental,
  exactly like raw stdout bytes. The `budget` block gains an OPTIONAL declared
  ceiling, `--budget-wall-seconds <LIMIT>`, valid only alongside the existing
  `--budget-steps`/`--budget-consumed` pair and refused when non-positive or
  non-finite; when present, `wall_exceeded` is DERIVED at emit from the two
  SEALED numbers (`wall_seconds > wall_seconds_limit`) and re-derived at
  verify from the same sealed pair only, never from verify's own re-measured
  time, so a slower verify machine cannot flip a receipt's coherence.
  Backward compatible: both new fields are `Option` with
  `skip_serializing_if`, so a receipt sealed before this change parses and
  re-seals byte-identically. No new `--self-test` case: a tampered wall field
  is rejected through the same `FIELD_CONTRACT_VIOLATION` arm the existing
  budget case already exercises.
- **Five modes, one chain**: a cli test (`five_modes_bind_into_one_chain`) emits
  one PASS receipt per computation mode (deterministic, probabilistic-exact,
  stochastic, Monte Carlo, heuristic, plus the cross-backend bonus when `rustc`
  is available) from the shipped example kernels, binds them with the existing
  `receipt chain build`/`verify` machinery, and asserts that tampering one
  member's stored `violation_count` without re-sealing breaks the chain with
  `CHAIN_LINK_UNVERIFIED`. New walkthrough doc: `docs/FIVE-MODES-TOUR.md`.
- **Cross-backend relation receipts**: `buildc run` gains `--cross-backend
  <TARGET>` (v0: `rust` only), running the kernel through the C anchor AND a
  secondary backend and sealing each step's two values as one row of a
  2-column relation checked under a new invariant, `cross-backend`
  (`cross_backend_columns_agree`), which reuses the `relation` family's
  evaluator unchanged. The declaration is a strict biconditional with the
  invariant (both directions refused) and requires `--columns` be unset or
  exactly 2 (an unset default is silently upgraded). Refused with `--gpu`,
  `--seed`, any `--mc-*` flag, and on a `Random`-observing kernel, because the
  Rust lane has no seeded PRNG builtin and the streams could not agree.
  Unlike `monte_carlo`/`budget`, the sealed `cross_backend` block is
  EXECUTED, not DECLARED: it carries the secondary lane's witnessed facts
  (target, toolchain version and digest, executable digest, raw-stdout
  digest, exit code), and `receipt verify` RE-EXECUTES both lanes rather
  than trusting the declaration, rebuilding the interleaved series from the
  two fresh re-runs before recomputing the verdict. rustc absence at verify
  exits 4, matching how the primary C toolchain's absence is classed.
  Reproduction of the secondary's raw stdout and executable digests is
  REPORTED, never required, exactly like the primary's. Verify also re-probes
  the local rustc and compares it against the sealed toolchain digest,
  mirroring the primary C lane's `toolchain_matched`: a drift WARNS (never
  fails) and is carried as `secondary_toolchain_matched`, closing a gap where
  a `RUSTC` override at verify time was previously invisible.

  Tolerance calibration: a measured probe (2026-07-28) found the C and Rust
  backends compute IDENTICAL doubles for the reference decay recurrence, but
  the C runtime prints `%g` (6 significant digits) while Rust prints
  shortest-roundtrip, so two bit-identical doubles can print up to ~5e-7
  apart on O(1) values. `relation`'s `1e-9` tolerance would reject that on
  formatting alone, so `cross-backend` uses a dedicated `1e-5`, clearing the
  display floor by ~20x while still catching a genuine O(1) divergence
  decisively. Ships with one kernel (`examples/decay_cross_backend.bld`,
  `x = x*0.9 + 0.01` for 40 steps) and deliberately NO negative-fixture
  partner: an honest deterministic kernel that computes different values on
  two backends cannot exist by construction, so the can-it-fail evidence
  lives in an evaluator-level divergence unit test, the CLI refusal gates,
  and a ninth verifier self-test tamper case (the invariant/block
  biconditional swapped). Corpus grows to 27 members (thirteen pairs plus
  the cross-backend singleton); self-test grows to nine cases. Backward
  compatible: receipts without the block keep their exact bytes and seals.
- **Model capability with the propose/dispose receipt boundary**: a new
  `model_complete(prompt) -> str` builtin carries a `Model` capability
  effect, transported over a deliberately dumb line protocol on the
  existing TCP runtime (connect to `BUILD_MODEL_ENDPOINT`, send one prompt
  line, read the reply until the shim closes the connection; a conforming
  shim writes one completion line and closes, and one trailing newline is
  trimmed). The model adapter (HTTP,
  tokenization, parameters) lives on the harness side of this seam, never
  in the compiler. FAIL CLOSED: no endpoint, a malformed endpoint, an
  embedded newline in the prompt, or a connection failure aborts rather
  than fabricating a completion. The receipt layer enforces propose/dispose
  at both ends: `buildc run --emit-receipt` refuses a Model-observing
  program up front, and `receipt verify` refuses any receipt whose
  RE-DERIVED capabilities include `Model`, both under a new failure class
  (`CAPABILITY_INADMISSIBLE`) -- models propose, oracles dispose. `Model`
  carries no explicit arm in the witnessed-absence derivation, so it falls
  through to the fail-closed default (a hazard for both `input_dataset` and
  `determinism`), which a unit test pins. v0 ships the capability, the
  transport contract, and the type-level rule only; it does not ship model
  receipts (digest, prompt hash, parameters) or the flywheel demo, which
  are a separate follow-on. No corpus member and no `--self-test` case: the
  refusal happens before a receipt can exist, so there is nothing for
  either to exercise; the corpus stays at 26 members and the self-test
  stays at eight cases.
- **Budgeted-search receipts**: `buildc run` gains `--budget-steps` /
  `--budget-consumed`, sealing the heuristic's admission facts (step
  ceiling, consumption, a DERIVED `exhausted` flag) as a `budget` receipt
  block. The declaration is all-or-nothing (a result without its budget
  ceiling hides whether it stopped at the limit) and, unlike `monte_carlo`,
  is deterministic: no `Random` capability or seed required. Verify
  re-checks the shape contracts (`FIELD_CONTRACT_VIOLATION`): a zero
  ceiling, a consumption above the ceiling, a hand-set `exhausted`, or a
  non-`DECLARED` status are all refused. Two mechanical honesty rules ride
  with it: every receipt's `labels` must contain `NOT_PROVES_OPTIMALITY`
  and `not_claimed` must contain `optimality` if and only if it carries a
  `budget` block, and `--method` / `--problem` text containing `optimal`
  on a budgeted run is refused (a budgeted search reports its incumbent,
  never a proof of optimality). The verifier self-test grows an eighth
  tamper case (`steps_consumed` above `steps_limit`, re-sealed). Ships with
  a heuristic kernel pair (greedy coin change over denominations
  {4, 3, 1}, a genuine non-optimal heuristic: amount 6 takes greedy's 3
  coins where 3+3 is the optimal 2, run under a calibrated step budget of
  23 against a measured worst of 16, and the same loop under a step budget
  of 14 that the worst amounts overrun), taking the corpus to 26 members.
  Backward compatible: receipts without the block keep their exact bytes
  and seals.
- **Monte Carlo estimator receipts**: `buildc run` gains `--mc-estimator` /
  `--mc-samples` / `--mc-interval`, sealing the estimator's admission facts
  (id, sample-count denominator, interval method) as a `monte_carlo` receipt
  block. The declaration is all-or-nothing (an estimator whose interval
  method is undeclared is refused, as is every partial combination or a zero
  denominator) and requires a seeded `Random` run; verify re-checks the
  shape contracts against the re-derived capabilities
  (`FIELD_CONTRACT_VIOLATION`), and the verifier self-test grows a seventh
  tamper case (a zero MC denominator, re-sealed). v0 claims reproducibility
  and declaration discipline, never correctness of the interval. Ships with
  a known-answer kernel pair (pi by rejection sampling inside a
  seed-calibrated truth band, and a wrong-area estimator that blows through
  it), taking the corpus to 24 members. Backward compatible: receipts
  without the block keep their exact bytes and seals.
- **`Random` capability + witnessed-seed receipts**: `random_f64()` is a new
  seeded PRNG builtin carrying its own `Random` capability effect (SplitMix64
  over the top 53 bits, bit-identical across platforms for a given seed).
  `buildc run --seed N` supplies the seed via `BUILD_RANDOM_SEED`; an
  unseeded draw aborts (fail closed, never a silent default stream). With
  `--emit-receipt` the pairing is enforced both ways (a Random-using kernel
  requires a seed, a seed requires a Random-using kernel) and the seed is
  sealed as `seed_value`; `receipt verify` re-runs the exact stream and
  re-checks the pairing against the re-derived capabilities
  (`FIELD_CONTRACT_VIOLATION`), so a seeded stochastic run is as
  re-derivable as a deterministic one. The receipt's `seed` field becomes a
  trichotomy (`NOT_APPLICABLE` / `SEALED` / `UNSEEDED`), determinism gains
  the honest seeded state (deterministic given the sealed seed), the
  verifier self-test grows a sixth tamper case (seed flipped against
  capabilities), and the example corpus gains its first seeded-stochastic
  pair (`random_walk_bound.bld` / `random_walk_bound_broken.bld`, a
  200-step random walk against its worst-case envelope and against a
  falsely tight one), taking the corpus to 22 members. Backward compatible:
  programs and receipts that never touch `random_f64()` are byte-identical.

## 1.2.0 - 2026-07-07 - general GPU compute

BuildLang programs now run real work on the GPU: `#[compute]` kernels compile
to valid SPIR-V, dispatch on an actual Vulkan device, and seal a re-checkable
GPU receipt. Minor release (backward compatible; programs that do not use the
GPU path or the new flags are unaffected).

- **General GPU execution path (Layers A/B/C)**: Layer A emits valid,
  dispatchable compute SPIR-V for `#[compute]` kernels; Layer B/C perform a
  real Vulkan dispatch on the device and seal a GPU receipt. A NUL-byte entry
  name is now a typed error instead of a panic.
- **GPU Phase 1 (arbitrary elementwise f32 kernels)**: dispatch and CPU
  cross-check generalized from a fixed demo to arbitrary elementwise kernels,
  with writability inference, scalar push constants in the SPIR-V interface,
  and a clear diagnostic refusing `f64` on the f32-only GPU path.
- **GPU Phase 2 (2D grids + matmul)**: per-kernel 2D workgroup size, a 2D
  dispatch grid, matmul shape validation in the Vulkan host, an in-kernel
  bounds guard for arbitrary dims, and rejection of non-workgroup-multiple
  matmul dims (out-of-bounds guard). Cross-checked against an identity
  closed-form sanity case.
- **GPU Phase 3 (1D stencil)**: a 3-point clamped-blur stencil kernel with a
  u32-length 1D driver, clamped-edge sanity checks, and Layer A emit + device
  match + boundary + negative tests.
- **GPU Phase 4a (workgroup shared memory + barrier)**: device-free machinery
  for workgroup-shared scratch memory and barriers, a distinct `Workgroup`
  `OpVariable` per scratch local, effect-gating tests, and a dead
  Function-storage variable skipped for workgroup-slot locals.
- **SPIR-V correctness fixes**: nested structured control flow corrected via
  dominator analysis (selection-in-loop nesting covered), void/unit stores
  skipped, constant-operand signedness reconciled in integer binops, and
  integer binops typed from the non-constant operand (const-left case).
- **Dimensional analysis (typed physical units), first slice**: a pure,
  dependency-free core (`compiler/src/units.rs`, public as `buildlang::units`)
  models a physical dimension as integer exponents over the seven SI base
  dimensions, with the checked algebra (multiply/divide/power; add, subtract,
  and compare require equal dimensions), a parser for a compact unit grammar
  (`m/s`, `kg*m/s^2`, `1/s`, `J`), and a canonical formatter.
  `buildc run --emit-receipt --units <UNIT>` canonicalizes the declared unit
  through this core: a malformed or unknown unit is a hard error before any
  compilation, and a valid unit is sealed into the scientific-runtime
  receipt's `measurement.units` in its checked canonical form (so `m*s^-1`
  and `m/s` seal identically and the receipt still re-verifies). Honest
  scope: unit annotation and receipt-label checking only; `f64<m/s>` is not
  yet a first-class type in the checker (integration specced in
  `docs/DIMENSIONAL-ANALYSIS.md`). Backward compatible: `run` without
  `--units` is byte-identical. Coverage: 18 core unit tests plus 2 CLI
  integration tests.
- **Receipt Wave 4**: `receipt verify --self-test` proves the verifier can
  FAIL; `receipt chain build` / `chain verify` seal a tamper-evident receipt
  bundle (the index is bound into the chain seal; all 4 failure classes
  covered); `receipt corpus` gates the example suite on declared
  classifications. Non-byte-reproducible receipt seals documented as
  by-design.
- **Invariant family grown to seven members**: added `non-negative`
  (algorithmic accountability), a reaction invariant checker (chemistry
  demo), a Born-rule normalization kernel (quantum conservation), and a
  funnel-hashing probe-bound kernel (algorithmic). README and STATUS synced
  to the family.
- **Typed `Array<T,N>` as function parameter and return type**: fixed-size
  arrays can now be passed to and returned from functions (returns lower via
  an out-param). Documented in the math-syntax guide.
- **Docs and visual identity**: spectrum banner and feature-first README
  header and body, a current introduction, and a live crates.io version badge
  plus a downloads badge (replacing the hardcoded version badge). Stale
  stencil exclusions dropped from host and packer comments.
- Sealed receipt corpus regenerated for corrected `&mut` MIR mutability.

## 1.1.0 - 2026-07-02 - accountable scientific compute

A second, independent receipt family beyond the capability (check) receipts:
the **scientific-runtime receipt** (`buildlang-scientific-runtime-receipt/v0`).
Minor release (backward-compatible; `run` without `--emit-receipt` is
byte-identical to 1.0.x). Details in `STATUS.md` / `docs/SCIENTIFIC-RECEIPT.md`.

- `buildc run --emit-receipt <path> --invariant <NAME>` runs a numeric `.bld`
  kernel, captures its output series, checks a stated invariant, and seals a
  re-checkable JSON receipt. `buildc receipt verify` RE-RUNS the program and
  re-derives the verdict; drift, tamper, or a source change fail with a typed
  `failure_class` and a verdict-gated exit code (0 faithful pass, 1 did-not-
  reproduce, 3 faithful fail, 4 no toolchain).
- **Invariant family (6 members)**, each a fixed re-checked tolerance with a
  paired positive/negative kernel: `energy-monotone`, `conservation`, `bounded`
  (discrete maximum principle), `energy-identity` (a quantitative per-step
  energy-balance residual), `relation` (`--columns N`, the verifier compares a
  row's columns), and `conserved-band` (approximate conservation within a fixed
  error budget, e.g. a symplectic integrator's energy).
- `buildc receipt export` re-verifies a receipt and emits witnessed
  Crucible-ingestible measurement rows (deviation derived from the fresh re-run,
  the replay command sealed as a recheck descriptor).
- The capability receipt gained the effect-policy chain and capability-derived
  witnessed-absence fields (input_dataset / seed / determinism, fail-closed).
- Honest scope preserved: every receipt carries `NOT_A_NEW_PHYSICAL_LAW`; the
  receipt witnesses the observed output series, not model correctness.
- Baseline at this work: `cargo test` from `compiler/` lib 940, bin 135, cli
  307, lexer 52, parser 88 (0 failing); `buildc corpus verify` 8/8.

## 1.0.5 - 2026-06-30

Documentation + packaging accuracy pass (no code change):

- README License section corrected from "MIT License" to the BuildLang
  Fair-Source License v1.0 (the published crate's crates.io page no longer
  misstates the license).
- Refreshed the test-count baselines across README / STATUS / TEST_RESULTS to
  lib 872 / bin 44 / cli 263 / lexer 51 / parser 83 (2026-06-30).
- Added `cargo install buildlang` to the install sections of README, USAGE, and
  the getting-started guide; refreshed the `compiler_version` example to 1.0.4.

## 1.0.4 - 2026-06-30

First public crates.io release under the **BuildLang** name. Headline changes
this cycle (granular entries follow):

- **Published to crates.io as [`buildlang`](https://crates.io/crates/buildlang)**
  (binary `buildc`): `cargo install buildlang`. Renamed from `quantalang`, which
  is deprecated and points here. The crate is licensed under the **BuildLang
  Fair-Source License v1.0** (source-available, not open source) — corrected from
  an earlier mislabel as MIT, including all source-file headers.
- **Linear types (`#[linear]`, experimental no-cloning)** — see the detailed
  entry below; the shared keystone for quantum / fin-sec / blockchain.
- **Integer literal widening**: an unsuffixed integer literal exceeding i32 range
  now widens to i64 / i128 instead of silently truncating to 32 bits (it used to
  print `9223372036854775000` as `-808`), in both the type checker and the MIR
  lowering.
- **Codegen (`Option<i64>`)**: an `Option<i64>`-returning function whose result is
  matched no longer miscompiles the 64-bit payload as `int32_t`; `lower_if`
  types the if-expression result by the aggregate branch.
- **Foundation direction**: `docs/QUANTUM-HOST.md`, `docs/FINSEC-BLOCKCHAIN-HOST.md`,
  and `docs/LINEAR-TYPES.md` document buildc as a base for quantum, fin-sec, and
  blockchain work, with runnable spikes (`examples/quantum/bell.bld`,
  `examples/finance/ledger.bld`, `checked.bld`, `safe_math.bld` — overflow-safe
  checked + saturating arithmetic).
- **LSP**: the `initialize` dispatch receipt digest is desensitized to the
  compiler version, so version bumps no longer churn the corpus receipt.

## Unreleased

- Type system (linear types / no-cloning): an opt-in `#[linear]` attribute on a
  struct or enum marks its values as a tracked resource that may be moved /
  consumed **at most once**. The type checker now rejects use-after-consume
  (`use of linear value 'q' after it was consumed`), which is the no-cloning rule
  for quantum qubits, the no-double-spend rule for on-chain assets, and
  resource-handle safety for fin-sec settlement obligations - one type-system
  feature for all three foundations. Borrows (`&q`) do not consume; ordinary
  (non-`#[linear]`) types keep copy-like reuse, so the change is backward
  compatible (full suite stays green). Coverage: let-bound locals, function
  parameters, branch joins (`if`/`if let`/`match`, conservative union-of-consumed
  so a value consumed on any path is poisoned afterward), and a loop guard
  (consuming an outer linear value inside a loop is a potential double-use and is
  rejected). A containment rule rejects a non-`#[linear]` aggregate that holds a
  linear field (`non-linear type 'Wallet' cannot contain linear field
  'coin: Coin'`), preventing the resource from being laundered out of an
  untracked wrapper. Built on the existing move/borrow analysis; 10 type-checker
  tests (`linear_*`, `nonlinear_struct_with_linear_field_is_rejected`); verified
  end-to-end via `buildc check`. Deferred to a follow-up: drop-without-consume
  ("must use") enforcement and per-path (non-conservative) branch tracking. The
  analysis is deliberately sound-over-complete - it may reject some safe programs
  rather than ever permit a clone. **Status: experimental, not yet fully sound.**
  Three adversarial verification passes closed 14 compositional escape classes
  (each now a regression test) but a third pass still found a few open classes
  (pattern-match-through-a-borrow, enum-variant shorthand init, generic
  deref/result, match-guard fall-through, borrow-after-move). Full no-cloning
  soundness needs an affine/borrow checker on MIR. What is enforced and what is
  open: `docs/LINEAR-TYPES.md`. See also `docs/QUANTUM-HOST.md` (brick 1).

- Stdlib (`HashMap::keys`): `m.keys()` now returns a `Vec<String>` handle (so
  `for k in m.keys()`, indexing, and `.len()` work), via a runtime
  `build_hmap_keys_str_f64` that walks the occupied buckets and wraps each key.
  Was an undefined symbol. Verified end-to-end under MSVC: iterating the keys of a
  two-entry map runs twice (`n 2`). Covered by `hashmap_keys_returns_a_string_vec`.
  (`.values()` is deferred - it needs the str->f64 map's insert key-coercion fix
  and value-type threading first; both logged as follow-ups.)

- Stdlib (iterator `.take(n)` / `.skip(n)`): both are now recognized steps.
  `.take(n)` yields the first `n` elements then exits the loop; `.skip(n)` drops
  the first `n`. They use per-iteration counters (not source-index checks), so
  they compose correctly with each other and with `.filter()` - e.g.
  `filter(|x| x>1).take(2)` takes the first two that pass, not the first two
  source positions. Verified end-to-end under MSVC over `[10,20,30,40,50]`:
  `take(2).sum()`=30, `skip(3).sum()`=90, `skip(1).take(2).sum()`=50; and
  `filter(|x| x>1).take(2).sum()` over `[1..6]`=5. Covered by
  `iterator_take_and_skip_steps_desugar`. (`.take()`/`.skip()` combined with
  `.rev()` use forward counters - reverse-position semantics is a follow-up.)
- Stdlib (iterator `.rev()`): `v.iter().rev()...` now iterates the source in
  reverse - the loop starts at `len-1`, steps the (signed) index down, and exits
  below 0. Was an unrecognized step, so the chain left `.iter()` undefined.
  Composes with the terminals and other steps. Verified end-to-end under MSVC over
  `[1,2,3,4]`: `rev().sum()` is `10`, and `rev().collect()` yields `[4,3,2,1]`
  (first `4`, last `1`). Covered by `iterator_rev_step_iterates_in_reverse`.
- Parser (`handle` as an identifier): a function named `handle` is now callable.
  `handle` is the effect-handler keyword (`handle { ... } with { ... }`), and the
  parser unconditionally parsed `handle(...)` as a handler expression - swallowing
  the rest of the block. So `let r = handle(x); <more> ` dropped every following
  statement: the binding `r` became "undefined variable" and, inline,
  `println("{}", handle(...))` silently emptied the whole function body (compiled
  and ran, printing nothing). The parser now only takes the handler path when
  `handle` is followed by `{`; otherwise it parses `handle` as an identifier.
  Verified end-to-end under MSVC: `fn handle(n) { ... }` called as `handle(5)`
  works, and `println("{}", handle(Msg::Move(3,4)))` prints `r 7` (previously
  empty). A real `handle { ... } with { ... }` handler still parses. Covered by
  `function_named_handle_is_callable_not_a_handler_expr`.
- Stdlib (iterator `enumerate().map(|(i, x)| ...)`): a map closure with a single
  tuple parameter `|(i, x)|` after `.enumerate()` now binds both the index and the
  element. Previously only `|i, x|` (two separate params) was handled; the tuple
  form bound neither (C2065 'i' undeclared). Verified end-to-end under MSVC:
  `v.iter().enumerate().map(|(i, x)| i + x).sum()` over `[10,20,30]` is
  `63 = (0+10)+(1+20)+(2+30)`. Covered by `enumerate_map_binds_a_tuple_param`.
- Stdlib (nested `Vec<Vec<_>>` / `Vec<HashMap<_,_>>`): a vector whose element is
  itself a collection now works. `grid.push(row)` dispatched to
  `build_hvec_push_i32`, passing a `BuildVecHandle` where an `int32_t` was expected
  (C2440). The Vec element-suffix logic (`hvec_elem_suffix`,
  `vec_elem_needs_sized_wrapper`, and the lowering's method `type_suffix`) now
  treats a `Vec`/`Map` element as an aggregate keyed by its handle type
  (`BuildVecHandle`/`BuildMapHandle`), so the monomorphized element-sized wrappers
  are generated and used. Verified end-to-end under MSVC: `Vec<Vec<i32>>` push +
  nested index reads `2`. Covered by `nested_vec_of_vec_uses_handle_element_wrappers`.
- Codegen (`for c in s.chars()`): iterating a string now works. `chars()`/`bytes()`
  are identity ops returning the BuildString, and `for` over a BuildString fell
  through to the no-op loop (zero iterations, so the body never ran - silently
  wrong). `lower_for` now has a BuildString arm (`lower_for_string`) that loops by
  `build_string_len` and binds each byte (as i32) via a new `build_string_byte_at`
  runtime helper. Verified end-to-end under MSVC: iterating `"hello"` runs 5 times;
  the byte sum of `"AB"` is `131` (65+66). Covered by
  `for_over_string_chars_emits_a_byte_loop`. (Yields raw bytes, not decoded UTF-8
  code points - multi-byte chars are a follow-up.)
- Stdlib (`Vec::sort`): `v.sort()` now sorts the vector in place (ascending) via
  the runtime `build_hvec_sort_{i32,i64,f64}` (qsort with a numeric comparator).
  Was an undefined symbol. Verified end-to-end under MSVC: sorting
  `[3,1,4,1,5,9,2,6]` gives first `1`, last `9`. Covered by
  `vec_sort_dispatches_to_runtime_qsort`. (String/struct element sort and
  `sort_by` remain follow-ups.)
- Codegen (Vec indexed assignment): `v[i] = x` now stores into the vector. It
  was silently dropped - `lower_assign` handled deref/field/identifier targets
  but not an `Index` target, so the write matched no arm and the element kept its
  old value. It now dispatches to a typed runtime setter
  (`build_hvec_set_{i32,i64,f64,str}`); compound forms (`v[i] += x`, `*=`, ...)
  read-modify-write. Verified end-to-end under MSVC: `v[1] = 99` then read is
  `99`; `v[0]=5; v[1]+=100; v[2]*=2` over `[10,20,30]` yields `5 / 120 / 60`.
  Covered by `vec_indexed_assignment_stores_through_a_setter`.
- Stdlib (`Vec::contains`): `v.contains(x)` now works (previously an undefined
  symbol). It dispatches to an element-typed runtime linear scan
  (`build_hvec_contains_{i32,i64,f64,str}`); the string variant compares with
  `build_string_eq`. Verified end-to-end under MSVC: `Vec<i32>` contains check is
  `true`; `Vec<String>` contains `banana` -> `true`, `cherry` -> `false`. Covered
  by `vec_contains_dispatches_to_runtime_scan`.
- Stdlib (`Result<T,E>` methods): `is_ok()`, `is_err()`, `unwrap()`,
  `unwrap_err()`, and `unwrap_or(default)` now work (previously undefined symbols).
  `is_ok`/`is_err` read the `is_ok` discriminant; `unwrap` reads the typed `ok`
  slot, `unwrap_err` the `err` slot; `unwrap_or` branches on `is_ok`, reading the
  ok payload when present and the default otherwise. Ok/Err slot types use the
  threaded `result_ok_types`/`result_err_types` (default i32 / BuildString).
  Parallel to the Option methods. Verified end-to-end under MSVC:
  `safediv(10,2).unwrap_or(-1)`=5, `safediv(10,0).unwrap_or(-1)`=-1,
  `is_ok()`/`is_err()` return the right booleans. Covered by
  `result_methods_is_ok_and_unwrap_or`.
- Codegen (C stdlib name collisions): a user function named like a C standard
  library function (e.g. `div`, `remove`, `system`, `rand`, `qsort`) is now
  emitted with a leading underscore at its definition, forward declaration, AND
  every call site. Previously only C macros (`min`/`max`/`abs`) were escaped, so
  `fn div(...)` collided with libc's `div` - a redefinition (C2371) and a
  `div_t`-vs-return-type mismatch (C2440). A shared `user_fn_emit_name` /
  `is_c_stdlib_collision` pair now drives all three emit sites consistently;
  runtime `build_*` helpers and intentional math builtins (`abs`, `exit`, ...)
  are untouched. Verified end-to-end under MSVC: `fn div` + `fn remove` compile
  and print `d 5` / `r 9`. Covered by
  `user_function_named_like_c_stdlib_is_escaped`.
- Codegen (`if let`): `if let Some(x) = opt { ... } else { ... }` now works. It
  was fundamentally broken for runtime Option/Result: it bound the pattern
  variable to the *whole* Option struct and ran the branches unconditionally (no
  discriminant test), so `if let Some(x) = get(5)` printed an empty `x` AND took
  the else branch. `if let` now tests the discriminant (`has_value` / `is_ok`,
  negated for `None` / `Err`), binds the payload from the typed union slot in the
  matched branch, and runs the unmatched branch otherwise. Verified end-to-end
  under MSVC: `if let Some(x) = get(5)` prints `got 10`, the `None` case prints
  `none 0`. Covered by `if_let_some_tests_discriminant_and_binds_payload`.
- Codegen (match on `&self` enum): `match self { Variant => ... }` inside a
  `&self`/`&mut self` enum method now compiles. The scrutinee is a pointer to the
  enum, so the enum-tag match path (which keys on a `Struct` scrutinee) was
  skipped and the generic fallback emitted an invalid struct/pointer `==`
  comparison (`(_2 == (Dir){ .tag = 0 })` - C2088/C2440). `lower_match` now
  dereferences a pointer-to-enum scrutinee to the enum value before dispatching,
  so the tag comparison applies. Verified end-to-end under MSVC: `impl Dir { fn
  code(&self) -> i32 { match self { ... } } }` prints `c 2`. Covered by
  `match_on_ref_enum_dereferences_for_the_tag_path`.
- Stdlib (`Option<T>` methods): `is_some()`, `is_none()`, `unwrap()`, and
  `unwrap_or(default)` now work. They were unimplemented (undefined symbols that
  failed to link). `is_some`/`is_none` read the `has_value` discriminant;
  `unwrap` reads the typed payload slot; `unwrap_or` branches on `has_value`,
  reading the payload when present and the default otherwise. Verified end-to-end
  under MSVC: `find(5).unwrap_or(0)` is `50`, `find(0).unwrap_or(-1)` is `-1`,
  `is_some()`/`is_none()` return the right booleans. Covered by
  `option_methods_is_some_and_unwrap_or`. (Payload slot uses the tracked inner
  type, defaulting to i32 when untracked - same threading caveat as the match.)
- Stdlib (iterator `.any()` / `.all()`): predicate terminals join the accumulator
  family. `.any(|x| pred)` folds the per-element predicate with OR from `false`;
  `.all(|x| pred)` with AND from `true`. Without them a chain ending in either
  left `.iter()` undefined. Verified end-to-end under MSVC over `[1,2,3,4]`:
  `any(x>3)`=true, `any(x>9)`=false, `all(x>0)`=true, `all(x>2)`=false. Covered by
  `iterator_any_all_predicate_terminals_desugar`. (Evaluates the whole range - no
  early short-circuit - which is correct but not optimal.)
- Stdlib (`String::push_str`): `s.push_str(x)` now appends in place. It was
  unimplemented (lowered to an undefined `push_str` symbol that failed to link).
  It now reassigns the receiver local to `build_string_concat(s, x)` (string
  literals already lower to `BuildString`, so the argument needs no coercion) and
  returns unit. Verified end-to-end under MSVC: `String::from("Hello")` then
  `push_str(", World")` prints `Hello, World`. Covered by
  `string_push_str_appends_in_place_via_concat`.
- Codegen (trait vtable wrappers): a trait method taking `&self` / `&mut self`
  now compiles. The generated vtable wrapper always dereferenced `void* __self`
  to a value before calling the concrete method, so a `&self` method (which takes
  `Type*`) got a value where a pointer was expected (`Dog_say((*(Dog*)__self))` -
  C2440). The wrapper now passes `(Type*)__self` when the self parameter is a
  pointer and the dereferenced value only for by-value `self`. This generated for
  every `impl Trait for Type`, so it broke compilation of any program with a
  `&self` trait method even without dynamic dispatch. Verified end-to-end under
  MSVC: `impl Speak for Dog { fn say(&self) ... }` prints `s 7`. Covered by
  `vtable_wrapper_passes_pointer_self_for_ref_methods`.
- Stdlib (iterator `.filter()`): `.filter(|x| pred)` is now a real iterator step.
  A chain containing it (e.g. `v.iter().filter(|x| x > 2).sum()`) previously left
  `.iter()` undefined because `filter` wasn't a recognized step. The desugaring
  evaluates the predicate per element and branches straight to the loop increment
  when it does not hold, skipping that element from the rest of the pipeline and
  the terminal. Composes with `.map()` and all terminals. Verified end-to-end
  under MSVC over `[1,2,3,4,5]`: `filter(x<3).sum()` is `3`,
  `filter(x>2).map(x*10).sum()` is `120`, `filter(x>3).count()` is `2`. Covered by
  `iterator_filter_step_skips_non_matching_elements`.
- Stdlib (`format!`): now actually formats. It was a stub that returned the raw
  template string and dropped every argument (so `format!("{} is {}", name, age)`
  yielded a bare `const char*` that printed as a pointer). `format!` now reuses
  the same format-string + argument processing as `println!` (extracted into a
  shared `prepare_format_call`) and builds an owned `BuildString` via a new
  variadic `build_sprintf` runtime function (vsnprintf into a heap buffer).
  Placeholders, precision (`{:.2}`), and mixed int/float/String arguments all
  work. Verified end-to-end under MSVC: `format!("{} is {}", name, 30)` →
  `Bob is 30`; `format!("{:.2} pi, {} {}", 3.14159, 42, "items")` → `3.14 pi, 42
  items`. Covered by `format_macro_builds_a_string_from_args_not_a_bare_template`.
- Stdlib (iterator `.count()` / `.product()`): both join `.sum()` as recognized
  accumulator terminals. `.count()` lowers to a `+1`-per-element i64 counter;
  `.product()` to an `acc = acc * elem` loop from one. Without them a chain
  ending in either left `.iter()` as an undefined call. Verified end-to-end under
  MSVC: over `[2,3,4]`, `count` is `3` and `product` is `24` (alongside the
  existing `sum`). Covered by `iterator_count_terminal_counts_elements` and
  `iterator_product_terminal_multiplies_elements`.
- Stdlib (iterator `.sum()`): `v.iter()...sum()` is now a recognized terminal,
  desugaring to an accumulator loop (`acc = acc + elem` from a zero of the output
  element type). Previously only `.collect()` and `.fold()` triggered iterator-
  chain desugaring, so a chain ending in `.sum()` left `.iter()` as an undefined
  `iter` call that failed to link. Composes with the existing `.map()` step.
  Verified end-to-end under MSVC: `v.iter().map(|x| x * 2).sum()` over
  `[1,2,3,4]` prints `sum 20`. Covered by
  `iterator_sum_terminal_desugars_to_an_accumulator_loop`.
- Stdlib (nested sum types): `Ok(None)` / `Some(None)` in a function returning a
  nested type like `Result<Option<i32>, String>` now box the inner `Option`
  payload correctly. `None` is a non-local constant, so the construction handler
  did not detect it as an aggregate and cast the 16-byte `Option` struct into the
  8-byte scalar slot (a C error and a mismatch with the boxed read). A shared
  `sumtype_arg_type` helper now resolves the `None` value to `Option`, so all
  three constructors (`Ok`/`Err`/`Some`) box it. Verified end-to-end under MSVC: a
  `Result<Option<i32>, String>` matched through both layers prints `some 5` /
  `none 0` / `err neg`. Covered by `nested_result_of_option_boxes_a_none_payload`.
- Stdlib (`Vec<struct>`): a vector of a user struct (or other aggregate element)
  now constructs, pushes, indexes, and pops correctly. `Vec<P>::new()` /
  `v.push(P { .. })` previously dispatched to the `i32` element family
  (`build_hvec_new_i32` / `build_hvec_push_i32`), passing a struct where an
  `int32_t` was expected (a C error). The backend now emits monomorphized,
  element-sized wrappers (`build_hvec_new_<T>` / `push` / `get` / `pop`) keyed by
  the struct name for each aggregate Vec element type, riding the size-aware
  generic `BuildVec`, and both the `Vec::new` and method dispatch select them.
  Verified end-to-end under MSVC: a `Vec<Pt>` with three pushes summed via index
  prints `sum 66`. Covered by `vec_of_struct_uses_sized_element_wrappers`.
- Stdlib (sum-type method-call scrutinees): `match recv.method() { ... }` and
  `recv.method()?` now thread the `Result`/`Option` payload type from the
  method's signature, so a method returning `Result<f64, _>` / `Option<f64>` is
  read from the correct union slot. Previously only free-function calls and
  let-annotations were threaded; a method-call scrutinee defaulted to `i32` and
  read the wrong slot (silent garbage for non-`i32` payloads). The collection
  pass now records each impl method's payload types keyed by its mangled
  `Type_method` name, and the match-site resolver handles `MethodCall` by
  resolving the receiver type to that name. Verified end-to-end under MSVC: a
  method returning `Result<f64, String>` matched directly prints `ok 2.5`.
  Covered by `match_on_method_call_threads_the_result_payload_type`.
- Stdlib (`Result<T, E>` arbitrary Err): the `Err` payload is no longer limited
  to `String`. The runtime `Result` struct's `err` field is now a typed union
  (`{ int64_t err_i; double err_f; void* err_p; }`) symmetric to `ok`, so
  `Result<i32, i32>`, `Result<i32, f64>`, `Result<_, MyError>`, etc. work.
  `Err(e)` writes the typed slot (boxing payloads >8 bytes such as `String`); the
  match `Err` arm and `?` propagation read it back with the threaded Err type
  (per-local annotation, then the matched call's `Result<Ok, Err>` signature,
  then `String` as the default for unannotated string-error matches). Previously
  `err` was a hardcoded `BuildString`, so `Err(404)` emitted `r.err = 404`
  (assigning an int to a struct - a C error). Verified end-to-end under MSVC:
  `Result<i32, i32>` -> `err 404`, `Result<i32, f64>` -> `errf 3.14`, let-bound
  `Result<i32, i32>` -> `errc 500`, and the `String` case still prints `err bad`.
  Covered by `result_supports_a_non_string_err_payload`.
- Stdlib (`?` try operator): `expr?` on a runtime `Result`/`Option` now unwraps
  the success payload as the expression value and early-returns the whole value
  to propagate `Err`/`None`. Previously `?` was a silent no-op for the runtime
  sum types (it only handled user-defined tagged enums), so `let v = parse(s)?;`
  bound `v` to the entire `Result` struct and a later `v * 2` multiplied a struct
  (a C compile error). `lower_try` now branches on the `is_ok` / `has_value`
  discriminant, reads the payload from the typed slot (threading- and
  boxing-aware), and returns the scrutinee unchanged on the failure arm. Verified
  end-to-end under MSVC: `Result` `?` chain prints `ok 10` / `err neg`; `Option`
  `?` chain prints `some 8` / `none 0`. Covered by
  `try_operator_unwraps_result_ok_and_propagates_err`.
- Stdlib (sum-type large payloads): `Option<T>` and `Result<T, E>` now carry
  payloads that do not fit the 8-byte union slot (e.g. `String`/`BuildString`,
  24 bytes). `Some(s)` / `Ok(s)` box the payload (`malloc` + copy, pointer stored
  in the `.value.p` / `.ok.ok_p` slot) and the match deref-reads it
  (`*(BuildString*)…`). Previously the construct cast a struct to `int64_t`.
  Scalars and pointers still go inline. Verified end-to-end under MSVC:
  `Option<String>` prints `some found`, `Result<String, String>` prints
  `ok nonzero` / `err zero`. Covered by
  `option_string_payload_is_boxed_through_the_pointer_slot`. (The boxed
  allocation is freed only under the opt-in drop-analysis path; in the default
  no-free mode it leaks, consistent with current owned-string handling.)
- Stdlib (`Option<T>` payload threading): `match call() { Some(x) => ... }` on a
  direct call to a `-> Option<T>` function now reads the correct union slot for a
  non-`i32` scalar payload. Previously the match defaulted the payload type to
  `i32` and read `.value.i` even when construction wrote `.value.f` (e.g.
  `Option<f64>`), so the float bits were reinterpreted as an int (silent-wrong).
  A per-function side-table (`fn_option_inner_types`), captured in the collection
  pass from `-> Option<T>`, threads the payload type to the match site (symmetric
  to the `Result` Ok threading). Verified end-to-end under MSVC: `Option<f64>`
  prints `some 2.5`. Covered by
  `option_match_on_direct_call_reads_the_threaded_payload_slot`.
- Stdlib (`Result<T, E>`): `Ok(x)` / `Err(e)` now construct the runtime
  `Result` struct and `match r { Ok(x) => ..., Err(e) => ... }` branches on the
  `is_ok` discriminant, reading the Ok payload from the typed `ok` union slot
  (`.ok_i` / `.ok_f` / `.ok_p`) and the Err payload from the `err` `BuildString`.
  The Ok payload type is threaded from the binding annotation
  (`let r: Result<i32, String> = ...`) or the matched call's return signature, so
  a non-`i32` Ok payload reads the correct slot instead of silently defaulting to
  `i32`. Previously `Ok`/`Err` lowered to undefined calls into an `i32` dest (a
  C2440) and the match emitted `if (true)` with whole-struct binds (silent-wrong).
  Covered by `ok_err_construct_result_struct_not_bare_call` and
  `result_match_tests_is_ok_and_binds_typed_slots`; verified end-to-end under MSVC
  for `i32` and `f64` Ok payloads across direct-call and let-bound matches. (Err is
  always `BuildString` and Ok payloads >8 bytes, e.g. `Result<String, _>`, still
  need boxing - tracked separately.)
- Native FFI (variadic): extern functions accept a trailing C-style `...`
  (e.g. `fn printf(fmt: &str, ...) -> i32`). The parser records it on
  `FnSig.is_variadic`, lowering carries it to the MIR signature so the C backend
  emits a trailing `, ...`, and the type checker (`FnTy.is_variadic`) lets a
  variadic call pass more arguments than there are fixed parameters while a
  non-variadic call still enforces exact arity. `printf("%d and %d\n", 1, 2)`
  now parses, type-checks, and lowers to `printf(fmt, 1, 2)`. Covered by
  `extern_variadic_fn_parses`, `variadic_extern_emits_ellipsis_in_c`,
  `variadic_extern_call_with_extra_args_typechecks`, and a non-variadic arity
  regression test.
- Native FFI (export header): `buildc build --emit header` writes a C header
  (`main.h`) declaring the program's `extern "C"` exports, with an include
  guard, the integer/bool/size typedefs the prototypes use, and a
  `#ifdef __cplusplus extern "C"` linkage guard. C and C++ consumers can
  `#include` it and call into the compiled BuildLang code. Covered by
  `extern_c_fn_is_marked_c_export` and `c_export_header_declares_exports_only`.
- Native FFI (export): `extern "C" fn` is now accepted as a function
  *definition*, not only inside extern blocks. A C-ABI function definition gets
  external linkage and a stable, unmangled name, so it compiles to a
  non-`static` C function callable from C and any C-ABI language. Ordinary
  functions stay internal (`static`). This is the reciprocal of header-backed
  extern blocks. Covered by `extern_c_fn_definition_parses_as_function`,
  `extern_c_fn_definition_emits_non_static_export`, and
  `regular_fn_keeps_internal_static_linkage`.
- Native FFI: extern blocks accept an optional `header "..."` clause naming the
  backing C header. The C backend emits the matching `#include` (angle-bracket
  form for `"<sqlite3.h>"`, quoted form for `"mylib.h"`), de-duplicated and
  sorted for reproducible output, and no longer synthesizes a prototype for a
  header-backed function, so the header's real declaration is authoritative.
  This is the native, embedded integration path for any C-ABI library. Covered
  by parser, lowering, and C-backend tests (`extern_block_header_*`,
  `extern_header_clause_lowers_to_mir_link_header`, `c_backend_*header*`).
- Native FFI: foreign `static` declarations in extern blocks now lower and
  generate correct C. A foreign static is treated as an external declaration,
  never a definition: it carries the block's `header`/`link` clauses, so the C
  backend includes the header (or emits a bare `extern <type> <name>;` when no
  header backs it) and links the library. Previously a foreign static
  type-checked but produced C that referenced an undeclared symbol. Covered by
  `extern_static_lowers_to_external_global_with_header` and
  `c_backend_foreign_static_*` tests.
- Native FFI: extern blocks also accept an optional `link "..."` clause naming
  the library to link. `buildc build` passes it to the C compiler (`-lname`
  for gcc/clang/cc, `name.lib` for MSVC) and the emitted C records a greppable
  `// buildc-link: name` note. The `link` and `header` clauses may appear in
  either order, so a program that calls a third-party C library builds and
  links in one command. Covered by parser, lowering, `GeneratedCode`, and
  `user_link_flags` tests (`extern_block_link_*`,
  `extern_link_clause_lowers_to_mir_link_lib`, `generated_code_*link*`,
  `user_link_flags_format_per_toolchain`).
- Presentation pass: README hero and brand assets under `docs/brand/`, Build ecosystem navigation, and Current status / Operator surface blocks.
- Documented the operator surface across the `buildc` CLI and the bundled LSP server.
- Relicensed to the BuildLang Fair-Source License v1.0 under the operator's umbrella.

## [1.0.5] - 2026-03-28 - Self-Hosted Compiler Verification

### Proven - Self-Hosting: Complete Audit of All 9 Versions
- All 9 versions compile to C through BuildLang; 6 run to completion, 3 have runtime bugs
- **6 of 9 run to completion with verified correct output**:
  - v1: 3-pass pipeline generating C (`int x = 3 + 4; int y = x * 2;`)
  - v2: Functions + if/else + while (`square()`, `abs_val()`, `sum_to()`)
  - v3: Character lexer tokenizing `fn add(a, b)` into 28 tokens
  - v4: Token-driven parser building 8-node AST from `let x = 3 + 4;`
  - v5: Function definition parsing from token stream
  - v6: Structs + branching + loops from tokens
- **3 of 9 compile but have runtime bugs (infinite loops in character-level parsing)**:
  - v7, v8, v9: Hang during codegen - nested while loops in hand-written character parsers don't advance past certain token boundaries. Bug is in the `.bld` program logic, not in the BuildLang compiler.
- Self-hosted support libraries (Option, Cmp, Span, LexerTokens) all produce correct output

---

## [1.0.4] - 2026-03-28 - Module System & Use Resolution

### Added - Module Registry
- `TypeContext` now maintains a `module_bindings` registry mapping module names to their exported bindings
- Inline `mod foo { ... }` blocks register their bindings in the registry after type checking
- `current_scope_bindings()` snapshots a module's scope before it's popped

### Added - Use Statement Resolution
- `use foo::bar;` resolves through the module registry and imports the binding
- `use foo::bar as baz;` supports renaming
- `use foo::*;` glob imports all module bindings
- `use foo::{bar, baz};` nested imports resolve each sub-tree
- Resolution happens during the collection pass so imported items are available for forward references

### Changed - DESIGN.md
- Module system limitation updated: inline modules and use statements now work; external file modules remain unimplemented

### Verified
- 132/132 test programs compile (zero regression)
- 591 unit tests pass
- New module + use test programs compile successfully

---

## [1.0.3] - 2026-03-28 - Exhaustiveness Checking & Builtin Fixes

### Added - Pattern Exhaustiveness Checking
- Match expressions over enum types now produce a type error if not all variants are covered
- Error message names the missing variants: `non-exhaustive match: missing variants Blue`
- Wildcard patterns (`_`) and binding patterns recognized as catch-all arms
- `Or` patterns (`A | B`) correctly accumulate covered variants
- Enum resolution works even when scrutinee is an unresolved type variable (resolves from pattern paths)

### Fixed - Missing Builtin Registrations
- Registered `assert(bool)`, `assert_eq`, `println` as builtin functions in the type checker
- Registered typed vector builtins: `vec_get_f64`, `vec_push_f64`, `vec_new_f64`, `vec_pop_f64`, and i64 variants
- Registered string methods: `parse_int() -> i64`, `parse_float() -> f64`
- **132/132 test programs now compile** (was 121/132 due to missing builtins)

### Changed - DESIGN.md
- Pattern exhaustiveness moved from "Known Limitations" to "Resolved"
- Effect system limitation reworded as a deliberate design trade-off with rationale

---

## [1.0.2] - 2026-03-28 - End-to-End Proof & Depth

### Proven - Full Compilation Pipeline
- **108/108 test programs compile and run correctly**
- Pipeline: `.bld` → `buildc` → C99 → MSVC → native x86-64 → correct output
- Coverage: functions, recursion, closures, generics, traits, dynamic dispatch, algebraic effects, pattern matching, iterators, hashmaps, file I/O, vectors, color science, self-hosted compiler components
- See [TEST_RESULTS.md](TEST_RESULTS.md) for documented outputs

### Added - Type System Tests (78 new tests)
- Type inference: 40 tests (unification properties, bidirectional flow, occurs check, effect inference)
- Parser: 38 tests (10 operator precedence, 8 expression forms, 10 items, 10 patterns)
- Compiler unit tests: 518 → 588

### Added - Design Rationale (DESIGN.md)
- Why bidirectional inference instead of Algorithm W
- Why Pratt parsing instead of recursive descent
- Why setjmp/longjmp for algebraic effects
- Why color space annotations in the type system
- Known Limitations section (no borrow checker, eager monomorphization, one-shot effects)

---

## [1.0.1] - 2026-03-28 - Production Readiness & Code Quality

### CI/CD
- Added **clippy lint** job to GitHub Actions CI (`cargo clippy -- -D warnings`)
- Added **rustfmt check** job (`cargo fmt --check`)
- Added `[lints.clippy]` configuration to `Cargo.toml`

### Error Handling
- **pkg/lockfile.rs**: Converted 24 `.unwrap()` calls to `?` propagation
  - Added `Fmt(fmt::Error)` variant to `LockfileError`
  - Renamed `to_string()` to `serialize()` returning `Result<String, LockfileError>`
- **pkg/version.rs**: Converted 14 `.unwrap()` calls to `?` in test functions
- **runtime/async_rt.rs**: Annotated 36 Mutex lock unwraps as standard Rust practice
- **runtime/gc.rs**: Annotated 9 unwraps (7 Mutex locks + 2 structural guarantees)

### Documentation
- Added **unwrap policy** to `codegen/mod.rs` explaining why codegen unwraps are intentional assertions on validated AST
- Added policy notes to 4 backend files: llvm.rs, c.rs, arm64.rs, x86_64.rs
- Documented **backend maturity levels**: C (production), others (experimental)

### Audit Results
- **Lexer**: All 28 `panic!()` calls confirmed to be in test code only - production lexer has proper error handling with 30+ error variants
- **Parser**: Already uses `expect()` with messages (not `unwrap()`) - correct practice
- **Codegen**: 651 unwraps are assertions on type-checked AST (intentional, documented)
- **Runtime**: 45 unwraps are all Mutex locks (standard Rust, annotated)

---

## [1.0.0] - 2026-03-22

### Language Features
- Generics with trait bounds and where clauses
- Pattern matching with exhaustiveness checking
- Closures with capture semantics
- Algebraic effects and effect handlers
- Built-in color space types (sRGB, Linear, ACES, Oklab, HSL, HSV)
- Ownership and borrowing system
- Module system with visibility controls
- Macro system with hygiene

### Compiler
- C backend (stable, primary target)
- HLSL shader output
- GLSL shader output
- SPIR-V binary shader output
- x86-64 native backend (experimental)
- AArch64 native backend (experimental)
- WASM backend (experimental)
- LLVM IR backend (experimental)
- 8 total code generation backends

### Tooling
- LSP server with completion, hover, and diagnostics
- VS Code extension with syntax highlighting and LSP integration
- CLI (`buildc`) with lex, parse, check, build, and run subcommands
- Package manager (`build pkg`) with dependency resolution
- Code formatter (`build fmt`)

### Known Limitations
- Non-C backends (x86-64, AArch64, WASM, LLVM) are experimental and may not support all language features
- Package manager is not connected to a live registry
- Formatter is not wired into the CLI pipeline
