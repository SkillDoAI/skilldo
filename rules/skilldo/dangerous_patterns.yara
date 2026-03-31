// SKILL.md Dangerous Code Pattern Detection Rules
//
// Author: SkillDo (https://github.com/SkilldoAI/skilldo)
// License: Apache-2.0
// Description: Detects dangerous code patterns in AI agent skill files
//   including code execution, credential access, data exfiltration,
//   obfuscation, persistence, privilege escalation, and resource abuse.
//
// These rules complement Cisco skill-scanner's code_execution_generic.yara
// and command_injection_generic.yara. Unique contributions include
// network exfiltration code detection, SQL injection patterns,
// resource abuse / DoS detection, and binary content flagging.

rule SD_201_dynamic_code_execution
{
    meta:
        id = "SD-201"
        severity = "critical"
        category = "code-execution"
        description = "Dynamic code execution via eval, exec, or deserialization"
        reference = "MITRE ATT&CK T1059"
        prose_only = true

    strings:
        $eval = /\beval\s*\(/
        $execSync = /\bexecSync\s*\(/
        $spawnSync = /\bspawnSync\s*\(/
        $funcCtor = /new\s+Function\s*\(/
        $child_proc = /require\s*\(\s*['"]child_process['"]\s*\)/
        $subprocess = /\bsubprocess\.\w+\s*\(/
        $os_system = /\bos\.system\s*\(/
        $os_popen = /\bos\.popen\s*\(/
        $dyn_import = /__import__\s*\(/
        $pickle_load = /\bpickle\.loads?\s*\(/

    condition:
        any of them
}

rule SD_202_credential_file_access
{
    meta:
        id = "SD-202"
        severity = "high"
        category = "credential-access"
        description = "Access to credential stores, key files, or sensitive config"
        reference = "MITRE ATT&CK T1552"
        prose_only = true

    strings:
        $ssh = ".ssh/"
        $aws = ".aws/"
        $gnupg = ".gnupg/"
        $auth = "auth-profiles.json"
        $creds = "credentials.json"
        $wallet = "wallet.dat"
        $seed = /seed[_\-]?phrase/
        $privkey = /private[_\-]?key/
        $keychain = "keychain"
        $shadow = "/etc/shadow"
        $sudoers = "/etc/sudoers"

    condition:
        any of them
}

rule SD_203_obfuscation
{
    meta:
        id = "SD-203"
        severity = "critical"
        category = "obfuscation"
        description = "Code obfuscation techniques — base64, char construction, encoding"

    strings:
        $atob = /\batob\s*\(/
        $btoa = /\bbtoa\s*\(/
        $buffer = /Buffer\.from\s*\([^)]*base64/
        $charCode = "fromCharCode"
        $b64dec = "base64.b64decode"
        $b64bytes = "base64.decodebytes"

    condition:
        any of them
}

rule SD_204_persistence
{
    meta:
        id = "SD-204"
        severity = "high"
        category = "persistence"
        description = "System persistence mechanisms — cron, systemd, shell rc files"
        reference = "MITRE ATT&CK T1053, T1546"
        prose_only = true

    strings:
        $crontab = /\bcrontab\b/
        $systemctl = /\bsystemctl\b/
        $systemd = /\bsystemd\b/
        $bashrc = ".bashrc"
        $zshrc = ".zshrc"
        $profile = ".profile"
        $rclocal = /\brc\.local\b/
        $launchd = /\blaunchd\b/
        $launchagent = /\bLaunchAgent\b/

    condition:
        any of them
}

rule SD_205_privilege_escalation
{
    meta:
        id = "SD-205"
        severity = "critical"
        category = "privilege-escalation"
        description = "Privilege escalation — sudo, setuid, NOPASSWD"
        reference = "MITRE ATT&CK T1548"
        prose_only = true

    strings:
        $sudo = /\bsudo\b/
        $setuid_bit = /\bchmod\s+\+s\b/
        $setuid = /\bsetuid\b/
        $setgid = /\bsetgid\b/
        $nopasswd = /\bNOPASSWD\b/

    condition:
        any of them
}

rule SD_206_reverse_shell
{
    meta:
        id = "SD-206"
        severity = "high"
        category = "data-exfiltration"
        description = "Reverse shell, network backdoor, or pipe-to-shell"
        reference = "MITRE ATT&CK T1059.004"

    strings:
        $devtcp = "/dev/tcp/"
        $nc = /\bnc\s+\-[elp]/
        $netcat = /\bnetcat\b/
        $ncat = /\bncat\b/
        $curl_sh = /curl\s+.*\|\s*(ba)?sh/
        $wget_sh = /wget\s+.*\|\s*(ba)?sh/
        $ngrok = /\bngrok\b/

    condition:
        any of them
}

rule SD_207_hardcoded_secret
{
    meta:
        id = "SD-207"
        severity = "critical"
        category = "credential-access"
        description = "Hardcoded API keys, tokens, or secrets in skill content"

    strings:
        $aws = /(AKIA|AGPA|AIDA|AROA|AIPA|ANPA|ANVA|ASIA)[0-9A-Z]{16}/
        $stripe = /sk_(live|test)_[0-9a-zA-Z]{24,}/
        $google = /AIza[0-9A-Za-z\-_]{35}/
        $github = /gh[pousr]_[0-9a-zA-Z]{36,}/
        $slack = /xox[baprs]\-[0-9a-zA-Z\-]{10,}/
        $pem = /\-\-\-\-\-BEGIN\s+(RSA|EC|DSA|OPENSSH)\s+PRIVATE\s+KEY\-\-\-\-\-/
        $openai = /sk\-[A-Za-z0-9]{32,}/

    condition:
        any of them
}

rule SD_208_sql_injection
{
    meta:
        id = "SD-208"
        severity = "high"
        category = "code-execution"
        description = "SQL injection patterns — tautology, UNION, blind, destructive"

    strings:
        $tautology = /OR\s+['"]?1['"]?\s*=\s*['"]?1/i
        $union = /UNION\s+(ALL\s+)?SELECT\s/i
        $drop = /;\s*DROP\s+TABLE\s/i
        $delete = /;\s*DELETE\s+FROM\s/i
        $sleep = /SLEEP\s*\(\s*\d+\s*\)/i
        $waitfor = /WAITFOR\s+DELAY\s/i
        $extract = /EXTRACTVALUE\s*\(/i

    condition:
        any of them
}

rule SD_209_network_exfiltration
{
    meta:
        id = "SD-209"
        severity = "high"
        category = "data-exfiltration"
        description = "Network exfiltration code — HTTP POST, socket, fetch to external URLs"
        prose_only = true

    strings:
        $req_post = /requests\.post\s*\(/i
        $urllib = /urllib\.request\.urlopen\s*\(/i
        $httpclient = /http\.client\.\w+\s*\(/i
        $socket = /socket\.connect\s*\(/i
        $fetch = /fetch\s*\(\s*['"]https?:\/\//i
        $axios = /axios\.post\s*\(/i
        $xhr = /new\s+XMLHttpRequest\s*\(/i

    condition:
        any of them
}

rule SD_210_resource_abuse
{
    meta:
        id = "SD-210"
        severity = "high"
        category = "resource-abuse"
        description = "Resource abuse / denial-of-service — infinite loops, fork bombs, large allocs"
        prose_only = true

    strings:
        $while_true_py = /while\s+(True|true|1)\s*:/
        $for_inf_c = /for\s*\(\s*;\s*;\s*\)/
        $fork = /\bos\.fork\s*\(/
        $fork_bomb = /:\(\)\{.*\|.*:;/
        $itertools = /itertools\.count\s*\(/i

    condition:
        any of them
}

rule SD_211_binary_content
{
    meta:
        id = "SD-211"
        severity = "critical"
        category = "obfuscation"
        description = "Actual binary/executable bytes embedded in skill file (ELF or PE headers)"

    strings:
        $elf = { 7F 45 4C 46 }
        $pe = { 4D 5A 90 00 }

    condition:
        any of them
}

rule SD_212_executable_extension_reference
{
    meta:
        id = "SD-212"
        severity = "high"
        category = "obfuscation"
        description = "References to executable file extensions in prose"
        prose_only = true

    strings:
        $exe_ext = /\.(exe|dll|so|dylib|scr|bat|cmd|ps1|vbs|wsf)\b/i

    condition:
        any of them
}
