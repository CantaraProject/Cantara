use crate::components::shared_components::{ImageIcon, MarkdownIcon, MusicIcon, PdfIcon};
use crate::logic::sourcefiles::SourceFileType;
use crate::logic::states::SelectedItemRepresentation;
use dioxus::prelude::*;
use dioxus_free_icons::icons::fa_regular_icons::FaTrashCan;
use dioxus_free_icons::icons::fa_solid_icons::{FaArrowDown, FaArrowUp};
use dioxus_free_icons::Icon;
use rust_i18n::t;

#[component]
pub(crate) fn SelectedItems(
    selected_items: Signal<Vec<SelectedItemRepresentation>>,
    active_selected_item_id: Signal<Option<usize>>,
) -> Element {
    let mut dragging_from: Signal<Option<usize>> = use_signal(|| None);
    let mut hover_over: Signal<Option<usize>> = use_signal(|| None);
    let mut anim_target: Signal<Option<usize>> = use_signal(|| None);
    let mut anim_flip: Signal<bool> = use_signal(|| false);

    rsx! {
        div {
            class: "selected-container",
            onmouseup: move |_| {
                if let (Some(from), Some(to)) = (dragging_from(), hover_over())
                    && from != to {
                        let mut items = selected_items.write();
                        let len_before = items.len();
                        if from < len_before && to <= len_before {
                            let item = items.remove(from);
                            let insert_at = if to > from { to - 1 } else { to };
                            let final_index = insert_at;
                            items.insert(insert_at, item);
                            anim_target.set(Some(final_index));
                            anim_flip.set(!anim_flip());
                        }
                    }
                dragging_from.set(None);
                hover_over.set(None);
            },
            onmouseleave: move |_| {
                dragging_from.set(None);
                hover_over.set(None);
            },
            for (number , _) in selected_items.read().iter().enumerate() {
                SelectedItem {
                    selected_items,
                    id: number,
                    active_selected_item_id,
                    dragging_from,
                    hover_over,
                    anim_target,
                    anim_flip,
                }
            }
            if dragging_from().is_some() {
                div {
                    style: {
                        let active = hover_over() == Some(selected_items.read().len());
                        let mut s = String::from(
                            "height: 12px; margin-top: 6px; border-top: 2px dashed #bbb;",
                        );
                        if active {
                            s.push_str(" border-color: #666;");
                        }
                        s
                    },
                    onmouseenter: move |_| {
                        hover_over.set(Some(selected_items.read().len()));
                    },
                }
            }
        }
    }
}

#[component]
fn SelectedItem(
    selected_items: Signal<Vec<SelectedItemRepresentation>>,
    id: usize,
    active_selected_item_id: Signal<Option<usize>>,
    dragging_from: Signal<Option<usize>>,
    hover_over: Signal<Option<usize>>,
    anim_target: Signal<Option<usize>>,
    anim_flip: Signal<bool>,
) -> Element {
    let current_item = selected_items.read().get(id).cloned();
    let Some(current_item) = current_item else {
        return rsx! {};
    };

    let is_first = id == 0;
    let is_last = id + 1 >= selected_items.read().len();

    rsx! {
        div {
            role: "button",
            class: "outline secondary selection_item selected-item",
            style: {
                let mut s = String::from("cursor: grab;");
                if dragging_from().is_some() && hover_over() == Some(id) {
                    s.push_str(" outline: 2px dashed #888; background-color: rgba(0,0,0,0.03);");
                }
                if anim_target() == Some(id) {
                    s.push_str(" background-color: rgba(255,230,150,0.8);");
                }
                s
            },
            tabindex: 0,
            onmouseenter: move |_| {
                if dragging_from.read().is_some() {
                    hover_over.set(Some(id));
                }
            },
            onmouseup: move |_| {}, // If mouse is released over the same item, the container onmouseup will also handle it,
            // The whole row opens the item, not only the words on it. With the
            // handler on the title alone, the padding around it and the empty
            // space after a short name did nothing, so an item had to be hit
            // squarely on its name to be selected.
            onmousedown: move |_| {
                anim_target.set(None);
                dragging_from.set(Some(id));
                hover_over.set(Some(id));
            },
            onclick: move |_| { active_selected_item_id.set(Some(id)) },
            span { class: "selected-item-label",
                match current_item.source_file.file_type {
                    SourceFileType::Song => rsx! {
                        MusicIcon {}
                    },
                    SourceFileType::Image => rsx! {
                        ImageIcon {}
                    },
                    SourceFileType::Pdf => rsx! {
                        PdfIcon {}
                    },
                    SourceFileType::Markdown => rsx! {
                        MarkdownIcon {}
                    },
                    _ => rsx! {},
                }
                span { class: "selected-item-name", {current_item.source_file.name.clone()} }
            }

            // Every row keeps all three slots, whether or not it can use them:
            // the first row has nothing to move up and the last nothing to move
            // down, and leaving those buttons out shifted the remaining ones
            // sideways so that no column of icons lined up with the next. The
            // group never wraps either — a name long enough to take two lines
            // used to push the wastebasket onto a line of its own.
            span {
                class: "selected-item-actions",
                // A click on one of these acts on the row; it must not also
                // select it, and pressing one must not start a drag.
                onmousedown: move |event: Event<MouseData>| event.stop_propagation(),
                onclick: move |event: Event<MouseData>| event.stop_propagation(),
                if is_first {
                    span { class: "selected-item-action selected-item-action-empty" }
                } else {
                    span {
                        class: "selected-item-action",
                        title: t!("selection.move_up").to_string(),
                        onclick: move |_| {
                            selected_items.write().swap(id, id - 1);
                        },
                        Icon { icon: FaArrowUp }
                    }
                }
                if is_last {
                    span { class: "selected-item-action selected-item-action-empty" }
                } else {
                    span {
                        class: "selected-item-action",
                        title: t!("selection.move_down").to_string(),
                        onclick: move |_| {
                            selected_items.write().swap(id, id + 1);
                        },
                        Icon { icon: FaArrowDown }
                    }
                }
                span {
                    class: "selected-item-action",
                    title: t!("general.delete").to_string(),
                    onclick: move |_| {
                        if *active_selected_item_id.read() == Some(id) {
                            active_selected_item_id.set(None);
                        }
                        selected_items.write().remove(id);
                    },
                    Icon { icon: FaTrashCan }
                }
            }
        }
    }
}
