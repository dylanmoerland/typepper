//! The SSH side: a command in, response lines and an exit status out.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use russh::keys::{PrivateKey, PublicKey};
use russh::server::{Auth, ChannelOpenHandle, Handler, Msg, Server, Session};
use russh::{Channel, ChannelId};

pub async fn serve(key: PrivateKey, port: u16) -> std::io::Result<()> {
    let config = russh::server::Config {
        keys: vec![key],
        inactivity_timeout: Some(Duration::from_secs(30)),
        auth_rejection_time: Duration::from_secs(1),
        // "none" is every client's first try: turn it down at once
        auth_rejection_time_initial: Some(Duration::ZERO),
        ..Default::default()
    };
    Door.run_on_address(Arc::new(config), ("::", port)).await
}

#[derive(Clone)]
struct Door;

impl Server for Door {
    type Handler = Self;
    fn new_client(&mut self, _: Option<SocketAddr>) -> Self {
        Door
    }
}

impl Handler for Door {
    type Error = russh::Error;

    // turning "none" down makes clients offer their key, which is who they are
    async fn auth_none(&mut self, _: &str) -> Result<Auth, Self::Error> {
        Ok(Auth::reject())
    }

    // any key gets in; linking a key to a player comes with `v1 link`
    async fn auth_publickey(&mut self, _: &str, _: &PublicKey) -> Result<Auth, Self::Error> {
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
        let (out, status) = run(command);
        session.channel_success(channel)?;
        session.data(channel, out.into_bytes())?;
        session.exit_status_request(channel, status)?;
        session.eof(channel)?;
        session.close(channel)?;
        Ok(())
    }
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
