# Contributing

## Ground rules

- **Registry, not scatter**: new settings go in
  `crates/faerie-core/src/config/schema.rs` (one entry) plus two strings in
  `locales/en-US.json`. Never read/write ad-hoc config files.
- **Tokens, not literals**: no colors, sizes, radii, shadows, or timings in
  components or CSS — add a theme token.
- **Keys, not strings**: user-facing text goes through `t("key")` and
  `locales/en-US.json`.
- **Commands, not inline handlers**: user-triggerable actions register in
  `ui/src/commands/registry.ts`.
- **Secrets**: any sensitive value is wrapped in `faerie_core::Secret` the
  moment it exists. Never format or log a raw token.
- **Scope**: this project does not accept gameplay cheat functionality or
  anti-cheat circumvention of any kind.

## Checks before pushing

```
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd ui && npm run build
```

## Structure

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md). Crates depend downward
only; the Tauri app layer stays logic-free; the UI talks to the backend only
through `ui/src/ipc/backend.ts`.
