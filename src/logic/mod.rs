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
//! - [`settings`]: Manages application settings, presentation designs, and configuration
//! - [`states`]: Handles application state and runtime information
//! - [`sourcefiles`]: Manages source files (songs, images, etc.) and repositories
//! - [`presentation`]: Controls presentation creation and management
//! - [`conversions`]: Provides utilities for data conversion and transformation
//! - [`css`]: Handles CSS generation and styling
//! - [`search`]: Implements search functionality for finding songs and other content
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
//! // Access repositories and source files
//! let source_files = settings.get_sourcefiles();
//! ```

pub mod settings;

pub mod states;

pub mod sourcefiles;

pub mod presentation;

pub mod conversions;
pub mod css;
pub mod search;