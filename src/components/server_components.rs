//! The servers this program is logged in to, in the settings.
//!
//! Logging in opens the server's own login page in the browser (see
//! [`crate::logic::server::login`]); what comes back is an instance in the
//! settings, with the organisation it acts in and the libraries it offers.
//! A library ticked here is a repository like any other from then on.

use cantara_api::{LibraryInfo, TenantMembership};
use dioxus::prelude::*;
use rust_i18n::t;
use uuid::Uuid;

use super::dialogs::{confirm_box, message_box, prompt_box};
use crate::logic::server::{client_for, ServerInstance};
use crate::logic::settings::use_settings;

rust_i18n::i18n!("locales", fallback = "en");

/// What a server says about an instance when asked.
#[derive(Clone, PartialEq)]
struct Offered {
    tenants: Vec<TenantMembership>,
    libraries: Result<Vec<LibraryInfo>, String>,
}

async fn offered(instance: ServerInstance) -> Offered {
    let Some(client) = client_for(instance.uuid) else {
        return Offered {
            tenants: Vec::new(),
            libraries: Err(t!("settings.server_unknown").to_string()),
        };
    };
    let tenants = client.me().await.map(|me| me.tenants).unwrap_or_default();
    let libraries = match instance.tenant {
        Some(tenant) => client.libraries(tenant).await.map_err(|error| error.to_string()),
        None => Ok(Vec::new()),
    };
    Offered { tenants, libraries }
}

#[component]
pub fn ServerSettings() -> Element {
    // Written only by the desktop's login; the browser build has none.
    #[allow(unused_mut)]
    let mut settings = use_settings();
    #[allow(unused_mut)]
    let mut busy = use_signal(|| false);
    // The code to type in elsewhere, while a login by code is waiting.
    #[allow(unused_mut)]
    let mut device_code: Signal<Option<cantara_api::DeviceAuthorization>> = use_signal(|| None);
    let instances = use_memo(move || settings.read().server_instances.clone());

    let login_with_code = move |_| async move {
        let Some(address) = prompt_box(t!("settings.server_login_prompt_code").to_string(), String::new()).await else {
            return;
        };
        #[cfg(not(target_arch = "wasm32"))]
        {
            busy.set(true);
            let result = crate::logic::server::login::login_with_code(&address, |authorization| {
                device_code.set(Some(authorization.clone()));
            })
            .await;
            busy.set(false);
            device_code.set(None);
            match result {
                Ok(logged_in) => settings.write().add_server_instance(logged_in.instance),
                Err(error) => {
                    message_box(t!("settings.server_login_failed", reason = error).to_string()).await;
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = address;
            message_box(t!("settings.server_login_unavailable").to_string()).await;
        }
    };

    let login = move |_| async move {
        let Some(address) = prompt_box(t!("settings.server_login_prompt").to_string(), String::new()).await else {
            return;
        };
        #[cfg(not(target_arch = "wasm32"))]
        {
            busy.set(true);
            let result = crate::logic::server::login::login(&address).await;
            busy.set(false);
            match result {
                Ok(logged_in) => {
                    settings.write().add_server_instance(logged_in.instance);
                }
                Err(error) => {
                    message_box(t!("settings.server_login_failed", reason = error).to_string()).await;
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = address;
            message_box(t!("settings.server_login_unavailable").to_string()).await;
        }
    };

    rsx! {
        hgroup {
            h3 { { t!("settings.servers_headline").to_string() } }
            p { { t!("settings.servers_description").to_string() } }
        }
        if instances().is_empty() {
            p { class: "muted", { t!("settings.servers_none").to_string() } }
        }
        for instance in instances() {
            ServerInstanceCard { key: "{instance.uuid}", instance: instance.clone() }
        }
        if let Some(code) = device_code() {
            article { class: "server-device-code",
                p { { t!("settings.server_code_instructions").to_string() } }
                p { a { href: "{code.verification_uri_complete}", target: "_blank", { code.verification_uri.clone() } } }
                p { class: "server-device-code-value", strong { { code.user_code.clone() } } }
            }
        }
        div { class: "grid",
            button {
                class: "smaller-buttons",
                disabled: busy(),
                aria_busy: if busy() { "true" } else { "false" },
                onclick: login,
                if busy() {
                    { t!("settings.server_login_waiting").to_string() }
                } else {
                    { t!("settings.server_login").to_string() }
                }
            }
            button {
                class: "smaller-buttons secondary",
                disabled: busy(),
                onclick: login_with_code,
                { t!("settings.server_login_code").to_string() }
            }
        }
    }
}

#[component]
fn ServerInstanceCard(instance: ServerInstance) -> Element {
    let mut settings = use_settings();
    let uuid = instance.uuid;
    let for_resource = instance.clone();
    let offered = use_resource(move || offered(for_resource.clone()));

    let logout = move |_| async move {
        if !confirm_box(t!("settings.server_logout_confirm").to_string()).await {
            return;
        }
        if let Some(client) = client_for(uuid) {
            // The token is revoked by forgetting it here; telling the server
            // too is a courtesy that may fail offline.
            let _ = client.logout().await;
        }
        settings.write().remove_server_instance(uuid);
    };

    let tenant = instance.tenant;
    rsx! {
        article { class: "listed-article",
            h6 {
                { instance.name.clone() }
                span { style: "float:right", class: "secondary", onclick: logout,
                    { t!("settings.server_logout").to_string() }
                }
                if offered.read().as_ref().is_some_and(|offered| offered.tenants.iter().any(|membership| Some(membership.tenant.uuid) == tenant && membership.is_admin)) {
                    Link { style: "float:right; margin-right: 1rem", to: crate::Route::AdminPage {},
                        { t!("admin.open").to_string() }
                    }
                }
            }
            p { class: "muted",
                { instance.url.clone() }
                br {}
                { t!("settings.server_logged_in_as", user = instance.user_name.clone(), email = instance.user_email.clone()).to_string() }
            }
            match offered.read().clone() {
                None => rsx! { p { aria_busy: "true", { t!("settings.server_asking").to_string() } } },
                Some(offered) => rsx! {
                    if offered.tenants.len() > 1 {
                        label {
                            { t!("settings.server_tenant").to_string() }
                            select {
                                onchange: move |event| {
                                    let chosen = event.value();
                                    if let Ok(chosen) = Uuid::parse_str(&chosen) {
                                        let name = offered
                                            .tenants
                                            .iter()
                                            .find(|membership| membership.tenant.uuid == chosen)
                                            .map(|membership| membership.tenant.name.clone())
                                            .unwrap_or_default();
                                        settings.write().set_server_tenant(uuid, chosen, name);
                                    }
                                },
                                for membership in offered.tenants.clone() {
                                    option {
                                        value: "{membership.tenant.uuid}",
                                        selected: Some(membership.tenant.uuid) == tenant,
                                        { membership.tenant.name.clone() }
                                    }
                                }
                            }
                        }
                    }
                    match offered.libraries.clone() {
                        Err(error) => rsx! {
                            p { class: "muted", { t!("settings.server_unreachable", reason = error).to_string() } }
                        },
                        Ok(libraries) if libraries.is_empty() => rsx! {
                            p { class: "muted", { t!("settings.server_no_libraries").to_string() } }
                        },
                        Ok(libraries) => rsx! {
                            fieldset {
                                legend { { t!("settings.server_libraries").to_string() } }
                                for library in libraries {
                                    LibraryChoice { key: "{library.uuid}", instance: uuid, tenant: tenant.unwrap_or_default(), library: library.clone() }
                                }
                            }
                        },
                    }
                    if let Some(tenant) = tenant {
                        ServerDesigns { instance: uuid, tenant }
                    }
                },
            }
        }
    }
}

#[component]
fn LibraryChoice(instance: Uuid, tenant: Uuid, library: LibraryInfo) -> Element {
    let mut settings = use_settings();
    let used = settings.read().uses_server_library(instance, library.uuid);
    let for_change = library.clone();
    rsx! {
        label {
            input {
                r#type: "checkbox",
                role: "switch",
                checked: used,
                onchange: move |event| {
                    settings.write().use_server_library(instance, tenant, &for_change, event.checked());
                },
            }
            { library.name.clone() }
            span { class: "muted",
                {
                    format!(
                        " — {}, {}",
                        t!("settings.server_library_count", count = library.item_count),
                        human_size(library.size_bytes),
                    )
                }
            }
        }
        if let Some(offline) = settings.read().server_library_offline(instance, library.uuid) {
            label { class: "server-library-offline",
                input {
                    r#type: "checkbox",
                    checked: offline,
                    onchange: move |event| {
                        settings.write().set_server_library_offline(instance, library.uuid, event.checked());
                    },
                }
                { t!("settings.server_library_offline").to_string() }
            }
        }
    }
}

/// `1.4 GB`, `320 MB`, `12 KB`.
fn human_size(bytes: i64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut size = bytes.max(0) as f64;
    let mut unit = 0;
    while size >= 1000.0 && unit < UNITS.len() - 1 {
        size /= 1000.0;
        unit += 1;
    }
    if unit == 0 || size >= 10.0 {
        format!("{size:.0} {}", UNITS[unit])
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

/// Shares the running presentation through a server: a link to watch and a
/// link to control, from anywhere the server is reached. Beside the LAN
/// stream, not instead of it.
#[component]
pub fn ServerShareSwitch() -> Element {
    use crate::logic::server::session;

    let settings = use_settings();
    let mut sharing = use_signal(session::status);
    let mut failure: Signal<Option<String>> = use_signal(|| None);
    let mut starting = use_signal(|| false);
    let mut anonymous_control = use_signal(|| false);
    let mut chosen: Signal<Option<Uuid>> = use_signal(|| None);

    // The connection lives outside this panel; this keeps what the panel shows
    // in step with it.
    use_future(move || async move {
        loop {
            crate::logic::timer::sleep(std::time::Duration::from_millis(1000)).await;
            let now = session::status();
            if *sharing.peek() != now {
                sharing.set(now);
            }
        }
    });

    let instances: Vec<ServerInstance> = settings
        .read()
        .server_instances
        .iter()
        .filter(|instance| instance.tenant.is_some() && instance.sharing)
        .cloned()
        .collect();
    if instances.is_empty() && sharing().is_none() {
        return rsx! {};
    }
    let instance = chosen()
        .and_then(|uuid| instances.iter().find(|instance| instance.uuid == uuid).cloned())
        .or_else(|| instances.first().cloned());

    let share = move |_| {
        let Some(instance) = instance.clone() else { return };
        let Some(tenant) = instance.tenant else { return };
        failure.set(None);
        starting.set(true);
        spawn(async move {
            let result = async {
                let client = client_for(instance.uuid).ok_or_else(|| t!("settings.server_unknown").to_string())?;
                let name = format!("{} {}", t!("selection.server_share_session_name"), chrono::Local::now().format("%d.%m.%Y %H:%M"));
                let created = client
                    .create_session(tenant, &cantara_api::CreateSession { name, anonymous_control: anonymous_control() })
                    .await
                    .map_err(|error| error.to_string())?;
                session::start(instance.uuid, tenant, created)
            }
            .await;
            starting.set(false);
            match result {
                Ok(()) => sharing.set(session::status()),
                Err(reason) => failure.set(Some(reason)),
            }
        });
    };

    rsx! {
        hgroup {
            h6 { { t!("selection.server_share_headline").to_string() } }
            p { { t!("selection.server_share_description").to_string() } }
        }
        match sharing() {
            Some(current) => rsx! {
                p {
                    if current.connected {
                        { t!("selection.server_share_running", viewers = current.viewers).to_string() }
                    } else if let Some(error) = current.error.clone() {
                        { t!("selection.server_share_broken", reason = error).to_string() }
                    } else {
                        span { aria_busy: "true", { t!("selection.server_share_connecting").to_string() } }
                    }
                }
                if let Some(link) = current.session.watch_link.clone() {
                    ShareLink { label: t!("selection.server_share_watch").to_string(), link }
                }
                if let Some(link) = current.session.control_link.clone() {
                    ShareLink { label: t!("selection.server_share_control").to_string(), link }
                }
                button {
                    class: "smaller-buttons secondary",
                    onclick: move |_| {
                        session::stop(true);
                        sharing.set(None);
                    },
                    { t!("selection.server_share_stop").to_string() }
                }
            },
            None => rsx! {
                if instances.len() > 1 {
                    select {
                        onchange: move |event| chosen.set(Uuid::parse_str(&event.value()).ok()),
                        for option in instances.clone() {
                            option { value: "{option.uuid}", { format!("{} — {}", option.name, option.tenant_name) } }
                        }
                    }
                }
                label {
                    input {
                        r#type: "checkbox",
                        checked: anonymous_control(),
                        onchange: move |event| anonymous_control.set(event.checked()),
                    }
                    { t!("selection.server_share_anonymous_control").to_string() }
                }
                button {
                    class: "smaller-buttons",
                    disabled: starting(),
                    aria_busy: if starting() { "true" } else { "false" },
                    onclick: share,
                    { t!("selection.server_share_start").to_string() }
                }
            },
        }
        if let Some(reason) = failure() {
            p { class: "stream-copy-failed", { t!("selection.server_share_failed", reason = reason).to_string() } }
        }
    }
}

#[component]
fn ShareLink(label: String, link: String) -> Element {
    let mut copied: Signal<Option<bool>> = use_signal(|| None);
    let for_copy = link.clone();
    rsx! {
        p { style: "margin: 0.3rem 0;",
            small { { label } }
            br {}
            code {
                class: "stream-address",
                title: t!("selection.stream_copy_hint").to_string(),
                onclick: move |_| {
                    let link = for_copy.clone();
                    spawn(async move {
                        copied.set(Some(crate::logic::clipboard::copy(&link).await));
                        crate::logic::timer::sleep(std::time::Duration::from_millis(3000)).await;
                        copied.set(None);
                    });
                },
                { link.clone() }
            }
            match copied() {
                Some(true) => rsx! { span { class: "stream-copied", { t!("selection.stream_copied").to_string() } } },
                Some(false) => rsx! { span { class: "stream-copy-failed", { t!("selection.stream_copy_failed").to_string() } } },
                None => rsx! {},
            }
        }
    }
}

// ── Watching or controlling a shared presentation ────────────────────────────

#[cfg(target_arch = "wasm32")]
mod viewer {
    use std::cell::RefCell;

    use cantara_api::{apply_patch, SessionAccess, SessionServerMessage};
    use serde_json::Value;

    #[derive(Default)]
    pub struct Viewer {
        pub remote: Option<Value>,
        pub changed: bool,
        pub access: Option<SessionAccess>,
        pub ended: bool,
        pub closed: bool,
        pub error: Option<String>,
        pub socket: Option<web_sys::WebSocket>,
    }

    thread_local! {
        pub static VIEWER: RefCell<Viewer> = RefCell::new(Viewer::default());
    }

    pub fn on_message(message: SessionServerMessage) {
        VIEWER.with(|viewer| {
            let mut viewer = viewer.borrow_mut();
            match message {
                SessionServerMessage::Snapshot { state, access, .. } => {
                    viewer.access = Some(access);
                    viewer.remote = state;
                    viewer.changed = true;
                }
                SessionServerMessage::Patch { changes, .. } => {
                    let mut state = viewer.remote.take().unwrap_or(Value::Null);
                    apply_patch(&mut state, &changes);
                    viewer.remote = Some(state);
                    viewer.changed = true;
                }
                SessionServerMessage::Ended => viewer.ended = true,
                SessionServerMessage::Error { message } => viewer.error = Some(message),
                SessionServerMessage::Viewers { .. } => {}
            }
        });
    }

    pub fn on_close(error: Option<String>) {
        VIEWER.with(|viewer| {
            let mut viewer = viewer.borrow_mut();
            viewer.closed = true;
            if error.is_some() {
                viewer.error = error;
            }
        });
    }
}

/// What a session link shows: the presentation, to watch, or the presenter
/// console, to control it. Only in the server's own web build, which is the
/// page such a link leads to.
#[component]
pub fn SharedSession(token: String) -> Element {
    #[cfg(target_arch = "wasm32")]
    {
        shared_session_web(token)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = token;
        rsx! { p { { t!("selection.server_share_not_here").to_string() } } }
    }
}

#[cfg(target_arch = "wasm32")]
fn shared_session_web(token: String) -> Element {
    use crate::components::presentation_components::PresentationPage;
    use crate::components::presenter_console_components::PresenterConsolePage;
    use crate::logic::server::SAME_ORIGIN;
    use crate::logic::states::RunningPresentation;
    use cantara_api::{diff_top_level, SessionAccess, SessionClientMessage};
    use viewer::VIEWER;

    let mut running_presentations: Signal<Vec<RunningPresentation>> = use_context();
    let mut access: Signal<Option<SessionAccess>> = use_signal(|| None);
    let mut problem: Signal<Option<String>> = use_signal(|| None);
    let mut ended = use_signal(|| false);
    let mut media_generation = use_signal(|| 0u32);

    use_future(move || {
        let token = token.clone();
        async move {
            let Some(client) = client_for(SAME_ORIGIN) else { return };
            let link = match client.link(&token).await {
                Ok(link) => link,
                Err(_) => {
                    problem.set(Some(t!("selection.server_share_link_invalid").to_string()));
                    return;
                }
            };
            if link.login_required && client.me().await.is_err() {
                if let Some(window) = web_sys::window() {
                    let _ = window.location().set_href(&format!("/login?next=/s/{token}"));
                }
                return;
            }
            let url = client.socket_url(link.session, Some(&token));
            match crate::logic::server::session::open_socket(&url, viewer::on_message, viewer::on_close) {
                Ok(socket) => VIEWER.with(|viewer| viewer.borrow_mut().socket = Some(socket)),
                Err(reason) => {
                    problem.set(Some(reason));
                    return;
                }
            }

            // What the server last said the presentation is, as this page
            // sent it or received it — the base a controller's changes are
            // measured against.
            let mut known: Option<serde_json::Value> = None;
            // The pictures and documents the slides name that are not here
            // yet, and when they were last asked for: the presenting computer
            // hands them over as it gets to them.
            let mut missing: std::collections::HashSet<String> = std::collections::HashSet::new();
            let mut ticks: u32 = 0;
            let mut asked_at: u32 = 0;
            loop {
                ticks = ticks.wrapping_add(1);
                // What is missing is asked for at once, and then every second
                // until the presenting computer has handed it over.
                if !missing.is_empty() && ticks.wrapping_sub(asked_at) >= 20 {
                    asked_at = ticks;
                    let mut arrived = false;
                    for path in missing.iter().cloned().collect::<Vec<_>>() {
                        if let Ok(bytes) = client.session_media(link.session, &path, Some(&token)).await {
                            crate::logic::settings::RepositoryType::store_web_file(&path, bytes);
                            missing.remove(&path);
                            arrived = true;
                        }
                    }
                    // The slides are drawn afresh: a picture slide whose
                    // picture was not there drew an empty box, and the same
                    // slide again would not be drawn again.
                    if arrived {
                        media_generation += 1;
                    }
                }
                crate::logic::timer::sleep(std::time::Duration::from_millis(50)).await;
                let (incoming, now_access, finished, error) = VIEWER.with(|viewer| {
                    let mut viewer = viewer.borrow_mut();
                    let incoming = if viewer.changed {
                        viewer.changed = false;
                        viewer.remote.clone()
                    } else {
                        None
                    };
                    (incoming, viewer.access, viewer.ended || viewer.closed, viewer.error.clone())
                });
                if *access.peek() != now_access {
                    access.set(now_access);
                }
                if let Some(state) = incoming {
                    // What the slides name is fetched before they are drawn,
                    // where the presenting computer has handed it over
                    // already; what it has not is asked for again below.
                    for path in cantara_api::media_paths(&state) {
                        if crate::logic::settings::RepositoryType::web_read_file(&path).is_some() {
                            continue;
                        }
                        match client.session_media(link.session, &path, Some(&token)).await {
                            Ok(bytes) => crate::logic::settings::RepositoryType::store_web_file(&path, bytes),
                            Err(_) => {
                                missing.insert(path);
                            }
                        }
                    }
                    if let Ok(presentation) = serde_json::from_value::<RunningPresentation>(state.clone()) {
                        running_presentations.set(vec![presentation]);
                    }
                    known = Some(state);
                } else if now_access == Some(SessionAccess::Control) {
                    // The console here changed the presentation: tell the
                    // server, which tells the room.
                    let here = running_presentations.peek().first().and_then(|presentation| serde_json::to_value(presentation).ok());
                    if let (Some(here), Some(before)) = (here, known.clone())
                        && here != before
                    {
                        let message = SessionClientMessage::Patch { changes: diff_top_level(&before, &here) };
                        if let Ok(text) = serde_json::to_string(&message) {
                            VIEWER.with(|viewer| {
                                if let Some(socket) = &viewer.borrow().socket {
                                    let _ = socket.send_with_str(&text);
                                }
                            });
                        }
                        known = Some(here);
                    }
                }
                if finished {
                    ended.set(true);
                    if let Some(error) = error {
                        problem.set(Some(error));
                    }
                    return;
                }
            }
        }
    });

    if ended() {
        return rsx! {
            main { class: "container",
                h3 { { t!("selection.server_share_ended").to_string() } }
                if let Some(problem) = problem() { p { class: "muted", { problem } } }
            }
        };
    }
    if let Some(problem) = problem() {
        return rsx! { main { class: "container", p { { problem } } } };
    }
    if running_presentations.read().is_empty() {
        return rsx! {
            main { class: "container",
                p { aria_busy: "true", { t!("selection.server_share_waiting").to_string() } }
            }
        };
    }
    // A picture that arrived after its slide was drawn needs the slide drawn
    // again, and the same slide in the same place is not. Two different
    // wrappers, taking turns, make it a different place.
    let fresh = media_generation() % 2 == 1;
    let control = access() == Some(SessionAccess::Control);
    match (control, fresh) {
        (true, false) => rsx! { div { PresenterConsolePage {} } },
        (true, true) => rsx! { section { PresenterConsolePage {} } },
        (false, false) => rsx! { div { PresentationPage {} } },
        (false, true) => rsx! { section { PresentationPage {} } },
    }
}

// ── Running orders kept on a server ──────────────────────────────────────────

/// One running order on one server, as the list shows it.
#[derive(Clone, PartialEq)]
struct ListedSelection {
    instance: ServerInstance,
    tenant: Uuid,
    summary: cantara_api::SelectionSummary,
}

async fn list_selections(instances: Vec<ServerInstance>) -> (Vec<ListedSelection>, Vec<String>) {
    let mut listed = Vec::new();
    let mut problems = Vec::new();
    for instance in instances {
        let (Some(tenant), Some(client)) = (instance.tenant, client_for(instance.uuid)) else {
            continue;
        };
        match client.selections(tenant).await {
            Ok(selections) => listed.extend(selections.into_iter().map(|summary| ListedSelection {
                instance: instance.clone(),
                tenant,
                summary,
            })),
            Err(error) => problems.push(format!("{}: {error}", instance.name)),
        }
    }
    (listed, problems)
}

/// The running order on screen, as a manifest a server keeps.
fn manifest_of(items: &[crate::logic::states::SelectedItemRepresentation]) -> Result<serde_json::Value, String> {
    crate::logic::selection_io::write_server_manifest(
        items,
        &crate::logic::sourcefiles::read_source_file_bytes,
        &|file| {
            crate::logic::server::item_of(&file.path.to_string_lossy()).map(|item| (item.lineage, item.version))
        },
    )
    .map_err(|error| format!("{error:?}"))
}

/// Opening a running order from a server, and saving the one on screen to
/// one. Only where the program is logged in to a server.
#[component]
pub fn ServerSelectionsButton(
    selected_items: Signal<Vec<crate::logic::states::SelectedItemRepresentation>>,
) -> Element {
    let settings = use_settings();
    let mut open = use_signal(|| false);
    let usable = settings.read().server_instances.iter().any(|instance| instance.tenant.is_some());
    if !usable {
        return rsx! {};
    }
    rsx! {
        button {
            class: "outline secondary smaller-buttons",
            onclick: move |_| open.set(true),
            span { class: "mobile-only",
                dioxus_free_icons::Icon { icon: dioxus_free_icons::icons::fa_solid_icons::FaCloud }
            }
            span { class: "desktop-only", { t!("selection.server_selections").to_string() } }
        }
        if open() {
            ServerSelectionsDialog { open, selected_items }
        }
    }
}

#[component]
fn ServerSelectionsDialog(
    open: Signal<bool>,
    selected_items: Signal<Vec<crate::logic::states::SelectedItemRepresentation>>,
) -> Element {
    use crate::logic::selection_io::{import_designs, read_server_manifest, resolve_selection, selected_item_of, AssetOrigin};
    use crate::logic::sourcefiles::SourceFile;

    let mut settings = use_settings();
    let source_files: Signal<Vec<SourceFile>> = use_context();
    let mut message: Signal<Option<String>> = use_signal(|| None);
    let mut generation = use_signal(|| 0u32);
    let instances: Vec<ServerInstance> = settings.read().server_instances.clone();
    let listing = use_resource(move || {
        let _ = generation();
        list_selections(instances.clone())
    });

    let open_selection = move |listed: ListedSelection| {
        spawn(async move {
            let Some(client) = client_for(listed.instance.uuid) else { return };
            let detail = match client.selection(listed.tenant, listed.summary.uuid).await {
                Ok(detail) => detail,
                Err(error) => {
                    message.set(Some(error.to_string()));
                    return;
                }
            };
            let document = match read_server_manifest(&detail.manifest) {
                Ok(document) => document,
                Err(error) => {
                    message.set(Some(format!("{error:?}")));
                    return;
                }
            };
            let known_divisions: Vec<_> = settings
                .read()
                .song_slide_settings
                .iter()
                .map(|named| named.settings.clone())
                .collect();
            let resolved = resolve_selection(
                &document,
                &source_files.read(),
                &settings.read().presentation_designs,
                &known_divisions,
            );
            let mut items = Vec::new();
            let mut missing = 0;
            for entry in &resolved.items {
                let item = match &entry.origin {
                    AssetOrigin::InLibrary(path) => selected_item_of(entry, Some(path.clone())),
                    AssetOrigin::Inline => selected_item_of(entry, None),
                    AssetOrigin::New | AssetOrigin::Missing => None,
                };
                match item {
                    Some(item) => items.push(item),
                    None => missing += 1,
                }
            }
            import_designs(&mut settings.write(), &resolved);
            selected_items.set(items);
            if missing > 0 {
                message.set(Some(t!("selection.server_selection_missing", count = missing).to_string()));
            } else {
                open.set(false);
            }
        });
    };

    let save_new = move |instance: ServerInstance| {
        spawn(async move {
            let (Some(tenant), Some(client)) = (instance.tenant, client_for(instance.uuid)) else { return };
            let manifest = match manifest_of(&selected_items.read()) {
                Ok(manifest) => manifest,
                Err(error) => {
                    message.set(Some(error));
                    return;
                }
            };
            let Some(name) = prompt_box(t!("selection.server_selection_name").to_string(), String::new()).await else {
                return;
            };
            let request = cantara_api::SaveSelection {
                name,
                date: Some(chrono::Local::now().date_naive()),
                note: String::new(),
                manifest,
            };
            match client.create_selection(tenant, &request).await {
                Ok(_) => {
                    message.set(Some(t!("selection.server_selection_saved").to_string()));
                    generation += 1;
                }
                Err(error) => message.set(Some(error.to_string())),
            }
        });
    };

    let overwrite = move |listed: ListedSelection| {
        spawn(async move {
            if !confirm_box(t!("selection.server_selection_overwrite_confirm", name = listed.summary.name.clone()).to_string()).await {
                return;
            }
            let Some(client) = client_for(listed.instance.uuid) else { return };
            let manifest = match manifest_of(&selected_items.read()) {
                Ok(manifest) => manifest,
                Err(error) => {
                    message.set(Some(error));
                    return;
                }
            };
            let request = cantara_api::SaveSelection {
                name: listed.summary.name.clone(),
                date: listed.summary.date,
                note: listed.summary.note.clone(),
                manifest,
            };
            match client.save_selection(listed.tenant, listed.summary.uuid, &request).await {
                Ok(_) => {
                    message.set(Some(t!("selection.server_selection_saved").to_string()));
                    generation += 1;
                }
                Err(error) => message.set(Some(error.to_string())),
            }
        });
    };

    let has_items = !selected_items.read().is_empty();
    let instances: Vec<ServerInstance> = settings
        .read()
        .server_instances
        .iter()
        .filter(|instance| instance.tenant.is_some())
        .cloned()
        .collect();

    rsx! {
        div { class: "modal-overlay export-menu-overlay", onclick: move |_| open.set(false),
            div {
                class: "export-menu-modal import-modal",
                onclick: move |event: Event<MouseData>| event.stop_propagation(),
                h3 { { t!("selection.server_selections_title").to_string() } }
                if has_items {
                    for instance in instances {
                        button {
                            class: "smaller-buttons",
                            onclick: {
                                let instance = instance.clone();
                                move |_| save_new(instance.clone())
                            },
                            { t!("selection.server_selection_save", server = instance.name.clone()).to_string() }
                        }
                    }
                }
                match listing.read().clone() {
                    None => rsx! { p { aria_busy: "true", { t!("settings.server_asking").to_string() } } },
                    Some((listed, problems)) => rsx! {
                        for problem in problems { p { class: "muted", { problem } } }
                        if listed.is_empty() {
                            p { class: "muted", { t!("selection.server_selections_none").to_string() } }
                        }
                        table {
                            tbody {
                                for entry in listed {
                                    tr { key: "{entry.summary.uuid}",
                                        td {
                                            strong { { entry.summary.name.clone() } }
                                            br {}
                                            small { class: "muted",
                                                {
                                                    format!(
                                                        "{}{} · {} · {}",
                                                        entry.summary.date.map(|date| date.format("%d.%m.%Y").to_string()).unwrap_or_default(),
                                                        if entry.summary.date.is_some() { "" } else { "–" },
                                                        entry.summary.owner.display_name,
                                                        t!("selection.server_selection_items", count = entry.summary.item_count),
                                                    )
                                                }
                                            }
                                        }
                                        td {
                                            button {
                                                class: "smaller-buttons",
                                                onclick: {
                                                    let entry = entry.clone();
                                                    move |_| open_selection(entry.clone())
                                                },
                                                { t!("selection.server_selection_open").to_string() }
                                            }
                                            if has_items && entry.summary.role >= cantara_api::Role::Write {
                                                button {
                                                    class: "smaller-buttons secondary",
                                                    onclick: {
                                                        let entry = entry.clone();
                                                        move |_| overwrite(entry.clone())
                                                    },
                                                    { t!("selection.server_selection_overwrite").to_string() }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    },
                }
                if let Some(message) = message() {
                    p { role: "status", { message } }
                }
                button { class: "secondary smaller-buttons", onclick: move |_| open.set(false),
                    { t!("selection.server_selections_close").to_string() }
                }
            }
        }
    }
}

// ── Designs and slide divisions shared on a server ───────────────────────────

/// The designs and slide divisions an organisation shares on a server: taken
/// over into this program's own lists, or this program's own ones offered to
/// everybody there (Q9 of `docs/specs/0008-server-version.md`). A design taken
/// over is a copy; changing it here changes nobody else's.
#[component]
fn ServerDesigns(instance: Uuid, tenant: Uuid) -> Element {
    use crate::logic::settings::{PresentationDesign, SongSlideSettings};
    use cantara_api::{SaveSettings, SettingsKind};

    let mut settings = use_settings();
    let mut generation = use_signal(|| 0u32);
    let mut message: Signal<Option<String>> = use_signal(|| None);
    let entries = use_resource(move || async move {
        let _ = generation();
        match client_for(instance) {
            Some(client) => client.settings(tenant).await.map_err(|error| error.to_string()),
            None => Err(t!("settings.server_unknown").to_string()),
        }
    });

    let mut take_over = move |entry: cantara_api::SettingsEntry| {
        let taken = match entry.kind {
            SettingsKind::Design => serde_json::from_value::<PresentationDesign>(entry.body.clone())
                .map(|design| settings.write().presentation_designs.push(design)),
            SettingsKind::SlideSettings => serde_json::from_value::<SongSlideSettings>(entry.body.clone())
                .map(|division| settings.write().song_slide_settings.push(division)),
        };
        message.set(Some(match taken {
            Ok(()) => t!("settings.server_design_taken", name = entry.name.clone()).to_string(),
            Err(error) => t!("settings.server_design_unreadable", reason = error.to_string()).to_string(),
        }));
    };

    let publish = move |kind: SettingsKind, index: usize| {
        let (name, body) = match kind {
            SettingsKind::Design => {
                let Some(design) = settings.read().presentation_designs.get(index).cloned() else { return };
                (design.name.clone(), serde_json::to_value(&design))
            }
            SettingsKind::SlideSettings => {
                let Some(division) = settings.read().song_slide_settings.get(index).cloned() else { return };
                (division.name.clone(), serde_json::to_value(&division))
            }
        };
        let Ok(body) = body else { return };
        spawn(async move {
            let Some(client) = client_for(instance) else { return };
            let request = SaveSettings { name: name.clone(), body, base_version: None };
            let path = kind.as_str();
            match client.publish_settings(tenant, path, &request).await {
                Ok(_) => {
                    message.set(Some(t!("settings.server_design_published", name = name).to_string()));
                    generation += 1;
                }
                Err(error) => message.set(Some(error.to_string())),
            }
        });
    };

    let local_designs: Vec<String> = settings.read().presentation_designs.iter().map(|design| design.name.clone()).collect();
    let local_divisions: Vec<String> = settings
        .read()
        .song_slide_settings
        .iter()
        .enumerate()
        .map(|(index, division)| if division.name.is_empty() { format!("#{}", index + 1) } else { division.name.clone() })
        .collect();

    rsx! {
        details {
            summary { { t!("settings.server_designs").to_string() } }
            match entries.read().clone() {
                None => rsx! { p { aria_busy: "true" } },
                Some(Err(error)) => rsx! { p { class: "muted", { error } } },
                Some(Ok(entries)) => rsx! {
                    if entries.is_empty() {
                        p { class: "muted", { t!("settings.server_designs_none").to_string() } }
                    }
                    ul {
                        for entry in entries {
                            li { key: "{entry.lineage}",
                                {
                                    format!(
                                        "{} ({})",
                                        entry.name,
                                        match entry.kind {
                                            SettingsKind::Design => t!("settings.server_kind_design"),
                                            SettingsKind::SlideSettings => t!("settings.server_kind_slides"),
                                        }
                                    )
                                }
                                " "
                                a {
                                    href: "#",
                                    onclick: {
                                        let entry = entry.clone();
                                        move |event: Event<MouseData>| {
                                            event.prevent_default();
                                            take_over(entry.clone());
                                        }
                                    },
                                    { t!("settings.server_design_take").to_string() }
                                }
                            }
                        }
                    }
                },
            }
            label {
                { t!("settings.server_design_publish").to_string() }
                select {
                    onchange: move |event| {
                        let value = event.value();
                        if let Some((kind, index)) = value.split_once(':') {
                            let index: usize = index.parse().unwrap_or(usize::MAX);
                            match kind {
                                "design" => publish(SettingsKind::Design, index),
                                "slides" => publish(SettingsKind::SlideSettings, index),
                                _ => {}
                            }
                        }
                    },
                    option { value: "", selected: true, "—" }
                    for (index, name) in local_designs.iter().enumerate() {
                        option { value: "design:{index}", { format!("{} ({})", name, t!("settings.server_kind_design")) } }
                    }
                    for (index, name) in local_divisions.iter().enumerate() {
                        option { value: "slides:{index}", { format!("{} ({})", name, t!("settings.server_kind_slides")) } }
                    }
                }
            }
            if let Some(message) = message() {
                p { role: "status", { message } }
            }
        }
    }
}

/// Who is logged in, in the web build a server serves — the counterpart of
/// the instance card the desktop shows.
#[component]
pub fn ServerAccount() -> Element {
    let settings = use_settings();
    let instance = settings.read().server_instance(crate::logic::server::SAME_ORIGIN).cloned();
    let Some(instance) = instance else { return rsx! {} };
    let logout = move |_| async move {
        if let Some(client) = client_for(crate::logic::server::SAME_ORIGIN) {
            let _ = client.logout().await;
        }
        #[cfg(target_arch = "wasm32")]
        if let Some(window) = web_sys::window() {
            let _ = window.location().set_href("/login");
        }
    };
    rsx! {
        hgroup {
            h3 { { t!("admin.account").to_string() } }
            p { { t!("settings.server_logged_in_as", user = instance.user_name.clone(), email = instance.user_email.clone()).to_string() } }
        }
        p { { format!("{} — {}", instance.name, instance.tenant_name) } }
        div { class: "grid",
            Link { class: "secondary", to: crate::Route::AdminPage {}, { t!("admin.open").to_string() } }
            button { class: "smaller-buttons secondary", onclick: logout, { t!("admin.logout").to_string() } }
        }
    }
}
