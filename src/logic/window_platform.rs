//! What has to be true of the environment before a window is opened.
//!
//! One function, called before every launcher. It was written out inside
//! `launch_app` and nowhere else, which was correct while `launch_app` was the
//! only thing that opened a window — and stopped being correct the moment
//! [`crate::logic::measure`] opened one too. That mode came up blank and
//! measured nothing, for a reason that had nothing to do with what it was
//! measuring.

/// Prepares the platform for a web view.
///
/// Does nothing at all except on Linux, where two environment variables decide
/// whether a window renders or comes up empty.
pub fn prepare() {
    #[cfg(target_os = "linux")]
    {
        if std::path::Path::new("/dev/dri").exists()
            && std::env::var("XDG_SESSION_TYPE").unwrap_or_default() == "wayland"
        {
            // GNOME's WebKit is buggy under Wayland and KDE, so it is run in
            // XWayland mode instead.
            // See: https://github.com/DioxusLabs/dioxus/issues/3667
            unsafe {
                // Explicit sync off, for the NVIDIA drivers under Wayland.
                std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
            }
        }
        // Unconditional, and deliberately left that way when this moved here.
        //
        // A review read the extraction as a regression — as though the x11
        // override had previously been inside the Wayland/DRI condition above.
        // It was not: `git show 6a97744:src/main.rs` has it outside, exactly as
        // it is here, and this move changed no behaviour at all.
        //
        // Whether it *should* be conditional is a fair question and a separate
        // one. It predates this file, it affects the projection window that
        // every service runs on, and a change to it belongs with somebody who
        // can try it on a machine without XWayland — not with a refactoring
        // that was only meant to stop the measuring window coming up blank.
        unsafe {
            std::env::set_var("GDK_BACKEND", "x11");
        }
    }
}
