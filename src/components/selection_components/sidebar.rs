use crate::components::reorder::{GRIP_STYLE, refuse_native_drag, use_reorder_drag};
use crate::components::shared_components::{ImageIcon, MarkdownIcon, MusicIcon, PdfIcon, VideoIcon};
use crate::logic::reorder::{Layout, move_into_gap};
use crate::logic::settings::{default_sidebar_order, use_settings, SelectionSidebarType};
use dioxus::prelude::*;

/// The `id` of the sidebar, which the drag finds its scrolling area from.
const SIDEBAR_ID: &str = "selection-sidebar";

/// This component renders a sidebar for the selection where the user can filter the sources.
/// The order of the icons is determined by `settings.sidebar_order` and can be reordered
/// by dragging — see [`crate::components::reorder`]. Changes are persisted to settings
/// automatically.
///
/// Unlike the longer lists, an icon here may be dragged by a finger from
/// anywhere on it: five icons never need scrolling, so claiming a touch on them
/// takes nothing away. It used to be done with mouse events, which a touch
/// screen does not send — and it swapped the two icons rather than moving one,
/// which is not what the line under the pointer promised.
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

    let len = order.read().len();
    let drag = use_reorder_drag(Layout::Column, SIDEBAR_ID, len, move |(from, to)| {
        if move_into_gap(&mut order.write(), from, to).is_some() {
            settings.write().sidebar_order = order.peek().clone();
            settings.peek().save();
        }
    });

    rsx! {
        div {
            id: SIDEBAR_ID,
            class: "selection-sidebar {drag.container_class()}",
            onpointermove: move |event: Event<PointerData>| drag.moved(&event.data()),
            onpointerup: move |_| drag.released(),
            onpointercancel: move |_| drag.cancelled(),
            onpointerenter: move |event: Event<PointerData>| drag.entered(&event.data()),
            ondragstart: refuse_native_drag,

            for (idx, filter_type) in order.read().clone().iter().enumerate() {
                {
                    let ft = *filter_type;
                    rsx! {
                        div {
                            key: "{idx}",
                            role: "button",
                            class: if active_selection() == ft {
                                "outline selection-sidebar-item {drag.item_class(idx)}"
                            } else {
                                "outline secondary selection-sidebar-item {drag.item_class(idx)}"
                            },
                            style: "{GRIP_STYLE} {drag.item_style(idx)}",
                            tabindex: 0,
                            onmounted: move |event: Event<MountedData>| drag.mounted(idx, event),
                            onpointerdown: move |event: Event<PointerData>| drag.press(idx, &event.data(), true),
                            onkeydown: move |event: Event<KeyboardData>| {
                                drag.keydown(idx, &event, false);
                            },
                            onclick: move |_| {
                                if !drag.swallows_click() {
                                    active_selection.set(ft);
                                }
                            },
                            match ft {
                                SelectionSidebarType::Songs => rsx! { MusicIcon {} },
                                SelectionSidebarType::Pictures => rsx! { ImageIcon {} },
                                SelectionSidebarType::Pdfs => rsx! { PdfIcon {} },
                                SelectionSidebarType::Markdown => rsx! { MarkdownIcon {} },
                                SelectionSidebarType::Videos => rsx! { VideoIcon {} },
                            }
                        }
                    }
                }
            }
        }
    }
}
