#!/bin/bash
# Overnight Agent 5 Testing Script
# Runs comprehensive tests on multiple packages with GPT-5.2

set -e

export OPENAI_API_KEY=$(cat ~/.openai)

echo "========================================="
echo "  Agent 5 Overnight Testing - GPT-5.2"
echo "========================================="
echo ""
echo "Configuration:"
echo "  - Main LLM: qwen3-coder (Ollama local)"
echo "  - Agent 5: GPT-5.2 (OpenAI)"
echo "  - Runtime: podman"
echo "  - Cleanup: disabled (containers left for inspection)"
echo ""

# Build release version
echo "[1/5] Building release binary..."
cargo build --release

# Test 1: Click (CLI library - good for basic validation)
echo ""
echo "[2/5] Testing Click (CLI library)..."
cd /tmp
rm -rf click
git clone --depth 1 https://github.com/pallets/click.git
cd ~/git/techbek/rulesbot
./target/release/skilldo generate /tmp/click --output /tmp/click-SKILL.md 2>&1 | tee /tmp/click-test.log
echo "✓ Click results in /tmp/click-SKILL.md"

# Test 2: FastAPI (Modern async web framework)
echo ""
echo "[3/5] Testing FastAPI (async web framework)..."
cd /tmp
rm -rf fastapi
git clone --depth 1 https://github.com/tiangolo/fastapi.git
cd ~/git/techbek/rulesbot
./target/release/skilldo generate /tmp/fastapi --output /tmp/fastapi-SKILL.md 2>&1 | tee /tmp/fastapi-test.log
echo "✓ FastAPI results in /tmp/fastapi-SKILL.md"

# Test 3: Requests (HTTP library - very popular)
echo ""
echo "[4/5] Testing Requests (HTTP library)..."
cd /tmp
rm -rf requests
git clone --depth 1 https://github.com/psf/requests.git
cd ~/git/techbek/rulesbot
./target/release/skilldo generate /tmp/requests --output /tmp/requests-SKILL.md 2>&1 | tee /tmp/requests-test.log
echo "✓ Requests results in /tmp/requests-SKILL.md"

# Test 4: PyTorch (Large ML library - complex validation)
echo ""
echo "[5/5] Testing PyTorch (ML library - most complex)..."
cd /tmp
if [ ! -d "pytorch" ]; then
    git clone --depth 1 https://github.com/pytorch/pytorch.git
fi
cd ~/git/techbek/rulesbot
./target/release/skilldo generate /tmp/pytorch --output /tmp/pytorch-SKILL.md 2>&1 | tee /tmp/pytorch-test.log
echo "✓ PyTorch results in /tmp/pytorch-SKILL.md"

# Generate summary
echo ""
echo "========================================="
echo "  OVERNIGHT TESTING COMPLETE"
echo "========================================="
echo ""
echo "Results:"
echo "  Click:    /tmp/click-SKILL.md    (log: /tmp/click-test.log)"
echo "  FastAPI:  /tmp/fastapi-SKILL.md  (log: /tmp/fastapi-test.log)"
echo "  Requests: /tmp/requests-SKILL.md (log: /tmp/requests-test.log)"
echo "  PyTorch:  /tmp/pytorch-SKILL.md  (log: /tmp/pytorch-test.log)"
echo ""
echo "Containers (left running for inspection):"
podman ps -a | grep skilldo-test || echo "  (no containers found)"
echo ""
echo "To inspect a container:"
echo "  podman exec -it <container-id> /bin/sh"
echo "  cat /workspace/test.py"
echo ""
echo "To check Agent 5 pass rates:"
grep -h "Agent 5:" /tmp/*-test.log
echo ""
echo "Done! Check AGENT5_FINAL_REPORT.md for analysis."
