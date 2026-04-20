//! The `arkived` CLI binary.

use anyhow::Result;
use clap::{Parser, Subcommand};

/// Arkived — a fast, Rust-native storage client for Microsoft Azure.
#[derive(Parser, Debug)]
#[command(name = "arkived", version, about, long_about = None)]
struct Cli {
    /// Log level: trace, debug, info, warn, error
    #[arg(long, env = "ARKIVED_LOG_LEVEL", default_value = "info", global = true)]
    log_level: String,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Sign in to Azure (interactive AAD flow by default)
    Login,
    /// List containers or blobs
    Ls {
        /// Optional container path; if omitted, lists containers
        path: Option<String>,
    },
    /// Stream a blob to stdout
    Cat {
        /// Blob path, e.g. `mycontainer/file.txt`
        path: String,
    },
    /// Copy a file or blob
    Cp {
        /// Source path (local or `container/blob`)
        src: String,
        /// Destination path (local or `container/blob`)
        dst: String,
    },
    /// Delete a blob
    Rm {
        /// Blob path, e.g. `mycontainer/file.txt`
        path: String,
        /// Skip the confirmation prompt
        #[arg(long)]
        yes: bool,
    },
    /// Generate a SAS URL for a container or blob
    Sas {
        /// Container or blob path
        path: String,
    },
    /// Run as an MCP server over stdio (Stage 2)
    Mcp,
    /// Run as an ACP host (Stage 4)
    ServeAcp,
    /// Launch the Tauri desktop app (Stage 3)
    Gui,
    /// Diagnose configuration and connectivity
    Doctor,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&cli.log_level)),
        )
        .init();

    match cli.command {
        None => {
            println!("Arkived — a fast, Rust-native storage client for Microsoft Azure.");
            println!("Run `arkived --help` for available commands.");
            println!();
            println!("🚧 Pre-release. Commands are not yet implemented.");
            println!("   Follow progress at https://github.com/Horizon-Tech-doo/arkived");
            Ok(())
        }
        Some(cmd) => {
            tracing::info!(?cmd, "command invoked");
            anyhow::bail!(
                "Not yet implemented. This is the pre-release scaffold. \
                 Track progress at https://github.com/Horizon-Tech-doo/arkived"
            );
        }
    }
}
