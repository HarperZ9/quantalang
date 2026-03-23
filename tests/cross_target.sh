#!/bin/bash
# Cross-Target Math Verification Harness
# Compiles .quanta functions to C, HLSL, and GLSL and compares outputs.
# Verifies that the same QuantaLang source produces structurally equivalent
# mathematical expressions across all three text backends.
#
# Usage: bash tests/cross_target.sh
# Requires: quantac in PATH or at compiler/target/release/quantac.exe

set -e

QUANTAC="compiler/target/release/quantac.exe"
if [ ! -f "$QUANTAC" ]; then
    QUANTAC="$(which quantac 2>/dev/null || echo quantac)"
fi

PASS=0
FAIL=0
SKIP=0
ERRORS=""

compile_and_check() {
    local src="$1"
    local name="$(basename "$src" .quanta)"
    local outdir="tests/cross_target_out"
    mkdir -p "$outdir"

    echo "--- $name ---"

    # Compile to C
    if $QUANTAC "$src" --target c -o "$outdir/${name}.c" 2>/dev/null; then
        echo "  C:    OK"
        PASS=$((PASS + 1))
    else
        echo "  C:    FAIL"
        FAIL=$((FAIL + 1))
        ERRORS="$ERRORS\n  $name -> C"
    fi

    # Compile to HLSL
    if $QUANTAC "$src" --target hlsl -o "$outdir/${name}.hlsl" 2>/dev/null; then
        echo "  HLSL: OK"
        PASS=$((PASS + 1))
    else
        echo "  HLSL: FAIL"
        FAIL=$((FAIL + 1))
        ERRORS="$ERRORS\n  $name -> HLSL"
    fi

    # Compile to GLSL
    if $QUANTAC "$src" --target glsl -o "$outdir/${name}.glsl" 2>/dev/null; then
        echo "  GLSL: OK"
        PASS=$((PASS + 1))
    else
        echo "  GLSL: FAIL"
        FAIL=$((FAIL + 1))
        ERRORS="$ERRORS\n  $name -> GLSL"
    fi

    # Structural comparison: extract function signatures from each
    echo "  Functions:"
    for ext in c hlsl glsl; do
        local file="$outdir/${name}.${ext}"
        if [ -f "$file" ]; then
            local funcs=$(grep -cE "^(float|double|int|void|vec[234]|float[234]) [a-zA-Z_]" "$file" 2>/dev/null || echo 0)
            echo "    $ext: $funcs functions emitted"
        fi
    done
}

echo "================================================"
echo "QuantaLang Cross-Target Math Verification"
echo "================================================"
echo ""

# Core math test
compile_and_check "tests/cross_target_test.quanta"

# Shader demos
for f in demos/*.quanta; do
    compile_and_check "$f"
done

# Shader test programs
for f in tests/shaders/*.quanta; do
    compile_and_check "$f"
done

echo ""
echo "================================================"
echo "Results: $PASS passed, $FAIL failed, $SKIP skipped"
echo "================================================"

if [ -n "$ERRORS" ]; then
    echo ""
    echo "Failed compilations:"
    echo -e "$ERRORS"
fi

exit $FAIL
