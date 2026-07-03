use crate::components::shared_components::{ImageIcon, MarkdownIcon, MusicIcon, PdfIcon};
use crate::logic::settings::{default_sidebar_order, use_settings, SelectionSidebarType};
use dioxus::prelude::*;

/// This component renders a sidebar for the selection where the user can filter the sources.
/// The order of the icons is determined by `settings.sidebar_order` and can be reordered
/// via drag and drop. Changes are persisted to settings automatically.
#[component]
pub(crate) fn SelectionFilterSideBar(active_selection: Signal<SelectionSidebarType>) -> Element {
    let mut settings = use_settings();

    let mut order: Signal<Vec<SelectionSidebarType>> = use_signal(|| {
        let s = settings.read();
        if s.sidebar_order.is_empty() {
            default_sidebar_order()
        } else {
            s.sidebar_order.clone()
        }
    });

    let mut dragging_from: Signal<Option<usize>> = use_signal(|| None);
    let mut hover_over: Signal<Option<usize>> = use_signal(|| None);
    let mut drag_completed: Signal<bool> = use_signal(|| false);

    rsx! {
        div {
            class: "selection-sidebar",
            onmouseup: move |_| {
                let did_drag = if let (Some(from), Some(to)) = (dragging_from(), hover_over()) {
                    if from != to {
                        let mut new_order = order.read().clone();
                        let len = new_order.len();
                        if from < len && to < len {
                            new_order.swap(from, to);
                            order.set(new_order.clone());
                            settings.write().sidebar_order = new_order;
                            settings.read().save();
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                };
                drag_completed.set(did_drag);
                dragging_from.set(None);
                hover_over.set(None);
            },
            onmouseleave: move |_| {
                dragging_from.set(None);
                hover_over.set(None);
            },
            for (idx, filter_type) in order.read().clone().iter().enumerate() {
                {
                    let ft = *filter_type;
                    rsx! {
                        div {
                            key: "{idx}",
                            role: "button",
                            class: if active_selection() == ft { "outline" } else { "outline secondary" },
                            style: {
                                let mut s = String::from("padding: 12px; cursor: grab;");
                                if dragging_from().is_some() && hover_over() == Some(idx) {
                                    s.push_str(" outline: 2px dashed #888; background-color: rgba(0,0,0,0.05);");
                                }
                                s
                            },
                            onmousedown: move |_| {
                                drag_completed.set(false);
                                dragging_from.set(Some(idx));
                                hover_over.set(Some(idx));
                            },
                            onmouseenter: move |_| {
                                if dragging_from.read().is_some() {
                                    hover_over.set(Some(idx));
                                }
                            },
                            onclick: move |_| {
                                if drag_completed() {
                                    drag_completed.set(false);
                                } else {
                                    active_selection.set(ft);
                                }
                            },
                            match ft {
                                SelectionSidebarType::Songs => rsx! { MusicIcon {} },
                                SelectionSidebarType::Pictures => rsx! { ImageIcon {} },
                                SelectionSidebarType::Pdfs => rsx! { PdfIcon {} },
                                SelectionSidebarType::Markdown => rsx! { MarkdownIcon {} },
                            }
                        }
                    }
                }
            }
        }
    }
}
