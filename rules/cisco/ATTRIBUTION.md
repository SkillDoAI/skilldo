# Cisco Skill Scanner YARA Rules — Attribution

- **Product**: [Cisco AI Defense — Skill Scanner](https://github.com/cisco-ai-defense/skill-scanner)
- **Source**: https://github.com/cisco-ai-defense/skill-scanner/tree/2.0.0/skill_scanner/data/packs/core/yara
- **Tag**: `2.0.0`
- **License**: Apache License 2.0 (see LICENSE in this directory)
- **Date fetched**: 2026-03-02
- **Rule count**: 14 files, 17 rules

These YARA rules are vendored from Cisco's skill-scanner repository and
compiled into the SkillDo binary at build time via `include_str!()`.

A runtime preprocessor (`patch_for_boreal` in `src/security/yara.rs`) patches
`(?:...)` non-capturing groups to `(...)` for boreal compatibility — upstream
Cisco targets YARA-X (which accepts `(?:` via the `regex_syntax` crate).

### SkillDo patches (condition logic)

Six rules have patched `condition:` blocks to fix evasion vulnerabilities
caused by global exclusions. YARA matches are file-level — a global
`not $exclusion` disables the entire rule if the exclusion matches anywhere
in the file, even when unrelated high-confidence patterns also match.

The fix: scope exclusions to medium-confidence patterns only, so
high-confidence detections (actual API keys, destructive SQL, VBScript
shells, etc.) always fire regardless of benign context elsewhere in the file.

| Rule | Patch |
|------|-------|
| `code_execution_generic.yara` | `\%s` → `%s` (invalid escape); `not $security_doc` scoped to medium-confidence patterns |
| `credential_harvesting_generic.yara` | Exclusions scoped to action patterns; actual keys/certs always detected |
| `prompt_injection_generic.yara` | `$tool_injection_commands`, `$shadow_parameters`, `$hidden_behavior`, `$privilege_escalation` bypass doc/test exclusions |
| `script_injection_generic.yara` | `$js_protocol_handler`, `$data_uri_script`, `$vbs_shell`, ANSI attacks bypass framework exclusions |
| `sql_injection_generic.yara` | `$destructive_injections`, `$database_system_objects` always fire; `not $non_sql_sleep` scoped to time-based only |
| `system_manipulation_generic.yara` | `$file_destruction`, `$permission_manipulation`, `$critical_system_write` always fire; exclusions scoped to recursive/process ops |

All string definitions and comments are unmodified from upstream.

### Known upstream issue: `cat` in `$critical_system_write`

In `system_manipulation_generic.yara`, the `$critical_system_write` pattern
includes `cat` alongside actual write commands (`echo`, `tee`, `>>`). This
means `cat /etc/sudoers` (read-only) triggers the same high-confidence
detection as `echo ... >> /etc/sudoers` (actual write). This is an upstream
issue — we leave the string definition unmodified to keep our diff minimal.

## Update Checklist

To update from a newer Cisco release:

1. Check https://github.com/cisco-ai-defense/skill-scanner/releases for new tags
2. `curl` each .yara file from the new tag into this directory
3. Update the **Tag** and **Date fetched** above
4. Run `cargo test --lib security::yara` — the `builtin_rules_compile` test
   will catch any new boreal incompatibilities
5. If new `(?:...)` patterns appear, `patch_for_boreal` handles them automatically
6. **Re-apply condition patches** from the table above. The upstream rules use
   global exclusions that create evasion paths. Check `git log -p -- rules/cisco/`
   for the exact diffs to re-apply. Each patch follows the same pattern: split
   the condition into high-confidence (no exclusions) and medium-confidence
   (scoped exclusions) tiers
