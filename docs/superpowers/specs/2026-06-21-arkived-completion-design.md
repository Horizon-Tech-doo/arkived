# Arkived completion — design spec

**Date:** 2026-06-21
**Goal:** Move Arkived from "core works, desktop works, CLI is an empty shell" to a
coherent, genuinely-working product across every surface, in dependency order.

## Context: where the project stands today

- **`arkived-core`** — strong. Real Azure Blob backend (`AzureBlobBackend`) with
  auth (SAS, account key, connection string, Entra device-code + browser,
  Azurite), `list_containers`, `list_blobs`, `read_blob`, `write_blob`
  (streaming + block staging), `delete_blob`, `copy_blob` — all policy-gated.
  Plus a SQLite `Store`, TOML config discovery, `Policy`/`ProgressSink` traits.
- **Tauri desktop app (`app/`)** — most complete surface. ~6,100 lines of IPC
  wired end-to-end to core. Real sign-in, ARM discovery, browse,
  upload/download/delete/rename/copy.
- **`arkived-cli`** — empty shell. Every subcommand prints "Not yet implemented."
  This is the published `cargo install arkived` binary.
- **MCP server / ACP host** — do not exist yet.

## Verification

The user has **live Azure credentials** and will provide a connection string / SAS
/ account for live testing. Integration tests also run against the local **Azurite**
emulator (existing `#[ignore]`d tests). Every phase is verified against real Azure
before it is considered done.

## Decisions locked during brainstorming

1. **Scope:** all four phases below, built and verified in dependency order. This
   is a sequence of independently-shippable increments, not one monolith.
2. **SAS:** implement **account-key (Service SAS)** first. User-delegation SAS
   (AAD-signed) is explicitly deferred to a later increment.
3. **CLI state:** the CLI **shares the core `Store` + OS keyring** with the
   desktop app. `arkived login` in the terminal is visible in the GUI and
   vice-versa.

---

## Phase 1 — Fill the core gaps

New blob/container operations in `crates/arkived-core/src/backend/azure/ops/`,
each mirroring the existing op pattern: an `impl AzureBlobBackend` block, a
`ctx.policy.confirm(...)` gate for any destructive/elevated action, requests sent
via `HttpPipeline { http, credential }` + `RequestTemplate`, and a `#[cfg(test)]`
module using `FakeAuth` + `DenyAllPolicy` to prove the policy gate short-circuits
before HTTP. Each new public method is also added to the `StorageBackend` trait
and re-exported where the existing ops are.

### 1.1 `generate_sas` (account-key Service SAS) — policy-gated

- New file `ops/generate_sas.rs`. Signature roughly:
  `pub fn generate_sas(&self, ctx, path: SasResource, opts: SasOpts) -> Result<String>`
  (synchronous — pure signing, no network).
- `SasResource` is container-or-blob; `SasOpts` carries permissions
  (`r/w/d/l/a/c/...`), start/expiry times, protocol, optional IP range, and
  signed resource type.
- Reuses the existing `auth/shared_key.rs` HMAC-SHA256 machinery to build the
  string-to-sign per the Azure Service SAS spec, then assembles the query string.
- **Requires the account key.** When the active credential is not key-based
  (SAS-only, Entra, anonymous), return a clear `Error` explaining that account-key
  SAS needs the storage key (and that user-delegation SAS is a future feature).
- Policy gate: `verb: "generate_sas"`, summary names the resource + granted
  permissions + expiry. Generating a write/delete-capable SAS is an elevated
  action and must route through `Policy::confirm`.

### 1.2 `set_access_tier` — policy-gated

- New file `ops/set_access_tier.rs`. `PUT {container}/{blob}?comp=tier` with
  `x-ms-access-tier: Hot|Cool|Cold|Archive`.
- Tier enum in `backend/types.rs`. Policy-gated (changing to/from Archive has cost
  + latency implications; `ActionContext` notes rehydration where relevant).

### 1.3 Properties & metadata

- `ops/properties.rs`: `get_properties` (`HEAD {blob}` → parse system properties:
  content-type, length, etag, last-modified, tier, lease state, etc. into a
  `BlobProperties` struct in `backend/types.rs`) and `set_properties`
  (`PUT ...?comp=properties`). `set_properties` is policy-gated (mutation).
- `ops/metadata.rs`: `get_metadata` (`HEAD` → collect `x-ms-meta-*`) and
  `set_metadata` (`PUT ...?comp=metadata`, policy-gated).

### 1.4 Container lifecycle

- `ops/container.rs`: `create_container` (`PUT {container}?restype=container`,
  optional `x-ms-blob-public-access`), `delete_container`
  (`DELETE {container}?restype=container`, policy-gated), and
  `set_container_public_access` (`PUT ...?restype=container&comp=acl`).

**Phase 1 exit:** all new ops compile, unit tests prove policy gating, and an
Azurite integration test exercises create-container → set-tier → set-metadata →
generate-sas (where the emulator supports it) → delete-container. Account-key SAS
is verified against live Azure (generate, then use the SAS to read a blob).

---

## Phase 2 — Wire the CLI end-to-end

Replace the `bail!("Not yet implemented")` scaffold in
`crates/arkived-cli/src/main.rs` with real command handlers. The CLI grows from a
single file into a small module tree (`main.rs` + `cmd/` for handlers + `output.rs`
for formatting + `policy.rs` + `context.rs` for resolving the active backend) so no
one file does too much.

### 2.1 Shared infrastructure

- **`CliPolicy`** (`policy.rs`): implements `Policy`. On a destructive action,
  prints a summary and prompts on stdin (`"delete mycontainer/file.txt? [y/N]"`).
  Honors `--yes` (auto-allow) and the config `ConfirmMode` (`Ask`/`Auto`/`Yes`).
  Non-interactive stdin with no `--yes` → `Deny`.
- **Backend resolution** (`context.rs`): mirrors the desktop's `build_backend`
  dispatch. Resolution order for "which account + auth do I use":
  1. Explicit flags / env (`--connection-string`, `ARKIVED_CONNECTION_STRING`,
     `--sas`, `--account` + key from keyring).
  2. `.arkived.toml` / `~/.config/arkived/config.toml`.
  3. The shared `Store` `CurrentContext` (set by `arkived account use`) — the
     account name resolves to a `StorageAccount` row; the credential comes from
     the OS keyring or the cached Entra token for the context's sign-in.
- **Output** (`output.rs`): a `render(value, OutputFormat)` helper so every
  read command supports `--format json|yaml|table|tsv`. `table` is the default
  for human use; `json`/`yaml`/`tsv` for scripting. Serializable view structs per
  command (container list, blob list, properties).
- **Progress:** a `ProgressSink` implementation that drives an indicatif-style
  progress bar (or a minimal hand-rolled one to avoid a new heavy dep — decided in
  the plan) for `cp` uploads/downloads.

### 2.2 Commands

- `arkived login` — Entra **device-code** flow (existing
  `EntraDeviceCodeProvider`); persist a `SignIn` to the shared `Store`, cache the
  token via the existing Entra cache, set it as the current context's sign-in.
- `arkived account list` — list `StorageAccount` rows from the `Store`.
  `arkived account use <name>` — set `CurrentContext` account.
- `arkived ls [container[/prefix]]` — no arg lists containers; a container path
  lists blobs (virtual-folder view via `/` delimiter; `--recursive` for flat).
- `arkived cat <container/blob>` — stream `read_blob` to stdout (supports `--range`).
- `arkived cp <src> <dst>` — local→remote (`write_blob`), remote→local
  (`read_blob`), remote→remote (`copy_blob`). Direction inferred from whether each
  side parses as a `container/blob` path or a local filesystem path.
- `arkived rm <container/blob>` — `delete_blob`, policy-gated, `--yes` to skip.
- `arkived sas <container[/blob]>` — `generate_sas`, policy-gated, prints the URL.
  Flags for permissions/expiry.
- `arkived set-tier <container/blob> <hot|cool|cold|archive>` — `set_access_tier`.
- `arkived doctor` — verify config discovery, that a backend can be built, that a
  trivial `list_containers` succeeds, and report auth/network/permission status in
  actionable language.

**Phase 2 exit:** a developer with the live account can run
`login → account use → ls → cat → cp (both directions) → rm → sas → set-tier →
doctor` end-to-end against real Azure, with `--format` honored on read commands and
confirmation prompts on destructive ones. CLI sign-in is visible in the desktop app.

---

## Phase 3 — MCP server

- New crate `crates/arkived-mcp` producing an `arkived-mcp` binary, plus an
  `arkived mcp` subcommand on the CLI that launches it over **stdio**.
- Built on the official Rust MCP SDK (`rmcp`). Each core op is exposed as an MCP
  tool: read-only tools (`list_containers`, `list_blobs`, `read_blob`,
  `get_properties`, `get_metadata`) require no confirmation; destructive/elevated
  tools (`write_blob`, `delete_blob`, `copy_blob`, `generate_sas`,
  `set_access_tier`, `set_metadata`, `create_container`, `delete_container`) route
  through an **`McpPolicy`** that issues an MCP **elicitation** the human must
  approve before the op runs.
- Tool schemas document their destructive scope so an agent can plan safely.
- Ship reference configs for Claude Desktop and Claude Code.

**Phase 3 exit:** an MCP client configured with `arkived mcp` can explore a storage
account read-only with zero confirmations, and cannot perform any destructive
operation without a user-visible elicitation step. (Detailed tool schema + auth
wiring refined into a plan when this phase starts.)

---

## Phase 4 — Desktop polish

Consume the Phase 1 core ops in the Tauri app:

- New IPC commands `generate_sas`, `set_blob_tier`, `get_blob_properties`,
  `set_blob_properties`, `get_blob_metadata`, `set_blob_metadata` in
  `app/src-tauri/src/commands.rs`, each delegating to the new core ops and routing
  destructive ones through the app's existing policy/activity machinery.
- UI: a blob **properties/metadata panel**, an **access-tier** action in the blob
  context menu, and a **SAS generation** dialog (permissions + expiry → copyable
  URL).

**Phase 4 exit:** SAS generation, tier changes, and property/metadata viewing+editing
are usable from the desktop UI against the live account. (UI layout refined into a
plan when this phase starts.)

---

## Cross-cutting principles

- **Reuse, don't duplicate.** The CLI and desktop both build backends from the
  same auth providers and the same `Store`; Phase 2 extracts the
  credential→backend resolution so it isn't reimplemented per surface where it can
  be shared in core.
- **Policy is non-negotiable.** Every new destructive/elevated op calls
  `Policy::confirm`. Each surface supplies its own `Policy` (CLI stdin, MCP
  elicitation, Tauri modal).
- **TDD per op.** Each core op lands with unit tests proving the policy gate and,
  where the emulator supports it, an Azurite integration test.
- **One file, one job.** New CLI handlers and core ops stay small and focused;
  large files (e.g. the desktop `commands.rs`) are extended, not bloated further
  than necessary.

## Out of scope (consciously deferred)

- User-delegation (AAD-signed) SAS.
- Managed identity / workload identity auth providers.
- Azure Files, Queues, Tables, ADLS Gen2 ACLs, managed disks (Storage Explorer
  parity milestone v0.6).
- ACP host (roadmap v0.4), second backend (v0.5), signed installers/auto-update
  (v0.3).
