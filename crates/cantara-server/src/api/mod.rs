//! The HTTP API under `/api/v1`, and the OpenAPI document that describes it.
//!
//! The document is generated from the handlers' annotations and the types of
//! `cantara-api`, so it cannot say something the server does not do.

use axum::Json;
use axum::routing::get;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::state::AppState;

pub mod account;
pub mod grants;
pub mod libraries;
pub mod oidc_admin;
pub mod selections;
pub mod sessions;
pub mod settings;
pub mod tenants;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Cantara server",
        description = "Libraries, designs, running orders and shared presentations for Cantara. \
                       See docs/specs/0008-server-version.md in the Cantara repository.",
        license(name = "AGPL-3.0-or-later"),
    ),
    servers((url = "/api/v1")),
    tags(
        (name = "account", description = "Logging in, the account, and tokens for programs"),
        (name = "tenants", description = "Organisations and their members"),
        (name = "groups", description = "Groups within a tenant"),
        (name = "invitations", description = "Inviting people into a tenant or a group"),
        (name = "grants", description = "Sharing resources with members and groups"),
        (name = "libraries", description = "Libraries and their index"),
        (name = "items", description = "Files in a library, and their versions"),
        (name = "settings", description = "Designs and slide divisions"),
        (name = "selections", description = "Saved running orders"),
        (name = "sessions", description = "Presentations running on the server"),
    ),
    modifiers(&Security),
)]
struct ApiDoc;

struct Security;

impl utoipa::Modify for Security {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        use utoipa::openapi::security::{ApiKey, ApiKeyValue, HttpAuthScheme, HttpBuilder, SecurityScheme};
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "token",
            SecurityScheme::Http(HttpBuilder::new().scheme(HttpAuthScheme::Bearer).build()),
        );
        components.add_security_scheme(
            "cookie",
            SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::new(cantara_api::SESSION_COOKIE))),
        );
        openapi.security = Some(vec![
            utoipa::openapi::security::SecurityRequirement::new("token", Vec::<String>::new()),
            utoipa::openapi::security::SecurityRequirement::new("cookie", Vec::<String>::new()),
        ]);
    }
}

fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(account::info))
        .routes(routes!(account::login))
        .routes(routes!(account::register))
        .routes(routes!(account::logout))
        .routes(routes!(account::token))
        .routes(routes!(crate::device::authorize))
        .routes(routes!(crate::device::token))
        .routes(routes!(account::me, account::update_me, account::delete_me))
        .routes(routes!(account::export_me))
        .routes(routes!(account::tokens))
        .routes(routes!(account::revoke_token))
        .routes(routes!(tenants::list, tenants::create))
        .routes(routes!(tenants::update))
        .routes(routes!(tenants::members))
        .routes(routes!(tenants::update_member, tenants::remove_member))
        .routes(routes!(tenants::groups, tenants::create_group))
        .routes(routes!(tenants::update_group, tenants::delete_group))
        .routes(routes!(tenants::group_members, tenants::save_group_member))
        .routes(routes!(tenants::remove_group_member))
        .routes(routes!(tenants::invitations, tenants::invite))
        .routes(routes!(tenants::withdraw))
        .routes(routes!(oidc_admin::list, oidc_admin::create))
        .routes(routes!(oidc_admin::update, oidc_admin::delete))
        .routes(routes!(tenants::preview))
        .routes(routes!(tenants::accept))
        .routes(routes!(grants::list, grants::create))
        .routes(routes!(grants::delete))
        .routes(routes!(libraries::list, libraries::create))
        .routes(routes!(libraries::get, libraries::update, libraries::delete))
        .routes(routes!(libraries::index))
        .routes(routes!(libraries::upload))
        .routes(routes!(libraries::item, libraries::replace, libraries::delete_item))
        .routes(routes!(libraries::versions))
        .routes(routes!(libraries::restore))
        .routes(routes!(libraries::content))
        .routes(routes!(libraries::import))
        .routes(routes!(settings::list))
        .routes(routes!(settings::create))
        .routes(routes!(settings::save, settings::delete))
        .routes(routes!(settings::put_package, settings::get_package))
        .routes(routes!(selections::list, selections::create))
        .routes(routes!(selections::get, selections::save, selections::delete))
        .routes(routes!(sessions::list, sessions::create))
        .routes(routes!(sessions::get, sessions::end))
        .routes(routes!(sessions::renew_links))
        .routes(routes!(sessions::link_info))
        .routes(routes!(sessions::socket))
        .routes(routes!(sessions::put_media, sessions::get_media))
}

/// The OpenAPI document, as the server publishes it.
pub fn openapi() -> utoipa::openapi::OpenApi {
    router().split_for_parts().1
}

/// The API, ready to be nested under `/api/v1`.
pub fn routes() -> axum::Router<AppState> {
    let (router, document) = router().split_for_parts();
    router
        .route("/openapi.json", get(move || async move { Json(document) }))
        .route("/docs", get(docs))
}

/// A readable reference, drawn by Scalar from the document above.
async fn docs() -> axum::response::Html<&'static str> {
    axum::response::Html(
        r#"<!doctype html>
<html><head><meta charset="utf-8"><title>Cantara API</title>
<meta name="viewport" content="width=device-width, initial-scale=1"></head>
<body><script id="api-reference" data-url="/api/v1/openapi.json"></script>
<script src="https://cdn.jsdelivr.net/npm/@scalar/api-reference@1.25.0"></script></body></html>"#,
    )
}
