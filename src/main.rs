pub mod selection;
pub mod settings;
pub mod settings;
pub mod sourcefiles;
pub mod states;
pub mod wizard;

use dioxus::prelude::*;
use dioxus_router::prelude::*;
use rust_i18n::t;
use selection::Selection;
use settings::*;
use sys_locale::get_locale;
use wizard::Wizard;

rust_i18n::i18n!("locales", fallback = "en");

const PICO_CSS: Asset = asset!("/node_modules/@picocss/pico/css/pico.min.css");
const MAIN_CSS: Asset = asset!("/assets/main.css");

pub const LOGO: Asset = asset!("/assets/cantara-logo_small.png");

#[derive(Routable, PartialEq, Clone)]
#[rustfmt::skip]
pub enum Route {
    #[route("/")]
    Selection,
    
    #[route("/wizard")]
    Wizard
}

fn main() {
    #[cfg(feature = "desktop")]
    fn launch_app() {
        #[cfg(target_os = "linux")]
        {
            if std::path::Path::new("/dev/dri").exists()
                && std::env::var("XDG_SESSION_TYPE").unwrap_or_default() == "wayland"
            {
                // Gnome Webkit is currently buggy under Wayland and KDE, so we will run it with XWayland mode.
                // See: https://github.com/DioxusLabs/dioxus/issues/3667
                unsafe {
                    // Disable explicit sync for NVIDIA drivers on Linux when using Way
                    std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
                    std::env::set_var("GDK_BACKEND", "x11");
                }
            }
        }

        use dioxus::desktop::tao;
        let window = tao::window::WindowBuilder::new()
            .with_resizable(true)
            .with_title("Cantara")
            .with_inner_size(tao::dpi::LogicalSize::new(800.0, 600.0))
            .with_decorations(true)
            .with_visible(true);
        dioxus::LaunchBuilder::new()
            .with_cfg(
                dioxus::desktop::Config::new()
                    .with_window(window)
                    .with_menu(None),
            )
            .launch(App);
    }

    #[cfg(not(feature = "desktop"))]
    fn launch_app() {
        dioxus::launch(App);
    }

    launch_app();
}

#[component]
fn App() -> Element {
    let locale = get_locale().unwrap_or_else(|| String::from("en-US"));

    rust_i18n::set_locale(&locale);

    let cloned_locale = locale.clone();
    use_context_provider(|| states::RuntimeInformation {
        language: cloned_locale,
    });

    // Initialize settings and provide them as a context to all components
    let settings: Signal<Settings> = use_signal(|| Settings::load());
    use_context_provider(|| settings);

    rsx! {
        document::Link { rel: "stylesheet", href: PICO_CSS }
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        document::Title { "Cantara" }
        document::Meta { name: "viewport", content: "width=device-width, initial-scale=1" }
        document::Meta { name: "color-scheme", content: "light dark" }
        document::Meta { name: "content-language", content: locale }

        Router::<Route> {}

    }
}
