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

Once Rust is installed, you can install the Dioxus CLI:

```bash
cargo install dioxus-cli --locked
```

The CLI and the `dioxus` crates have to be the same version. A CLI older than
the crates fails at runtime, not at build time: assets are serialised into the
binary by the crates and read back by the CLI, so a mismatch surfaces as
`Failed to deserialize asset. Make sure you built with the matching version of
the Dioxus CLI` — a panic in whichever component first uses an asset. Check with
`dx --version` against the `dioxus` version in `Cargo.toml`, and pin the CLI if
they drift apart:

```bash
cargo install dioxus-cli --version 0.7.10 --locked
```

### Building Cantara

Clone the repository and build the application:

```bash
git clone https://github.com/CantaraProject/Cantara.git
cd Cantara
cargo build --release
```

The compiled binary will be available in the `target/release` directory.

## Development

To develop the app locally, run the following command in the root of your project:

```bash
dx serve
```

This will start a development server with hot reloading.

## Running the tests

There are three suites, and they cover different things because they can *see*
different things. The reasoning behind the split is in
[`docs/specs/0004-testing-playwright.md`](docs/specs/0004-testing-playwright.md);
this is how to run them.

### The Rust tests — most of the coverage

```bash
cargo test
```

Around 780 tests: the logic, the settings migration, the network side, and the
markup every kind of slide produces in every kind of design. No browser, no
window, a couple of seconds.

Clippy is part of the suite rather than an extra, because the rule that nothing
outside tests may `unwrap` or `expect` is a lint and `cargo test` does not run
lints:

```bash
cargo clippy --all-targets -- -D warnings
```

A handful of tests only exist in the harness build (below), so occasionally:

```bash
cargo test --features test-harness
cargo clippy --all-targets --features test-harness -- -D warnings
```

### The browser tests — the stream as a phone sees it

```bash
npm install                 # once
npx playwright install chromium   # once
npm run test:browser
```

Playwright starts **Cantara itself** and drives the page a phone in the
congregation would open. It is the real network side — the real helper process,
the real rendering, the real socket — with a test asking over HTTP instead of an
operator pressing keys. See [`src/logic/harness.rs`](src/logic/harness.rs).

The harness is behind the `test-harness` cargo feature and is *not* in a release
build: it opens a port and takes instructions on it, which is precisely what the
rest of the program is careful not to do.

Starting it by hand is useful when something fails and you want to look at the
page yourself:

```bash
cargo run --features test-harness -- --test-harness
```

It then serves three addresses — `/` for the congregation, `/stage` and
`/speaker` for stage monitors — and takes instructions on port 8431:

```bash
curl "http://127.0.0.1:8431/show?service=video"   # names from logic::fixtures
curl "http://127.0.0.1:8431/show?service=song&widgets=yes"
curl "http://127.0.0.1:8431/next"
curl "http://127.0.0.1:8431/stop"
```

If it refuses to start saying the control port is taken, an earlier harness is
still running — that is deliberate, and better than quietly moving to another
port and leaving the tests waiting on an address nothing answers.

### The window check — what no browser can reach

```bash
npm run test:window
```

Opens the real projection window and checks the geometry of what came out: that
the stage fills the screen, that a slide is scaled to fit rather than to nothing,
that it does not overflow the box it was scaled into. Needs a display; on a
headless machine put `xvfb-run -a` in front of it.

Geometry rather than screenshots on purpose — a picture comparison goes red for
every font update, and every failure worth catching here is a number. What it
**cannot** see is whether anything was painted: an element of the right size in
black on black measures perfectly. Somebody still has to look at the screen
before a release.

### In CI

All three run on every push, in
[`.github/workflows/dioxus.yml`](.github/workflows/dioxus.yml). The Rust tests
run on Linux, Windows and macOS; the browser tests and the window check run on
Linux only, because every surface they reach is drawn by a browser the runner
brings with it and a second operating system would exercise the same code twice.

A failing browser run uploads its report as an artifact. The window check is
`continue-on-error` until it has proved itself on a runner.

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
