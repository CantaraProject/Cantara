//! This submodule contains shared components which can be reused among different parts of the program.

use dioxus::prelude::*;
use dioxus_free_icons::icons::fa_regular_icons::FaTrashCan;
use dioxus_free_icons::Icon;

#[component]
pub fn DeleteIcon() -> Element {
    rsx! {
        Icon {
            icon: FaTrashCan,
        }
    }
}
