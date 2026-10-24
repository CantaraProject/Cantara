//! The OpenID Connect providers an organisation brings itself.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use cantara_api::{SaveTenantOidcProvider, TenantOidcProvider};
use uuid::Uuid;

use crate::auth::{member, CurrentUser};
use crate::error::{AppError, AppResult};
use crate::oidc::tenant_key;
use crate::state::AppState;

type ProviderRow = (Uuid, String, String, String, String, bool, bool);

async fn tenant_slug(state: &AppState, tenant: Uuid) -> AppResult<String> {
    Ok(sqlx::query_scalar("SELECT slug FROM tenants WHERE uuid = $1")
        .bind(tenant)
        .fetch_one(&state.db)
        .await?)
}

fn info(state: &AppState, tenant_slug: &str, row: ProviderRow) -> TenantOidcProvider {
    let (uuid, slug, name, issuer, client_id, has_secret, auto_join) = row;
    TenantOidcProvider {
        uuid,
        login_path: format!("/auth/oidc/t/{tenant_slug}/{slug}"),
        redirect_uri: format!("{}/auth/oidc/callback", state.config.public_url),
        slug,
        name,
        issuer,
        client_id,
        has_secret,
        auto_join,
    }
}

fn check(request: &SaveTenantOidcProvider) -> AppResult<()> {
    let slug_ok = !request.slug.is_empty()
        && request.slug.len() <= 40
        && request.slug.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    let issuer_ok = url::Url::parse(&request.issuer).is_ok_and(|url| matches!(url.scheme(), "https" | "http"));
    if !slug_ok || !issuer_ok || request.name.trim().is_empty() || request.client_id.trim().is_empty() {
        return Err(AppError::BadRequest(
            "a provider needs a slug of lowercase letters, digits and dashes, a name, an issuer URL and a client id"
                .to_string(),
        ));
    }
    Ok(())
}

const PROVIDER_COLUMNS: &str =
    "uuid, slug, name, issuer, client_id, client_secret IS NOT NULL AND client_secret <> '', auto_join";

/// The organisation's own providers. Admins only.
#[utoipa::path(get, path = "/tenants/{tenant}/oidc-providers", tag = "tenants",
    params(("tenant" = Uuid, Path)), responses((status = 200, body = [TenantOidcProvider])))]
pub async fn list(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(tenant): Path<Uuid>,
) -> AppResult<Json<Vec<TenantOidcProvider>>> {
    member(&state, &user, tenant).await?.require_admin()?;
    let slug = tenant_slug(&state, tenant).await?;
    let rows: Vec<ProviderRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {PROVIDER_COLUMNS} FROM tenant_oidc_providers WHERE tenant_uuid = $1 ORDER BY name"
    )))
    .bind(tenant)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows.into_iter().map(|row| info(&state, &slug, row)).collect()))
}

/// Adds a provider.
#[utoipa::path(post, path = "/tenants/{tenant}/oidc-providers", tag = "tenants", request_body = SaveTenantOidcProvider,
    params(("tenant" = Uuid, Path)), responses((status = 200, body = TenantOidcProvider)))]
pub async fn create(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(tenant): Path<Uuid>,
    Json(request): Json<SaveTenantOidcProvider>,
) -> AppResult<Json<TenantOidcProvider>> {
    member(&state, &user, tenant).await?.require_admin()?;
    check(&request)?;
    let uuid = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO tenant_oidc_providers (uuid, tenant_uuid, slug, name, issuer, client_id, client_secret, auto_join)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(uuid)
    .bind(tenant)
    .bind(&request.slug)
    .bind(request.name.trim())
    .bind(request.issuer.trim())
    .bind(request.client_id.trim())
    .bind(request.client_secret.as_deref().map(str::trim).filter(|secret| !secret.is_empty()))
    .bind(request.auto_join)
    .execute(&state.db)
    .await?;
    one(&state, tenant, uuid).await
}

async fn one(state: &AppState, tenant: Uuid, uuid: Uuid) -> AppResult<Json<TenantOidcProvider>> {
    let slug = tenant_slug(state, tenant).await?;
    let row: ProviderRow = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {PROVIDER_COLUMNS} FROM tenant_oidc_providers WHERE tenant_uuid = $1 AND uuid = $2"
    )))
    .bind(tenant)
    .bind(uuid)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(info(state, &slug, row)))
}

/// Changes a provider. A secret left out stays as it is.
#[utoipa::path(put, path = "/tenants/{tenant}/oidc-providers/{provider}", tag = "tenants", request_body = SaveTenantOidcProvider,
    params(("tenant" = Uuid, Path), ("provider" = Uuid, Path)), responses((status = 200, body = TenantOidcProvider)))]
pub async fn update(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((tenant, provider)): Path<(Uuid, Uuid)>,
    Json(request): Json<SaveTenantOidcProvider>,
) -> AppResult<Json<TenantOidcProvider>> {
    member(&state, &user, tenant).await?.require_admin()?;
    check(&request)?;
    let old_slug: String = sqlx::query_scalar("SELECT slug FROM tenant_oidc_providers WHERE uuid = $1 AND tenant_uuid = $2")
        .bind(provider)
        .bind(tenant)
        .fetch_one(&state.db)
        .await?;
    sqlx::query(
        "UPDATE tenant_oidc_providers
            SET slug = $3, name = $4, issuer = $5, client_id = $6,
                client_secret = COALESCE($7, client_secret), auto_join = $8
          WHERE uuid = $1 AND tenant_uuid = $2",
    )
    .bind(provider)
    .bind(tenant)
    .bind(&request.slug)
    .bind(request.name.trim())
    .bind(request.issuer.trim())
    .bind(request.client_id.trim())
    .bind(request.client_secret.as_deref().map(str::trim).filter(|secret| !secret.is_empty()))
    .bind(request.auto_join)
    .execute(&state.db)
    .await?;
    state.oidc.forget(&tenant_key(tenant, &old_slug));
    one(&state, tenant, provider).await
}

/// Removes a provider. Accounts that logged in through it stay; they log in
/// another way from now on.
#[utoipa::path(delete, path = "/tenants/{tenant}/oidc-providers/{provider}", tag = "tenants",
    params(("tenant" = Uuid, Path), ("provider" = Uuid, Path)), responses((status = 204)))]
pub async fn delete(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((tenant, provider)): Path<(Uuid, Uuid)>,
) -> AppResult<StatusCode> {
    member(&state, &user, tenant).await?.require_admin()?;
    let slug: Option<String> = sqlx::query_scalar(
        "DELETE FROM tenant_oidc_providers WHERE uuid = $1 AND tenant_uuid = $2 RETURNING slug",
    )
    .bind(provider)
    .bind(tenant)
    .fetch_optional(&state.db)
    .await?;
    let slug = slug.ok_or(AppError::NotFound)?;
    state.oidc.forget(&tenant_key(tenant, &slug));
    Ok(StatusCode::NO_CONTENT)
}
