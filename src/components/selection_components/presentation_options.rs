use crate::components::shared_components::SelectedItemPreview;
use crate::logic::settings::{use_settings, AfterLastSlide, SlideTimerSettings, SlideTransition};
use crate::logic::states::SelectedItemRepresentation;
use crate::TEST_STATE;
use dioxus::prelude::*;
use rust_i18n::t;

#[derive(Debug, Clone, PartialEq, Eq)]
enum PresentationOptionTabState {
    General,
    Specific,
}

#[component]
pub(crate) fn PresentationOptions(
    selected_items: Signal<Vec<SelectedItemRepresentation>>,
    active_selected_item_id: Signal<Option<usize>>,
) -> Element {
    let mut tab_state: Signal<PresentationOptionTabState> =
        use_signal(|| PresentationOptionTabState::General);
    let settings = use_settings();

    use_effect(move || {
        if active_selected_item_id.read().is_some() {
            tab_state.set(PresentationOptionTabState::Specific);
        }
    });

    let active_id = active_selected_item_id.read();
    let Some(item_index) = *active_id else {
        return rsx! {};
    };
    drop(active_id);

    rsx! {
        div {
            role: "group",
            button {
                class: "smaller-buttons",
                class: if *tab_state.read() != PresentationOptionTabState::General {
                    "secondary"
                },
                onclick: move |_| { tab_state.set(PresentationOptionTabState::General) },
                { t!("selection.presentation_options.tab.general").to_string() }
            }
            button {
                class: "smaller-buttons",
                class: if *tab_state.read() != PresentationOptionTabState::Specific {
                    "secondary"
                },
                onclick: move |_| { tab_state.set(PresentationOptionTabState::Specific) },
                { t!("selection.presentation_options.tab.specific").to_string() }
            }
        }

        match *tab_state.read() {
            PresentationOptionTabState::General => {
                rsx! {
                    p { { TEST_STATE.read().clone() } }
                }
            }
            PresentationOptionTabState::Specific => {
                let items = selected_items.read();
                let Some(item) = items.get(item_index).cloned() else {
                    return rsx! {};
                };

                let timer_enabled = item.timer_settings_option.is_some();
                let default_timer_settings = SlideTimerSettings::default();
                let timer_seconds = item
                    .timer_settings_option
                    .as_ref()
                    .map(|t| t.timer_seconds)
                    .unwrap_or(default_timer_settings.timer_seconds);
                let after_last = item
                    .timer_settings_option
                    .as_ref()
                    .map(|t| t.after_last_slide)
                    .unwrap_or_default();
                let current_transition = item.transition_effect;

                rsx! {
                    div {
                        class: "grid",
                        div {
                            label { { t!("selection.presentation_options.design").to_string() } }
                            select {
                                onchange: move |evt| {
                                    let val = evt.value();
                                    let mut items = selected_items.write();
                                    if val == "default" {
                                        items[item_index].presentation_design_option = None;
                                    } else if let Ok(idx) = val.parse::<usize>() {
                                        items[item_index].presentation_design_option = Some(settings.read().presentation_designs[idx].clone());
                                    }
                                },
                                option {
                                    value: "default",
                                    selected: item.presentation_design_option.is_none(),
                                    { t!("selection.presentation_options.default").to_string() }
                                }
                                for (idx, pd) in settings.read().presentation_designs.iter().enumerate() {
                                    option {
                                        value: "{idx}",
                                        selected: item.presentation_design_option.as_ref().map_or(false, |p| p.name == pd.name),
                                        "{pd.name}"
                                    }
                                }
                            }
                        }
                        div {
                            label { { t!("selection.presentation_options.slide_settings").to_string() } }
                            select {
                                onchange: move |evt| {
                                    let val = evt.value();
                                    let mut items = selected_items.write();
                                    if val == "default" {
                                        items[item_index].slide_settings_option = None;
                                    } else if let Ok(idx) = val.parse::<usize>() {
                                        items[item_index].slide_settings_option = Some(settings.read().song_slide_settings[idx].clone());
                                    }
                                },
                                option {
                                    value: "default",
                                    selected: item.slide_settings_option.is_none(),
                                    { t!("selection.presentation_options.default").to_string() }
                                }
                                for (idx, _) in settings.read().song_slide_settings.iter().enumerate() {
                                    option {
                                        value: "{idx}",
                                        selected: item.slide_settings_option.as_ref().map_or(false, |s| s == &settings.read().song_slide_settings[idx]),
                                        { format!("{} {}", t!("selection.presentation_options.slide_settings").to_string(), idx + 1) }
                                    }
                                }
                            }
                        }
                    }
                    div {
                        class: "grid",
                        div {
                            label { { t!("selection.presentation_options.transition.label").to_string() } }
                            select {
                                onchange: move |evt| {
                                    let val = evt.value();
                                    let transition = match val.as_str() {
                                        "none" => SlideTransition::None,
                                        "fade" => SlideTransition::Fade,
                                        "slide_from_right" => SlideTransition::SlideFromRight,
                                        "slide_from_left" => SlideTransition::SlideFromLeft,
                                        "zoom_in" => SlideTransition::ZoomIn,
                                        _ => SlideTransition::Fade,
                                    };
                                    selected_items.write()[item_index].transition_effect = transition;
                                },
                                option {
                                    value: "none",
                                    selected: current_transition == SlideTransition::None,
                                    { t!("selection.presentation_options.transition.none").to_string() }
                                }
                                option {
                                    value: "fade",
                                    selected: current_transition == SlideTransition::Fade,
                                    { t!("selection.presentation_options.transition.fade").to_string() }
                                }
                                option {
                                    value: "slide_from_right",
                                    selected: current_transition == SlideTransition::SlideFromRight,
                                    { t!("selection.presentation_options.transition.slide_from_right").to_string() }
                                }
                                option {
                                    value: "slide_from_left",
                                    selected: current_transition == SlideTransition::SlideFromLeft,
                                    { t!("selection.presentation_options.transition.slide_from_left").to_string() }
                                }
                                option {
                                    value: "zoom_in",
                                    selected: current_transition == SlideTransition::ZoomIn,
                                    { t!("selection.presentation_options.transition.zoom_in").to_string() }
                                }
                            }
                        }
                        div {
                            label { { t!("selection.presentation_options.timer.label").to_string() } }
                            div {
                                role: "group",
                                input {
                                    r#type: "checkbox",
                                    role: "switch",
                                    id: "timer-enabled-{item_index}",
                                    checked: timer_enabled,
                                    onchange: move |evt| {
                                        let checked = evt.checked();
                                        let mut items = selected_items.write();
                                        if checked {
                                            items[item_index].timer_settings_option = Some(SlideTimerSettings::default());
                                        } else {
                                            items[item_index].timer_settings_option = None;
                                        }
                                    }
                                }
                                label {
                                    r#for: "timer-enabled-{item_index}",
                                    style: "margin-left: 4px;",
                                    { t!("selection.presentation_options.timer.label").to_string() }
                                }
                            }
                            if timer_enabled {
                                input {
                                    r#type: "number",
                                    min: "1",
                                    max: "3600",
                                    value: "{timer_seconds}",
                                    style: "margin-top: 8px;",
                                    onchange: move |evt| {
                                        if let Ok(secs) = evt.value().parse::<u32>() {
                                            if secs > 0 {
                                                let mut items = selected_items.write();
                                                if let Some(ref mut ts) = items[item_index].timer_settings_option {
                                                    ts.timer_seconds = secs;
                                                }
                                            }
                                        }
                                    }
                                }
                                div {
                                    style: "margin-top: 8px;",
                                    label { { t!("selection.presentation_options.timer.after_last_slide.label").to_string() } }
                                    select {
                                        onchange: move |evt| {
                                            let val = evt.value();
                                            let behavior = match val.as_str() {
                                                "restart" => AfterLastSlide::RestartCurrentChapter,
                                                _ => AfterLastSlide::GoToNextChapter,
                                            };
                                            let mut items = selected_items.write();
                                            if let Some(ref mut ts) = items[item_index].timer_settings_option {
                                                ts.after_last_slide = behavior;
                                            }
                                        },
                                        option {
                                            value: "next",
                                            selected: after_last == AfterLastSlide::GoToNextChapter,
                                            { t!("selection.presentation_options.timer.after_last_slide.go_to_next").to_string() }
                                        }
                                        option {
                                            value: "restart",
                                            selected: after_last == AfterLastSlide::RestartCurrentChapter,
                                            { t!("selection.presentation_options.timer.after_last_slide.restart_chapter").to_string() }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div {
                        style: "margin-top: 20px; display: flex; flex-direction: column; align-items: center;",
                        SelectedItemPreview {
                            selected_item: item.clone(),
                            default_presentation_design: settings.read().presentation_designs[0].clone(),
                            default_slide_settings: settings.read().song_slide_settings[0].clone(),
                            width: 400,
                        }
                    }
                }
            }
        }
    }
}
