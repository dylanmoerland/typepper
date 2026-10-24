//! The game's one door to typepper.com: a replay goes up, a link comes back.
//! Only ever when the player asks, on the results screen; never at startup
//! or while playing, and a failure is one line of text. Desktop runs the
//! system `ssh` (once players can link, their key will be who they are); the web
//! uses fetch. Sharing is anonymous for now. See docs/online.md, "Game client".

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

/// The answer to a one-replay submission: its link, or why not.
pub fn answer(text: &str) -> Result<String, String> {
    let line = text.lines().next().unwrap_or("");
    let words: Vec<&str> = line.split(' ').collect();
    match words[..] {
        ["ok", _id, _score, _rank, link] => Ok(link.trim_start_matches("https://").to_string()),
        ["err", _code, ..] => Err(words[2..].join(" ")),
        _ => Err("typepper.com didn't answer".into()),
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use std::io::{Read, Write};
    use std::process::{Child, Command, Stdio};
    use std::time::{Duration, Instant};

    use crate::store;

    /// An ssh upload under way.
    pub struct Upload {
        child: Child,
        started: Instant,
    }

    impl Upload {
        pub fn start(body: Vec<u8>) -> Result<Upload, String> {
            let addr = std::env::var("TYPEPPER_SSH").unwrap_or(super::SSH.into());
            let (host, port) = addr.rsplit_once(':').ok_or("TYPEPPER_SSH is host:port")?;
            // the host key under a fixed alias, so any address works with it
            // (TYPEPPER_SSH_KNOWN_HOST: a local server's, while working on it)
            let key = std::env::var("TYPEPPER_SSH_KNOWN_HOST").unwrap_or(super::HOST_KEY.into());
            let key = key.split_whitespace().take(2).collect::<Vec<_>>();
            store::write("known_hosts", &format!("typepper {}\n", key.join(" ")))
                .map_err(|e| format!("can't keep the host key: {e}"))?;
            let known = store::path("known_hosts").ok_or("no data dir")?;
            let mut child = Command::new("ssh")
                .args(["-T", "-p", port])
                .args(["-o", "ConnectTimeout=5", "-o", "LogLevel=ERROR"])
                // never a prompt: the host key is checked, not asked about;
                // sharing is anonymous, so no key is offered (an agent could
                // ask its owner to approve one) and no password: the server
                // lets that in without a question. BatchMode would refuse it.
                .args(["-o", "StrictHostKeyChecking=yes"])
                .args(["-o", "HostKeyAlias=typepper"])
                .arg("-o")
                .arg(format!("UserKnownHostsFile={}", known.display()))
                .args(["-o", "PubkeyAuthentication=no"])
                .args(["-o", "PasswordAuthentication=no"])
                .args(["-o", "NumberOfPasswordPrompts=1"])
                .args([host, "v1", "submit"])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|_| "ssh isn't installed".to_string())?;
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

        /// The answer, once there is one (never waits).
        pub fn poll(&mut self) -> Option<Result<String, String>> {
            match self.child.try_wait() {
                Ok(Some(_)) => {
                    let mut out = String::new();
                    let _ = self.child.stdout.take()?.read_to_string(&mut out);
                    Some(match out.is_empty() {
                        true => Err("couldn't reach typepper.com".into()),
                        false => super::answer(&out),
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
                let answer = post(body).await.and_then(|text| super::answer(&text));
                if let Ok(link) = &answer {
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
    fn answers_become_links_or_reasons() {
        let ok = "ok 7Xf9qyuC 108 - https://r.typepper.com/7Xf9qyuC\n";
        assert_eq!(answer(ok), Ok("r.typepper.com/7Xf9qyuC".into()));
        let err = "err old-engine update typepper to submit\n";
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
