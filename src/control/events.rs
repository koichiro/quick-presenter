//! Bounded event delivery; presentation state stays with its existing owner.
use super::protocol::{Event, EventEnvelope, Status, PROTOCOL_VERSION};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, SyncSender, TrySendError},
    Arc,
};

pub const MAX_WATCHERS: usize = 4;
pub const EVENT_BUFFER: usize = 64;
pub struct Subscriber {
    pub sender: SyncSender<EventEnvelope>,
    pub lagged: Arc<AtomicBool>,
    pub closed: Arc<AtomicBool>,
}
pub struct StreamReceiver {
    pub events: Receiver<EventEnvelope>,
    pub lagged: Arc<AtomicBool>,
    pub closed: Arc<AtomicBool>,
}
impl Drop for StreamReceiver {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::Relaxed);
    }
}
pub fn channel() -> (Subscriber, StreamReceiver) {
    let (sender, receiver) = mpsc::sync_channel(EVENT_BUFFER);
    let lagged = Arc::new(AtomicBool::new(false));
    let closed = Arc::new(AtomicBool::new(false));
    (
        Subscriber {
            sender,
            lagged: lagged.clone(),
            closed: closed.clone(),
        },
        StreamReceiver {
            events: receiver,
            lagged,
            closed,
        },
    )
}
#[derive(Default)]
pub struct EventHub {
    sequence: u64,
    subscribers: Vec<Subscriber>,
}
impl EventHub {
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
    pub fn subscribe(&mut self, subscriber: Subscriber) -> bool {
        self.subscribers
            .retain(|subscriber| !subscriber.closed.load(Ordering::Relaxed));
        if self.subscribers.len() >= MAX_WATCHERS {
            return false;
        }
        self.subscribers.push(subscriber);
        true
    }
    pub fn publish(&mut self, state: &Status, event: Event) {
        self.sequence = self
            .sequence
            .checked_add(1)
            .expect("event sequence exhausted");
        let envelope = EventEnvelope {
            protocol_version: PROTOCOL_VERSION,
            session_id: state.session_id.clone(),
            document_revision: state.document_revision,
            sequence: self.sequence,
            event,
        };
        self.subscribers.retain(|subscriber| {
            if subscriber.closed.load(Ordering::Relaxed) {
                return false;
            }
            match subscriber.sender.try_send(envelope.clone()) {
                Ok(()) => true,
                Err(TrySendError::Full(_)) => {
                    subscriber.lagged.store(true, Ordering::Relaxed);
                    false
                }
                Err(TrySendError::Disconnected(_)) => false,
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn status() -> Status {
        serde_json::from_str(r#"{"session_id":"test","document_revision":1,"document":"/slides.pdf","page":1,"pages":3,"fullscreen":false,"blackout":false,"timer":{"running":false,"elapsed_seconds":0},"opening":false,"render_state":"ready","notes_state":"ready"}"#).unwrap()
    }
    #[test]
    fn slow_subscriber_is_disconnected_without_blocking_a_healthy_subscriber() {
        let mut hub = EventHub::default();
        let (slow, slow_receiver) = channel();
        let (healthy, healthy_receiver) = channel();
        assert!(hub.subscribe(slow));
        assert!(hub.subscribe(healthy));
        for index in 0..=EVENT_BUFFER {
            hub.publish(
                &status(),
                Event::BlackoutChanged {
                    value: index % 2 == 0,
                },
            );
            assert_eq!(
                healthy_receiver.events.try_recv().unwrap().sequence,
                index as u64 + 1
            );
        }
        assert!(slow_receiver.lagged.load(Ordering::Relaxed));
        assert_eq!(slow_receiver.events.try_iter().count(), EVENT_BUFFER);
        assert!(slow_receiver.events.try_recv().is_err());
    }
    #[test]
    fn watcher_limit_reserves_capacity_and_disconnect_releases_slot_without_events() {
        let mut hub = EventHub::default();
        let receivers: Vec<_> = (0..MAX_WATCHERS)
            .map(|_| {
                let (subscriber, receiver) = channel();
                assert!(hub.subscribe(subscriber));
                receiver
            })
            .collect();
        let (extra, _) = channel();
        assert!(!hub.subscribe(extra));
        drop(receivers);
        let (replacement, _receiver) = channel();
        assert!(hub.subscribe(replacement));
    }
    #[test]
    fn event_json_is_typed_and_independently_parseable() {
        let mut hub = EventHub::default();
        let (subscriber, receiver) = channel();
        hub.subscribe(subscriber);
        hub.publish(&status(), Event::PageChanged { page: 2, pages: 3 });
        let envelope = receiver.events.try_recv().unwrap();
        let json = super::super::client::format_event(&envelope, true).unwrap();
        assert_eq!(
            serde_json::from_str::<EventEnvelope>(&json).unwrap(),
            envelope
        );
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["event"], "page.changed");
        assert_eq!(value["sequence"], 1);
        assert_eq!(value["page"], 2);
        assert_eq!(value["protocol_version"], 1);
    }
}
