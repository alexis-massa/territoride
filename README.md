# TerritoRide

Runs on the Raspberry Pi at https://territoride.duckdns.org. Built from the pi-kit template (Rust + axum + sqlx, Svelte 5
PWA embedded in the binary); the shared code lives in [pi-kit](https://github.com/alexis-massa/pi-kit) and is
pinned by tag in `Cargo.toml` and `web/package.json`.

## Commands

| Command | Does |
| --- | --- |
| `just setup` | once after cloning: tools (mise), web dependencies, git hooks |
| `just dev` | API + Vite with hot reload on http://localhost:5173 (starts the local Postgres) |
| `just fmt` / `just lint` | format / check everything (the pre-commit hook runs the checks) |
| `just test` | Rust tests against a fresh local database, then the web tests |
| `just prepare` | refresh `.sqlx/` after changing a query or a migration (commit it) |
| `just build` / `just image` | arm64 binary in `dist/app` / container image |
| `just deploy` / `just rollback` | ship the current commit to the Pi / go back to the previous image |
| `just logs` / `just status` | the app on the Pi |
| `just create-user NAME` | create a user on the Pi |

The binary itself: `territoride serve | migrate | healthcheck | create-user NAME | set-password NAME`.

## Layout

- `src/` the server: `main.rs` (CLI), `routes.rs` (HTTP), `users.rs` (users and passwords).
- `migrations/` SQL migrations, applied at startup; additive only, each with a rollback note.
- `web/` the Svelte app; `web/dist` is embedded into the binary at build time.
- `compose.yml`, `Dockerfile`, `deploy.sh`, `justfile`, `lefthook.yml`, `.gitignore`, `mise.toml`,
  `rust-toolchain.toml`, `.cargo/config.toml`: copied from pi-kit (header `# pi-kit vX.Y.Z`); change them by
  upgrading pi-kit, not by hand.

## Configuration

Environment only (see `.env.example` and pi-infra `APP_CONTRACT.md`): `DATABASE_URL`, `APP_DOMAIN`,
`BIND_ADDR`, `COOKIE_SECURE`, `SESSION_DAYS`, `RUST_LOG`.

## First deploy

On the Pi, from pi-infra: `./add-app.sh territoride --env-file ~/territoride/.env`, then add
`APP_DOMAIN=territoride.duckdns.org` to that file. Then here: `just deploy` and `just create-user territoride`.

A second instance (e.g. a preview on another host name, with its own database): `add-app.sh territoride-preview
--env-file ~/territoride-preview/.env`, its own `APP_DOMAIN`, then `INSTANCE=territoride-preview just deploy`. Instances share
the app's image tags, so deploy the live one again after removing a preview.
