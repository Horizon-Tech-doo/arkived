# Arkived Architecture

## Overview

Arkived is structured as a **Rust workspace** with one shared core library and multiple surface-specific binaries. All business logic lives in `arkived-core`. The surfaces (CLI, MCP server, ACP host, Tauri app) are thin wrappers that delegate to the core.

```
┌─────────────────────────────────────────────────────────┐
│                    arkived-core                         │
│                                                         │
│  • Azure SDK bindings (Blob, Queue, Table, File)        │
│  • AuthProvider trait (SAS, AAD, managed identity, …)   │
│  • StorageBackend trait (pub(crate), internal)          │
│  • AzureBackend implementation                          │
│  • Policy trait (destructive-op confirmation)           │
│  • Progress event streams                               │
│  • Error types                                          │
└─────────────────────────────────────────────────────────┘
          │            │            │              │
          ▼            ▼            ▼              ▼
    ┌──────────┐ ┌──────────┐ ┌──────────┐  ┌──────────┐
    │ arkived  │ │ arkived- │ │ arkived- │  │ arkived- │
    │  (CLI)   │ │   mcp    │ │   acp    │  │   app    │
    │          │ │          │ │          │  │          │
    │ clap +   │ │ MCP tool │ │ ACP host │  │ Tauri +  │
    │ stdout   │ │ schema   │ │ for CC / │  │ React UI │
    │ prompts  │ │ over     │ │ Gemini / │  │          │
    │          │ │ stdio/   │ │ Codex    │  │          │
    │          │ │ HTTP     │ │          │  │          │
    └──────────┘ └──────────┘ └──────────┘  └──────────┘
```

## Design Principles

### Azure-first public API with a trait seam

The public API of `arkived-core` surfaces Azure concepts natively: containers, blobs, SAS tokens, access tiers, leases. We do not hide these behind a generic abstraction.

Internally, we maintain a `StorageBackend` trait (`pub(crate)`) that isolates backend-specific code. When a second backend is added, the team will decide then whether to promote the trait to public API or offer a parallel `arkived::s3::*` surface.

This avoids the common trap of over-abstracting on day one — we don't know what S3 support will need until we build it.

### One engine, four surfaces

All four surfaces call into the same `arkived-core` APIs. There is **one** implementation of "list blobs," "upload blob," etc. The surfaces differ only in:

- How they parse input (CLI args vs. MCP tool params vs. Tauri IPC)
- How they present output (stdout vs. JSON vs. GUI)
- How they implement the `Policy` trait (prompt vs. modal vs. elicitation vs. ACP permission)

### Policy-gated destructive operations

Every destructive operation — delete, overwrite, SAS generation, public access changes, tier changes — is gated behind `Policy::confirm()`. The core library itself never invokes destructive ops without a policy decision.

This is what makes Arkived safe for agents. Even if an LLM decides to "clean up old blobs," the tool will ask the human user first.

### Versatility

- **Auth:** `AuthProvider` trait supports SAS, connection string, AAD, managed identity, workload identity.
- **Output:** CLI supports `--format json|yaml|table|tsv`. JSON for machines, tables for humans.
- **Streaming:** Long operations return `Stream<ProgressEvent>`. Callers decide.
- **Config:** Standard discovery: `.arkived.toml` → `~/.config/arkived/config.toml` → env vars.

## Roadmap

See [`ROADMAP.md`](../ROADMAP.md) for current status and exit criteria per
milestone, and the [`CHANGELOG`](../CHANGELOG.md) for shipped work.
