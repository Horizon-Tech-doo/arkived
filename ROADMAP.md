# Roadmap

Arkived is pre-1.0. This document tracks what's shipped, what's next, and the
exit criteria for each milestone. Milestones are listed in order of delivery,
with no calendar dates — we ship when the exit criteria are met.

For context on the architectural principles that shape this plan, see
[`docs/architecture.md`](./docs/architecture.md).

---

## Where we are

### ✅ Stage 0 — Name reservation & scaffolding (`0.0.1`)

Shipped. `arkived` is published on [crates.io](https://crates.io/crates/arkived)
and the repository hosts the workspace layout plus project docs.

- Workspace with `arkived-core` and `arkived-cli`
- `StorageBackend` trait (`pub(crate)`) and `Policy` trait skeletons in core
- CLI skeleton with placeholder subcommands (clap)
- Apache-2.0 license, Contributor Covenant v2.1, SECURITY policy
- CI: fmt, clippy, test matrix (Linux/macOS/Windows), MSRV 1.75

### 🟡 Stage 3 (partial) — Desktop app shell

The Tauri 2 + React + TypeScript app in `app/` is scaffolded with the design
ported pixel-for-pixel, but it runs on **mock data**. Exit criteria for a real
`0.3.0` release are in the Stage 3 section below.

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

- Queue, Table, and File Share operations (Azure). Scope is in `arkived-core`
  from the start, but CLI/app surfaces land after v0.1.0 Blob support stabilizes.
- Azurite local-emulator connection.
- Team / multi-user policy presets.
- Managed cloud hosting (`arkived.app` as a service, not just a download page).

If you want one of these sooner, open an issue — we'll move it based on demand.
