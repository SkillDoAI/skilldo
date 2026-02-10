#!/usr/bin/env bash
# Overnight Batch SKILL.md Generation
# Regenerates all example skills with generated_with tracking
# Uses variety of models across libraries
# Usage: ./dev/scripts/batch-generate.sh [--skip-clone]

set +e  # Continue on failure

# --- Paths ---
PROJECT_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SKILLDO="$PROJECT_ROOT/target/release/skilldo"
CONFIG_DIR="$PROJECT_ROOT/dev/configs/batch"
OUTPUT_DIR="$PROJECT_ROOT/examples/skills"
BAD_DIR="$PROJECT_ROOT/dev/bad-outputs"
LOG_DIR="/tmp/skilldo-batch/logs"
COMPARE_DIR="/tmp/skilldo-batch/comparison"
CLONE_DIR="/tmp/skilldo-repos"
SUMMARY="/tmp/skilldo-batch/summary.md"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

mkdir -p "$LOG_DIR" "$BAD_DIR" "$COMPARE_DIR" "$CLONE_DIR"

# --- API Keys ---
# Handle key files that may contain bare keys or "export VAR=key" format
_extract_key() {
    local raw
    raw=$(cat "$1" 2>/dev/null | head -1) || return
    # Strip "export VAR=" prefix if present
    raw=$(echo "$raw" | sed 's/^export [^=]*=//')
    # Strip surrounding quotes
    raw="${raw%\"}" ; raw="${raw#\"}"
    raw="${raw%\'}" ; raw="${raw#\'}"
    echo "$raw"
}
export OPENAI_API_KEY=$(_extract_key ~/.openai)
export ANTHROPIC_API_KEY=$(_extract_key ~/.anthropic)

# --- Library repos (parallel arrays for bash 3.2 compat) ---
LIB_NAMES=(arrow click flask keras pandas pydantic pytorch requests rich scikit-learn scipy sqlalchemy transformers httpx fastapi django matplotlib pytest typer)
LIB_URLS=(
    "https://github.com/arrow-py/arrow.git"
    "https://github.com/pallets/click.git"
    "https://github.com/pallets/flask.git"
    "https://github.com/keras-team/keras.git"
    "https://github.com/pandas-dev/pandas.git"
    "https://github.com/pydantic/pydantic.git"
    "https://github.com/pytorch/pytorch.git"
    "https://github.com/psf/requests.git"
    "https://github.com/Textualize/rich.git"
    "https://github.com/scikit-learn/scikit-learn.git"
    "https://github.com/scipy/scipy.git"
    "https://github.com/sqlalchemy/sqlalchemy.git"
    "https://github.com/huggingface/transformers.git"
    "https://github.com/encode/httpx.git"
    "https://github.com/fastapi/fastapi.git"
    "https://github.com/django/django.git"
    "https://github.com/matplotlib/matplotlib.git"
    "https://github.com/pytest-dev/pytest.git"
    "https://github.com/fastapi/typer.git"
)

# ========== FUNCTIONS ==========

preflight() {
    echo "=== Pre-flight checks ==="
    local ok=1

    [ -f "$SKILLDO" ] && echo "  OK: Binary" || { echo "  FAIL: Binary not at $SKILLDO"; ok=0; }
    [ -n "$OPENAI_API_KEY" ] && echo "  OK: OpenAI key" || { echo "  FAIL: OPENAI_API_KEY empty"; ok=0; }
    [ -n "$ANTHROPIC_API_KEY" ] && echo "  OK: Anthropic key" || { echo "  FAIL: ANTHROPIC_API_KEY empty"; ok=0; }
    curl -s http://localhost:11434/api/tags >/dev/null 2>&1 && echo "  OK: Ollama running" || echo "  WARN: Ollama not running (local models will skip)"
    command -v podman &>/dev/null && echo "  OK: Podman" || { echo "  FAIL: Podman not found"; ok=0; }

    [ "$ok" -eq 0 ] && { echo "FATAL: Pre-flight failed"; exit 1; }
    echo ""
}

clone_repos() {
    echo "=== Cloning repos to $CLONE_DIR ==="
    local i=0
    for lib in "${LIB_NAMES[@]}"; do
        if [ -d "$CLONE_DIR/$lib" ]; then
            echo "  Skip: $lib (exists)"
        else
            echo "  Cloning: $lib..."
            git clone --depth 1 "${LIB_URLS[$i]}" "$CLONE_DIR/$lib" 2>/dev/null
            [ $? -ne 0 ] && echo "  FAIL: Could not clone $lib"
        fi
        i=$((i + 1))
    done
    echo ""
}

# Run skilldo and output to the given path
# Args: config_key lib_name output_path
run_one() {
    local config_key="$1"
    local lib="$2"
    local out="$3"
    local config_file="$CONFIG_DIR/${config_key}.toml"
    local log_file="$LOG_DIR/${config_key}-${lib}-${TIMESTAMP}.log"
    local start=$(date +%s)

    # Ensure output directory exists
    mkdir -p "$(dirname "$out")"

    echo "[$(date +%H:%M:%S)] $lib  <-  $config_key"

    cd "$PROJECT_ROOT"
    timeout 3600 "$SKILLDO" generate "$CLONE_DIR/$lib" \
        --language python \
        --config "$config_file" \
        --output "$out" \
        2>&1 | tee "$log_file"
    local rc=${PIPESTATUS[0]}

    local dur=$(( $(date +%s) - start ))
    local lines=0
    [ -f "$out" ] && lines=$(wc -l < "$out" 2>/dev/null)

    if [ $rc -ne 0 ]; then
        echo "  FAIL ($rc) ${dur}s"
        echo "| $lib | $config_key | FAIL | exit=$rc | ${dur}s |" >> "$SUMMARY"
        return 1
    fi

    if [ "$lines" -lt 50 ]; then
        echo "  BAD (${lines} lines) ${dur}s"
        [ -f "$out" ] && cp "$out" "$BAD_DIR/${config_key}-${lib}-SKILL.md" 2>/dev/null
        echo "| $lib | $config_key | BAD | ${lines}L | ${dur}s |" >> "$SUMMARY"
        return 1
    fi

    # Check for generated_with
    local gw=$(grep -c "generated_with:" "$out" 2>/dev/null || echo 0)
    echo "  OK (${lines}L, gw=$gw) ${dur}s"
    echo "| $lib | $config_key | PASS | ${lines}L | ${dur}s |" >> "$SUMMARY"
    return 0
}

# Run primary: output to examples/skills/. On fail, save bad output.
run_primary() {
    local config_key="$1"
    local lib="$2"
    local out="$OUTPUT_DIR/${lib}-SKILL.md"

    run_one "$config_key" "$lib" "$out"
    local result=$?
    if [ $result -ne 0 ] && [ -f "$out" ]; then
        cp "$out" "$BAD_DIR/${config_key}-${lib}-SKILL.md" 2>/dev/null
    fi
    return $result
}

# Run comparison: output to comparison dir (doesn't overwrite primary)
run_comparison() {
    local config_key="$1"
    local lib="$2"
    run_one "$config_key" "$lib" "$COMPARE_DIR/${config_key}-${lib}-SKILL.md"
}

# ========== MAIN ==========

echo "========================================="
echo "  Batch SKILL.md Generation"
echo "  Started: $(date)"
echo "========================================="
echo ""

preflight

# Build
echo "=== Building release binary ==="
cd "$PROJECT_ROOT"
cargo build --release 2>&1 | tail -3
echo ""

# Clone (unless --skip-clone)
if [[ "$*" != *"--skip-clone"* ]]; then
    clone_repos
fi

# Init summary
cat > "$SUMMARY" << 'HEADER'
# Batch Generation Summary

| Library | Model | Result | Details | Time |
|---------|-------|--------|---------|------|
HEADER

# ---- Tier 1: GPT-5.2 on small/medium libs ----
echo "=== Tier 1: GPT-5.2 (small/medium) ==="
for lib in requests flask click httpx fastapi pytest typer; do
    run_primary "gpt52" "$lib"
done
echo ""

# ---- Tier 2: Hybrid (qwen3+gpt52) on large libs ----
echo "=== Tier 2: Hybrid qwen3+gpt52 (large libs) ==="
T2_FAILS=()
for lib in pandas pytorch scikit-learn django matplotlib transformers; do
    run_primary "hybrid-qwen-gpt52" "$lib"
    [ $? -ne 0 ] && T2_FAILS+=("$lib")
done
echo ""

# Tier 2 fallback: if hybrid failed, try claude-sonnet
if [ ${#T2_FAILS[@]} -gt 0 ]; then
    echo "=== Tier 2 fallback: claude-sonnet for ${#T2_FAILS[@]} failures ==="
    for lib in "${T2_FAILS[@]}"; do
        run_primary "claude-sonnet" "$lib"
    done
    echo ""
fi

# ---- Tier 3: Claude Sonnet ----
echo "=== Tier 3: Claude Sonnet ==="
for lib in keras sqlalchemy pydantic scipy; do
    run_primary "claude-sonnet" "$lib"
done
echo ""

# ---- Tier 4: Qwen3-Coder solo (free) ----
echo "=== Tier 4: Qwen3-Coder (local) ==="
T4_FAILS=()
for lib in arrow rich; do
    run_primary "qwen3-coder" "$lib"
    [ $? -ne 0 ] && T4_FAILS+=("$lib")
done
# If qwen3 tanks, escalate to claude-sonnet per user request
if [ ${#T4_FAILS[@]} -gt 0 ]; then
    echo "  qwen3 failed on: ${T4_FAILS[*]} — escalating to claude-sonnet"
    for lib in "${T4_FAILS[@]}"; do
        # Save qwen3's bad output for analysis
        [ -f "$OUTPUT_DIR/${lib}-SKILL.md" ] && cp "$OUTPUT_DIR/${lib}-SKILL.md" "$BAD_DIR/qwen3-coder-${lib}-SKILL.md"
        run_primary "claude-sonnet" "$lib"
    done
fi
echo ""

# ---- Tier 5: Comparison runs (variety data) ----
echo "=== Tier 5: Comparison runs ==="
for lib in click arrow httpx; do
    run_comparison "claude-haiku" "$lib"
done
for lib in click requests flask; do
    run_comparison "gpt41" "$lib"
done
for lib in click arrow; do
    run_comparison "qwen25-14b" "$lib"
done
for lib in click requests; do
    run_comparison "gpt4o" "$lib"
done
echo ""

# ---- Final summary ----
echo "" >> "$SUMMARY"
echo "## Completed: $(date)" >> "$SUMMARY"
echo "" >> "$SUMMARY"

# Show generated_with for all primary outputs
echo "## generated_with in final skills:" >> "$SUMMARY"
echo '```' >> "$SUMMARY"
grep "generated_with:" "$OUTPUT_DIR"/*-SKILL.md >> "$SUMMARY" 2>/dev/null
echo '```' >> "$SUMMARY"

echo "========================================="
echo "  BATCH COMPLETE: $(date)"
echo "========================================="
echo ""
echo "Summary: $SUMMARY"
echo "Logs:    $LOG_DIR/"
echo "Compare: $COMPARE_DIR/"
echo "Bad:     $BAD_DIR/"
echo ""
cat "$SUMMARY"
