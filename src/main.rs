use anyhow::Result;
use clap::{Parser, Subcommand};

mod agent5;
mod changelog;
mod cli;
mod config;
mod detector;
mod ecosystems;
mod lint;
mod llm;
mod pipeline;
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

        /// Override Agent 5 LLM model
        #[arg(long)]
        agent5_model: Option<String>,

        /// Override Agent 5 LLM provider
        #[arg(long)]
        agent5_provider: Option<String>,

        /// Disable Agent 5 validation
        #[arg(long)]
        no_agent5: bool,

        /// Agent 5 validation mode: thorough, adaptive, minimal
        #[arg(long)]
        agent5_mode: Option<String>,

        /// Container runtime: docker or podman
        #[arg(long)]
        runtime: Option<String>,

        /// Container execution timeout in seconds
        #[arg(long)]
        timeout: Option<u64>,

        /// Agent 5 install source: registry, local-install, local-mount
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
            agent5_model,
            agent5_provider,
            no_agent5,
            agent5_mode,
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
                agent5_model,
                agent5_provider,
                no_agent5,
                agent5_mode,
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

    #[test]
    fn test_parse_generate_defaults() {
        let cli = Cli::try_parse_from(["skilldo", "generate"]).unwrap();
        match cli.command {
            Commands::Generate {
                path,
                language,
                output,
                dry_run,
                ..
            } => {
                assert_eq!(path, ".");
                assert!(language.is_none());
                assert_eq!(output, "SKILL.md");
                assert!(!dry_run);
            }
            _ => panic!("Expected Generate command"),
        }
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
        match cli.command {
            Commands::Generate {
                path,
                language,
                output,
                model,
                max_retries,
                version,
                dry_run,
                ..
            } => {
                assert_eq!(path, "/tmp/repo");
                assert_eq!(language.unwrap(), "python");
                assert_eq!(output, "out.md");
                assert_eq!(model.unwrap(), "gpt-5.2");
                assert_eq!(max_retries.unwrap(), 5);
                assert_eq!(version.unwrap(), "2.0.0");
                assert!(dry_run);
            }
            _ => panic!("Expected Generate command"),
        }
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
        match cli.command {
            Commands::Generate { input, output, .. } => {
                assert_eq!(input.unwrap(), "old-SKILL.md");
                assert_eq!(output, "new-SKILL.md");
            }
            _ => panic!("Expected Generate command"),
        }
    }

    #[test]
    fn test_parse_generate_version_from() {
        let cli =
            Cli::try_parse_from(["skilldo", "generate", "--version-from", "git-tag"]).unwrap();
        match cli.command {
            Commands::Generate { version_from, .. } => {
                assert_eq!(version_from.unwrap(), "git-tag");
            }
            _ => panic!("Expected Generate command"),
        }
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
        match cli.command {
            Commands::Lint { path } => {
                assert_eq!(path, "SKILL.md");
            }
            _ => panic!("Expected Lint command"),
        }
    }

    #[test]
    fn test_parse_lint_missing_path() {
        let result = Cli::try_parse_from(["skilldo", "lint"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_config_check() {
        let cli = Cli::try_parse_from(["skilldo", "config", "check"]).unwrap();
        match cli.command {
            Commands::Config { action } => match action {
                ConfigAction::Check { config } => {
                    assert!(config.is_none());
                }
            },
            _ => panic!("Expected Config command"),
        }
    }

    #[test]
    fn test_parse_config_check_with_path() {
        let cli =
            Cli::try_parse_from(["skilldo", "config", "check", "--config", "my.toml"]).unwrap();
        match cli.command {
            Commands::Config { action } => match action {
                ConfigAction::Check { config } => {
                    assert_eq!(config.unwrap(), "my.toml");
                }
            },
            _ => panic!("Expected Config command"),
        }
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
        match cli.command {
            Commands::Generate {
                provider, base_url, ..
            } => {
                assert_eq!(provider.unwrap(), "openai");
                assert_eq!(base_url.unwrap(), "http://localhost:11434/v1");
            }
            _ => panic!("Expected Generate"),
        }
    }

    #[test]
    fn test_parse_generate_no_agent5() {
        let cli = Cli::try_parse_from(["skilldo", "generate", "--no-agent5"]).unwrap();
        match cli.command {
            Commands::Generate { no_agent5, .. } => {
                assert!(no_agent5);
            }
            _ => panic!("Expected Generate"),
        }
    }

    #[test]
    fn test_parse_generate_agent5_overrides() {
        let cli = Cli::try_parse_from([
            "skilldo",
            "generate",
            "--agent5-model",
            "gpt-5.2",
            "--agent5-provider",
            "openai",
        ])
        .unwrap();
        match cli.command {
            Commands::Generate {
                agent5_model,
                agent5_provider,
                ..
            } => {
                assert_eq!(agent5_model.unwrap(), "gpt-5.2");
                assert_eq!(agent5_provider.unwrap(), "openai");
            }
            _ => panic!("Expected Generate"),
        }
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
        match cli.command {
            Commands::Generate {
                runtime, timeout, ..
            } => {
                assert_eq!(runtime.unwrap(), "podman");
                assert_eq!(timeout.unwrap(), 300);
            }
            _ => panic!("Expected Generate"),
        }
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
        match cli.command {
            Commands::Generate {
                install_source,
                source_path,
                ..
            } => {
                assert_eq!(install_source.unwrap(), "local-mount");
                assert_eq!(source_path.unwrap(), "/tmp/mylib");
            }
            _ => panic!("Expected Generate"),
        }
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
            "--agent5-model",
            "gpt-5.2",
            "--agent5-provider",
            "openai",
            "--no-agent5",
            "--agent5-mode",
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
                agent5_model,
                agent5_provider,
                no_agent5,
                agent5_mode,
                runtime,
                timeout,
                install_source,
                source_path,
                no_parallel,
                dry_run,
            } => {
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
                assert_eq!(agent5_model.unwrap(), "gpt-5.2");
                assert_eq!(agent5_provider.unwrap(), "openai");
                assert!(no_agent5);
                assert_eq!(agent5_mode.unwrap(), "minimal");
                assert_eq!(runtime.unwrap(), "podman");
                assert_eq!(timeout.unwrap(), 300);
                assert_eq!(install_source.unwrap(), "local-mount");
                assert!(no_parallel);
                assert_eq!(source_path.unwrap(), "/tmp/mylib");
                assert!(dry_run);
            }
            _ => panic!("Expected Generate"),
        }
    }

    #[test]
    fn test_parse_generate_provider_only() {
        let cli = Cli::try_parse_from(["skilldo", "generate", "--provider", "openai-compatible"])
            .unwrap();
        match cli.command {
            Commands::Generate { provider, .. } => {
                assert_eq!(provider.unwrap(), "openai-compatible");
            }
            _ => panic!("Expected Generate"),
        }
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
        match cli.command {
            Commands::Generate { base_url, .. } => {
                assert_eq!(base_url.unwrap(), "http://localhost:11434/v1");
            }
            _ => panic!("Expected Generate"),
        }
    }

    #[test]
    fn test_parse_generate_agent5_model_only() {
        let cli =
            Cli::try_parse_from(["skilldo", "generate", "--agent5-model", "gpt-5.2"]).unwrap();
        match cli.command {
            Commands::Generate { agent5_model, .. } => {
                assert_eq!(agent5_model.unwrap(), "gpt-5.2");
            }
            _ => panic!("Expected Generate"),
        }
    }

    #[test]
    fn test_parse_generate_agent5_provider_only() {
        let cli =
            Cli::try_parse_from(["skilldo", "generate", "--agent5-provider", "anthropic"]).unwrap();
        match cli.command {
            Commands::Generate {
                agent5_provider, ..
            } => {
                assert_eq!(agent5_provider.unwrap(), "anthropic");
            }
            _ => panic!("Expected Generate"),
        }
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
        match cli.command {
            Commands::Generate {
                install_source,
                source_path,
                ..
            } => {
                assert_eq!(install_source.unwrap(), "local-install");
                assert_eq!(source_path.unwrap(), "/tmp/lib");
            }
            _ => panic!("Expected Generate"),
        }
    }

    #[test]
    fn test_parse_generate_timeout_only() {
        let cli = Cli::try_parse_from(["skilldo", "generate", "--timeout", "600"]).unwrap();
        match cli.command {
            Commands::Generate { timeout, .. } => {
                assert_eq!(timeout.unwrap(), 600);
            }
            _ => panic!("Expected Generate"),
        }
    }

    #[test]
    fn test_parse_generate_agent5_mode_only() {
        let cli =
            Cli::try_parse_from(["skilldo", "generate", "--agent5-mode", "adaptive"]).unwrap();
        match cli.command {
            Commands::Generate { agent5_mode, .. } => {
                assert_eq!(agent5_mode.unwrap(), "adaptive");
            }
            _ => panic!("Expected Generate"),
        }
    }

    #[test]
    fn test_parse_quiet_with_generate() {
        let cli = Cli::try_parse_from(["skilldo", "-q", "generate"]).unwrap();
        assert!(cli.quiet);
        assert!(!cli.verbose);
        match cli.command {
            Commands::Generate { path, .. } => {
                assert_eq!(path, ".");
            }
            _ => panic!("Expected Generate"),
        }
    }

    #[test]
    fn test_parse_verbose_with_config_check() {
        let cli = Cli::try_parse_from(["skilldo", "-v", "config", "check"]).unwrap();
        assert!(cli.verbose);
        assert!(!cli.quiet);
        match cli.command {
            Commands::Config { action } => match action {
                ConfigAction::Check { config } => {
                    assert!(config.is_none());
                }
            },
            _ => panic!("Expected Config"),
        }
    }

    #[test]
    fn test_parse_quiet_and_verbose_both() {
        let cli = Cli::try_parse_from(["skilldo", "-q", "-v", "generate"]).unwrap();
        assert!(cli.quiet);
        assert!(cli.verbose);
    }
}
