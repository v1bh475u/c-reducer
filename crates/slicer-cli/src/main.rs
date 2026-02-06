//! C Program Slicer CLI
//!
//! Command-line interface for the C program slicer.

mod commands;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

/// C Program Slicer - Reduce C programs while preserving behavior
#[derive(Parser)]
#[command(name = "slicer")]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Verbosity level (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Output format (text, json)
    #[arg(short, long, default_value = "text")]
    format: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Reduce a C source file
    Reduce(commands::ReduceArgs),

    /// Validate that a reduced file is equivalent to the original
    Validate(commands::ValidateArgs),

    /// Parse and analyze a C source file
    Parse(commands::ParseArgs),

    /// List available reduction passes
    Passes,

    /// Generate a default configuration file
    Init(commands::InitArgs),
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Set up logging
    let log_level = match cli.verbose {
        0 => Level::WARN,
        1 => Level::INFO,
        2 => Level::DEBUG,
        _ => Level::TRACE,
    };

    let subscriber = FmtSubscriber::builder()
        .with_max_level(log_level)
        .with_target(false)
        .finish();

    tracing::subscriber::set_global_default(subscriber)?;

    // Dispatch to command handlers
    match cli.command {
        Commands::Reduce(args) => commands::reduce(args, &cli.format),
        Commands::Validate(args) => commands::validate(args),
        Commands::Parse(args) => commands::parse(args),
        Commands::Passes => commands::list_passes(),
        Commands::Init(args) => commands::init(args),
    }
}
