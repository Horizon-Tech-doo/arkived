//! The `arkived` CLI binary.

use anyhow::{bail, Result};
use arkived_core::config::{ArkivedConfig, ConfirmMode, OutputFormat};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod auth;
mod commands;
mod output;
mod path;
mod policy;

use auth::AuthArgs;

/// Arkived — a fast, Rust-native storage client for Microsoft Azure.
#[derive(Parser, Debug)]
#[command(name = "arkived", version, about, long_about = None)]
struct Cli {
    /// Log level: trace, debug, info, warn, error
    #[arg(long, env = "ARKIVED_LOG_LEVEL", default_value = "info", global = true)]
    log_level: String,

    /// Output format: json, yaml, table, tsv (default from config, else table)
    #[arg(long, env = "ARKIVED_FORMAT", global = true)]
    format: Option<String>,

    /// Approve destructive actions without prompting.
    #[arg(long, global = true)]
    yes: bool,

    #[command(flatten)]
    auth: AuthArgs,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Sign in to Azure (interactive AAD device-code flow)
    Login,
    /// Manage saved storage accounts
    Account {
        #[command(subcommand)]
        action: AccountAction,
    },
    /// List containers or blobs
    Ls {
        /// Optional container path; if omitted, lists containers
        path: Option<String>,
        /// List blobs recursively (no virtual-directory grouping)
        #[arg(long, short)]
        recursive: bool,
    },
    /// Stream a blob to stdout
    Cat {
        /// Blob path, e.g. `mycontainer/file.txt`
        path: String,
    },
    /// Copy a file or blob (local↔remote, remote↔remote)
    Cp {
        /// Source path (local or `container/blob`)
        src: String,
        /// Destination path (local or `container/blob`)
        dst: String,
        /// Overwrite the destination blob if it already exists
        #[arg(long)]
        force: bool,
    },
    /// Delete a blob
    Rm {
        /// Blob path, e.g. `mycontainer/file.txt`
        path: String,
    },
    /// Generate a SAS URL for a container or blob
    Sas {
        /// Container or blob path
        path: String,
        /// Permission letters (r=read, w=write, d=delete, l=list, a=add, c=create)
        #[arg(long, default_value = "r")]
        permissions: String,
        /// Hours until the SAS expires
        #[arg(long, default_value_t = 1)]
        expiry_hours: i64,
    },
    /// Set a blob's access tier (hot|cool|cold|archive)
    SetTier {
        /// Blob path, e.g. `mycontainer/file.txt`
        path: String,
        /// Target tier
        tier: String,
    },
    /// Show a blob's system (HTTP) properties
    Properties {
        /// Blob path, e.g. `mycontainer/file.txt`
        path: String,
    },
    /// Show a blob's user-defined metadata
    Meta {
        /// Blob path, e.g. `mycontainer/file.txt`
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

#[derive(Subcommand, Debug)]
enum AccountAction {
    /// List saved storage accounts
    List,
    /// Set the active storage account by name
    Use {
        /// Storage account name
        name: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&cli.log_level)),
        )
        .init();

    let config = discover_config();
    let format = resolve_format(cli.format.as_deref(), config.default_format)?;
    let confirm_mode = config.default_confirm;

    match cli.command {
        None => {
            println!("Arkived — a fast, Rust-native storage client for Microsoft Azure.");
            println!("Run `arkived --help` for available commands.");
            Ok(())
        }
        Some(cmd) => dispatch(cmd, &cli.auth, format, confirm_mode, cli.yes).await,
    }
}

async fn dispatch(
    cmd: Command,
    auth: &AuthArgs,
    format: OutputFormat,
    confirm_mode: ConfirmMode,
    yes: bool,
) -> Result<()> {
    match cmd {
        Command::Ls { path, recursive } => {
            let backend = auth.resolve_backend().await?;
            commands::ls(&backend, path, recursive, format).await
        }
        Command::Cat { path } => {
            let backend = auth.resolve_backend().await?;
            commands::cat(&backend, path).await
        }
        Command::Cp { src, dst, force } => {
            let backend = auth.resolve_backend().await?;
            let ctx = commands::make_ctx(confirm_mode, yes);
            commands::cp(&backend, &ctx, src, dst, force).await
        }
        Command::Rm { path } => {
            let backend = auth.resolve_backend().await?;
            let ctx = commands::make_ctx(confirm_mode, yes);
            commands::rm(&backend, &ctx, path).await
        }
        Command::Sas {
            path,
            permissions,
            expiry_hours,
        } => {
            let backend = auth.resolve_backend().await?;
            let ctx = commands::make_ctx(confirm_mode, yes);
            commands::sas(&backend, &ctx, path, permissions, expiry_hours).await
        }
        Command::SetTier { path, tier } => {
            let backend = auth.resolve_backend().await?;
            let ctx = commands::make_ctx(confirm_mode, yes);
            commands::set_tier(&backend, &ctx, path, tier).await
        }
        Command::Properties { path } => {
            let backend = auth.resolve_backend().await?;
            commands::properties(&backend, path, format).await
        }
        Command::Meta { path } => {
            let backend = auth.resolve_backend().await?;
            commands::metadata(&backend, path, format).await
        }
        Command::Doctor => commands::doctor(auth).await,
        Command::Login | Command::Account { .. } => bail!(
            "saved sign-in / account management lands in the next CLI increment. \
             For now, connect with --connection-string, --sas, --account-key, or --azurite."
        ),
        Command::Mcp => arkived_mcp::run().await,
        Command::ServeAcp => bail!("`arkived serve-acp` is a later milestone (v0.4)."),
        Command::Gui => bail!("`arkived gui` will launch the desktop app in a later milestone."),
    }
}

/// Resolve the output format from `--format` (if given) or the config default.
fn resolve_format(flag: Option<&str>, default: OutputFormat) -> Result<OutputFormat> {
    match flag {
        None => Ok(default),
        Some(s) => match s.to_ascii_lowercase().as_str() {
            "json" => Ok(OutputFormat::Json),
            "yaml" | "yml" => Ok(OutputFormat::Yaml),
            "table" => Ok(OutputFormat::Table),
            "tsv" => Ok(OutputFormat::Tsv),
            other => bail!("unknown --format '{other}' (use json|yaml|table|tsv)"),
        },
    }
}

/// Discover preferences from `./.arkived.toml` then the user config dir.
fn discover_config() -> ArkivedConfig {
    let project = std::env::current_dir().ok();
    let user = user_config_dir();
    ArkivedConfig::discover(project.as_deref(), user.as_deref()).unwrap_or_default()
}

/// `$ARKIVED_CONFIG_DIR`, else `%APPDATA%\arkived` (Windows) /
/// `$XDG_CONFIG_HOME/arkived` or `~/.config/arkived` (Unix).
fn user_config_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("ARKIVED_CONFIG_DIR") {
        return Some(PathBuf::from(d));
    }
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("arkived"))
    }
    #[cfg(not(windows))]
    {
        if let Some(x) = std::env::var_os("XDG_CONFIG_HOME") {
            return Some(PathBuf::from(x).join("arkived"));
        }
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config").join("arkived"))
    }
}
