use skilldo::llm::prompts_v2::{
    agent1_api_extractor_v2, agent2_pattern_extractor_v2, agent3_context_extractor_v2,
    agent4_synthesizer_v2, agent4_update_v2, agent5_reviewer_v2,
};

#[test]
fn test_agent1_api_extractor_basic() {
    let prompt =
        agent1_api_extractor_v2("fastapi", "0.100.0", "class FastAPI: pass", 1, None, false);

    assert!(prompt.contains("fastapi"));
    assert!(prompt.contains("0.100.0"));
    assert!(prompt.contains("class FastAPI: pass"));
}

#[test]
fn test_agent1_includes_library_category_instructions() {
    let prompt = agent1_api_extractor_v2("requests", "2.31.0", "def get(): pass", 1, None, false);

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
    let prompt = agent1_api_extractor_v2("click", "8.1.0", "", 1, None, false);

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
    let prompt = agent1_api_extractor_v2("django", "4.2.0", "", 1, None, false);

    assert!(prompt.contains("120 characters"));
    assert!(prompt.contains("signature_truncated"));
    assert!(prompt.contains("optional"));
}

#[test]
fn test_agent1_includes_method_type_classification() {
    let prompt = agent1_api_extractor_v2("sqlalchemy", "2.0.0", "", 1, None, false);

    assert!(prompt.contains("function"));
    assert!(prompt.contains("method"));
    assert!(prompt.contains("classmethod"));
    assert!(prompt.contains("staticmethod"));
    assert!(prompt.contains("property"));
    assert!(prompt.contains("descriptor"));
}

#[test]
fn test_agent1_includes_type_hint_handling() {
    let prompt = agent1_api_extractor_v2("pydantic", "2.0.0", "", 1, None, false);

    assert!(prompt.contains("Annotated"));
    assert!(prompt.contains("Union"));
    assert!(prompt.contains("Optional"));
    assert!(prompt.contains("Generic"));
    assert!(prompt.contains("Callable"));
}

#[test]
fn test_agent1_includes_deprecation_tracking() {
    let prompt = agent1_api_extractor_v2("flask", "3.0.0", "", 1, None, false);

    assert!(prompt.contains("@deprecated"));
    assert!(prompt.contains("DeprecationWarning"));
    assert!(prompt.contains("since_version"));
    assert!(prompt.contains("removal_version"));
    assert!(prompt.contains("replacement"));
}

#[test]
fn test_agent1_includes_library_specific_patterns() {
    let prompt = agent1_api_extractor_v2("fastapi", "0.100.0", "", 1, None, false);

    assert!(prompt.contains("Web Frameworks"));
    assert!(prompt.contains("CLI Tools"));
    assert!(prompt.contains("ORMs"));
    assert!(prompt.contains("HTTP Clients"));
}

#[test]
fn test_agent1_excludes_private_apis() {
    let prompt = agent1_api_extractor_v2("package", "1.0.0", "", 1, None, false);

    // Verify prompt excludes private APIs
    assert!(prompt.contains("starting with `_`"));
    assert!(prompt.contains("`__all__`"));
}

#[test]
fn test_agent1_output_format() {
    let prompt = agent1_api_extractor_v2("numpy", "1.24.0", "", 1, None, false);

    assert!(prompt.contains("Return JSON"));
    assert!(prompt.contains(r#""library_category""#));
    assert!(prompt.contains(r#""apis""#));
    assert!(prompt.contains(r#""name""#));
    assert!(prompt.contains(r#""type""#));
    assert!(prompt.contains(r#""signature""#));
}

#[test]
fn test_agent2_pattern_extractor_basic() {
    let prompt = agent2_pattern_extractor_v2(
        "pytest",
        "7.4.0",
        "def test_something(): assert True",
        None,
        false,
    );

    assert!(prompt.contains("pytest"));
    assert!(prompt.contains("7.4.0"));
    assert!(prompt.contains("def test_something(): assert True"));
}

#[test]
fn test_agent2_includes_extraction_requirements() {
    let prompt = agent2_pattern_extractor_v2("click", "8.1.0", "", None, false);

    assert!(prompt.contains("API Being Tested"));
    assert!(prompt.contains("Setup Code"));
    assert!(prompt.contains("Usage Pattern"));
    assert!(prompt.contains("Assertions"));
    assert!(prompt.contains("Test Infrastructure"));
}

#[test]
fn test_agent2_includes_test_client_patterns() {
    let prompt = agent2_pattern_extractor_v2("fastapi", "0.100.0", "", None, false);

    assert!(prompt.contains("TestClient"));
    assert!(prompt.contains("CliRunner"));
    assert!(prompt.contains("Pytest fixtures"));
}

#[test]
fn test_agent2_includes_parametrized_tests() {
    let prompt = agent2_pattern_extractor_v2("package", "1.0.0", "", None, false);

    assert!(prompt.contains("@pytest.mark.parametrize"));
    assert!(prompt.contains("parameter combinations"));
}

#[test]
fn test_agent2_includes_async_patterns() {
    let prompt = agent2_pattern_extractor_v2("httpx", "0.24.0", "", None, false);

    assert!(prompt.contains("async def test_async"));
    assert!(prompt.contains("await"));
    assert!(prompt.contains("Mark patterns as async"));
}

#[test]
fn test_agent2_includes_dependency_injection() {
    let prompt = agent2_pattern_extractor_v2("fastapi", "0.100.0", "", None, false);

    assert!(prompt.contains("Depends"));
    assert!(prompt.contains("dependency patterns"));
}

#[test]
fn test_agent2_includes_error_handling() {
    let prompt = agent2_pattern_extractor_v2("requests", "2.31.0", "", None, false);

    assert!(prompt.contains("Error Handling"));
    assert!(prompt.contains("expected error responses"));
    assert!(prompt.contains("validation patterns"));
}

#[test]
fn test_agent2_output_format() {
    let prompt = agent2_pattern_extractor_v2("django", "4.2.0", "", None, false);

    assert!(prompt.contains("Return JSON"));
    assert!(prompt.contains("pattern"));
    assert!(prompt.contains("Setup Code"));
    assert!(prompt.contains("Usage Pattern"));
}

#[test]
fn test_agent3_context_extractor_basic() {
    let prompt = agent3_context_extractor_v2(
        "flask",
        "3.0.0",
        "# Breaking Changes\n- Removed old API",
        None,
        false,
    );

    assert!(prompt.contains("flask"));
    assert!(prompt.contains("3.0.0"));
    assert!(prompt.contains("# Breaking Changes"));
}

#[test]
fn test_agent3_includes_extraction_requirements() {
    let prompt = agent3_context_extractor_v2("django", "4.2.0", "", None, false);

    assert!(prompt.contains("CONVENTIONS"));
    assert!(prompt.contains("PITFALLS"));
    assert!(prompt.contains("BREAKING CHANGES"));
    assert!(prompt.contains("MIGRATION NOTES"));
}

#[test]
fn test_agent3_includes_pitfall_structure() {
    let prompt = agent3_context_extractor_v2("package", "1.0.0", "", None, false);

    assert!(prompt.contains("Wrong:"));
    assert!(prompt.contains("Why it fails:"));
    assert!(prompt.contains("Right:"));
}

#[test]
fn test_agent3_includes_breaking_change_structure() {
    let prompt = agent3_context_extractor_v2("sqlalchemy", "2.0.0", "", None, false);

    assert!(prompt.contains("version_from"));
    assert!(prompt.contains("version_to"));
    assert!(prompt.contains("change"));
    assert!(prompt.contains("migration"));
}

#[test]
fn test_agent3_includes_docstring_styles() {
    let prompt = agent3_context_extractor_v2("numpy", "1.24.0", "", None, false);

    assert!(prompt.contains("ReStructuredText"));
    assert!(prompt.contains("Google style"));
    assert!(prompt.contains("NumPy style"));
}

#[test]
fn test_agent3_includes_framework_specific_considerations() {
    let prompt = agent3_context_extractor_v2("django", "4.2.0", "", None, false);

    assert!(prompt.contains("Large Frameworks"));
    assert!(prompt.contains("CLI Tools"));
    assert!(prompt.contains("Async Frameworks"));
}

#[test]
fn test_agent3_output_format() {
    let prompt = agent3_context_extractor_v2("click", "8.1.0", "", None, false);

    assert!(prompt.contains("Return JSON"));
    assert!(prompt.contains(r#""conventions""#));
    assert!(prompt.contains(r#""pitfalls""#));
    assert!(prompt.contains(r#""breaking_changes""#));
    assert!(prompt.contains(r#""migration_notes""#));
}

#[test]
fn test_agent4_synthesizer_basic() {
    let prompt = agent4_synthesizer_v2(
        "requests",
        "2.31.0",
        Some("Apache-2.0"),
        &[],
        "python",
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
    let prompt = agent4_synthesizer_v2(
        "django",
        "4.2.0",
        Some("BSD-3-Clause"),
        &[],
        "python",
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
    let prompt = agent4_synthesizer_v2(
        "mypackage",
        "1.0.0",
        None,
        &[],
        "python",
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
    let prompt = agent4_synthesizer_v2(
        "package",
        "1.0.0",
        None,
        &[],
        "python",
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

    let prompt = agent4_synthesizer_v2(
        "package", "1.0.0", None, &urls, "python", "", "", "", None, false,
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

    let prompt = agent4_synthesizer_v2(
        "package", "1.0.0", None, &urls, "python", "", "", "", None, false,
    );

    assert!(prompt.contains("- [Documentation](https://docs.example.com)"));
    assert!(prompt.contains("- [GitHub](https://github.com/user/repo)"));
    assert!(prompt.contains("- [PyPI](https://pypi.org/project/package)"));
}

#[test]
fn test_agent4_custom_instructions_none() {
    let prompt = agent4_synthesizer_v2(
        "package",
        "1.0.0",
        None,
        &[],
        "python",
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

    let prompt = agent4_synthesizer_v2(
        "package",
        "1.0.0",
        None,
        &[],
        "python",
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
    let prompt = agent4_synthesizer_v2(
        "fastapi",
        "0.100.0",
        None,
        &[],
        "python",
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
    let prompt = agent4_synthesizer_v2(
        "django",
        "4.2.0",
        None,
        &[],
        "python",
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
    let prompt = agent4_synthesizer_v2(
        "requests",
        "2.31.0",
        None,
        &[],
        "python",
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
    let prompt = agent4_synthesizer_v2(
        "package",
        "1.0.0",
        None,
        &[],
        "python",
        "",
        "",
        "",
        None,
        false,
    );

    // Pitfall requirements now in <instructions> and template
    assert!(prompt.contains("3-5 common mistakes"));
    assert!(prompt.contains("### Wrong:"));
    assert!(prompt.contains("### Right:"));
    assert!(prompt.contains("Pitfalls section is mandatory"));
}

#[test]
fn test_agent4_includes_references_requirement() {
    let prompt = agent4_synthesizer_v2(
        "package",
        "1.0.0",
        None,
        &[],
        "python",
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
    let prompt = agent4_synthesizer_v2(
        "fastapi",
        "0.100.0",
        None,
        &[],
        "python",
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
    let prompt = agent4_synthesizer_v2(
        "click",
        "8.1.0",
        None,
        &[],
        "python",
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
    let prompt = agent4_synthesizer_v2(
        "sqlalchemy",
        "2.0.0",
        None,
        &[],
        "python",
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
    let prompt = agent4_synthesizer_v2(
        "requests",
        "2.31.0",
        None,
        &[],
        "python",
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
    let prompt = agent4_synthesizer_v2(
        "httpx",
        "0.24.0",
        None,
        &[],
        "python",
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

    let prompt = agent4_synthesizer_v2(
        "mypackage",
        "1.2.3",
        Some("GPL-3.0"),
        &urls,
        "python",
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
fn test_agent5_reviewer_basic() {
    let prompt = agent5_reviewer_v2("flask", "3.0.0", "def Flask(): pass", "# SKILL.md content");

    assert!(prompt.contains("flask"));
    assert!(prompt.contains("3.0.0"));
    assert!(prompt.contains("def Flask(): pass"));
    assert!(prompt.contains("# SKILL.md content"));
}

#[test]
fn test_agent5_includes_review_checklist() {
    let prompt = agent5_reviewer_v2("package", "1.0.0", "", "");

    assert!(prompt.contains("Review Checklist"));
    assert!(prompt.contains("API Accuracy"));
    assert!(prompt.contains("Code Completeness"));
    assert!(prompt.contains("Library-Specific Validation"));
    assert!(prompt.contains("Pattern Correctness"));
    assert!(prompt.contains("Pitfalls Section"));
    assert!(prompt.contains("Factual Accuracy"));
    assert!(prompt.contains("Completeness"));
}

#[test]
fn test_agent5_api_accuracy_checks() {
    let prompt = agent5_reviewer_v2("django", "4.2.0", "", "");

    assert!(prompt.contains("hallucinated API"));
    assert!(prompt.contains("API signatures correct"));
    assert!(prompt.contains("Parameter names must match"));
    assert!(prompt.contains("Type hints must match"));
    assert!(prompt.contains("Default values must match"));
}

#[test]
fn test_agent5_code_completeness_checks() {
    let prompt = agent5_reviewer_v2("requests", "2.31.0", "", "");

    assert!(prompt.contains("run standalone"));
    assert!(prompt.contains("All imports present"));
    assert!(prompt.contains("All required parameters"));
    assert!(prompt.contains("Valid Python syntax"));
    assert!(prompt.contains("No placeholder names"));
}

#[test]
fn test_agent5_library_specific_validation() {
    let prompt = agent5_reviewer_v2("fastapi", "0.100.0", "", "");

    assert!(prompt.contains("Web framework MUST show routing"));
    assert!(prompt.contains("CLI tool MUST show command"));
    assert!(prompt.contains("ORM MUST show model"));
    assert!(prompt.contains("HTTP client MUST show request"));
}

#[test]
fn test_agent5_pattern_correctness_checks() {
    let prompt = agent5_reviewer_v2("httpx", "0.24.0", "", "");

    assert!(prompt.contains("async functions use await"));
    assert!(prompt.contains("decorators in the right order"));
    assert!(prompt.contains("error handling shown correctly"));
    assert!(prompt.contains("type hints used correctly"));
}

#[test]
fn test_agent5_pitfalls_section_requirements() {
    let prompt = agent5_reviewer_v2("package", "1.0.0", "", "");

    assert!(prompt.contains("Do \"Wrong\" examples actually demonstrate"));
    assert!(prompt.contains("Do \"Right\" examples actually solve"));
    assert!(prompt.contains("At least 3 pitfalls"));
    assert!(prompt.contains("Fewer than 3 = FAIL"));
}

#[test]
fn test_agent5_strict_failure_criteria() {
    let prompt = agent5_reviewer_v2("click", "8.1.0", "", "");

    assert!(prompt.contains("STRICT FAILURE CRITERIA"));
    assert!(prompt.contains("MUST FAIL the review"));
    assert!(prompt.contains("ANY API used that is NOT in api_surface"));
    assert!(prompt.contains("Generic placeholder names"));
    assert!(prompt.contains("Pitfalls section has fewer than 3"));
}

#[test]
fn test_agent5_output_format_pass() {
    let prompt = agent5_reviewer_v2("numpy", "1.24.0", "", "");

    assert!(prompt.contains("If ALL checks pass"));
    assert!(prompt.contains(r#"{"status": "pass"}"#));
}

#[test]
fn test_agent5_output_format_fail() {
    let prompt = agent5_reviewer_v2("pandas", "2.0.0", "", "");

    assert!(prompt.contains("If ANY fail"));
    assert!(prompt.contains(r#""status": "fail""#));
    assert!(prompt.contains(r#""issues""#));
    assert!(prompt.contains(r#""type""#));
    assert!(prompt.contains(r#""location""#));
    assert!(prompt.contains(r#""problem""#));
    assert!(prompt.contains(r#""fix""#));
}

#[test]
fn test_agent5_issue_types() {
    let prompt = agent5_reviewer_v2("package", "1.0.0", "", "");

    assert!(prompt.contains("hallucinated_api"));
    assert!(prompt.contains("incomplete_code"));
    assert!(prompt.contains("incorrect_syntax"));
}

#[test]
fn test_all_agents_include_package_name_and_version() {
    let package = "testpkg";
    let version = "1.2.3";

    let p1 = agent1_api_extractor_v2(package, version, "", 1, None, false);
    let p2 = agent2_pattern_extractor_v2(package, version, "", None, false);
    let p3 = agent3_context_extractor_v2(package, version, "", None, false);
    let p4 = agent4_synthesizer_v2(
        package,
        version,
        None,
        &[],
        "python",
        "",
        "",
        "",
        None,
        false,
    );
    let p5 = agent5_reviewer_v2(package, version, "", "");

    for prompt in [p1, p2, p3, p4, p5] {
        assert!(prompt.contains(package));
        assert!(prompt.contains(version));
    }
}

#[test]
fn test_template_rendering_with_special_characters() {
    let source = "def func():\n    '''Docstring with \"quotes\" and {braces}'''";
    let prompt = agent1_api_extractor_v2("pkg", "1.0", source, 1, None, false);

    assert!(prompt.contains(source));
}

#[test]
fn test_agent4_escapes_braces_in_format_string() {
    let prompt = agent4_synthesizer_v2(
        "fastapi",
        "0.100.0",
        None,
        &[],
        "python",
        "",
        "",
        "",
        None,
        false,
    );

    // format! converts {{ to { in output — check {package_name} placeholder renders correctly
    assert!(prompt.contains("{package_name}"));
}

#[test]
fn test_agent4_references_section_formatting() {
    let urls = vec![("Home".to_string(), "https://home.example.com".to_string())];

    let prompt = agent4_synthesizer_v2(
        "package", "1.0.0", None, &urls, "python", "", "", "", None, false,
    );

    // Check markdown link format
    assert!(prompt.contains("- [Home](https://home.example.com)"));
}

#[test]
fn test_agent4_includes_ecosystem_in_frontmatter() {
    let prompt = agent4_synthesizer_v2(
        "package",
        "1.0.0",
        None,
        &[],
        "python",
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
    let prompt =
        agent4_synthesizer_v2("mylib", "2.5.8", None, &[], "rust", "", "", "", None, false);

    assert!(prompt.contains("version: 2.5.8"));
}

#[test]
fn test_empty_inputs_handled_gracefully() {
    let p1 = agent1_api_extractor_v2("", "", "", 1, None, false);
    let p2 = agent2_pattern_extractor_v2("", "", "", None, false);
    let p3 = agent3_context_extractor_v2("", "", "", None, false);
    let p4 = agent4_synthesizer_v2("", "", None, &[], "", "", "", "", None, false);
    let p5 = agent5_reviewer_v2("", "", "", "");

    // All should produce valid strings without panicking
    assert!(!p1.is_empty());
    assert!(!p2.is_empty());
    assert!(!p3.is_empty());
    assert!(!p4.is_empty());
    assert!(!p5.is_empty());
}

#[test]
fn test_agent1_json_structure_validity() {
    let prompt = agent1_api_extractor_v2("package", "1.0", "", 1, None, false);

    // Should have JSON structure with braces
    assert!(prompt.contains(r#"{"#));
    assert!(prompt.contains(r#"}"#));
}

#[test]
fn test_agent2_json_structure_validity() {
    let prompt = agent2_pattern_extractor_v2("package", "1.0", "", None, false);

    // Should have JSON structure with braces
    assert!(prompt.contains(r#"{"#));
    assert!(prompt.contains(r#"}"#));
}

#[test]
fn test_agent3_json_structure_validity() {
    let prompt = agent3_context_extractor_v2("package", "1.0", "", None, false);

    // Should have JSON structure with braces
    assert!(prompt.contains(r#"{"#));
    assert!(prompt.contains(r#"}"#));
}

#[test]
fn test_agent5_json_structure_validity() {
    let prompt = agent5_reviewer_v2("package", "1.0", "", "");

    // Should have JSON structure with braces
    assert!(prompt.contains(r#"{"#));
    assert!(prompt.contains(r#"}"#));
}

#[test]
fn test_agent4_default_license_is_mit() {
    let prompt = agent4_synthesizer_v2(
        "unlicensed",
        "1.0.0",
        None,
        &[],
        "python",
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
    let prompt =
        agent1_api_extractor_v2("comprehensive_test", "1.0.0", "test_code", 1, None, false);

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
    let prompt =
        agent2_pattern_extractor_v2("comprehensive_test", "1.0.0", "test_code", None, false);

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
    let prompt = agent3_context_extractor_v2("comprehensive_test", "1.0.0", "docs", None, false);

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
    let prompt = agent4_synthesizer_v2(
        "comprehensive_test",
        "1.0.0",
        Some("MIT"),
        &[],
        "python",
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

#[test]
fn test_comprehensive_coverage_agent5() {
    let prompt = agent5_reviewer_v2("comprehensive_test", "1.0.0", "api", "rules");

    let required_sections = [
        "API Accuracy",
        "Code Completeness",
        "Library-Specific Validation",
        "Pattern Correctness",
        "Pitfalls Section",
        "Factual Accuracy",
        "Completeness",
        "STRICT FAILURE CRITERIA",
        "Output Format",
    ];

    for section in &required_sections {
        assert!(prompt.contains(section), "Missing section: {}", section);
    }
}

// --- Overwrite mode and custom instructions ---

#[test]
fn test_agent1_overwrite_with_custom() {
    let custom = "My custom agent1 prompt";
    let prompt = agent1_api_extractor_v2("pkg", "1.0", "source", 1, Some(custom), true);
    assert_eq!(prompt, custom);
}

#[test]
fn test_agent1_overwrite_without_custom_uses_default() {
    let prompt = agent1_api_extractor_v2("pkg", "1.0", "source", 1, None, true);
    // No custom provided, should fall through to default prompt
    assert!(prompt.contains("pkg"));
    assert!(prompt.contains("Extract"));
}

#[test]
fn test_agent1_append_custom() {
    let custom = "Also extract internal APIs";
    let prompt = agent1_api_extractor_v2("pkg", "1.0", "source", 1, Some(custom), false);
    // Custom should be appended, default prompt still present
    assert!(prompt.contains("pkg"));
    assert!(prompt.contains("Also extract internal APIs"));
}

#[test]
fn test_agent2_overwrite_with_custom() {
    let custom = "My custom agent2 prompt";
    let prompt = agent2_pattern_extractor_v2("pkg", "1.0", "tests", Some(custom), true);
    assert_eq!(prompt, custom);
}

#[test]
fn test_agent2_append_custom() {
    let custom = "Focus on error patterns";
    let prompt = agent2_pattern_extractor_v2("pkg", "1.0", "tests", Some(custom), false);
    assert!(prompt.contains("pkg"));
    assert!(prompt.contains("Focus on error patterns"));
}

#[test]
fn test_agent3_overwrite_with_custom() {
    let custom = "My custom agent3 prompt";
    let prompt = agent3_context_extractor_v2("pkg", "1.0", "docs", Some(custom), true);
    assert_eq!(prompt, custom);
}

#[test]
fn test_agent3_append_custom() {
    let custom = "Include performance tips";
    let prompt = agent3_context_extractor_v2("pkg", "1.0", "docs", Some(custom), false);
    assert!(prompt.contains("pkg"));
    assert!(prompt.contains("Include performance tips"));
}

#[test]
fn test_agent4_overwrite_with_custom() {
    let custom = "My custom agent4 prompt";
    let prompt = agent4_synthesizer_v2(
        "pkg",
        "1.0",
        None,
        &[],
        "python",
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
    let prompt = agent1_api_extractor_v2("biglib", "1.0", "source", 1500, None, false);
    assert!(prompt.contains("LARGE LIBRARY"));
    assert!(prompt.contains("1000+ files"));
}

#[test]
fn test_agent1_scale_hint_very_large_library() {
    let prompt = agent1_api_extractor_v2("hugelib", "1.0", "source", 3000, None, false);
    assert!(prompt.contains("LARGE LIBRARY ALERT"));
    assert!(prompt.contains("2000+ files"));
}

#[test]
fn test_agent1_no_scale_hint_small_library() {
    let prompt = agent1_api_extractor_v2("smalllib", "1.0", "source", 50, None, false);
    assert!(!prompt.contains("LARGE LIBRARY"));
}

// --- Agent 4 update mode ---

#[test]
fn test_agent4_update_v2_basic() {
    let prompt = agent4_update_v2(
        "requests",
        "2.32.0",
        "# Existing SKILL.md content",
        "API surface",
        "Patterns",
        "Context",
    );
    assert!(prompt.contains("requests"));
    assert!(prompt.contains("2.32.0"));
    assert!(prompt.contains("# Existing SKILL.md content"));
    assert!(prompt.contains("API surface"));
}
