# Contributing to MyLib

Thank you for improving MyLib. Keep changes focused, testable and easy to review.

## Development setup

Requirements:

- Rust 1.98 or newer;
- Node.js 22 or newer;
- npm;
- FFmpeg and FFprobe for playback development.

Install the frontend dependencies once:

```bash
npm --prefix web ci
```

Run the backend and frontend in separate terminals:

```bash
cargo run
npm --prefix web run dev
```

The frontend runs at `http://localhost:5173` and proxies API calls to the backend at
`http://localhost:8096`.

## Before changing code

Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md). Put behavior in the feature that owns it and
avoid creating a new top-level module for feature-specific code.

For database changes, add the same numbered migration to both `migrations/sqlite` and
`migrations/mysql`. Never edit a released migration; create the next version instead.

## Quality gate

Run before submitting a pull request:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
npm --prefix web run lint
npm --prefix web run typecheck
npm --prefix web test -- --run
npm --prefix web run build
```

New behavior should include tests. Bug fixes should include a regression test whenever the
failure can be reproduced deterministically.

## Pull requests

- Use a concise title that describes the result.
- Explain the problem, the chosen solution and how it was verified.
- Keep unrelated formatting or refactors out of a functional change.
- Document new environment variables, routes and migrations.
- Never commit credentials, tokens, personal media, databases, `data/`, `target/`,
  `web/node_modules/` or generated test reports.

By contributing, you agree that your contribution is licensed under the repository's MIT
license.
