#!/bin/bash
# Batch generate SKILL.md for top 25 Python libraries
# Runs: 11 new libraries (14 already exist in examples/skills/)
set -eo pipefail

SKILLDO="./target/release/skilldo"
CONFIG="./dev/configs/gpt52-config.toml"
REPOS="/tmp/skilldo-repos"
OUTPUT_DIR="./examples/skills"
LOG_DIR="/tmp/skilldo-batch/logs"

# Load API keys
source ~/.openai
export ANTHROPIC_API_KEY=$(cat ~/.anthropic)

mkdir -p "$LOG_DIR" "$OUTPUT_DIR"

# Clone repos we don't have yet
clone_if_missing() {
    local name=$1 url=$2
    if [ ! -d "$REPOS/$name" ]; then
        echo "$(date '+%H:%M:%S') Cloning $name..."
        git clone --depth 1 "$url" "$REPOS/$name" 2>/dev/null
    fi
}

clone_if_missing numpy   "https://github.com/numpy/numpy.git"
clone_if_missing celery  "https://github.com/celery/celery.git"
clone_if_missing aiohttp "https://github.com/aio-libs/aiohttp.git"
clone_if_missing pillow  "https://github.com/python-pillow/Pillow.git"
clone_if_missing boto3   "https://github.com/boto/boto3.git"
clone_if_missing jinja2  "https://github.com/pallets/jinja.git"

# Libraries to generate (11 new — the other 14 already exist)
LIBS="numpy django fastapi httpx celery pytest aiohttp pillow boto3 jinja2 typer"
TOTAL=11
PASS=0
FAIL=0
NUM=0

echo ""
echo "=========================================="
echo " Skilldo Batch Generation — Top 25"
echo " Config: gpt-5.2 (all agents)"
echo " Libraries: $TOTAL new"
echo " Started: $(date)"
echo "=========================================="
echo ""

for lib in $LIBS; do
    NUM=$((NUM + 1))
    log="$LOG_DIR/${lib}.log"
    out="$OUTPUT_DIR/${lib}-SKILL.md"

    echo "[$NUM/$TOTAL] Generating $lib..."
    START=$(date +%s)

    if $SKILLDO generate "$REPOS/$lib" \
        --language python \
        --config "$CONFIG" \
        -o "$out" \
        > "$log" 2>&1; then
        END=$(date +%s)
        ELAPSED=$(( END - START ))
        echo "  ✓ $lib — ${ELAPSED}s — $out"
        PASS=$((PASS + 1))
    else
        END=$(date +%s)
        ELAPSED=$(( END - START ))
        echo "  ✗ $lib — ${ELAPSED}s — see $log"
        FAIL=$((FAIL + 1))
    fi
done

echo ""
echo "=========================================="
echo " Batch Complete: $(date)"
echo " Pass: $PASS / $TOTAL"
echo " Fail: $FAIL / $TOTAL"
echo "=========================================="

echo ""
echo "Skills in $OUTPUT_DIR:"
ls -1 "$OUTPUT_DIR"/*-SKILL.md 2>/dev/null | wc -l | tr -d ' '
echo " files total"
