//! What the server asks GitHub: a user's public ssh keys (to check a key
//! linking to them is theirs) and their id (logins can be renamed).
//! `TYPEPPER_GITHUB` points both at a stand-in, for tests.

use std::time::Duration;

/// A GitHub login: 1-39 letters, digits or inner single hyphens.
pub fn valid_login(s: &str) -> bool {
    (1..=39).contains(&s.len())
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        && !s.starts_with('-')
        && !s.ends_with('-')
        && !s.contains("--")
}

/// `login`'s public keys as `algorithm base64` pairs.
pub async fn keys(login: &str) -> Result<Vec<String>, String> {
    let url = format!("{}/{login}.keys", web());
    let text = get(url).await?;
    Ok(text
        .lines()
        .filter_map(|l| {
            let mut w = l.split_whitespace();
            Some(format!("{} {}", w.next()?, w.next()?))
        })
        .collect())
}

/// `login`'s numeric id and its login as GitHub spells it.
pub async fn user(login: &str) -> Result<(i64, String), String> {
    let url = format!("{}/users/{login}", api());
    let user: serde_json::Value =
        serde_json::from_str(&get(url).await?).map_err(|e| e.to_string())?;
    let id = user["id"].as_i64().ok_or("no id")?;
    let login = user["login"].as_str().ok_or("no login")?.to_string();
    Ok((id, login))
}

fn web() -> String {
    std::env::var("TYPEPPER_GITHUB").unwrap_or("https://github.com".into())
}

fn api() -> String {
    std::env::var("TYPEPPER_GITHUB").unwrap_or("https://api.github.com".into())
}

async fn get(url: String) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(10)))
            .build()
            .into();
        agent
            .get(&url)
            // GitHub's API turns away requests without one
            .header("User-Agent", "typepper-server")
            .call()
            .map_err(|e| format!("github: {e}"))?
            .body_mut()
            .read_to_string()
            .map_err(|e| format!("github: {e}"))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logins_are_what_github_allows() {
        for ok in [
            "leodiegues",
            "a",
            "y0sif",
            "Vaishnav-Sabari-Girish",
            &"a".repeat(39),
        ] {
            assert!(valid_login(ok), "{ok}");
        }
        for bad in [
            "",
            "-a",
            "a-",
            "a--b",
            "a b",
            "a/b",
            "../x",
            "é",
            &"a".repeat(40),
        ] {
            assert!(!valid_login(bad), "{bad:?}");
        }
    }
}
