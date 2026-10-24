//! The HTTP side: thin handlers over the core.

use std::time::Instant;

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::http::header::CONTENT_TYPE;
use axum::response::IntoResponse;
use axum::routing::get;
use serde_json::{Value, json};
use sqlx::PgPool;

pub async fn serve(db: PgPool, port: u16) -> std::io::Result<()> {
    let app = Router::new().route("/health", get(health)).with_state(db);
    // "::" takes IPv4 too, and Railway's private network is IPv6
    let listener = tokio::net::TcpListener::bind(("::", port)).await?;
    axum::serve(listener, app).await
}

/// Ready once Postgres answers: 200 "pass", else 503 "fail" (Railway only
/// moves traffic to a deploy once it's 200).
async fn health(State(db): State<PgPool>) -> impl IntoResponse {
    let t0 = Instant::now();
    let up = sqlx::query("select 1")
        .execute(&db)
        .await
        // the reason stays in the logs; the response only says pass or fail
        .inspect_err(|e| eprintln!("health: postgres: {e}"))
        .is_ok();
    let deployment = std::env::var("RAILWAY_DEPLOYMENT_ID").unwrap_or_default();
    let (code, body) = report(up, t0.elapsed().as_millis() as u64, &deployment);
    (
        code,
        [(CONTENT_TYPE, "application/health+json")],
        body.to_string(),
    )
}

/// The response, in the IETF health check format
/// (draft-inadarei-api-health-check, `application/health+json`).
fn report(postgres_up: bool, postgres_ms: u64, deployment: &str) -> (StatusCode, Value) {
    let (code, status) = match postgres_up {
        true => (StatusCode::OK, "pass"),
        false => (StatusCode::SERVICE_UNAVAILABLE, "fail"),
    };
    let mut body = json!({
        "status": status,
        "version": "1",                         // the API: /v1
        "releaseId": env!("CARGO_PKG_VERSION"), // this server
        "serviceId": "typepper-server",
        "checks": {
            "postgres:responseTime": [{
                "componentType": "datastore",
                "status": status,
                "observedValue": postgres_ms,
                "observedUnit": "ms",
            }],
        },
    });
    if !deployment.is_empty() {
        let short = deployment.get(..8).unwrap_or(deployment);
        body["description"] = json!(format!("deployment {short}"));
    }
    (code, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passes_with_postgres_and_fails_without() {
        let (code, body) = report(true, 2, "d0aa815e-a00e-471e-917c-592e4935b63b");
        assert_eq!(code, StatusCode::OK);
        assert_eq!(body["status"], "pass");
        assert_eq!(body["releaseId"], env!("CARGO_PKG_VERSION"));
        assert_eq!(body["description"], "deployment d0aa815e");
        let check = &body["checks"]["postgres:responseTime"][0];
        assert_eq!(
            (&check["status"], &check["observedValue"]),
            (&json!("pass"), &json!(2))
        );

        let (code, body) = report(false, 3000, "");
        assert_eq!(code, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["status"], "fail");
        assert_eq!(body["checks"]["postgres:responseTime"][0]["status"], "fail");
        assert!(
            body.get("description").is_none(),
            "no deployment, no description"
        );
    }
}
