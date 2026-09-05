# Backend architecture

MyLib uses a pragmatic feature-oriented architecture. The goal is to make ownership obvious
without forcing every change through artificial layers.

## Dependency direction

```text
main -> app -> http -> features -> core
             |          |
             +----------+-> infrastructure
```

- `core` contains stable, shared contracts: configuration, application errors and common API
  models. It must not depend on product features.
- `infrastructure` owns adapters for external concerns such as SQLx databases and embedded web
  assets. It may use `core`, but it must not assemble HTTP routes.
- `features` contains product capabilities. A feature owns its handlers, DTOs and services.
  Cross-feature calls should use a small public function or type rather than reaching into
  another feature's private implementation.
- `http` composes feature routers and owns transport-wide endpoints.
- `app` creates shared state, configures middleware and wires infrastructure to features.
- `main.rs` only handles process lifecycle: configuration, listener, shutdown and background
  task startup.

## Source tree

```text
src/
├── app/mod.rs
├── core/
│   ├── config.rs
│   ├── errors.rs
│   └── models.rs
├── features/
│   ├── auth/
│   ├── catalog/         # browsing, identification, metadata and scanner
│   ├── libraries/       # library API and synchronization scheduler
│   ├── operations/
│   ├── playback/
│   └── recommendations/
├── http/api.rs
├── infrastructure/
│   ├── database.rs
│   └── web_assets.rs
└── bin/
```

Each directory has a `mod.rs` that declares its boundary. The crate root reexports the old
module names (`db`, `playback`, `scanner`, and others) to preserve compatibility. New internal
code should prefer canonical paths such as `crate::features::playback` and
`crate::infrastructure::database` when practical.

## Where new code belongs

- A new user-visible capability belongs in `src/features/<feature>/`.
- A new HTTP route belongs to the feature that owns the behavior; its router is merged by
  `src/http/api.rs`.
- Code shared by only one feature stays inside that feature.
- Stable types shared by several features may move to `core`.
- Database engines, object storage, metadata clients and similar external integrations are
  infrastructure adapters. Feature-specific adapters may stay inside their feature.
- Executables that are not the server belong in `src/bin/`.
- Schema changes require matching, versioned SQLite and MySQL migrations.

## Module size guideline

There is no hard line limit, but a module should be split when it has more than one reason to
change. Prefer the following names inside a large feature:

- `api.rs`: routes, extractors and HTTP response mapping;
- `service.rs`: use-case orchestration and business rules;
- `repository.rs`: feature-specific persistence queries;
- `models.rs`: feature-local request, response and domain types;
- `runtime.rs`: long-lived processes, workers and external commands.

Do not split code solely to satisfy a line count. A cohesive module is easier to understand
than many tiny forwarding files.

## Testing

- Unit tests stay beside the module they exercise in `#[cfg(test)]` blocks.
- End-to-end HTTP behavior belongs in `tests/`.
- Tests must not contact TMDB or depend on a developer's media library.
- Changes to database behavior should cover SQLite and keep the paired MySQL migration valid.

Run the complete local quality gate before opening a pull request:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
npm --prefix web run lint
npm --prefix web run typecheck
npm --prefix web test -- --run
npm --prefix web run build
```
