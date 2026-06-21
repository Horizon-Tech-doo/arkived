# Arkived MCP server

Arkived ships an [MCP](https://modelcontextprotocol.io) server so LLM agents can
explore and operate Azure Blob Storage through a standard protocol — safely.

- **Read-only tools** run with no confirmation: `list_containers`, `list_blobs`,
  `read_blob`, `get_properties`, `get_metadata`.
- **Destructive / elevated tools** require **human approval** via an MCP
  *elicitation* before they run: `write_blob`, `delete_blob`, `copy_blob`,
  `generate_sas`, `set_access_tier`. If the client cannot elicit (no elicitation
  capability) or the human declines, the operation is refused.

The server reads its Azure credentials from the environment, so they live in the
MCP client's config, not on a command line.

## Running

```bash
# As a standalone binary:
arkived-mcp

# Or via the CLI subcommand (same server):
arkived mcp
```

Credentials (set in the client config's `env`):

| Variable | Purpose |
| --- | --- |
| `ARKIVED_CONNECTION_STRING` | Full Azure Storage connection string |
| `ARKIVED_ACCOUNT` + `ARKIVED_ACCOUNT_KEY` | Account name + key |
| `ARKIVED_ACCOUNT` + `ARKIVED_SAS` | Account name + SAS token |
| `ARKIVED_AZURITE=1` | Local Azurite emulator |

## Claude Desktop

Add to `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "arkived": {
      "command": "arkived-mcp",
      "env": {
        "ARKIVED_CONNECTION_STRING": "DefaultEndpointsProtocol=https;AccountName=...;AccountKey=...;EndpointSuffix=core.windows.net"
      }
    }
  }
}
```

## Claude Code

```bash
claude mcp add arkived \
  --env ARKIVED_CONNECTION_STRING="DefaultEndpointsProtocol=https;AccountName=...;AccountKey=...;EndpointSuffix=core.windows.net" \
  -- arkived-mcp
```

Or add the same `mcpServers` block to your project's `.mcp.json`.

## Safety model

Every destructive tool calls back to the connected client to ask a human before
acting, mirroring the `Policy` gate the CLI and desktop app enforce. An agent
can freely browse a storage account read-only, but it cannot write, delete,
copy, change tiers, or mint a SAS without a visible approval step.
