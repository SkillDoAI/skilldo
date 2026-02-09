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
#[command(name = "skilldo")]
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
                dry_run,
            )
            .await?;
        }
    }

    Ok(())
}
