//! The module contains the dioxus components of Cantara. The components are split into further submodules
//! according to the logic which they represent.
//!
//! ## Hint
//! Besides the [main] module, all Signals, Memos and effects have to be created from within these components.
//! If they are created from the [logic] crate, run time exceptions are likely to occur.

pub mod selection_components;

pub mod presentation_components;

pub mod presentation_design_settings_components;

pub mod settings_components;

pub mod shared_components;

pub mod wizard_components;
