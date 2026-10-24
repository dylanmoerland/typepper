//! Players and their ssh keys: a key linked to a GitHub account is who the
//! terminal game is.

use sqlx::{PgPool, Row};

use crate::id::{Id, with_new_id};

/// The player a key belongs to, if it's linked: `(id, login)`.
pub async fn player_of_key(db: &PgPool, fingerprint: &str) -> sqlx::Result<Option<(i64, String)>> {
    let row = sqlx::query(
        "select p.id, p.login from ssh_keys k join players p on p.id = k.player_id
         where k.fingerprint = $1",
    )
    .bind(fingerprint)
    .fetch_optional(db)
    .await?;
    Ok(row.map(|r| (r.get("id"), r.get("login"))))
}

/// Ties `fingerprint` to the GitHub account `github_id` (making its player,
/// or refreshing the login, as needed); returns the player.
pub async fn link(
    db: &PgPool,
    github_id: i64,
    login: &str,
    fingerprint: &str,
) -> sqlx::Result<i64> {
    let player: i64 = with_new_id(|id| {
        sqlx::query_scalar(
            "insert into players (id, github_id, login) values ($1, $2, $3)
             on conflict (github_id) do update set login = excluded.login
             returning id",
        )
        .bind(id.0)
        .bind(github_id)
        .bind(login)
        .fetch_one(db)
    })
    .await?;
    with_new_id(|id: Id| {
        sqlx::query(
            "insert into ssh_keys (id, fingerprint, player_id) values ($1, $2, $3)
             on conflict (fingerprint) do update set player_id = excluded.player_id",
        )
        .bind(id.0)
        .bind(fingerprint)
        .bind(player)
        .execute(db)
    })
    .await?;
    Ok(player)
}

/// Lets go of a key; true if it was linked.
pub async fn unlink(db: &PgPool, fingerprint: &str) -> sqlx::Result<bool> {
    let done = sqlx::query("delete from ssh_keys where fingerprint = $1")
        .bind(fingerprint)
        .execute(db)
        .await?;
    Ok(done.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runs::tests::test_db;

    #[tokio::test]
    async fn a_key_links_relinks_and_lets_go() {
        let Some(db) = test_db().await else {
            return eprintln!("skipped: no TYPEPPER_TEST_DATABASE_URL");
        };
        let fpr = format!("SHA256:test-{}", Id::random());
        let gh = Id::random().0;
        assert_eq!(player_of_key(&db, &fpr).await.unwrap(), None);
        let p = link(&db, gh, "old-name", &fpr).await.unwrap();
        assert_eq!(
            player_of_key(&db, &fpr).await.unwrap(),
            Some((p, "old-name".into()))
        );
        // renamed on GitHub: same player, new login
        assert_eq!(link(&db, gh, "new-name", &fpr).await.unwrap(), p);
        assert_eq!(
            player_of_key(&db, &fpr).await.unwrap(),
            Some((p, "new-name".into()))
        );
        assert!(unlink(&db, &fpr).await.unwrap());
        assert!(!unlink(&db, &fpr).await.unwrap());
        assert_eq!(player_of_key(&db, &fpr).await.unwrap(), None);
    }
}
