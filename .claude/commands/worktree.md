---
description: Create or switch git worktrees for parallel work
allowed-tools: Bash(git:*)
---

# Git Worktree Management

## Create a New Worktree
```bash
git worktree add ../quantalang-<branch-name> -b <branch-name>
```

## List Worktrees
```bash
git worktree list
```

## Remove a Worktree
```bash
git worktree remove ../quantalang-<branch-name>
```

## Workflow
1. Ask the user what feature/fix they want to work on
2. Create a descriptive branch name: `feat/<name>`, `fix/<name>`, `refactor/<name>`
3. Create the worktree at `../quantalang-<branch-name>`
4. Confirm the worktree was created and show the path
5. Remind the user to `cd` into the worktree to work on it

## Rules
- Always create worktrees as siblings to the main repo (in `../`)
- Use the naming convention `quantalang-<branch-name>` for worktree directories
- Never create a worktree for `main` or `master`
