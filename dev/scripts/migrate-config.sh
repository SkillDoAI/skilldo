#!/usr/bin/env bash
# Migrate old skilldo config files to current naming.
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
    # v0.2.3: provider → provider_type (provider still works as serde alias)
    # Only rename bare "provider" at key position, not inside section headers or values
)

# Special handling: rename "provider = " to "provider_type = " (word-boundary safe)
# This avoids matching "provider" inside "provider_name" or other compound names.
PROVIDER_RENAMES=(
    's/^provider = /provider_type = /g'
    's/^provider=/provider_type=/g'
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

# v0.2.3: rename bare "provider = " → "provider_type = "
for pattern in "${PROVIDER_RENAMES[@]}"; do
    if grep -qE '^provider[= ]' "$FILE" && ! grep -q 'provider_type' "$FILE"; then
        count=$((count + 1))
        if $DRY_RUN; then
            echo "  would rename: provider -> provider_type"
        else
            sed -i.bak "$pattern" "$FILE"
        fi
        break  # only apply once
    fi
done

# v0.2.3: if provider_type exists but provider_name doesn't, add provider_name.
# Only backfill when exactly one provider_type line exists — multi-section configs
# (e.g., [llm] + [extract_llm]) would get the wrong value otherwise.
# In dry-run mode, provider→provider_type rename hasn't happened yet, so also check
# for bare "provider" to give an accurate preview.
EFFECTIVE_HAS_PTYPE=false
grep -q 'provider_type' "$FILE" && EFFECTIVE_HAS_PTYPE=true
$DRY_RUN && grep -qE '^provider[= ]' "$FILE" && EFFECTIVE_HAS_PTYPE=true

PTYPE_COUNT=$(grep -c 'provider_type' "$FILE" || true)
# In dry-run, count bare "provider" lines too (they would become provider_type)
$DRY_RUN && PTYPE_COUNT=$((PTYPE_COUNT + $(grep -cE '^provider[= ]' "$FILE" || true)))

if $EFFECTIVE_HAS_PTYPE && [ "$PTYPE_COUNT" -le 1 ] && ! grep -q 'provider_name' "$FILE"; then
    PTYPE=$(grep -m1 'provider_type' "$FILE" | sed 's/.*= *"\{0,1\}\([^"]*\)"\{0,1\}.*/\1/' | tr -d ' ')
    # In dry-run, provider_type line may not exist yet — fall back to provider line
    if [ -z "$PTYPE" ]; then
        PTYPE=$(grep -m1 -E '^provider[= ]' "$FILE" | sed 's/.*= *"\{0,1\}\([^"]*\)"\{0,1\}.*/\1/' | tr -d ' ')
    fi
    if [ -n "$PTYPE" ]; then
        count=$((count + 1))
        if $DRY_RUN; then
            echo "  would add: provider_name = \"$PTYPE\""
        else
            sed -i.bak "/provider_type/a\\
provider_name = \"$PTYPE\"" "$FILE"
        fi
    fi
fi

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
