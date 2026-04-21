# Changelog

All notable changes to Arkived will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Initial workspace scaffolding
- `arkived-core` crate with `StorageBackend` and `Policy` trait skeletons
- `arkived-cli` binary entry point
- Project documentation (README, architecture, trademark compliance)
- CI pipeline (fmt, clippy, test, MSRV)

### Changed

- **MSRV bumped from 1.88 to 1.88.** Required by the current Microsoft
  Azure SDK for Rust (`azure_storage_blob 0.11`). Updated
  `rust-toolchain.toml`, workspace `rust-version`, and CI matrix.
- **MSRV bumped from 1.75 to 1.85.** Modern crates in the foundation
  dependency chain (`uuid`, `keyring`, transitive `getrandom 0.4.x`)
  require `edition2024`, stabilized in Rust 1.85 (Feb 2025). Was flagged
  as a likely requirement in the v0.1.0 design spec. Updated
  `rust-toolchain.toml`, workspace `rust-version`, and CI matrix
  accordingly.

## [0.0.1] — 2026-04-20

Initial name reservation release on crates.io. No functional code yet.
