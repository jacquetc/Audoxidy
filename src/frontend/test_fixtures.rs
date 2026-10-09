//! Seeded in-memory libraries for tests and the `teksilo-mocks` build.
//!
//! [`LibraryFixture`] describes songs (with every field the library and
//! playlist views show: tags, technical details, ReplayGain, cover), playlists
//! and settings, and seeds them into a [`Database`] through the same public
//! insert functions the scanner uses. It returns the ids the database gave
//! them ([`SeededLibrary`]), so tests can refer to "the second song of the
//! Favoritas playlist" without hard-coding ids. [`LibraryFixture::demo`] is a
//! small, varied library for the mocks build.

#![warn(missing_docs)]

use std::collections::BTreeMap;

use super::db_worker::DbWorker;
use super::error::FrontendError;
use super::reply::Reply;
use crate::db::Database;
use crate::db::database::{SongData, SongMetadataExtended};

/// One song to seed.
#[derive(Debug, Clone, PartialEq)]
pub struct FixtureSong {
    /// Absolute file path; its parent directory becomes the song's folder.
    pub path: String,
    /// Title tag.
    pub title: String,
    /// Track artist.
    pub artist: String,
    /// Album artist, when it differs from the track artist (compilations).
    pub album_artist: Option<String>,
    /// Album title.
    pub album: String,
    /// Genre.
    pub genre: Option<String>,
    /// Release year.
    pub year: Option<String>,
    /// Track number, as tagged ("3", "03"...).
    pub track_number: Option<String>,
    /// Duration, in seconds.
    pub duration_secs: f64,
    /// Container format ("FLAC", "MP3"...).
    pub format: String,
    /// Bits per sample.
    pub bit_depth: i64,
    /// Sample rate, in Hz.
    pub sample_rate: i64,
    /// Channel count.
    pub channels: i64,
    /// File size, in bytes.
    pub size_bytes: i64,
    /// Track ReplayGain, in dB.
    pub track_gain: Option<f64>,
    /// Album ReplayGain, in dB.
    pub album_gain: Option<f64>,
    /// Whether the song has an embedded cover (the album then gets a cover
    /// path under `cache/covers/`; the file itself is not created).
    pub with_cover: bool,
}

impl FixtureSong {
    /// A 4-minute 16-bit/44.1 kHz stereo FLAC with the given tags and no
    /// ReplayGain or cover; adjust the public fields for anything else.
    pub fn new(path: &str, title: &str, artist: &str, album: &str) -> Self {
        Self {
            path: path.to_string(),
            title: title.to_string(),
            artist: artist.to_string(),
            album_artist: None,
            album: album.to_string(),
            genre: None,
            year: None,
            track_number: None,
            duration_secs: 240.0,
            format: "FLAC".to_string(),
            bit_depth: 16,
            sample_rate: 44_100,
            channels: 2,
            size_bytes: 28_000_000,
            track_gain: None,
            album_gain: None,
            with_cover: false,
        }
    }

    fn folder(&self) -> &str {
        std::path::Path::new(&self.path)
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or("")
    }

    fn album_artist_name(&self) -> &str {
        self.album_artist.as_deref().unwrap_or(&self.artist)
    }
}

/// One playlist to seed: a new playlist, or an existing one (such as the
/// system playlist "Audoxidy") when the name is taken.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixturePlaylist {
    /// The playlist name.
    pub name: String,
    /// The songs, by path, in playlist order.
    pub song_paths: Vec<String>,
    /// The songs (by path) to disable in the playlist.
    pub disabled_paths: Vec<String>,
}

/// A song as seeded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeededSong {
    /// Its database id.
    pub id: i64,
    /// Its path.
    pub path: String,
    /// Its title.
    pub title: String,
    /// The id of its folder.
    pub folder_id: i64,
    /// The id of its track artist.
    pub artist_id: i64,
    /// The hash id of its album (the UI's composite album id is
    /// `"{album artist}|{hash}"`).
    pub album_hash: String,
}

/// An album as seeded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeededAlbum {
    /// The album's hash id.
    pub hash: String,
    /// Its title.
    pub title: String,
    /// Its album artist.
    pub artist: String,
    /// The album artist's id.
    pub artist_id: i64,
}

/// A playlist as seeded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeededPlaylist {
    /// Its database id.
    pub id: i64,
    /// Its name.
    pub name: String,
    /// The ids of the songs added to it, in order.
    pub song_ids: Vec<i64>,
    /// The ids of the songs disabled in it.
    pub disabled_song_ids: Vec<i64>,
}

/// The ids of everything [`LibraryFixture::seed`] created.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeededLibrary {
    /// The songs, in fixture order.
    pub songs: Vec<SeededSong>,
    /// The folders, as `(id, path)`, sorted by path.
    pub folders: Vec<(i64, String)>,
    /// The artists (track and album artists), as `(id, name)`, sorted by name.
    pub artists: Vec<(i64, String)>,
    /// The albums, sorted by hash.
    pub albums: Vec<SeededAlbum>,
    /// The playlists, in fixture order.
    pub playlists: Vec<SeededPlaylist>,
    /// The id of the system playlist "Audoxidy".
    pub main_playlist_id: i64,
    /// The id of the system playlist "Oxidy Drift".
    pub external_playlist_id: i64,
}

impl SeededLibrary {
    /// The seeded song at `path`.
    pub fn song(&self, path: &str) -> Option<&SeededSong> {
        self.songs.iter().find(|song| song.path == path)
    }

    /// The id of the seeded song at `path`.
    pub fn song_id(&self, path: &str) -> Option<i64> {
        self.song(path).map(|song| song.id)
    }

    /// The seeded playlist called `name`.
    pub fn playlist(&self, name: &str) -> Option<&SeededPlaylist> {
        self.playlists.iter().find(|playlist| playlist.name == name)
    }
}

/// A library to seed; see the module documentation.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LibraryFixture {
    songs: Vec<FixtureSong>,
    playlists: Vec<FixturePlaylist>,
    settings: Vec<(String, String)>,
}

impl LibraryFixture {
    /// An empty fixture.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a song.
    pub fn song(mut self, song: FixtureSong) -> Self {
        self.songs.push(song);
        self
    }

    /// Adds a playlist with the songs at `paths`, all enabled.
    pub fn playlist(self, name: &str, paths: &[&str]) -> Self {
        self.playlist_with_disabled(name, paths, &[])
    }

    /// Adds a playlist with the songs at `paths`, those in `disabled`
    /// disabled.
    pub fn playlist_with_disabled(mut self, name: &str, paths: &[&str], disabled: &[&str]) -> Self {
        self.playlists.push(FixturePlaylist {
            name: name.to_string(),
            song_paths: paths.iter().map(|p| (*p).to_string()).collect(),
            disabled_paths: disabled.iter().map(|p| (*p).to_string()).collect(),
        });
        self
    }

    /// Stores an `APP_SETTINGS` value.
    pub fn setting(mut self, key: &str, value: &str) -> Self {
        self.settings.push((key.to_string(), value.to_string()));
        self
    }

    /// The songs of this fixture.
    pub fn songs(&self) -> &[FixtureSong] {
        &self.songs
    }

    /// A small, varied library: three artists, a compilation, several formats,
    /// an album name containing `|`, ReplayGain on some tracks, covers on some
    /// albums, a "Favoritas" playlist with a disabled song, and songs in the
    /// system playlist "Audoxidy".
    pub fn demo() -> Self {
        const NIGHT: &str = "/fixtures/music/Aurora Lane/Night Signals";
        const DAYBREAK: &str = "/fixtures/music/Aurora Lane/Daybreak EP";
        const WIRES: &str = "/fixtures/music/The Copper Wires/Left | Right";
        const SUMMER: &str = "/fixtures/music/Various/Summer Mix";

        let night = |n: u32, title: &str, secs: f64| FixtureSong {
            genre: Some("Synthpop".into()),
            year: Some("2019".into()),
            track_number: Some(n.to_string()),
            duration_secs: secs,
            bit_depth: 24,
            sample_rate: 96_000,
            size_bytes: 95_000_000,
            track_gain: Some(-7.2),
            album_gain: Some(-6.8),
            with_cover: true,
            ..FixtureSong::new(
                &format!("{NIGHT}/{n:02} - {title}.flac"),
                title,
                "Aurora Lane",
                "Night Signals",
            )
        };
        let daybreak = |n: u32, title: &str, secs: f64| FixtureSong {
            genre: Some("Synthpop".into()),
            year: Some("2021".into()),
            track_number: Some(n.to_string()),
            duration_secs: secs,
            format: "MP3".into(),
            size_bytes: 9_400_000,
            ..FixtureSong::new(
                &format!("{DAYBREAK}/{n:02} - {title}.mp3"),
                title,
                "Aurora Lane",
                "Daybreak EP",
            )
        };
        let wires = |n: u32, title: &str, secs: f64| FixtureSong {
            genre: Some("Rock".into()),
            year: Some("2015".into()),
            track_number: Some(n.to_string()),
            duration_secs: secs,
            with_cover: true,
            ..FixtureSong::new(
                &format!("{WIRES}/{n:02} - {title}.flac"),
                title,
                "The Copper Wires",
                "Left | Right",
            )
        };
        let summer = |n: u32, title: &str, artist: &str, secs: f64| FixtureSong {
            album_artist: Some("Various Artists".into()),
            genre: Some("Pop".into()),
            year: Some("2023".into()),
            track_number: Some(n.to_string()),
            duration_secs: secs,
            format: "OGG".into(),
            size_bytes: 7_100_000,
            ..FixtureSong::new(
                &format!("{SUMMER}/{n:02} - {title}.ogg"),
                title,
                artist,
                "Summer Mix",
            )
        };

        let songs = vec![
            night(1, "Static Bloom", 252.4),
            night(2, "Glass Harbour", 301.0),
            night(3, "Neon Tide", 198.7),
            night(4, "Last Transmission", 415.2),
            daybreak(1, "First Light", 187.0),
            daybreak(2, "Morning Static", 224.5),
            wires(1, "Left", 176.0),
            wires(2, "Right", 203.3),
            wires(3, "Both Ways", 289.9),
            summer(1, "Sol y Sombra", "Mira Kovač", 210.0),
            summer(2, "Copper Sun", "The Copper Wires", 195.5),
        ];
        let path = |i: usize| songs[i].path.clone();
        let favoritas = [path(1), path(4), path(7), path(9)];
        let main = [path(0), path(1), path(2), path(3), path(4)];

        let mut fixture = LibraryFixture::new();
        for song in songs.iter().cloned() {
            fixture = fixture.song(song);
        }
        let favoritas: Vec<&str> = favoritas.iter().map(String::as_str).collect();
        let main: Vec<&str> = main.iter().map(String::as_str).collect();
        fixture
            .playlist_with_disabled("Favoritas", &favoritas, &[favoritas[2]])
            .playlist("Audoxidy", &main)
            .setting("last_active_playlist_id", "1")
    }

    /// Seeds the database and returns the ids it gave everything.
    ///
    /// Fails with [`FrontendError::InvalidInput`] if a playlist names a path
    /// that is not one of the fixture's songs.
    pub fn seed(&self, db: &mut Database) -> Result<SeededLibrary, FrontendError> {
        for (index, song) in self.songs.iter().enumerate() {
            let data = SongData {
                full_file_path: song.path.as_str().into(),
                title: Some(song.title.as_str().into()),
                artist: Some(song.artist.as_str().into()),
                album: Some(song.album.as_str().into()),
                album_artist: song.album_artist.as_deref().map(Into::into),
                genre: song.genre.as_deref().map(Into::into),
                release_year: song.year.as_deref().map(Into::into),
                track_number: song.track_number.as_deref().map(Into::into),
                duration_secs: Some(song.duration_secs),
                format: Some(song.format.as_str().into()),
                bit_depth: Some(song.bit_depth),
                sample_rate: Some(song.sample_rate),
                size: Some(song.size_bytes),
                channels: Some(song.channels),
                embedded_cover: song.with_cover,
                import_order: i64::try_from(index).unwrap_or(i64::MAX).saturating_add(1),
                ..SongData::default()
            };
            let extended = SongMetadataExtended {
                track_gain: song.track_gain,
                album_gain: song.album_gain,
                ..SongMetadataExtended::default()
            };
            let cover_hash = song.with_cover.then(|| {
                Database::generate_hash(&format!("{}{}", song.album_artist_name(), song.album))
            });
            db.insert_song_full(&data, &extended, cover_hash, Vec::new(), false)?;
        }

        let mut artists = BTreeMap::new();
        let mut albums = BTreeMap::new();
        let mut songs = Vec::with_capacity(self.songs.len());
        for song in &self.songs {
            let id = db.get_song_id_by_path(&song.path)?.ok_or_else(|| {
                FrontendError::InvalidInput(format!("song {} was not inserted", song.path))
            })?;
            let folder_id = db.upsert_folder(song.folder())?;
            let artist_id = db.upsert_artist(&song.artist)?;
            let album_artist = song.album_artist_name().trim().to_string();
            let album_artist_id = db.upsert_artist(&album_artist)?;
            artists.insert(song.artist.trim().to_string(), artist_id);
            artists.insert(album_artist.clone(), album_artist_id);
            let album_hash = Database::generate_hash(&format!(
                "{}{}{}",
                song.album,
                album_artist_id,
                song.folder()
            ));
            albums
                .entry(album_hash.clone())
                .or_insert_with(|| SeededAlbum {
                    hash: album_hash.clone(),
                    title: song.album.clone(),
                    artist: album_artist,
                    artist_id: album_artist_id,
                });
            songs.push(SeededSong {
                id,
                path: song.path.clone(),
                title: song.title.clone(),
                folder_id,
                artist_id,
                album_hash,
            });
        }

        let mut playlists = Vec::with_capacity(self.playlists.len());
        for fixture in &self.playlists {
            let id = match db.get_playlist_by_name(&fixture.name)? {
                Some(existing) => existing.id,
                None => db.create_playlist(&fixture.name, false)?,
            };
            let ids_of = |paths: &[String]| -> Result<Vec<i64>, FrontendError> {
                paths
                    .iter()
                    .map(|path| {
                        songs
                            .iter()
                            .find(|song| &song.path == path)
                            .map(|song| song.id)
                            .ok_or_else(|| {
                                FrontendError::InvalidInput(format!(
                                    "playlist {} lists {path}, which is not a fixture song",
                                    fixture.name
                                ))
                            })
                    })
                    .collect()
            };
            let song_ids = ids_of(&fixture.song_paths)?;
            let disabled_song_ids = ids_of(&fixture.disabled_paths)?;
            db.add_songs_to_playlist(id, &song_ids)?;
            for song_id in &disabled_song_ids {
                db.set_song_enabled_in_playlist(id, *song_id, false)?;
            }
            playlists.push(SeededPlaylist {
                id,
                name: fixture.name.clone(),
                song_ids,
                disabled_song_ids,
            });
        }

        for (key, value) in &self.settings {
            db.set_setting(key, value)?;
        }

        let system_id = |db: &Database, name: &str| -> Result<i64, FrontendError> {
            db.get_playlist_by_name(name)?
                .map(|playlist| playlist.id)
                .ok_or_else(|| FrontendError::InvalidInput(format!("no system playlist {name}")))
        };
        Ok(SeededLibrary {
            songs,
            folders: db
                .get_all_folders()?
                .into_iter()
                .map(|(id, path, _)| (id, path))
                .collect(),
            artists: artists.into_iter().map(|(name, id)| (id, name)).collect(),
            albums: albums.into_values().collect(),
            playlists,
            main_playlist_id: system_id(db, "Audoxidy")?,
            external_playlist_id: system_id(db, "Oxidy Drift")?,
        })
    }

    /// Seeds the database through `worker` (a write job).
    pub fn seed_on(self, worker: &DbWorker) -> Reply<SeededLibrary> {
        worker.write(move |db| self.seed(db))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::threading::test_support::{TEST_TIMEOUT, with_timeout};
    use std::sync::{Arc, Mutex};

    fn worker() -> DbWorker {
        DbWorker::new(Arc::new(Mutex::new(Database::new_memory().unwrap()))).unwrap()
    }

    #[test]
    fn the_seeded_library_reads_back_through_the_database_worker() {
        with_timeout(TEST_TIMEOUT, || {
            let worker = worker();
            let seeded = LibraryFixture::demo()
                .seed_on(&worker)
                .recv_blocking()
                .unwrap();
            assert_eq!(seeded.songs.len(), 11);
            assert_eq!(seeded.folders.len(), 4);
            assert_eq!(seeded.albums.len(), 4);
            assert!(
                seeded
                    .artists
                    .iter()
                    .any(|(_, name)| name == "Various Artists")
            );

            let songs = worker
                .read(|db| Ok(db.get_all_songs()?))
                .recv_blocking()
                .unwrap();
            assert_eq!(songs.len(), 11);
            let glass = songs
                .iter()
                .find(|song| song.title.as_deref() == Some("Glass Harbour"))
                .unwrap();
            assert_eq!(glass.artist.as_deref(), Some("Aurora Lane"));
            assert_eq!(glass.album.as_deref(), Some("Night Signals"));
            assert_eq!(glass.genre.as_deref(), Some("Synthpop"));
            assert_eq!(glass.release_year.as_deref(), Some("2019"));
            assert_eq!(glass.track_number.as_deref(), Some("2"));
            assert_eq!(glass.bit_depth, Some(24));
            assert_eq!(glass.sample_rate, Some(96_000));
            assert_eq!(glass.duration_secs, Some(301.0));
            assert!(
                glass.compressed_cached_cover_root.is_some(),
                "the album has a cover"
            );
            assert_eq!(glass.id, seeded.song_id(&glass.full_file_path).unwrap());

            let compilation = songs
                .iter()
                .find(|song| song.title.as_deref() == Some("Sol y Sombra"))
                .unwrap();
            assert_eq!(compilation.album_artist.as_deref(), Some("Various Artists"));

            let path = glass.full_file_path.to_string();
            let gains = worker
                .read(move |db| Ok(db.get_replay_gain_by_path(&path)?))
                .recv_blocking()
                .unwrap();
            assert_eq!(gains, (Some(-7.2), Some(-6.8)));
        });
    }

    #[test]
    fn the_seeded_playlists_keep_their_order_and_disabled_songs() {
        with_timeout(TEST_TIMEOUT, || {
            let worker = worker();
            let seeded = LibraryFixture::demo()
                .seed_on(&worker)
                .recv_blocking()
                .unwrap();
            let favoritas = seeded.playlist("Favoritas").unwrap().clone();
            assert_eq!(favoritas.disabled_song_ids.len(), 1);
            let main = seeded.playlist("Audoxidy").unwrap();
            assert_eq!(
                main.id, seeded.main_playlist_id,
                "the system playlist is reused"
            );

            let id = favoritas.id;
            let items = worker
                .read(move |db| Ok(db.get_playlist_songs(id)?))
                .recv_blocking()
                .unwrap();
            let ids: Vec<i64> = items.iter().map(|item| item.song_id).collect();
            assert_eq!(ids, favoritas.song_ids);
            let disabled: Vec<i64> = items
                .iter()
                .filter(|item| !item.enabled)
                .map(|item| item.song_id)
                .collect();
            assert_eq!(disabled, favoritas.disabled_song_ids);

            let setting = worker
                .read(|db| Ok(db.get_setting("last_active_playlist_id")))
                .recv_blocking()
                .unwrap();
            assert_eq!(setting.as_deref(), Some("1"));
        });
    }

    #[test]
    fn a_playlist_naming_an_unknown_path_is_refused() {
        with_timeout(TEST_TIMEOUT, || {
            let worker = worker();
            let result = LibraryFixture::new()
                .song(FixtureSong::new("/m/a.flac", "A", "Artist", "Album"))
                .playlist("Broken", &["/m/missing.flac"])
                .seed_on(&worker)
                .recv_blocking();
            assert!(matches!(result, Err(FrontendError::InvalidInput(_))));
        });
    }
}
