//! The SSH side: a command in (and its stdin), response lines and an exit
//! status out. The game runs the system `ssh`, so nothing here prompts.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use russh::keys::{HashAlg, PrivateKey, PublicKey};
use russh::server::{Auth, ChannelOpenHandle, Handler, Msg, Response, Server, Session};
use russh::{Channel, ChannelId};
use sqlx::PgPool;

use crate::runs::{self, Quota};

pub async fn serve(
    key: PrivateKey,
    db: PgPool,
    quota: Arc<Quota>,
    port: u16,
) -> std::io::Result<()> {
    let config = russh::server::Config {
        keys: vec![key],
        inactivity_timeout: Some(Duration::from_secs(30)),
        auth_rejection_time: Duration::from_secs(1),
        // "none" is every client's first try: turn it down at once
        auth_rejection_time_initial: Some(Duration::ZERO),
        ..Default::default()
    };
    let mut door = Door {
        db,
        quota,
        who: String::new(),
        uploads: HashMap::new(),
    };
    door.run_on_address(Arc::new(config), ("::", port)).await
}

/// One connection.
#[derive(Clone)]
struct Door {
    db: PgPool,
    quota: Arc<Quota>,
    /// Whose quota this is: their key's fingerprint, else where they came from.
    who: String,
    /// `v1 submit` channels still reading their stdin.
    uploads: HashMap<ChannelId, Vec<u8>>,
}

impl Server for Door {
    type Handler = Self;
    fn new_client(&mut self, addr: Option<SocketAddr>) -> Self {
        Door {
            who: addr.map_or("?".into(), |a| a.ip().to_string()),
            uploads: HashMap::new(),
            ..self.clone()
        }
    }
}

impl Handler for Door {
    type Error = russh::Error;

    // turning "none" down makes clients offer their key, which is who they are
    async fn auth_none(&mut self, _: &str) -> Result<Auth, Self::Error> {
        Ok(Auth::reject())
    }

    // any key gets in; linking a key to a player comes with `v1 link`
    async fn auth_publickey(&mut self, _: &str, key: &PublicKey) -> Result<Auth, Self::Error> {
        self.who = key.fingerprint(HashAlg::Sha256).to_string();
        Ok(Auth::Accept)
    }

    // no key at all: in anyway, with nothing asked (anonymous sharing)
    async fn auth_keyboard_interactive<'a>(
        &'a mut self,
        _: &str,
        _: &str,
        _: Option<Response<'a>>,
    ) -> Result<Auth, Self::Error> {
        Ok(Auth::Accept)
    }

    async fn channel_open_session(
        &mut self,
        _: Channel<Msg>,
        reply: ChannelOpenHandle,
        _: &mut Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        command: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if command == b"v1 submit" {
            // the submission comes on stdin; it's answered at its end
            session.channel_success(channel)?;
            self.uploads.insert(channel, Vec::new());
            return Ok(());
        }
        let (out, status) = run(command);
        reply(channel, out, status, session)
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let Some(body) = self.uploads.get_mut(&channel) else {
            return Ok(());
        };
        if body.len() + data.len() > runs::MAX_BODY {
            self.uploads.remove(&channel);
            let out = "err too-big that's more than a submission\n".into();
            return reply(channel, out, 1, session);
        }
        body.extend_from_slice(data);
        Ok(())
    }

    async fn channel_eof(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let Some(body) = self.uploads.remove(&channel) else {
            return Ok(());
        };
        if !self.quota.take(&self.who) {
            let out = "err rate-limited try again in an hour\n".into();
            return reply(channel, out, 1, session);
        }
        // anonymous until keys can be linked to players
        let out = runs::submit(&self.db, None, &body).await;
        reply(channel, out, 0, session)
    }

    // a plain `ssh ssh.typepper.com`: no shell here, say so instead of hanging
    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let out = "typepper: no shell here; the game runs its commands (try `ping`)\r\n";
        reply(channel, out.into(), 1, session)
    }
}

/// Answer a command: its output, its exit status, then hang up.
fn reply(
    channel: ChannelId,
    out: String,
    status: u32,
    session: &mut Session,
) -> Result<(), russh::Error> {
    session.channel_success(channel)?;
    session.data(channel, out.into_bytes())?;
    session.exit_status_request(channel, status)?;
    session.eof(channel)?;
    session.close(channel)
}

/// One command → what to print, and the exit status.
fn run(command: &[u8]) -> (String, u32) {
    match command {
        b"ping" => ("pong\n".into(), 0),
        _ => {
            let c = String::from_utf8_lossy(command);
            (format!("err unknown-command {c}\n"), 1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_pongs_and_the_rest_is_an_error() {
        assert_eq!(run(b"ping"), ("pong\n".into(), 0));
        assert_eq!(run(b"v9 nope"), ("err unknown-command v9 nope\n".into(), 1));
    }
}
