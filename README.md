# Cantara - Song Presentation Software
![GitHub branch check runs](https://img.shields.io/github/check-runs/CantaraProject/cantara/master)

## Overview

Cantara is a powerful song presentation software designed for churches and small groups. It allows you to quickly create beautiful presentations with song lyrics, sheet music, PDF presentations and more. This repository contains version 3.0, a complete rewrite of [the original Cantara](https://github.com/reckel-jm/cantara) in Rust using the Dioxus framework.

*Work is currently in progress.* Contributions are welcome!

**Try it out**:
You can find a [web browser demo version](https://cantaraproject.github.io/Cantara)  with limited features of Cantara.

### Key Features

- **Song Lyrics Presentation**: Display song lyrics and scores with configurable formatting
- **Present multiple content types**: Beside song presentations, Cantara supports PDF files, pictures, videos and Markdown files
- **Presentation Styling**: Customize the appearance of your presentations
- **Multi-platform**: Works on Windows, macOS, and Linux as well as the Web. Android and iOS are going to be implemented soon.
- **Network Streaming**: Cantara natively implements network streaming of the presentation and a presentation console for remote control
- **User-friendly Interface**: Easy to use for both technical and non-technical users
- **Repository Management**: Organize songs and other presentation types from multiple sources
- **Remote Repository Support**: Download and use song collections from remote sources

## Installation

Ready-made packages for Windows, Linux (.deb, AppImage, Flatpak), macOS and Android are attached to every [release](https://github.com/CantaraProject/Cantara/releases). How they are made is described in [`packaging/README.md`](packaging/README.md).

To build Cantara yourself:

### Prerequisites

- Rust (latest stable version)
- Dioxus CLI, **at the same version as the `dioxus` dependency in `Cargo.toml`**

### Installing Rust

If you don't have Rust installed, you can install it using [rustup](https://rustup.rs/):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

For Windows, download and run the installer from the [rustup website](https://rustup.rs/).

### Installing Dioxus CLI

Once Rust is installed, you can install the Dioxus CLI. Cantara currently
builds on a pre-release of Dioxus (0.8.0-alpha.3), which `cargo install` does
not pick on its own, so the version has to be named:

```bash
cargo install dioxus-cli --version 0.8.0-alpha.3 --locked
```

The CLI and the `dioxus` crates have to be the same version. A CLI older than
the crates fails at runtime, not at build time: assets are serialised into the
binary by the crates and read back by the CLI, so a mismatch surfaces as
`Failed to deserialize asset. Make sure you built with the matching version of
the Dioxus CLI` — a panic in whichever component first uses an asset. Check with
`dx --version` against the `dioxus` version in `Cargo.toml`, and pin the CLI if
they drift apart:

```bash
cargo install dioxus-cli --version 0.8.0-alpha.3 --locked
```

### Building Cantara

Clone the repository and build the application:

```bash
git clone https://github.com/CantaraProject/Cantara.git
cd Cantara
cargo build --release
```

The compiled binary will be available in the `target/release` directory.

## Running a Cantara server

Besides the program, this repository contains a server
(`crates/cantara-server`). It holds an organisation's libraries, designs,
slide settings and running orders, and lets a running presentation be watched
and controlled from anywhere through a link. The desktop program logs in to
one or several servers under *Settings → Online instances*. Their libraries
then become repositories, cached on the computer so that they keep working
offline. The server also serves the browser version of Cantara, which works
with what is on that server. What it is and why it is built this way is
explained in [`docs/specs/0008-server-version.md`](docs/specs/0008-server-version.md).

```bash
cp deploy/.env.example deploy/.env    # then fill in the passwords and the address
docker compose -f deploy/docker-compose.yml up -d
```

The server is then reachable at `http://localhost:8080`. The administrator
from `deploy/.env` can log in there. The API is described at
`/api/v1/docs` (OpenAPI document: `/api/v1/openapi.json`, also committed as
[`crates/cantara-server/openapi.json`](crates/cantara-server/openapi.json)).
Files are kept in the `/data` volume, or in S3-compatible storage
(`CANTARA_S3_*` in `deploy/.env.example`). Everything else is kept in
PostgreSQL 18. A backup is a `pg_dump` together with a copy of the volume or
the bucket. The server is
licensed under the AGPL-3.0-or-later.

To run it without Docker against a PostgreSQL of your own:

```bash
DATABASE_URL=postgres://cantara:secret@localhost/cantara \
CANTARA_BOOTSTRAP_ADMIN_EMAIL=admin@example.org \
CANTARA_BOOTSTRAP_ADMIN_PASSWORD=change-me-too \
cargo run -p cantara-server
```

## Development

To develop the app locally, run the following command in the root of your project:

```bash
dx serve
```

This will start a development server with hot reloading.

## Running tests

Cantara is texted extensively to ensure a high stability and code quality. All tests run as part of the CI pipeline. However, they can also be manually started during the development process.

### Rust module and integration tests

```bash
cargo test
```

The Rust tests cover the logic, the settings migration, the network side, and the
markup every kind of slide produces in every kind of design. No browser.

In addition to that, `clippy` is used to enforce an idiomatic code syntax:

```bash
cargo clippy --all-targets -- -D warnings
```

There is a `test-harness` build feature which allows some additional remote control for additional testing purposes.

```bash
cargo test --features test-harness
cargo clippy --all-targets --features test-harness -- -D warnings
```

### The server

The server's tests need a PostgreSQL. Each test creates a database of its own
on it. Without `CANTARA_TEST_DATABASE_URL` the database tests are skipped.

```bash
CANTARA_TEST_DATABASE_URL=postgres://cantara@127.0.0.1:5432/postgres cargo test -p cantara-server -p cantara-api
cargo clippy -p cantara-server -p cantara-api --all-targets -- -D warnings
```

A change to the API changes the committed OpenAPI document, and a test fails
until it is regenerated with `UPDATE_OPENAPI=1 cargo test -p cantara-server`.

The server mode in a browser has a Playwright suite of its own. It covers the
web build a server serves, the desktop login through the browser and with a
code, and a presentation shared with its pictures. It needs a database it may
wipe:

```bash
CANTARA_BROWSER_DATABASE_URL=postgres://cantara@127.0.0.1:5432/cantara_browser npm run test:server
```

### Browser tests via Playwright

```bash
npm install                 # once
npx playwright install chromium   # once
npm run test:browser
```

### The window test

```bash
npm run test:window
```

Opens the real projection window and tests the geometry towards the window behavior.
### In CI

## Project Structure and Documentation

The project is documented with Rust's documentation features.
Generate documentation with `cargo doc` to explore the structure and the meaning of the modules and symbols.

High-level structure:

- `src/main.rs`: app bootstrap, routing, and shared context setup
- `src/components/`: Dioxus UI components (pages and reusable widgets)
- `src/logic/settings.rs`: persistent settings, repository configuration, and settings file I/O
- `src/logic/states.rs`: in-memory runtime state for selections and running presentations
- `src/logic/presentation.rs`: presentation assembly and content transformation helpers
- `src/logic/sourcefiles.rs`: source-file discovery and type classification
- `src/logic/selection_io.rs`: saving a running order to a file and opening one
- `src/logic/server/`: Cantara as a client of a Cantara server (login, cached libraries, shared presentations)
- `crates/cantara-api/`: the types the server and every client exchange
- `crates/cantara-server/`: the server (axum, PostgreSQL), see `deploy/` for running it

File formats:

- [`docs/formats/cantara-zip.md`](docs/formats/cantara-zip.md): the `.cantara.zip`
  selection file — what is in the archive, what the manifest says, and what the
  two Cantara 2 formats (`.songtex` and Cantara 2's selection JSON) can and
  cannot carry.
- [`docs/formats/cantara-design.md`](docs/formats/cantara-design.md): handing a
  single presentation design (`.cantara-design.zip`, with its background picture
  and its fonts) or a single slide division (`.cantara-slides.json`) to somebody
  else.

Configuration:

- [`docs/tag-mapping.md`](docs/tag-mapping.md): reading one collection's tag
  names as another's, so that a meta line asking for `{{composer}}` still fills
  for a song whose file says `author`. No file is changed by it.

## Implementation Status

As this is a rewrite, the implementation status is not directly comparable to the original Cantara repository. The following table shows the features that are currently implemented or planned for this version:

| Feature | Status |
| --- | --- |
| Song Lyrics Presentation | ✅ Implemented |
| Presentation Styling |  ✅ Implemented |
| Chord Presentation | ✅ Implemented |
| Image Presentation |  ✅ Implemented |
| PDF Presentation |  ✅ Implemented |
| Search Functionality | ✅ Implemented |
| Import SongText Files | ✅ Implemented |
| Export SongText Files | ✅ Implemented |
| Export pptx Files | ✅ Implemented |

## Contributing

Contributions are welcome! If you'd like to contribute to Cantara, please follow these steps:

1. Fork the repository.
2. Create a new branch for your feature or bugfix.
3. Make your changes.
4. Write tests for your changes if applicable.
5. Run the existing tests to ensure your changes don't break anything.
6. Submit a pull request.

If you would like additional features for Cantara, please feel free to open an issue or a pull request.

### Code Style

- Follow the Rust standard code style.
- Use meaningful variable and function names.
- Write clear and concise documentation comments.
- Avoid `unwrap` calls in production code, you can use `ùnwrap_or_else` or `unwrap_or_default` instead.

## License

This project is licensed under the terms of AGPL. See the [COPYING](COPYING) file for details.
