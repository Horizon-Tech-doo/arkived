# Contributing to Arkived

Thanks for considering a contribution. Arkived is an open-source project developed by [Horizon Tech d.o.o.](https://horizon-tech.io), and we welcome contributions from the community.

## Before you start

- Read the [Code of Conduct](./CODE_OF_CONDUCT.md). All contributors are expected to follow it.
- Check open [issues](https://github.com/Horizon-Tech-doo/arkived/issues).
- For significant changes, open an issue first to discuss the approach.

## Development setup

```bash
git clone https://github.com/Horizon-Tech-doo/arkived.git
cd arkived
cargo build --workspace
cargo test --workspace
```

Requirements:

- Rust 1.85+ (install via [rustup](https://rustup.rs))
- An Azure storage account for integration tests (optional for unit tests)

## Coding standards

- Run `cargo fmt --all` before committing.
- Run `cargo clippy --workspace --all-targets -- -D warnings` and fix all lints.
- All public items must have rustdoc comments.
- New functionality requires tests.

## Commit messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
feat(cli): add `arkived ls` pagination support
fix(core): handle 429 throttling in blob uploads
docs(readme): clarify MCP quickstart
```

## Pull requests

1. Fork the repository.
2. Create a feature branch: `git checkout -b feat/my-feature`.
3. Make your changes, add tests, update docs.
4. Ensure `cargo fmt`, `cargo clippy`, and `cargo test` all pass.
5. Open a PR against `main`.
6. A maintainer will review. Expect feedback.

## License

By contributing, you agree your contributions will be licensed under Apache 2.0.

## Trademark compliance

If you contribute docs or marketing content, read [`docs/trademark-compliance.md`](./docs/trademark-compliance.md). Content must not improperly use Microsoft trademarks.

## Contact

Questions? Email `hamza.abdagic@horizon-tech.io` or open a discussion on GitHub.
