#!/usr/bin/env bash
# Branch Protection Hook — PreToolUse (Bash)
# Blocks committing directly to main/master.

INPUT=$(cat)

if command -v jq &>/dev/null; then
    COMMAND=$(echo "$INPUT" | jq -r '.tool_input.command // ""' 2>/dev/null)
else
    COMMAND=$(echo "$INPUT" | grep -o '"command"[[:space:]]*:[[:space:]]*"[^"]*"' | head -1 | sed 's/.*:[[:space:]]*"//;s/"$//')
fi

if [ -z "$COMMAND" ]; then exit 0; fi
if ! echo "$COMMAND" | grep -qE 'git\s+commit'; then exit 0; fi

if ! git rev-parse --is-inside-work-tree &>/dev/null; then exit 0; fi
if ! git rev-parse HEAD &>/dev/null 2>&1; then exit 0; fi

BRANCH=$(git branch --show-current 2>/dev/null)
if [ "$BRANCH" != "main" ] && [ "$BRANCH" != "master" ]; then exit 0; fi

echo "WARNING: You're committing directly to '$BRANCH'." >&2
echo "Consider using a feature branch: git checkout -b feat/<name>" >&2
exit 0
