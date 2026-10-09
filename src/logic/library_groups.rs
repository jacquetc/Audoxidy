//! Grouping of library songs by artist, the album grid of a filtered library,
//! and the library / selection statistics.
//!
//! Extracted from the Iced `LibraryManager::update_processed_data`,
//! `update_selection_stats` and `get_songs_for_album_id`.

use crate::db::database::SongData;
use crate::logic::id_codec;
use crate::logic::library_sort::sort_albums;
use crate::utils::{SortColumn, get_effective_artist};
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

/// Album name used when a song has no album tag (for statistics and album lookups).
pub const UNKNOWN_ALBUM: &str = "Desconocido";

/// Aggregate figures shown in the library status bar.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LibraryStats {
    /// Number of songs.
    pub songs: u64,
    /// Number of distinct (effective artist, album) pairs.
    pub albums: u64,
    /// Number of distinct effective artists.
    pub artists: u64,
    /// Total duration in seconds (missing durations count as 0).
    pub duration_secs: f64,
    /// Total size in bytes (missing sizes count as 0).
    pub size_bytes: f64,
}

/// The songs of one artist, as shown by an artist header in the list views.
#[derive(Debug, Clone)]
pub struct ArtistGroup {
    /// Display name: the effective artist of the first song seen for this group.
    pub name: String,
    /// Number of distinct album titles among the songs (untagged songs not counted).
    pub albums_count: usize,
    /// Number of songs.
    pub songs_count: usize,
    /// The songs, in input order.
    pub songs: Vec<Arc<SongData>>,
    /// Total duration in seconds.
    pub duration_secs: f64,
}

/// An album of the library, as listed by the album grid.
#[derive(Debug, Clone, PartialEq)]
pub struct AlbumEntry {
    /// Album identifier (the hash used in composite album ids).
    pub id: String,
    /// Album title.
    pub title: String,
    /// Album artist.
    pub artist: String,
    /// Genre.
    pub genre: String,
    /// Release year.
    pub year: String,
    /// Path of the cached cover, if any.
    pub cover_path: Option<String>,
}

/// The composite id of an album in the list views: `"{artist}|{album_hash}"`,
/// with `|` and `\` escaped inside each part by [`id_codec::join_parts`]. For
/// names without those characters it is the plain `"{artist}|{album_hash}"`.
pub fn album_composite_id(artist: &str, album_hash: &str) -> String {
    id_codec::join_parts(&[artist, album_hash])
}

/// The `(artist, album_hash)` of a composite album id made by
/// [`album_composite_id`]; `None` unless the id has exactly two parts.
pub fn parse_album_composite_id(composite_id: &str) -> Option<(String, String)> {
    let mut parts = id_codec::split_parts(composite_id).into_iter();
    match (parts.next(), parts.next(), parts.next()) {
        (Some(artist), Some(album_hash), None) => Some((artist, album_hash)),
        _ => None,
    }
}

/// Groups songs by effective artist, case-insensitively.
///
/// Groups are ordered by the lowercased effective artist (byte order). Each
/// group keeps its songs in input order and takes its display name from the
/// first song seen.
pub fn group_by_artist(songs: &[Arc<SongData>]) -> Vec<ArtistGroup> {
    let mut groups_map: BTreeMap<String, ArtistGroup> = BTreeMap::new();
    for song in songs {
        let artist_name = get_effective_artist(song);
        let group = groups_map
            .entry(artist_name.to_lowercase())
            .or_insert_with(|| ArtistGroup {
                name: artist_name.to_string(),
                albums_count: 0,
                songs_count: 0,
                songs: Vec::new(),
                duration_secs: 0.0,
            });
        group.duration_secs += song.duration_secs.unwrap_or(0.0);
        group.songs.push(song.clone());
    }

    for group in groups_map.values_mut() {
        let unique_albums: HashSet<&Arc<str>> = group
            .songs
            .iter()
            .filter_map(|s| s.album.as_ref())
            .collect();
        group.albums_count = unique_albums.len();
        group.songs_count = group.songs.len();
    }

    groups_map.into_values().collect()
}

/// The album grid of a filtered library.
///
/// Walks the groups' songs in order and keeps one album per
/// (lowercased group name, trimmed lowercased album title). Each album is
/// looked up in `cached_albums`, first by exact title and group name, then by
/// exact title alone; the entry found is cloned with its artist replaced by the
/// group name. Albums not found in the cache are skipped, and so are songs
/// without an album tag. The result is sorted with [`sort_albums`].
pub fn filtered_albums(
    groups: &[ArtistGroup],
    cached_albums: &[AlbumEntry],
    sort_column: Option<SortColumn>,
    sort_ascending: Option<bool>,
) -> Vec<AlbumEntry> {
    let mut grid_filtered = Vec::new();
    let mut seen_albums = HashSet::new();

    for group in groups {
        for song in &group.songs {
            let Some(alb_title) = &song.album else {
                continue;
            };
            let key = (group.name.to_lowercase(), alb_title.trim().to_lowercase());
            if !seen_albums.insert(key) {
                continue;
            }
            let entry = cached_albums
                .iter()
                .find(|a| a.title.as_str() == &**alb_title && a.artist == group.name)
                .or_else(|| {
                    cached_albums
                        .iter()
                        .find(|a| a.title.as_str() == &**alb_title)
                });
            if let Some(entry) = entry {
                let mut unified_entry = entry.clone();
                unified_entry.artist = group.name.clone();
                grid_filtered.push(unified_entry);
            }
        }
    }

    sort_albums(&mut grid_filtered, sort_column, sort_ascending);
    grid_filtered
}

/// The composite id of the album a song belongs to, from its tags.
///
/// The artist is the song's effective artist and the album its album tag
/// ([`UNKNOWN_ALBUM`] when missing). The album hash is the id of the first
/// cached album with that exact title and artist, or the album name itself
/// when there is no such album (or no cache).
pub fn album_composite_id_for_song(
    song: &SongData,
    cached_albums: Option<&[AlbumEntry]>,
) -> String {
    let artist = get_effective_artist(song);
    let album_name = song.album.as_deref().unwrap_or(UNKNOWN_ALBUM);
    let album_hash = cached_albums
        .and_then(|all| {
            all.iter()
                .find(|a| a.title == album_name && a.artist == artist)
        })
        .map(|a| a.id.clone())
        .unwrap_or_else(|| album_name.to_string());
    album_composite_id(artist, &album_hash)
}

/// Statistics of a list of songs.
///
/// Albums are distinct (effective artist, album) pairs, with untagged songs
/// counted under [`UNKNOWN_ALBUM`]; artists are distinct effective artists.
pub fn compute_stats(songs: &[Arc<SongData>]) -> LibraryStats {
    let duration_secs: f64 = songs.iter().map(|s| s.duration_secs.unwrap_or(0.0)).sum();
    let size_bytes: f64 = songs.iter().map(|s| s.size.unwrap_or(0) as f64).sum();
    let mut unique_albums: HashSet<(&str, &str)> = HashSet::new();
    let mut unique_artists: HashSet<&str> = HashSet::new();
    for song in songs {
        let artist = get_effective_artist(song);
        let album = song.album.as_deref().unwrap_or(UNKNOWN_ALBUM);
        unique_albums.insert((artist, album));
        unique_artists.insert(artist);
    }
    LibraryStats {
        songs: songs.len() as u64,
        albums: unique_albums.len() as u64,
        artists: unique_artists.len() as u64,
        duration_secs,
        size_bytes,
    }
}

/// Statistics of the selected songs, or `None` when nothing is selected
/// (the status bar then falls back to the filter statistics).
pub fn selection_stats(selected_songs: &[Arc<SongData>]) -> Option<LibraryStats> {
    if selected_songs.is_empty() {
        None
    } else {
        Some(compute_stats(selected_songs))
    }
}

/// The songs of the album with composite id `"{artist}|{album_hash}"` (as
/// made by [`album_composite_id`]).
///
/// The id must have exactly two parts (see [`parse_album_composite_id`]). The
/// album is looked up in `cached_albums` by hash and artist; the result is
/// every song of `songs` whose album tag equals the album title and whose
/// effective artist equals the id's artist, in `songs` order. Returns `None`
/// for a malformed id or an album that is not in the cache.
pub fn songs_for_album_id(
    composite_id: &str,
    songs: &[Arc<SongData>],
    cached_albums: &[AlbumEntry],
) -> Option<Vec<Arc<SongData>>> {
    let (artist, album_id) = parse_album_composite_id(composite_id)?;
    let album = cached_albums
        .iter()
        .find(|a| a.id == album_id && a.artist == artist)?;
    Some(
        songs
            .iter()
            .filter(|s| {
                s.album.as_deref() == Some(album.title.as_str())
                    && get_effective_artist(s) == artist
            })
            .cloned()
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(
        id: i64,
        artist: Option<&str>,
        album: Option<&str>,
        duration: Option<f64>,
    ) -> Arc<SongData> {
        Arc::new(SongData {
            id,
            artist: artist.map(Arc::from),
            album: album.map(Arc::from),
            duration_secs: duration,
            size: Some(id * 100),
            ..SongData::default()
        })
    }

    fn album(id: &str, title: &str, artist: &str, year: &str) -> AlbumEntry {
        AlbumEntry {
            id: id.to_string(),
            title: title.to_string(),
            artist: artist.to_string(),
            genre: String::new(),
            year: year.to_string(),
            cover_path: None,
        }
    }

    #[test]
    fn the_composite_album_id_joins_artist_and_hash_with_a_pipe() {
        assert_eq!(album_composite_id("Queen", "abc123"), "Queen|abc123");
    }

    #[test]
    fn a_song_album_id_uses_the_cached_hash_or_falls_back_to_the_album_name() {
        let cached = vec![
            album("h1", "Innuendo", "Queen", ""),
            album("h2", "Innuendo", "Queen", ""),
        ];
        let tagged = song(1, Some("Queen"), Some("Innuendo"), None);
        assert_eq!(
            album_composite_id_for_song(&tagged, Some(&cached)),
            "Queen|h1"
        );
        assert_eq!(album_composite_id_for_song(&tagged, None), "Queen|Innuendo");

        let other = song(2, Some("ABBA"), Some("Innuendo"), None);
        assert_eq!(
            album_composite_id_for_song(&other, Some(&cached)),
            "ABBA|Innuendo"
        );

        let untagged = song(3, None, None, None);
        assert_eq!(
            album_composite_id_for_song(&untagged, Some(&cached)),
            "Artista Desconocido|Desconocido"
        );
    }

    #[test]
    fn songs_are_grouped_by_lowercased_effective_artist_keeping_the_first_name_seen() {
        let mut with_album_artist = (*song(4, Some("Guest"), Some("Hits"), Some(5.0))).clone();
        with_album_artist.album_artist = Some(Arc::from("queen"));
        let songs = vec![
            song(1, Some("Queen"), Some("Innuendo"), Some(10.0)),
            song(2, Some("ABBA"), Some("Arrival"), None),
            song(3, Some("QUEEN"), Some("Innuendo"), Some(20.0)),
            Arc::new(with_album_artist),
            song(5, None, None, Some(1.0)),
        ];
        let groups = group_by_artist(&songs);

        let names: Vec<&str> = groups.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, vec!["ABBA", "Artista Desconocido", "Queen"]);

        let queen = &groups[2];
        let queen_ids: Vec<i64> = queen.songs.iter().map(|s| s.id).collect();
        assert_eq!(queen_ids, vec![1, 3, 4]);
        assert_eq!(queen.songs_count, 3);
        assert_eq!(queen.albums_count, 2);
        assert_eq!(queen.duration_secs, 35.0);

        let unknown = &groups[1];
        assert_eq!(unknown.albums_count, 0, "untagged songs add no album");
        assert_eq!(unknown.songs_count, 1);
    }

    #[test]
    fn grouping_no_songs_gives_no_groups() {
        assert!(group_by_artist(&[]).is_empty());
    }

    #[test]
    fn the_filtered_grid_deduplicates_albums_per_group_ignoring_case_and_padding() {
        let songs = vec![
            song(1, Some("Queen"), Some("Innuendo"), None),
            song(2, Some("Queen"), Some(" innuendo "), None),
            song(3, Some("Queen"), Some("Jazz"), None),
            song(4, Some("Queen"), None, None),
        ];
        let groups = group_by_artist(&songs);
        let cached = vec![
            album("h1", "Innuendo", "Queen", "1991"),
            album("h2", "Jazz", "Queen", "1978"),
            album("h3", " innuendo ", "Queen", "1991"),
        ];
        let grid = filtered_albums(&groups, &cached, None, None);
        let ids: Vec<&str> = grid.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, vec!["h2", "h1"], "canonical order: year then title");
    }

    #[test]
    fn the_filtered_grid_falls_back_to_a_title_only_match_and_renames_the_artist() {
        let songs = vec![
            song(1, Some("Freddie"), Some("Barcelona"), None),
            song(2, Some("Freddie"), Some("Missing"), None),
        ];
        let groups = group_by_artist(&songs);
        let cached = vec![
            album("other", "Barcelona", "Montserrat", "1988"),
            album("x", "Unrelated", "Freddie", "1985"),
        ];
        let grid = filtered_albums(&groups, &cached, None, None);
        assert_eq!(grid.len(), 1, "albums missing from the cache are skipped");
        assert_eq!(grid[0].id, "other");
        assert_eq!(grid[0].artist, "Freddie");
    }

    #[test]
    fn the_filtered_grid_prefers_the_album_of_the_same_artist() {
        let songs = vec![song(1, Some("B"), Some("Greatest Hits"), None)];
        let groups = group_by_artist(&songs);
        let cached = vec![
            album("a-hits", "Greatest Hits", "A", ""),
            album("b-hits", "Greatest Hits", "B", ""),
        ];
        let grid = filtered_albums(&groups, &cached, None, None);
        assert_eq!(grid[0].id, "b-hits");
    }

    #[test]
    fn the_filtered_grid_is_sorted_with_the_album_sort() {
        let songs = vec![
            song(1, Some("A"), Some("Zeta"), None),
            song(2, Some("B"), Some("Alpha"), None),
        ];
        let groups = group_by_artist(&songs);
        let cached = vec![album("z", "Zeta", "A", ""), album("a", "Alpha", "B", "")];
        let grid = filtered_albums(&groups, &cached, Some(SortColumn::Album), Some(true));
        let ids: Vec<&str> = grid.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "z"]);
    }

    #[test]
    fn stats_count_distinct_artist_album_pairs_and_sum_durations_and_sizes() {
        let songs = vec![
            song(1, Some("Queen"), Some("Innuendo"), Some(10.0)),
            song(2, Some("Queen"), Some("Innuendo"), None),
            song(3, Some("Queen"), None, Some(5.0)),
            song(4, Some("ABBA"), None, Some(1.5)),
        ];
        let stats = compute_stats(&songs);
        assert_eq!(stats.songs, 4);
        assert_eq!(
            stats.albums, 3,
            "untagged songs count as one album per artist"
        );
        assert_eq!(stats.artists, 2);
        assert_eq!(stats.duration_secs, 16.5);
        assert_eq!(stats.size_bytes, 1000.0);
    }

    #[test]
    fn stats_of_no_songs_are_zero() {
        let stats = compute_stats(&[]);
        assert_eq!(stats.songs, 0);
        assert_eq!(stats.albums, 0);
        assert_eq!(stats.artists, 0);
        assert_eq!(stats.duration_secs, 0.0);
        assert_eq!(stats.size_bytes, 0.0);
    }

    #[test]
    fn an_empty_selection_has_no_stats() {
        assert_eq!(selection_stats(&[]), None);
        let one = vec![song(7, Some("A"), Some("B"), Some(2.0))];
        assert_eq!(selection_stats(&one), Some(compute_stats(&one)));
    }

    #[test]
    fn album_songs_match_the_cached_title_and_the_effective_artist() {
        let mut by_album_artist = (*song(3, Some("Guest"), Some("Innuendo"), None)).clone();
        by_album_artist.album_artist = Some(Arc::from("Queen"));
        let songs = vec![
            song(1, Some("Queen"), Some("Innuendo"), None),
            song(2, Some("Other"), Some("Innuendo"), None),
            Arc::new(by_album_artist),
            song(4, Some("Queen"), Some("Jazz"), None),
        ];
        let cached = vec![album("h1", "Innuendo", "Queen", "")];
        let found = songs_for_album_id("Queen|h1", &songs, &cached).expect("album in cache");
        let ids: Vec<i64> = found.iter().map(|s| s.id).collect();
        assert_eq!(ids, vec![1, 3]);
    }

    #[test]
    fn album_songs_need_a_well_formed_id_and_a_cached_album() {
        let songs = vec![song(1, Some("Queen"), Some("Innuendo"), None)];
        let cached = vec![album("h1", "Innuendo", "Queen", "")];
        assert!(songs_for_album_id("Queen", &songs, &cached).is_none());
        assert!(songs_for_album_id("Queen|h1|extra", &songs, &cached).is_none());
        assert!(songs_for_album_id("Queen|nope", &songs, &cached).is_none());
        assert!(songs_for_album_id("ABBA|h1", &songs, &cached).is_none());
        assert_eq!(
            songs_for_album_id("Queen|h1", &[], &cached).map(|v| v.len()),
            Some(0)
        );
    }

    #[test]
    fn d4_songs_for_album_id_finds_the_album_of_an_artist_named_with_a_pipe() {
        // Before the fix the id `A|B|h1` had three parts, so the lookup failed.
        let songs = vec![
            song(1, Some("A|B"), Some("Live"), None),
            song(2, Some("A"), Some("Live"), None),
            song(3, Some("A|B"), Some("Other"), None),
        ];
        let cached = vec![album("h1", "Live", "A|B", ""), album("h2", "Live", "A", "")];
        let id = album_composite_id("A|B", "h1");
        assert_eq!(id, r"A\|B|h1");
        let found = songs_for_album_id(&id, &songs, &cached).expect("album in cache");
        assert_eq!(found.iter().map(|s| s.id).collect::<Vec<_>>(), vec![1]);
        // The id built from the song's tags is the same one.
        assert_eq!(album_composite_id_for_song(&songs[0], Some(&cached)), id);
    }

    #[test]
    fn d4_album_ids_with_pipes_and_backslashes_round_trip() {
        for (artist, hash) in [
            ("A|B", "h1"),
            (r"AC\DC", "h2"),
            (r"x\|y|", r"Album|Name\"),
            ("Queen", "abc123"),
        ] {
            let id = album_composite_id(artist, hash);
            assert_eq!(
                parse_album_composite_id(&id),
                Some((artist.to_string(), hash.to_string())),
                "id {id:?}"
            );
        }
        // An untagged album falls back to its name as the hash, which is escaped too.
        let untagged = song(1, Some("A|B"), Some("C|D"), None);
        let id = album_composite_id_for_song(&untagged, None);
        assert_eq!(
            parse_album_composite_id(&id),
            Some(("A|B".to_string(), "C|D".to_string()))
        );
        assert_eq!(parse_album_composite_id("Queen"), None);
        assert_eq!(parse_album_composite_id("Queen|h1|extra"), None);
    }

    #[test]
    fn d4_album_ids_without_pipes_or_backslashes_are_unchanged() {
        for (artist, hash) in [("Queen", "abc123"), ("AC/DC", "Back in Black"), ("", "")] {
            assert_eq!(
                album_composite_id(artist, hash),
                format!("{}|{}", artist, hash)
            );
        }
    }
}
