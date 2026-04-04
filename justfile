# https://just.systems
set dotenv-load := true

default:
    echo 'Hello, world!'

build: build-backend build-frontend
check: check-backend check-frontend
fmt: fmt-backend fmt-frontend

build-backend:
    cargo build --release

check-backend:
    cargo clippy

fmt-backend:
    cargo fmt

[working-directory: 'frontend']
build-frontend:
    pnpm install
    pnpm run build

[working-directory: 'frontend']
check-frontend:
    pnpm run check

[working-directory: 'frontend']
fmt-frontend:
    pnpm run fmt

import pbf='../osm-pbf-benchmark/ireland-and-northern-ireland-latest.osm.pbf':
    cargo run --release -- import {{pbf}}
    
serve: build
    cargo run --release serve

