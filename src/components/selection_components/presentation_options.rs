use crate::components::shared_components::SelectedItemPreview;
use crate::logic::settings::{use_settings, AfterLastSlide, SlideTimerSettings, SlideTransition};
use crate::logic::states::SelectedItemRepresentation;
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

    // Only the *specific* half needs an element; the general half is about the
    // presentation as a whole and has to be reachable before anything is
    // picked — the streaming switch lives there.
    let selected_index = active_selected_item_id
        .read()
        .filter(|index| *index < selected_items.read().len());

    rsx! {
        div { role: "group",
            button {
                class: "smaller-buttons",
                class: if *tab_state.read() != PresentationOptionTabState::General { "secondary" },
                onclick: move |_| { tab_state.set(PresentationOptionTabState::General) },
                {t!("selection.presentation_options.tab.general").to_string()}
            }
            button {
                class: "smaller-buttons",
                class: if *tab_state.read() != PresentationOptionTabState::Specific { "secondary" },
                onclick: move |_| { tab_state.set(PresentationOptionTabState::Specific) },
                {t!("selection.presentation_options.tab.specific").to_string()}
            }
        }

        match *tab_state.read() {
            PresentationOptionTabState::General => {
                rsx! { StreamSwitch {} }
            }
            PresentationOptionTabState::Specific => {
                let items = selected_items.read();
                let Some(item) = selected_index.and_then(|index| items.get(index).cloned()) else {
                    return rsx! {};
                };
                let item_index = selected_index.unwrap_or(0);

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
                    div { class: "grid",
                        div {
                            label { {t!("selection.presentation_options.design").to_string()} }
                            select {
                                onchange: move |evt| {
                                    let val = evt.value();
                                    let mut items = selected_items.write();
                                    if val == "default" {
                                        items[item_index].presentation_design_option = None;
                                    } else if let Ok(idx) = val.parse::<usize>() {
                                        items[item_index].presentation_design_option = Some(
                                            settings.read().presentation_designs[idx].clone(),
                                        );
                                    }
                                },
                                option {
                                    value: "default",
                                    selected: item.presentation_design_option.is_none(),
                                    {t!("selection.presentation_options.default").to_string()}
                                }
                                for (idx , pd) in settings.read().presentation_designs.iter().enumerate() {
                                    option {
                                        value: "{idx}",
                                        selected: item
                                                                                    .presentation_design_option
                                                                                    .as_ref()
                                                                                    .is_some_and(|p| p.name == pd.name),
                                        "{pd.name}"
                                    }
                                }
                            }
                        }
                        div {
                            label { {t!("selection.presentation_options.slide_settings").to_string()} }
                            select {
                                onchange: move |evt| {
                                    let val = evt.value();
                                    let mut items = selected_items.write();
                                    if val == "default" {
                                        items[item_index].slide_settings_option = None;
                                    } else if let Ok(idx) = val.parse::<usize>() {
                                        items[item_index].slide_settings_option = Some(
                                            settings.read().song_slide_settings[idx].clone(),
                                        );
                                    }
                                },
                                option {
                                    value: "default",
                                    selected: item.slide_settings_option.is_none(),
                                    {t!("selection.presentation_options.default").to_string()}
                                }
                                for (idx , _) in settings.read().song_slide_settings.iter().enumerate() {
                                    option {
                                        value: "{idx}",
                                        selected: item.slide_settings_option
                                            .as_ref()
                                            .is_some_and(|s| { s == &settings.read().song_slide_settings[idx] }),
                                        {
                                            format!(
                                                "{} {}",
                                                t!("selection.presentation_options.slide_settings"),
                                                idx + 1,
                                            )
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "grid",
                        div {
                            label { {t!("selection.presentation_options.transition.label").to_string()} }
                            select {
                                onchange: move |evt| {
                                    let val = evt.value();
                                    let transition = match val.as_str() {
                                        "none" => SlideTransition::None,
                                        "fade" => SlideTransition::Fade,
                                        "slide_from_right" => SlideTransition::SlideFromRight,
                                        "slide_from_left" => SlideTransition::SlideFromLeft,
                                        "zoom_in" => SlideTransition::ZoomIn,
                                        "morph" => SlideTransition::Morph,
                                        _ => SlideTransition::Fade,
                                    };
                                    selected_items.write()[item_index].transition_effect = transition;
                                },
                                option {
                                    value: "none",
                                    selected: current_transition == SlideTransition::None,
                                    {t!("selection.presentation_options.transition.none").to_string()}
                                }
                                option {
                                    value: "fade",
                                    selected: current_transition == SlideTransition::Fade,
                                    {t!("selection.presentation_options.transition.fade").to_string()}
                                }
                                option {
                                    value: "slide_from_right",
                                    selected: current_transition == SlideTransition::SlideFromRight,
                                    {t!("selection.presentation_options.transition.slide_from_right").to_string()}
                                }
                                option {
                                    value: "slide_from_left",
                                    selected: current_transition == SlideTransition::SlideFromLeft,
                                    {t!("selection.presentation_options.transition.slide_from_left").to_string()}
                                }
                                option {
                                    value: "zoom_in",
                                    selected: current_transition == SlideTransition::ZoomIn,
                                    {t!("selection.presentation_options.transition.zoom_in").to_string()}
                                }
                                option {
                                    value: "morph",
                                    selected: current_transition == SlideTransition::Morph,
                                    {t!("selection.presentation_options.transition.morph").to_string()}
                                }
                            }
                        }
                        div {
                            label { {t!("selection.presentation_options.timer.label").to_string()} }
                            div { role: "group",
                                input {
                                    r#type: "checkbox",
                                    role: "switch",
                                    id: "timer-enabled-{item_index}",
                                    checked: timer_enabled,
                                    onchange: move |evt| {
                                        let checked = evt.checked();
                                        let mut items = selected_items.write();
                                        if checked {
                                            items[item_index].timer_settings_option = Some(
                                                SlideTimerSettings::default(),
                                            );
                                        } else {
                                            items[item_index].timer_settings_option = None;
                                        }
                                    },
                                }
                                label {
                                    r#for: "timer-enabled-{item_index}",
                                    style: "margin-left: 4px;",
                                    {t!("selection.presentation_options.timer.label").to_string()}
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
                                        if let Ok(secs) = evt.value().parse::<u32>()
                                            && secs > 0 {
                                                let mut items = selected_items.write();
                                                if let Some(ref mut ts) = items[item_index].timer_settings_option {
                                                    ts.timer_seconds = secs;
                                                }
                                            }
                                    },
                                }
                                div { style: "margin-top: 8px;",
                                    label { {t!("selection.presentation_options.timer.after_last_slide.label").to_string()} }
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
                                            {
                                                t!("selection.presentation_options.timer.after_last_slide.go_to_next")
                                                    .to_string()
                                            }
                                        }
                                        option {
                                            value: "restart",
                                            selected: after_last == AfterLastSlide::RestartCurrentChapter,
                                            {
                                                t!("selection.presentation_options.timer.after_last_slide.restart_chapter")
                                                    .to_string()
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { style: "margin-top: 20px; display: flex; flex-direction: column; align-items: center;",
                        SelectedItemPreview {
                            selected_item: item.clone(),
                            default_presentation_design: settings
                                                                                        .read()
                                                                                        .presentation_designs
                                                                                        .first()
                                                                                        .cloned()
                                                                                        .unwrap_or_default(),
                            default_slide_settings: settings
                                                                                        .read()
                                                                                        .song_slide_settings
                                                                                        .first()
                                                                                        .cloned()
                                                                                        .unwrap_or_default(),
                            width: 400,
                        }
                    }
                }
            }
        }
    }
}

/// Turns network streaming on for the presentation at hand, and says where to
/// find it.
///
/// The switch is here rather than in the settings on purpose: how streaming
/// works — the port, the password — is a setting and worth keeping, but
/// *whether* a given service is broadcast to the network is a decision for
/// that service. A program that streamed once should not quietly start doing
/// it again the next time it opens.
#[cfg(not(target_arch = "wasm32"))]
#[component]
fn StreamSwitch() -> Element {
    let settings = use_settings();
    // Read from the server rather than remembered here: this panel is built
    // and thrown away as the user moves about, and the server is not.
    let mut enabled = use_signal(crate::logic::stream::is_enabled);
    let mut address = use_signal(crate::logic::stream::address);
    let mut failure: Signal<Option<String>> = use_signal(|| None);
    // Bumped so that the publisher in `App` comes round and sends the
    // presentation as it stands — switching this on is not a change to the
    // presentation, and a viewer should not have to wait for the next slide.
    let mut stream_generation: Signal<u64> = use_context();

    rsx! {
        hgroup {
            h6 { { t!("selection.stream_headline").to_string() } }
            p { { t!("selection.stream_description").to_string() } }
        }
        label {
            class: "switch",
            input {
                r#type: "checkbox",
                role: "switch",
                checked: enabled(),
                onchange: move |event| {
                    let wanted: bool = event.value().parse().unwrap_or(false);
                    failure.set(None);
                    if wanted {
                        let stream = settings.read().stream.clone();
                        match crate::logic::stream::enable(stream.port, stream.password) {
                            Ok(reachable_at) => {
                                enabled.set(true);
                                address.set(Some(reachable_at));
                                stream_generation += 1;
                            }
                            // A port that is already in use is the likeliest
                            // outcome and is worth saying out loud, next to the
                            // switch that did not go on.
                            Err(reason) => {
                                enabled.set(false);
                                address.set(None);
                                failure.set(Some(
                                    t!("selection.stream_failed", reason = reason).to_string(),
                                ));
                            }
                        }
                    } else {
                        crate::logic::stream::disable();
                        enabled.set(false);
                        address.set(None);
                        stream_generation += 1;
                    }
                },
            }
            span { class: "slider" }
            { t!("selection.stream_enable").to_string() }
        }

        if let Some(reachable_at) = address() {
            p {
                style: "margin-top: 0.5rem;",
                { t!("selection.stream_address_hint").to_string() }
                br {}
                code {
                    style: "user-select: all; font-size: 1.1em;",
                    { reachable_at }
                }
            }
        }

        if let Some(reason) = failure() {
            p { style: "color: var(--pico-del-color, #b3261e);", { reason } }
        }
    }
}

/// There is no server inside a browser, so the web build has no switch.
#[cfg(target_arch = "wasm32")]
#[component]
fn StreamSwitch() -> Element {
    rsx! {}
}
