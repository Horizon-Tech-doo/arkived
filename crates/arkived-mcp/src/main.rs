//! The `arkived-mcp` binary — runs the Arkived MCP server over stdio.

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Logs MUST go to stderr; stdout is the MCP JSON-RPC channel.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    arkived_mcp::run().await
}
