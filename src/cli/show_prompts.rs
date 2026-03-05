//! `skilldo show-prompts` — dump the LLM prompts for inspection.

use crate::detector::Language;
use crate::llm::prompts_v2;
use crate::test_agent::code_generator::{build_test_prompt, TestEnv};
use crate::test_agent::go_code_gen::GO_ENV;
use crate::test_agent::python_code_gen::PYTHON_ENV;
use crate::test_agent::CodePattern;
use crate::test_agent::PatternCategory;

const STAGES: &[&str] = &["extract", "map", "learn", "create", "review", "test"];

/// Returns the canonical TestEnv for a language (reuses the real constants).
fn test_env_for(lang: &Language) -> &'static TestEnv {
    match lang {
        Language::Go => &GO_ENV,
        _ => &PYTHON_ENV,
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
                );
                println!("{prompt}");
            }
            "review" => {
                let prompt = prompts_v2::review_verdict_prompt(
                    "<SKILL_MD_CONTENT>",
                    "<INTROSPECTION_OUTPUT>",
                    None,
                    &language,
                );
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
