# Cisco Skill Scanner YARA Rules — Attribution

- **Product**: [Cisco AI Defense — Skill Scanner](https://github.com/cisco-ai-defense/skill-scanner)
- **Source**: https://github.com/cisco-ai-defense/skill-scanner/tree/2.0.0/skill_scanner/data/packs/core/yara
- **Tag**: `2.0.0`
- **License**: Apache License 2.0 (see LICENSE in this directory)
- **Date fetched**: 2026-03-02
- **Rule count**: 14 files, 17 rules

These YARA rules are vendored from Cisco's skill-scanner repository and
compiled into the SkillDo binary at build time via `include_str!()`.

Rules are loaded unmodified. A runtime preprocessor (`patch_for_boreal` in
`src/security/yara.rs`) patches `(?:...)` non-capturing groups to `(...)`
for boreal compatibility — upstream Cisco targets libyara (C).

## Update Checklist

To update from a newer Cisco release:

1. Check https://github.com/cisco-ai-defense/skill-scanner/releases for new tags
2. `curl` each .yara file from the new tag into this directory
3. Update the **Tag** and **Date fetched** above
4. Run `cargo test --lib security::yara` — the `builtin_rules_compile` test
   will catch any new boreal incompatibilities
5. If new `(?:...)` patterns appear, `patch_for_boreal` handles them automatically
