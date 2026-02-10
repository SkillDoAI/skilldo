#!/usr/bin/env bash
set -euo pipefail

# Batch comparison run: generate skills with the new prompt (no prompt leaks)
# Output goes to /tmp/skilldo-compare/ so we can diff against examples/skills/

SKILLDO="./target/release/skilldo"
CONFIG="./dev/configs/gpt52-config.toml"
REPOS="/tmp/skilldo-repos"
OUTDIR="/tmp/skilldo-compare"
LOGDIR="/tmp/skilldo-compare/logs"

source ~/.openai

mkdir -p "$OUTDIR" "$LOGDIR"

# Libraries that had prompt leaks in v0.1
LIBS="numpy fastapi typer boto3 click requests pillow flask"

passed=0
failed=0
total=0

for lib in $LIBS; do
    total=$((total + 1))
    echo ""
    echo "=== [$total] Generating $lib ==="
    start=$(date +%s)

    if "$SKILLDO" generate "$REPOS/$lib" \
        --config "$CONFIG" \
        -o "$OUTDIR/${lib}-SKILL.md" \
        > "$LOGDIR/${lib}.log" 2>&1; then
        end=$(date +%s)
        echo "  PASS ($(( end - start ))s)"
        passed=$((passed + 1))
    else
        end=$(date +%s)
        echo "  FAIL ($(( end - start ))s) — see $LOGDIR/${lib}.log"
        failed=$((failed + 1))
    fi
done

echo ""
echo "=== SUMMARY ==="
echo "Passed: $passed / $total"
echo "Failed: $failed / $total"
echo ""

# Quick prompt leak check on generated files
echo "=== PROMPT LEAK CHECK ==="
leak_count=0
for f in "$OUTDIR"/*-SKILL.md; do
    [ -f "$f" ] || continue
    leaks=$(grep -cE "(CRITICAL:|Show the standard import|do NOT skip|REQUIRED sections:|minimum 3, maximum 5)" "$f" 2>/dev/null || echo "0")
    if [ "$leaks" -gt 0 ]; then
        echo "  LEAK: $(basename "$f") — $leaks leaked phrases"
        leak_count=$((leak_count + 1))
    fi
done

if [ "$leak_count" -eq 0 ]; then
    echo "  No prompt leaks detected!"
else
    echo "  $leak_count files with prompt leaks"
fi

# Check generated_with
echo ""
echo "=== GENERATED_WITH CHECK ==="
for f in "$OUTDIR"/*-SKILL.md; do
    [ -f "$f" ] || continue
    gw=$(grep "generated_with:" "$f" 2>/dev/null || true)
    if [ -n "$gw" ]; then
        echo "  OK: $(basename "$f") — $gw"
    else
        echo "  MISSING: $(basename "$f")"
    fi
done
