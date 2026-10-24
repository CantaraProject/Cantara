//! Administering an organisation on a Cantara server: its members, groups,
//! invitations and libraries, and who may use which library.
//!
//! In the same program rather than a second front end (S7 of
//! `docs/specs/0008-server-version.md`), and through the same API every
//! client uses. What a user may not do the server refuses; this page only
//! leaves out what would be refused anyway.

use cantara_api::{
    CreateGrant, CreateInvitation, GrantInfo, Grantee, GroupInfo, GroupMemberInfo, GroupRole, LibraryInfo,
    MemberInfo, Role, SaveGroupMember, SaveLibrary,
};
use dioxus::prelude::*;
use rust_i18n::t;
use uuid::Uuid;

use super::dialogs::{confirm_box, prompt_box};
use crate::logic::server::{client_for, Client};
use crate::logic::settings::use_settings;
use crate::Route;

rust_i18n::i18n!("locales", fallback = "en");

/// Which server, and which organisation on it, the page administers.
#[derive(Clone, Copy, PartialEq)]
struct Target {
    instance: Uuid,
    tenant: Uuid,
}

impl Target {
    fn client(&self) -> Option<Client> {
        client_for(self.instance)
    }
}

/// Everything shown at once, fetched together and again after every change.
#[derive(Clone, PartialEq, Default)]
struct Overview {
    members: Vec<MemberInfo>,
    groups: Vec<GroupInfo>,
    invitations: Vec<cantara_api::InvitationInfo>,
    libraries: Vec<LibraryInfo>,
}

async fn overview(target: Target) -> Result<Overview, String> {
    let client = target.client().ok_or_else(|| t!("settings.server_unknown").to_string())?;
    let error = |error: crate::logic::server::ClientError| error.to_string();
    Ok(Overview {
        members: client.members(target.tenant).await.map_err(error)?,
        groups: client.groups(target.tenant).await.map_err(error)?,
        invitations: client.invitations(target.tenant).await.unwrap_or_default(),
        libraries: client.libraries(target.tenant).await.map_err(error)?,
    })
}

#[component]
pub fn AdminPage() -> Element {
    let settings = use_settings();
    let nav = use_navigator();
    let instances: Vec<_> = settings
        .read()
        .server_instances
        .iter()
        .filter(|instance| instance.tenant.is_some())
        .cloned()
        .collect();
    let mut chosen: Signal<Option<Uuid>> = use_signal(|| None);
    let instance = chosen()
        .and_then(|uuid| instances.iter().find(|instance| instance.uuid == uuid).cloned())
        .or_else(|| instances.first().cloned());

    rsx! {
        main { class: "container admin-page",
            nav {
                ul {
                    li {
                        button {
                            class: "secondary smaller-buttons",
                            onclick: move |_| { nav.push(Route::Selection {}); },
                            { t!("admin.back").to_string() }
                        }
                    }
                }
                ul { li { strong { { t!("admin.title").to_string() } } } }
            }
            if instances.len() > 1 {
                select {
                    onchange: move |event| chosen.set(Uuid::parse_str(&event.value()).ok()),
                    for option in instances.clone() {
                        option { value: "{option.uuid}", { format!("{} — {}", option.name, option.tenant_name) } }
                    }
                }
            }
            match instance {
                None => rsx! { p { { t!("admin.no_server").to_string() } } },
                Some(instance) => rsx! {
                    AdminContent {
                        key: "{instance.uuid}",
                        target: Target { instance: instance.uuid, tenant: instance.tenant.unwrap_or_default() },
                        name: format!("{} — {}", instance.name, instance.tenant_name),
                    }
                },
            }
        }
    }
}

#[component]
fn AdminContent(target: Target, name: String) -> Element {
    let mut generation = use_signal(|| 0u32);
    let mut message: Signal<Option<String>> = use_signal(|| None);
    let data = use_resource(move || async move {
        let _ = generation();
        overview(target).await
    });

    // Runs a change, says how it went, and fetches everything again.
    let act = move |change: std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<String>, String>>>>| {
        spawn(async move {
            match change.await {
                Ok(note) => message.set(note),
                Err(error) => message.set(Some(error)),
            }
            generation += 1;
        });
    };

    let add_library = move |_| {
        act(Box::pin(async move {
            let Some(name) = prompt_box(t!("admin.library_name").to_string(), String::new()).await else {
                return Ok(None);
            };
            let client = target.client().ok_or_else(|| t!("settings.server_unknown").to_string())?;
            client
                .create_library(target.tenant, &SaveLibrary { name, description: String::new() })
                .await
                .map_err(|error| error.to_string())?;
            Ok(None)
        }));
    };

    let add_group = move |_| {
        act(Box::pin(async move {
            let Some(name) = prompt_box(t!("admin.group_name").to_string(), String::new()).await else {
                return Ok(None);
            };
            let client = target.client().ok_or_else(|| t!("settings.server_unknown").to_string())?;
            client.create_group(target.tenant, &name).await.map_err(|error| error.to_string())?;
            Ok(None)
        }));
    };

    let mut invite_email = use_signal(String::new);
    let mut invite_group: Signal<Option<Uuid>> = use_signal(|| None);
    let mut invite_manager = use_signal(|| false);
    let mut invitation_link: Signal<Option<String>> = use_signal(|| None);
    let mut invitation_mailed = use_signal(|| false);
    let invite = move |event: Event<FormData>| {
        event.prevent_default();
        let email = invite_email();
        let request = CreateInvitation {
            email,
            group: invite_group(),
            role: if invite_manager() { GroupRole::Manager } else { GroupRole::Member },
        };
        spawn(async move {
            let Some(client) = target.client() else { return };
            match client.invite(target.tenant, &request).await {
                Ok(created) => {
                    invitation_mailed.set(created.mailed);
                    invitation_link.set(created.link);
                    invite_email.set(String::new());
                    message.set(None);
                }
                Err(error) => message.set(Some(error.to_string())),
            }
            generation += 1;
        });
    };

    let overview = data.read().clone();
    rsx! {
        h3 { { name } }
        if let Some(message) = message() {
            p { role: "status", class: "admin-message", { message } }
        }
        match overview {
            None => rsx! { p { aria_busy: "true", { t!("settings.server_asking").to_string() } } },
            Some(Err(error)) => rsx! { p { { error } } },
            Some(Ok(overview)) => rsx! {
                // ── Libraries ───────────────────────────────────────────
                section {
                    h4 { { t!("admin.libraries").to_string() } }
                    for library in overview.libraries.clone() {
                        LibraryAdmin {
                            key: "{library.uuid}",
                            target,
                            library: library.clone(),
                            members: overview.members.clone(),
                            groups: overview.groups.clone(),
                            generation,
                            message,
                        }
                    }
                    button { class: "smaller-buttons", onclick: add_library, { t!("admin.library_add").to_string() } }
                }
                hr {}
                // ── Members ─────────────────────────────────────────────
                section {
                    h4 { { t!("admin.members").to_string() } }
                    table {
                        thead { tr {
                            th { { t!("admin.member_name").to_string() } }
                            th { { t!("admin.member_email").to_string() } }
                            th { { t!("admin.member_admin").to_string() } }
                            th {}
                        } }
                        tbody {
                            for member in overview.members.clone() {
                                tr { key: "{member.user.uuid}",
                                    td { { member.user.display_name.clone() } }
                                    td { { member.user.email.clone() } }
                                    td {
                                        input {
                                            r#type: "checkbox",
                                            role: "switch",
                                            checked: member.is_admin,
                                            onchange: {
                                                let user = member.user.uuid;
                                                move |event: Event<FormData>| {
                                                    let wanted = event.checked();
                                                    act(Box::pin(async move {
                                                        let client = target.client().ok_or_else(|| t!("settings.server_unknown").to_string())?;
                                                        client.set_admin(target.tenant, user, wanted).await.map_err(|error| error.to_string())?;
                                                        Ok(None)
                                                    }));
                                                }
                                            },
                                        }
                                    }
                                    td {
                                        a {
                                            href: "#",
                                            onclick: {
                                                let user = member.user.uuid;
                                                let name = member.user.display_name.clone();
                                                move |event: Event<MouseData>| {
                                                    event.prevent_default();
                                                    let name = name.clone();
                                                    act(Box::pin(async move {
                                                        if !confirm_box(t!("admin.member_remove_confirm", name = name).to_string()).await {
                                                            return Ok(None);
                                                        }
                                                        let client = target.client().ok_or_else(|| t!("settings.server_unknown").to_string())?;
                                                        client.remove_member(target.tenant, user).await.map_err(|error| error.to_string())?;
                                                        Ok(None)
                                                    }));
                                                }
                                            },
                                            { t!("admin.remove").to_string() }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                hr {}
                // ── Invitations ─────────────────────────────────────────
                section {
                    h4 { { t!("admin.invitations").to_string() } }
                    form { onsubmit: invite,
                        fieldset { role: "group",
                            input {
                                r#type: "email",
                                required: true,
                                placeholder: t!("admin.member_email").to_string(),
                                value: invite_email(),
                                oninput: move |event| invite_email.set(event.value()),
                            }
                            select {
                                onchange: move |event| invite_group.set(Uuid::parse_str(&event.value()).ok()),
                                option { value: "", { t!("admin.invite_no_group").to_string() } }
                                for group in overview.groups.iter().filter(|group| !group.system).cloned() {
                                    option { value: "{group.uuid}", { group.name.clone() } }
                                }
                            }
                            button { r#type: "submit", { t!("admin.invite").to_string() } }
                        }
                        if invite_group().is_some() {
                            label {
                                input {
                                    r#type: "checkbox",
                                    checked: invite_manager(),
                                    onchange: move |event| invite_manager.set(event.checked()),
                                }
                                { t!("admin.invite_as_manager").to_string() }
                            }
                        }
                    }
                    if let Some(link) = invitation_link() {
                        p {
                            if invitation_mailed() {
                                { t!("admin.invitation_mailed").to_string() }
                                br {}
                            }
                            { t!("admin.invitation_link").to_string() }
                            br {}
                            code { { link } }
                        }
                    }
                    ul {
                        for invitation in overview.invitations.clone() {
                            li { key: "{invitation.uuid}",
                                {
                                    format!(
                                        "{}{} — {} {}",
                                        invitation.email,
                                        invitation.group.as_ref().map(|group| format!(" ({})", group.name)).unwrap_or_default(),
                                        t!("admin.invitation_until"),
                                        invitation.expires_at.format("%d.%m.%Y"),
                                    )
                                }
                                " "
                                a {
                                    href: "#",
                                    onclick: {
                                        let uuid = invitation.uuid;
                                        move |event: Event<MouseData>| {
                                            event.prevent_default();
                                            act(Box::pin(async move {
                                                let client = target.client().ok_or_else(|| t!("settings.server_unknown").to_string())?;
                                                client.withdraw_invitation(target.tenant, uuid).await.map_err(|error| error.to_string())?;
                                                Ok(None)
                                            }));
                                        }
                                    },
                                    { t!("admin.withdraw").to_string() }
                                }
                            }
                        }
                    }
                }
                hr {}
                // ── Groups ──────────────────────────────────────────────
                section {
                    h4 { { t!("admin.groups").to_string() } }
                    for group in overview.groups.clone() {
                        GroupAdmin { key: "{group.uuid}", target, group: group.clone(), members: overview.members.clone(), generation, message }
                    }
                    button { class: "smaller-buttons", onclick: add_group, { t!("admin.group_add").to_string() } }
                }
                hr {}
                OidcProviders { target }
            },
        }
    }
}

#[component]
fn GroupAdmin(
    target: Target,
    group: GroupInfo,
    members: Vec<MemberInfo>,
    generation: Signal<u32>,
    message: Signal<Option<String>>,
) -> Element {
    let uuid = group.uuid;
    let mut inner = use_signal(|| 0u32);
    let people = use_resource(move || async move {
        let _ = inner();
        let _ = generation();
        match target.client() {
            Some(client) => client.group_members(target.tenant, uuid).await.unwrap_or_default(),
            None => Vec::new(),
        }
    });
    let run = move |change: std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), crate::logic::server::ClientError>>>>| {
        spawn(async move {
            if let Err(error) = change.await {
                message.set(Some(error.to_string()));
            }
            inner += 1;
        });
    };
    let in_group: Vec<GroupMemberInfo> = people.read().clone().unwrap_or_default();
    let outside: Vec<MemberInfo> = members
        .iter()
        .filter(|member| !in_group.iter().any(|inside| inside.user.uuid == member.user.uuid))
        .cloned()
        .collect();

    rsx! {
        details {
            summary {
                { format!("{} ({})", group.name, group.member_count) }
                if group.system { small { class: "muted", { format!(" — {}", t!("admin.group_admins")) } } }
            }
            ul {
                for person in in_group.clone() {
                    li { key: "{person.user.uuid}",
                        { person.user.display_name.clone() }
                        if person.role == GroupRole::Manager { small { class: "muted", { format!(" ({})", t!("admin.manager")) } } }
                        " "
                        a {
                            href: "#",
                            onclick: {
                                let user = person.user.uuid;
                                move |event: Event<MouseData>| {
                                    event.prevent_default();
                                    run(Box::pin(async move {
                                        match target.client() {
                                            Some(client) => client.remove_group_member(target.tenant, uuid, user).await,
                                            None => Ok(()),
                                        }
                                    }));
                                }
                            },
                            { t!("admin.remove").to_string() }
                        }
                    }
                }
            }
            if !outside.is_empty() {
                select {
                    onchange: {
                        move |event: Event<FormData>| {
                            let Ok(user) = Uuid::parse_str(&event.value()) else { return };
                            run(Box::pin(async move {
                                match target.client() {
                                    Some(client) => client.save_group_member(target.tenant, uuid, &SaveGroupMember { user, role: GroupRole::Member }).await,
                                    None => Ok(()),
                                }
                            }));
                        }
                    },
                    option { value: "", selected: true, { t!("admin.group_add_member").to_string() } }
                    for member in outside {
                        option { value: "{member.user.uuid}", { member.user.display_name.clone() } }
                    }
                }
            }
            if !group.system {
                a {
                    href: "#",
                    onclick: {
                        let name = group.name.clone();
                        let mut generation = generation;
                        move |event: Event<MouseData>| {
                            event.prevent_default();
                            let name = name.clone();
                            spawn(async move {
                                if !confirm_box(t!("admin.group_delete_confirm", name = name).to_string()).await {
                                    return;
                                }
                                if let Some(client) = target.client()
                                    && let Err(error) = client.delete_group(target.tenant, uuid).await
                                {
                                    message.set(Some(error.to_string()));
                                }
                                generation += 1;
                            });
                        }
                    },
                    { t!("admin.group_delete").to_string() }
                }
            }
        }
    }
}

fn role_label(role: Role) -> String {
    match role {
        Role::Read => t!("admin.role_read").to_string(),
        Role::Write => t!("admin.role_write").to_string(),
        Role::Manage => t!("admin.role_manage").to_string(),
    }
}

#[component]
fn LibraryAdmin(
    target: Target,
    library: LibraryInfo,
    members: Vec<MemberInfo>,
    groups: Vec<GroupInfo>,
    generation: Signal<u32>,
    message: Signal<Option<String>>,
) -> Element {
    let uuid = library.uuid;
    let mut inner = use_signal(|| 0u32);
    let grants = use_resource(move || async move {
        let _ = inner();
        match target.client() {
            Some(client) => client.grants(target.tenant, "library", uuid).await.unwrap_or_default(),
            None => Vec::new(),
        }
    });
    let mut uploading = use_signal(|| false);
    let mut grantee: Signal<Option<Grantee>> = use_signal(|| None);
    let mut role = use_signal(|| Role::Read);

    let upload = move |event: Event<FormData>| async move {
        let files = event.files();
        if files.is_empty() {
            return;
        }
        let Some(client) = target.client() else { return };
        uploading.set(true);
        let mut stored = 0;
        let mut problems = Vec::new();
        for file in files {
            let name = file.name();
            let import = name.to_lowercase().ends_with(".zip");
            match file.read_bytes().await {
                Ok(bytes) => match client.upload(target.tenant, uuid, &name, bytes.to_vec(), import).await {
                    Ok(_) => stored += 1,
                    Err(error) => problems.push(format!("{name}: {error}")),
                },
                Err(error) => problems.push(format!("{name}: {error}")),
            }
        }
        uploading.set(false);
        message.set(Some(if problems.is_empty() {
            t!("admin.uploaded", count = stored).to_string()
        } else {
            problems.join("; ")
        }));
        let mut generation = generation;
        generation += 1;
    };

    let add_grant = move |_| {
        let Some(grantee) = grantee() else { return };
        let request = CreateGrant { grantee, role: role() };
        spawn(async move {
            if let Some(client) = target.client()
                && let Err(error) = client.grant(target.tenant, "library", uuid, &request).await
            {
                message.set(Some(error.to_string()));
            }
            inner += 1;
        });
    };

    let listed: Vec<GrantInfo> = grants.read().clone().unwrap_or_default();
    let manage = library.role == Role::Manage;
    rsx! {
        article { class: "listed-article",
            h6 {
                { library.name.clone() }
                small { class: "muted", { format!(" — {}", t!("settings.server_library_count", count = library.item_count)) } }
            }
            if library.role >= Role::Write {
                label {
                    { t!("admin.upload").to_string() }
                    input {
                        r#type: "file",
                        multiple: true,
                        disabled: uploading(),
                        onchange: upload,
                    }
                }
                if uploading() { p { aria_busy: "true", { t!("admin.uploading").to_string() } } }
            }
            if manage {
                p { small { { t!("admin.grants").to_string() } } }
                ul {
                    for grant in listed {
                        li { key: "{grant.uuid}",
                            { format!("{} — {}", grant.grantee_name, role_label(grant.role)) }
                            " "
                            a {
                                href: "#",
                                onclick: {
                                    let grant_uuid = grant.uuid;
                                    move |event: Event<MouseData>| {
                                        event.prevent_default();
                                        spawn(async move {
                                            if let Some(client) = target.client()
                                                && let Err(error) = client.revoke_grant(target.tenant, "library", uuid, grant_uuid).await
                                            {
                                                message.set(Some(error.to_string()));
                                            }
                                            inner += 1;
                                        });
                                    }
                                },
                                { t!("admin.remove").to_string() }
                            }
                        }
                    }
                }
                fieldset { role: "group",
                    select {
                        onchange: move |event| {
                            let value = event.value();
                            grantee.set(match value.split_once(':') {
                                Some(("user", id)) => Uuid::parse_str(id).ok().map(Grantee::User),
                                Some(("group", id)) => Uuid::parse_str(id).ok().map(Grantee::Group),
                                _ => None,
                            });
                        },
                        option { value: "", { t!("admin.grant_to").to_string() } }
                        for group in groups.iter().filter(|group| !group.system).cloned() {
                            option { value: "group:{group.uuid}", { format!("{} ({})", group.name, t!("admin.group")) } }
                        }
                        for member in members.clone() {
                            option { value: "user:{member.user.uuid}", { member.user.display_name.clone() } }
                        }
                    }
                    select {
                        onchange: move |event| role.set(match event.value().as_str() {
                            "write" => Role::Write,
                            "manage" => Role::Manage,
                            _ => Role::Read,
                        }),
                        option { value: "read", { role_label(Role::Read) } }
                        option { value: "write", { role_label(Role::Write) } }
                        option { value: "manage", { role_label(Role::Manage) } }
                    }
                    button { onclick: add_grant, disabled: grantee().is_none(), { t!("admin.grant").to_string() } }
                }
                a {
                    href: "#",
                    class: "secondary",
                    onclick: {
                        let name = library.name.clone();
                        let mut generation = generation;
                        move |event: Event<MouseData>| {
                            event.prevent_default();
                            let name = name.clone();
                            spawn(async move {
                                if !confirm_box(t!("admin.library_delete_confirm", name = name).to_string()).await {
                                    return;
                                }
                                if let Some(client) = target.client()
                                    && let Err(error) = client.delete_library(target.tenant, uuid).await
                                {
                                    message.set(Some(error.to_string()));
                                }
                                generation += 1;
                            });
                        }
                    },
                    { t!("admin.library_delete").to_string() }
                }
            }
        }
    }
}

/// The organisation's own OpenID Connect providers: its people log in through
/// their own directory, and — where wanted — become members by doing so.
#[component]
fn OidcProviders(target: Target) -> Element {
    use cantara_api::SaveTenantOidcProvider;

    let mut generation = use_signal(|| 0u32);
    let mut message: Signal<Option<String>> = use_signal(|| None);
    let providers = use_resource(move || async move {
        let _ = generation();
        match target.client() {
            Some(client) => client.oidc_providers(target.tenant).await.map_err(|error| error.to_string()),
            None => Err(t!("settings.server_unknown").to_string()),
        }
    });
    let mut name = use_signal(String::new);
    let mut slug = use_signal(String::new);
    let mut issuer = use_signal(String::new);
    let mut client_id = use_signal(String::new);
    let mut client_secret = use_signal(String::new);
    let mut auto_join = use_signal(|| false);

    let add = move |event: Event<FormData>| {
        event.prevent_default();
        let request = SaveTenantOidcProvider {
            slug: slug(),
            name: name(),
            issuer: issuer(),
            client_id: client_id(),
            client_secret: Some(client_secret()).filter(|secret| !secret.is_empty()),
            auto_join: auto_join(),
        };
        spawn(async move {
            let Some(client) = target.client() else { return };
            match client.create_oidc_provider(target.tenant, &request).await {
                Ok(_) => {
                    for mut field in [name, slug, issuer, client_id, client_secret] {
                        field.set(String::new());
                    }
                    auto_join.set(false);
                    message.set(None);
                }
                Err(error) => message.set(Some(error.to_string())),
            }
            generation += 1;
        });
    };

    rsx! {
        section {
            h4 { { t!("admin.oidc").to_string() } }
            p { class: "muted", { t!("admin.oidc_description").to_string() } }
            match providers.read().clone() {
                None => rsx! { p { aria_busy: "true" } },
                Some(Err(error)) => rsx! { p { { error } } },
                Some(Ok(providers)) => rsx! {
                    for provider in providers {
                        article { key: "{provider.uuid}", class: "listed-article",
                            h6 { { provider.name.clone() } }
                            small {
                                { format!("{} · {}", provider.issuer, provider.client_id) }
                                if provider.auto_join { { format!(" · {}", t!("admin.oidc_auto_join_short")) } }
                                br {}
                                { format!("{} {}", t!("admin.oidc_redirect"), provider.redirect_uri) }
                                br {}
                                { format!("{} {}", t!("admin.oidc_login_path"), provider.login_path) }
                            }
                            br {}
                            a {
                                href: "#",
                                onclick: {
                                    let uuid = provider.uuid;
                                    let provider_name = provider.name.clone();
                                    move |event: Event<MouseData>| {
                                        event.prevent_default();
                                        let provider_name = provider_name.clone();
                                        spawn(async move {
                                            if !confirm_box(t!("admin.oidc_delete_confirm", name = provider_name).to_string()).await {
                                                return;
                                            }
                                            if let Some(client) = target.client()
                                                && let Err(error) = client.delete_oidc_provider(target.tenant, uuid).await
                                            {
                                                message.set(Some(error.to_string()));
                                            }
                                            generation += 1;
                                        });
                                    }
                                },
                                { t!("admin.remove").to_string() }
                            }
                        }
                    }
                },
            }
            form { onsubmit: add,
                div { class: "grid",
                    input { placeholder: t!("admin.oidc_name").to_string(), required: true, value: name(), oninput: move |event| name.set(event.value()) }
                    input { placeholder: t!("admin.oidc_slug").to_string(), required: true, value: slug(), oninput: move |event| slug.set(event.value()) }
                }
                input { r#type: "url", placeholder: t!("admin.oidc_issuer").to_string(), required: true, value: issuer(), oninput: move |event| issuer.set(event.value()) }
                div { class: "grid",
                    input { placeholder: t!("admin.oidc_client_id").to_string(), required: true, value: client_id(), oninput: move |event| client_id.set(event.value()) }
                    input { r#type: "password", placeholder: t!("admin.oidc_client_secret").to_string(), value: client_secret(), oninput: move |event| client_secret.set(event.value()) }
                }
                label {
                    input { r#type: "checkbox", checked: auto_join(), onchange: move |event| auto_join.set(event.checked()) }
                    { t!("admin.oidc_auto_join").to_string() }
                }
                button { r#type: "submit", class: "smaller-buttons", { t!("admin.oidc_add").to_string() } }
            }
            if let Some(message) = message() {
                p { role: "status", { message } }
            }
        }
    }
}
