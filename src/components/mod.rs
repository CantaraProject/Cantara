//! # Components Module
//!
//! This module contains all the Dioxus UI components used in Cantara. The components are organized
//! into submodules based on their functionality and the part of the application they serve.
//!
//! ## Module Structure
//!
//! - [`selection_components`]: Components for selecting songs and other content for presentations
//! - [`presentation_components`]: Components for rendering and displaying presentations
//! - [`presentation_design_settings_components`]: Components for customizing presentation appearance
//! - [`settings_components`]: Components for application settings
//! - [`shared_components`]: Reusable components shared across different parts of the application
//! - [`wizard_components`]: Components for the first-time setup wizard
//! - [`font_settings`]: Components for font configuration (private module)
//!
//! ## Important Usage Notes
//!
//! ### State Management
//!
//! All Dioxus state management primitives (Signals, Memos, and effects) must be created within
//! these component modules. Creating them in the [`crate::logic`] module will likely cause runtime
//! exceptions due to how Dioxus manages component lifecycles.
//!
//! ### Example
//!
//! ```rust
//! // Correct: Creating signals within a component
//! #[component]
//! fn MyComponent() -> Element {
//!     let counter = use_signal(|| 0);
//!     // ...
//! }
//!
//! // Incorrect: Creating signals in a logic module function
//! // This may cause runtime exceptions
//! fn initialize_state() -> Signal<i32> {
//!     use_signal(|| 0) // Don't do this!
//! }
//! ```
//!
//! ## Component Design Principles
//!
//! Components in Cantara follow these design principles:
//!
//! 1. **Single Responsibility**: Each component should have a clear, focused purpose
//! 2. **Composability**: Complex UIs are built by composing smaller, simpler components
//! 3. **Reusability**: Common UI patterns are extracted into reusable components
//! 4. **Separation of Concerns**: UI components are separated from business logic

pub mod selection_components;

pub mod presentation_components;

pub mod presentation_design_settings_components;

pub mod settings_components;

pub mod shared_components;

pub mod wizard_components;

mod font_settings;