//! Layer A: the reactive models between the facade and the view-models.
//!
//! A model owns a `teksilo::data` collection (`ListModel` with keyed
//! reconcile, so a row keeps its identity across a reload) or a snapshot
//! behind a `Signal`, and keeps it current from the facade:
//!
//! - it loads through the facade's commands, which return a
//!   [`Reply`](crate::frontend::Reply) that is awaited on the main-thread
//!   executor ([`load`]); nothing here blocks the UI thread (quirk fix 5);
//! - it reloads on the facade's events, coalesced so that a burst triggers one
//!   reload per frame ([`coalesced_reload`]);
//! - a newer request supersedes an older one ([`Latest`](crate::frontend::Latest)),
//!   so an older answer can never overwrite a newer one.
//!
//! Every model has the same lifecycle. It is built with the
//! [`AppContext`](crate::frontend::AppContext) and the executor's
//! [`AsyncRuntimeHandle`](teksilo::prelude::AsyncRuntimeHandle); a long-lived
//! widget's `build` calls its `wire`, which subscribes it to the events it
//! reloads on (a subscription lasts as long as the build that made it, so
//! `wire` subscribes afresh on every call); its `reload` starts a load.
//!
//! ## Real and mock builds
//!
//! The mocks build already runs the whole facade over an in-memory database
//! with fake devices, so no model has a real/mock seam: they read the seeded
//! database through the same commands. The two seams of Layer A are the
//! startup data (`mock_data`) and the cover
//! pixels ([`CoverStore`](crate::teksilo_ui::shared::cover_store::CoverStore)).

pub mod album_list_model;
pub mod audio_devices_model;
pub mod coalesced_reload;
pub mod eq_presets_model;
pub mod filter_hierarchy_model;
pub mod library_songs_model;
pub mod load;
pub mod playlist_content_model;
pub mod playlist_tabs_model;
pub mod reconcile;
#[cfg(test)]
pub(crate) mod test_env;

pub use album_list_model::{AlbumCard, AlbumListModel};
pub use audio_devices_model::{AudioDeviceRow, AudioDevicesModel};
pub use coalesced_reload::{ReloadCoalescer, reload_on_events};
pub use eq_presets_model::EqPresetsModel;
pub use filter_hierarchy_model::FilterHierarchyModel;
pub use library_songs_model::{LibrarySongsModel, has_any_filter};
pub use playlist_content_model::PlaylistContentModel;
pub use playlist_tabs_model::{PlaylistTab, PlaylistTabsModel};
