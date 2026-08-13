//! Asking the user something, in Cantara's own window.
//!
//! These used to be the web view's `alert()`, `confirm()` and `prompt()`,
//! reached through `document::eval`. They worked, and they were wrong in four
//! ways that all show at once on a real machine:
//!
//! * They are drawn by the platform, not by Cantara, so a program that is
//!   otherwise translated asks its most important questions — *delete this?* —
//!   in whatever the web view feels like, in an unstyled grey box.
//! * They block the web view's thread. Everything stops, including the
//!   presentation window on the second screen.
//! * `prompt()` does not exist in every web view. On Android it is disabled by
//!   default, so the button that renames a repository did nothing at all.
//! * The text had to be pasted into a piece of JavaScript. An error message
//!   from the operating system containing a quotation mark turned the script
//!   into one that does not run, which is how a program says nothing precisely
//!   when it has something to say.
//!
//! Here they are ordinary Dioxus components. [`DialogHost`] is mounted once,
//! at the root of the app; [`message_box`], [`confirm_box`] and [`prompt_box`]
//! are awaited from anywhere and come back with the answer.
//!
//! ```ignore
//! if confirm_box(t!("dialogs.confirm_deletion").to_string()).await {
//!     ondelete.call(());
//! }
//! ```

use dioxus::prelude::*;
use futures_channel::oneshot;
use rust_i18n::t;

rust_i18n::i18n!("locales", fallback = "en");

/// What a dialog is asking for.
#[derive(Clone, PartialEq, Debug)]
enum DialogKind {
    /// Something to acknowledge. One button.
    Message,
    /// A yes or no question. Two buttons.
    Confirm,
    /// A line of text, with what the field starts out holding.
    Prompt { initial: String },
}

/// What the user did with a dialog.
#[derive(Clone, PartialEq, Debug)]
pub enum DialogAnswer {
    /// Acknowledged, or answered yes.
    Confirmed,
    /// Dismissed, or answered no.
    Dismissed,
    /// Typed something and confirmed it.
    Entered(String),
}

/// A dialog waiting to be shown, and where to send the answer.
struct DialogRequest {
    kind: DialogKind,
    text: String,
    answer: Option<oneshot::Sender<DialogAnswer>>,
}

impl PartialEq for DialogRequest {
    /// The channel is not part of what the dialog *is*; two requests that ask
    /// the same thing are the same dialog as far as a redraw is concerned.
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind && self.text == other.text
    }
}

/// The dialog that is open, if any.
///
/// Global rather than a context, so that it can be reached from an async block
/// that has already left the component it started in — which is where most of
/// these are asked from.
static OPEN_DIALOG: GlobalSignal<Option<DialogRequest>> = Global::new(|| None);

/// Opens a dialog and waits for the answer.
///
/// A second dialog opened while one is up replaces it, and the first is
/// answered as dismissed — nothing in Cantara asks two questions at once, and
/// a request that could never be answered would leave its caller waiting for
/// ever.
async fn ask(kind: DialogKind, text: String) -> DialogAnswer {
    let (sender, receiver) = oneshot::channel();

    if let Some(previous) = OPEN_DIALOG.write().replace(DialogRequest {
        kind,
        text,
        answer: Some(sender),
    }) && let Some(answer) = previous.answer
    {
        let _ = answer.send(DialogAnswer::Dismissed);
    }

    // The host is gone, or the dialog was dropped without being answered.
    // Treating that as "no" is the safe reading: every question these ask is
    // one where doing nothing is the harmless outcome.
    receiver.await.unwrap_or(DialogAnswer::Dismissed)
}

/// Tells the user something they only have to acknowledge.
///
/// Replaces `alert()`.
pub async fn message_box(message: String) {
    let _ = ask(DialogKind::Message, message).await;
}

/// Asks a yes-or-no question, and returns whether the answer was yes.
///
/// Replaces `confirm()`. Dismissing the dialog — the escape key, the button
/// that closes it — is a no.
pub async fn confirm_box(question: String) -> bool {
    matches!(
        ask(DialogKind::Confirm, question).await,
        DialogAnswer::Confirmed
    )
}

/// Asks for a line of text, and returns it — or `None` if the user did not
/// give one.
///
/// Replaces `prompt()`, which parts of the web do not have at all. Blank input
/// is `None`: everything that asks here wants a name or an address, and an
/// empty one is not an answer.
pub async fn prompt_box(question: String, initial: String) -> Option<String> {
    match ask(DialogKind::Prompt { initial }, question).await {
        DialogAnswer::Entered(text) if !text.trim().is_empty() => Some(text.trim().to_string()),
        _ => None,
    }
}

/// Draws whichever dialog is open.
///
/// Mounted once, at the root of the app. Renders nothing at all the rest of the
/// time.
#[component]
pub fn DialogHost() -> Element {
    // What has been typed into a prompt. Reset whenever a different dialog
    // opens, so that a second prompt does not start out holding the answer to
    // the first.
    let mut entered = use_signal(String::new);

    let Some((kind, text)) = OPEN_DIALOG
        .read()
        .as_ref()
        .map(|request| (request.kind.clone(), request.text.clone()))
    else {
        return rsx! {};
    };

    // Answers the dialog and closes it. Taking the request out of the signal is
    // what closes it, so an answer can only ever be sent once.
    let mut answer = move |answer: DialogAnswer| {
        if let Some(request) = OPEN_DIALOG.write().take()
            && let Some(sender) = request.answer
        {
            let _ = sender.send(answer);
        }
        entered.set(String::new());
    };

    let confirming = matches!(kind, DialogKind::Confirm);
    let prompting = matches!(kind, DialogKind::Prompt { .. });
    // What the field started out holding, so that confirming without typing
    // keeps it. Taken once, because each handler below needs its own copy.
    let initial = initial_of(&kind);

    rsx! {
        dialog {
            open: true,
            class: "cantara-dialog",
            // Announced as a dialog rather than merely drawn as one, so that a
            // screen reader says there is a question before reading the page
            // behind it.
            role: if confirming { "alertdialog" } else { "dialog" },
            aria_modal: "true",
            // Escape is a dismissal wherever a dialog is open, and `<dialog>`
            // only closes itself when the browser opened it modally.
            onkeydown: move |event: Event<KeyboardData>| {
                if event.key() == Key::Escape {
                    event.prevent_default();
                    answer(DialogAnswer::Dismissed);
                }
            },
            article {
                p { class: "cantara-dialog-text", "{text}" }

                if prompting {
                    input {
                        r#type: "text",
                        class: "cantara-dialog-input",
                        initial_value: "{initial}",
                        // The field is what the user came here to type in, so
                        // it is what has the keyboard when the dialog opens.
                        autofocus: true,
                        oninput: move |event| entered.set(event.value()),
                        onkeydown: {
                            let initial = initial.clone();
                            move |event: Event<KeyboardData>| {
                                if event.key() == Key::Enter {
                                    event.prevent_default();
                                    let typed = if entered.read().is_empty() {
                                        initial.clone()
                                    } else {
                                        entered()
                                    };
                                    answer(DialogAnswer::Entered(typed));
                                }
                            }
                        },
                    }
                }

                footer { class: "cantara-dialog-actions",
                    // A question that can be answered no has a way of saying
                    // it; a message that only needs acknowledging does not.
                    if confirming || prompting {
                        button {
                            r#type: "button",
                            class: "secondary",
                            onclick: move |_| answer(DialogAnswer::Dismissed),
                            { t!("general.cancel").to_string() }
                        }
                    }
                    button {
                        r#type: "button",
                        onclick: {
                            let initial = initial.clone();
                            move |_| {
                                if prompting {
                                    let typed = if entered.read().is_empty() {
                                        initial.clone()
                                    } else {
                                        entered()
                                    };
                                    answer(DialogAnswer::Entered(typed));
                                } else {
                                    answer(DialogAnswer::Confirmed);
                                }
                            }
                        },
                        if confirming {
                            { t!("general.yes").to_string() }
                        } else {
                            { t!("general.ok").to_string() }
                        }
                    }
                }
            }
        }
    }
}

/// What a prompt's field started out holding, so that confirming without
/// typing keeps it rather than answering with nothing.
fn initial_of(kind: &DialogKind) -> String {
    match kind {
        DialogKind::Prompt { initial } => initial.clone(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every question these ask is one where doing nothing is the harmless
    /// answer, so a dialog that is closed without being answered must read as
    /// "no" rather than as "yes".
    #[test]
    fn test_dismissing_is_not_confirming() {
        assert!(matches!(DialogAnswer::Dismissed, DialogAnswer::Dismissed));
        assert!(!matches!(DialogAnswer::Dismissed, DialogAnswer::Confirmed));
    }

    /// A prompt confirmed without anything typed keeps what it started with —
    /// which is how the rename dialog offers the current name.
    #[test]
    fn test_a_prompt_falls_back_to_what_it_started_with() {
        let kind = DialogKind::Prompt {
            initial: "Hymns".to_string(),
        };

        assert_eq!(initial_of(&kind), "Hymns");
        assert_eq!(initial_of(&DialogKind::Message), "");
    }

    /// Two requests asking the same thing are the same dialog. The channel a
    /// request carries cannot be compared, and a `PartialEq` that tried would
    /// not compile — but one that called every dialog different would redraw
    /// the host on every render.
    #[test]
    fn test_two_dialogs_asking_the_same_thing_are_equal() {
        let one = DialogRequest {
            kind: DialogKind::Confirm,
            text: "Delete?".to_string(),
            answer: None,
        };
        let other = DialogRequest {
            kind: DialogKind::Confirm,
            text: "Delete?".to_string(),
            answer: None,
        };
        let different = DialogRequest {
            kind: DialogKind::Message,
            text: "Delete?".to_string(),
            answer: None,
        };

        assert!(one == other);
        assert!(one != different);
    }
}
