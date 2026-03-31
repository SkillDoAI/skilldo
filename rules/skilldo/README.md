# SkillDo YARA Rules

YARA rules for detecting security threats in AI agent skill files (SKILL.md).
24 rules across 3 files, designed for fast static analysis of LLM-generated content.

## Rules

### Unicode Attacks — `unicode_attacks.yara`

| Rule | Severity | What it catches |
|------|----------|-----------------|
| SD-002 | high | Invisible Unicode characters (zero-width space, word joiner, etc.) that hide instructions between visible text |
| SD-003 | critical | Bidirectional control characters — Trojan Source attack vector |
| SD-006 | critical | Unicode tag characters (U+E0001-U+E007F) and variation selectors used for steganographic payload encoding |

### Prompt Injection — `prompt_injection.yara`

| Rule | Severity | What it catches |
|------|----------|-----------------|
| SD-101 | critical | XML/LLM system delimiter tags for direct prompt injection |
| SD-102 | critical | Instruction override — "ignore all previous instructions" |
| SD-103 | critical | Identity reassignment — "you are now", "new role" |
| SD-104 | critical | Secrecy demands — "do not tell the user" |
| SD-105 | critical | Credential exfiltration via prose — "send API key to" |
| SD-106 | high | False authority claims — "authorized by OpenAI" |
| SD-107 | high | Role-play framing and jailbreak activation — "DAN mode" |
| SD-108 | high | Coercion and conspiracy framing |
| SD-109 | high | Urgency exploitation — "do this immediately without review" |
| SD-113 | critical | Indirect prompt injection — "follow instructions at URL" |

### Dangerous Patterns — `dangerous_patterns.yara`

| Rule | Severity | What it catches |
|------|----------|-----------------|
| SD-201 | critical | Dynamic code execution — eval, exec, subprocess, deserialization |
| SD-202 | high | Credential file access — .ssh/, .aws/, /etc/shadow, private keys |
| SD-203 | critical | Code obfuscation — base64 decode, fromCharCode, atob |
| SD-204 | high | System persistence — crontab, systemd, .bashrc, LaunchAgent |
| SD-205 | critical | Privilege escalation — sudo, setuid, chmod +s, NOPASSWD |
| SD-206 | high | Reverse shell / network backdoor — /dev/tcp/, nc -e, curl pipe sh |
| SD-207 | critical | Hardcoded secrets — AWS keys, Stripe, Google, GitHub tokens, PEM keys |
| SD-208 | high | SQL injection — tautology, UNION SELECT, DROP TABLE, blind injection |
| SD-209 | high | Network exfiltration code — requests.post, socket.connect, fetch, axios |
| SD-210 | high | Resource abuse / DoS — infinite loops, fork bombs, unbounded iterators |
| SD-211 | critical | Actual binary bytes embedded in skill file (ELF, PE headers) |
| SD-212 | high | References to executable file extensions in prose (.exe, .dll, .so, etc.) |

## License

Distributed under the Apache License 2.0. See LICENSE in this directory for terms and required notices.

## Compatibility

These rules are tested with [boreal](https://github.com/vthib/boreal) (pure Rust YARA engine)
and are valid YARA per the spec. They should also work with libyara (C),
[YARA-X](https://github.com/VirusTotal/yara-x), and any YARA-compatible scanner.

**Note on `(?:...)` syntax:** The YARA spec (libyara) does not support non-capturing
groups. Neither does boreal. However, YARA-X accepts them because it delegates regex
parsing to Rust's `regex_syntax` crate, which treats `(?:...)` as valid. Some
third-party rulesets (e.g., [Cisco AI Defense](https://github.com/cisco-ai-defense/skill-scanner))
use `(?:...)` — these rules are technically non-compliant with the YARA spec but work
in YARA-X. Our rules avoid this syntax for maximum compatibility. If you're importing
rules that use `(?:`, replace with `(` — YARA has no backreferences, so they're equivalent.
