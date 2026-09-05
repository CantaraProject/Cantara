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
