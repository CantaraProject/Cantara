//! # Logic Module
//!
//! This module implements the core business logic of the Cantara application, separating it from
//! the UI components. It handles all the non-UI functionality, including:
//!
//! - Managing application settings and configuration
//! - Handling source files (songs, images, etc.)
//! - Running presentations
//! - Managing application state
//! - Performing conversions and transformations
//! - Implementing search functionality
//!
//! ## Module Structure
//!
//! - [`settings`]: Manages persistent application settings, repositories, and configuration I/O
//! - [`states`]: Handles in-memory runtime state for selections and running presentations
//! - [`sourcefiles`]: Manages source files (songs, images, etc.) and repositories
//! - [`presentation`]: Controls presentation creation and management
//! - [`conversions`]: Provides utilities for data conversion and transformation
//! - [`css`]: Handles CSS generation and styling
//! - [`search`]: Implements search functionality for finding songs and other content
//! - [`images`]: Gets a picture from the file system into a web view
//! - [`pdf`]: Talks to the PDF viewer that lives in the page
//! - [`stream`]: Offers the running presentation to browsers on the network
//! - [`parallel`]: Spreads the per-file work of a library scan over the cores
//!
//! ## Separation of Concerns
//!
//! The logic module is deliberately separated from the UI components to:
//!
//! 1. **Improve Testability**: Logic can be tested independently of the UI
//! 2. **Enhance Maintainability**: Changes to the UI don't affect the core business logic
//! 3. **Enable Reusability**: The same logic can be used with different UI implementations
//! 4. **Simplify Reasoning**: Easier to understand and reason about the application's behavior
//!
//! ## Usage Guidelines
//!
//! When working with the logic module:
//!
//! - Do not create Dioxus state primitives (Signals, Memos, effects) in logic functions
//! - Use pure functions where possible, avoiding side effects
//! - Handle errors explicitly rather than using `unwrap()` or `expect()`
//! - Prefer returning `Result` or `Option` types over throwing exceptions
//! - Document public functions and types thoroughly
//!
//! ## Example
//!
//! ```rust
//! // Using the settings module to load application settings
//! use crate::logic::settings::Settings;
//!
//! // Load settings from storage or create default settings
//! let settings = Settings::load();
//!
//! // Read the library of every configured repository
//! let source_files = Settings::sourcefiles_of_async(&settings.repositories).await;
//! ```

pub mod settings;

pub mod states;

pub mod sourcefiles;

/// Only the web build has a use for the repositories that were embedded at
/// build time: it is the one without a file system to read them from.
#[cfg(target_arch = "wasm32")]
pub mod bundled_repos;

pub mod detail;
/// The second reading of the same service: what the network stream shows.
pub mod stream_view;
pub mod element_id;
pub mod images;
pub mod pdf;

/// Offering the running presentation to browsers on the local network.
/// There is no server in a browser, so the web build has none of this.
#[cfg(not(target_arch = "wasm32"))]
pub mod stream;
pub mod export;
pub mod fonts;
/// Writing the running order to a file and reading one back — Cantara 3's own
/// archive and the two formats Cantara 2 wrote.
pub mod selection_io;
pub mod pptx;
pub mod presentation;

pub mod conversions;
pub mod css;
pub mod search;

/// Reading a library is thousands of independent file reads, and only the
/// native builds have threads to spread them over.
#[cfg(not(target_arch = "wasm32"))]
pub mod parallel;

#[cfg(target_arch = "wasm32")]
pub mod sync;

#[cfg(feature = "desktop")]
pub mod screens;

/// Remembering the main window's size between sessions. Only the desktop has
/// a window whose size is the user's to choose.
#[cfg(feature = "desktop")]
pub mod window_state;
