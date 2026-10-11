//! `AudioDevicesModel`: the audio hosts and output devices of the Audio
//! Center's configuration tab.
//!
//! From `commands::audio_settings::audio_center_state`, which enumerates the
//! hosts and devices on the engine worker (never on the UI thread, quirk fix
//! 5) and reads everything else the tab opens with. The model keeps:
//!
//! - `hosts`, a `ListModel<String>`, and `devices`, a
//!   `ListModel<AudioDeviceRow>`, reconciled by key so a combo box keeps its
//!   selection when a reload brings the same entries;
//! - the whole [`AudioCenterState`] (the pending selections, the output
//!   stream, the current track, the persisted settings) as a signal, for the
//!   tab's view-model.
//!
//! It reloads on [`PlaybackEvent::DeviceChanged`] (coalesced) and on request
//! ([`reload`](AudioDevicesModel::reload), when the tab opens).

use std::rc::Rc;

use teksilo::data::ListModel;
use teksilo::prelude::{AsyncRuntimeHandle, BuildContext, Signal};

use super::coalesced_reload::{ReloadCoalescer, reload_on_events};
use super::load::{finish_tracked, prepare_runtime};
use super::reconcile::reconcile_unique;
use crate::frontend::commands::audio_settings::{self, AudioCenterState};
use crate::frontend::events::PlaybackEvent;
use crate::frontend::{AppContext, Event, Latest, Origin};

/// An output device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioDeviceRow {
    /// The device's stable id, when it has one.
    pub id: Option<String>,
    /// The name it is listed by.
    pub name: String,
}

/// Whether `event` is a change of the output device.
pub fn is_device_event(event: &Event) -> bool {
    matches!(event, Event::Playback(PlaybackEvent::DeviceChanged(_)))
}

struct Inner {
    ctx: AppContext,
    runtime: AsyncRuntimeHandle,
    hosts: ListModel<String>,
    devices: ListModel<AudioDeviceRow>,
    state: Signal<Option<Rc<AudioCenterState>>>,
    latest: Latest,
    version: Signal<u64>,
    loading: Signal<bool>,
}

/// The hosts and devices. UI thread only; clones share one state.
#[derive(Clone)]
pub struct AudioDevicesModel {
    inner: Rc<Inner>,
}

impl std::fmt::Debug for AudioDevicesModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioDevicesModel")
            .field("hosts", &self.inner.hosts.len())
            .field("devices", &self.inner.devices.len())
            .finish_non_exhaustive()
    }
}

impl AudioDevicesModel {
    /// An empty model. Nothing is loaded until [`reload`](Self::reload).
    pub fn new(ctx: AppContext, runtime: AsyncRuntimeHandle) -> Self {
        Self {
            inner: Rc::new(Inner {
                ctx,
                runtime,
                hosts: ListModel::new(),
                devices: ListModel::new(),
                state: Signal::new(None),
                latest: Latest::new(),
                version: Signal::new(0),
                loading: Signal::new(false),
            }),
        }
    }

    /// Subscribes the model to [`PlaybackEvent::DeviceChanged`]; the reload
    /// runs once per frame however many arrive. Call it from the widget that
    /// shows the tab.
    pub fn wire(&self, ctx: &mut BuildContext) -> ReloadCoalescer {
        prepare_runtime(ctx, &self.inner.runtime);
        let model = self.clone();
        reload_on_events(ctx, [Origin::Playback], is_device_event, move || {
            model.reload();
        })
    }

    /// The audio hosts.
    pub fn hosts(&self) -> ListModel<String> {
        self.inner.hosts.clone()
    }

    /// The output devices.
    pub fn devices(&self) -> ListModel<AudioDeviceRow> {
        self.inner.devices.clone()
    }

    /// Everything the tab opens with; `None` until the first load answers.
    pub fn state(&self) -> Signal<Option<Rc<AudioCenterState>>> {
        self.inner.state.clone()
    }

    /// A counter that bumps with every answer applied.
    pub fn version(&self) -> Signal<u64> {
        self.inner.version.clone()
    }

    /// Whether a request is in flight.
    pub fn loading(&self) -> Signal<bool> {
        self.inner.loading.clone()
    }

    /// Enumerates the hosts and devices and reads the tab's state again.
    pub fn reload(&self) {
        let inner = &self.inner;
        inner.loading.set(true);
        let reply = inner
            .latest
            .track(audio_settings::audio_center_state(&inner.ctx));
        let model = self.clone();
        finish_tracked(&inner.runtime, "the audio devices", reply, move |state| {
            model.inner.loading.set(false);
            if let Some(state) = state {
                model.apply(state);
            }
        });
    }

    fn apply(&self, state: AudioCenterState) {
        let inner = &self.inner;
        reconcile_unique(&inner.hosts, state.hosts.clone(), String::clone);
        let devices = state
            .devices
            .iter()
            .map(|device| AudioDeviceRow {
                id: device.id.clone(),
                name: device.name.clone(),
            })
            .collect();
        reconcile_unique(&inner.devices, devices, |row| {
            (row.id.clone(), row.name.clone())
        });
        inner.state.set(Some(Rc::new(state)));
        let version = &inner.version;
        version.set(version.get().wrapping_add(1));
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use teksilo::data::DataChange;

    use super::*;
    use crate::audio::device_manager::AudioDeviceInfo;
    use crate::frontend::events::DeviceSnapshot;
    use crate::teksilo_ui::models::test_env::{TIMEOUT, mount, wake};
    use crate::teksilo_ui::test_support::{run_until, tree_with_async};

    fn devices(model: &AudioDevicesModel) -> Vec<AudioDeviceRow> {
        let list = model.devices();
        (0..list.len())
            .filter_map(|i| list.with_item(i, Clone::clone))
            .collect()
    }

    fn wait(runtime: &AsyncRuntimeHandle, model: &AudioDevicesModel, version: u64) {
        assert!(run_until(runtime, TIMEOUT, || model.version().get() >= version));
    }

    #[test]
    fn the_fake_engine_lists_one_host_and_one_device() {
        let ctx = AppContext::in_memory().unwrap();
        let runtime = AsyncRuntimeHandle::new();
        let model = AudioDevicesModel::new(ctx, runtime.clone());
        assert!(model.state().get().is_none());
        model.reload();
        assert!(model.loading().get());
        wait(&runtime, &model, 1);
        assert!(!model.loading().get());
        assert_eq!(
            model.hosts().with_item(0, Clone::clone),
            Some("Fake".to_string())
        );
        assert_eq!(
            devices(&model),
            vec![AudioDeviceRow {
                id: Some("fake-output".into()),
                name: "Fake Output".into()
            }]
        );
        let state = model.state().get().unwrap();
        assert_eq!(state.hosts, vec!["Fake".to_string()]);
    }

    #[test]
    fn a_reload_keeps_unchanged_rows_and_reconciles_the_changed_ones() {
        let (ctx, fakes) = AppContext::in_memory_with_fakes().unwrap();
        let runtime = AsyncRuntimeHandle::new();
        let model = AudioDevicesModel::new(ctx, runtime.clone());
        model.reload();
        wait(&runtime, &model, 1);
        let log = Rc::new(RefCell::new(Vec::new()));
        let sink = log.clone();
        let _handle = model.devices().observe_changes(move |change: &DataChange| {
            sink.borrow_mut().push(format!("{change:?}"));
        });

        model.reload();
        wait(&runtime, &model, 2);
        assert!(log.borrow().is_empty(), "the same devices change nothing");

        fakes.player.set_devices(vec![
            AudioDeviceInfo {
                name: "Fake Output".into(),
                id: Some("fake-output".into()),
            },
            AudioDeviceInfo {
                name: "USB DAC".into(),
                id: Some("usb-dac".into()),
            },
        ]);
        model.reload();
        wait(&runtime, &model, 3);
        assert_eq!(devices(&model).len(), 2);
        assert_eq!(log.borrow().len(), 1);
        assert!(log.borrow()[0].starts_with("ItemsInserted"));
    }

    #[test]
    fn two_devices_with_one_name_and_id_do_not_panic_the_reconcile() {
        let (ctx, fakes) = AppContext::in_memory_with_fakes().unwrap();
        let twin = AudioDeviceInfo {
            name: "Twin".into(),
            id: None,
        };
        fakes.player.set_devices(vec![twin.clone(), twin]);
        let runtime = AsyncRuntimeHandle::new();
        let model = AudioDevicesModel::new(ctx, runtime.clone());
        model.reload();
        wait(&runtime, &model, 1);
        assert_eq!(devices(&model).len(), 1);
    }

    #[test]
    fn only_a_device_change_reloads_the_devices() {
        let snapshot = DeviceSnapshot {
            device_id: None,
            sample_rate: 48_000,
            channels: 2,
            bit_depth_display: String::new(),
            buffer_size: 0,
        };
        assert!(is_device_event(&Event::Playback(
            PlaybackEvent::DeviceChanged(snapshot)
        )));
        assert!(!is_device_event(&Event::Playback(
            PlaybackEvent::NoticeChanged(None)
        )));
    }

    #[test]
    fn a_burst_of_device_changes_reloads_once_per_frame() {
        let ctx = AppContext::in_memory().unwrap();
        let (mut tree, runtime) = tree_with_async(&ctx);
        let model = AudioDevicesModel::new(ctx, runtime.clone());
        let slot = Rc::new(RefCell::new(None));
        {
            let (model, slot) = (model.clone(), slot.clone());
            mount(&mut tree, move |build| {
                *slot.borrow_mut() = Some(model.wire(build));
            });
        }
        let coalescer = slot.borrow().clone().unwrap();
        let wake = wake();
        for _ in 0..20 {
            coalescer.mark(&wake);
        }
        tree.frame_tick().set(16.0);
        tree.frame_tick().set(16.0);
        wait(&runtime, &model, 1);
        runtime.tick();
        assert_eq!(model.version().get(), 1);
    }
}
