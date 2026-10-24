//! A server's libraries in the browser build.
//!
//! The browser has no file system, so a library goes where every other
//! repository of the web build goes: the in-memory VFS, under
//! `web-server://{library}/{relative path}`. What version each path holds is
//! remembered, so that a second look only fetches what changed.

use std::cell::RefCell;
use std::collections::HashMap;

use uuid::Uuid;

use super::{client_for, remember_item, ServerItem};
use crate::logic::settings::RepositoryType;
use crate::logic::sourcefiles::SourceFile;

thread_local! {
    /// The version each VFS path holds.
    static VERSIONS: RefCell<HashMap<String, Uuid>> = RefCell::new(HashMap::new());
}

/// Fetches a lazily listed file into the VFS.
pub async fn fetch(path: &str, lazy: &super::Lazy) -> Result<(), String> {
    let client = client_for(lazy.instance).ok_or("the server is not known")?;
    let bytes = client
        .content(lazy.tenant, lazy.version)
        .await
        .map_err(|error| error.to_string())?
        .bytes()
        .await
        .map_err(|error| error.to_string())?;
    RepositoryType::store_web_file(path, bytes.to_vec());
    VERSIONS.with(|versions| versions.borrow_mut().insert(path.to_string(), lazy.version));
    Ok(())
}

/// Whether the VFS holds `path`.
pub fn is_local(path: &str) -> bool {
    RepositoryType::web_read_file(path).is_some()
}

pub fn prefix(library: Uuid) -> String {
    format!("web-server://{library}")
}

/// The files of a server library, fetched into the VFS.
pub async fn files_of(instance: Uuid, tenant: Uuid, library: Uuid, offline: bool) -> Vec<SourceFile> {
    let Some(client) = client_for(instance) else {
        return Vec::new();
    };
    let index = match client.index(tenant, library, None).await {
        Ok(index) => index,
        Err(error) => {
            log::warn!("{}: {error}", client.base());
            return Vec::new();
        }
    };
    let prefix = prefix(library);
    let mut files = Vec::with_capacity(index.entries.len());
    for entry in &index.entries {
        let path = format!("{prefix}/{}", entry.relative_path);
        let current = VERSIONS.with(|versions| versions.borrow().get(&path) == Some(&entry.version));
        // A picture, a PDF or a video of a library that is not kept whole is
        // fetched when it is first used — see `ensure_local`.
        let lazy = !offline && !entry.kind.is_text();
        if lazy && !current {
            super::remember_lazy(
                &path,
                super::Lazy {
                    instance,
                    tenant,
                    version: entry.version,
                    sha256: entry.sha256.clone(),
                },
            );
        } else if !current {
            let bytes = match client.content(tenant, entry.version).await {
                Ok(response) => match response.bytes().await {
                    Ok(bytes) => bytes.to_vec(),
                    Err(error) => {
                        log::warn!("{}: {error}", entry.relative_path);
                        continue;
                    }
                },
                Err(error) => {
                    log::warn!("{}: {error}", entry.relative_path);
                    continue;
                }
            };
            RepositoryType::store_web_file(&path, bytes);
            VERSIONS.with(|versions| versions.borrow_mut().insert(path.clone(), entry.version));
        }
        let Some(mut file) = SourceFile::from_web_path(&path, &prefix) else {
            continue;
        };
        file.md5_hash = Some(entry.md5.clone());
        remember_item(
            &path,
            ServerItem {
                instance,
                tenant,
                library,
                lineage: entry.lineage,
                version: entry.version,
            },
        );
        files.push(file);
    }
    files
}

/// What the web build a server serves does first: find out who is logged in
/// — or send them to the server's login page — and make the libraries they
/// may use the repositories.
#[cfg(feature = "server-client")]
pub async fn connect_to_origin(mut settings: dioxus::prelude::Signal<crate::logic::settings::Settings>) {
    use dioxus::prelude::*;

    use super::{ClientError, ServerInstance, SAME_ORIGIN};

    let Some(client) = client_for(SAME_ORIGIN) else { return };
    let me = match client.me().await {
        Ok(me) => me,
        Err(ClientError::Unauthorized) => {
            if let Some(window) = web_sys::window() {
                let location = window.location();
                let here = location.pathname().unwrap_or_else(|_| "/".to_string());
                // A shared presentation decides for itself whether a login is
                // needed: a link to watch is for anybody.
                if !here.starts_with("/s/") {
                    let _ = location.set_href(&format!("/login?next={here}"));
                }
            }
            return;
        }
        Err(error) => {
            log::warn!("{error}");
            return;
        }
    };
    let info = client.info().await.ok();
    let name = info.as_ref().map(|info| info.name.clone()).unwrap_or_else(|| "Cantara".to_string());
    let sharing = info.as_ref().is_none_or(|info| info.sharing_enabled);

    let previous = settings.peek().server_instance(SAME_ORIGIN).and_then(|instance| instance.tenant);
    let membership = me
        .tenants
        .iter()
        .find(|membership| Some(membership.tenant.uuid) == previous)
        .or_else(|| me.tenants.first());
    let Some(membership) = membership.cloned() else {
        log::warn!("the account belongs to no organisation on this server");
        return;
    };
    let tenant = membership.tenant.uuid;
    let libraries = client.libraries(tenant).await.unwrap_or_default();

    let mut settings = settings.write();
    settings.wizard_completed = true;
    settings.add_server_instance(ServerInstance {
        uuid: SAME_ORIGIN,
        url: client.base().to_string(),
        name,
        tenant: Some(tenant),
        tenant_name: membership.tenant.name.clone(),
        user_email: me.user.email.clone(),
        user_name: me.user.display_name.clone(),
        sharing,
        token_fallback: None,
    });
    settings.repositories.retain(|repository| match &repository.repository_type {
        RepositoryType::Server { instance, tenant: used, library, .. } if *instance == SAME_ORIGIN => {
            *used == tenant && libraries.iter().any(|offered| offered.uuid == *library)
        }
        _ => true,
    });
    for library in &libraries {
        settings.use_server_library(SAME_ORIGIN, tenant, library, true);
    }
}
