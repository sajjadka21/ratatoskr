# Engineering Rules

## Architecture

Rust backend is authoritative.

React must never implement or independently own critical download state.

Do not silently change architecture.

## Security

Never store or log:

- cookies
- passwords
- authorization headers
- access tokens
- private browser credentials

## Development

Work milestone-by-milestone.

Do not begin future features unless explicitly requested.

Every backend behavior must include appropriate tests.

Before finishing a change, run:

cargo fmt --all
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo check --workspace
npm run build

## Product priorities

Prefer:

1. correctness
2. crash recovery
3. download integrity
4. reliability
5. performance
6. additional features

Do not implement media extraction, torrents, cloud sync, accounts,
telemetry, or browser integration unless the active milestone requires it.
