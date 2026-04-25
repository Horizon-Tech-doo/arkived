# Roadmap

Arkived is pre-1.0. This document tracks what's shipped, what's next, and the
exit criteria for each milestone. Milestones are listed in order of delivery,
with no calendar dates — we ship when the exit criteria are met.

For context on the architectural principles that shape this plan, see
[`docs/architecture.md`](./docs/architecture.md).

The product target has moved from "Blob client with a Storage Explorer-like
shell" to **full Azure Storage Explorer parity, then beyond parity**. The
capability contract lives in
[`docs/storage-explorer-parity-and-beyond.md`](./docs/storage-explorer-parity-and-beyond.md).

---

## Where we are

### ✅ Stage 0 — Name reservation & scaffolding (`0.0.1`)

Shipped. `arkived` is published on [crates.io](https://crates.io/crates/arkived)
and the repository hosts the workspace layout plus project docs.

- Workspace with `arkived-core` and `arkived-cli`
- `StorageBackend` trait (`pub(crate)`) and `Policy` trait skeletons in core
- CLI skeleton with placeholder subcommands (clap)
- Apache-2.0 license, Contributor Covenant v2.1, SECURITY policy
- CI: fmt, clippy, test matrix (Linux/macOS/Windows), MSRV 1.85

### 🟡 Stage 3 (partial) — Desktop app shell

The Tauri 2 + React + TypeScript app in `app/` now has live Azure account
sign-in, ARM discovery, storage account activation, container tabs, context
menus, and persisted account metadata. It is not Storage Explorer parity yet:
most write operations, File Shares, Queues, Tables, managed disks, transfer
jobs, SAS workflows, direct links, and advanced Blob/ADLS features are still
planned work.

---

## v0.1.0 — Azure Blob operations (the real unlock)

This is the biggest milestone. Everything else is a surface on top.

- [ ] `AuthProvider` trait in `arkived-core` with implementations:
  - SAS (shared access signature)
  - Connection string
  - AAD / Entra interactive
  - Managed identity
  - Workload identity
- [ ] `AzureBackend` implementation of the internal `StorageBackend` trait
  - List containers, list blobs (with prefix + recursion)
  - Read blob (streaming)
  - Write blob (streaming, resumable upload)
  - Delete blob (policy-gated)
  - Copy blob
  - Generate SAS URL (policy-gated)
  - Set access tier (policy-gated)
  - Get/set properties and metadata
- [ ] CLI verbs wired end-to-end:
  - [ ] `arkived login` (interactive AAD, device code fallback)
  - [ ] `arkived account list` / `arkived account use`
  - [ ] `arkived ls`
  - [ ] `arkived cat`
  - [ ] `arkived cp` (local↔remote, remote↔remote)
  - [ ] `arkived rm` (with policy confirm)
  - [ ] `arkived sas` (policy confirm for write/delete scopes)
  - [ ] `arkived doctor` (verify auth, network, permissions)
- [ ] `--format json|yaml|table|tsv` on every command
- [ ] Config discovery: `.arkived.toml` → `~/.config/arkived/config.toml` → `ARKIVED_*` env
- [ ] `ProgressEvent` stream for long operations
- [ ] Tauri desktop app: IPC commands in `app/src-tauri` replace the mocks and
      delegate to `arkived-core`. UI still renders identically to the scaffold.
- [ ] Integration tests against a real Azure storage account (CI secret)
- [ ] README quick-start produces a working `cargo install arkived` → `arkived ls` flow

**Exit criteria:** a developer with an Azure storage account can install, auth,
list, read, write, and delete blobs from either CLI or desktop app.

---

## v0.2.0 — MCP server

Agents can drive Arkived over the [Model Context Protocol](https://modelcontextprotocol.io).

- [ ] New crate: `crates/arkived-mcp/` with an `arkived-mcp` binary
- [ ] Every Stage 1 verb exposed as an MCP tool (list, read, write, delete, copy, sas, set-tier)
- [ ] MCP-specific `Policy` implementation using MCP elicitation
- [ ] `arkived mcp` subcommand on the CLI that launches the MCP server over stdio
- [ ] Reference configurations for Claude Desktop and Claude Code
- [ ] Tool schemas document their destructive scope so agents can plan safely

**Exit criteria:** an LLM client configured with `arkived mcp` can explore a
storage account read-only without any confirmation, and cannot perform any
destructive operation without a user-visible elicitation step.

---

## v0.3.0 — Desktop app (production-ready)

The UI shell already exists; this milestone takes it from "renders nicely" to
"installable and trustworthy."

- [ ] Replace placeholder icons with a real brand icon set (all Tauri targets)
- [ ] Tauri-specific `Policy` implementation using native modal dialogs
- [ ] Command palette (⌘K) executes real navigation and actions
- [ ] Activity queue reflects real transfers with live progress
- [ ] Tauri app menu + keyboard accelerators
- [ ] Installers from GitHub Releases:
  - [ ] macOS `.dmg` (signed + notarized)
  - [ ] Windows `.msi` (signed)
  - [ ] Linux `.AppImage`, `.deb`, `.rpm`
- [ ] Auto-update via Tauri updater plugin
- [ ] First-run onboarding: sign in with Azure → pick subscription → done

**Exit criteria:** a non-developer can download an installer from the Releases
page, run it, authenticate once, and browse their storage. No CLI required.

---

## v0.6.0 — Storage Explorer parity

This milestone supersedes the old "defer Files, Queues, Tables" posture. It is
done only when the workflows in
[`docs/storage-explorer-parity-and-beyond.md`](./docs/storage-explorer-parity-and-beyond.md)
are implemented deeply enough that a daily Storage Explorer user can move to
Arkived without losing required functionality.

- [ ] Blob container parity: create/delete, public access, leases, properties,
      stored access policies, SAS, soft-delete policy, deleted container flows
- [ ] Blob parity: upload/download/open/copy/rename/delete, properties,
      metadata, tags, tiers, rehydrate, snapshots, versions, soft delete,
      undelete, immutability, legal hold
- [ ] ADLS Gen2 parity: HNS directory operations, ACL view/edit, recursive ACL
      propagation, DFS/blob endpoint handling
- [ ] Azure Files parity: shares, directories, files, quotas, snapshots, SAS,
      stored access policies, SMB helper
- [ ] Queue parity: queues, messages, peek/dequeue/delete, clear, metadata,
      SAS, stored access policies
- [ ] Table parity: tables, OData query, add/edit/delete entities, CSV
      import/export, SAS, stored access policies
- [ ] Managed disk parity: VHD upload/download, disk copy, snapshots
- [ ] Direct links and SAS direct links
- [ ] Activities pane with cancel/retry/progress/logs for all transfer jobs
- [ ] Settings/diagnostics/accessibility parity: proxy, token cache,
      high-contrast themes, key-usage control, signed-in identity diagnostics

**Exit criteria:** the parity checklist is green in desktop UI and the same
operations are available through reusable core APIs and CLI commands where
appropriate.

---

## v0.7.0 — Beyond Storage Explorer

Once parity is real, Arkived differentiates on safety, automation, diagnostics,
and agent-native workflows.

- [ ] Dry-run plans for bulk delete/copy/tier/ACL/metadata operations
- [ ] Cost and risk estimates before destructive or billable operations
- [ ] Auth/network/RBAC/private-endpoint diagnostics that explain failures in
      actionable language
- [ ] Agent-readable job plans and operation graphs
- [ ] MCP server and ACP host integrated with the same Policy gates
- [ ] Saved workspaces, global resource search, and repeatable transfer recipes
- [ ] Local audit log for destructive operations and generated SAS links

---

## v0.4.0 — ACP host

Coding agents can run inside Arkived via the [Agent Client Protocol](https://agentclientprotocol.com).

- [ ] New crate: `crates/arkived-acp/` implementing an ACP host
- [ ] Launch targets: Claude Code, Gemini CLI, Codex (starting with Claude Code)
- [ ] ACP-specific `Policy` forwarding through ACP's permission flow
- [ ] Embedded terminal surface inside the Tauri app
- [ ] `arkived serve-acp` CLI subcommand for headless ACP mode

**Exit criteria:** a user can open an agent session in the desktop app, type a
goal ("archive all blobs older than 90 days"), and the agent proposes steps
that route through the same confirmation UI as a human-initiated destructive
operation.

---

## v0.5.0 — Second backend

Only after the Azure experience is genuinely excellent.

- [ ] Decide: promote `StorageBackend` to a public trait, or add a parallel
      `arkived::s3::*` surface. The decision is deferred to this milestone on
      purpose — we don't know what the right abstraction is until we have a
      second concrete backend in hand.
- [ ] Implement the first non-Azure backend (likely S3 or S3-compatible).
- [ ] Update docs and marketing to reflect multi-cloud scope.
- [ ] Migration notes for any public-API break.

**Exit criteria:** both backends pass the same integration test suite, and at
least one real workflow is validated end-to-end against the non-Azure backend.

---

## Not on the roadmap (yet)

Listed here so we remember we considered them and consciously deferred:

- Team / multi-user policy presets.
- Managed cloud hosting (`arkived.app` as a service, not just a download page).

If you want one of these sooner, open an issue — we'll move it based on demand.
