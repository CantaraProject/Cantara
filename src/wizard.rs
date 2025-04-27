use dioxus::{
    html::{g::decelerate, img::decoding},
    prelude::*,
};
use crate::states::RuntimeInformation;
use rust_i18n::t;

use crate::LOGO;

rust_i18n::i18n!("locales", fallback = "en");

const MAX_STEPS: u8 = 3;

/// This is a struct representing the step status of the wizard.
#[derive(Debug, Clone, Copy)]
struct WizardStatus {
    is_done: Signal<bool>,
}

#[component]
pub fn Wizard() -> Element {
    let mut step: Signal<u8> = use_signal(|| 1);
    let is_done = use_signal(|| false);

    use_context_provider(|| WizardStatus { is_done });

    let runtime_info: RuntimeInformation = use_context();
    let locale: Signal<String> = use_signal(|| runtime_info.language.clone());

    let mut increase_step = move || {
        if step() < MAX_STEPS {
            step.set(step + 1);
        }
    };

    let mut decrease_step = move || {
        if step() > 1 {
            step.set(step - 1);
        }
    };

    rsx!(
        header {
            lang: locale,
            class: "top-bar",
            h1 { {t!("wizard.title")} }
        }
        main {
            lang: locale,
            class: "container-fluid content",
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
    )
}

#[component]
fn WizardButtons(step: Signal<u8>) -> Element {
    let wizard_status: WizardStatus = use_context();

    let mut increase_step = move || {
        if step() < MAX_STEPS {
            step.set(step + 1);
        }
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

#[component]
fn WizardPage(step: Signal<u8>) -> Element {
    rsx! {
        match step() {
            1 => rsx! { FirstStep {} },
            2 => rsx! { SecondStep {} },
            3 => rsx! { ThirdStep {} },
            _ => rsx! { ThirdStep {} },
        }
    }
}

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

#[component]
fn SecondStep() -> Element {
    let mut wizard_status: WizardStatus = use_context::<WizardStatus>();
    use_effect(move || {
        wizard_status.is_done.set(false);
    });
    rsx! {
        div {
            class: "wizard-step",
            p { "This is the second step!" }
        }
    }
}

#[component]
fn ThirdStep() -> Element {
    rsx! {
        div {
            class: "wizard-step",
            p { "Well done, you finished!" }
        }
    }
}
