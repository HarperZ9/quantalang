# Security Policy

BuildLang is a compiler and toolchain. A defect here can turn source a user
trusts into a binary that behaves differently than the source says, so security
reports are treated as first-class.

## Supported versions

Fixes land on the latest published `1.2.x` line on crates.io. Older lines do not
receive backports. Upgrade to the current release before reporting, and name the
`buildc version` output in your report.

| Version | Supported |
|---|---|
| latest `1.2.x` | yes |
| earlier | no |

## Reporting a vulnerability

Report privately through GitHub. Open the repository's **Security** tab and
choose **Report a vulnerability**, which starts a private advisory visible only
to you and the maintainer. Do not open a public issue or pull request for a
suspected vulnerability, and do not disclose it elsewhere until a fix ships.

A useful report includes:

- the `buildc version` and your host OS and C compiler,
- the smallest `.bld` input or command that reproduces the problem,
- what you observed and what you expected,
- the impact you see, stated plainly.

This is a single-maintainer project. Expect an acknowledgement within about a
week and a status update as the fix is worked. Those are good-faith targets, not
a contractual SLA. A report that includes a minimal reproduction is handled
faster, because reproduction is most of the work.

## What is in scope

- **Miscompilation.** Any input where `buildc` produces a binary whose behavior
  contradicts the source, silently and without a diagnostic. The compiler's core
  promise is that it never returns a silently wrong answer: it computes correctly
  or it emits a diagnostic and exits nonzero. A counterexample is a security bug,
  not just a correctness bug.
- **Capability-effect escapes.** An input that calls a capability-gated operation
  (`FileSystem`, `Network`, `Foreign`, `Console`, and the rest) without the effect
  appearing in the function's type, so the checker fails to see ambient access it
  should have required.
- **Receipt forgery.** A sealed receipt that `buildc receipt verify` accepts even
  though the sealed content was altered, or a program state the receipt claims to
  witness that it did not.
- **Toolchain execution flaws.** Path handling, temporary-file handling, or C
  compiler invocation in `buildc` that lets crafted input read or write outside
  the intended files, or run a command the user did not ask for.

## What is out of scope

- The experimental backends (SPIR-V, LLVM IR, WASM, Rust, x86-64, ARM64), GPU
  dispatch, and `#[linear]` types are labeled experimental in the README and are
  known to be incomplete. Soundness gaps there are tracked as ordinary issues,
  not security reports, until their status changes.
- Bugs in your system C compiler, linker, or a third-party library linked through
  `extern "C"`. Report those to their own projects.
- A program that does exactly what its declared capabilities allow. Effects are a
  policy surface, not a sandbox; a function that declares `~ FileSystem` may touch
  the file system.

## Disclosure

Coordinated. Once a fix is released, the advisory is published with credit to the
reporter unless the reporter asks to stay anonymous. A CVE is requested through
GitHub when the impact warrants one.
