use skilldo::detector::Language;
use skilldo::llm::prompts_v2::{
    create_prompt, create_update_prompt, extract_prompt, learn_prompt, map_prompt,
};
use skilldo::pipeline::collector::{DepSource, StructuredDep};

#[test]
fn test_extract_api_extractor_basic() {
    let prompt = extract_prompt(
        "fastapi",
        "0.100.0",
        "class FastAPI: pass",
        1,
        None,
        false,
        &Language::Python,
    );

    assert!(prompt.contains("fastapi"));
    assert!(prompt.contains("0.100.0"));
    assert!(prompt.contains("class FastAPI: pass"));
}

#[test]
fn test_extract_includes_library_category_instructions() {
    let prompt = extract_prompt(
        "requests",
        "2.31.0",
        "def get(): pass",
        1,
        None,
        false,
        &Language::Python,
    );

    assert!(prompt.contains("library_category"));
    assert!(prompt.contains("web_framework"));
    assert!(prompt.contains("orm"));
    assert!(prompt.contains("cli"));
    assert!(prompt.contains("http_client"));
    assert!(prompt.contains("async_framework"));
    assert!(prompt.contains("testing"));
    assert!(prompt.contains("general"));
}

#[test]
fn test_extract_includes_extraction_requirements() {
    let prompt = extract_prompt("click", "8.1.0", "", 1, None, false, &Language::Python);

    assert!(prompt.contains("Name"));
    assert!(prompt.contains("Type"));
    assert!(prompt.contains("Signature"));
    assert!(prompt.contains("Return Type"));
    assert!(prompt.contains("Module/File"));
    assert!(prompt.contains("Decorator Information"));
    assert!(prompt.contains("Deprecation Status"));
}

#[test]
fn test_extract_includes_signature_handling() {
    let prompt = extract_prompt("django", "4.2.0", "", 1, None, false, &Language::Python);

    assert!(prompt.contains("120 characters"));
    assert!(prompt.contains("signature_truncated"));
    assert!(prompt.contains("optional"));
}

#[test]
fn test_extract_includes_method_type_classification() {
    let prompt = extract_prompt("sqlalchemy", "2.0.0", "", 1, None, false, &Language::Python);

    assert!(prompt.contains("function"));
    assert!(prompt.contains("method"));
    assert!(prompt.contains("classmethod"));
    assert!(prompt.contains("staticmethod"));
    assert!(prompt.contains("property"));
    assert!(prompt.contains("descriptor"));
}

#[test]
fn test_extract_includes_type_hint_handling() {
    let prompt = extract_prompt("pydantic", "2.0.0", "", 1, None, false, &Language::Python);

    assert!(prompt.contains("Type Information"));
    assert!(prompt.contains("type_hints"));
    assert!(prompt.contains("is_optional"));
    assert!(prompt.contains("base_type"));
    assert!(prompt.contains("default_value"));
}

#[test]
fn test_extract_includes_deprecation_tracking() {
    let prompt = extract_prompt("flask", "3.0.0", "", 1, None, false, &Language::Python);

    assert!(prompt.contains("@deprecated"));
    assert!(prompt.contains("DeprecationWarning"));
    assert!(prompt.contains("since_version"));
    assert!(prompt.contains("removal_version"));
    assert!(prompt.contains("replacement"));
}

#[test]
fn test_extract_includes_library_specific_patterns() {
    let prompt = extract_prompt("fastapi", "0.100.0", "", 1, None, false, &Language::Python);

    assert!(prompt.contains("Web Frameworks"));
    assert!(prompt.contains("CLI Tools"));
    assert!(prompt.contains("ORMs"));
    assert!(prompt.contains("HTTP Clients"));
}

#[test]
fn test_extract_excludes_private_apis() {
    let prompt = extract_prompt("package", "1.0.0", "", 1, None, false, &Language::Python);

    // Verify prompt excludes private APIs
    assert!(prompt.contains("starting with `_`"));
    assert!(prompt.contains("`__all__`"));
}

#[test]
fn test_extract_output_format() {
    let prompt = extract_prompt("numpy", "1.24.0", "", 1, None, false, &Language::Python);

    assert!(prompt.contains("Return JSON"));
    assert!(prompt.contains(r#""library_category""#));
    assert!(prompt.contains(r#""apis""#));
    assert!(prompt.contains(r#""name""#));
    assert!(prompt.contains(r#""type""#));
    assert!(prompt.contains(r#""signature""#));
}

#[test]
fn test_map_pattern_extractor_basic() {
    let prompt = map_prompt(
        "pytest",
        "7.4.0",
        "def test_something(): assert True",
        None,
        false,
        &Language::Python,
    );

    assert!(prompt.contains("pytest"));
    assert!(prompt.contains("7.4.0"));
    assert!(prompt.contains("def test_something(): assert True"));
}

#[test]
fn test_map_includes_extraction_requirements() {
    let prompt = map_prompt("click", "8.1.0", "", None, false, &Language::Python);

    assert!(prompt.contains("API Being Tested"));
    assert!(prompt.contains("Setup Code"));
    assert!(prompt.contains("Usage Pattern"));
    assert!(prompt.contains("Assertions"));
    assert!(prompt.contains("Test Infrastructure"));
}

#[test]
fn test_map_includes_test_client_patterns() {
    let prompt = map_prompt("fastapi", "0.100.0", "", None, false, &Language::Python);

    assert!(prompt.contains("TestClient"));
    assert!(prompt.contains("CliRunner"));
    assert!(prompt.contains("pytest.fixture"));
}

#[test]
fn test_map_includes_parametrized_tests() {
    let prompt = map_prompt("package", "1.0.0", "", None, false, &Language::Python);

    assert!(prompt.contains("@pytest.mark.parametrize"));
    assert!(prompt.contains("parameter combinations"));
}

#[test]
fn test_map_includes_async_patterns() {
    let prompt = map_prompt("httpx", "0.24.0", "", None, false, &Language::Python);

    assert!(prompt.contains("Async Patterns"));
    assert!(prompt.contains("async/await"));
    assert!(prompt.contains("mark patterns as async"));
}

#[test]
fn test_map_includes_dependency_injection() {
    let prompt = map_prompt("fastapi", "0.100.0", "", None, false, &Language::Python);

    assert!(prompt.contains("Depends()"));
    assert!(prompt.contains("dependencies are created/injected"));
}

#[test]
fn test_map_includes_error_handling() {
    let prompt = map_prompt("requests", "2.31.0", "", None, false, &Language::Python);

    assert!(prompt.contains("Error Handling"));
    assert!(prompt.contains("expected error responses"));
    assert!(prompt.contains("validation patterns"));
}

#[test]
fn test_map_output_format() {
    let prompt = map_prompt("django", "4.2.0", "", None, false, &Language::Python);

    assert!(prompt.contains("Return JSON"));
    assert!(prompt.contains("pattern"));
    assert!(prompt.contains("Setup Code"));
    assert!(prompt.contains("Usage Pattern"));
}

#[test]
fn test_learn_context_extractor_basic() {
    let prompt = learn_prompt(
        "flask",
        "3.0.0",
        "# Breaking Changes\n- Removed old API",
        None,
        false,
        &Language::Python,
    );

    assert!(prompt.contains("flask"));
    assert!(prompt.contains("3.0.0"));
    assert!(prompt.contains("# Breaking Changes"));
}

#[test]
fn test_learn_includes_extraction_requirements() {
    let prompt = learn_prompt("django", "4.2.0", "", None, false, &Language::Python);

    assert!(prompt.contains("CONVENTIONS"));
    assert!(prompt.contains("PITFALLS"));
    assert!(prompt.contains("BREAKING CHANGES"));
    assert!(prompt.contains("MIGRATION NOTES"));
}

#[test]
fn test_learn_includes_pitfall_structure() {
    let prompt = learn_prompt("package", "1.0.0", "", None, false, &Language::Python);

    assert!(prompt.contains("Wrong:"));
    assert!(prompt.contains("Why it fails:"));
    assert!(prompt.contains("Right:"));
}

#[test]
fn test_learn_includes_breaking_change_structure() {
    let prompt = learn_prompt("sqlalchemy", "2.0.0", "", None, false, &Language::Python);

    assert!(prompt.contains("version_from"));
    assert!(prompt.contains("version_to"));
    assert!(prompt.contains("change"));
    assert!(prompt.contains("migration"));
}

#[test]
fn test_learn_includes_docstring_styles() {
    let prompt = learn_prompt("numpy", "1.24.0", "", None, false, &Language::Python);

    assert!(prompt.contains("ReStructuredText"));
    assert!(prompt.contains("Google/NumPy docstring styles"));
}

#[test]
fn test_learn_includes_framework_specific_considerations() {
    let prompt = learn_prompt("django", "4.2.0", "", None, false, &Language::Python);

    assert!(prompt.contains("Special Considerations"));
    assert!(prompt.contains("Configuration patterns"));
    assert!(prompt.contains("Async requirements"));
}

#[test]
fn test_learn_output_format() {
    let prompt = learn_prompt("click", "8.1.0", "", None, false, &Language::Python);

    assert!(prompt.contains("Return JSON"));
    assert!(prompt.contains(r#""conventions""#));
    assert!(prompt.contains(r#""pitfalls""#));
    assert!(prompt.contains(r#""breaking_changes""#));
    assert!(prompt.contains(r#""migration_notes""#));
}

#[test]
fn test_create_synthesizer_basic() {
    let prompt = create_prompt(
        "requests",
        "2.31.0",
        Some("Apache-2.0"),
        &[],
        &Language::Python,
        "API surface data",
        "Pattern data",
        "Context data",
        None,
        false,
        &[],
    );

    assert!(prompt.contains("requests"));
    assert!(prompt.contains("2.31.0"));
    assert!(prompt.contains("Apache-2.0"));
    assert!(prompt.contains("API surface data"));
    assert!(prompt.contains("Pattern data"));
    assert!(prompt.contains("Context data"));
}

#[test]
fn test_create_license_field_with_value() {
    let prompt = create_prompt(
        "django",
        "4.2.0",
        Some("BSD-3-Clause"),
        &[],
        &Language::Python,
        "",
        "",
        "",
        None,
        false,
        &[],
    );

    assert!(prompt.contains("license: BSD-3-Clause"));
}

#[test]
fn test_create_license_field_without_value() {
    let prompt = create_prompt(
        "mypackage",
        "1.0.0",
        None,
        &[],
        &Language::Python,
        "",
        "",
        "",
        None,
        false,
        &[],
    );

    assert!(prompt.contains("license: MIT"));
}

#[test]
fn test_create_project_urls_empty() {
    let prompt = create_prompt(
        "package",
        "1.0.0",
        None,
        &[],
        &Language::Python,
        "",
        "",
        "",
        None,
        false,
        &[],
    );

    assert!(prompt.contains("[Official Documentation](search for official docs)"));
    assert!(prompt.contains("[GitHub Repository](search for GitHub repo)"));
}

#[test]
fn test_create_project_urls_single() {
    let urls = vec![(
        "Documentation".to_string(),
        "https://docs.example.com".to_string(),
    )];

    let prompt = create_prompt(
        "package",
        "1.0.0",
        None,
        &urls,
        &Language::Python,
        "",
        "",
        "",
        None,
        false,
        &[],
    );

    assert!(prompt.contains("- [Documentation](https://docs.example.com)"));
    assert!(!prompt.contains("search for official docs"));
}

#[test]
fn test_create_project_urls_multiple() {
    let urls = vec![
        (
            "Documentation".to_string(),
            "https://docs.example.com".to_string(),
        ),
        (
            "GitHub".to_string(),
            "https://github.com/user/repo".to_string(),
        ),
        (
            "PyPI".to_string(),
            "https://pypi.org/project/package".to_string(),
        ),
    ];

    let prompt = create_prompt(
        "package",
        "1.0.0",
        None,
        &urls,
        &Language::Python,
        "",
        "",
        "",
        None,
        false,
        &[],
    );

    assert!(prompt.contains("- [Documentation](https://docs.example.com)"));
    assert!(prompt.contains("- [GitHub](https://github.com/user/repo)"));
    assert!(prompt.contains("- [PyPI](https://pypi.org/project/package)"));
}

#[test]
fn test_create_custom_instructions_none() {
    let prompt = create_prompt(
        "package",
        "1.0.0",
        None,
        &[],
        &Language::Python,
        "",
        "",
        "",
        None,
        false,
        &[],
    );

    // Without custom instructions, no CUSTOM INSTRUCTIONS section is emitted.
    assert!(!prompt.contains("CUSTOM INSTRUCTIONS"));
}

#[test]
fn test_create_custom_instructions_present() {
    let custom = "Always use type hints\nPrefer async functions";

    let prompt = create_prompt(
        "package",
        "1.0.0",
        None,
        &[],
        &Language::Python,
        "",
        "",
        "",
        Some(custom),
        false,
        &[],
    );

    assert!(prompt.contains("CUSTOM INSTRUCTIONS"));
    assert!(prompt.contains("Always use type hints"));
    assert!(prompt.contains("Prefer async functions"));
}

#[test]
fn test_create_includes_skill_md_structure() {
    let prompt = create_prompt(
        "fastapi",
        "0.100.0",
        None,
        &[],
        &Language::Python,
        "",
        "",
        "",
        None,
        false,
        &[],
    );

    assert!(prompt.contains("## Imports"));
    assert!(prompt.contains("## Core Patterns"));
    assert!(prompt.contains("## Configuration"));
    assert!(prompt.contains("## Pitfalls"));
    assert!(prompt.contains("## References"));
    assert!(prompt.contains("## Migration from"));
    assert!(prompt.contains("## API Reference"));
}

#[test]
fn test_create_includes_validation_rules() {
    let prompt = create_prompt(
        "requests",
        "2.31.0",
        None,
        &[],
        &Language::Python,
        "",
        "",
        "",
        None,
        false,
        &[],
    );

    // Core anti-hallucination rules in the create prompt.
    assert!(prompt.contains("Do not invent APIs"));
    assert!(prompt.contains("REAL APIs"));
}

#[test]
fn test_create_includes_pitfall_requirements() {
    let prompt = create_prompt(
        "package",
        "1.0.0",
        None,
        &[],
        &Language::Python,
        "",
        "",
        "",
        None,
        false,
        &[],
    );

    // Pitfall requirements in <instructions> and output structure
    assert!(prompt.contains("3-5 common mistakes") || prompt.contains("3-5 Wrong/Right pairs"));
    assert!(prompt.contains("Wrong") && prompt.contains("Right"));
    assert!(prompt.contains("Pitfalls section is mandatory") || prompt.contains("## Pitfalls"));
}

// Library-specific prompt guidance (per-framework categories, routing/middleware/
// CLI/ORM hints, etc.) was removed intentionally per CLAUDE.md's "no hardcoded
// package-name logic" rule — any per-library steering now lives in the prompt's
// universal rules or user-provided custom instructions, not in hard-coded
// keyword hints. The obsolete per-category tests were removed with this change.

#[test]
fn test_create_parameter_order() {
    let urls = vec![("Docs".to_string(), "https://docs.example.com".to_string())];

    let prompt = create_prompt(
        "mypackage",
        "1.2.3",
        Some("GPL-3.0"),
        &urls,
        &Language::Python,
        "API",
        "Patterns",
        "Context",
        Some("Custom"),
        false,
        &[],
    );

    assert!(prompt.contains("mypackage"));
    assert!(prompt.contains("1.2.3"));
    assert!(prompt.contains("GPL-3.0"));
    assert!(prompt.contains("[Docs](https://docs.example.com)"));
    assert!(prompt.contains("python"));
    assert!(prompt.contains("API"));
    assert!(prompt.contains("Patterns"));
    assert!(prompt.contains("Context"));
    assert!(prompt.contains("Custom"));
}

#[test]
fn test_all_agents_include_package_name_and_version() {
    let package = "testpkg";
    let version = "1.2.3";

    let p1 = extract_prompt(package, version, "", 1, None, false, &Language::Python);
    let p2 = map_prompt(package, version, "", None, false, &Language::Python);
    let p3 = learn_prompt(package, version, "", None, false, &Language::Python);
    let p4 = create_prompt(
        package,
        version,
        None,
        &[],
        &Language::Python,
        "",
        "",
        "",
        None,
        false,
        &[],
    );
    for prompt in [p1, p2, p3, p4] {
        assert!(prompt.contains(package));
        assert!(prompt.contains(version));
    }
}

#[test]
fn test_template_rendering_with_special_characters() {
    let source = "def func():\n    '''Docstring with \"quotes\" and {braces}'''";
    let prompt = extract_prompt("pkg", "1.0", source, 1, None, false, &Language::Python);

    assert!(prompt.contains(source));
}

#[test]
fn test_create_escapes_braces_in_format_string() {
    let prompt = create_prompt(
        "fastapi",
        "0.100.0",
        None,
        &[],
        &Language::Python,
        "",
        "",
        "",
        None,
        false,
        &[],
    );

    // Verify the prompt doesn't have broken format string escaping
    // (old test checked for {{package_name}} → {package_name} in template;
    //  new template doesn't include import examples, so check format! didn't panic)
    assert!(!prompt.is_empty());
}

#[test]
fn test_create_references_section_formatting() {
    let urls = vec![("Home".to_string(), "https://home.example.com".to_string())];

    let prompt = create_prompt(
        "package",
        "1.0.0",
        None,
        &urls,
        &Language::Python,
        "",
        "",
        "",
        None,
        false,
        &[],
    );

    // Check markdown link format
    assert!(prompt.contains("- [Home](https://home.example.com)"));
}

#[test]
fn test_create_includes_ecosystem_in_frontmatter() {
    let prompt = create_prompt(
        "package",
        "1.0.0",
        None,
        &[],
        &Language::Python,
        "",
        "",
        "",
        None,
        false,
        &[],
    );

    assert!(prompt.contains("ecosystem: python"));
}

#[test]
fn test_create_includes_version_in_frontmatter() {
    let prompt = create_prompt(
        "mylib",
        "2.5.8",
        None,
        &[],
        &Language::Rust,
        "",
        "",
        "",
        None,
        false,
        &[],
    );

    assert!(prompt.contains("version: \"2.5.8\""));
}

#[test]
fn test_empty_inputs_handled_gracefully() {
    let p1 = extract_prompt("", "", "", 1, None, false, &Language::Python);
    let p2 = map_prompt("", "", "", None, false, &Language::Python);
    let p3 = learn_prompt("", "", "", None, false, &Language::Python);
    let p4 = create_prompt(
        "",
        "",
        None,
        &[],
        &Language::Python,
        "",
        "",
        "",
        None,
        false,
        &[],
    );
    // All should produce valid strings without panicking
    assert!(!p1.is_empty());
    assert!(!p2.is_empty());
    assert!(!p3.is_empty());
    assert!(!p4.is_empty());
}

#[test]
fn test_extract_json_structure_validity() {
    let prompt = extract_prompt("package", "1.0", "", 1, None, false, &Language::Python);

    // Should have JSON structure with braces
    assert!(prompt.contains(r#"{"#));
    assert!(prompt.contains(r#"}"#));
}

#[test]
fn test_map_json_structure_validity() {
    let prompt = map_prompt("package", "1.0", "", None, false, &Language::Python);

    // Should have JSON structure with braces
    assert!(prompt.contains(r#"{"#));
    assert!(prompt.contains(r#"}"#));
}

#[test]
fn test_learn_json_structure_validity() {
    let prompt = learn_prompt("package", "1.0", "", None, false, &Language::Python);

    // Should have JSON structure with braces
    assert!(prompt.contains(r#"{"#));
    assert!(prompt.contains(r#"}"#));
}

#[test]
fn test_create_default_license_is_mit() {
    let prompt = create_prompt(
        "unlicensed",
        "1.0.0",
        None,
        &[],
        &Language::Python,
        "",
        "",
        "",
        None,
        false,
        &[],
    );

    assert!(prompt.contains("license: MIT"));
    assert!(!prompt.contains("license: Apache"));
    assert!(!prompt.contains("license: GPL"));
}

#[test]
fn test_comprehensive_coverage_extract() {
    let prompt = extract_prompt(
        "comprehensive_test",
        "1.0.0",
        "test_code",
        1,
        None,
        false,
        &Language::Python,
    );

    // Ensure all major sections are covered
    let required_sections = [
        "library_category",
        "What to Extract",
        "Signature Handling",
        "Method Type Classification",
        "Type Hint Handling",
        "Deprecation Tracking",
        "DECORATOR STACKS",
        "CLASS HIERARCHIES",
        "LIBRARY PATTERNS",
        "Extraction Priorities",
        "Output Format",
    ];

    for section in &required_sections {
        assert!(prompt.contains(section), "Missing section: {}", section);
    }
}

#[test]
fn test_comprehensive_coverage_map() {
    let prompt = map_prompt(
        "comprehensive_test",
        "1.0.0",
        "test_code",
        None,
        false,
        &Language::Python,
    );

    let required_sections = [
        "What to Extract",
        "Key Testing Patterns",
        "Test Clients",
        "Parametrized / Data-Driven Tests",
        "Async Patterns",
        "Dependency injection",
        "Error Handling",
        "Output Format",
    ];

    for section in &required_sections {
        assert!(prompt.contains(section), "Missing section: {}", section);
    }
}

#[test]
fn test_comprehensive_coverage_learn() {
    let prompt = learn_prompt(
        "comprehensive_test",
        "1.0.0",
        "docs",
        None,
        false,
        &Language::Python,
    );

    let required_sections = [
        "CONVENTIONS",
        "PITFALLS",
        "BREAKING CHANGES",
        "MIGRATION NOTES",
        "Documentation Patterns",
        "Special Considerations",
        "Output Format",
    ];

    for section in &required_sections {
        assert!(prompt.contains(section), "Missing section: {}", section);
    }
}

#[test]
fn test_comprehensive_coverage_create() {
    let prompt = create_prompt(
        "comprehensive_test",
        "1.0.0",
        Some("MIT"),
        &[],
        &Language::Python,
        "api",
        "patterns",
        "context",
        None,
        false,
        &[],
    );

    let required_sections = [
        "## Imports",
        "## Core Patterns",
        "## Configuration",
        "## Pitfalls",
        "## References",
        "## API Reference",
    ];

    for section in &required_sections {
        assert!(prompt.contains(section), "Missing section: {}", section);
    }
}

// --- Overwrite mode and custom instructions ---

#[test]
fn test_extract_overwrite_with_custom() {
    let custom = "My custom extract prompt";
    let prompt = extract_prompt(
        "pkg",
        "1.0",
        "source",
        1,
        Some(custom),
        true,
        &Language::Python,
    );
    assert_eq!(prompt, custom);
}

#[test]
fn test_extract_overwrite_without_custom_uses_default() {
    let prompt = extract_prompt("pkg", "1.0", "source", 1, None, true, &Language::Python);
    // No custom provided, should fall through to default prompt
    assert!(prompt.contains("pkg"));
    assert!(prompt.contains("Extract"));
}

#[test]
fn test_extract_append_custom() {
    let custom = "Also extract internal APIs";
    let prompt = extract_prompt(
        "pkg",
        "1.0",
        "source",
        1,
        Some(custom),
        false,
        &Language::Python,
    );
    // Custom should be appended, default prompt still present
    assert!(prompt.contains("pkg"));
    assert!(prompt.contains("Also extract internal APIs"));
}

#[test]
fn test_map_overwrite_with_custom() {
    let custom = "My custom map prompt";
    let prompt = map_prompt("pkg", "1.0", "tests", Some(custom), true, &Language::Python);
    assert_eq!(prompt, custom);
}

#[test]
fn test_map_append_custom() {
    let custom = "Focus on error patterns";
    let prompt = map_prompt(
        "pkg",
        "1.0",
        "tests",
        Some(custom),
        false,
        &Language::Python,
    );
    assert!(prompt.contains("pkg"));
    assert!(prompt.contains("Focus on error patterns"));
}

#[test]
fn test_learn_overwrite_with_custom() {
    let custom = "My custom learn prompt";
    let prompt = learn_prompt("pkg", "1.0", "docs", Some(custom), true, &Language::Python);
    assert_eq!(prompt, custom);
}

#[test]
fn test_learn_append_custom() {
    let custom = "Include performance tips";
    let prompt = learn_prompt("pkg", "1.0", "docs", Some(custom), false, &Language::Python);
    assert!(prompt.contains("pkg"));
    assert!(prompt.contains("Include performance tips"));
}

#[test]
fn test_create_overwrite_with_custom() {
    let custom = "My custom create prompt";
    let prompt = create_prompt(
        "pkg",
        "1.0",
        None,
        &[],
        &Language::Python,
        "",
        "",
        "",
        Some(custom),
        true,
        &[],
    );
    assert_eq!(prompt, custom);
}

// --- Scale hints for large libraries ---

#[test]
fn test_extract_scale_hint_large_library() {
    let prompt = extract_prompt(
        "biglib",
        "1.0",
        "source",
        1500,
        None,
        false,
        &Language::Python,
    );
    assert!(prompt.contains("LARGE LIBRARY"));
    assert!(prompt.contains("1000+ files"));
}

#[test]
fn test_extract_scale_hint_very_large_library() {
    let prompt = extract_prompt(
        "hugelib",
        "1.0",
        "source",
        3000,
        None,
        false,
        &Language::Python,
    );
    assert!(prompt.contains("LARGE LIBRARY ALERT"));
    assert!(prompt.contains("2000+ files"));
}

#[test]
fn test_extract_no_scale_hint_small_library() {
    let prompt = extract_prompt(
        "smalllib",
        "1.0",
        "source",
        50,
        None,
        false,
        &Language::Python,
    );
    assert!(!prompt.contains("LARGE LIBRARY"));
}

// --- Language-aware prompts ---

#[test]
fn test_extract_prompt_uses_language_and_ecosystem_term() {
    let prompt = extract_prompt(
        "mymod",
        "0.1.0",
        "func main() {}",
        1,
        None,
        false,
        &Language::Go,
    );
    assert!(prompt.contains("go module \"mymod\""));
    assert!(!prompt.contains("Python package"));
}

#[test]
fn test_map_prompt_uses_language_and_ecosystem_term() {
    let prompt = map_prompt(
        "mycrate",
        "1.0.0",
        "fn test_it() {}",
        None,
        false,
        &Language::Rust,
    );
    assert!(prompt.contains("rust crate \"mycrate\""));
    assert!(!prompt.contains("Python package"));
}

#[test]
fn test_learn_prompt_uses_language_and_ecosystem_term() {
    let prompt = learn_prompt(
        "mypkg",
        "2.0.0",
        "# Changelog",
        None,
        false,
        &Language::JavaScript,
    );
    assert!(prompt.contains("javascript package \"mypkg\""));
    assert!(!prompt.contains("Python package"));
}

#[test]
fn test_create_prompt_uses_ecosystem_term() {
    let prompt = create_prompt(
        "mymod",
        "0.1.0",
        None,
        &[],
        &Language::Go,
        "",
        "",
        "",
        None,
        false,
        &[],
    );
    assert!(prompt.contains("go module \"mymod\""));
    assert!(!prompt.contains("Python package"));
}

// --- create agent update mode ---

#[test]
fn test_create_update_prompt_basic() {
    let prompt = create_update_prompt(
        "requests",
        "2.32.0",
        "# Existing SKILL.md content",
        "API surface",
        "Patterns",
        "Context",
        &Language::Python,
        &[],
        None,
    );
    assert!(prompt.contains("requests"));
    assert!(prompt.contains("2.32.0"));
    assert!(prompt.contains("# Existing SKILL.md content"));
    assert!(prompt.contains("API surface"));
}

// ============================================================================
// SECURITY CLAUSE REGRESSION TESTS
// ============================================================================

#[test]
fn test_create_synthesizer_contains_security_rule() {
    let prompt = create_prompt(
        "test",
        "1.0",
        None,
        &[],
        &Language::Python,
        "apis",
        "patterns",
        "context",
        None,
        false,
        &[],
    );
    // Core security rule exists with its key threat categories.
    assert!(
        prompt.contains("RULE — SECURITY"),
        "create prompt must contain the SECURITY rule"
    );
    assert!(
        prompt.contains("destroy data"),
        "Missing destruction category"
    );
    assert!(
        prompt.contains("exfiltrate secrets"),
        "Missing exfiltration category"
    );
    assert!(
        prompt.contains("persist access"),
        "Missing persistence category"
    );
    assert!(
        prompt.contains("escalate privileges"),
        "Missing privilege escalation category"
    );
    assert!(
        prompt.contains("manipulate AI agents"),
        "Missing prompt injection category"
    );
}

#[test]
fn test_create_update_contains_security_rule() {
    let prompt = create_update_prompt(
        "test",
        "1.0",
        "existing skill",
        "apis",
        "patterns",
        "context",
        &Language::Python,
        &[],
        None,
    );
    // Security section exists in update prompt too
    assert!(
        prompt.contains("SECURITY (CRITICAL)") || prompt.contains("Security (CRITICAL)"),
        "create agent update prompt must contain Security section"
    );
    assert!(
        prompt.contains("weaponized")
            || prompt.contains("destroy data")
            || prompt.contains("exfiltrate"),
        "Missing security warning in update prompt"
    );
    assert!(
        prompt.contains("bypass authentication") || prompt.contains("persist access"),
        "Missing auth bypass warning in update prompt"
    );
    assert!(
        prompt.contains("Do not preserve harmful content")
            || prompt.contains("Remove harmful content"),
        "Update prompt must address harmful content from previous versions"
    );
}

#[test]
fn test_create_update_prompt_injects_rust_deps() {
    let deps = vec![
        StructuredDep {
            name: "tokio".to_string(),
            raw_spec: Some("{ version = \"1\", features = [\"full\"] }".to_string()),
            source: DepSource::Manifest,
        },
        StructuredDep {
            name: "serde".to_string(),
            raw_spec: Some("\"1.0\"".to_string()),
            source: DepSource::Manifest,
        },
    ];
    let prompt = create_update_prompt(
        "my-crate",
        "2.0.0",
        "existing skill",
        "apis",
        "patterns",
        "context",
        &Language::Rust,
        &deps,
        None,
    );
    assert!(
        prompt.contains("[dependencies]"),
        "Rust update prompt must include the Known Dependencies block"
    );
    assert!(
        prompt.contains("tokio"),
        "Rust update deps should include tokio"
    );
    assert!(
        prompt.contains("serde"),
        "Rust update deps should include serde"
    );
}

#[test]
fn test_create_update_prompt_empty_deps_rust_guidance() {
    let prompt = create_update_prompt(
        "my-crate",
        "2.0.0",
        "existing skill",
        "apis",
        "patterns",
        "context",
        &Language::Rust,
        &[],
        None,
    );
    assert!(
        prompt.contains("Do NOT invent or guess dependency versions"),
        "Empty-deps Rust update prompt must include guidance against fabrication"
    );
}

#[test]
fn test_create_update_prompt_no_deps_for_non_rust() {
    let deps = vec![StructuredDep {
        name: "requests".to_string(),
        raw_spec: Some("\"2.31\"".to_string()),
        source: DepSource::Manifest,
    }];
    let prompt = create_update_prompt(
        "my-lib",
        "1.0.0",
        "existing skill",
        "apis",
        "patterns",
        "context",
        &Language::Python,
        &deps,
        None,
    );
    // Python update prompt should NOT inject the Rust-specific deps block
    assert!(
        !prompt.contains("[dependencies]"),
        "Non-Rust update prompt must not include [dependencies] block"
    );
}

#[test]
fn test_create_overwrite_mode_bypasses_security() {
    // Document the known limitation: overwrite mode replaces the entire prompt
    let prompt = create_prompt(
        "test",
        "1.0",
        None,
        &[],
        &Language::Python,
        "apis",
        "patterns",
        "context",
        Some("custom prompt"),
        true,
        &[],
    );
    // In overwrite mode, the security rules are NOT present (by design)
    assert!(
        !prompt.contains("RULE 8"),
        "Overwrite mode should replace entire prompt including security rules"
    );
    assert_eq!(
        prompt, "custom prompt",
        "Overwrite mode should return custom prompt verbatim"
    );
}

#[test]
fn test_create_security_is_mechanism_level() {
    let prompt = create_prompt(
        "test",
        "1.0",
        None,
        &[],
        &Language::Python,
        "apis",
        "patterns",
        "context",
        None,
        false,
        &[],
    );
    // Security rule must describe behaviours (destroy/exfiltrate/persist/etc.)
    // rather than a filename or tool list — behavioural wording keeps the rule
    // robust against new attack mechanisms.
    assert!(
        prompt.contains("destroy data"),
        "behaviour: destructive ops must be called out"
    );
    assert!(
        prompt.contains("exfiltrate secrets"),
        "behaviour: exfiltration must be called out"
    );
    assert!(
        prompt.contains("manipulate AI agents"),
        "behaviour: prompt-injection must be called out"
    );
}

// ============================================================================
// create_update_prompt: LANGUAGE HINTS AND DEPS COVERAGE
// ============================================================================

#[test]
fn test_create_update_prompt_rust_has_language_hints() {
    let prompt = create_update_prompt(
        "my-crate",
        "2.0.0",
        "existing skill",
        "apis",
        "patterns",
        "context",
        &Language::Rust,
        &[],
        None,
    );
    assert!(
        prompt.contains("RUST-SPECIFIC HINTS"),
        "Rust update prompt must include Rust-specific language hints"
    );
    assert!(
        prompt.contains("Result"),
        "Rust create hints should mention Result type"
    );
}

#[test]
fn test_create_update_prompt_python_has_language_hints() {
    let prompt = create_update_prompt(
        "requests",
        "2.32.0",
        "existing skill",
        "apis",
        "patterns",
        "context",
        &Language::Python,
        &[],
        None,
    );
    assert!(
        prompt.contains("PYTHON-SPECIFIC HINTS"),
        "Python update prompt must include Python-specific language hints"
    );
}

#[test]
fn test_create_update_prompt_go_has_language_hints() {
    let prompt = create_update_prompt(
        "cobra",
        "1.9.0",
        "existing skill",
        "apis",
        "patterns",
        "context",
        &Language::Go,
        &[],
        None,
    );
    assert!(
        prompt.contains("GO-SPECIFIC HINTS"),
        "Go update prompt must include Go-specific language hints"
    );
}

#[test]
fn test_create_update_prompt_js_no_language_hints() {
    let prompt = create_update_prompt(
        "lodash",
        "5.0.0",
        "existing skill",
        "apis",
        "patterns",
        "context",
        &Language::JavaScript,
        &[],
        None,
    );
    // JavaScript has no specific hints yet
    assert!(
        !prompt.contains("PYTHON-SPECIFIC"),
        "JS update should not contain Python hints"
    );
    assert!(
        !prompt.contains("RUST-SPECIFIC"),
        "JS update should not contain Rust hints"
    );
}

#[test]
fn test_create_update_prompt_deps_block_format() {
    let deps = vec![
        StructuredDep {
            name: "tokio".to_string(),
            raw_spec: Some("{ version = \"1\", features = [\"full\"] }".to_string()),
            source: DepSource::Manifest,
        },
        StructuredDep {
            name: "serde".to_string(),
            raw_spec: Some("\"1.0\"".to_string()),
            source: DepSource::Manifest,
        },
    ];
    let prompt = create_update_prompt(
        "my-crate",
        "2.0.0",
        "existing skill",
        "apis",
        "patterns",
        "context",
        &Language::Rust,
        &deps,
        None,
    );
    // Verify the toml block structure
    assert!(
        prompt.contains("```toml\n[dependencies]\n"),
        "Deps block must start with ```toml fenced [dependencies]"
    );
    assert!(
        prompt.contains("tokio = { version = \"1\", features = [\"full\"] }\n"),
        "tokio dep should preserve full TOML spec"
    );
    assert!(
        prompt.contains("serde = \"1.0\"\n"),
        "serde dep should use raw_spec value"
    );
    assert!(
        prompt.contains("```\n"),
        "Deps block must be closed with ```"
    );
}

#[test]
fn test_create_update_prompt_dep_none_raw_spec_gets_wildcard() {
    let deps = vec![StructuredDep {
        name: "rand".to_string(),
        raw_spec: None,
        source: DepSource::Manifest,
    }];
    let prompt = create_update_prompt(
        "my-crate",
        "1.0.0",
        "existing skill",
        "apis",
        "patterns",
        "context",
        &Language::Rust,
        &deps,
        None,
    );
    assert!(
        prompt.contains("rand = \"*\"\n"),
        "Dep with None raw_spec must use wildcard \"*\""
    );
}

#[test]
fn test_create_update_prompt_mixed_deps_some_none_raw_spec() {
    let deps = vec![
        StructuredDep {
            name: "tokio".to_string(),
            raw_spec: Some("\"1.0\"".to_string()),
            source: DepSource::Manifest,
        },
        StructuredDep {
            name: "unknown-crate".to_string(),
            raw_spec: None,
            source: DepSource::Manifest,
        },
    ];
    let prompt = create_update_prompt(
        "my-crate",
        "1.0.0",
        "existing skill",
        "apis",
        "patterns",
        "context",
        &Language::Rust,
        &deps,
        None,
    );
    assert!(
        prompt.contains("tokio = \"1.0\"\n"),
        "tokio should use its raw_spec"
    );
    assert!(
        prompt.contains("unknown-crate = \"*\"\n"),
        "unknown-crate with None raw_spec should use wildcard"
    );
}

#[test]
fn test_create_update_prompt_uses_ecosystem_term() {
    let prompt = create_update_prompt(
        "tokio",
        "2.0.0",
        "existing",
        "api",
        "patterns",
        "context",
        &Language::Rust,
        &[],
        None,
    );
    assert!(
        prompt.contains("crate \"tokio\""),
        "Rust update prompt must use ecosystem term 'crate'"
    );
}

#[test]
fn test_create_update_prompt_uses_lang_str_in_instructions() {
    let prompt = create_update_prompt(
        "tokio",
        "2.0.0",
        "existing",
        "api",
        "patterns",
        "context",
        &Language::Rust,
        &[],
        None,
    );
    assert!(
        prompt.contains("rust code examples"),
        "Instructions should reference language name for code examples"
    );
}

#[test]
fn test_create_update_prompt_version_in_instructions() {
    let prompt = create_update_prompt(
        "serde",
        "3.5.0",
        "existing",
        "api",
        "patterns",
        "context",
        &Language::Rust,
        &[],
        None,
    );
    // version appears twice: once in the header and once in instruction #2
    assert!(
        prompt.contains("Update metadata.version in frontmatter to 3.5.0"),
        "Instructions must reference the target version"
    );
}

#[test]
fn test_create_update_prompt_preserves_existing_skill_content() {
    let existing = "---\nname: my-crate\nversion: 1.0.0\n---\n## Core Patterns\nold pattern here";
    let prompt = create_update_prompt(
        "my-crate",
        "2.0.0",
        existing,
        "api",
        "patterns",
        "context",
        &Language::Rust,
        &[],
        None,
    );
    assert!(
        prompt.contains("old pattern here"),
        "Existing skill content must be embedded in the prompt"
    );
}

// ============================================================================
// create_prompt: RUST DEPS INJECTION (create mode, not just update mode)
// ============================================================================

#[test]
fn test_create_prompt_rust_with_deps() {
    let deps = vec![
        StructuredDep {
            name: "reqwest".to_string(),
            raw_spec: Some("{ version = \"0.12\", features = [\"json\"] }".to_string()),
            source: DepSource::Manifest,
        },
        StructuredDep {
            name: "tokio".to_string(),
            raw_spec: Some("\"1\"".to_string()),
            source: DepSource::Manifest,
        },
    ];
    let prompt = create_prompt(
        "my-crate",
        "1.0.0",
        None,
        &[],
        &Language::Rust,
        "api",
        "patterns",
        "context",
        None,
        false,
        &deps,
    );
    assert!(
        prompt.contains("[dependencies]"),
        "Rust create prompt with deps must include [dependencies] block"
    );
    assert!(
        prompt.contains("reqwest = { version = \"0.12\", features = [\"json\"] }"),
        "reqwest dep must appear with full spec"
    );
    assert!(
        prompt.contains("tokio = \"1\""),
        "tokio dep must appear with its version"
    );
}

#[test]
fn test_create_prompt_rust_empty_deps_guidance() {
    let prompt = create_prompt(
        "my-crate",
        "1.0.0",
        None,
        &[],
        &Language::Rust,
        "api",
        "patterns",
        "context",
        None,
        false,
        &[],
    );
    assert!(
        prompt.contains("Dependencies Note"),
        "Empty-deps Rust create prompt must include Dependencies Note"
    );
    assert!(
        prompt.contains("Do NOT invent or guess dependency versions"),
        "Empty-deps guidance must warn against fabrication"
    );
}

#[test]
fn test_create_prompt_rust_dep_none_raw_spec_wildcard() {
    let deps = vec![StructuredDep {
        name: "anyhow".to_string(),
        raw_spec: None,
        source: DepSource::Manifest,
    }];
    let prompt = create_prompt(
        "my-crate",
        "1.0.0",
        None,
        &[],
        &Language::Rust,
        "api",
        "patterns",
        "context",
        None,
        false,
        &deps,
    );
    assert!(
        prompt.contains("anyhow = \"*\""),
        "Rust dep with None raw_spec in create mode must use wildcard"
    );
}

#[test]
fn test_create_prompt_python_ignores_deps() {
    let deps = vec![StructuredDep {
        name: "requests".to_string(),
        raw_spec: Some("\"2.31\"".to_string()),
        source: DepSource::Manifest,
    }];
    let prompt = create_prompt(
        "my-lib",
        "1.0.0",
        None,
        &[],
        &Language::Python,
        "api",
        "patterns",
        "context",
        None,
        false,
        &deps,
    );
    assert!(
        !prompt.contains("Known Dependencies (from Cargo.toml"),
        "Python create prompt must not include Rust [dependencies] block"
    );
    assert!(
        !prompt.contains("Dependencies Note"),
        "Python create prompt must not include Rust deps guidance"
    );
}

// ============================================================================
// Java language hints coverage
// ============================================================================

#[test]
fn test_create_prompt_java_has_language_hints() {
    let prompt = create_prompt(
        "jackson-core",
        "2.15.0",
        None,
        &[],
        &Language::Java,
        "api",
        "patterns",
        "context",
        None,
        false,
        &[],
    );
    assert!(
        prompt.contains("JAVA-SPECIFIC HINTS"),
        "Java create prompt must include Java-specific hints"
    );
    assert!(
        prompt.contains("import com.example"),
        "Java create hints should mention import conventions"
    );
}

#[test]
fn test_extract_prompt_java_has_language_hints() {
    let prompt = extract_prompt(
        "jackson-core",
        "2.15.0",
        "public class Foo {}",
        1,
        None,
        false,
        &Language::Java,
    );
    assert!(
        prompt.contains("JAVA-SPECIFIC HINTS"),
        "Java extract prompt must include Java-specific hints"
    );
    assert!(
        prompt.contains("pom.xml"),
        "Java extract hints should mention pom.xml"
    );
}

#[test]
fn test_map_prompt_java_has_language_hints() {
    let prompt = map_prompt(
        "jackson-core",
        "2.15.0",
        "test code",
        None,
        false,
        &Language::Java,
    );
    assert!(
        prompt.contains("JAVA-SPECIFIC HINTS"),
        "Java map prompt must include Java-specific hints"
    );
    assert!(
        prompt.contains("JUnit"),
        "Java map hints should mention JUnit"
    );
}

#[test]
fn test_learn_prompt_java_has_language_hints() {
    let prompt = learn_prompt(
        "jackson-core",
        "2.15.0",
        "docs",
        None,
        false,
        &Language::Java,
    );
    assert!(
        prompt.contains("JAVA-SPECIFIC HINTS"),
        "Java learn prompt must include Java-specific hints"
    );
    assert!(
        prompt.contains("Javadoc"),
        "Java learn hints should mention Javadoc"
    );
}

#[test]
fn test_create_update_prompt_java_has_language_hints() {
    let prompt = create_update_prompt(
        "jackson-core",
        "2.15.0",
        "existing skill",
        "apis",
        "patterns",
        "context",
        &Language::Java,
        &[],
        None,
    );
    assert!(
        prompt.contains("JAVA-SPECIFIC HINTS"),
        "Java update prompt must include Java-specific hints"
    );
}

#[test]
fn test_create_update_prompt_java_no_deps_block() {
    let deps = vec![StructuredDep {
        name: "guava".to_string(),
        raw_spec: Some("\"31.1-jre\"".to_string()),
        source: DepSource::Manifest,
    }];
    let prompt = create_update_prompt(
        "jackson-core",
        "2.15.0",
        "existing skill",
        "apis",
        "patterns",
        "context",
        &Language::Java,
        &deps,
        None,
    );
    assert!(
        !prompt.contains("[dependencies]"),
        "Java update prompt must not include Rust [dependencies] block"
    );
}
