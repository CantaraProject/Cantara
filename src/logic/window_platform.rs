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
        unsafe {
            std::env::set_var("GDK_BACKEND", "x11");
        }
    }
}
