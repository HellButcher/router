# Router

Rust workspace — a routing engine with a web server, importer, and algorithm crates.

## Structure

- `src/` — main binary (CLI, server startup)
- `crates/algorithm/` — routing algorithm
- `crates/import-pbf/` — OSM PBF importer
- `crates/server/` — axum HTTP server
- `crates/service/` — routing service logic
- `crates/storage/` — storage layer
- `crates/types/` — shared types
- `crates/polyline/` — polyline encoding
- `frontend/` — web frontend

## Commands

- `cargo build` — build all crates
- `cargo test` — run tests
- `cargo fmt` — format (edition 2024)
- `cargo clippy` — lint

## Notes

- Rust edition 2024, resolver 3
- `openapi` feature flag gates OpenAPI support (enabled by default)
