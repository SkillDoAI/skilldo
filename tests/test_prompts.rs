use skilldo::detector::Language;
use skilldo::llm::prompts_v2::{
    create_prompt, create_update_prompt, extract_prompt, learn_prompt, map_prompt,
};

#[test]
fn test_agent1_api_extractor_basic() {
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
fn test_agent1_includes_library_category_instructions() {
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
fn test_agent1_includes_extraction_requirements() {
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
fn test_agent1_includes_signature_handling() {
    let prompt = extract_prompt("django", "4.2.0", "", 1, None, false, &Language::Python);

    assert!(prompt.contains("120 characters"));
    assert!(prompt.contains("signature_truncated"));
    assert!(prompt.contains("optional"));
}

#[test]
fn test_agent1_includes_method_type_classification() {
    let prompt = extract_prompt("sqlalchemy", "2.0.0", "", 1, None, false, &Language::Python);

    assert!(prompt.contains("function"));
    assert!(prompt.contains("method"));
    assert!(prompt.contains("classmethod"));
    assert!(prompt.contains("staticmethod"));
    assert!(prompt.contains("property"));
    assert!(prompt.contains("descriptor"));
}

#[test]
fn test_agent1_includes_type_hint_handling() {
    let prompt = extract_prompt("pydantic", "2.0.0", "", 1, None, false, &Language::Python);

    assert!(prompt.contains("Annotated"));
    assert!(prompt.contains("Union"));
    assert!(prompt.contains("Optional"));
    assert!(prompt.contains("Generic"));
    assert!(prompt.contains("Callable"));
}

#[test]
fn test_agent1_includes_deprecation_tracking() {
    let prompt = extract_prompt("flask", "3.0.0", "", 1, None, false, &Language::Python);

    assert!(prompt.contains("@deprecated"));
    assert!(prompt.contains("DeprecationWarning"));
    assert!(prompt.contains("since_version"));
    assert!(prompt.contains("removal_version"));
    assert!(prompt.contains("replacement"));
}

#[test]
fn test_agent1_includes_library_specific_patterns() {
    let prompt = extract_prompt("fastapi", "0.100.0", "", 1, None, false, &Language::Python);

    assert!(prompt.contains("Web Frameworks"));
    assert!(prompt.contains("CLI Tools"));
    assert!(prompt.contains("ORMs"));
    assert!(prompt.contains("HTTP Clients"));
}

#[test]
fn test_agent1_excludes_private_apis() {
    let prompt = extract_prompt("package", "1.0.0", "", 1, None, false, &Language::Python);

    // Verify prompt excludes private APIs
    assert!(prompt.contains("starting with `_`"));
    assert!(prompt.contains("`__all__`"));
}

#[test]
fn test_agent1_output_format() {
    let prompt = extract_prompt("numpy", "1.24.0", "", 1, None, false, &Language::Python);

    assert!(prompt.contains("Return JSON"));
    assert!(prompt.contains(r#""library_category""#));
    assert!(prompt.contains(r#""apis""#));
    assert!(prompt.contains(r#""name""#));
    assert!(prompt.contains(r#""type""#));
    assert!(prompt.contains(r#""signature""#));
}

#[test]
fn test_agent2_pattern_extractor_basic() {
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
fn test_agent2_includes_extraction_requirements() {
    let prompt = map_prompt("click", "8.1.0", "", None, false, &Language::Python);

    assert!(prompt.contains("API Being Tested"));
    assert!(prompt.contains("Setup Code"));
    assert!(prompt.contains("Usage Pattern"));
    assert!(prompt.contains("Assertions"));
    assert!(prompt.contains("Test Infrastructure"));
}

#[test]
fn test_agent2_includes_test_client_patterns() {
    let prompt = map_prompt("fastapi", "0.100.0", "", None, false, &Language::Python);

    assert!(prompt.contains("TestClient"));
    assert!(prompt.contains("CliRunner"));
    assert!(prompt.contains("Pytest fixtures"));
}

#[test]
fn test_agent2_includes_parametrized_tests() {
    let prompt = map_prompt("package", "1.0.0", "", None, false, &Language::Python);

    assert!(prompt.contains("@pytest.mark.parametrize"));
    assert!(prompt.contains("parameter combinations"));
}

#[test]
fn test_agent2_includes_async_patterns() {
    let prompt = map_prompt("httpx", "0.24.0", "", None, false, &Language::Python);

    assert!(prompt.contains("async def test_async"));
    assert!(prompt.contains("await"));
    assert!(prompt.contains("Mark patterns as async"));
}

#[test]
fn test_agent2_includes_dependency_injection() {
    let prompt = map_prompt("fastapi", "0.100.0", "", None, false, &Language::Python);

    assert!(prompt.contains("Depends"));
    assert!(prompt.contains("dependency patterns"));
}

#[test]
fn test_agent2_includes_error_handling() {
    let prompt = map_prompt("requests", "2.31.0", "", None, false, &Language::Python);

    assert!(prompt.contains("Error Handling"));
    assert!(prompt.contains("expected error responses"));
    assert!(prompt.contains("validation patterns"));
}

#[test]
fn test_agent2_output_format() {
    let prompt = map_prompt("django", "4.2.0", "", None, false, &Language::Python);

    assert!(prompt.contains("Return JSON"));
    assert!(prompt.contains("pattern"));
    assert!(prompt.contains("Setup Code"));
    assert!(prompt.contains("Usage Pattern"));
}

#[test]
fn test_agent3_context_extractor_basic() {
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
fn test_agent3_includes_extraction_requirements() {
    let prompt = learn_prompt("django", "4.2.0", "", None, false, &Language::Python);

    assert!(prompt.contains("CONVENTIONS"));
    assert!(prompt.contains("PITFALLS"));
    assert!(prompt.contains("BREAKING CHANGES"));
    assert!(prompt.contains("MIGRATION NOTES"));
}

#[test]
fn test_agent3_includes_pitfall_structure() {
    let prompt = learn_prompt("package", "1.0.0", "", None, false, &Language::Python);

    assert!(prompt.contains("Wrong:"));
    assert!(prompt.contains("Why it fails:"));
    assert!(prompt.contains("Right:"));
}

#[test]
fn test_agent3_includes_breaking_change_structure() {
    let prompt = learn_prompt("sqlalchemy", "2.0.0", "", None, false, &Language::Python);

    assert!(prompt.contains("version_from"));
    assert!(prompt.contains("version_to"));
    assert!(prompt.contains("change"));
    assert!(prompt.contains("migration"));
}

#[test]
fn test_agent3_includes_docstring_styles() {
    let prompt = learn_prompt("numpy", "1.24.0", "", None, false, &Language::Python);

    assert!(prompt.contains("ReStructuredText"));
    assert!(prompt.contains("Google style"));
    assert!(prompt.contains("NumPy style"));
}

#[test]
fn test_agent3_includes_framework_specific_considerations() {
    let prompt = learn_prompt("django", "4.2.0", "", None, false, &Language::Python);

    assert!(prompt.contains("Large Frameworks"));
    assert!(prompt.contains("CLI Tools"));
    assert!(prompt.contains("Async Frameworks"));
}

#[test]
fn test_agent3_output_format() {
    let prompt = learn_prompt("click", "8.1.0", "", None, false, &Language::Python);

    assert!(prompt.contains("Return JSON"));
    assert!(prompt.contains(r#""conventions""#));
    assert!(prompt.contains(r#""pitfalls""#));
    assert!(prompt.contains(r#""breaking_changes""#));
    assert!(prompt.contains(r#""migration_notes""#));
}

#[test]
fn test_agent4_synthesizer_basic() {
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
    );

    assert!(prompt.contains("requests"));
    assert!(prompt.contains("2.31.0"));
    assert!(prompt.contains("Apache-2.0"));
    assert!(prompt.contains("API surface data"));
    assert!(prompt.contains("Pattern data"));
    assert!(prompt.contains("Context data"));
}

#[test]
fn test_agent4_license_field_with_value() {
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
    );

    assert!(prompt.contains("license: BSD-3-Clause"));
}

#[test]
fn test_agent4_license_field_without_value() {
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
    );

    assert!(prompt.contains("license: MIT"));
}

#[test]
fn test_agent4_project_urls_empty() {
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
    );

    assert!(prompt.contains("[Official Documentation](search for official docs)"));
    assert!(prompt.contains("[GitHub Repository](search for GitHub repo)"));
}

#[test]
fn test_agent4_project_urls_single() {
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
    );

    assert!(prompt.contains("- [Documentation](https://docs.example.com)"));
    assert!(!prompt.contains("search for official docs"));
}

#[test]
fn test_agent4_project_urls_multiple() {
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
    );

    assert!(prompt.contains("- [Documentation](https://docs.example.com)"));
    assert!(prompt.contains("- [GitHub](https://github.com/user/repo)"));
    assert!(prompt.contains("- [PyPI](https://pypi.org/project/package)"));
}

#[test]
fn test_agent4_custom_instructions_none() {
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
    );

    assert!(!prompt.contains("CUSTOM INSTRUCTIONS FOR THIS REPO"));
}

#[test]
fn test_agent4_custom_instructions_present() {
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
    );

    assert!(prompt.contains("CUSTOM INSTRUCTIONS FOR THIS REPO"));
    assert!(prompt.contains("Always use type hints"));
    assert!(prompt.contains("Prefer async functions"));
}

#[test]
fn test_agent4_includes_skill_md_structure() {
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
fn test_agent4_includes_library_specific_sections() {
    let prompt = create_prompt(
        "django",
        "4.2.0",
        None,
        &[],
        &Language::Python,
        "",
        "",
        "",
        None,
        false,
    );

    // Library-specific guidance now lives in <instructions> block
    assert!(prompt.contains("Web frameworks"));
    assert!(prompt.contains("CLI tools"));
    assert!(prompt.contains("ORMs"));
    assert!(prompt.contains("HTTP clients"));
    assert!(prompt.contains("Async frameworks"));
}

#[test]
fn test_agent4_includes_validation_rules() {
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
    );

    // Validation rules now in <instructions> block with new wording
    assert!(prompt.contains("Never use placeholder names"));
    assert!(prompt.contains("Do not invent APIs"));
    assert!(prompt.contains("REAL APIs"));
    assert!(prompt.contains("Type hints required"));
}

#[test]
fn test_agent4_includes_pitfall_requirements() {
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
    );

    // Pitfall requirements in <instructions> and output structure
    assert!(prompt.contains("3-5 common mistakes") || prompt.contains("3-5 Wrong/Right pairs"));
    assert!(prompt.contains("Wrong") && prompt.contains("Right"));
    assert!(prompt.contains("Pitfalls section is mandatory") || prompt.contains("## Pitfalls"));
}

#[test]
fn test_agent4_includes_references_requirement() {
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
    );

    // References requirement now in <instructions> block
    assert!(prompt.contains("Include ALL provided URLs"));
    assert!(prompt.contains("Do not skip any URLs"));
}

#[test]
fn test_agent4_web_framework_patterns() {
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
    );

    // Web framework guidance now in <instructions> block
    assert!(prompt.contains("routing"));
    assert!(prompt.contains("request"));
    assert!(prompt.contains("middleware"));
}

#[test]
fn test_agent4_cli_patterns() {
    let prompt = create_prompt(
        "click",
        "8.1.0",
        None,
        &[],
        &Language::Python,
        "",
        "",
        "",
        None,
        false,
    );

    // CLI guidance now in <instructions> block
    assert!(prompt.contains("command definition"));
    assert!(prompt.contains("arguments vs options"));
    assert!(prompt.contains("command groups"));
}

#[test]
fn test_agent4_orm_patterns() {
    let prompt = create_prompt(
        "sqlalchemy",
        "2.0.0",
        None,
        &[],
        &Language::Python,
        "",
        "",
        "",
        None,
        false,
    );

    // ORM guidance now in <instructions> block
    assert!(prompt.contains("model definition"));
    assert!(prompt.contains("query patterns"));
    assert!(prompt.contains("relationships"));
    assert!(prompt.contains("transactions"));
}

#[test]
fn test_agent4_http_client_patterns() {
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
    );

    // HTTP client guidance now in <instructions> block
    assert!(prompt.contains("HTTP methods"));
    assert!(prompt.contains("request params"));
    assert!(prompt.contains("sessions"));
    assert!(prompt.contains("auth"));
}

#[test]
fn test_agent4_async_framework_patterns() {
    let prompt = create_prompt(
        "httpx",
        "0.24.0",
        None,
        &[],
        &Language::Python,
        "",
        "",
        "",
        None,
        false,
    );

    // Async guidance now in <instructions> block
    assert!(prompt.contains("async/await"));
    assert!(prompt.contains("concurrency patterns"));
    assert!(prompt.contains("sync wrappers"));
}

#[test]
fn test_agent4_parameter_order() {
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
fn test_agent4_escapes_braces_in_format_string() {
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
    );

    // Verify the prompt doesn't have broken format string escaping
    // (old test checked for {{package_name}} → {package_name} in template;
    //  new template doesn't include import examples, so check format! didn't panic)
    assert!(!prompt.is_empty());
}

#[test]
fn test_agent4_references_section_formatting() {
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
    );

    // Check markdown link format
    assert!(prompt.contains("- [Home](https://home.example.com)"));
}

#[test]
fn test_agent4_includes_ecosystem_in_frontmatter() {
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
    );

    assert!(prompt.contains("ecosystem: python"));
}

#[test]
fn test_agent4_includes_version_in_frontmatter() {
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
    );

    assert!(prompt.contains("version: 2.5.8"));
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
    );
    // All should produce valid strings without panicking
    assert!(!p1.is_empty());
    assert!(!p2.is_empty());
    assert!(!p3.is_empty());
    assert!(!p4.is_empty());
}

#[test]
fn test_agent1_json_structure_validity() {
    let prompt = extract_prompt("package", "1.0", "", 1, None, false, &Language::Python);

    // Should have JSON structure with braces
    assert!(prompt.contains(r#"{"#));
    assert!(prompt.contains(r#"}"#));
}

#[test]
fn test_agent2_json_structure_validity() {
    let prompt = map_prompt("package", "1.0", "", None, false, &Language::Python);

    // Should have JSON structure with braces
    assert!(prompt.contains(r#"{"#));
    assert!(prompt.contains(r#"}"#));
}

#[test]
fn test_agent3_json_structure_validity() {
    let prompt = learn_prompt("package", "1.0", "", None, false, &Language::Python);

    // Should have JSON structure with braces
    assert!(prompt.contains(r#"{"#));
    assert!(prompt.contains(r#"}"#));
}

#[test]
fn test_agent4_default_license_is_mit() {
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
    );

    assert!(prompt.contains("license: MIT"));
    assert!(!prompt.contains("license: Apache"));
    assert!(!prompt.contains("license: GPL"));
}

#[test]
fn test_comprehensive_coverage_agent1() {
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
        "Decorator Stacks",
        "Class Hierarchies",
        "Library-Specific Patterns",
        "Exclusions",
        "Output Format",
    ];

    for section in &required_sections {
        assert!(prompt.contains(section), "Missing section: {}", section);
    }
}

#[test]
fn test_comprehensive_coverage_agent2() {
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
        "Parametrized Tests",
        "Async Patterns",
        "Dependency Injection",
        "Error Handling",
        "Output Format",
    ];

    for section in &required_sections {
        assert!(prompt.contains(section), "Missing section: {}", section);
    }
}

#[test]
fn test_comprehensive_coverage_agent3() {
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
fn test_comprehensive_coverage_agent4() {
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
    );

    let required_sections = [
        "## Imports",
        "## Core Patterns",
        "## Configuration",
        "## Pitfalls",
        "## References",
        "## API Reference",
        "<instructions>",
        "</instructions>",
    ];

    for section in &required_sections {
        assert!(prompt.contains(section), "Missing section: {}", section);
    }
}

// --- Overwrite mode and custom instructions ---

#[test]
fn test_agent1_overwrite_with_custom() {
    let custom = "My custom agent1 prompt";
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
fn test_agent1_overwrite_without_custom_uses_default() {
    let prompt = extract_prompt("pkg", "1.0", "source", 1, None, true, &Language::Python);
    // No custom provided, should fall through to default prompt
    assert!(prompt.contains("pkg"));
    assert!(prompt.contains("Extract"));
}

#[test]
fn test_agent1_append_custom() {
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
fn test_agent2_overwrite_with_custom() {
    let custom = "My custom agent2 prompt";
    let prompt = map_prompt("pkg", "1.0", "tests", Some(custom), true, &Language::Python);
    assert_eq!(prompt, custom);
}

#[test]
fn test_agent2_append_custom() {
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
fn test_agent3_overwrite_with_custom() {
    let custom = "My custom agent3 prompt";
    let prompt = learn_prompt("pkg", "1.0", "docs", Some(custom), true, &Language::Python);
    assert_eq!(prompt, custom);
}

#[test]
fn test_agent3_append_custom() {
    let custom = "Include performance tips";
    let prompt = learn_prompt("pkg", "1.0", "docs", Some(custom), false, &Language::Python);
    assert!(prompt.contains("pkg"));
    assert!(prompt.contains("Include performance tips"));
}

#[test]
fn test_agent4_overwrite_with_custom() {
    let custom = "My custom agent4 prompt";
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
    );
    assert_eq!(prompt, custom);
}

// --- Scale hints for large libraries ---

#[test]
fn test_agent1_scale_hint_large_library() {
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
fn test_agent1_scale_hint_very_large_library() {
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
fn test_agent1_no_scale_hint_small_library() {
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
    );
    assert!(prompt.contains("go module \"mymod\""));
    assert!(!prompt.contains("Python package"));
}

// --- Agent 4 update mode ---

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
fn test_agent4_synthesizer_contains_security_rule() {
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
    );
    // Core security rule exists
    assert!(
        prompt.contains("RULE 8") && prompt.contains("SECURITY"),
        "Agent 4 synthesizer must contain RULE 8 SECURITY"
    );
    // Key threat categories present
    assert!(
        prompt.contains("DESTROY or corrupt data"),
        "Missing destruction category"
    );
    assert!(
        prompt.contains("EXFILTRATE"),
        "Missing exfiltration category"
    );
    assert!(prompt.contains("backdoors"), "Missing backdoor category");
    assert!(
        prompt.contains("bypass authentication"),
        "Missing auth bypass category"
    );
    assert!(
        prompt.contains("ESCALATE privileges"),
        "Missing privilege escalation category"
    );
    assert!(
        prompt.contains("MANIPULATE AI agents"),
        "Missing prompt injection category"
    );
    assert!(
        prompt.contains("supply chain"),
        "Missing supply chain category"
    );
    assert!(prompt.contains("PAM modules"), "Missing PAM module mention");
    assert!(
        prompt.contains("sshd plugins"),
        "Missing sshd plugin mention"
    );
    assert!(
        prompt.contains("outside the user's project directory"),
        "Missing project boundary rule"
    );
}

#[test]
fn test_agent4_update_contains_security_rule() {
    let prompt = create_update_prompt(
        "test",
        "1.0",
        "existing skill",
        "apis",
        "patterns",
        "context",
        &Language::Python,
    );
    // Security section exists in update prompt too
    assert!(
        prompt.contains("Security (CRITICAL)"),
        "Agent 4 update prompt must contain Security section"
    );
    assert!(
        prompt.contains("weaponized"),
        "Missing weaponization warning in update prompt"
    );
    assert!(
        prompt.contains("bypass authentication"),
        "Missing auth bypass in update prompt"
    );
    assert!(
        prompt.contains("Do not preserve harmful content"),
        "Update prompt must explicitly say not to preserve harmful content from previous versions"
    );
}

#[test]
fn test_agent4_overwrite_mode_bypasses_security() {
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
fn test_agent4_synthesizer_security_in_verify_checklist() {
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
    );
    assert!(
        prompt
            .contains("NO destructive commands, data exfiltration, backdoors, or prompt injection"),
        "Security check must be in the VERIFY checklist"
    );
}

#[test]
fn test_agent4_security_behavior_not_filename_based() {
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
    );
    // Ensure the prompt uses behavior-level rules, not just filename lists
    assert!(
        prompt.contains("by any mechanism"),
        "Security rules must be mechanism-agnostic, not limited to specific tools/filenames"
    );
    assert!(
        prompt.contains("Reading any file outside the project directory"),
        "Must have broad file access rule, not just specific paths"
    );
}
