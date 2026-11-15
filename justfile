# pi-kit v0.4.1
# The same recipes in every app (update them by following pi-kit's UPGRADING.md). App-specific recipes go
# at the end, below the marker. `just` alone lists them.

set dotenv-load
set shell := ["bash", "-euo", "pipefail", "-c"]

app := `sed -n 's/^name = "\(.*\)"$/\1/p' Cargo.toml | head -n1`
crate := replace(app, "-", "_")
version := `git describe --tags --always --dirty 2>/dev/null || echo dev`
target := "aarch64-unknown-linux-musl"
pi := env("PI_HOST", "amassa@raspberrypi.local")
# The instance on the Pi (folder ~/INSTANCE, compose project INSTANCE): the app, or e.g. a preview.
instance := env("INSTANCE", app)
compose := "docker compose -p " + instance + " -f compose.yml"
dev_db := "pi-dev-postgres"
dev_url := "postgres://postgres:postgres@127.0.0.1:55432"

# List the recipes
default:
    @just --list --unsorted

# Install tools, dependencies and git hooks (once after cloning; creates the lockfiles in a new app)
setup:
    mise install
    cargo fetch
    cd web && pnpm install
    lefthook install

# Start the local Postgres (PostGIS, as on the Pi; shared by all apps) and create this app's databases
db-up:
    #!/usr/bin/env bash
    set -euo pipefail
    if ! docker ps --format '{{{{.Names}}' | grep -qx {{ dev_db }}; then
      docker start {{ dev_db }} >/dev/null 2>&1 || docker run -d --name {{ dev_db }} \
        -p 127.0.0.1:55432:5432 -e POSTGRES_PASSWORD=postgres -v {{ dev_db }}:/var/lib/postgresql/data \
        imresamu/postgis:16-3.4-alpine3.22 >/dev/null
      # The image runs its init scripts on a socket-only server, then restarts: wait for TCP.
      for _ in $(seq 120); do docker exec {{ dev_db }} pg_isready -q -h 127.0.0.1 -U postgres && break; sleep 0.5; done
    fi
    for db in {{ crate }} {{ crate }}_test; do
      docker exec {{ dev_db }} psql -U postgres -tAc "SELECT 1 FROM pg_database WHERE datname = '$db'" | grep -q 1 \
        || docker exec {{ dev_db }} createdb -U postgres "$db"
    done
    [ -f .env ] || cp .env.example .env

# Stop the local Postgres (its data stays in the Docker volume)
db-down:
    docker stop {{ dev_db }}

# Run the API and the Vite dev server with hot reload: http://localhost:5173
dev: db-up
    #!/usr/bin/env bash
    set -euo pipefail
    trap 'kill 0' EXIT
    cargo run -- serve &
    (cd web && pnpm run dev) &
    wait

# Format all code
fmt:
    cargo fmt
    cd web && pnpm run format
    shfmt -w -i 2 deploy.sh

# Check formatting and lint everything (the pre-commit hook runs the same checks)
lint:
    cargo fmt --check
    cargo clippy --all-targets --locked -- -D warnings
    cd web && pnpm run lint && pnpm run check
    shellcheck deploy.sh
    shfmt -d -i 2 deploy.sh

# Run all tests (Rust against a fresh test database, then the web tests)
test: db-up
    docker exec {{ dev_db }} dropdb -U postgres --if-exists --force {{ crate }}_test
    docker exec {{ dev_db }} createdb -U postgres {{ crate }}_test
    TEST_DATABASE_URL={{ dev_url }}/{{ crate }}_test cargo test --locked -- --include-ignored
    cd web && pnpm test

# Refresh .sqlx/ (committed query metadata for offline builds) after changing a query or a migration
prepare: db-up
    SQLX_OFFLINE=false cargo sqlx migrate run
    SQLX_OFFLINE=false cargo sqlx prepare -- --all-targets

# Build the web app and the static arm64 binary into dist/app
build:
    cd web && pnpm run build
    cargo zigbuild --release --locked --target {{ target }}
    mkdir -p dist && cp "${CARGO_TARGET_DIR:-target}/{{ target }}/release/{{ app }}" dist/app
    @ls -lh dist/app

# Build the container image {{app}}:<git version>
image: build
    docker buildx build --platform linux/arm64 --provenance=false -t {{ app }}:{{ version }} --load .
    @docker image ls {{ app }}:{{ version }}

# Deploy this commit to the Pi (INSTANCE=name for another instance): backup, ship, start, check, roll back
deploy: image
    ./deploy.sh {{ app }} {{ version }}

# Switch the Pi back to the image that ran before the last deploy
rollback:
    ./deploy.sh --rollback {{ app }}

# Follow the app's logs on the Pi
logs:
    ssh {{ pi }} "cd ~/{{ instance }} && {{ compose }} logs -f --tail 100"

# Show what runs on the Pi
status:
    ssh {{ pi }} "cd ~/{{ instance }} && cat .deployed && {{ compose }} ps"

# ---- app-specific recipes below ----

# Create a user on the Pi (asks for the password)
create-user name:
    ssh -t {{ pi }} "cd ~/{{ instance }} && {{ compose }} exec app /app create-user {{ name }}"
