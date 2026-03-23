---
description: Run security audit — cargo audit, unsafe blocks, secrets scan
allowed-tools: Bash(cargo:*, grep:*, find:*, git:*), Read, Grep, Glob
---

# Security Check

Run a comprehensive security audit of the QuantaLang project.

## 1. Dependency Audit
```bash
# Check for known vulnerabilities in dependencies
cargo audit 2>&1 || echo "cargo-audit not installed — run: cargo install cargo-audit"
```

## 2. Unsafe Code Audit
```bash
# Find all unsafe blocks and functions
grep -rn "unsafe" --include="*.rs"
```

For each `unsafe` block found:
- Verify it has a `// SAFETY:` comment explaining why it's needed
- Check that the invariants are actually upheld
- Determine if a safe alternative exists

## 3. Secrets Scan
```bash
# Check for hardcoded secrets
grep -rniE '(api[_-]?key|secret|password|token)\s*[:=]' --include="*.rs" --include="*.toml"
# Check for test credentials that look real
grep -rnE 'AKIA[0-9A-Z]{16}' --include="*.rs"
grep -rnE '(ghp_|gho_|github_pat_)' --include="*.rs"
```

## 4. Input Validation
- Check all places that read user input (file I/O, CLI args, `.quanta` source parsing)
- Verify bounds checking on array/slice access
- Check for potential integer overflow in arithmetic
- Verify no `panic!()` in library code paths (use `Result` instead)

## 5. File System Safety
- Check for path traversal vulnerabilities in file operations
- Verify temp files are cleaned up
- Check that file permissions are set correctly

## Output Format
```
## Security Audit Results

### Critical (must fix)
- ...

### Warnings (should fix)
- ...

### Info (consider)
- ...

### Clean
- [x] No known dependency vulnerabilities
- [x] All unsafe blocks justified
- [x] No hardcoded secrets
```
