#!/bin/bash
# QuantaLang End-to-End Test Runner
# Compiles each .quanta test program to C, compiles the C, runs it,
# and compares output to .expected file.

COMPILER="cargo run --manifest-path compiler/Cargo.toml --quiet --"
PASS=0
FAIL=0
SKIP=0
TOTAL=0

for qfile in tests/programs/*.quanta; do
    name=$(basename "$qfile" .quanta)
    expected="tests/programs/${name}.expected"
    c_out="/tmp/quantatest_${name}.c"
    exe_out="/tmp/quantatest_${name}"

    TOTAL=$((TOTAL + 1))

    if [ ! -f "$expected" ]; then
        echo "SKIP $name (no .expected file)"
        SKIP=$((SKIP + 1))
        continue
    fi

    # Step 1: Compile .quanta to .c
    $COMPILER "$qfile" -o "$c_out" 2>/dev/null
    if [ $? -ne 0 ]; then
        echo "FAIL $name (compilation to C failed)"
        FAIL=$((FAIL + 1))
        continue
    fi

    # Step 2: Compile .c to executable
    cc -std=c99 -o "$exe_out" "$c_out" -lm 2>/dev/null
    if [ $? -ne 0 ]; then
        echo "FAIL $name (C compilation failed)"
        FAIL=$((FAIL + 1))
        continue
    fi

    # Step 3: Run and compare output (normalize \r\n to \n for Windows)
    actual=$("$exe_out" 2>&1 | tr -d '\r')
    expected_content=$(cat "$expected" | tr -d '\r')

    if [ "$actual" = "$expected_content" ]; then
        echo "PASS $name"
        PASS=$((PASS + 1))
    else
        echo "FAIL $name"
        echo "  Expected: $(head -1 "$expected")"
        echo "  Actual:   $(echo "$actual" | head -1)"
        FAIL=$((FAIL + 1))
    fi

    # Cleanup
    rm -f "$c_out" "$exe_out"
done

echo ""
echo "Results: $PASS passed, $FAIL failed, $SKIP skipped ($TOTAL total)"
exit $FAIL
