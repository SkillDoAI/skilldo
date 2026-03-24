//! `skilldo show-prompts` — dump the LLM prompts for inspection.

use crate::detector::Language;
use crate::llm::prompts_v2;
use crate::test_agent::code_generator::{build_test_prompt, TestEnv};
use crate::test_agent::go_code_gen::GO_ENV;
use crate::test_agent::java_code_gen::JAVA_ENV;
use crate::test_agent::js_code_gen::JS_ENV;
use crate::test_agent::python_code_gen::PYTHON_ENV;
use crate::test_agent::rust_code_gen::RUST_ENV;
use crate::test_agent::CodePattern;
use crate::test_agent::PatternCategory;

const STAGES: &[&str] = &["extract", "map", "learn", "create", "review", "test"];

/// Returns the canonical TestEnv for a language (reuses the real constants).
fn test_env_for(lang: &Language) -> &'static TestEnv {
    match lang {
        Language::Python => &PYTHON_ENV,
        Language::Go => &GO_ENV,
        Language::JavaScript => &JS_ENV,
        Language::Rust => &RUST_ENV,
        Language::Java => &JAVA_ENV,
    }
}

fn sample_pattern() -> CodePattern {
    CodePattern {
        name: "Basic Usage Example".to_string(),
        description: "Create a simple instance and call its main method".to_string(),
        code: "client = MyLib()\nresult = client.do_thing()\nprint(result)".to_string(),
        category: PatternCategory::BasicUsage,
    }
}

pub fn run(language_str: &str, stage_filter: Option<&str>) -> anyhow::Result<()> {
    let language: Language = language_str.parse()?;

    let stages: Vec<&str> = match stage_filter {
        Some(s) => {
            if !STAGES.contains(&s) {
                anyhow::bail!("Unknown stage: {s}. Valid: {}", STAGES.join(", "));
            }
            vec![s]
        }
        None => STAGES.to_vec(),
    };

    for stage in &stages {
        println!("═══ {} ({}) ═══\n", stage.to_uppercase(), language.as_str());

        match *stage {
            "extract" => {
                let prompt = prompts_v2::extract_prompt(
                    "<PACKAGE_NAME>",
                    "<VERSION>",
                    "<SOURCE_CODE — truncated>",
                    10,
                    None,
                    false,
                    &language,
                );
                println!("{prompt}");
            }
            "map" => {
                let prompt = prompts_v2::map_prompt(
                    "<PACKAGE_NAME>",
                    "<VERSION>",
                    "<EXTRACTION_OUTPUT>",
                    None,
                    false,
                    &language,
                );
                println!("{prompt}");
            }
            "learn" => {
                let prompt = prompts_v2::learn_prompt(
                    "<PACKAGE_NAME>",
                    "<VERSION>",
                    "<SOURCE_TESTS>",
                    None,
                    false,
                    &language,
                );
                println!("{prompt}");
            }
            "create" => {
                let prompt = prompts_v2::create_prompt(
                    "<PACKAGE_NAME>",
                    "<VERSION>",
                    Some("MIT"),
                    &[],
                    &language,
                    "<EXTRACT_OUTPUT>",
                    "<LEARN_OUTPUT>",
                    "<MAP_OUTPUT>",
                    None,
                    false,
                    &[],
                );
                println!("{prompt}");
            }
            "review" => {
                let prompt =
                    prompts_v2::review_verdict_prompt("<SKILL_MD_CONTENT>", None, &language, None);
                println!("{prompt}");
            }
            "test" => {
                let env = test_env_for(&language);
                let pattern = sample_pattern();
                let prompt = build_test_prompt(&pattern, env, None, None);
                println!("{prompt}");
            }
            _ => {}
        }

        // Show language-specific hints for this stage
        let hints = prompts_v2::language_hints(&language, stage);
        if !hints.is_empty() {
            println!("\n--- Language hints ({}) ---", language.as_str());
            println!("{hints}");
        }

        println!();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_all_stages_python() {
        run("python", None).unwrap();
    }

    #[test]
    fn run_all_stages_go() {
        run("go", None).unwrap();
    }

    #[test]
    fn run_all_stages_java() {
        run("java", None).unwrap();
    }

    #[test]
    fn run_single_stage_extract() {
        run("python", Some("extract")).unwrap();
    }

    #[test]
    fn run_single_stage_map() {
        run("python", Some("map")).unwrap();
    }

    #[test]
    fn run_single_stage_learn() {
        run("python", Some("learn")).unwrap();
    }

    #[test]
    fn run_single_stage_create() {
        run("python", Some("create")).unwrap();
    }

    #[test]
    fn run_single_stage_review() {
        run("python", Some("review")).unwrap();
    }

    #[test]
    fn run_single_stage_test() {
        run("python", Some("test")).unwrap();
    }

    #[test]
    fn run_go_test_stage() {
        run("go", Some("test")).unwrap();
    }

    #[test]
    fn run_invalid_stage_errors() {
        let err = run("python", Some("nonexistent")).unwrap_err();
        assert!(err.to_string().contains("Unknown stage"));
    }

    #[test]
    fn run_invalid_language_errors() {
        let err = run("cobol", None).unwrap_err();
        assert!(err.to_string().contains("cobol"));
    }

    #[test]
    fn test_env_for_go_returns_go_env() {
        let env = test_env_for(&Language::Go);
        assert_eq!(env.lang_tag, "go");
    }

    #[test]
    fn test_env_for_python_returns_python_env() {
        let env = test_env_for(&Language::Python);
        assert_eq!(env.lang_tag, "python");
    }

    #[test]
    fn run_all_stages_javascript() {
        run("javascript", None).unwrap();
    }

    #[test]
    fn run_all_stages_rust() {
        run("rust", None).unwrap();
    }

    #[test]
    fn test_env_for_javascript_returns_js_env() {
        let env = test_env_for(&Language::JavaScript);
        assert_eq!(env.lang_tag, "javascript");
    }

    #[test]
    fn test_env_for_rust_returns_rust_env() {
        let env = test_env_for(&Language::Rust);
        assert_eq!(env.lang_tag, "rust");
    }

    #[test]
    fn test_env_for_java_returns_java_env() {
        let env = test_env_for(&Language::Java);
        assert_eq!(env.lang_tag, "java");
    }

    #[test]
    fn sample_pattern_has_basic_category() {
        let pattern = sample_pattern();
        assert_eq!(pattern.category, PatternCategory::BasicUsage);
        assert!(!pattern.code.is_empty());
    }
}
