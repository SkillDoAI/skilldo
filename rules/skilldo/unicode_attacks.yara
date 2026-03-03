// SKILL.md Unicode Attack Detection Rules
//
// Author: SkillDo (https://github.com/SkilldoAI/skilldo)
// License: Apache-2.0
// Description: Detects unicode-based attacks in AI agent skill files
//   including invisible characters, bidirectional overrides, and
//   tag character steganography.
//
// Based on the Trojan Source paper (Boucher & Anderson, 2021),
// Unicode Technical Report #36, and the os-info-checker-es6 attack (2025).
//
// NOTE: Homoglyph detection (SD-001) and mixed-script analysis (SD-005)
// require character-level Unicode analysis beyond YARA's capabilities.
// These remain in the Rust scanning layer.

rule SD_002_invisible_unicode
{
    meta:
        id = "SD-002"
        severity = "high"
        category = "unicode-attack"
        description = "Invisible Unicode characters that can hide instructions between visible text"
        reference = "Unicode TR#36 - Security Considerations"

    strings:
        // Zero-width characters (UTF-8 encoded)
        $zwsp = { E2 80 8B }        // U+200B zero-width space
        $zwnj = { E2 80 8C }        // U+200C zero-width non-joiner
        $zwj  = { E2 80 8D }        // U+200D zero-width joiner
        $bom  = { EF BB BF }        // U+FEFF byte order mark
        $shy  = { C2 AD }           // U+00AD soft hyphen
        $wj   = { E2 81 A0 }        // U+2060 word joiner
        $fa   = { E2 81 A1 }        // U+2061 function application
        $it   = { E2 81 A2 }        // U+2062 invisible times
        $is   = { E2 81 A3 }        // U+2063 invisible separator
        $ip   = { E2 81 A4 }        // U+2064 invisible plus
        $mvs  = { E1 A0 8E }        // U+180E mongolian vowel separator

    condition:
        any of them
}

rule SD_003_bidi_override
{
    meta:
        id = "SD-003"
        severity = "critical"
        category = "unicode-attack"
        description = "Bidirectional control characters — Trojan Source attack vector"
        reference = "Boucher & Anderson, 2021 - Trojan Source"

    strings:
        // Bidi control characters (UTF-8 encoded)
        $lrm  = { E2 80 8E }        // U+200E left-to-right mark
        $rlm  = { E2 80 8F }        // U+200F right-to-left mark
        $lre  = { E2 80 AA }        // U+202A left-to-right embedding
        $rle  = { E2 80 AB }        // U+202B right-to-left embedding
        $pdf  = { E2 80 AC }        // U+202C pop directional formatting
        $lro  = { E2 80 AD }        // U+202D left-to-right override
        $rlo  = { E2 80 AE }        // U+202E right-to-left override
        $lri  = { E2 81 A6 }        // U+2066 left-to-right isolate
        $rli  = { E2 81 A7 }        // U+2067 right-to-left isolate
        $fsi  = { E2 81 A8 }        // U+2068 first strong isolate
        $pdi  = { E2 81 A9 }        // U+2069 pop directional isolate

    condition:
        any of them
}

rule SD_006_tag_steganography
{
    meta:
        id = "SD-006"
        severity = "critical"
        category = "unicode-attack"
        description = "Unicode tag characters or variation selectors used for steganographic payload encoding"
        reference = "os-info-checker-es6 malware (2025), Unicode Supplemental plane abuse"

    strings:
        // Tag characters U+E0001-U+E007F (UTF-8: F3 A0 80 81 to F3 A0 81 BF)
        // These encode ASCII values in Unicode supplemental plane
        $tag_lang = { F3 A0 80 81 }           // U+E0001 language tag
        $tag_a    = { F3 A0 81 [1] }          // U+E0041-U+E007F tag chars
        $tag_low  = { F3 A0 80 [1] }          // U+E0000-U+E003F tag range

        // Variation selectors supplement U+E0100-U+E01EF (UTF-8: F3 A0 84 80+)
        $vs_supp  = { F3 A0 84 [1] }          // Variation selector range

    condition:
        any of them
}
