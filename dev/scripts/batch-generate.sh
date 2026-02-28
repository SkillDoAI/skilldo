#!/usr/bin/env bash
# Batch regenerate all 27 example SKILL.md files with latest stable library versions.
# Uses frontier models, 10 retries, to surface bugs.
#
# Usage: ./dev/scripts/batch-generate.sh
# Designed to run unattended overnight.

set +e  # Continue on failure
set -o pipefail  # Propagate failures through pipes

# --- Paths ---
PROJECT_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SKILLDO="$PROJECT_ROOT/target/release/skilldo"
CONFIG_DIR="$PROJECT_ROOT/dev/configs/batch"
OUTPUT_DIR="$PROJECT_ROOT/examples/skills"
BAD_DIR="$PROJECT_ROOT/dev/bad-outputs"
LOG_DIR="/tmp/skilldo-batch/logs"
CLONE_DIR="/tmp/skilldo-repos"
SUMMARY="/tmp/skilldo-batch/summary.md"

mkdir -p "$LOG_DIR" "$BAD_DIR" "$CLONE_DIR"

# --- API Keys (both files are `export VAR=value` format) ---
source ~/.openai
source ~/.anthropic

# --- All 27 libraries with latest stable versions ---
# Format: name|github_org_repo|tag
LIB_NAMES=()
LIB_REPOS=()
LIB_TAGS=()

add_lib() { LIB_NAMES+=("$1"); LIB_REPOS+=("$2"); LIB_TAGS+=("$3"); }

add_lib "aiohttp"        "aio-libs/aiohttp"           "v3.13.3"
add_lib "arrow"          "arrow-py/arrow"              "1.4.0"
add_lib "boto3"          "boto/boto3"                  "1.42.58"
add_lib "celery"         "celery/celery"               "v5.6.2"
add_lib "click"          "pallets/click"               "8.3.1"
add_lib "cryptography"   "pyca/cryptography"           "46.0.5"
add_lib "django"         "django/django"               "6.0.2"
add_lib "fastapi"        "fastapi/fastapi"             "0.133.1"
add_lib "flask"          "pallets/flask"               "3.1.3"
add_lib "httpx"          "encode/httpx"                "0.28.1"
add_lib "jinja2"         "pallets/jinja"               "3.1.6"
add_lib "keras"          "keras-team/keras"            "v3.13.2"
add_lib "matplotlib"     "matplotlib/matplotlib"       "v3.10.8"
add_lib "numpy"          "numpy/numpy"                 "v2.4.2"
add_lib "pandas"         "pandas-dev/pandas"           "v3.0.1"
add_lib "pillow"         "python-pillow/Pillow"        "12.1.1"
add_lib "pydantic"       "pydantic/pydantic"           "v2.12.5"
add_lib "pytest"         "pytest-dev/pytest"           "9.0.2"
add_lib "pytorch"        "pytorch/pytorch"             "v2.10.0"
add_lib "requests"       "psf/requests"                "v2.32.5"
add_lib "rich"           "Textualize/rich"             "v14.3.3"
add_lib "scikit-learn"   "scikit-learn/scikit-learn"   "1.8.0"
add_lib "scipy"          "scipy/scipy"                 "v1.17.1"
add_lib "sqlalchemy"     "sqlalchemy/sqlalchemy"       "rel_2_0_47"
add_lib "transformers"   "huggingface/transformers"    "v5.2.0"
add_lib "typer"          "fastapi/typer"               "0.24.1"
add_lib "unstructured"   "Unstructured-IO/unstructured" "0.21.5"

# --- Model assignments ---
# Spread across frontier models for variety and bug surfacing.
# Ollama models run sequentially (no parallel). Cloud models run sequentially too (rate limits).
#
# Sonnet: large/complex libs where quality matters most
# GPT-4.1: reliable for medium libs
# Codestral: new model, test on smaller libs to evaluate quality
# Qwen3-Coder: known decent local model, test on a few

declare -A MODEL_CONFIGS
# Claude Sonnet — 8 libs
MODEL_CONFIGS[aiohttp]="claude-sonnet"
MODEL_CONFIGS[boto3]="claude-sonnet"
MODEL_CONFIGS[django]="claude-sonnet"
MODEL_CONFIGS[flask]="claude-sonnet"
MODEL_CONFIGS[pandas]="claude-sonnet"
MODEL_CONFIGS[pytorch]="claude-sonnet"
MODEL_CONFIGS[sqlalchemy]="claude-sonnet"
MODEL_CONFIGS[transformers]="claude-sonnet"

# GPT-4.1 — 10 libs
MODEL_CONFIGS[celery]="gpt41"
MODEL_CONFIGS[cryptography]="gpt41"
MODEL_CONFIGS[fastapi]="gpt41"
MODEL_CONFIGS[httpx]="gpt41"
MODEL_CONFIGS[keras]="gpt41"
MODEL_CONFIGS[matplotlib]="gpt41"
MODEL_CONFIGS[numpy]="gpt41"
MODEL_CONFIGS[pydantic]="gpt41"
MODEL_CONFIGS[requests]="gpt41"
MODEL_CONFIGS[scipy]="gpt41"

# Codestral/Qwen3 reassigned to cloud — ollama hangs on concurrent large requests
# See backlog: ollama sends max_tokens=16384 which exceeds context window for large prompts
# and concurrent requests from reqwest cause ollama to stall (0% CPU)
MODEL_CONFIGS[arrow]="claude-sonnet"
MODEL_CONFIGS[click]="gpt41"
MODEL_CONFIGS[pillow]="gpt41"
MODEL_CONFIGS[pytest]="gpt41"
MODEL_CONFIGS[typer]="gpt41"
MODEL_CONFIGS[jinja2]="claude-sonnet"
MODEL_CONFIGS[rich]="claude-sonnet"
MODEL_CONFIGS[scikit-learn]="gpt41"
MODEL_CONFIGS[unstructured]="gpt41"

# ========== FUNCTIONS ==========

log() { echo "[$(date '+%H:%M:%S')] $*"; }

# Pre-load an ollama model with extended keep_alive to prevent mid-generation unload
ollama_preload() {
    local model="$1"
    log "  Pre-loading ollama model: $model (keep_alive=120m)"
    curl -s http://localhost:11434/api/generate \
        -d "{\"model\": \"$model\", \"keep_alive\": \"120m\"}" >/dev/null 2>&1
    sleep 2
}

preflight() {
    echo "=== Pre-flight checks ==="
    local ok=1

    [ -f "$SKILLDO" ] && echo "  OK: Binary" || { echo "  FAIL: Binary not at $SKILLDO"; ok=0; }
    [ -n "$OPENAI_API_KEY" ] && echo "  OK: OpenAI key" || { echo "  FAIL: OPENAI_API_KEY empty"; ok=0; }
    [ -n "$ANTHROPIC_API_KEY" ] && echo "  OK: Anthropic key" || { echo "  FAIL: ANTHROPIC_API_KEY empty"; ok=0; }
    curl -s http://localhost:11434/api/tags >/dev/null 2>&1 && echo "  OK: Ollama running" || echo "  WARN: Ollama not running (local models will fallback to cloud)"
    command -v podman &>/dev/null && echo "  OK: Podman" || echo "  WARN: Podman not found (Agent 5 container validation disabled)"

    [ "$ok" -eq 0 ] && { echo "FATAL: Pre-flight failed"; exit 1; }
    echo ""
}

clone_repos() {
    echo "=== Cloning repos ==="
    local i=0
    for name in "${LIB_NAMES[@]}"; do
        local repo="${LIB_REPOS[$i]}"
        local tag="${LIB_TAGS[$i]}"
        local dest="$CLONE_DIR/$name"

        if [ -d "$dest" ]; then
            log "  Skip: $name (exists)"
        else
            log "  Cloning $name@$tag..."
            if git clone --branch "$tag" --depth 1 "https://github.com/$repo.git" "$dest" 2>/dev/null; then
                log "  OK: $name $tag"
            else
                log "  FAIL: $name clone failed"
            fi
        fi
        i=$((i + 1))
    done
    echo ""
}

# Run one generation
# Args: lib_name config_key output_path
run_one() {
    local lib="$1"
    local config_key="$2"
    local out="$3"
    local config_file="$CONFIG_DIR/${config_key}.toml"
    local log_file="$LOG_DIR/${config_key}-${lib}.log"
    local start
    start=$(date +%s)

    mkdir -p "$(dirname "$out")"
    log "  $lib <- $config_key"

    cd "$PROJECT_ROOT"
    timeout 3600 "$SKILLDO" generate "$CLONE_DIR/$lib" \
        --language python \
        --config "$config_file" \
        --output "$out" \
        > "$log_file" 2>&1
    local rc=$?

    local dur=$(( $(date +%s) - start ))
    local bytes=0
    [ -f "$out" ] && bytes=$(wc -c < "$out" | tr -d ' ')

    if [ $rc -ne 0 ]; then
        log "  FAIL ($rc) ${dur}s"
        echo "| $lib | $config_key | FAIL | exit=$rc | ${dur}s |" >> "$SUMMARY"
        return 1
    fi

    # Lint check
    if "$SKILLDO" lint "$out" >> "$log_file" 2>&1; then
        log "  PASS (${bytes}b) ${dur}s"
        echo "| $lib | $config_key | PASS | ${bytes}b | ${dur}s |" >> "$SUMMARY"
        return 0
    else
        log "  LINT_FAIL (${bytes}b) ${dur}s"
        echo "| $lib | $config_key | LINT_FAIL | ${bytes}b | ${dur}s |" >> "$SUMMARY"
        [ -f "$out" ] && cp "$out" "$BAD_DIR/${config_key}-${lib}-SKILL.md"
        return 1
    fi
}

# Generate with primary model, fallback to Sonnet on failure
run_with_fallback() {
    local lib="$1"
    local config_key="$2"
    local out="$OUTPUT_DIR/${lib}-SKILL.md"

    if run_one "$lib" "$config_key" "$out"; then
        return 0
    fi

    # Save bad output before fallback
    [ -f "$out" ] && cp "$out" "$BAD_DIR/${config_key}-${lib}-SKILL.md" 2>/dev/null

    # Fallback to a different cloud model
    if [ "$config_key" = "claude-sonnet" ]; then
        log "  Fallback: trying gpt41..."
        run_one "$lib" "gpt41" "$out"
    else
        log "  Fallback: trying claude-sonnet..."
        run_one "$lib" "claude-sonnet" "$out"
    fi
}

# ========== MAIN ==========

echo "========================================="
echo "  Skilldo Batch Generation (v2)"
echo "  Started: $(date)"
echo "  Libraries: ${#LIB_NAMES[@]}"
echo "  Max retries: 10"
echo "========================================="
echo ""

# Build first so preflight can verify the binary
echo "=== Building release binary ==="
cd "$PROJECT_ROOT" || exit 1
if ! cargo build --release 2>&1 | tail -3; then
    echo "FATAL: cargo build --release failed"
    exit 1
fi
echo ""

preflight

# Clone all repos
clone_repos

# Init summary
cat > "$SUMMARY" << 'HEADER'
# Batch Generation Summary

| Library | Model | Result | Size | Time |
|---------|-------|--------|------|------|
HEADER

PASS=0
FAIL=0
TOTAL=${#LIB_NAMES[@]}

# --- Phase 1: Ollama models (sequential, no parallel) ---
echo "=== Phase 1: Ollama models (sequential) ==="
# Pre-load the first ollama model with extended keep_alive
CURRENT_OLLAMA_MODEL=""
for name in "${LIB_NAMES[@]}"; do
    config="${MODEL_CONFIGS[$name]}"
    # Only process ollama (codestral, qwen3-coder, qwen25-14b) models
    case "$config" in
        codestral|qwen3-coder|qwen25-14b)
            # Swap ollama model if needed (only one fits in VRAM at a time)
            WANT_MODEL="${config}:latest"
            if [ "$CURRENT_OLLAMA_MODEL" != "$WANT_MODEL" ]; then
                # Unload previous model
                if [ -n "$CURRENT_OLLAMA_MODEL" ]; then
                    curl -s http://localhost:11434/api/generate \
                        -d "{\"model\": \"$CURRENT_OLLAMA_MODEL\", \"keep_alive\": 0}" >/dev/null 2>&1
                    sleep 2
                fi
                ollama_preload "$WANT_MODEL"
                CURRENT_OLLAMA_MODEL="$WANT_MODEL"
            fi
            log "[$((PASS + FAIL + 1))/$TOTAL] $name"
            if run_with_fallback "$name" "$config"; then
                PASS=$((PASS + 1))
            else
                FAIL=$((FAIL + 1))
            fi
            ;;
    esac
done
echo ""

# --- Phase 2: Cloud models (sequential) ---
echo "=== Phase 2: Cloud models ==="
for name in "${LIB_NAMES[@]}"; do
    config="${MODEL_CONFIGS[$name]}"
    case "$config" in
        codestral|qwen3-coder|qwen25-14b)
            continue  # Already processed in Phase 1
            ;;
    esac
    log "[$((PASS + FAIL + 1))/$TOTAL] $name"
    if run_with_fallback "$name" "$config"; then
        PASS=$((PASS + 1))
    else
        FAIL=$((FAIL + 1))
    fi
done
echo ""

# --- Phase 3: Local source dir test (Agent 5 functional test) ---
# Test a few libs by pointing at cloned local source to verify local dir + Agent 5 works
echo "=== Phase 3: Local source dir test ==="
LOCAL_TEST_LIBS=("click" "requests" "httpx")
LOCAL_PASS=0
LOCAL_FAIL=0
for lib in "${LOCAL_TEST_LIBS[@]}"; do
    local_out="/tmp/skilldo-batch/local-test/${lib}-SKILL.md"
    log "  Local dir test: $lib"
    if run_one "$lib" "claude-sonnet" "$local_out"; then
        LOCAL_PASS=$((LOCAL_PASS + 1))
    else
        LOCAL_FAIL=$((LOCAL_FAIL + 1))
    fi
done
echo "Local dir tests: $LOCAL_PASS pass, $LOCAL_FAIL fail"
echo "" >> "$SUMMARY"
echo "## Local Source Dir Tests" >> "$SUMMARY"
echo "Pass: $LOCAL_PASS / ${#LOCAL_TEST_LIBS[@]}" >> "$SUMMARY"
echo ""

# --- Phase 4: Final lint sweep ---
echo "=== Phase 4: Final lint sweep ==="
LINT_FAIL=0
for skill in "$OUTPUT_DIR"/*-SKILL.md; do
    name=$(basename "$skill" -SKILL.md)
    if ! "$SKILLDO" lint "$skill" > /dev/null 2>&1; then
        log "  LINT FAIL: $name"
        LINT_FAIL=$((LINT_FAIL + 1))
    fi
done
log "Lint: $((TOTAL - LINT_FAIL)) pass, $LINT_FAIL fail"

# --- Summary ---
{
    echo ""
    echo "## Totals"
    echo "- **Pass**: $PASS / $TOTAL"
    echo "- **Fail**: $FAIL / $TOTAL"
    echo "- **Lint failures**: $LINT_FAIL"
    echo "- **Date**: $(date)"
    echo ""
    echo "## generated_with in final skills"
    echo '```'
    grep "generated_with:" "$OUTPUT_DIR"/*-SKILL.md 2>/dev/null || echo "(none found)"
    echo '```'
} >> "$SUMMARY"

echo ""
echo "========================================="
echo "  BATCH COMPLETE: $(date)"
echo "  Pass: $PASS / $TOTAL"
echo "  Fail: $FAIL / $TOTAL"
echo "  Lint failures: $LINT_FAIL"
echo "========================================="
echo ""
echo "Summary: $SUMMARY"
echo "Logs:    $LOG_DIR/"
echo "Bad:     $BAD_DIR/"
echo ""
cat "$SUMMARY"
