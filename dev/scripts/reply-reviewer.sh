#!/usr/bin/env bash
# Reply to a reviewer bot's comment — in the correct place.
#
# Usage:
#   ./dev/scripts/reply-reviewer.sh <pr> <bot> <search-or-id> <body> [repo]
#
# Arguments:
#   pr            PR number (e.g., 75)
#   bot           Bot login: "coderabbitai" or "greptile-apps"
#   search-or-id  Comment ID (numeric) from pr-triage output, OR regex to search
#   body          Reply text (bot is auto-tagged)
#   repo          Optional repo (default: SkillDoAI/skilldo)
#
# Behavior:
#   - If search-or-id is numeric → reply directly to that comment ID (fastest)
#   - If search-or-id is text → search inline comments for a match → reply in thread
#   - If no match found → post a general PR comment
#   - Always tags the bot so it sees the reply
#
# Examples:
#   ./dev/scripts/reply-reviewer.sh 80 coderabbitai 3004506004 "Fixed in abc123"
#   ./dev/scripts/reply-reviewer.sh 80 greptile-apps "backward scan" "Fixed — forward scan now"
#   ./dev/scripts/reply-reviewer.sh 80 coderabbitai "" "General response to all findings"

set -euo pipefail

PR="${1:?Usage: reply-reviewer.sh <pr> <bot> <search-or-id> <body> [repo]}"
BOT="${2:?Missing bot name (coderabbitai or greptile-apps)}"
SEARCH="${3:?Missing search pattern or comment ID}"
BODY="${4:?Missing reply body}"
REPO="${5:-SkillDoAI/skilldo}"
BOT_LOGIN="${BOT}[bot]"

# Auto-tag the bot if not already tagged
if ! echo "$BODY" | grep -q "@${BOT}"; then
    BODY="@${BOT} ${BODY}"
fi

# If search-or-id is numeric, reply directly by ID
if echo "$SEARCH" | grep -qE '^[0-9]+$'; then
    echo "Replying to comment ${SEARCH}..."
    URL=$(gh api "repos/${REPO}/pulls/${PR}/comments/${SEARCH}/replies" \
        -f body="${BODY}" \
        --jq '.html_url' 2>/dev/null)
    if [ -n "$URL" ]; then
        echo "✓ Replied in thread: ${URL}"
        exit 0
    else
        echo "✗ Failed to reply to comment ${SEARCH} — posting PR comment instead"
    fi
fi

# If search is empty or ID reply failed, post a general PR comment
if [ -z "$SEARCH" ] || echo "$SEARCH" | grep -qE '^[0-9]+$'; then
    gh pr comment "${PR}" --repo "${REPO}" --body "${BODY}"
    echo "✓ Posted PR comment"
    exit 0
fi

# Search for inline review comment matching the pattern
COMMENT_ID=$(gh api --paginate "repos/${REPO}/pulls/${PR}/comments" \
    --jq ".[] | select(.user.login == \"${BOT_LOGIN}\" and (.body | test(\"${SEARCH}\"))) | .id" \
    2>/dev/null | tail -1)

if [ -n "$COMMENT_ID" ]; then
    echo "Found inline comment ${COMMENT_ID} — replying in thread..."
    gh api "repos/${REPO}/pulls/${PR}/comments/${COMMENT_ID}/replies" \
        -f body="${BODY}" \
        --jq '.html_url' 2>/dev/null
    echo "✓ Replied in thread"
else
    echo "No inline comment found — posting PR comment..."
    gh pr comment "${PR}" --repo "${REPO}" --body "${BODY}"
    echo "✓ Posted PR comment (summary/outside-diff finding)"
fi
