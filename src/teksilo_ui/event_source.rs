//! The bridge from the facade's event hub to Teksilo's `EventSource`.
//!
//! The facade publishes typed [`Event`]s from its threads (the playback
//! monitor, the workers, the long operations). Registered with
//! `TeksiloAppBuilder::event_source`, this adapter lets any widget call
//! `ctx.subscribe_event(Origin::Playback, |event: &Event| …)`: Teksilo hands
//! the event to the UI thread and runs the callback there, and drops the
//! subscription with the widget that made it.
//!
//! The hub already has the shape Teksilo expects (a `Clone + Eq + Hash`
//! origin, a `Clone + Send` event, an `Arc` callback, a token that
//! unsubscribes on drop), so the adapter only forwards.

use std::sync::Arc;

use teksilo::core::event_source::{EventSource, SubscriptionHandle};

use crate::frontend::{Event, EventHub, Origin};

/// The application's event source: the facade's [`EventHub`].
#[derive(Debug, Clone)]
pub struct FrontendEventSource {
    hub: EventHub,
}

impl FrontendEventSource {
    /// An event source over `hub`.
    pub fn new(hub: EventHub) -> Self {
        Self { hub }
    }
}

impl EventSource for FrontendEventSource {
    type Origin = Origin;
    type Event = Event;

    fn subscribe(
        &self,
        origin: Self::Origin,
        callback: Arc<dyn Fn(Self::Event) + Send + Sync + 'static>,
    ) -> SubscriptionHandle {
        SubscriptionHandle::new(self.hub.subscribe(origin, callback))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use super::*;
    use crate::frontend::events::{LibraryEvent, PlaylistsEvent};

    const TIMEOUT: Duration = Duration::from_secs(5);

    #[test]
    fn a_subscription_receives_the_events_of_its_origin_only() {
        let hub = EventHub::new().unwrap();
        let source = FrontendEventSource::new(hub.clone());
        let (tx, rx) = mpsc::channel();
        let tx = std::sync::Mutex::new(tx);
        let _handle = source.subscribe(
            Origin::Library,
            Arc::new(move |event| {
                let _ = tx.lock().unwrap().send(event.origin());
            }),
        );
        hub.publish(Event::Playlists(PlaylistsEvent::ListChanged));
        hub.publish(Event::Library(LibraryEvent::Changed));
        assert_eq!(rx.recv_timeout(TIMEOUT).unwrap(), Origin::Library);
        assert!(rx.recv_timeout(Duration::from_millis(100)).is_err());
        assert!(hub.shutdown(TIMEOUT));
    }

    #[test]
    fn dropping_the_handle_unsubscribes() {
        let hub = EventHub::new().unwrap();
        let source = FrontendEventSource::new(hub.clone());
        let handle = source.subscribe(Origin::Library, Arc::new(|_| {}));
        assert_eq!(hub.subscriber_count(), 1);
        drop(handle);
        assert_eq!(hub.subscriber_count(), 0);
        assert!(hub.shutdown(TIMEOUT));
    }
}
