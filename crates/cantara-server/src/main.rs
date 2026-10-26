//! `cantara-server`: reads its configuration from the environment and serves.

use cantara_server::config::Config;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,tower_http=info,sqlx=warn"));
    if std::env::var("CANTARA_LOG_FORMAT").as_deref() == Ok("json") {
        tracing_subscriber::fmt().with_env_filter(filter).json().init();
    } else {
        tracing_subscriber::fmt().with_env_filter(filter).init();
    }

    let config = match Config::from_env() {
        Ok(config) => config,
        Err(error) => {
            tracing::error!("{error}");
            std::process::exit(2);
        }
    };
    let listen = config.listen;
    let state = match cantara_server::prepare(config).await {
        Ok(state) => state,
        Err(error) => {
            tracing::error!("{error}");
            std::process::exit(1);
        }
    };

    cantara_server::gc::run_periodically(state.clone());

    let listener = match tokio::net::TcpListener::bind(listen).await {
        Ok(listener) => listener,
        Err(error) => {
            tracing::error!("cannot listen on {listen}: {error}");
            std::process::exit(1);
        }
    };
    tracing::info!("serving on {listen}");
    let shutdown = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    if let Err(error) = axum::serve(listener, cantara_server::app(state))
        .with_graceful_shutdown(shutdown)
        .await
    {
        tracing::error!("{error}");
        std::process::exit(1);
    }
}
