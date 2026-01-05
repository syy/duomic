mod audio;
mod commands;
mod config;
mod ipc;
mod tui;

use clap::{Parser, Subcommand};
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

#[derive(Parser)]
#[command(name = "duomic")]
#[command(
    author,
    version,
    about = "Split multi-channel USB mic into virtual mono mics"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Increase verbosity (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    /// Disable colored output
    #[arg(long, global = true)]
    no_color: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Start interactive TUI (device selection, channel config, dashboard)
    Run {
        /// Device name to use (skip device selection)
        #[arg(short, long)]
        device: Option<String>,
    },
    /// Show driver status and active devices
    Status,
}

fn setup_logging(verbosity: u8) {
    let level = match verbosity {
        0 => Level::ERROR,
        1 => Level::INFO,
        2 => Level::DEBUG,
        _ => Level::TRACE,
    };

    let subscriber = FmtSubscriber::builder()
        .with_max_level(level)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .compact()
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    setup_logging(cli.verbose);

    // Set color preference
    if cli.no_color {
        std::env::set_var("NO_COLOR", "1");
    }

    match cli.command {
        Some(Commands::Run { device }) => commands::run::execute(device),
        Some(Commands::Status) => commands::status::execute(),
        None => {
            // Default to run command (includes setup flow)
            commands::run::execute(None)
        }
    }
}
