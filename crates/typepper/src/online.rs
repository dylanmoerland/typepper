//! The game's one door to typepper.com: replays go up, links come back.
//! Only ever on the player's say-so (pressing u, or having linked their
//! account), never at startup or while playing, and a failure is one line of
//! text. Desktop runs the system `ssh`, where a linked key is who the player
//! is; the web uses fetch, anonymously for now. See docs/online.md.

/// The server's public host key: ssh refuses anything else.
#[cfg(not(target_arch = "wasm32"))]
const HOST_KEY: &str = include_str!("../../../infra/ssh/typepper-server.pub");
/// Where the game connects over SSH (`TYPEPPER_SSH=host:port` overrides it,
/// `TYPEPPER_SSH_KNOWN_HOST` the key it expects there).
#[cfg(not(target_arch = "wasm32"))]
const SSH: &str = "ssh.typepper.com:36080";
/// The HTTP API, for the web.
#[cfg(target_arch = "wasm32")]
const API: &str = "https://api.typepper.com";

/// A replay as a one-item submission: `claimed: f64 LE | replay`.
pub fn submission(replay: &[u8], claimed: Option<f64>) -> Vec<u8> {
    let mut body = claimed.unwrap_or(f64::NAN).to_le_bytes().to_vec();
    body.extend_from_slice(replay);
    body
}

/// One line of the server's answer: the replay's link, or why not.
pub fn answer(line: &str) -> Result<String, String> {
    let words: Vec<&str> = line.trim().split(' ').collect();
    match words[..] {
        ["ok", _id, _score, _rank, link] => Ok(link.trim_start_matches("https://").to_string()),
        ["err", _code, ..] => Err(words[2..].join(" ")),
        _ => Err("typepper.com didn't answer".into()),
    }
}

/// Whether one line of an answer settles its replay: stored, or never will
/// be (too old, broken, too big). Anything else (busy, rate-limited, no line
/// at all) keeps it for the next try.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
fn settled(line: &str) -> bool {
    let done = ["ok ", "err old-engine ", "err bad-replay ", "err too-big "];
    done.iter().any(|d| line.starts_with(d))
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use std::io::{Read, Write};
    use std::process::{Child, Command, Stdio};
    use std::time::{Duration, Instant};

    use crate::store;

    /// The game's user on the server; any other is a GitHub login linking.
    const GAME_USER: &str = "typepper";
    /// Rounds a linked player's outbox keeps until they go up.
    const OUTBOX: usize = 20;

    /// The GitHub login this machine's ssh key is linked to, if it is.
    pub fn account() -> Option<String> {
        let login = store::read("account")?;
        Some(login.trim().to_string()).filter(|l| !l.is_empty())
    }

    /// An ssh upload under way.
    pub struct Upload {
        child: Child,
        started: Instant,
    }

    impl Upload {
        /// Sends `body` (a submission) as the linked player when there is
        /// one, else anonymously.
        pub fn start(body: Vec<u8>) -> Result<Upload, String> {
            // anonymous: no key offered (an agent could ask its owner to
            // approve one). Linked: keys are offered, the server takes only
            // the linked one, and a locked key goes to an askpass that isn't
            // there, so nothing ever prompts over the game.
            let pubkey = match account() {
                Some(_) => "PubkeyAuthentication=yes",
                None => "PubkeyAuthentication=no",
            };
            let mut ssh = ssh(GAME_USER, &["-o", pubkey], &["v1", "submit"])?;
            ssh.env("SSH_ASKPASS", "typepper-asks-no-passphrases")
                .env("SSH_ASKPASS_REQUIRE", "force")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null());
            let mut child = ssh.spawn().map_err(|_| "ssh isn't installed".to_string())?;
            let mut stdin = child.stdin.take().unwrap();
            // a big outbox could fill the pipe before ssh connects: write aside
            std::thread::spawn(move || {
                let _ = stdin.write_all(&body);
            });
            Ok(Upload {
                child,
                started: Instant::now(),
            })
        }

        /// The server's answer, once there is one (never waits).
        pub fn poll(&mut self) -> Option<Result<String, String>> {
            match self.child.try_wait() {
                Ok(Some(_)) => {
                    let mut out = String::new();
                    let _ = self.child.stdout.take()?.read_to_string(&mut out);
                    Some(match out.is_empty() {
                        true => Err("couldn't reach typepper.com".into()),
                        false => Ok(out),
                    })
                }
                Ok(None) if self.started.elapsed() > Duration::from_secs(20) => {
                    let _ = self.child.kill();
                    Some(Err("typepper.com took too long".into()))
                }
                Ok(None) => None,
                Err(e) => Some(Err(e.to_string())),
            }
        }
    }

    /// `typepper link <login>`: ties this machine's ssh key to a GitHub
    /// account (the server checks the key is on github.com/<login>.keys).
    /// Runs in the terminal, not the game, so the agent may ask as it likes.
    pub fn link(login: &str) -> Result<String, String> {
        let out = ssh(login, &["-o", "PubkeyAuthentication=yes"], &["v1", "link"])?
            .stderr(Stdio::inherit())
            .output()
            .map_err(|_| "ssh isn't installed".to_string())?;
        let line = String::from_utf8_lossy(&out.stdout).trim().to_string();
        match line.strip_prefix("ok linked ") {
            Some(login) => {
                store::write("account", login).map_err(|e| e.to_string())?;
                Ok(login.to_string())
            }
            None if line.is_empty() => Err(format!(
                "none of your ssh keys is on github.com/{login}.keys (add one in GitHub's settings)"
            )),
            None => Err(line),
        }
    }

    /// `typepper unlink`: this machine's key lets go of the account.
    pub fn unlink() -> Result<(), String> {
        let out = ssh(
            GAME_USER,
            &["-o", "PubkeyAuthentication=yes"],
            &["v1", "unlink"],
        )?
        .stderr(Stdio::inherit())
        .output()
        .map_err(|_| "ssh isn't installed".to_string())?;
        store::remove("account");
        match String::from_utf8_lossy(&out.stdout).starts_with("ok") {
            true => Ok(()),
            false => Err("forgot it here, but typepper.com didn't answer".into()),
        }
    }

    /// Keeps a finished round for upload (dropping the oldest past 20).
    pub fn outbox_add(replay: &[u8], claimed: Option<f64>) {
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        store::write_bytes(&format!("outbox/{ms}"), &super::submission(replay, claimed));
        let all = store::list("outbox");
        for old in &all[..all.len().saturating_sub(OUTBOX)] {
            store::remove(old);
        }
    }

    /// The outbox, oldest first: its names and, back to back, a submission.
    pub fn outbox() -> (Vec<String>, Vec<u8>) {
        let mut body = Vec::new();
        let names = store::list("outbox")
            .into_iter()
            .filter(|n| store::read_bytes(n).map(|b| body.extend(b)).is_some())
            .collect();
        (names, body)
    }

    /// Takes out what the server's answer settles: kept, or never will be.
    pub fn outbox_settle(names: &[String], answer: &str) {
        for (name, line) in names.iter().zip(answer.lines()) {
            if super::settled(line) {
                store::remove(name);
            }
        }
    }

    /// `ssh` to the server as `user` with `options`, running `command`, with
    /// no prompt about the host: its key is checked, never asked about.
    fn ssh(user: &str, options: &[&str], command: &[&str]) -> Result<Command, String> {
        let addr = std::env::var("TYPEPPER_SSH").unwrap_or(super::SSH.into());
        let (host, port) = addr.rsplit_once(':').ok_or("TYPEPPER_SSH is host:port")?;
        // the host key under a fixed alias, so any address works with it
        // (TYPEPPER_SSH_KNOWN_HOST: a local server's, while working on it)
        let key = std::env::var("TYPEPPER_SSH_KNOWN_HOST").unwrap_or(super::HOST_KEY.into());
        let key = key.split_whitespace().take(2).collect::<Vec<_>>();
        store::write("known_hosts", &format!("typepper {}\n", key.join(" ")))
            .map_err(|e| format!("can't keep the host key: {e}"))?;
        let known = store::path("known_hosts").ok_or("no data dir")?;
        let mut ssh = Command::new("ssh");
        ssh.args(["-T", "-p", port, "-l", user])
            .args(["-o", "ConnectTimeout=5", "-o", "LogLevel=ERROR"])
            .args([
                "-o",
                "StrictHostKeyChecking=yes",
                "-o",
                "HostKeyAlias=typepper",
            ])
            .arg("-o")
            .arg(format!("UserKnownHostsFile={}", known.display()))
            // no passwords; the server lets the game in without a key, and
            // without asking anything (BatchMode would refuse that)
            .args([
                "-o",
                "PasswordAuthentication=no",
                "-o",
                "NumberOfPasswordPrompts=1",
            ])
            .args(options)
            .arg(host)
            .args(command);
        Ok(ssh)
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use std::cell::RefCell;
    use std::rc::Rc;

    use ratzilla::web_sys::{self, js_sys};
    use wasm_bindgen_futures::JsFuture;

    type Slot = Rc<RefCell<Option<Result<String, String>>>>;

    /// A fetch under way.
    pub struct Upload(Slot);

    impl Upload {
        pub fn start(body: Vec<u8>) -> Result<Upload, String> {
            let slot = Slot::default();
            let done = slot.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let answer = post(body).await;
                let first = answer.as_deref().map(|t| t.lines().next().unwrap_or(""));
                if let Ok(Ok(link)) = first.map(super::answer) {
                    copy(&format!("https://{link}"));
                }
                *done.borrow_mut() = Some(answer);
            });
            Ok(Upload(slot))
        }

        pub fn poll(&mut self) -> Option<Result<String, String>> {
            self.0.borrow_mut().take()
        }
    }

    async fn post(body: Vec<u8>) -> Result<String, String> {
        let window = web_sys::window().ok_or("no window")?;
        let init = web_sys::RequestInit::new();
        init.set_method("POST");
        init.set_body(&js_sys::Uint8Array::from(&body[..]).into());
        let url = format!("{}/v1/runs", api());
        let response = JsFuture::from(window.fetch_with_str_and_init(&url, &init))
            .await
            .map_err(|_| "couldn't reach typepper.com".to_string())?;
        let response: web_sys::Response = response.into();
        let text = JsFuture::from(response.text().map_err(|_| "no answer")?)
            .await
            .map_err(|_| "no answer".to_string())?;
        text.as_string().ok_or("no answer".into())
    }

    // the web is anonymous until it gets a login: no account, no outbox
    pub fn account() -> Option<String> {
        None
    }

    pub fn outbox_add(_: &[u8], _: Option<f64>) {}

    pub fn outbox() -> (Vec<String>, Vec<u8>) {
        (Vec::new(), Vec::new())
    }

    pub fn outbox_settle(_: &[String], _: &str) {}

    /// A replay's bytes, for `?replay=<id>`.
    pub async fn fetch(id: &str) -> Result<Vec<u8>, String> {
        let window = web_sys::window().ok_or("no window")?;
        let url = format!("{}/v1/runs/{id}", api());
        let response = JsFuture::from(window.fetch_with_str(&url))
            .await
            .map_err(|_| "couldn't reach typepper.com".to_string())?;
        let response: web_sys::Response = response.into();
        if !response.ok() {
            return Err("that replay is gone (they keep for a week)".into());
        }
        let buffer = JsFuture::from(response.array_buffer().map_err(|_| "no body")?)
            .await
            .map_err(|_| "no body".to_string())?;
        Ok(js_sys::Uint8Array::new(&buffer).to_vec())
    }

    /// The API, or `?api=` (for a local server while working on it).
    fn api() -> String {
        let query = web_sys::window()
            .and_then(|w| w.location().search().ok())
            .unwrap_or_default();
        query
            .trim_start_matches('?')
            .split('&')
            .find_map(|kv| kv.strip_prefix("api="))
            .map_or(super::API.into(), |a| a.to_string())
    }

    /// The link on the clipboard: the canvas has no text to select.
    fn copy(text: &str) {
        if let Some(w) = web_sys::window() {
            let _ = w.navigator().clipboard().write_text(text);
        }
    }
}

pub use imp::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_final_answers_empty_the_outbox() {
        for done in [
            "ok x 1 - r",
            "err old-engine update",
            "err bad-replay truncated",
            "err too-big big",
        ] {
            assert!(settled(done), "{done}");
        }
        for keep in [
            "err busy try later",
            "err rate-limited later",
            "err too-many only 20",
            "",
            "ok",
        ] {
            assert!(!settled(keep), "{keep:?}");
        }
    }

    #[test]
    fn answers_become_links_or_reasons() {
        let ok = "ok 7Xf9qyuC 108 - https://r.typepper.com/7Xf9qyuC";
        assert_eq!(answer(ok), Ok("r.typepper.com/7Xf9qyuC".into()));
        let err = "err old-engine update typepper to submit";
        assert_eq!(answer(err), Err("update typepper to submit".into()));
        assert!(answer("").is_err());
        assert!(answer("<html>").is_err());
        let body = submission(&[1, 2], Some(108.0));
        assert_eq!(
            (&body[..8], &body[8..]),
            (&108f64.to_le_bytes()[..], &[1u8, 2][..])
        );
    }
}
