//! This module implements the business logic of the Cantara application, such as
//! - selecting songs or other source files
//! - manipulating the settings
//! - running the presentation
//! It does not contain dioxus components by itself and therefore can be seperately tested.

pub mod settings;

pub mod states;

pub mod sourcefiles;

pub mod presentation;

pub mod css;
pub mod conversions;
