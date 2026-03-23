---
description: Show available Claude Code commands for the QuantaLang project
allowed-tools:
---

# Available Commands

| Command | Description |
|---------|-------------|
| `/review` | Review code for safety, correctness, and Rust best practices |
| `/commit` | Stage changes and create a well-formatted commit |
| `/refactor` | Refactor code with cargo fmt, clippy, and size gates |
| `/worktree` | Create or switch git worktrees for parallel work |
| `/progress` | Show project stats: .rs file count, test count, build status |
| `/test-plan` | Generate a test plan for recent changes |
| `/security-check` | Run security audit: cargo audit, unsafe blocks, secrets scan |
| `/help` | Show this help message |

## Agents

| Agent | Description |
|-------|-------------|
| `code-reviewer` | Senior Rust code reviewer (safety, correctness, performance) |
| `test-writer` | Writes comprehensive Rust tests with proper assertions |

## Hooks (automatic)

- **PreToolUse**: Blocks access to sensitive files, warns on main branch commits
- **PostToolUse**: Runs `cargo check` when .rs files are saved
- **Stop**: Scans staged files for secrets before session ends
