---
description: Generate a test plan for recent changes
allowed-tools: Read, Grep, Glob, Bash(git:*, cargo:*)
---

# Test Plan Generator

## Steps

1. Run `git diff --name-only HEAD~5` to see recently changed files
2. For each changed `.rs` file:
   - Read the file to understand what changed
   - Identify public functions, structs, and traits that need tests
   - Check if tests already exist (look for `#[cfg(test)]` module or `tests/` directory)
3. Generate a test plan covering:

## Test Categories

### Unit Tests
- Test each public function with valid inputs (happy path)
- Test each public function with invalid inputs (error path)
- Test edge cases (empty input, max values, unicode, nested structures)
- Test Result/Option return types for both variants

### Integration Tests
- Test compiler pipeline stages end-to-end
- Test that valid `.quanta` programs compile and produce correct output
- Test that invalid `.quanta` programs produce helpful error messages

### Regression Tests
- For each bug fix, add a test that would have caught the bug
- Test previously failing `.quanta` programs from `examples/` or `tests/`

### Property Tests (if proptest is available)
- Parsing round-trips: parse -> AST -> pretty-print -> parse again
- Type checking: well-typed programs don't produce type errors
- Codegen: generated code is deterministic

## Output Format
```
## Test Plan for [files]

### [module_name]
- [ ] test_function_happy_path — description
- [ ] test_function_error_case — description
- [ ] test_function_edge_case — description
```
