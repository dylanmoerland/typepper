//! Ids: random bigints under 62⁸, written as 8 base62 characters
//! (`r.typepper.com/x7Kp2QaZ`). Not sequential, so links can't be counted
//! through; every insert draws one with `with_new_id`.

use std::fmt;
use std::str::FromStr;

const ALPHABET: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const LEN: usize = 8;
const SPACE: u64 = 62u64.pow(LEN as u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Id(pub i64);

impl Id {
    pub fn random() -> Id {
        // ponytail: modulo bias of 62⁸/2⁶⁴, nothing anyone could exploit
        Id((rand::random::<u64>() % SPACE) as i64)
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut n = self.0 as u64;
        let mut out = [b'0'; LEN];
        for c in out.iter_mut().rev() {
            *c = ALPHABET[(n % 62) as usize];
            n /= 62;
        }
        f.write_str(std::str::from_utf8(&out).unwrap())
    }
}

impl FromStr for Id {
    type Err = ();

    fn from_str(s: &str) -> Result<Id, ()> {
        if s.len() != LEN {
            return Err(());
        }
        s.bytes()
            .try_fold(0i64, |n, c| {
                let d = ALPHABET.iter().position(|&a| a == c).ok_or(())?;
                Ok(n * 62 + d as i64)
            })
            .map(Id)
    }
}

/// Runs `insert` with a fresh id until one isn't taken (only a collision on
/// a primary key retries; any other error is real).
pub async fn with_new_id<T, F, Fut>(mut insert: F) -> sqlx::Result<T>
where
    F: FnMut(Id) -> Fut,
    Fut: Future<Output = sqlx::Result<T>>,
{
    loop {
        match insert(Id::random()).await {
            Err(e) if collided(&e) => continue,
            r => return r,
        }
    }
}

fn collided(e: &sqlx::Error) -> bool {
    e.as_database_error()
        .and_then(|d| d.constraint())
        .is_some_and(|c| c.ends_with("_pkey"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_taken_id_draws_another() {
        let Some(db) = crate::runs::tests::test_db().await else {
            return eprintln!("skipped: no TYPEPPER_TEST_DATABASE_URL");
        };
        let insert = |id: Id| {
            sqlx::query(
                "insert into runs (id, replay, engine, mode, wpm, accuracy)
                 values ($1, '', 1, 'zen', 0, 100)",
            )
            .bind(id.0)
            .execute(&db)
        };
        let taken = Id::random();
        insert(taken).await.unwrap();
        let mut tries = 0;
        let got = with_new_id(|id| {
            tries += 1;
            let id = if tries == 1 { taken } else { id };
            async move { insert(id).await.map(|_| id) }
        })
        .await
        .unwrap();
        assert_ne!(got, taken);
        assert_eq!(tries, 2, "one collision, one retry");
    }

    #[test]
    fn base62_round_trips_at_the_edges() {
        for n in [0, 1, 61, 62, SPACE as i64 - 1, 123_456_789_012] {
            let s = Id(n).to_string();
            assert_eq!(s.len(), 8);
            assert_eq!(s.parse(), Ok(Id(n)), "{s}");
        }
        assert_eq!(Id(0).to_string(), "00000000");
        assert_eq!(Id(SPACE as i64 - 1).to_string(), "zzzzzzzz");
        for junk in ["", "1234567", "123456789", "abc-defg", "ééééé"] {
            assert_eq!(junk.parse::<Id>(), Err(()), "{junk:?}");
        }
        assert!((0..1000).all(|_| (0..SPACE as i64).contains(&Id::random().0)));
    }
}
