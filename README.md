# JustGoto

A self-hosted URL redirect service with a minimal React/TypeScript interface, user accounts, and role-based access.

[![Demo Video](https://raw.githubusercontent.com/zephyrbash/just-goto/main/.github/assets/demo.mp4)](https://raw.githubusercontent.com/zephyrbash/just-goto/main/.github/assets/demo.mp4)

## Features

- **Fast URL Redirects**: Manage short links with configurable HTTP redirect codes (301, 302, 307, 308).
- **Role-Based Access Control**: Admin, Editor, and User roles with session authentication.
- **Embedded Frontend**: Single standalone binary embeds the React 19 UI and Swagger UI assets—no separate web server or runtime assets needed.
- **Bulk CSV Import**: Import up to 10,000 redirects at once via web UI or API.
- **OpenAPI & Swagger**: Fully typed OpenAPI 3.0 specification with interactive documentation at `/swagger-ui/`.
- **Reliable Storage**: SQLite in WAL mode with automated migrations and connection pooling.

## Quick Start

1. **Configure environment variables**:

    ```sh
    cp .env.example .env
    # Edit .env with your desired PORT, DATABASE_PATH, ADMIN_USERNAME, ADMIN_PASSWORD, etc.
    ```

2. **Build and run**:

    ```sh
    npm ci
    npm run build:all
    ./target/release/justgoto
    ```

    Alternatively, for development:

    ```sh
    npm ci
    npm run build
    cargo run --locked
    ```

3. **Access the application**:
    - Web UI: `http://127.0.0.1:3000/` (or your configured `HOST:PORT`).
    - Sign in using the initial administrator credentials (`ADMIN_USERNAME` and `ADMIN_PASSWORD` in `.env`).
    - Public short links resolve at `/{slug}`.

## Development

### Running locally

- **Backend**: `cargo run --locked`
- **Frontend (Vite dev server with API proxy)**: `npm run dev`
    - Set `JUSTGOTO_API_URL` if the backend runs on a different port/host.

### Common commands

| Command                | Description                                                      |
| ---------------------- | ---------------------------------------------------------------- |
| `npm run dev`          | Start Vite dev server with hot reload                            |
| `npm run build`        | Check and build the frontend to `web/dist/`                      |
| `npm run build:all`    | Build frontend and compile the Rust release binary               |
| `npm run check`        | Run TypeScript, ESLint, and Prettier checks                      |
| `npm run check:rust`   | Run rustfmt check, Clippy, and cargo tests                       |
| `npm run check:all`    | Full verification (frontend check + build + Rust checks & tests) |
| `npm run format:all`   | Format frontend, Markdown, and Rust code                         |
| `npm run generate:api` | Regenerate TypeScript API types from the running backend         |

## API & Documentation

- **Swagger UI**: `/swagger-ui/`
- **OpenAPI Schema**: `/api-docs/openapi.json`

### Bulk CSV Import

Redirects can be bulk-imported via `POST /api/v1/redirects/import` (multipart `file` field) or through the web interface.

```csv
slug,destination,redirect_code,is_enabled
example,https://example.com,302,true
docs,https://example.com/docs,,
```

- `slug` and `destination` are required.
- `redirect_code` defaults to `307` if omitted.
- `is_enabled` defaults to `true` if omitted.
- Maximum file size: 10 MiB, up to 10,000 rows.

## Contributing

- **Workflows**: Simple workflows should use `ubuntu-slim`. Actions must pin commit hashes.
- **Runtimes**: Node 26 (`.nvmrc`), Rust pinned via `rust-toolchain.toml`.
- **Dependencies**: Pin versions and provide justification when adding new dependencies. Dependabot handles automated updates.
- **Git**: Follow Conventional Commits (`feat(api): ...`) and use type-prefixed branch names (`feat/...`, `fix/...`).
