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
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
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

        /// Override max generation retries (default: from config)
        #[arg(long)]
        max_retries: Option<usize>,

        /// Use mock LLM client for testing
        #[arg(long)]
        dry_run: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    let cli = Cli::parse();

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
            max_retries,
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
                max_retries,
                dry_run,
            )
            .await?;
        }
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
}
