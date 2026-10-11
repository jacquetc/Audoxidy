//! `SingleOutputDevice`: the output device and the format of its stream.
//!
//! The [`DeviceSnapshot`] of the engine, initialised from the player's state
//! at construction and replaced on every [`PlaybackEvent::DeviceChanged`].

use teksilo::prelude::{BuildContext, Signal};

use crate::frontend::events::{DeviceSnapshot, PlaybackEvent};
use crate::frontend::{AppContext, Event, Origin};

/// The output device. UI thread only; clones share one state.
#[derive(Clone)]
pub struct SingleOutputDevice {
    device: Signal<DeviceSnapshot>,
}

impl std::fmt::Debug for SingleOutputDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SingleOutputDevice")
            .field("device", &self.device.get())
            .finish()
    }
}

impl SingleOutputDevice {
    /// A single initialised from the player's current state.
    pub fn new(ctx: &AppContext) -> Self {
        Self {
            device: Signal::new(DeviceSnapshot::from_state(&ctx.player.snapshot())),
        }
    }

    /// Subscribes the single to the playback events. Call it from a
    /// long-lived widget's `build`.
    pub fn wire(&self, ctx: &mut BuildContext) {
        let single = self.clone();
        ctx.subscribe_event(Origin::Playback, move |event: &Event| {
            single.on_event(event);
        });
    }

    /// The device and its stream format.
    pub fn device(&self) -> Signal<DeviceSnapshot> {
        self.device.clone()
    }

    /// Applies a playback event; every event but
    /// [`PlaybackEvent::DeviceChanged`] is ignored.
    pub fn on_event(&self, event: &Event) {
        if let Event::Playback(PlaybackEvent::DeviceChanged(device)) = event {
            self.device.set_if_changed(device.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::*;

    #[test]
    fn it_starts_from_the_players_state() {
        let ctx = AppContext::in_memory().unwrap();
        let single = SingleOutputDevice::new(&ctx);
        assert_eq!(
            single.device().get(),
            DeviceSnapshot::from_state(&ctx.player.snapshot())
        );
    }

    #[test]
    fn a_device_change_replaces_the_snapshot_once() {
        let ctx = AppContext::in_memory().unwrap();
        let single = SingleOutputDevice::new(&ctx);
        let notified = Rc::new(Cell::new(0));
        let counter = notified.clone();
        let _handle = single
            .device()
            .observe(move |_| counter.set(counter.get() + 1));
        let next = DeviceSnapshot {
            device_id: Some("usb-dac".into()),
            sample_rate: 96_000,
            channels: 2,
            bit_depth_display: "24-bit".into(),
            buffer_size: 1024,
        };
        let event = Event::Playback(PlaybackEvent::DeviceChanged(next.clone()));
        single.on_event(&event);
        single.on_event(&event);
        assert_eq!(single.device().get(), next);
        assert_eq!(notified.get(), 1);
    }

    #[test]
    fn other_playback_events_are_ignored() {
        let ctx = AppContext::in_memory().unwrap();
        let single = SingleOutputDevice::new(&ctx);
        let before = single.device().get();
        single.on_event(&Event::Playback(PlaybackEvent::NoticeChanged(None)));
        assert_eq!(single.device().get(), before);
    }
}
