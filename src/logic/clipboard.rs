//! Putting something on the system clipboard.
//!
//! One address, offered in two places: beside the switch that started the
//! stream, in the options panel, and beside the preview of that stream in the
//! presenter console. Both are somebody reading a `http://…:8080` off a screen
//! and wanting it in a message to the congregation, and neither should be a
//! second implementation of the same trick.

use dioxus::prelude::*;

/// Puts `text` on the system clipboard, reporting whether it got there.
///
/// Two ways, because one is not enough. `navigator.clipboard` is the right one
/// and is refused outside a secure context — which is exactly what a desktop
/// webview serving from a custom scheme is on some platforms. The fallback is
/// the old trick of selecting a throwaway textarea, which is deprecated
/// everywhere and works everywhere.
///
/// Done in the page rather than through a clipboard crate because a remote
/// console is a browser somewhere else: the clipboard that matters is the one
/// in front of whoever clicked, and the only thing this program can reach
/// there is the page it is already drawing.
pub async fn copy(text: &str) -> bool {
    // Through JSON rather than quoted by hand: the address is ours today, but
    // a string spliced into a script is a hole waiting for the day it is not.
    let literal = serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_string());
    let mut script = document::eval(&format!(
        r#"
        const text = {literal};
        let done = false;
        try {{
            if (navigator.clipboard && window.isSecureContext) {{
                await navigator.clipboard.writeText(text);
                done = true;
            }}
        }} catch (error) {{ done = false; }}
        if (!done) {{
            const area = document.createElement("textarea");
            area.value = text;
            area.setAttribute("readonly", "");
            area.style.position = "fixed";
            area.style.top = "-1000px";
            document.body.appendChild(area);
            area.select();
            try {{ done = document.execCommand("copy"); }} catch (error) {{ done = false; }}
            area.remove();
        }}
        dioxus.send(done);
        "#
    ));
    script.recv::<bool>().await.unwrap_or(false)
}
