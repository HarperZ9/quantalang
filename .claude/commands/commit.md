---
description: Stage changes and create a well-formatted commit
allowed-tools: Bash(git:*), Read, Grep, Glob
---

# Commit Workflow

## Steps

1. Run `git status` to see all changes
2. Run `git diff` to review what changed
3. Run `git log --oneline -10` to match the project's commit style
4. Run `cargo check` to verify the project compiles
5. Run `cargo test` to verify tests pass
6. Stage the relevant files (prefer explicit paths over `git add .`)
7. Write a clear commit message following this format:

```
<type>: <short summary>

<optional body explaining why, not what>
```

Types: feat, fix, refactor, test, docs, perf, chore

## Rules
- Do NOT stage files matching: `.env*`, `secrets.*`, `*.pem`, `*.key`
- Do NOT commit if `cargo check` fails
- Do NOT commit if tests fail (unless explicitly told to skip tests)
- Warn if committing to main/master
