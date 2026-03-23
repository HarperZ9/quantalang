#!/usr/bin/env bash
# Lint on Save Hook — PostToolUse (Rust project)

INPUT=$(cat)
FILE_PATH=$(echo "$INPUT" | python -c "import sys,json; print(json.load(sys.stdin).get('tool_input',{}).get('file_path',''))" 2>/dev/null)

if [ -z "$FILE_PATH" ]; then exit 0; fi

EXTENSION="${FILE_PATH##*.}"

case "$EXTENSION" in
    rs)
        # Rust — run cargo check if Cargo.toml is present
        CARGO_DIR="$FILE_PATH"
        while [ "$CARGO_DIR" != "/" ] && [ "$CARGO_DIR" != "." ]; do
            CARGO_DIR=$(dirname "$CARGO_DIR")
            if [ -f "$CARGO_DIR/Cargo.toml" ]; then
                cd "$CARGO_DIR" && cargo check 2>&1 | tail -10
                break
            fi
        done
        ;;
    toml)
        # TOML — basic syntax check
        ;;
    quanta)
        # QuantaLang source — could run quantac check if available
        ;;
esac

exit 0
