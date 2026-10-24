//! The HTTP side: thin handlers over the core.

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use sqlx::PgPool;

pub async fn serve(db: PgPool, port: u16) -> std::io::Result<()> {
    let app = Router::new().route("/health", get(health)).with_state(db);
    // "::" takes IPv4 too, and Railway's private network is IPv6
    let listener = tokio::net::TcpListener::bind(("::", port)).await?;
    axum::serve(listener, app).await
}

/// Up, and Postgres answers (Railway only switches traffic over once it's 200).
async fn health(State(db): State<PgPool>) -> StatusCode {
    match sqlx::query("select 1").execute(&db).await {
        Ok(_) => StatusCode::OK,
        Err(e) => {
            eprintln!("health: postgres: {e}");
            StatusCode::SERVICE_UNAVAILABLE
        }
    }
}
