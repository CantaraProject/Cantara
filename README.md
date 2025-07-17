# Cantara - Song Presentation Software
![GitHub branch check runs](https://img.shields.io/github/check-runs/CantaraProject/cantara/master)

## Overview

Cantara is a powerful song presentation software designed for churches and small groups. It allows you to quickly create beautiful presentations with song lyrics, chords, sheet music, and more. This repository contains version 3.0, a complete rewrite of [the original Cantara](https://github.com/reckel-jm/cantara) in Rust using the Dioxus framework.

*Work is currently in progress.* Contributions are welcome!

### Key Features

- **Song Lyrics Presentation**: Display song lyrics with beautiful formatting
- **Presentation Styling**: Customize the appearance of your presentations
- **Multi-platform**: Works on Windows, macOS, and Linux
- **User-friendly Interface**: Easy to use for both technical and non-technical users
- **Repository Management**: Organize songs from multiple sources
- **Remote Repository Support**: Download and use song collections from remote sources

## Installation

### Prerequisites

- Rust (latest stable version)
- Dioxus CLI

### Installing Rust

If you don't have Rust installed, you can install it using [rustup](https://rustup.rs/):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

For Windows, download and run the installer from the [rustup website](https://rustup.rs/).

### Installing Dioxus CLI

Once Rust is installed, you can install the Dioxus CLI:

```bash
cargo install dioxus-cli
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

## Implementation Status

As this is a rewrite, the implementation status is not directly comparable to the original Cantara repository. The following table shows the features that are currently implemented or planned for this version:

| Feature | Status |
| --- | --- |
| Song Lyrics Presentation | ✅ Implemented |
| Presentation Styling | Partially implemented |
| Chord Presentation | ❌ Not Implemented, in Progress |
| Image Presentation | ❌ Not Implemented, in Progress |
| PDF Presentation | ❌ Not Implemented, in Progress |
| Search Functionality | ✅ Implemented |
| Import SongText Files | ❌ Not Implemented, in Progress |
| Export SongText Files | ❌ Not Implemented, in Progress |
| Export pptx Files | ❌ Not Implemented, in Progress |

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