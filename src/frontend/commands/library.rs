//! Library commands: loading and searching the library, the songs of an album,
//! deleting songs and scanning a folder.
//!
//! Every database access goes through the [`DbWorker`](crate::frontend::DbWorker)
//! or a long operation, never on the caller's thread.

#![warn(missing_docs)]

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use crate::db::database::{Database, LibrarySearchParams, SongData};
use crate::frontend::app_context::AppContext;
use crate::frontend::error::FrontendError;
use crate::frontend::events::{Event, LibraryEvent, LongOpKind, OpId};
use crate::frontend::reply::{Latest, Reply, TrackedReply};
use crate::logic::library_groups::{
    AlbumEntry, LibraryStats, UNKNOWN_ALBUM, parse_album_composite_id,
};
use crate::utils::get_effective_artist;

/// Where the cached cover files live, relative to the working directory (as
/// the Iced UI does).
const COVERS_DIR: &str = "cache/covers";

/// Everything the library views need after a refresh.
#[derive(Debug, Clone)]
pub struct LibrarySnapshot {
    /// The songs matching the search parameters.
    pub songs: Vec<Arc<SongData>>,
    /// Every album of the library (not only those matching the parameters).
    pub albums: Vec<AlbumEntry>,
    /// The totals of the whole library.
    pub stats: LibraryStats,
    /// The library folders as `(id, path, name)`.
    pub folders: Vec<(i64, String, String)>,
}

/// An album row of the grid query turned into an [`AlbumEntry`].
fn album_entry(row: (String, String, String, String, String, Option<String>)) -> AlbumEntry {
    AlbumEntry {
        id: row.0,
        title: row.1,
        artist: row.2,
        genre: row.3,
        year: row.4,
        cover_path: row.5,
    }
}

/// The totals tuple of the database as [`LibraryStats`].
fn library_stats(db: &Database) -> Result<LibraryStats, FrontendError> {
    let (songs, albums, duration_secs, size_bytes, artists) = db.get_library_stats()?;
    Ok(LibraryStats {
        songs: songs as u64,
        albums: albums as u64,
        artists: artists as u64,
        duration_secs,
        size_bytes,
    })
}

/// Loads the songs matching `params`, the album grid, the totals and the
/// folders: what the Iced `LibraryRefresh` produces.
pub fn load_library(ctx: &AppContext, params: LibrarySearchParams) -> Reply<LibrarySnapshot> {
    ctx.db.read(move |db| {
        Ok(LibrarySnapshot {
            songs: db.get_library_songs_filtered(&params)?,
            albums: db
                .get_grid_items_by_artist()?
                .into_iter()
                .map(album_entry)
                .collect(),
            stats: library_stats(db)?,
            folders: db.get_all_folders()?,
        })
    })
}

/// The totals of the whole library.
pub fn stats(ctx: &AppContext) -> Reply<LibraryStats> {
    ctx.db.read(library_stats)
}

/// The songs matching the search text and the artist, album, genre, year and
/// folder filters of `params`.
///
/// A newer call on the same `latest` makes the reply of an older one stale:
/// its `recv` then yields `None`.
pub fn search(
    ctx: &AppContext,
    params: LibrarySearchParams,
    latest: &Latest,
) -> TrackedReply<Vec<Arc<SongData>>> {
    let reply = ctx
        .db
        .read(move |db| Ok(db.get_library_songs_filtered(&params)?));
    latest.track(reply)
}

/// The songs of the album `hash` whose album artist is `artist`.
pub fn album_songs(ctx: &AppContext, artist: &str, hash: &str) -> Reply<Vec<Arc<SongData>>> {
    let (artist, hash) = (artist.to_string(), hash.to_string());
    ctx.db
        .read(move |db| Ok(db.get_songs_by_album_and_artist(&hash, &artist)?))
}

/// The songs of an album given its composite id (`"{artist}|{hash}"`, see
/// [`album_composite_id`]).
///
/// An id that does not parse is taken as a bare album hash, as in the Iced
/// UI.
pub fn album_songs_by_composite_id(
    ctx: &AppContext,
    composite_id: &str,
) -> Reply<Vec<Arc<SongData>>> {
    match parse_album_composite_id(composite_id) {
        Some((artist, hash)) => album_songs(ctx, &artist, &hash),
        None => {
            let id = composite_id.to_string();
            ctx.db.read(move |db| Ok(db.get_songs_by_album(&id)?))
        }
    }
}

/// The cover hashes a deletion may leave unused: the album hash of each
/// deleted song and its cover override.
fn covers_to_check(db: &Database, ids: &HashSet<i64>) -> Result<Vec<String>, FrontendError> {
    let albums: HashMap<(String, String), String> = db
        .get_grid_items_by_artist()?
        .into_iter()
        .map(|(hash, title, artist, ..)| ((artist, title), hash))
        .collect();
    let mut covers = Vec::new();
    for song in db.get_all_songs()? {
        if !ids.contains(&song.id) {
            continue;
        }
        let artist = get_effective_artist(&song).to_string();
        let album = song.album.as_deref().unwrap_or(UNKNOWN_ALBUM).to_string();
        if let Some(hash) = albums.get(&(artist, album)) {
            covers.push(hash.clone());
        }
        if let Some(cover) = &song.cover_override {
            covers.push(cover.to_string());
        }
    }
    covers.sort();
    covers.dedup();
    Ok(covers)
}

/// Deletes the songs, the albums and artists left empty, and returns the
/// number of songs deleted with the cover hashes nothing uses any more.
fn delete_in_database(
    db: &mut Database,
    song_ids: &[i64],
) -> Result<(usize, Vec<String>), FrontendError> {
    let ids: HashSet<i64> = song_ids.iter().copied().collect();
    let existing = db
        .get_all_songs()?
        .iter()
        .filter(|song| ids.contains(&song.id))
        .count();
    let covers = covers_to_check(db, &ids)?;
    let ids: Vec<i64> = ids.into_iter().collect();

    db.begin_transaction()?;
    let outcome = db
        .batch_delete_songs(&ids)
        .and_then(|()| db.cleanup_empty_metadata())
        .and_then(|()| db.commit_transaction());
    if let Err(e) = outcome {
        // Best effort: the transaction may already be closed.
        let _ = db.rollback_transaction();
        return Err(e.into());
    }

    let mut orphans = Vec::new();
    for hash in covers {
        if !db.is_cover_hash_in_use(&hash)? {
            orphans.push(hash);
        }
    }
    Ok((existing, orphans))
}

/// Removes `<dir>/<hash>.avif` for every hash; a missing file is fine.
fn remove_cover_files(dir: &Path, hashes: &[String]) {
    for hash in hashes {
        let _ = std::fs::remove_file(dir.join(format!("{hash}.avif")));
    }
}

/// Deletes songs from the library as a long operation
/// ([`LongOpKind::LibraryDelete`]).
///
/// The songs, then the albums and artists they leave empty, are deleted in one
/// transaction on the database worker; the cover files that nothing uses any
/// more are removed afterwards. [`LibraryEvent::Changed`] is published when
/// the database changed. The result is the number of songs deleted, taken with
/// `ctx.ops.take_result::<usize>(op_id)`.
///
/// Asking the user to confirm is the UI's job.
pub fn delete_songs(ctx: &AppContext, song_ids: Vec<i64>) -> OpId {
    let db = ctx.db.clone();
    let hub = ctx.hub.clone();
    ctx.ops.start(LongOpKind::LibraryDelete, move |op| {
        op.check_cancelled()?;
        if song_ids.is_empty() {
            return Ok(0usize);
        }
        let (deleted, orphans) = db
            .write(move |db| delete_in_database(db, &song_ids))
            .recv_blocking()?;
        op.report_progress(0.9);
        remove_cover_files(Path::new(COVERS_DIR), &orphans);
        hub.publish(Event::Library(LibraryEvent::Changed));
        Ok(deleted)
    })
}

/// Starts scanning `path` in the background.
///
/// The playback monitor publishes [`LibraryEvent::ScanStarted`],
/// [`LibraryEvent::Changed`] and [`LibraryEvent::ScanFinished`] from the
/// scanner's flags, so this publishes nothing itself. Fails with
/// [`FrontendError::InvalidInput`] for an empty path.
pub fn scan_folder(ctx: &AppContext, path: &str) -> Result<(), FrontendError> {
    if path.trim().is_empty() {
        return Err(FrontendError::InvalidInput(
            "the folder to scan has no path".into(),
        ));
    }
    ctx.scanner.scan_folder_async(path.to_string());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::events::{LongOpEvent, Origin};
    use crate::frontend::test_fixtures::{FixtureSong, LibraryFixture, SeededLibrary};
    use crate::frontend::threading::test_support::{TEST_TIMEOUT, with_timeout};
    use crate::logic::library_groups::album_composite_id;
    use std::sync::{Mutex, mpsc};

    fn seeded(fixture: LibraryFixture) -> (AppContext, SeededLibrary) {
        let ctx = AppContext::in_memory().unwrap();
        let seeded = fixture.seed_on(&ctx.db).recv_blocking().unwrap();
        (ctx, seeded)
    }

    fn deleted_count(ctx: &AppContext, op: OpId) -> usize {
        loop {
            if let Some(result) = ctx.ops.take_result::<usize>(op) {
                return result.unwrap();
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }

    fn titles(songs: &[Arc<SongData>]) -> Vec<String> {
        let mut titles: Vec<String> = songs
            .iter()
            .map(|s| s.title.as_deref().unwrap_or_default().to_string())
            .collect();
        titles.sort();
        titles
    }

    fn found(ctx: &AppContext, params: LibrarySearchParams) -> Vec<Arc<SongData>> {
        let latest = Latest::new();
        search(ctx, params, &latest)
            .recv_blocking()
            .unwrap()
            .unwrap()
    }

    #[test]
    fn the_snapshot_matches_the_seeded_library() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, seeded) = seeded(LibraryFixture::demo());
            let snapshot = load_library(&ctx, LibrarySearchParams::default())
                .recv_blocking()
                .unwrap();
            assert_eq!(snapshot.songs.len(), 11);
            assert_eq!(snapshot.albums.len(), 4);
            assert_eq!(snapshot.folders.len(), seeded.folders.len());

            let night = snapshot
                .albums
                .iter()
                .find(|a| a.title == "Night Signals")
                .unwrap();
            assert_eq!(night.artist, "Aurora Lane");
            assert_eq!(night.genre, "Synthpop");
            assert_eq!(night.year, "2019");
            assert!(seeded.albums.iter().any(|a| a.hash == night.id));
            let summer = snapshot
                .albums
                .iter()
                .find(|a| a.title == "Summer Mix")
                .unwrap();
            assert_eq!(summer.artist, "Various Artists");

            let fixture = LibraryFixture::demo();
            let duration: f64 = fixture.songs().iter().map(|s| s.duration_secs).sum();
            let size: i64 = fixture.songs().iter().map(|s| s.size_bytes).sum();
            assert_eq!(snapshot.stats.songs, 11);
            assert_eq!(snapshot.stats.albums, 4);
            assert_eq!(snapshot.stats.artists, 3);
            assert!((snapshot.stats.duration_secs - duration).abs() < 1e-6);
            assert!((snapshot.stats.size_bytes - size as f64).abs() < 1.0);
            assert_eq!(stats(&ctx).recv_blocking().unwrap(), snapshot.stats);
        });
    }

    #[test]
    fn the_snapshot_songs_follow_the_search_parameters_but_the_albums_do_not() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, _) = seeded(LibraryFixture::demo());
            let snapshot = load_library(
                &ctx,
                LibrarySearchParams {
                    genre: Some("Rock".into()),
                    ..LibrarySearchParams::default()
                },
            )
            .recv_blocking()
            .unwrap();
            assert_eq!(snapshot.songs.len(), 3);
            assert_eq!(snapshot.albums.len(), 4);
            assert_eq!(snapshot.stats.songs, 11);
        });
    }

    #[test]
    fn searching_by_text_finds_matching_titles() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, _) = seeded(LibraryFixture::demo());
            let songs = found(
                &ctx,
                LibrarySearchParams {
                    query: Some("Static".into()),
                    ..LibrarySearchParams::default()
                },
            );
            assert_eq!(titles(&songs), vec!["Morning Static", "Static Bloom"]);
        });
    }

    #[test]
    fn searching_by_each_filter_narrows_the_songs() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, seeded) = seeded(LibraryFixture::demo());
            let by = |params: LibrarySearchParams| found(&ctx, params).len();
            assert_eq!(by(LibrarySearchParams::default()), 11);
            assert_eq!(
                by(LibrarySearchParams {
                    artist: Some("Aurora Lane".into()),
                    ..LibrarySearchParams::default()
                }),
                6
            );
            assert_eq!(
                by(LibrarySearchParams {
                    album: Some("Left | Right".into()),
                    ..LibrarySearchParams::default()
                }),
                3
            );
            assert_eq!(
                by(LibrarySearchParams {
                    genre: Some("Pop".into()),
                    ..LibrarySearchParams::default()
                }),
                2
            );
            assert_eq!(
                by(LibrarySearchParams {
                    year: Some("2021".into()),
                    ..LibrarySearchParams::default()
                }),
                2
            );
            let summer_folder = seeded
                .folders
                .iter()
                .find(|(_, path)| path.ends_with("Summer Mix"))
                .unwrap()
                .0;
            assert_eq!(
                by(LibrarySearchParams {
                    folder_id: Some(summer_folder),
                    ..LibrarySearchParams::default()
                }),
                2
            );
        });
    }

    #[test]
    fn an_older_search_reply_is_dropped_when_a_newer_search_was_issued() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, _) = seeded(LibraryFixture::demo());
            let latest = Latest::new();
            let older = search(&ctx, LibrarySearchParams::default(), &latest);
            let newer = search(
                &ctx,
                LibrarySearchParams {
                    genre: Some("Rock".into()),
                    ..LibrarySearchParams::default()
                },
                &latest,
            );
            assert!(older.recv_blocking().is_none());
            assert_eq!(newer.recv_blocking().unwrap().unwrap().len(), 3);
        });
    }

    #[test]
    fn album_songs_by_composite_id_handles_a_pipe_in_the_artist_name() {
        with_timeout(TEST_TIMEOUT, || {
            let fixture = LibraryFixture::new()
                .song(FixtureSong::new("/m/a/1.flac", "One", "A | B", "Pipes"))
                .song(FixtureSong::new("/m/a/2.flac", "Two", "A | B", "Pipes"))
                .song(FixtureSong::new("/m/c/1.flac", "Other", "C", "Plain"));
            let (ctx, seeded) = seeded(fixture);
            let album = seeded.albums.iter().find(|a| a.title == "Pipes").unwrap();
            assert_eq!(album.artist, "A | B");
            let id = album_composite_id(&album.artist, &album.hash);
            assert!(parse_album_composite_id(&id).is_some());

            let songs = album_songs_by_composite_id(&ctx, &id)
                .recv_blocking()
                .unwrap();
            assert_eq!(titles(&songs), vec!["One", "Two"]);

            let direct = album_songs(&ctx, &album.artist, &album.hash)
                .recv_blocking()
                .unwrap();
            assert_eq!(titles(&direct), vec!["One", "Two"]);

            let wrong_artist = album_songs(&ctx, "Nobody", &album.hash)
                .recv_blocking()
                .unwrap();
            assert!(wrong_artist.is_empty());
        });
    }

    #[test]
    fn an_id_that_does_not_parse_falls_back_to_the_bare_album_hash() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, seeded) = seeded(LibraryFixture::demo());
            let album = seeded
                .albums
                .iter()
                .find(|a| a.title == "Night Signals")
                .unwrap();
            assert!(parse_album_composite_id(&album.hash).is_none());
            let songs = album_songs_by_composite_id(&ctx, &album.hash)
                .recv_blocking()
                .unwrap();
            assert_eq!(songs.len(), 4);
        });
    }

    #[test]
    fn deleting_songs_removes_them_cleans_the_metadata_and_publishes_the_events() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, seeded) = seeded(LibraryFixture::demo());
            let (tx, rx) = mpsc::channel();
            let tx = Mutex::new(tx);
            let tx_ops = Mutex::new(tx.lock().unwrap().clone());
            let _library = ctx.hub.subscribe(
                Origin::Library,
                Arc::new(move |event| {
                    let _ = tx.lock().unwrap().send(format!("{event:?}"));
                }),
            );
            let _ops = ctx.hub.subscribe(
                Origin::LongOperation,
                Arc::new(move |event| {
                    if let Event::LongOperation(LongOpEvent::Completed(_)) = event {
                        let _ = tx_ops.lock().unwrap().send("completed".to_string());
                    }
                }),
            );

            let ids: Vec<i64> = seeded
                .songs
                .iter()
                .filter(|s| s.path.contains("Night Signals"))
                .map(|s| s.id)
                .collect();
            assert_eq!(ids.len(), 4);
            let op = delete_songs(&ctx, ids);
            assert_eq!(ctx.ops.kind(op), Some(LongOpKind::LibraryDelete));

            let mut seen = Vec::new();
            while !seen.iter().any(|e| e == "completed") {
                seen.push(rx.recv_timeout(TEST_TIMEOUT).unwrap());
            }
            assert!(seen.iter().any(|e| e.contains("Changed")));
            assert_eq!(ctx.ops.take_result::<usize>(op).unwrap().unwrap(), 4);

            let snapshot = load_library(&ctx, LibrarySearchParams::default())
                .recv_blocking()
                .unwrap();
            assert_eq!(snapshot.songs.len(), 7);
            assert_eq!(snapshot.albums.len(), 3);
            assert!(snapshot.albums.iter().all(|a| a.title != "Night Signals"));
            assert_eq!(snapshot.stats.songs, 7);
            // Aurora Lane still has the Daybreak EP, so the artist stays.
            assert_eq!(snapshot.stats.artists, 3);
        });
    }

    #[test]
    fn deleting_an_artists_last_songs_removes_the_artist() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, seeded) = seeded(LibraryFixture::demo());
            let ids: Vec<i64> = seeded
                .songs
                .iter()
                .filter(|s| s.path.contains("Aurora Lane"))
                .map(|s| s.id)
                .collect();
            let op = delete_songs(&ctx, ids);
            assert_eq!(deleted_count(&ctx, op), 6);
            let stats = stats(&ctx).recv_blocking().unwrap();
            assert_eq!(stats.songs, 5);
            assert_eq!(stats.artists, 2);
            let artists = ctx
                .db
                .read(|db| Ok(db.get_filter_hierarchy("Artist")?))
                .recv_blocking()
                .unwrap();
            assert!(artists.iter().all(|(l1, _, _)| &**l1 != "Aurora Lane"));
        });
    }

    #[test]
    fn deleting_nothing_succeeds_without_an_event() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, _) = seeded(LibraryFixture::demo());
            let op = delete_songs(&ctx, Vec::new());
            assert_eq!(deleted_count(&ctx, op), 0);
            assert_eq!(stats(&ctx).recv_blocking().unwrap().songs, 11);
        });
    }

    #[test]
    fn orphan_cover_files_are_removed_and_missing_ones_ignored() {
        let dir = std::env::temp_dir().join(format!("audoxidy-covers-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("gone.avif"), b"x").unwrap();
        std::fs::write(dir.join("kept.avif"), b"x").unwrap();
        remove_cover_files(&dir, &["gone".to_string(), "never-existed".to_string()]);
        assert!(!dir.join("gone.avif").exists());
        assert!(dir.join("kept.avif").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_cover_still_used_by_a_remaining_song_is_not_an_orphan() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, seeded) = seeded(LibraryFixture::demo());
            let night = seeded
                .albums
                .iter()
                .find(|a| a.title == "Night Signals")
                .unwrap()
                .hash
                .clone();
            let one =
                seeded.song_id("/fixtures/music/Aurora Lane/Night Signals/01 - Static Bloom.flac");
            let orphans = ctx
                .db
                .write(move |db| delete_in_database(db, &[one.unwrap()]))
                .recv_blocking()
                .unwrap();
            assert_eq!(orphans.0, 1);
            assert!(!orphans.1.contains(&night));
        });
    }

    #[test]
    fn scanning_an_empty_path_is_refused() {
        with_timeout(TEST_TIMEOUT, || {
            let ctx = AppContext::in_memory().unwrap();
            assert!(matches!(
                scan_folder(&ctx, "  "),
                Err(FrontendError::InvalidInput(_))
            ));
        });
    }

    #[test]
    fn scanning_a_folder_runs_the_scanner_in_the_background() {
        with_timeout(TEST_TIMEOUT, || {
            let ctx = AppContext::in_memory().unwrap();
            let (root, music) = scan_root();
            let started = std::time::Instant::now();
            scan_folder(&ctx, &root).unwrap();
            assert!(started.elapsed() < TEST_TIMEOUT, "the call does not wait");
            // The scan of a folder without songs ends on its own.
            while ctx
                .scanner
                .is_scanning
                .load(std::sync::atomic::Ordering::SeqCst)
            {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            let _ = std::fs::remove_dir_all(music);
        });
    }

    fn scan_root() -> (String, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("audoxidy-scan-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        (dir.to_string_lossy().into_owned(), dir)
    }
}
