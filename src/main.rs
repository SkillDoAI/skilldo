use anyhow::Result;
use clap::{Parser, Subcommand};

mod auth;
mod changelog;
mod cli;
mod config;
mod detector;
mod ecosystems;
mod error;
mod git;
mod lint;
mod llm;
mod pipeline;
mod review;
mod security;
mod telemetry;
mod test_agent;
mod util;

#[derive(Parser)]
#[command(name = "skilldo", version)]
#[command(
    about = concat!("Skilldo — Generate agent rules files for your libraries — v", env!("CARGO_PKG_VERSION")),
    long_about = None,
)]
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

        /// Output file path (default: SKILL.md, or from config)
        #[arg(short = 'o', long)]
        output: Option<String>,

        /// Explicit library version override (e.g., "2.1.0")
        #[arg(long = "skill-version")]
        skill_version: Option<String>,

        /// Version extraction strategy: git-tag, package, branch, commit
        #[arg(long)]
        version_from: Option<String>,

        /// Path to config file (defaults to ~/.config/skilldo/config.toml or ./skilldo.toml)
        #[arg(long)]
        config: Option<String>,

        /// Override LLM model (e.g., "gpt-5.2", "claude-sonnet-4-5-20250929")
        #[arg(long)]
        model: Option<String>,

        /// LLM provider: anthropic, openai, chatgpt, gemini, openai-compatible
        #[arg(long)]
        provider: Option<String>,

        /// Base URL for openai-compatible providers (e.g., http://localhost:11434/v1)
        #[arg(long)]
        base_url: Option<String>,

        /// Override max generation retries (default: from config)
        #[arg(long)]
        max_retries: Option<usize>,

        /// Override test stage LLM model
        #[arg(long = "test-model")]
        test_model: Option<String>,

        /// Override test stage LLM provider
        #[arg(long = "test-provider")]
        test_provider: Option<String>,

        /// Disable test stage validation
        #[arg(long = "no-test")]
        no_test: bool,

        /// Test stage validation mode: thorough, adaptive, minimal
        #[arg(long = "test-mode")]
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

        /// Run test agent in container mode (default: bare-metal with uv)
        #[arg(long)]
        container: bool,

        /// Disable review agent validation
        #[arg(long = "no-review")]
        no_review: bool,

        /// Disable security scan (YARA + unicode + injection)
        #[arg(long = "no-security-scan")]
        no_security_scan: bool,

        /// Override review stage LLM model
        #[arg(long = "review-model")]
        review_model: Option<String>,

        /// Override review stage LLM provider
        #[arg(long = "review-provider")]
        review_provider: Option<String>,

        /// Run agents 1-3 sequentially instead of in parallel
        #[arg(long)]
        no_parallel: bool,

        /// Exit 0 even when lint/review/test errors remain (default: exit 1)
        #[arg(long)]
        best_effort: bool,

        /// Enable generation telemetry logging to ~/.skilldo/runs.csv
        #[arg(long)]
        telemetry: bool,

        /// Disable telemetry (overrides config file telemetry = true)
        #[arg(long, conflicts_with = "telemetry")]
        no_telemetry: bool,

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

        /// LLM provider: anthropic, openai, chatgpt, gemini, openai-compatible
        #[arg(long)]
        provider: Option<String>,

        /// Base URL for openai-compatible providers
        #[arg(long)]
        base_url: Option<String>,

        /// Use mock LLM client for testing
        #[arg(long)]
        dry_run: bool,
    },

    /// Validate configuration file
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Show the prompts that will be sent to the LLM
    ShowPrompts {
        /// Language/ecosystem: python, go
        #[arg(long, default_value = "python")]
        language: String,

        /// Show only a specific stage: extract, map, learn, create, review, test
        #[arg(long)]
        stage: Option<String>,
    },

    /// Manage OAuth authentication for LLM providers
    Auth {
        #[command(subcommand)]
        action: AuthAction,
    },

    /// Quick LLM auth smoke test (requires --config)
    #[command(hide = true)]
    HelloWorld {
        /// Path to config file (required)
        #[arg(long)]
        config: String,
    },

    /// Print the embedded SKILL.md for the skilldo CLI
    Skill,
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Check configuration for errors
    Check {
        /// Path to config file
        #[arg(long)]
        config: Option<String>,

        /// Exit with error code on validation failures (for CI)
        #[arg(long)]
        strict: bool,
    },
}

#[derive(Subcommand)]
enum AuthAction {
    /// Log in to configured OAuth providers (opens browser)
    Login {
        /// Path to config file
        #[arg(long)]
        config: Option<String>,
    },
    /// Show OAuth token status for configured providers
    Status {
        /// Path to config file
        #[arg(long)]
        config: Option<String>,
    },
    /// Remove stored OAuth tokens
    Logout {
        /// Path to config file
        #[arg(long)]
        config: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging: RUST_LOG env var takes priority (for power users),
    // otherwise fall back to --quiet / --verbose CLI flags.
    if std::env::var_os("RUST_LOG").is_some() {
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .init();
    } else {
        let log_level = if cli.quiet {
            tracing::Level::WARN
        } else if cli.verbose {
            tracing::Level::DEBUG
        } else {
            tracing::Level::INFO
        };
        tracing_subscriber::fmt().with_max_level(log_level).init();
    }

    match cli.command {
        Commands::Generate {
            path,
            language,
            input,
            output,
            skill_version,
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
            no_review,
            no_security_scan,
            review_model,
            review_provider,
            runtime,
            timeout,
            install_source,
            source_path,
            container,
            no_parallel,
            best_effort,
            telemetry,
            no_telemetry,
            dry_run,
        } => {
            cli::generate::run(cli::generate::GenerateOptions {
                path,
                language,
                input,
                output,
                version_override: skill_version,
                version_from: version_from
                    .map(|s| s.parse::<crate::config::VersionStrategy>())
                    .transpose()?,
                config_path: config,
                model_override: model,
                provider_override: provider,
                base_url_override: base_url,
                max_retries_override: max_retries,
                test_model_override: test_model,
                test_provider_override: test_provider,
                no_test,
                test_mode_override: test_mode,
                no_review,
                no_security_scan,
                review_model_override: review_model,
                review_provider_override: review_provider,
                runtime_override: runtime,
                timeout_override: timeout,
                install_source_override: install_source,
                source_path_override: source_path,
                container,
                no_parallel,
                best_effort,
                telemetry,
                no_telemetry,
                dry_run,
            })
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
            dry_run,
        } => {
            cli::review::run(path, config, model, provider, base_url, dry_run).await?;
        }
        Commands::Config { action } => match action {
            ConfigAction::Check { config, strict } => {
                cli::config_check::run(config, strict)?;
            }
        },
        Commands::ShowPrompts { language, stage } => {
            cli::show_prompts::run(&language, stage.as_deref())?;
        }
        Commands::Auth { action } => match action {
            AuthAction::Login { config } => {
                cli::auth::login(config).await?;
            }
            AuthAction::Status { config } => {
                cli::auth::status(config)?;
            }
            AuthAction::Logout { config } => {
                cli::auth::logout(config)?;
            }
        },
        Commands::HelloWorld { config } => {
            let cfg = crate::config::Config::load_with_path(Some(config))?;
            let client = crate::llm::factory::create_client(&cfg, false).await?;
            println!(
                "\u{1F426} Asking {} ({})...\n",
                cfg.llm.resolved_provider_name(),
                cfg.llm.model
            );
            let response = client
                .complete(
                    "What is the airspeed velocity of an unladen swallow? Be brief and witty.",
                )
                .await?;
            println!("{response}");
        }
        Commands::Skill => {
            print!("{}", include_str!("../SKILL.md"));
        }
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
            let ConfigAction::Check { $field, .. } = action;
            $body
        };
    }

    #[test]
    fn test_parse_generate_defaults() {
        let cli = Cli::try_parse_from(["skilldo", "generate"]).unwrap();
        assert_generate!(cli, |path, language, output, dry_run| {
            assert_eq!(path, ".");
            assert!(language.is_none());
            assert!(output.is_none(), "output should default to None");
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
            "--skill-version",
            "2.0.0",
            "--dry-run",
        ])
        .unwrap();
        assert_generate!(cli, |path,
                               language,
                               output,
                               model,
                               max_retries,
                               skill_version,
                               dry_run| {
            assert_eq!(path, "/tmp/repo");
            assert_eq!(language.unwrap(), "python");
            assert_eq!(output.unwrap(), "out.md");
            assert_eq!(model.unwrap(), "gpt-5.2");
            assert_eq!(max_retries.unwrap(), 5);
            assert_eq!(skill_version.unwrap(), "2.0.0");
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
            assert_eq!(output.unwrap(), "new-SKILL.md");
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
    fn test_parse_config_check_strict() {
        let cli = Cli::try_parse_from(["skilldo", "config", "check", "--strict"]).unwrap();
        let Commands::Config { action } = cli.command else {
            panic!("Expected Config command");
        };
        let ConfigAction::Check { strict, .. } = action;
        assert!(strict);
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
    fn test_parse_generate_no_review() {
        let cli = Cli::try_parse_from(["skilldo", "generate", "--no-review"]).unwrap();
        assert_generate!(cli, |no_review| {
            assert!(no_review);
        });
    }

    #[test]
    fn test_parse_generate_no_security_scan() {
        let cli = Cli::try_parse_from(["skilldo", "generate", "--no-security-scan"]).unwrap();
        assert_generate!(cli, |no_security_scan| {
            assert!(no_security_scan);
        });
    }

    #[test]
    fn test_parse_generate_review_overrides() {
        let cli = Cli::try_parse_from([
            "skilldo",
            "generate",
            "--review-model",
            "gpt-5.2",
            "--review-provider",
            "openai",
        ])
        .unwrap();
        assert_generate!(cli, |review_model, review_provider| {
            assert_eq!(review_model.unwrap(), "gpt-5.2");
            assert_eq!(review_provider.unwrap(), "openai");
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
            "--no-review",
            "--review-model",
            "gpt-5.2",
            "--review-provider",
            "openai",
            "--runtime",
            "podman",
            "--timeout",
            "300",
            "--install-source",
            "local-mount",
            "--source-path",
            "/tmp/mylib",
            "--container",
            "--no-parallel",
            "--best-effort",
            "--telemetry",
            "--dry-run",
        ])
        .unwrap();
        assert_generate!(cli, |path,
                               language,
                               input,
                               output,
                               skill_version,
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
                               no_review,
                               review_model,
                               review_provider,
                               runtime,
                               timeout,
                               install_source,
                               source_path,
                               container,
                               no_parallel,
                               best_effort,
                               telemetry,
                               dry_run| {
            assert_eq!(path, "/tmp/repo");
            assert_eq!(language.unwrap(), "python");
            assert!(input.is_none());
            assert!(output.is_none());
            assert!(skill_version.is_none());
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
            assert!(no_review);
            assert_eq!(review_model.unwrap(), "gpt-5.2");
            assert_eq!(review_provider.unwrap(), "openai");
            assert_eq!(runtime.unwrap(), "podman");
            assert_eq!(timeout.unwrap(), 300);
            assert_eq!(install_source.unwrap(), "local-mount");
            assert!(container);
            assert!(no_parallel);
            assert!(best_effort);
            assert!(telemetry);
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
        assert_review!(cli, |path, config, model, provider, base_url, dry_run| {
            assert_eq!(path, "SKILL.md");
            assert!(config.is_none());
            assert!(model.is_none());
            assert!(provider.is_none());
            assert!(base_url.is_none());
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
            "--dry-run",
        ])
        .unwrap();
        assert_review!(cli, |path, config, model, provider, base_url, dry_run| {
            assert_eq!(path, "test.md");
            assert_eq!(config.unwrap(), "my.toml");
            assert_eq!(model.unwrap(), "codestral:latest");
            assert_eq!(provider.unwrap(), "openai-compatible");
            assert_eq!(base_url.unwrap(), "http://localhost:11434/v1");
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

    // --- Generate --best-effort ---

    #[test]
    fn test_parse_generate_best_effort() {
        let cli = Cli::try_parse_from(["skilldo", "generate", "--best-effort"]).unwrap();
        assert_generate!(cli, |best_effort| {
            assert!(best_effort);
        });
    }

    // --- Generate boolean defaults are false ---

    #[test]
    fn test_parse_generate_boolean_defaults() {
        let cli = Cli::try_parse_from(["skilldo", "generate"]).unwrap();
        assert!(!cli.quiet);
        assert!(!cli.verbose);
        assert_generate!(cli, |no_test,
                               no_review,
                               no_security_scan,
                               container,
                               no_parallel,
                               best_effort,
                               telemetry,
                               no_telemetry,
                               dry_run| {
            assert!(!no_test);
            assert!(!no_review);
            assert!(!no_security_scan);
            assert!(!container);
            assert!(!no_parallel);
            assert!(!best_effort);
            assert!(!telemetry);
            assert!(!no_telemetry);
            assert!(!dry_run);
        });
    }

    #[test]
    fn test_parse_generate_no_telemetry() {
        let cli = Cli::try_parse_from(["skilldo", "generate", "--no-telemetry"]).unwrap();
        assert_generate!(cli, |telemetry, no_telemetry| {
            assert!(!telemetry);
            assert!(no_telemetry);
        });
    }

    #[test]
    fn test_parse_generate_telemetry_and_no_telemetry_conflict() {
        let result = Cli::try_parse_from(["skilldo", "generate", "--telemetry", "--no-telemetry"]);
        assert!(
            result.is_err(),
            "--telemetry and --no-telemetry should conflict"
        );
    }

    // --- Generate all Option fields default to None ---

    #[test]
    fn test_parse_generate_option_defaults_are_none() {
        let cli = Cli::try_parse_from(["skilldo", "generate"]).unwrap();
        assert_generate!(cli, |language,
                               input,
                               skill_version,
                               version_from,
                               config,
                               model,
                               provider,
                               base_url,
                               max_retries,
                               test_model,
                               test_provider,
                               test_mode,
                               review_model,
                               review_provider,
                               runtime,
                               timeout,
                               install_source,
                               source_path| {
            assert!(language.is_none());
            assert!(input.is_none());
            assert!(skill_version.is_none());
            assert!(version_from.is_none());
            assert!(config.is_none());
            assert!(model.is_none());
            assert!(provider.is_none());
            assert!(base_url.is_none());
            assert!(max_retries.is_none());
            assert!(test_model.is_none());
            assert!(test_provider.is_none());
            assert!(test_mode.is_none());
            assert!(review_model.is_none());
            assert!(review_provider.is_none());
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

    // --- Lint with various paths ---

    #[test]
    fn test_parse_lint_absolute_path() {
        let cli = Cli::try_parse_from(["skilldo", "lint", "/tmp/skills/foo.md"]).unwrap();
        assert_lint!(cli, |path| {
            assert_eq!(path, "/tmp/skills/foo.md");
        });
    }

    // --- Generate with all test flags combined ---

    #[test]
    fn test_parse_generate_all_test_flags() {
        let cli = Cli::try_parse_from([
            "skilldo",
            "generate",
            "--no-test",
            "--test-model",
            "claude-3",
            "--test-provider",
            "anthropic",
            "--test-mode",
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

    #[test]
    fn test_dispatch_auth_login() {
        let cli = Cli::try_parse_from(["skilldo", "auth", "login"]).unwrap();
        let Commands::Auth { action } = cli.command else {
            panic!("Expected Auth command");
        };
        assert!(matches!(action, AuthAction::Login { config: None }));
    }

    #[test]
    fn test_dispatch_auth_status() {
        let cli = Cli::try_parse_from(["skilldo", "auth", "status"]).unwrap();
        assert!(matches!(cli.command, Commands::Auth { .. }));
    }

    #[test]
    fn test_dispatch_auth_logout() {
        let cli = Cli::try_parse_from(["skilldo", "auth", "logout"]).unwrap();
        assert!(matches!(cli.command, Commands::Auth { .. }));
    }

    #[test]
    fn test_dispatch_auth_login_with_config() {
        let cli = Cli::try_parse_from(["skilldo", "auth", "login", "--config", "my.toml"]).unwrap();
        let Commands::Auth { action } = cli.command else {
            panic!("Expected Auth command");
        };
        let AuthAction::Login { config } = action else {
            panic!("Expected Login action");
        };
        assert_eq!(config, Some("my.toml".to_string()));
    }

    #[test]
    fn test_dispatch_hello_world_requires_config() {
        // Without --config, parsing should fail (config is required)
        let result = Cli::try_parse_from(["skilldo", "hello-world"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_dispatch_hello_world_with_config() {
        let cli = Cli::try_parse_from(["skilldo", "hello-world", "--config", "test.toml"]).unwrap();
        let Commands::HelloWorld { config } = cli.command else {
            panic!("Expected HelloWorld command");
        };
        assert_eq!(config, "test.toml".to_string());
    }

    #[test]
    fn test_parse_skill_command() {
        let cli = Cli::try_parse_from(["skilldo", "skill"]).unwrap();
        assert!(matches!(cli.command, Commands::Skill));
    }

    #[test]
    fn test_hello_world_is_hidden() {
        use clap::CommandFactory;
        let cmd = Cli::command();
        let hello = cmd
            .get_subcommands()
            .find(|s| s.get_name() == "hello-world");
        assert!(hello.is_some());
        assert!(hello.unwrap().is_hide_set());
    }
}
