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

## Docker

Create a `docker-compose.yml` file with this example:

```yaml
services:
    justgoto:
        image: zephnx/just-goto:alpine # Change the tag to debian-slim for the Debian image.
        restart: unless-stopped
        ports:
            - '3000:3000' # Change the first port to use a different host port.
        environment:
            ADMIN_USERNAME: admin # Change the initial administrator username.
            ADMIN_PASSWORD: change-this-password # Change this before starting.
            HOST: 0.0.0.0
            PORT: '3000'
            DB_PATH: /data/db.sqlite3
        labels:
            com.centurylinklabs.watchtower.enable: 'true'
            com.centurylinklabs.watchtower.scope: justgoto # Change all three scope values together.
        volumes:
            - justgoto-data:/data

    watchtower:
        image: nickfedor/watchtower:latest
        profiles: ['watchtower']
        restart: unless-stopped
        environment:
            WATCHTOWER_LABEL_ENABLE: 'true'
            WATCHTOWER_SCOPE: justgoto
            WATCHTOWER_POLL_INTERVAL: '86400' # Update interval in seconds; this is 24 hours in seconds
        labels:
            com.centurylinklabs.watchtower.scope: justgoto
        volumes:
            - /var/run/docker.sock:/var/run/docker.sock

volumes:
    justgoto-data:
```

Change `ADMIN_PASSWORD` in the file before starting the application:

```sh
docker compose pull
docker compose up -d
```

Open `http://localhost:3000`. The database and administrator are initialized on the first start; changing the credentials in Compose later does not change the existing account. SQLite data lives in `/data/db.sqlite3` on the `justgoto-data` named volume and survives container replacement.

Use `docker compose logs -f` to view logs and `docker compose down` to stop the service. `docker compose down --volumes` also deletes the database volume.

### Automatic updates with Watchtower

To enable automatic updates, start the optional Watchtower service:

```sh
docker compose --profile watchtower up -d
docker compose --profile watchtower logs -f watchtower
```

The example uses the maintained [Watchtower fork](https://github.com/nicholas-fedor/watchtower). It checks the selected image tag every 24 hours and recreates the JustGoto container when that tag has a new image. The named database volume is preserved, but the service will be briefly unavailable during an update. Use a moving tag such as `alpine` or `debian-slim` to receive new releases; a version-specific tag only changes if it is republished.

Watchtower selects containers with the `justgoto` scope and enable label. If you run multiple copies of this example, give each copy a different scope by changing all three scope values in its Compose file. Watchtower needs access to the Docker socket, which gives it control over the Docker host.

To stop automatic updates, run `docker compose --profile watchtower stop watchtower`. To stop the whole stack when the profile is enabled, run `docker compose --profile watchtower down`.

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
