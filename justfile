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

boundaries_file := "data/country_boundaries.geojson"
boundaries_url := "https://raw.githubusercontent.com/nvkelso/natural-earth-vector/master/geojson/ne_10m_admin_0_countries.geojson"

download-boundaries:
    #!/usr/bin/env sh
    if [ ! -f "{{boundaries_file}}" ]; then
        mkdir -p data
        curl -fL -o "{{boundaries_file}}" "{{boundaries_url}}"
    fi

import pbf='../osm-pbf-benchmark/ireland-and-northern-ireland-latest.osm.pbf': download-boundaries
    cargo run --release -- import {{pbf}}

serve: build
    cargo run --release serve

