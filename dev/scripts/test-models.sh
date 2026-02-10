#!/bin/bash
# Test skilldo with different models on Django

set -e
export NONE="dummy"
BINARY="/Users/admin/git/techbek/rulesbot/target/debug/skilldo"
DJANGO="/tmp/test-repos/django"

echo "🧪 Testing skilldo with different models on Django..."
echo ""

# Test 1: DeepSeek-Coder-V2:16b (SOTA)
echo "📊 Test 1/3: DeepSeek-Coder-V2:16b (SOTA)"
cd "$DJANGO"
cp /Users/admin/git/techbek/rulesbot/skilldo-deepseek.toml ./skilldo.toml
timeout 600 "$BINARY" generate . --language python --output /Users/admin/git/techbek/rulesbot/test-outputs/django-deepseek.md 2>&1 | tee /tmp/deepseek-test.log
echo "✅ DeepSeek complete"
echo ""

# Test 2: Yi-Coder:9b (Fast Python)
echo "📊 Test 2/3: Yi-Coder:9b (Fast Python specialist)"
cp /Users/admin/git/techbek/rulesbot/skilldo-yi.toml ./skilldo.toml
timeout 600 "$BINARY" generate . --language python --output /Users/admin/git/techbek/rulesbot/test-outputs/django-yi.md 2>&1 | tee /tmp/yi-test.log
echo "✅ Yi-Coder complete"
echo ""

# Test 3: Qwen:7b (Baseline)
echo "📊 Test 3/3: Qwen2.5-Coder:7b (Baseline)"
cp /Users/admin/git/techbek/rulesbot/skilldo-ollama.toml ./skilldo.toml
timeout 600 "$BINARY" generate . --language python --output /Users/admin/git/techbek/rulesbot/test-outputs/django-qwen7b.md 2>&1 | tee /tmp/qwen7b-test.log
echo "✅ Qwen 7b complete"
echo ""

echo "🎉 All tests complete!"
echo ""
echo "Results:"
echo "  DeepSeek: $(wc -l < /Users/admin/git/techbek/rulesbot/test-outputs/django-deepseek.md) lines"
echo "  Yi-Coder: $(wc -l < /Users/admin/git/techbek/rulesbot/test-outputs/django-yi.md) lines"
echo "  Qwen 7b:  $(wc -l < /Users/admin/git/techbek/rulesbot/test-outputs/django-qwen7b.md) lines"
echo ""
echo "Check outputs in test-outputs/"
