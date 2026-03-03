// SKILL.md Prompt Injection Detection Rules
//
// Author: SkillDo (https://github.com/SkilldoAI/skilldo)
// License: Apache-2.0
// Description: Detects prompt injection, jailbreak, and manipulation
//   patterns in AI agent skill files. Designed for SKILL.md format
//   but applicable to any AI agent instruction content.
//
// These rules complement Cisco skill-scanner's prompt_injection_generic.yara
// by adding patterns specific to the SKILL.md ecosystem.

rule SD_101_system_tag_injection
{
    meta:
        id = "SD-101"
        severity = "critical"
        category = "prompt-injection"
        description = "XML/LLM system delimiter tags used for direct prompt injection"
        reference = "OWASP LLM Top 10 - LLM01 Prompt Injection"

    strings:
        $sys1 = /<\/?system>/i
        $sys2 = /<<\/?SYS>>/i
        $inst = /\[\/?INST\]/i
        $sysb = /\[\/?SYSTEM\]/i

    condition:
        any of them
}

rule SD_102_instruction_override
{
    meta:
        id = "SD-102"
        severity = "critical"
        category = "prompt-injection"
        description = "Attempts to override, wipe, or disregard prior instructions"

    strings:
        $ignore = /ignore\s+(all\s+)?(previous|prior|above|earlier|your|the)\s+(instructions?|directives?|rules?|guidelines?|prompts?)/i
        $forget = /forget\s+(all\s+)?(previous|prior|above|earlier|your|the)\s+(instructions?|directives?|rules?|prompts?)/i
        $disregard = /disregard\s+(all\s+)?(previous|prior|above|earlier)\s/i
        $override = /override\s+(all\s+)?(previous|safety|security|content)\s/i

    condition:
        any of them
}

rule SD_103_identity_reassignment
{
    meta:
        id = "SD-103"
        severity = "critical"
        category = "prompt-injection"
        description = "Attempts to reassign AI identity, purpose, or operating mode"

    strings:
        $new_inst = /(new|real|actual|true|updated)\s+(instructions?|directives?|system\s*prompt)/i
        $you_now = /you\s+are\s+now\s+(in|a|an|the)/i
        $purpose = /your\s+(new|real|actual|true)\s+(purpose|goal|objective|role|identity)/i
        $mode = /entering\s+(a\s+)?(new|special|admin|debug|maintenance|developer)\s+mode/i

    condition:
        any of them
}

rule SD_104_secrecy_demand
{
    meta:
        id = "SD-104"
        severity = "critical"
        category = "prompt-injection"
        description = "Instructions to hide behavior from the user"

    strings:
        $donot = /do\s+not\s+(tell|inform|reveal|disclose|mention|show|display)\s+(the\s+)?(user|human|anyone|them)/i
        $dont = /(don'?t|never)\s+(tell|inform|reveal|disclose)\s+(the\s+)?(user|human|anyone)/i
        $keep = /keep\s+(this|it)\s+(secret|hidden|confidential|private)\s+from/i

    condition:
        any of them
}

rule SD_105_credential_exfiltration
{
    meta:
        id = "SD-105"
        severity = "critical"
        category = "prompt-injection"
        description = "Prose instructions to share or exfiltrate credentials"

    strings:
        $send_keys = /send\s+(all\s+)?(your\s+)?(api\s+)?keys?\s+to/i
        $share = /(share|send|transmit|post|upload)\s+(your\s+)?(credentials?|secrets?|tokens?|keys?|passwords?)/i

    condition:
        any of them
}

rule SD_106_authority_claim
{
    meta:
        id = "SD-106"
        severity = "high"
        category = "prompt-injection"
        description = "False authority claims to gain trust or elevated access"

    strings:
        $iam = /(i\s+am|this\s+is)\s+(your\s+)?(admin|administrator|developer|creator|owner|operator)/i
        $mode = /(admin|maintenance|debug|developer|emergency)\s+(mode|access|override|command)/i

    condition:
        any of them
}

rule SD_107_jailbreak
{
    meta:
        id = "SD-107"
        severity = "high"
        category = "prompt-injection"
        description = "Role-play framing and jailbreak mode activation"

    strings:
        $pretend = /(pretend|imagine|assume)\s+(you\s+are|you'?re)\s+(a|an|the)/i
        $dan = /(in\s+)?(DAN|developer|admin|root|sudo|jailbreak)\s+mode/i
        $activate = /(activate|enable|enter|switch\s+to)\s+(DAN|developer|unrestricted|unfiltered)\s+mode/i

    condition:
        any of them
}

rule SD_108_manipulative_language
{
    meta:
        id = "SD-108"
        severity = "high"
        category = "prompt-injection"
        description = "Coercion, trust manipulation, and conspiracy framing"

    strings:
        $threat = /(if\s+you\s+don'?t|unless\s+you)\s+(do\s+this|comply|follow|obey)/i
        $trust = /(you\s+(have|need)\s+to|you\s+must)\s+(trust\s+me|believe\s+me|do\s+(as|what)\s+I\s+say)/i
        $secret = /(between\s+us|our\s+(little\s+)?secret|nobody\s+(needs?\s+to|has\s+to|will)\s+know)/i

    condition:
        any of them
}

rule SD_109_urgent_execution
{
    meta:
        id = "SD-109"
        severity = "high"
        category = "prompt-injection"
        description = "Urgent execution demands to bypass review"

    strings:
        $exec = /(execute|run|perform)\s+(this\s+)?(immediately|now|right\s+away|at\s+once)/i

    condition:
        any of them
}

rule SD_113_indirect_injection
{
    meta:
        id = "SD-113"
        severity = "critical"
        category = "prompt-injection"
        description = "Indirect prompt injection — redirects to external instruction source"

    strings:
        $follow = /(follow|execute|obey|read)\s+(the\s+)?instructions?\s+(from|at|in)\s+(the\s+)?(url|link|webpage|website|file)/i
        $fetch = /(fetch|download|load|read)\s+(and\s+)?(execute|run|follow|obey)\s+(the\s+)?(code|instructions?|commands?)\s+(from|at)/i
        $real = /(the\s+)?(real|actual|true)\s+(instructions?|code|commands?)\s+(are|is)\s+(at|in|on)\s+https?:\/\//i

    condition:
        any of them
}
