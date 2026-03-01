use anyhow::Result;
use clap::{Parser, Subcommand};

mod changelog;
mod cli;
mod config;
mod detector;
mod ecosystems;
mod lint;
mod llm;
mod pipeline;
mod review;
mod test_agent;
mod util;
mod validator;

#[derive(Parser)]
#[command(name = "skilldo", version)]
#[command(about = "Generate agent rules files for open source libraries", long_about = None)]
struct Cli {
    /// Suppress informational output (only show warnings and errors)
    #[arg(short, long, global = true)]
    quiet: bool,

    /// Show detailed debug output
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
enum Commands {
    /// Generate SKILL.md rules file for a repository
    Generate {
        /// Repository path (defaults to current directory)
        #[arg(default_value = ".")]
        path: String,

        /// Language/ecosystem (python, javascript, rust, go). Auto-detected if not specified.
        #[arg(long)]
        language: Option<String>,

        /// Input SKILL.md to update (updates in-place if output exists)
        #[arg(short = 'i', long = "input")]
        input: Option<String>,

        /// Output file path
        #[arg(short = 'o', long, default_value = "SKILL.md")]
        output: String,

        /// Explicit version override (e.g., "2.1.0")
        #[arg(long)]
        version: Option<String>,

        /// Version extraction strategy: git-tag, package, branch, commit
        #[arg(long)]
        version_from: Option<String>,

        /// Path to config file (defaults to ~/.config/skilldo/config.toml or ./skilldo.toml)
        #[arg(long)]
        config: Option<String>,

        /// Override LLM model (e.g., "gpt-5.2", "claude-sonnet-4-5-20250929")
        #[arg(long)]
        model: Option<String>,

        /// LLM provider: anthropic, openai, gemini, openai-compatible
        #[arg(long)]
        provider: Option<String>,

        /// Base URL for openai-compatible providers (e.g., http://localhost:11434/v1)
        #[arg(long)]
        base_url: Option<String>,

        /// Override max generation retries (default: from config)
        #[arg(long)]
        max_retries: Option<usize>,

        /// Override test stage LLM model (alias: --agent5-model)
        #[arg(long = "test-model", visible_alias = "agent5-model")]
        test_model: Option<String>,

        /// Override test stage LLM provider (alias: --agent5-provider)
        #[arg(long = "test-provider", visible_alias = "agent5-provider")]
        test_provider: Option<String>,

        /// Disable test stage validation (alias: --no-agent5)
        #[arg(long = "no-test", visible_alias = "no-agent5")]
        no_test: bool,

        /// Test stage validation mode: thorough, adaptive, minimal (alias: --agent5-mode)
        #[arg(long = "test-mode", visible_alias = "agent5-mode")]
        test_mode: Option<String>,

        /// Container runtime: docker or podman
        #[arg(long)]
        runtime: Option<String>,

        /// Container execution timeout in seconds
        #[arg(long)]
        timeout: Option<u64>,

        /// Test stage install source: registry, local-install, local-mount
        #[arg(long)]
        install_source: Option<String>,

        /// Path to local source for local-install/local-mount modes
        #[arg(long)]
        source_path: Option<String>,

        /// Run agents 1-3 sequentially instead of in parallel
        #[arg(long)]
        no_parallel: bool,

        /// Use mock LLM client for testing
        #[arg(long)]
        dry_run: bool,
    },

    /// Lint a SKILL.md file for errors
    Lint {
        /// Path to SKILL.md file
        path: String,
    },

    /// Review an existing SKILL.md for accuracy and safety issues
    Review {
        /// Path to SKILL.md file
        path: String,

        /// Path to config file
        #[arg(long)]
        config: Option<String>,

        /// Override LLM model
        #[arg(long)]
        model: Option<String>,

        /// LLM provider: anthropic, openai, gemini, openai-compatible
        #[arg(long)]
        provider: Option<String>,

        /// Base URL for openai-compatible providers
        #[arg(long)]
        base_url: Option<String>,

        /// Container runtime: docker or podman
        #[arg(long)]
        runtime: Option<String>,

        /// Container execution timeout in seconds
        #[arg(long)]
        timeout: Option<u64>,

        /// Skip container introspection (LLM-only review)
        #[arg(long)]
        no_container: bool,

        /// Use mock LLM client for testing
        #[arg(long)]
        dry_run: bool,
    },

    /// Validate configuration file
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Check configuration for errors
    Check {
        /// Path to config file
        #[arg(long)]
        config: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging based on flags
    let log_level = if cli.quiet {
        tracing::Level::WARN
    } else if cli.verbose {
        tracing::Level::DEBUG
    } else {
        tracing::Level::INFO
    };

    tracing_subscriber::fmt().with_max_level(log_level).init();

    match cli.command {
        Commands::Generate {
            path,
            language,
            input,
            output,
            version,
            version_from,
            config,
            model,
            provider,
            base_url,
            max_retries,
            test_model,
            test_provider,
            no_test,
            test_mode,
            runtime,
            timeout,
            install_source,
            source_path,
            no_parallel,
            dry_run,
        } => {
            cli::generate::run(
                path,
                language,
                input,
                output,
                version,
                version_from,
                config,
                model,
                provider,
                base_url,
                max_retries,
                test_model,
                test_provider,
                no_test,
                test_mode,
                runtime,
                timeout,
                install_source,
                source_path,
                no_parallel,
                dry_run,
            )
            .await?;
        }
        Commands::Lint { path } => {
            cli::lint::run(&path)?;
        }
        Commands::Review {
            path,
            config,
            model,
            provider,
            base_url,
            runtime,
            timeout,
            no_container,
            dry_run,
        } => {
            cli::review::run(
                path,
                config,
                model,
                provider,
                base_url,
                runtime,
                timeout,
                no_container,
                dry_run,
            )
            .await?;
        }
        Commands::Config { action } => match action {
            ConfigAction::Check { config } => {
                cli::config_check::run(config)?;
            }
        },
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    // Helper macros: extract enum fields or panic. Using macros avoids
    // per-test `_ => panic!()` / `let...else` branches that llvm-cov counts
    // as uncoverable lines.
    macro_rules! assert_generate {
        ($cli:expr, |$($field:ident),+ $(,)?| $body:block) => {
            let Commands::Generate { $($field),+, .. } = $cli.command else {
                panic!("Expected Generate command");
            };
            $body
        };
    }

    macro_rules! assert_review {
        ($cli:expr, |$($field:ident),+ $(,)?| $body:block) => {
            let Commands::Review { $($field),+, .. } = $cli.command else {
                panic!("Expected Review command");
            };
            $body
        };
    }

    macro_rules! assert_lint {
        ($cli:expr, |$($field:ident),+ $(,)?| $body:block) => {
            let Commands::Lint { $($field),+ } = $cli.command else {
                panic!("Expected Lint command");
            };
            $body
        };
    }

    macro_rules! assert_config_check {
        ($cli:expr, |$field:ident| $body:block) => {
            let Commands::Config { action } = $cli.command else {
                panic!("Expected Config command");
            };
            let ConfigAction::Check { $field } = action;
            $body
        };
    }

    #[test]
    fn test_parse_generate_defaults() {
        let cli = Cli::try_parse_from(["skilldo", "generate"]).unwrap();
        assert_generate!(cli, |path, language, output, dry_run| {
            assert_eq!(path, ".");
            assert!(language.is_none());
            assert_eq!(output, "SKILL.md");
            assert!(!dry_run);
        });
    }

    #[test]
    fn test_parse_generate_with_all_args() {
        let cli = Cli::try_parse_from([
            "skilldo",
            "generate",
            "/tmp/repo",
            "--language",
            "python",
            "--output",
            "out.md",
            "--model",
            "gpt-5.2",
            "--max-retries",
            "5",
            "--version",
            "2.0.0",
            "--dry-run",
        ])
        .unwrap();
        assert_generate!(cli, |path,
                               language,
                               output,
                               model,
                               max_retries,
                               version,
                               dry_run| {
            assert_eq!(path, "/tmp/repo");
            assert_eq!(language.unwrap(), "python");
            assert_eq!(output, "out.md");
            assert_eq!(model.unwrap(), "gpt-5.2");
            assert_eq!(max_retries.unwrap(), 5);
            assert_eq!(version.unwrap(), "2.0.0");
            assert!(dry_run);
        });
    }

    #[test]
    fn test_parse_generate_with_input() {
        let cli = Cli::try_parse_from([
            "skilldo",
            "generate",
            ".",
            "-i",
            "old-SKILL.md",
            "-o",
            "new-SKILL.md",
        ])
        .unwrap();
        assert_generate!(cli, |input, output| {
            assert_eq!(input.unwrap(), "old-SKILL.md");
            assert_eq!(output, "new-SKILL.md");
        });
    }

    #[test]
    fn test_parse_generate_version_from() {
        let cli =
            Cli::try_parse_from(["skilldo", "generate", "--version-from", "git-tag"]).unwrap();
        assert_generate!(cli, |version_from| {
            assert_eq!(version_from.unwrap(), "git-tag");
        });
    }

    #[test]
    fn test_parse_missing_subcommand() {
        let result = Cli::try_parse_from(["skilldo"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_unknown_subcommand() {
        let result = Cli::try_parse_from(["skilldo", "foobar"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_lint() {
        let cli = Cli::try_parse_from(["skilldo", "lint", "SKILL.md"]).unwrap();
        assert_lint!(cli, |path| {
            assert_eq!(path, "SKILL.md");
        });
    }

    #[test]
    fn test_parse_lint_missing_path() {
        let result = Cli::try_parse_from(["skilldo", "lint"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_config_check() {
        let cli = Cli::try_parse_from(["skilldo", "config", "check"]).unwrap();
        assert_config_check!(cli, |config| {
            assert!(config.is_none());
        });
    }

    #[test]
    fn test_parse_config_check_with_path() {
        let cli =
            Cli::try_parse_from(["skilldo", "config", "check", "--config", "my.toml"]).unwrap();
        assert_config_check!(cli, |config| {
            assert_eq!(config.unwrap(), "my.toml");
        });
    }

    #[test]
    fn test_parse_quiet_flag() {
        let cli = Cli::try_parse_from(["skilldo", "-q", "generate"]).unwrap();
        assert!(cli.quiet);
        assert!(!cli.verbose);
    }

    #[test]
    fn test_parse_verbose_flag() {
        let cli = Cli::try_parse_from(["skilldo", "--verbose", "generate"]).unwrap();
        assert!(!cli.quiet);
        assert!(cli.verbose);
    }

    #[test]
    fn test_parse_generate_with_provider() {
        let cli = Cli::try_parse_from([
            "skilldo",
            "generate",
            "--provider",
            "openai",
            "--base-url",
            "http://localhost:11434/v1",
        ])
        .unwrap();
        assert_generate!(cli, |provider, base_url| {
            assert_eq!(provider.unwrap(), "openai");
            assert_eq!(base_url.unwrap(), "http://localhost:11434/v1");
        });
    }

    #[test]
    fn test_parse_generate_no_test() {
        let cli = Cli::try_parse_from(["skilldo", "generate", "--no-test"]).unwrap();
        assert_generate!(cli, |no_test| {
            assert!(no_test);
        });
    }

    #[test]
    fn test_parse_generate_test_overrides() {
        let cli = Cli::try_parse_from([
            "skilldo",
            "generate",
            "--test-model",
            "gpt-5.2",
            "--test-provider",
            "openai",
        ])
        .unwrap();
        assert_generate!(cli, |test_model, test_provider| {
            assert_eq!(test_model.unwrap(), "gpt-5.2");
            assert_eq!(test_provider.unwrap(), "openai");
        });
    }

    #[test]
    fn test_parse_generate_runtime_timeout() {
        let cli = Cli::try_parse_from([
            "skilldo",
            "generate",
            "--runtime",
            "podman",
            "--timeout",
            "300",
        ])
        .unwrap();
        assert_generate!(cli, |runtime, timeout| {
            assert_eq!(runtime.unwrap(), "podman");
            assert_eq!(timeout.unwrap(), 300);
        });
    }

    #[test]
    fn test_parse_generate_install_source() {
        let cli = Cli::try_parse_from([
            "skilldo",
            "generate",
            "--install-source",
            "local-mount",
            "--source-path",
            "/tmp/mylib",
        ])
        .unwrap();
        assert_generate!(cli, |install_source, source_path| {
            assert_eq!(install_source.unwrap(), "local-mount");
            assert_eq!(source_path.unwrap(), "/tmp/mylib");
        });
    }

    #[test]
    fn test_parse_generate_all_new_flags() {
        let cli = Cli::try_parse_from([
            "skilldo",
            "generate",
            "/tmp/repo",
            "--language",
            "python",
            "--provider",
            "openai",
            "--model",
            "gpt-5.2",
            "--base-url",
            "http://localhost:11434/v1",
            "--max-retries",
            "10",
            "--test-model",
            "gpt-5.2",
            "--test-provider",
            "openai",
            "--no-test",
            "--test-mode",
            "minimal",
            "--runtime",
            "podman",
            "--timeout",
            "300",
            "--install-source",
            "local-mount",
            "--source-path",
            "/tmp/mylib",
            "--no-parallel",
            "--dry-run",
        ])
        .unwrap();
        assert_generate!(cli, |path,
                               language,
                               input,
                               output,
                               version,
                               version_from,
                               config,
                               model,
                               provider,
                               base_url,
                               max_retries,
                               test_model,
                               test_provider,
                               no_test,
                               test_mode,
                               runtime,
                               timeout,
                               install_source,
                               source_path,
                               no_parallel,
                               dry_run| {
            assert_eq!(path, "/tmp/repo");
            assert_eq!(language.unwrap(), "python");
            assert!(input.is_none());
            assert_eq!(output, "SKILL.md");
            assert!(version.is_none());
            assert!(version_from.is_none());
            assert!(config.is_none());
            assert_eq!(model.unwrap(), "gpt-5.2");
            assert_eq!(provider.unwrap(), "openai");
            assert_eq!(base_url.unwrap(), "http://localhost:11434/v1");
            assert_eq!(max_retries.unwrap(), 10);
            assert_eq!(test_model.unwrap(), "gpt-5.2");
            assert_eq!(test_provider.unwrap(), "openai");
            assert!(no_test);
            assert_eq!(test_mode.unwrap(), "minimal");
            assert_eq!(runtime.unwrap(), "podman");
            assert_eq!(timeout.unwrap(), 300);
            assert_eq!(install_source.unwrap(), "local-mount");
            assert!(no_parallel);
            assert_eq!(source_path.unwrap(), "/tmp/mylib");
            assert!(dry_run);
        });
    }

    #[test]
    fn test_parse_generate_provider_only() {
        let cli = Cli::try_parse_from(["skilldo", "generate", "--provider", "openai-compatible"])
            .unwrap();
        assert_generate!(cli, |provider| {
            assert_eq!(provider.unwrap(), "openai-compatible");
        });
    }

    #[test]
    fn test_parse_generate_base_url_only() {
        let cli = Cli::try_parse_from([
            "skilldo",
            "generate",
            "--base-url",
            "http://localhost:11434/v1",
        ])
        .unwrap();
        assert_generate!(cli, |base_url| {
            assert_eq!(base_url.unwrap(), "http://localhost:11434/v1");
        });
    }

    #[test]
    fn test_parse_generate_test_model_only() {
        let cli = Cli::try_parse_from(["skilldo", "generate", "--test-model", "gpt-5.2"]).unwrap();
        assert_generate!(cli, |test_model| {
            assert_eq!(test_model.unwrap(), "gpt-5.2");
        });
    }

    #[test]
    fn test_parse_generate_test_provider_only() {
        let cli =
            Cli::try_parse_from(["skilldo", "generate", "--test-provider", "anthropic"]).unwrap();
        assert_generate!(cli, |test_provider| {
            assert_eq!(test_provider.unwrap(), "anthropic");
        });
    }

    #[test]
    fn test_parse_generate_install_source_local_install() {
        let cli = Cli::try_parse_from([
            "skilldo",
            "generate",
            "--install-source",
            "local-install",
            "--source-path",
            "/tmp/lib",
        ])
        .unwrap();
        assert_generate!(cli, |install_source, source_path| {
            assert_eq!(install_source.unwrap(), "local-install");
            assert_eq!(source_path.unwrap(), "/tmp/lib");
        });
    }

    #[test]
    fn test_parse_generate_timeout_only() {
        let cli = Cli::try_parse_from(["skilldo", "generate", "--timeout", "600"]).unwrap();
        assert_generate!(cli, |timeout| {
            assert_eq!(timeout.unwrap(), 600);
        });
    }

    #[test]
    fn test_parse_generate_test_mode_only() {
        let cli = Cli::try_parse_from(["skilldo", "generate", "--test-mode", "adaptive"]).unwrap();
        assert_generate!(cli, |test_mode| {
            assert_eq!(test_mode.unwrap(), "adaptive");
        });
    }

    #[test]
    fn test_parse_quiet_with_generate() {
        let cli = Cli::try_parse_from(["skilldo", "-q", "generate"]).unwrap();
        assert!(cli.quiet);
        assert!(!cli.verbose);
        assert!(matches!(cli.command, Commands::Generate { .. }));
    }

    #[test]
    fn test_parse_verbose_with_config_check() {
        let cli = Cli::try_parse_from(["skilldo", "-v", "config", "check"]).unwrap();
        assert!(cli.verbose);
        assert!(!cli.quiet);
        assert_config_check!(cli, |config| {
            assert!(config.is_none());
        });
    }

    #[test]
    fn test_parse_quiet_and_verbose_both() {
        let cli = Cli::try_parse_from(["skilldo", "-q", "-v", "generate"]).unwrap();
        assert!(cli.quiet);
        assert!(cli.verbose);
    }

    #[test]
    fn test_parse_review() {
        let cli = Cli::try_parse_from(["skilldo", "review", "SKILL.md"]).unwrap();
        assert_review!(cli, |path,
                             config,
                             model,
                             provider,
                             base_url,
                             runtime,
                             timeout,
                             no_container,
                             dry_run| {
            assert_eq!(path, "SKILL.md");
            assert!(config.is_none());
            assert!(model.is_none());
            assert!(provider.is_none());
            assert!(base_url.is_none());
            assert!(runtime.is_none());
            assert!(timeout.is_none());
            assert!(!no_container);
            assert!(!dry_run);
        });
    }

    #[test]
    fn test_parse_review_missing_path() {
        let result = Cli::try_parse_from(["skilldo", "review"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_review_with_all_flags() {
        let cli = Cli::try_parse_from([
            "skilldo",
            "review",
            "test.md",
            "--config",
            "my.toml",
            "--model",
            "codestral:latest",
            "--provider",
            "openai-compatible",
            "--base-url",
            "http://localhost:11434/v1",
            "--runtime",
            "podman",
            "--timeout",
            "120",
            "--no-container",
            "--dry-run",
        ])
        .unwrap();
        assert_review!(cli, |path,
                             config,
                             model,
                             provider,
                             base_url,
                             runtime,
                             timeout,
                             no_container,
                             dry_run| {
            assert_eq!(path, "test.md");
            assert_eq!(config.unwrap(), "my.toml");
            assert_eq!(model.unwrap(), "codestral:latest");
            assert_eq!(provider.unwrap(), "openai-compatible");
            assert_eq!(base_url.unwrap(), "http://localhost:11434/v1");
            assert_eq!(runtime.unwrap(), "podman");
            assert_eq!(timeout.unwrap(), 120);
            assert!(no_container);
            assert!(dry_run);
        });
    }

    #[test]
    fn test_parse_review_dry_run_only() {
        let cli = Cli::try_parse_from(["skilldo", "review", "SKILL.md", "--dry-run"]).unwrap();
        assert_review!(cli, |dry_run| {
            assert!(dry_run);
        });
    }

    // --- Alias tests: --no-agent5, --agent5-model, --agent5-provider, --agent5-mode ---

    #[test]
    fn test_parse_generate_no_test_agent_alias() {
        let cli = Cli::try_parse_from(["skilldo", "generate", "--no-agent5"]).unwrap();
        assert_generate!(cli, |no_test| {
            assert!(no_test);
        });
    }

    #[test]
    fn test_parse_generate_test_model_alias() {
        let cli =
            Cli::try_parse_from(["skilldo", "generate", "--agent5-model", "gpt-5.2"]).unwrap();
        assert_generate!(cli, |test_model| {
            assert_eq!(test_model.unwrap(), "gpt-5.2");
        });
    }

    #[test]
    fn test_parse_generate_test_provider_alias() {
        let cli =
            Cli::try_parse_from(["skilldo", "generate", "--agent5-provider", "anthropic"]).unwrap();
        assert_generate!(cli, |test_provider| {
            assert_eq!(test_provider.unwrap(), "anthropic");
        });
    }

    #[test]
    fn test_parse_generate_test_mode_alias() {
        let cli =
            Cli::try_parse_from(["skilldo", "generate", "--agent5-mode", "thorough"]).unwrap();
        assert_generate!(cli, |test_mode| {
            assert_eq!(test_mode.unwrap(), "thorough");
        });
    }

    // --- Global flags after subcommand ---

    #[test]
    fn test_parse_quiet_after_subcommand() {
        let cli = Cli::try_parse_from(["skilldo", "generate", "-q"]).unwrap();
        assert!(cli.quiet);
        assert!(!cli.verbose);
    }

    #[test]
    fn test_parse_verbose_after_subcommand() {
        let cli = Cli::try_parse_from(["skilldo", "lint", "x.md", "--verbose"]).unwrap();
        assert!(cli.verbose);
        assert!(!cli.quiet);
    }

    #[test]
    fn test_parse_quiet_after_review() {
        let cli = Cli::try_parse_from(["skilldo", "review", "SKILL.md", "-q"]).unwrap();
        assert!(cli.quiet);
    }

    // --- Generate --config flag ---

    #[test]
    fn test_parse_generate_config_flag() {
        let cli = Cli::try_parse_from(["skilldo", "generate", "--config", "/tmp/my.toml"]).unwrap();
        assert_generate!(cli, |config| {
            assert_eq!(config.unwrap(), "/tmp/my.toml");
        });
    }

    // --- Generate --no-parallel alone ---

    #[test]
    fn test_parse_generate_no_parallel_alone() {
        let cli = Cli::try_parse_from(["skilldo", "generate", "--no-parallel"]).unwrap();
        assert_generate!(cli, |no_parallel| {
            assert!(no_parallel);
        });
    }

    // --- Generate boolean defaults are false ---

    #[test]
    fn test_parse_generate_boolean_defaults() {
        let cli = Cli::try_parse_from(["skilldo", "generate"]).unwrap();
        assert!(!cli.quiet);
        assert!(!cli.verbose);
        assert_generate!(cli, |no_test, no_parallel, dry_run| {
            assert!(!no_test);
            assert!(!no_parallel);
            assert!(!dry_run);
        });
    }

    // --- Generate all Option fields default to None ---

    #[test]
    fn test_parse_generate_option_defaults_are_none() {
        let cli = Cli::try_parse_from(["skilldo", "generate"]).unwrap();
        assert_generate!(cli, |language,
                               input,
                               version,
                               version_from,
                               config,
                               model,
                               provider,
                               base_url,
                               max_retries,
                               test_model,
                               test_provider,
                               test_mode,
                               runtime,
                               timeout,
                               install_source,
                               source_path| {
            assert!(language.is_none());
            assert!(input.is_none());
            assert!(version.is_none());
            assert!(version_from.is_none());
            assert!(config.is_none());
            assert!(model.is_none());
            assert!(provider.is_none());
            assert!(base_url.is_none());
            assert!(max_retries.is_none());
            assert!(test_model.is_none());
            assert!(test_provider.is_none());
            assert!(test_mode.is_none());
            assert!(runtime.is_none());
            assert!(timeout.is_none());
            assert!(install_source.is_none());
            assert!(source_path.is_none());
        });
    }

    // --- Invalid type arguments ---

    #[test]
    fn test_parse_generate_invalid_timeout_type() {
        let result = Cli::try_parse_from(["skilldo", "generate", "--timeout", "abc"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_generate_invalid_max_retries_type() {
        let result = Cli::try_parse_from(["skilldo", "generate", "--max-retries", "xyz"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_review_invalid_timeout_type() {
        let result = Cli::try_parse_from(["skilldo", "review", "x.md", "--timeout", "not-num"]);
        assert!(result.is_err());
    }

    // --- Config without subaction ---

    #[test]
    fn test_parse_config_missing_subaction() {
        let result = Cli::try_parse_from(["skilldo", "config"]);
        assert!(result.is_err());
    }

    // --- Review individual optional flags ---

    #[test]
    fn test_parse_review_with_config_only() {
        let cli = Cli::try_parse_from(["skilldo", "review", "s.md", "--config", "c.toml"]).unwrap();
        assert_review!(cli, |config| {
            assert_eq!(config.unwrap(), "c.toml");
        });
    }

    #[test]
    fn test_parse_review_with_model_only() {
        let cli = Cli::try_parse_from(["skilldo", "review", "s.md", "--model", "gpt-5"]).unwrap();
        assert_review!(cli, |model| {
            assert_eq!(model.unwrap(), "gpt-5");
        });
    }

    #[test]
    fn test_parse_review_no_container_only() {
        let cli = Cli::try_parse_from(["skilldo", "review", "s.md", "--no-container"]).unwrap();
        assert_review!(cli, |no_container| {
            assert!(no_container);
        });
    }

    #[test]
    fn test_parse_review_no_container_default() {
        let cli = Cli::try_parse_from(["skilldo", "review", "s.md"]).unwrap();
        assert_review!(cli, |no_container| {
            assert!(!no_container);
        });
    }

    // --- Verify command metadata via CommandFactory ---

    #[test]
    fn test_cli_command_name() {
        use clap::CommandFactory;
        let cmd = Cli::command();
        assert_eq!(cmd.get_name(), "skilldo");
    }

    #[test]
    fn test_cli_has_version() {
        use clap::CommandFactory;
        let cmd = Cli::command();
        assert!(cmd.get_version().is_some());
    }

    #[test]
    fn test_cli_subcommands_exist() {
        use clap::CommandFactory;
        let cmd = Cli::command();
        let names: Vec<&str> = cmd.get_subcommands().map(|s| s.get_name()).collect();
        assert!(names.contains(&"generate"));
        assert!(names.contains(&"lint"));
        assert!(names.contains(&"review"));
        assert!(names.contains(&"config"));
    }

    // --- Generate with --dry-run alone ---

    #[test]
    fn test_parse_generate_dry_run_alone() {
        let cli = Cli::try_parse_from(["skilldo", "generate", "--dry-run"]).unwrap();
        assert_generate!(cli, |dry_run| {
            assert!(dry_run);
        });
    }

    // --- Generate with explicit path ---

    #[test]
    fn test_parse_generate_explicit_path() {
        let cli = Cli::try_parse_from(["skilldo", "generate", "/some/repo"]).unwrap();
        assert_generate!(cli, |path| {
            assert_eq!(path, "/some/repo");
        });
    }

    // --- Unknown flags rejected ---

    #[test]
    fn test_parse_generate_unknown_flag() {
        let result = Cli::try_parse_from(["skilldo", "generate", "--nonexistent"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_review_unknown_flag() {
        let result = Cli::try_parse_from(["skilldo", "review", "s.md", "--badarg"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_lint_unknown_flag() {
        let result = Cli::try_parse_from(["skilldo", "lint", "s.md", "--extra"]);
        assert!(result.is_err());
    }

    // --- Review with provider and base_url ---

    #[test]
    fn test_parse_review_provider_base_url() {
        let cli = Cli::try_parse_from([
            "skilldo",
            "review",
            "s.md",
            "--provider",
            "openai-compatible",
            "--base-url",
            "http://localhost:8080/v1",
        ])
        .unwrap();
        assert_review!(cli, |provider, base_url| {
            assert_eq!(provider.unwrap(), "openai-compatible");
            assert_eq!(base_url.unwrap(), "http://localhost:8080/v1");
        });
    }

    // --- Review with runtime and timeout ---

    #[test]
    fn test_parse_review_runtime_timeout() {
        let cli = Cli::try_parse_from([
            "skilldo",
            "review",
            "s.md",
            "--runtime",
            "podman",
            "--timeout",
            "60",
        ])
        .unwrap();
        assert_review!(cli, |runtime, timeout| {
            assert_eq!(runtime.unwrap(), "podman");
            assert_eq!(timeout.unwrap(), 60);
        });
    }

    // --- Lint with various paths ---

    #[test]
    fn test_parse_lint_absolute_path() {
        let cli = Cli::try_parse_from(["skilldo", "lint", "/tmp/skills/foo.md"]).unwrap();
        assert_lint!(cli, |path| {
            assert_eq!(path, "/tmp/skills/foo.md");
        });
    }

    // --- Generate with all aliases combined ---

    #[test]
    fn test_parse_generate_all_aliases() {
        let cli = Cli::try_parse_from([
            "skilldo",
            "generate",
            "--no-agent5",
            "--agent5-model",
            "claude-3",
            "--agent5-provider",
            "anthropic",
            "--agent5-mode",
            "minimal",
        ])
        .unwrap();
        assert_generate!(cli, |no_test, test_model, test_provider, test_mode| {
            assert!(no_test);
            assert_eq!(test_model.unwrap(), "claude-3");
            assert_eq!(test_provider.unwrap(), "anthropic");
            assert_eq!(test_mode.unwrap(), "minimal");
        });
    }

    // --- Generate: --version-from strategies ---

    #[test]
    fn test_parse_generate_version_from_package() {
        let cli =
            Cli::try_parse_from(["skilldo", "generate", "--version-from", "package"]).unwrap();
        assert_generate!(cli, |version_from| {
            assert_eq!(version_from.unwrap(), "package");
        });
    }

    #[test]
    fn test_parse_generate_version_from_branch() {
        let cli = Cli::try_parse_from(["skilldo", "generate", "--version-from", "branch"]).unwrap();
        assert_generate!(cli, |version_from| {
            assert_eq!(version_from.unwrap(), "branch");
        });
    }

    #[test]
    fn test_parse_generate_version_from_commit() {
        let cli = Cli::try_parse_from(["skilldo", "generate", "--version-from", "commit"]).unwrap();
        assert_generate!(cli, |version_from| {
            assert_eq!(version_from.unwrap(), "commit");
        });
    }

    // --- Command dispatch variant matching (mirrors main() match arms) ---

    #[test]
    fn test_dispatch_generate_variant() {
        let cli = Cli::try_parse_from(["skilldo", "generate", "--dry-run"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Generate { dry_run: true, .. }
        ));
    }

    #[test]
    fn test_dispatch_lint_variant() {
        let cli = Cli::try_parse_from(["skilldo", "lint", "test.md"]).unwrap();
        assert!(matches!(cli.command, Commands::Lint { .. }));
    }

    #[test]
    fn test_dispatch_review_variant() {
        let cli = Cli::try_parse_from(["skilldo", "review", "test.md"]).unwrap();
        assert!(matches!(cli.command, Commands::Review { .. }));
    }

    #[test]
    fn test_dispatch_config_variant() {
        let cli = Cli::try_parse_from(["skilldo", "config", "check"]).unwrap();
        assert!(matches!(cli.command, Commands::Config { .. }));
    }
}
