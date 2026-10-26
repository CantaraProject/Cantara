//! What a server does on its very first start, and what creating a tenant
//! involves.

use sqlx::PgPool;
use uuid::Uuid;

use crate::config::Config;
use crate::error::AppResult;
use crate::secrets::hash_password;

/// The name of the group whose members administer a tenant.
pub const ADMINS_GROUP: &str = "admins";

/// Creates a tenant with its `admins` group, and puts `creator` into both.
pub async fn create_tenant(db: &PgPool, name: &str, slug: &str, creator: Option<Uuid>) -> AppResult<Uuid> {
    let mut tx = db.begin().await?;
    let tenant = Uuid::now_v7();
    sqlx::query("INSERT INTO tenants (uuid, name, slug) VALUES ($1, $2, $3)")
        .bind(tenant)
        .bind(name)
        .bind(slug)
        .execute(&mut *tx)
        .await?;
    let admins = Uuid::now_v7();
    sqlx::query("INSERT INTO groups (uuid, tenant_uuid, name, system) VALUES ($1, $2, $3, true)")
        .bind(admins)
        .bind(tenant)
        .bind(ADMINS_GROUP)
        .execute(&mut *tx)
        .await?;
    if let Some(creator) = creator {
        sqlx::query("INSERT INTO memberships (tenant_uuid, user_uuid) VALUES ($1, $2)")
            .bind(tenant)
            .bind(creator)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO group_members (group_uuid, user_uuid, role) VALUES ($1, $2, 'manager')")
            .bind(admins)
            .bind(creator)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(tenant)
}

/// The tenant everybody lands in on a single-tenant server: the oldest one.
pub async fn default_tenant(db: &PgPool) -> AppResult<Option<Uuid>> {
    Ok(sqlx::query_scalar("SELECT uuid FROM tenants ORDER BY created_at, uuid LIMIT 1")
        .fetch_optional(db)
        .await?)
}

/// Makes a fresh server usable: the administrator from the environment, and on
/// a single-tenant server the one tenant. Does nothing on a server that has
/// users already, so the environment variables can stay where they are.
pub async fn run(db: &PgPool, config: &Config) -> AppResult<()> {
    let users: i64 = sqlx::query_scalar("SELECT count(*) FROM users").fetch_one(db).await?;
    if users > 0 {
        return Ok(());
    }

    let (Some(email), Some(password)) = (&config.bootstrap_admin_email, &config.bootstrap_admin_password) else {
        tracing::warn!(
            "the server has no users yet; set CANTARA_BOOTSTRAP_ADMIN_EMAIL and \
             CANTARA_BOOTSTRAP_ADMIN_PASSWORD to create the first administrator"
        );
        return Ok(());
    };

    let admin = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO users (uuid, email, display_name, password_hash, is_server_admin)
         VALUES ($1, $2, $3, $4, true)",
    )
    .bind(admin)
    .bind(email)
    .bind("Administrator")
    .bind(hash_password(password)?)
    .execute(db)
    .await?;
    tracing::info!("created the administrator {email} — change the password after logging in");

    if config.single_tenant && default_tenant(db).await?.is_none() {
        create_tenant(db, &config.default_tenant_name, "default", Some(admin)).await?;
    }
    Ok(())
}
