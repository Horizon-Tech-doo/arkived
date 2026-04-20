# Arkived

**A fast, open-source, Rust-native storage client for Microsoft Azure. Built for agents.**

[![Crates.io](https://img.shields.io/crates/v/arkived.svg)](https://crates.io/crates/arkived)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](./LICENSE-APACHE)
[![CI](https://github.com/Horizon-Tech-doo/arkived/actions/workflows/ci.yml/badge.svg)](https://github.com/Horizon-Tech-doo/arkived/actions/workflows/ci.yml)

Arkived is a modern, performant alternative to Microsoft Azure Storage Explorer, built in Rust. It ships as a CLI, an MCP server, an ACP host, and a Tauri desktop app — all powered by a single shared core.

> **Status:** 🚧 Pre-release. Active development. Not yet ready for production use.

## Why Arkived

- **Native performance.** Written in Rust. No Electron, no JVM, no Python startup lag.
- **Agent-ready by design.** First-class [Model Context Protocol (MCP)](https://modelcontextprotocol.io) and [Agent Client Protocol (ACP)](https://agentclientprotocol.com) support. Let LLMs and coding agents operate your Azure storage — safely, with scoped credentials and human-in-the-loop confirmation.
- **One engine, four surfaces.** CLI for scripts, MCP for LLMs, ACP for coding agents, Tauri for desktop users — all sharing the same Rust core.
- **Safety by default.** Every destructive operation flows through a policy layer that requires explicit confirmation. Agents cannot silently delete your data.
- **Open source.** Apache-2.0 licensed. Built in the open.

## Scope

Arkived is built primarily for **Microsoft Azure** — Blob Storage, Queues, Tables, File Shares, and Data Lake Gen2.

The core library (`arkived-core`) is structured with a `StorageBackend` abstraction that will make additional backends (S3, GCS, MinIO, Cloudflare R2) possible in the future, but Azure is the only first-class target today. We will only add other backends once the Azure experience is genuinely excellent.

## Install

```bash
cargo install arkived
```

Desktop app installers for macOS, Windows, and Linux will be published from [Releases](https://github.com/Horizon-Tech-doo/arkived/releases) once the GUI is available.

## Quick start

```bash
# Sign in with your Azure account
arkived login

# List storage accounts
arkived account list

# List containers in the default account
arkived ls

# Stream a blob to stdout
arkived cat mycontainer/myblob.json

# Copy a local file to Azure
arkived cp ./report.pdf mycontainer/reports/

# Run as an MCP server (stdio transport)
arkived mcp
```

## Architecture

See [`docs/architecture.md`](./docs/architecture.md) for the full design.

Briefly: `arkived-core` is a shared Rust library that implements all storage logic. The CLI, MCP server, ACP host, and Tauri app are thin wrappers. A `Policy` trait ensures every destructive operation routes through human-in-the-loop confirmation, regardless of which surface initiated it.

## Compatibility with Microsoft Azure

Arkived is an independent open-source project. It is not affiliated with, endorsed by, or sponsored by Microsoft Corporation. "Microsoft," "Azure," and "Microsoft Azure Storage Explorer" are trademarks of Microsoft Corporation. Arkived is designed to be compatible with Microsoft Azure Storage. See [`docs/trademark-compliance.md`](./docs/trademark-compliance.md) for details.

## Contributing

See [`CONTRIBUTING.md`](./CONTRIBUTING.md). Please read our [Code of Conduct](./CODE_OF_CONDUCT.md) before participating.

## Security

To report a security vulnerability, please see [`SECURITY.md`](./SECURITY.md). **Do not file public issues for security problems.**

## Authors

Arkived is developed by [Horizon Tech d.o.o.](https://horizon-tech.io), an IT consulting and software development company based in Sarajevo, Bosnia and Herzegovina.

Lead maintainer: [Hamza Abdagić](https://github.com/ghostrider0470), CEO & Founder.

## License

Copyright © 2026 Horizon Tech d.o.o.

Licensed under the Apache License, Version 2.0. See [`LICENSE-APACHE`](./LICENSE-APACHE) for the full text.
