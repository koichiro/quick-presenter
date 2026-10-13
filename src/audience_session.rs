//! Session boundary shared by local HTTP hosting and outbound relay connections.
use crate::{
    audience::{LocalAudienceSession, SessionSnapshot},
    audience_events::AcceptedAudienceEvent,
    audience_relay::RemoteAudienceSession,
};
use std::net::Ipv4Addr;

pub enum AudienceSession {
    Local(LocalAudienceSession),
    Remote(RemoteAudienceSession),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_session_requires_a_lan_address_without_starting_network_worker() {
        let error = AudienceSession::start(None, None).err().unwrap();
        assert_eq!(error.kind(), std::io::ErrorKind::Other);
        assert_eq!(error.to_string(), "No LAN address available");
    }
}
impl AudienceSession {
    pub fn start(
        address: Option<Ipv4Addr>,
        relay_config: Option<crate::audience_relay_http::ManagedRelay>,
    ) -> std::io::Result<Self> {
        if let Some(path) = relay_config {
            RemoteAudienceSession::start_managed(path).map(Self::Remote)
        } else {
            LocalAudienceSession::start(
                address.ok_or_else(|| std::io::Error::other("No LAN address available"))?,
            )
            .map(Self::Local)
        }
    }
    pub fn snapshot(&self) -> SessionSnapshot {
        match self {
            Self::Local(s) => s.snapshot(),
            Self::Remote(s) => s.snapshot(),
        }
    }
    pub fn drain_events(&self) -> Vec<AcceptedAudienceEvent> {
        match self {
            Self::Local(s) => s.drain_events(),
            Self::Remote(s) => s.drain_events(),
        }
    }
    pub fn advance_comment_generation(&self, paused: bool) -> u64 {
        match self {
            Self::Local(s) => s.advance_comment_generation(),
            Self::Remote(s) => s.set_paused(paused),
        }
    }
    pub fn comment_log_failed(&self) -> bool {
        match self {
            Self::Local(s) => s.comment_log_failed(),
            Self::Remote(s) => s.comment_log_failed(),
        }
    }
    pub fn request_stop(&self) {
        match self {
            Self::Local(s) => s.request_stop(),
            Self::Remote(s) => s.request_stop(),
        }
    }
    pub fn is_finished(&self) -> bool {
        match self {
            Self::Local(s) => s.is_finished(),
            Self::Remote(s) => s.is_finished(),
        }
    }
    pub fn shutdown(&mut self) {
        match self {
            Self::Local(s) => s.shutdown(),
            Self::Remote(s) => s.shutdown(),
        }
    }
}
