//! typepper's server: the HTTP API and the SSH door, over one Postgres.
//! See docs/online.md.

mod http;
mod ssh;

use std::env;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let var = |k: &str| env::var(k).map_err(|_| format!("{k} is not set"));
    let port = |k: &str, default| {
        env::var(k)
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(default)
    };
    // lazy: the server boots without Postgres, and /health says (quickly)
    // whether it answers
    let db = sqlx::postgres::PgPoolOptions::new()
        .acquire_timeout(Duration::from_secs(3))
        .connect_lazy(&var("DATABASE_URL")?)?;
    // a fixed key from the environment: a new one per deploy would make every
    // player's ssh refuse the server (the game ships the public half)
    let host_key = russh::keys::PrivateKey::from_openssh(var("SSH_HOST_KEY")?)?;
    tokio::try_join!(
        http::serve(db, port("PORT", 8080)),
        ssh::serve(host_key, port("SSH_PORT", 2222)),
    )?;
    Ok(())
}
