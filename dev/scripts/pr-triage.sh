#!/usr/bin/env bash
# PR Triage — gather all open threads, bot findings, and nits for review.
#
# Usage: ./dev/scripts/pr-triage.sh <pr> [repo]
#
# Outputs a structured report with:
#   1. Open (unresolved) review threads
#   2. Greptile summary findings (from latest review)
#   3. CodeRabbit actionable/nitpick comments (from latest review)
#   4. CI status
#   5. Greptile confidence score
#
# This is the "what needs attention" view before merge.

set -euo pipefail

PR="${1:?Usage: pr-triage.sh <pr> [repo]}"
REPO="${2:-SkillDoAI/skilldo}"

echo "═══════════════════════════════════════════════════"
echo "  PR #${PR} Triage Report — $(date '+%Y-%m-%d %H:%M')"
echo "═══════════════════════════════════════════════════"
echo ""

# --- Greptile Score ---
SCORE=$(gh api "repos/${REPO}/issues/${PR}/comments" \
    --jq '.[] | select(.user.login == "greptile-apps[bot]") | .body' \
    | grep -o 'Confidence Score: [0-9]/5' | tail -1)
echo "Greptile: ${SCORE:-not reviewed}"

# --- CI Status ---
FAILS=$(gh pr checks "$PR" --repo "$REPO" 2>&1 | grep -c 'fail' || true)
PENDING=$(gh pr checks "$PR" --repo "$REPO" 2>&1 | grep -c 'pending' || true)
echo "CI: ${FAILS} failures, ${PENDING} pending"
echo ""

# --- Open Threads ---
echo "───────────────────────────────────────────────────"
echo "  OPEN THREADS (unresolved)"
echo "───────────────────────────────────────────────────"
THREADS=$(gh api graphql -f query="{ repository(owner: \"$(echo $REPO | cut -d/ -f1)\", name: \"$(echo $REPO | cut -d/ -f2)\") { pullRequest(number: ${PR}) { reviewThreads(first: 50) { nodes { isResolved comments(first: 1) { nodes { author { login } body path line createdAt } } } } } } }" \
    --jq '.data.repository.pullRequest.reviewThreads.nodes[] | select(.isResolved == false) | .comments.nodes[0] | "\(.author.login) | \(.path):\(.line) | \(.createdAt[:10]) | \(.body[:120])"' 2>/dev/null)

if [ -z "$THREADS" ]; then
    echo "  None — all threads resolved ✅"
else
    echo "$THREADS" | while IFS= read -r line; do
        echo "  • $line"
    done
fi
echo ""

# --- Greptile Findings ---
echo "───────────────────────────────────────────────────"
echo "  GREPTILE FINDINGS"
echo "───────────────────────────────────────────────────"
# Inline review comments
GREPTILE_INLINE=$(gh api "repos/${REPO}/pulls/${PR}/comments?per_page=100" \
    --jq '.[] | select(.user.login == "greptile-apps[bot]") | "\(.path):\(.original_line) | \(.body | capture("alt=\"P(?<sev>[0-9])\"") | "P\(.sev)") | \(.body | capture("\\*\\*(?<title>[^*]+)\\*\\*") | .title)"' 2>/dev/null)

if [ -n "$GREPTILE_INLINE" ]; then
    echo "  Inline:"
    echo "$GREPTILE_INLINE" | while IFS= read -r line; do
        echo "    • $line"
    done
fi

# Summary findings (outside diff / "Prompt To Fix All")
GREPTILE_SUMMARY=$(gh api "repos/${REPO}/issues/${PR}/comments" \
    --jq '.[] | select(.user.login == "greptile-apps[bot]") | .body' \
    | grep -A2 'Issue requiring\|One finding\|findings\|P[12]:' \
    | grep -v '^--$' | head -10)

if [ -n "$GREPTILE_SUMMARY" ]; then
    echo "  Summary:"
    echo "$GREPTILE_SUMMARY" | while IFS= read -r line; do
        echo "    $line"
    done
fi

if [ -z "$GREPTILE_INLINE" ] && [ -z "$GREPTILE_SUMMARY" ]; then
    echo "  No findings"
fi
echo ""

# --- CodeRabbit Findings ---
echo "───────────────────────────────────────────────────"
echo "  CODERABBIT FINDINGS (latest review)"
echo "───────────────────────────────────────────────────"
CR_INLINE=$(gh api "repos/${REPO}/pulls/${PR}/comments?per_page=100" \
    --jq '.[] | select(.user.login == "coderabbitai[bot]" and (.in_reply_to_id == null)) | "\(.path):\(.original_line) | \(.body | capture("_🟡 Minor_|_🟠 Major_|_🔴 Critical_") // "info") | \(.body | capture("\\*\\*(?<title>[^*]+)\\*\\*") | .title)"' 2>/dev/null)

if [ -n "$CR_INLINE" ]; then
    echo "  Inline:"
    echo "$CR_INLINE" | while IFS= read -r line; do
        echo "    • $line"
    done
fi

# "Prompt for all review comments" from latest review
CR_PROMPT_ALL=$(gh api "repos/${REPO}/pulls/${PR}/reviews" \
    --jq '[.[] | select(.user.login == "coderabbitai[bot]")] | last | .body' \
    | sed -n '/Prompt for all review comments/,/^<\/details>/p' \
    | grep -v 'Prompt for all\|</details>\|<summary>\|````' \
    | head -30)

if [ -n "$CR_PROMPT_ALL" ]; then
    echo "  Aggregated fix prompts:"
    echo "$CR_PROMPT_ALL" | while IFS= read -r line; do
        echo "    $line"
    done
fi

if [ -z "$CR_INLINE" ] && [ -z "$CR_PROMPT_ALL" ]; then
    echo "  No findings"
fi
echo ""

# --- Last Checked Timestamp ---
TIMESTAMP_FILE="/tmp/.pr-triage-${PR}-last-checked"
LAST_CHECKED=$(cat "$TIMESTAMP_FILE" 2>/dev/null || echo "never")
echo "───────────────────────────────────────────────────"
echo "  Last checked: ${LAST_CHECKED}"
date -u '+%Y-%m-%dT%H:%M:%SZ' > "$TIMESTAMP_FILE"
echo "  Updated to: $(cat "$TIMESTAMP_FILE")"
echo ""

# --- Summary ---
echo "───────────────────────────────────────────────────"
THREAD_COUNT=$(echo "$THREADS" | grep -c '.' 2>/dev/null || echo "0")
echo "  Summary: ${SCORE:-?} | ${THREAD_COUNT} open threads | ${FAILS} CI fails"
echo "═══════════════════════════════════════════════════"
