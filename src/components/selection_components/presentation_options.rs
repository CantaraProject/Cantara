use crate::components::shared_components::SelectedItemPreview;
use crate::logic::settings::{
    AfterLastSlide, PresentationDesign, Settings, SlideTimerSettings, SlideTransition, use_settings,
};
use crate::logic::sourcefiles::SourceFileType;
use crate::logic::states::SelectedItemRepresentation;
use crate::logic::stream_view::reconcile_max_lines;
use cantara_songlib::slides::SlideSettings;
use dioxus::prelude::*;
use rust_i18n::t;

#[derive(Debug, Clone, PartialEq, Eq)]
enum PresentationOptionTabState {
    General,
    Specific,
}

/// One choice out of the configured presentation designs.
///
/// Every panel that offers this choice offers the same list, differing only in
/// what an unmade choice means — the general setting, the projection's design,
/// or nothing at all where the choice cannot be left open. That difference is
/// the `fallback` label; the list itself is written once, here, so that the
/// names on offer cannot drift apart between the panels.
#[component]
fn DesignSelect(
    label: String,
    fallback: Option<String>,
    selected: Option<usize>,
    onselect: EventHandler<Option<usize>>,
) -> Element {
    let settings = use_settings();

    rsx! {
        div {
            label { {label} }
            select {
                onchange: move |event| onselect.call(event.value().parse::<usize>().ok()),
                if let Some(fallback) = fallback {
                    option { value: "", selected: selected.is_none(), {fallback} }
                }
                for (index , design) in settings.read().presentation_designs.iter().enumerate() {
                    option {
                        value: "{index}",
                        selected: selected == Some(index),
                        "{design.name}"
                    }
                }
            }
        }
    }
}

/// One choice out of the configured slide settings — the counterpart to
/// [`DesignSelect`], and written once for the same reason: an entry is named by
/// the user, and a panel that made up its own label for it would show a
/// different name for the very same thing.
#[component]
fn SlideSettingsSelect(
    label: String,
    fallback: Option<String>,
    selected: Option<usize>,
    onselect: EventHandler<Option<usize>>,
) -> Element {
    let settings = use_settings();

    rsx! {
        div {
            label { {label} }
            select {
                onchange: move |event| onselect.call(event.value().parse::<usize>().ok()),
                if let Some(fallback) = fallback {
                    option { value: "", selected: selected.is_none(), {fallback} }
                }
                for (index , named) in settings.read().song_slide_settings.iter().enumerate() {
                    option {
                        value: "{index}",
                        selected: selected == Some(index),
                        {named.display_name(index)}
                    }
                }
            }
        }
    }
}

/// Where a design an element carries sits in the configured list, if it is
/// still one of them. Matched by name, as the rest of the program does.
fn design_position(settings: &Settings, chosen: &Option<PresentationDesign>) -> Option<usize> {
    let chosen = chosen.as_ref()?;
    settings
        .presentation_designs
        .iter()
        .position(|design| design.name == chosen.name)
}

/// The same for slide settings, which carry no name of their own once chosen
/// and so are matched on what they say.
fn slide_settings_position(settings: &Settings, chosen: &Option<SlideSettings>) -> Option<usize> {
    let chosen = chosen.as_ref()?;
    settings
        .song_slide_settings
        .iter()
        .position(|named| named.settings == *chosen)
}

#[component]
pub(crate) fn PresentationOptions(
    selected_items: Signal<Vec<SelectedItemRepresentation>>,
    active_selected_item_id: Signal<Option<usize>>,
) -> Element {
    let mut tab_state: Signal<PresentationOptionTabState> =
        use_signal(|| PresentationOptionTabState::General);

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
                rsx! {
                    DefaultDesignSettings {}
                    StreamSwitch {}
                    StreamViewSettings {}
                }
            }
            PresentationOptionTabState::Specific => {
                rsx! {
                    SpecificOptions { selected_items, selected_index }
                }
            }
        }
    }
}

/// The half of the options that belongs to one selected element.
///
/// A component of its own, because everything here needs an element to work
/// on: without one it says so and stops, and that has to leave the two tabs
/// above it standing.
#[component]
fn SpecificOptions(
    selected_items: Signal<Vec<SelectedItemRepresentation>>,
    /// Which element is open, if any.
    selected_index: Option<usize>,
) -> Element {
    let settings = use_settings();

    let items = selected_items.read();
    let Some(item) = selected_index.and_then(|index| items.get(index).cloned()) else {
        // Everything on this half belongs to one element, so
        // without one there is nothing to set — and saying so is
        // better than the empty panel that used to be here.
        return rsx! {
            p { class: "presentation-options-hint",
                {t!("selection.presentation_options.select_item_first").to_string()}
            }
        };
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

    // Only a PDF has pages to choose between. What the field says about a
    // pattern it cannot read is worked out here rather than on every keystroke
    // in the handler, so that the message and the value can never disagree.
    let is_pdf = item.source_file.file_type == SourceFileType::Pdf;

    // …and only a video is played rather than merely shown.
    let is_video = item.source_file.file_type == SourceFileType::Video;
    let video_settings = item.video_settings;
    let pdf_pages = item.pdf_pages.clone();
    let pdf_path = item.source_file.path.clone();

    // How long the document is, so that a page it does not have can be said
    // rather than silently dropped. Counting means opening and parsing the
    // file, which must not happen while this is being drawn — so it is asked
    // for here and read when it arrives. Until then only the pattern itself is
    // checked, which is the honest thing: nothing yet knows the length.
    let mut pages_ready: Signal<u64> = use_signal(crate::logic::pdf::page_count_generation);
    use_effect({
        let pdf_path = pdf_path.clone();
        move || {
            if !is_pdf {
                return;
            }
            crate::logic::pdf::prepare_page_count(pdf_path.clone());

            spawn(async move {
                loop {
                    let generation = crate::logic::pdf::page_count_generation();
                    if generation != *pages_ready.peek() {
                        pages_ready.set(generation);
                    }
                    if !crate::logic::pdf::page_counts_in_progress() {
                        return;
                    }
                    let _ = document::eval("await new Promise(r => setTimeout(r, 100))").await;
                }
            });
        }
    });

    // Read while rendering, because that is what subscribes this to it.
    let _ = pages_ready();
    let page_total = crate::logic::pdf::page_count(&pdf_path);

    let pdf_pages_problem = crate::logic::pdf_pages::PageSelection::parse(&pdf_pages)
        .and_then(|selection| match page_total {
            Some(total) => selection.check_against(total),
            // Not counted yet. Saying nothing beats guessing.
            None => Ok(()),
        })
        .err()
        .map(|error| {
            let (key, parameters) = error.message_key();
            crate::components::shared_components::translate(key, &parameters)
        });

    // The element keeps what it was given, not where it came from; the lists
    // are addressed by position, so a choice is looked back up here. An entry
    // that has since been edited away no longer matches, and the element then
    // reads as following the general setting — which is what it now does.
    let design_choice = design_position(&settings.read(), &item.presentation_design_option);
    let stream_design_choice = design_position(&settings.read(), &item.stream_design_option);
    let slide_settings_choice =
        slide_settings_position(&settings.read(), &item.slide_settings_option);
    let stream_slide_settings_choice =
        slide_settings_position(&settings.read(), &item.stream_slide_settings_option);

    rsx! {
        div { class: "grid",
            DesignSelect {
                label: t!("selection.presentation_options.design").to_string(),
                fallback: t!("selection.presentation_options.default").to_string(),
                selected: design_choice,
                onselect: move |chosen: Option<usize>| {
                    let picked = chosen.map(|index| settings.read().presentation_designs[index].clone());
                    selected_items.write()[item_index].presentation_design_option = picked;
                },
            }
            SlideSettingsSelect {
                label: t!("selection.presentation_options.slide_settings").to_string(),
                fallback: t!("selection.presentation_options.default").to_string(),
                selected: slide_settings_choice,
                onselect: move |chosen: Option<usize>| {
                    let picked = chosen
                        .map(|index| settings.read().song_slide_settings[index].settings.clone());
                    selected_items.write()[item_index].slide_settings_option = picked;
                },
            }
        }

        div { style: "margin-top: 20px; display: flex; flex-direction: column; align-items: center;",
            SelectedItemPreview {
                selected_item: item.clone(),
                // "Default" here means what the general half was set
                // to, so the preview shows what this element will
                // actually look like.
                default_presentation_design: settings.read().default_presentation_design(),
                default_slide_settings: settings.read().default_song_slide_settings(),
                width: 400,
            }
        }

        if is_pdf {
            div {
                label { {t!("selection.pdf_pages_label").to_string()} }
                input {
                    r#type: "text",
                    // Keys typed in here belong in here. The library view sends
                    // any key it sees to the search field so that looking a
                    // song up needs no click first, and it decides that by what
                    // reaches it — so a field that is being typed into keeps
                    // its own keys. See the `onkeydown` on `main` in
                    // `selection_components.rs`.
                    onkeydown: move |event: Event<KeyboardData>| event.stop_propagation(),
                    // `aria-invalid` is what Pico marks a bad field with, so a
                    // wrong pattern looks the way every other wrong field does.
                    aria_invalid: pdf_pages_problem.is_some().to_string(),
                    value: "{pdf_pages}",
                    placeholder: t!("selection.pdf_pages_placeholder").to_string(),
                    oninput: move |event| {
                        // Kept as typed, wrong or not: a pattern is wrong for
                        // as long as it takes to finish writing it, and a field
                        // that refuses the half of it cannot be typed into.
                        selected_items.write()[item_index].pdf_pages = event.value();
                    },
                }
                match pdf_pages_problem.clone() {
                    Some(problem) => rsx! {
                        small { class: "pdf-pages-problem", "{problem}" }
                    },
                    None => rsx! {
                        small { class: "pdf-pages-hint",
                            {t!("selection.pdf_pages_hint").to_string()}
                        }
                    },
                }
            }
        }

        if is_video {
            fieldset {
                legend { {t!("selection.video.headline").to_string()} }

                label {
                    input {
                        r#type: "checkbox",
                        role: "switch",
                        checked: video_settings.autostart,
                        onchange: move |event: Event<FormData>| {
                            selected_items.write()[item_index].video_settings.autostart = event
                                .checked();
                        },
                    }
                    {t!("selection.video.autostart").to_string()}
                }
                small { class: "video-option-hint",
                    {t!("selection.video.autostart_hint").to_string()}
                }

                label {
                    input {
                        r#type: "checkbox",
                        role: "switch",
                        checked: video_settings.looping,
                        onchange: move |event: Event<FormData>| {
                            selected_items.write()[item_index].video_settings.looping = event
                                .checked();
                        },
                    }
                    {t!("selection.video.loop").to_string()}
                }
                small { class: "video-option-hint",
                    {t!("selection.video.loop_hint").to_string()}
                }
            }
        }

        // What the phones are shown for *this* element, overriding
        // whatever the service chose generally. "Same as the
        // presentation" here means exactly that and not "fall back
        // to the general choice": an element singled out to look
        // like the projection has to be able to say so even when
        // the service as a whole does not.
        hgroup { style: "margin-top: 1rem;",
            h6 { {t!("selection.presentation_options.stream_view.headline").to_string()} }
        }
        div { class: "grid",
            DesignSelect {
                label: t!("selection.presentation_options.stream_view.design").to_string(),
                fallback: t!("selection.presentation_options.default").to_string(),
                selected: stream_design_choice,
                onselect: move |chosen: Option<usize>| {
                    let picked = chosen.map(|index| settings.read().presentation_designs[index].clone());
                    selected_items.write()[item_index].stream_design_option = picked;
                },
            }
            SlideSettingsSelect {
                label: t!("selection.presentation_options.stream_view.slide_settings").to_string(),
                fallback: t!("selection.presentation_options.default").to_string(),
                selected: stream_slide_settings_choice,
                onselect: move |chosen: Option<usize>| {
                    let picked = chosen
                        .map(|index| settings.read().song_slide_settings[index].settings.clone());
                    selected_items.write()[item_index].stream_slide_settings_option = picked;
                },
            }
        }

        {
            // Reconciled against *this* element's own wrap, which
            // is the reference the mapping is built on.
            let projection_wrap = item
                .slide_settings_option
                .clone()
                .or_else(|| Some(settings.read().default_song_slide_settings()))
                .and_then(|slide_settings| slide_settings.max_lines);
            let chosen_wrap = item
                .stream_slide_settings_option
                .as_ref()
                .and_then(|slide_settings| slide_settings.max_lines);
            wrap_note(chosen_wrap, reconcile_max_lines(projection_wrap, chosen_wrap))
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
                        // As with the field above: what is typed into a field
                        // stays in it.
                        onkeydown: move |event: Event<KeyboardData>| event.stop_propagation(),
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
    }
}

/// What the phones are shown, for the service as a whole.
///
/// Beside the switch rather than in the settings, and for the same reason: how
/// streaming works is a setting, but what a given service sends is a decision
/// for that service. This one is kept, though — it is a choice between designs
/// the user has already built, and rebuilding it every Sunday would be a chore
/// rather than a safeguard.
#[component]
fn StreamViewSettings() -> Element {
    let mut settings = use_settings();

    // The projection's own general choice, which is what "the same" means here
    // and what the line wrap is reconciled against.
    let projection_wrap = settings.read().default_song_slide_settings().max_lines;

    let design_index = settings.read().stream.design_index;
    let slide_settings_index = settings.read().stream.slide_settings_index;

    // What the chosen wrap will actually come to. Said out loud rather than
    // silently applied: a user who asks for five lines and gets six should be
    // told why, next to the control that did it.
    let chosen_wrap = slide_settings_index
        .and_then(|index| {
            settings
                .read()
                .song_slide_settings
                .get(index)
                .map(|named| named.settings.clone())
        })
        .and_then(|slide_settings| slide_settings.max_lines);
    let used_wrap = reconcile_max_lines(projection_wrap, chosen_wrap);

    rsx! {
        hgroup { style: "margin-top: 1.5rem;",
            h6 { {t!("selection.presentation_options.stream_view.headline").to_string()} }
        }
        div { class: "grid",
            DesignSelect {
                label: t!("selection.presentation_options.stream_view.design").to_string(),
                fallback: t!("selection.presentation_options.stream_view.same_as_presentation")
                    .to_string(),
                selected: design_index,
                onselect: move |chosen: Option<usize>| {
                    settings.write().stream.design_index = chosen;
                    settings.read().save();
                },
            }
            SlideSettingsSelect {
                label: t!("selection.presentation_options.stream_view.slide_settings").to_string(),
                fallback: t!("selection.presentation_options.stream_view.same_as_presentation")
                    .to_string(),
                selected: slide_settings_index,
                onselect: move |chosen: Option<usize>| {
                    settings.write().stream.slide_settings_index = chosen;
                    settings.read().save();
                },
            }
        }

        {wrap_note(chosen_wrap, used_wrap)}
    }
}

/// Says what a chosen line wrap will actually come to, where that is not what
/// was asked for.
///
/// Silence where the two agree: a note that only ever says "yes, that one" is
/// noise, and the user stops reading it before the one time it matters.
fn wrap_note(chosen: Option<usize>, used: Option<usize>) -> Element {
    if chosen == used {
        return rsx! {};
    }

    let text = match (chosen, used) {
        (Some(chosen), Some(used)) => t!(
            "selection.presentation_options.stream_view.max_lines_note",
            chosen = chosen,
            used = used
        )
        .to_string(),
        // Asked for a wrap under a projection that has none.
        _ => t!("selection.presentation_options.stream_view.max_lines_dropped").to_string(),
    };

    rsx! {
        p { class: "stream-wrap-note", { text } }
    }
}

/// The design and the slide division everything is shown with unless the
/// element says otherwise.
///
/// The same two choices the specific half offers, one level up: what is picked
/// here is what "Standard" means down there. Kept in the settings rather than
/// with the service, because it is a choice between designs the user has
/// already built and is not worth making again every Sunday — the same
/// reasoning as for the stream's own defaults below.
///
/// Both are stored as a position in their list, so that editing the chosen
/// design reaches every presentation built from it. See
/// [`Settings::default_presentation_design`](crate::logic::settings::Settings::default_presentation_design).
#[component]
fn DefaultDesignSettings() -> Element {
    let mut settings = use_settings();

    let design_index = use_memo(move || settings.read().default_design_index);
    let slide_settings_index = use_memo(move || settings.read().default_slide_settings_index);

    rsx! {
        div { class: "grid",
            DesignSelect {
                label: t!("selection.presentation_options.design").to_string(),
                selected: Some(design_index()),
                onselect: move |chosen: Option<usize>| {
                    if let Some(index) = chosen {
                        settings.write().default_design_index = index;
                        settings.read().save();
                    }
                },
            }
            SlideSettingsSelect {
                label: t!("selection.presentation_options.slide_settings").to_string(),
                selected: Some(slide_settings_index()),
                onselect: move |chosen: Option<usize>| {
                    if let Some(index) = chosen {
                        settings.write().default_slide_settings_index = index;
                        settings.read().save();
                    }
                },
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
/// Starts the network helper off the thread the window is drawn on.
///
/// Switching either service on spawns a second copy of Cantara and waits for it
/// to connect back and say it is serving — a handshake with a fifteen-second
/// deadline behind it (see [`crate::logic::network_host`]). Run where it was
/// written, in the `onchange` handler, that wait is the window: the panel, the
/// running order and the presentation all stop until the helper answers, and a
/// helper that will never answer freezes the program for a quarter of a minute
/// while the operator is preparing a service.
///
/// So it is run on a thread of its own and what it says comes back over a
/// channel, which the switch waits for without holding anything up.
///
/// A plain thread rather than `spawn_blocking`, though everything Dioxus
/// spawns on the desktop is in tokio's runtime today: `spawn_blocking` panics
/// where there is no runtime, and a panic when the switch is clicked would be
/// a worse answer than the freeze this replaces. Waiting on the channel needs
/// no runtime at all — it is a future like any other, woken by the thread that
/// finishes. This happens when somebody clicks a switch, so the thread costs
/// nothing worth counting.
///
/// A thread that panics drops the sending end, which arrives here as the
/// channel closing; it is reported like any other reason the switch did not go
/// on, rather than as a switch that stays busy for ever.
#[cfg(all(not(target_arch = "wasm32"), feature = "desktop"))]
async fn starting_a_helper<F>(start: F) -> Result<String, String>
where
    F: FnOnce() -> Result<String, String> + Send + 'static,
{
    let (tell, told) = tokio::sync::oneshot::channel();

    if let Err(error) = std::thread::Builder::new()
        .name("cantara-network-switch".to_string())
        .spawn(move || {
            // Nobody left to tell is not a failure: the panel was closed while
            // the helper was starting, and the helper is running either way.
            let _ = tell.send(start());
        })
    {
        return Err(format!("{error}"));
    }

    told.await
        .unwrap_or_else(|_| Err("the network server was lost while starting".to_string()))
}

/// Turns network streaming on for the presentation at hand, and says where to
/// find it.
///
/// The switch is here rather than in the settings on purpose: how streaming
/// works — the port, the password — is a setting and worth keeping, but
/// *whether* a given service is broadcast to the network is a decision for
/// that service. A program that streamed once should not quietly start doing
/// it again the next time it opens.
///
/// Desktop only, like [`RemoteConsoleSwitch`] below and for the same reason:
/// the switch starts a helper process and asks it for an address, and
/// [`crate::logic::network_host`] — the half of that which lives in Cantara —
/// is built for the desktop alone. `not(wasm32)` is not the same question. It
/// is also true of the Android build, which is neither a browser nor a desktop
/// and has no `network_host` to call, and saying otherwise is what stopped
/// that build.
#[cfg(all(not(target_arch = "wasm32"), feature = "desktop"))]
#[component]
fn StreamSwitch() -> Element {
    let settings = use_settings();
    // Read from the server rather than remembered here: this panel is built
    // and thrown away as the user moves about, and the server is not.
    let mut enabled = use_signal(crate::logic::network_host::is_viewer_enabled);
    let mut address = use_signal(crate::logic::network_host::viewer_address);
    let mut failure: Signal<Option<String>> = use_signal(|| None);
    // `None` while nothing has been clicked, then whether it worked.
    let mut copied: Signal<Option<bool>> = use_signal(|| None);
    // Bumped so that the publisher in `App` comes round and sends the
    // presentation as it stands — switching this on is not a change to the
    // presentation, and a viewer should not have to wait for the next slide.
    let mut stream_generation: Signal<u64> = use_context();
    // While the helper is being started. See [`starting_a_helper`].
    let mut starting = use_signal(|| false);

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
                // The switch stands where it was put while the helper starts,
                // and comes back by itself if it could not be started. Both
                // edges of `starting` change this attribute, which is what
                // makes the second of those happen: `enabled` is false before
                // the attempt and false after a failed one, and an attribute
                // that does not change is not written back to a checkbox the
                // user has already clicked.
                checked: enabled() || starting(),
                // One attempt at a time. Two helpers for one port is the
                // failure this avoids, and a switch that cannot be clicked is
                // how the panel says it is busy.
                disabled: starting(),
                onchange: move |event| {
                    let wanted: bool = event.value().parse().unwrap_or(false);
                    failure.set(None);
                    if !wanted {
                        crate::logic::network_host::disable_viewer();
                        enabled.set(false);
                        address.set(None);
                        stream_generation += 1;
                        return;
                    }

                    let stream = settings.read().stream.clone();
                    starting.set(true);
                    spawn(async move {
                        let started = starting_a_helper(move || {
                            crate::logic::network_host::enable_viewer(stream.port, stream.password)
                        })
                            .await;
                        starting.set(false);
                        match started {
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
                    });
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
                    class: "stream-address",
                    title: t!("selection.stream_copy_hint").to_string(),
                    onclick: {
                        let address = reachable_at.clone();
                        move |_| {
                            let address = address.clone();
                            spawn(async move {
                                copied.set(Some(crate::logic::clipboard::copy(&address).await));
                                // Long enough to be read, short enough that the
                                // panel does not keep saying it forever.
                                //
                                // Through the WebView rather than `tokio::time`,
                                // as everywhere else here: a Rust-side sleep does
                                // not pump the WebView's event loop.
                                let _ = document::eval(
                                    "await new Promise(r => setTimeout(r, 3000))",
                                )
                                .await;
                                copied.set(None);
                            });
                        }
                    },
                    { reachable_at.clone() }
                }
                match copied() {
                    Some(true) => rsx! {
                        span {
                            class: "stream-copied",
                            { t!("selection.stream_copied").to_string() }
                        }
                    },
                    // Worth saying: a webview without clipboard access leaves
                    // the user clicking a thing that appears to do nothing.
                    Some(false) => rsx! {
                        span {
                            class: "stream-copy-failed",
                            { t!("selection.stream_copy_failed").to_string() }
                        }
                    },
                    None => rsx! {},
                }
            }
        }

        if let Some(reason) = failure() {
            p { style: "color: var(--pico-del-color, #b3261e);", { reason } }
        }

        RemoteConsoleSwitch {}
    }
}

/// Offers the presenter console to a browser on the network.
///
/// Beside the streaming switch because it is the same kind of decision about
/// the same service, and — like it — not remembered between services: a
/// program that was driven from a tablet once must not quietly offer that
/// again next Sunday. The password is a setting; the switch is a decision
/// about this service.
///
/// Independent of streaming. A service can be remote-controlled without being
/// streamed, and streamed without being remote-controlled; both switches feed
/// one server on one port.
#[cfg(all(not(target_arch = "wasm32"), feature = "desktop"))]
#[component]
fn RemoteConsoleSwitch() -> Element {
    let settings = use_settings();
    // Asked of the server, for the reason the streaming switch gives: this
    // panel comes and goes, the server does not.
    let mut enabled = use_signal(crate::logic::network_host::is_console_enabled);
    let mut address = use_signal(crate::logic::network_host::console_address);
    // What is running now, to hand over the moment the helper is up. Switching
    // this on is not a change to the presentation, so the publisher in `App`
    // will not come round by itself — the same trap the streaming switch
    // documents next door, and the reason a console switched on mid-service
    // used to say that nothing was running.
    let running_presentations: Signal<Vec<crate::logic::states::RunningPresentation>> =
        use_context();
    let mut failure: Signal<Option<String>> = use_signal(|| None);
    let mut copied: Signal<Option<bool>> = use_signal(|| None);

    // Whether anyone who reaches the address may drive the presentation. Said
    // plainly beside the switch rather than prevented: a locked room on a
    // network with nothing else on it is a real situation, and the person
    // running the service is the one who knows which network they are on.
    let remote_presenter_console_is_open_to_anyone =
        use_memo(move || settings.read().stream.remote_password.is_empty());

    // How many browsers have the console open, as a signal rather than as a
    // number read while rendering.
    //
    // The count lives in an atomic that the thread reading the helper writes
    // to (see [`crate::logic::network_host`]). Nothing about reading an atomic
    // tells Dioxus to render this panel again, so the line said whatever had
    // been true when the panel was last built — in practice "0 connected",
    // for as long as the panel stayed open.
    //
    // Polled rather than pushed because the writer is another thread, and a
    // signal belongs to the runtime that made it. A second is far below what
    // this is read at — somebody glancing to see whether the tablet at the
    // back has arrived — and the loop stops with the panel.
    let mut connected = use_signal(crate::logic::network_host::connected);
    // While the helper is being started. See [`starting_a_helper`].
    let mut starting = use_signal(|| false);
    use_future(move || async move {
        loop {
            crate::logic::timer::sleep(std::time::Duration::from_secs(1)).await;
            let now = crate::logic::network_host::connected();
            // Only on a change: writing a signal renders everything that
            // reads it, and this loop would otherwise do that once a second
            // for as long as the panel is open.
            if now != *connected.peek() {
                connected.set(now);
            }
        }
    });

    rsx! {
        hgroup { style: "margin-top: 1.5rem;",
            h6 { { t!("selection.remote_headline").to_string() } }
            p { { t!("selection.remote_description").to_string() } }
        }
        label {
            class: "switch",
            input {
                r#type: "checkbox",
                role: "switch",
                // Held where it was clicked while the helper starts, and put
                // back by itself if it could not be — see the streaming switch
                // above, where the same two lines are explained.
                checked: enabled() || starting(),
                disabled: starting(),
                onchange: move |event| {
                    // A checkbox that says something other than "true" or
                    // "false" is not a checkbox; off is the safe reading of
                    // anything else.
                    let wanted = event.value().parse::<bool>().unwrap_or(false);
                    failure.set(None);
                    if !wanted {
                        crate::logic::network_host::disable_console();
                        enabled.set(false);
                        address.set(None);
                        return;
                    }

                    let stream = settings.read().stream.clone();
                    starting.set(true);
                    spawn(async move {
                        // The same port the stream uses, because it is the
                        // same server: both are served by the helper process
                        // in [`crate::logic::network_server`]. Whichever
                        // switch goes on first starts it.
                        let started = starting_a_helper(move || {
                            crate::logic::network_host::enable_console(
                                stream.port,
                                stream.remote_password,
                            )
                        })
                            .await;
                        starting.set(false);
                        match started {
                            Ok(reachable_at) => {
                                enabled.set(true);
                                address.set(Some(reachable_at));
                                crate::logic::network_host::publish(
                                    running_presentations.read().first().cloned(),
                                );
                            }
                            Err(reason) => {
                                enabled.set(false);
                                address.set(None);
                                failure.set(Some(
                                    t!("selection.remote_failed", reason = reason).to_string(),
                                ));
                            }
                        }
                    });
                },
            }
            span { class: "slider" }
            { t!("selection.remote_enable").to_string() }
        }

        if remote_presenter_console_is_open_to_anyone() {
            p { style: "margin-top: 0.5rem;",
                { t!("selection.remote_no_password").to_string() }
            }
        }

        if let Some(reachable_at) = address() {
            p {
                style: "margin-top: 0.5rem;",
                { t!("selection.remote_address_hint").to_string() }
                br {}
                code {
                    class: "stream-address",
                    title: t!("selection.stream_copy_hint").to_string(),
                    onclick: {
                        let address = reachable_at.clone();
                        move |_| {
                            let address = address.clone();
                            spawn(async move {
                                copied.set(Some(crate::logic::clipboard::copy(&address).await));
                                let _ = document::eval(
                                    "await new Promise(r => setTimeout(r, 3000))",
                                )
                                .await;
                                copied.set(None);
                            });
                        }
                    },
                    { reachable_at.clone() }
                }
                match copied() {
                    Some(true) => rsx! {
                        span {
                            class: "stream-copied",
                            { t!("selection.stream_copied").to_string() }
                        }
                    },
                    Some(false) => rsx! {
                        span {
                            class: "stream-copy-failed",
                            { t!("selection.stream_copy_failed").to_string() }
                        }
                    },
                    None => rsx! {},
                }
            }
            // How many browsers have it open. Several are allowed — a phone
            // that locked and was opened again is a second connection for a
            // while — and the last thing anyone did is what the presentation
            // shows.
            p { style: "margin-top: 0.25rem; opacity: 0.8;",
                { t!("selection.remote_connected", count = connected()).to_string() }
            }
        }

        if let Some(reason) = failure() {
            p { style: "color: var(--pico-del-color, #b3261e);", { reason } }
        }
    }
}

/// A build without a desktop has no console to offer and no server to offer it
/// from.
#[cfg(all(not(target_arch = "wasm32"), not(feature = "desktop")))]
#[component]
fn RemoteConsoleSwitch() -> Element {
    rsx! {}
}


/// There is no server inside a browser, so the web build has no switch.
#[cfg(target_arch = "wasm32")]
#[component]
fn StreamSwitch() -> Element {
    rsx! {}
}

/// And no helper to start where there is no desktop: on Android the panel is
/// the same panel, without the two switches that would need one.
#[cfg(all(not(target_arch = "wasm32"), not(feature = "desktop")))]
#[component]
fn StreamSwitch() -> Element {
    rsx! {}
}
