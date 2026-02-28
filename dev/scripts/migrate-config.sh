#!/usr/bin/env bash
# Migrate old skilldo config files to v0.1.5+ naming.
# Serde aliases for old names will be removed in v0.3.0 — delete this script then.
#
# Old names still work (serde aliases), but this script
# updates configs to use the canonical field names.
#
# Usage: ./dev/scripts/migrate-config.sh path/to/config.toml
#        ./dev/scripts/migrate-config.sh path/to/config.toml --dry-run
set -euo pipefail

if [ $# -lt 1 ]; then
    echo "Usage: $0 <config.toml> [--dry-run]"
    exit 1
fi

FILE="$1"
DRY_RUN=false
[ "${2:-}" = "--dry-run" ] && DRY_RUN=true

if [ ! -f "$FILE" ]; then
    echo "File not found: $FILE"
    exit 1
fi

# All old → new field name mappings
RENAMES=(
    # enable/mode flags
    "enable_agent5=enable_test"
    "agent5_mode=test_mode"
    # per-agent LLM sections
    "agent1_llm=extract_llm"
    "agent2_llm=map_llm"
    "agent3_llm=learn_llm"
    "agent4_llm=create_llm"
    "agent5_llm=test_llm"
    # per-agent mode overrides
    "agent1_mode=extract_mode"
    "agent2_mode=map_mode"
    "agent3_mode=learn_mode"
    "agent4_mode=create_mode"
    # per-agent custom prompts
    "agent1_custom=extract_custom"
    "agent2_custom=map_custom"
    "agent3_custom=learn_custom"
    "agent4_custom=create_custom"
    "agent5_custom=test_custom"
)

count=0
for pair in "${RENAMES[@]}"; do
    old="${pair%%=*}"
    new="${pair#*=}"
    if grep -q "$old" "$FILE"; then
        count=$((count + 1))
        if $DRY_RUN; then
            echo "  would rename: $old -> $new"
        else
            sed -i.bak "s/$old/$new/g" "$FILE"
        fi
    fi
done

# Clean up sed backup file
if ! $DRY_RUN && [ -f "${FILE}.bak" ]; then
    rm "${FILE}.bak"
fi

if [ "$count" -eq 0 ]; then
    echo "No old field names found in $FILE — already up to date."
else
    if $DRY_RUN; then
        echo "$count rename(s) would be applied. Run without --dry-run to apply."
    else
        echo "$count rename(s) applied to $FILE."
    fi
fi
