use crate::logic::{settings::*, states::RuntimeInformation};

use dioxus::prelude::*;
use dioxus_router::prelude::navigator;
use rust_i18n::t;

use rfd::FileDialog;

use crate::{Route, LOGO};

rust_i18n::i18n!("locales", fallback = "en");

const MAX_STEPS: u8 = 3;

/// This is a struct representing the step status of the wizard.
#[derive(Debug, Clone, Copy)]
struct WizardStatus {
    is_done: Signal<bool>,
}

#[component]
pub fn Wizard() -> Element {
    let step: Signal<u8> = use_signal(|| 1);
    let is_done = use_signal(|| false);

    use_context_provider(|| WizardStatus { is_done });

    let runtime_info: RuntimeInformation = use_context();
    let locale: Signal<String> = use_signal(|| runtime_info.language.clone());

    rsx!(
        div {
            class: "wrapper",
            header {
                lang: locale,
                class: "top-bar",
                h1 { {t!("wizard.title")} }
            }
            main {
                lang: locale,
                class: "container-fluid content height-100",
                WizardPage { step }
            }
            footer {
                lang: locale,
                class: "bottom-bar",
                div {
                    class: "grid",
                    div {
                        progress {
                            value: step,
                            max: MAX_STEPS,
                        }
                    }
                }
                WizardButtons { step }
            }
        }

    )
}

#[component]
fn WizardButtons(step: Signal<u8>) -> Element {
    let wizard_status: WizardStatus = use_context();

    let mut increase_step = move || {
        step.set(step + 1);
    };

    let mut decrease_step = move || {
        if step() > 1 {
            step.set(step - 1);
        }
    };

    rsx! {
        div {
            role: "group",
            button {
                class: "secondary",
                disabled: step() <= 1,
                onclick: move |_| decrease_step(),
                { t!("wizard.back") }
            }
            button {
                class: "primary",
                disabled: !*wizard_status.is_done.read(),
                onclick: move |_| increase_step(),
                { t!("wizard.next") }
            }
        }
    }
}

/// The WizardPage component routes to a wizard page based on the current step.
#[component]
fn WizardPage(step: Signal<u8>) -> Element {
    let nav = navigator();

    match step() {
        1 => rsx! { FirstStep {} },
        2 => rsx! { SecondStep {} },
        3 => rsx! { ThirdStep {} },

        _ => {
            nav.replace(Route::Selection);
            rsx! {}
        }
    }
}

/// The FirstStep component represents the first step of the wizard.
///
/// As the first step consists only of a brief intruduction, it is immediately marked as done.
#[component]
fn FirstStep() -> Element {
    let mut wizard_status: WizardStatus = use_context::<WizardStatus>();
    use_effect(move || {
        wizard_status.is_done.set(true);
    });

    let explanation_html: String = t!("wizard.first_step").to_string();

    rsx! {
        div {
            class: "wizard-step",
            div {
                class: "grid fade-in",
                div {
                    dangerous_inner_html: explanation_html
                }
                div {
                    img {
                        src: LOGO,
                        class: "logo center",
                        alt: "Cantara Logo"
                    }
                }
            }
        }
    }
}

/// The SecondStep component represents the second step of the wizard.
///
/// The second step lets the user choose a song repository folder.
/// It will be marked as done once the user has chosen a valid folder.
#[component]
fn SecondStep() -> Element {
    let mut wizard_status: WizardStatus = use_context::<WizardStatus>();
    use_effect(move || {
        wizard_status.is_done.set(false);
    });
    let mut chosen_directory = use_signal(|| "".to_string());

    let mut choose_directory = move || {
        let path = FileDialog::new().pick_folder();

        let mut settings_signal: Signal<Settings> = use_context();

        if let Some(path) = path {
            if path.is_dir() && path.exists() {
                chosen_directory.set(path.to_str().unwrap_or_default().to_string());
                let mut settings = settings_signal.write();
                settings.add_repository_folder(chosen_directory.read().to_string());
                settings.save();
                let mut wizard_status = use_context::<WizardStatus>();
                wizard_status.is_done.set(true);
            }
        }
    };

    rsx! {
        div {
            class: "wizard-step",
            h3 { { t!("wizard.second_step.title") } }
            div {
                class: "grid fade-in",
                div {
                    dangerous_inner_html: t!("wizard.second_step.explanation").to_string()
                }
                div {
                    div {
                        role: "group",
                        button {
                            class: "primary",
                            onclick: move |_| {
                                choose_directory();
                            },
                            { t!("wizard.second_step.chose_directory") }
                        }
                    }
                    if chosen_directory.read().is_empty().ne(&true) {
                        {
                            rsx! {
                                p { { t!("wizard.second_step.dir_selected", dir=chosen_directory.read()) } }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ThirdStep() -> Element {
    let mut wizard_status: WizardStatus = use_context::<WizardStatus>();
    use_effect(move || {
        wizard_status.is_done.set(true);
    });

    let mut settings_signal: Signal<Settings> = use_context();
    use_effect(move || {
        let mut settings = settings_signal.write();
        settings.wizard_completed = true;
        settings.save();
    });

    rsx! {
        div {
            class: "wizard-step",
            dangerous_inner_html: t!("wizard.third_step.explanation").to_string()
        }
    }
}
