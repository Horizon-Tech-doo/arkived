//! The `arkived` CLI binary.

use anyhow::{bail, Result};
use arkived_core::config::{ArkivedConfig, ConfirmMode, OutputFormat};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod account;
mod auth;
mod commands;
mod discover;
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
    /// Sign in to Azure (interactive Entra device-code flow)
    Login {
        /// Entra tenant: "organizations" (work/school), "common", "consumers", or a tenant id
        #[arg(long, default_value = "organizations")]
        tenant: String,
    },
    /// Sign out the active Entra sign-in (removes its cached token)
    Logout,
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
    /// Replace a blob's user-defined metadata (key=value pairs)
    SetMeta {
        /// Blob path, e.g. `mycontainer/file.txt`
        path: String,
        /// One or more `key=value` pairs (replaces all existing metadata)
        #[arg(required = true)]
        pairs: Vec<String>,
    },
    /// Update a blob's system properties (preserves unspecified ones)
    SetProps {
        /// Blob path, e.g. `mycontainer/file.txt`
        path: String,
        #[arg(long)]
        content_type: Option<String>,
        #[arg(long)]
        cache_control: Option<String>,
        #[arg(long)]
        content_encoding: Option<String>,
        #[arg(long)]
        content_language: Option<String>,
        #[arg(long)]
        content_disposition: Option<String>,
    },
    /// Manage containers
    Container {
        #[command(subcommand)]
        action: ContainerAction,
    },
    /// Show a blob's index tags
    Tags {
        /// Blob path, e.g. `mycontainer/file.txt`
        path: String,
    },
    /// Replace a blob's index tags (key=value pairs)
    SetTags {
        /// Blob path, e.g. `mycontainer/file.txt`
        path: String,
        /// One or more `key=value` pairs (replaces all existing tags)
        #[arg(required = true)]
        pairs: Vec<String>,
    },
    /// Create a snapshot of a blob (prints the snapshot id)
    Snapshot {
        /// Blob path, e.g. `mycontainer/file.txt`
        path: String,
    },
    /// Restore a soft-deleted blob
    Undelete {
        /// Blob path, e.g. `mycontainer/file.txt`
        path: String,
    },
    /// Rehydrate an archived blob to an online tier
    Rehydrate {
        /// Blob path, e.g. `mycontainer/file.txt`
        path: String,
        /// Target tier: hot, cool, or cold
        tier: String,
        /// Use High rehydrate priority (more expensive, faster)
        #[arg(long)]
        high: bool,
    },
    /// Manage blob leases
    Lease {
        #[command(subcommand)]
        action: LeaseAction,
    },
    /// Manage queues and messages
    Queue {
        #[command(subcommand)]
        action: QueueAction,
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
    /// Save the current connection under a name (credentials go to the OS keychain)
    Add {
        /// Friendly name to save this account under
        name: String,
    },
    /// Discover subscriptions + storage accounts for the active sign-in (ARM)
    Discover,
    /// List saved storage accounts
    List,
    /// Set the active storage account by name
    Use {
        /// Storage account name
        name: String,
    },
    /// Show the active sign-in / subscription / account
    Current,
    /// Remove a saved account (secret + metadata)
    Forget {
        /// Storage account name
        name: String,
    },
}

#[derive(Subcommand, Debug)]
enum ContainerAction {
    /// Create a container
    Create {
        /// Container name
        name: String,
        /// Public access level: private, blob, or container
        #[arg(long)]
        public_access: Option<String>,
    },
    /// Delete a container and all its blobs
    Delete {
        /// Container name
        name: String,
    },
    /// Set a container's public access level
    SetAccess {
        /// Container name
        name: String,
        /// Access level: private, blob, or container
        access: String,
    },
}

#[derive(Subcommand, Debug)]
enum LeaseAction {
    /// Acquire a lease on a blob (prints the lease id)
    Acquire {
        /// Blob path
        path: String,
        /// Lease duration in seconds (15-60, or -1 for infinite)
        #[arg(long, default_value_t = -1)]
        duration: i32,
    },
    /// Release a held lease
    Release {
        /// Blob path
        path: String,
        /// The lease id to release
        lease_id: String,
    },
    /// Forcibly break a blob's lease
    Break {
        /// Blob path
        path: String,
    },
}

#[derive(Subcommand, Debug)]
enum QueueAction {
    /// List queues
    List,
    /// Create a queue
    Create {
        /// Queue name
        name: String,
    },
    /// Delete a queue and all its messages
    Delete {
        /// Queue name
        name: String,
    },
    /// Enqueue a message
    Put {
        /// Queue name
        name: String,
        /// Message text
        text: String,
    },
    /// Peek messages without dequeuing
    Peek {
        /// Queue name
        name: String,
        /// Number of messages to peek
        #[arg(long, default_value_t = 1)]
        count: u32,
    },
    /// Dequeue messages (hides them for a visibility window)
    Get {
        /// Queue name
        name: String,
        /// Number of messages to dequeue
        #[arg(long, default_value_t = 1)]
        count: u32,
        /// Visibility timeout in seconds
        #[arg(long, default_value_t = 30)]
        visibility: u32,
    },
    /// Clear all messages from a queue
    Clear {
        /// Queue name
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
        Command::SetMeta { path, pairs } => {
            let backend = auth.resolve_backend().await?;
            let ctx = commands::make_ctx(confirm_mode, yes);
            commands::set_meta(&backend, &ctx, path, pairs).await
        }
        Command::SetProps {
            path,
            content_type,
            cache_control,
            content_encoding,
            content_language,
            content_disposition,
        } => {
            let backend = auth.resolve_backend().await?;
            let ctx = commands::make_ctx(confirm_mode, yes);
            commands::set_props(
                &backend,
                &ctx,
                path,
                content_type,
                cache_control,
                content_encoding,
                content_language,
                content_disposition,
            )
            .await
        }
        Command::Container { action } => {
            let backend = auth.resolve_backend().await?;
            let ctx = commands::make_ctx(confirm_mode, yes);
            match action {
                ContainerAction::Create {
                    name,
                    public_access,
                } => commands::container_create(&backend, &ctx, name, public_access).await,
                ContainerAction::Delete { name } => {
                    commands::container_delete(&backend, &ctx, name).await
                }
                ContainerAction::SetAccess { name, access } => {
                    commands::container_set_access(&backend, &ctx, name, access).await
                }
            }
        }
        Command::Tags { path } => {
            let backend = auth.resolve_backend().await?;
            commands::tags(&backend, path, format).await
        }
        Command::SetTags { path, pairs } => {
            let backend = auth.resolve_backend().await?;
            let ctx = commands::make_ctx(confirm_mode, yes);
            commands::set_tags(&backend, &ctx, path, pairs).await
        }
        Command::Snapshot { path } => {
            let backend = auth.resolve_backend().await?;
            commands::snapshot(&backend, path).await
        }
        Command::Undelete { path } => {
            let backend = auth.resolve_backend().await?;
            commands::undelete(&backend, path).await
        }
        Command::Rehydrate { path, tier, high } => {
            let backend = auth.resolve_backend().await?;
            let ctx = commands::make_ctx(confirm_mode, yes);
            commands::rehydrate(&backend, &ctx, path, tier, high).await
        }
        Command::Lease { action } => {
            let backend = auth.resolve_backend().await?;
            match action {
                LeaseAction::Acquire { path, duration } => {
                    commands::lease_acquire(&backend, path, duration).await
                }
                LeaseAction::Release { path, lease_id } => {
                    commands::lease_release(&backend, path, lease_id).await
                }
                LeaseAction::Break { path } => {
                    let ctx = commands::make_ctx(confirm_mode, yes);
                    commands::lease_break(&backend, &ctx, path).await
                }
            }
        }
        Command::Queue { action } => {
            let backend = auth.resolve_queue_backend().await?;
            match action {
                QueueAction::List => commands::queue_list(&backend, format).await,
                QueueAction::Create { name } => commands::queue_create(&backend, name).await,
                QueueAction::Delete { name } => {
                    let ctx = commands::make_ctx(confirm_mode, yes);
                    commands::queue_delete(&backend, &ctx, name).await
                }
                QueueAction::Put { name, text } => commands::queue_put(&backend, name, text).await,
                QueueAction::Peek { name, count } => {
                    commands::queue_peek(&backend, name, count, format).await
                }
                QueueAction::Get {
                    name,
                    count,
                    visibility,
                } => commands::queue_get(&backend, name, count, visibility, format).await,
                QueueAction::Clear { name } => {
                    let ctx = commands::make_ctx(confirm_mode, yes);
                    commands::queue_clear(&backend, &ctx, name).await
                }
            }
        }
        Command::Doctor => commands::doctor(auth).await,
        Command::Account { action } => {
            let store = account::open_store()?;
            let secrets = account::keyring();
            match action {
                AccountAction::Add { name } => {
                    let parts = auth.connection_parts();
                    account::add(&store, &secrets, &name, &parts)?;
                    println!("saved account '{name}' (credentials stored in the OS keychain)");
                    Ok(())
                }
                AccountAction::Discover => {
                    let s = discover::discover(&store, &secrets).await?;
                    println!(
                        "discovered {} subscription(s), {} storage account(s) \
                         ({} ready to use, {} metadata-only)",
                        s.subscriptions, s.accounts, s.with_keys, s.metadata_only
                    );
                    if s.accounts > 0 {
                        println!("run `arkived account list`, then `arkived account use <name>`.");
                    }
                    Ok(())
                }
                AccountAction::List => output::print_accounts(&account::list(&store)?, format),
                AccountAction::Use { name } => {
                    account::use_account(&store, &name)?;
                    println!("active account is now '{name}'");
                    Ok(())
                }
                AccountAction::Current => {
                    let ctx = account::current(&store)?;
                    match ctx.account_name {
                        Some(name) => println!("active account: {name}"),
                        None => println!("no active account (use `arkived account use <name>`)"),
                    }
                    Ok(())
                }
                AccountAction::Forget { name } => {
                    account::forget(&store, &secrets, &name)?;
                    println!("forgot account '{name}'");
                    Ok(())
                }
            }
        }
        Command::Login { tenant } => {
            let store = account::open_store()?;
            let secrets = account::keyring();
            let sign_in =
                arkived_core::auth::entra::login::device_login(&store, &secrets, &tenant, |dc| {
                    use std::io::Write;
                    println!(
                        "\nTo sign in, open:\n  {}\nand enter code:  {}\n",
                        dc.verification_uri, dc.user_code
                    );
                    println!("Waiting for you to finish signing in…");
                    // Flush so the code shows immediately even when stdout is
                    // piped (block-buffered), not just on a TTY.
                    let _ = std::io::stdout().flush();
                })
                .await?;
            println!(
                "signed in as {} (tenant {})",
                sign_in.user_principal, sign_in.tenant_id
            );
            println!(
                "note: browsing storage with this sign-in needs account discovery \
                 (next increment); for now use `account add`/`use` or direct credentials."
            );
            Ok(())
        }
        Command::Logout => {
            let store = account::open_store()?;
            let secrets = account::keyring();
            match store.context_get()?.sign_in_id {
                None => {
                    println!("no active sign-in");
                    Ok(())
                }
                Some(sign_in_id) => {
                    let who = store
                        .sign_in_get(&sign_in_id)?
                        .map(|s| s.user_principal)
                        .unwrap_or_else(|| sign_in_id.clone());
                    arkived_core::auth::entra::login::logout(&store, &secrets, &sign_in_id)?;
                    println!("signed out {who}");
                    Ok(())
                }
            }
        }
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
