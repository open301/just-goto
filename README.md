# JustGoto

A self-hosted short link redirection service with a web UI, built on top of Rust and Node.

JustGoto uses Rust for the API and Database, and Node for the frontend.

[![Demo video](https://raw.githubusercontent.com/open301/just-goto/main/.github/assets/demo.mp4)](https://raw.githubusercontent.com/open301/just-goto/main/.github/assets/demo.mp4)

## Quick start

Requires Node 26 and Rust (version pinned in `rust-toolchain.toml`).

```sh
cp .env.example .env
# Set ADMIN_USERNAME and ADMIN_PASSWORD in .env.
npm ci
npm run build:all
./target/release/justgoto
```

Open `http://localhost:3000` and sign in with those credentials. Set `HOST`, `PORT`, and `DB_PATH` in `.env` as needed. Short links use `/{slug}` and support 301, 302, 307, and 308 redirects.

## Development

Run `npm run build` once, then `cargo run --locked` for the backend and `npm run dev` for the frontend. Set `JUSTGOTO_API_URL` if the backend uses a different address.

| Command                | Description                               |
| ---------------------- | ----------------------------------------- |
| `npm run build`        | Check and build the frontend              |
| `npm run build:all`    | Build the release binary with UI          |
| `npm run check`        | Check frontend types, lint, and format    |
| `npm run check:rust`   | Check Rust lint, format, and tests        |
| `npm run check:all`    | Run all checks and build the frontend     |
| `npm run format:all`   | Format code and Markdown                  |
| `npm run generate:api` | Update API types from the running backend |

## API

An API is provided and there's an OpenAPI schema available to use with a Swagger frontend.

- Interactive docs: `/swagger-ui/`
- OpenAPI schema: `/api-docs/openapi.json`

You can import CSV files in the UI or send a multipart `file` field to `POST /api/v1/redirects/import`:

```csv
slug,destination,redirect_code,is_enabled
example,https://example.com,302,true
docs,https://example.com/docs,,
```

`slug` and `destination` are required. Blank `redirect_code` and `is_enabled` values default to `307` and `true`. Limit: 10 MiB and 10,000 rows.

## Contributing

- **Workflows**: Simple workflows should use `ubuntu-slim`. Actions must pin commit hashes.
- **Runtimes**: Node 26 (`.nvmrc`), Rust pinned via `rust-toolchain.toml`.
- **Dependencies**: Pin versions and provide justification when adding new dependencies. Dependabot handles automated updates.
- **Git**: Follow Conventional Commits (`feat(api): ...`) and use type-prefixed branch names (`feat/...`, `fix/...`).
