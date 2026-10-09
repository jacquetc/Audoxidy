//! Sorting of library songs and albums, and the three-state column-header cycle.
//!
//! Extracted from the Iced `LibraryManager::sort_songs_static` /
//! `sort_albums_static` and the `LibrarySortChanged` handler. Both UIs sort with
//! these functions so a column click orders the library identically.

use crate::db::database::SongData;
use crate::logic::library_groups::AlbumEntry;
use crate::utils::{
    SortColumn, compare_albums_for_listing, compare_songs_for_listing, compare_strings_ignore_case,
    compare_track_numbers, get_effective_artist,
};
use std::borrow::Borrow;
use std::cmp::Ordering;

/// The library sort state: which column is sorted and in which direction.
///
/// `column == None` means the canonical listing order
/// ([`compare_songs_for_listing`] / [`compare_albums_for_listing`]); the
/// direction is then ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SortState {
    /// The sorted column, or `None` for the canonical order.
    pub column: Option<SortColumn>,
    /// `Some(false)` sorts descending; `None` and `Some(true)` sort ascending.
    pub ascending: Option<bool>,
}

impl Default for SortState {
    /// No column, ascending: the state the library starts in.
    fn default() -> Self {
        Self {
            column: None,
            ascending: Some(true),
        }
    }
}

impl SortState {
    /// The state after a click on the header of `clicked`.
    ///
    /// A different column sorts ascending; the same column while ascending
    /// sorts descending; the same column while descending (or with no
    /// direction) clears the sort, leaving both fields `None`.
    pub fn after_header_click(self, clicked: SortColumn) -> Self {
        if self.column == Some(clicked) {
            if self.ascending == Some(true) {
                Self {
                    column: Some(clicked),
                    ascending: Some(false),
                }
            } else {
                Self {
                    column: None,
                    ascending: None,
                }
            }
        } else {
            Self {
                column: Some(clicked),
                ascending: Some(true),
            }
        }
    }
}

/// Estimated bitrate of a song in kbit/s: `size * 8 / (duration * 1000)`.
///
/// A missing size counts as 0; a missing or non-positive duration gives 0, so
/// such songs sort first in ascending order.
pub fn song_bitrate_kbps(song: &SongData) -> f64 {
    song.duration_secs
        .filter(|d| *d > 0.0)
        .map(|d| song.size.unwrap_or(0) as f64 * 8.0 / (d * 1000.0))
        .unwrap_or(0.0)
}

/// Ascending comparison of two songs on one column, with the column's tie-breakers.
///
/// `AlbumCard` and `AlbumThumbnail` are not sortable and compare equal.
pub fn compare_songs_by_column(a: &SongData, b: &SongData, column: SortColumn) -> Ordering {
    let tracks = || compare_track_numbers(a.track_number.as_deref(), b.track_number.as_deref());
    let years = || compare_strings_ignore_case(str_of(&a.release_year), str_of(&b.release_year));
    let albums = || compare_strings_ignore_case(str_of(&a.album), str_of(&b.album));
    let artists = || compare_strings_ignore_case(get_effective_artist(a), get_effective_artist(b));

    match column {
        SortColumn::TrackNumber => tracks(),
        SortColumn::Title => {
            compare_strings_ignore_case(str_of(&a.title), str_of(&b.title)).then(tracks())
        }
        SortColumn::Artist | SortColumn::AlbumArtist => {
            artists().then(years()).then(albums()).then(tracks())
        }
        SortColumn::Album => albums().then(artists()).then(tracks()),
        SortColumn::Format => compare_strings_ignore_case(str_of(&a.format), str_of(&b.format)),
        SortColumn::Size => a.size.unwrap_or(0).cmp(&b.size.unwrap_or(0)),
        SortColumn::SampleRate => a.sample_rate.unwrap_or(0).cmp(&b.sample_rate.unwrap_or(0)),
        SortColumn::Channels => a.channels.unwrap_or(0).cmp(&b.channels.unwrap_or(0)),
        SortColumn::Bitrate => song_bitrate_kbps(a)
            .partial_cmp(&song_bitrate_kbps(b))
            .unwrap_or(Ordering::Equal),
        SortColumn::Genre => compare_strings_ignore_case(str_of(&a.genre), str_of(&b.genre))
            .then(artists())
            .then(albums())
            .then(tracks()),
        SortColumn::Year => years().then(artists()).then(albums()).then(tracks()),
        SortColumn::Duration => {
            let dur_a = a.duration_secs.unwrap_or(0.0);
            let dur_b = b.duration_secs.unwrap_or(0.0);
            dur_a.partial_cmp(&dur_b).unwrap_or(Ordering::Equal)
        }
        SortColumn::BitDepth => a.bit_depth.unwrap_or(0).cmp(&b.bit_depth.unwrap_or(0)),
        SortColumn::AlbumCard | SortColumn::AlbumThumbnail => Ordering::Equal,
    }
}

/// An optional text field as `&str`, empty when missing.
fn str_of(field: &Option<std::sync::Arc<str>>) -> &str {
    field.as_deref().unwrap_or("")
}

/// Sorts songs in place (stable sort).
///
/// With a column, songs are ordered by [`compare_songs_by_column`], reversed
/// when `ascending == Some(false)`. Without a column they follow the canonical
/// listing order ([`compare_songs_for_listing`]).
pub fn sort_songs<S: Borrow<SongData>>(
    songs: &mut [S],
    column: Option<SortColumn>,
    ascending: Option<bool>,
) {
    if let Some(column) = column {
        let is_asc = ascending.unwrap_or(true);
        songs.sort_by(|a, b| {
            let res = compare_songs_by_column(a.borrow(), b.borrow(), column);
            if is_asc { res } else { res.reverse() }
        });
    } else {
        songs.sort_by(|a, b| compare_songs_for_listing(a.borrow(), b.borrow()));
    }
}

/// Ascending comparison of two albums on one column.
///
/// Only `Album`, `Artist`/`AlbumArtist`, `Genre` and `Year` order albums; every
/// other column compares equal (the stable sort then keeps the current order).
pub fn compare_albums_by_column(a: &AlbumEntry, b: &AlbumEntry, column: SortColumn) -> Ordering {
    match column {
        SortColumn::Album => compare_strings_ignore_case(&a.title, &b.title)
            .then(compare_strings_ignore_case(&a.artist, &b.artist))
            .then(compare_strings_ignore_case(&a.year, &b.year)),
        SortColumn::Artist | SortColumn::AlbumArtist => {
            compare_albums_for_listing(&a.artist, &a.year, &a.title, &b.artist, &b.year, &b.title)
        }
        SortColumn::Genre => compare_strings_ignore_case(&a.genre, &b.genre)
            .then(compare_strings_ignore_case(&a.artist, &b.artist))
            .then(compare_strings_ignore_case(&a.year, &b.year))
            .then(compare_strings_ignore_case(&a.title, &b.title)),
        SortColumn::Year => compare_strings_ignore_case(&a.year, &b.year)
            .then(compare_strings_ignore_case(&a.artist, &b.artist))
            .then(compare_strings_ignore_case(&a.title, &b.title)),
        _ => Ordering::Equal,
    }
}

/// Sorts albums in place (stable sort).
///
/// With a column, albums are ordered by [`compare_albums_by_column`], reversed
/// when `ascending == Some(false)`. Without a column they follow the canonical
/// listing order (artist, year, title).
pub fn sort_albums(albums: &mut [AlbumEntry], column: Option<SortColumn>, ascending: Option<bool>) {
    if let Some(column) = column {
        let is_asc = ascending.unwrap_or(true);
        albums.sort_by(|a, b| {
            let res = compare_albums_by_column(a, b, column);
            if is_asc { res } else { res.reverse() }
        });
    } else {
        albums.sort_by(|a, b| {
            compare_albums_for_listing(&a.artist, &a.year, &a.title, &b.artist, &b.year, &b.title)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn song_with(duration: Option<f64>, size: Option<i64>) -> Arc<SongData> {
        Arc::new(SongData {
            duration_secs: duration,
            size,
            ..SongData::default()
        })
    }

    fn song(id: i64, artist: &str, year: &str, album: &str, track: &str, title: &str) -> SongData {
        SongData {
            id,
            artist: Some(Arc::from(artist)),
            release_year: Some(Arc::from(year)),
            album: Some(Arc::from(album)),
            track_number: Some(Arc::from(track)),
            title: Some(Arc::from(title)),
            ..SongData::default()
        }
    }

    fn album(id: &str, title: &str, artist: &str, genre: &str, year: &str) -> AlbumEntry {
        AlbumEntry {
            id: id.to_string(),
            title: title.to_string(),
            artist: artist.to_string(),
            genre: genre.to_string(),
            year: year.to_string(),
            cover_path: None,
        }
    }

    fn ids(songs: &[SongData]) -> Vec<i64> {
        songs.iter().map(|s| s.id).collect()
    }

    fn album_ids(albums: &[AlbumEntry]) -> Vec<&str> {
        albums.iter().map(|a| a.id.as_str()).collect()
    }

    /// Port of the Iced regression guard `test_bitrate_sort_handles_missing_duration`.
    #[test]
    fn bitrate_sort_handles_a_missing_or_zero_duration_and_is_stable() {
        let mut songs = vec![
            song_with(None, Some(10_000)),
            song_with(Some(0.0), Some(10_000)),
            song_with(Some(120.0), Some(4_000_000)),
            song_with(Some(60.0), Some(500_000)),
        ];

        let sort_snapshot = |songs: &mut Vec<Arc<SongData>>| {
            sort_songs(songs, Some(SortColumn::Bitrate), Some(true));
            songs
                .iter()
                .map(|s| (s.duration_secs, s.size))
                .collect::<Vec<_>>()
        };

        let first = sort_snapshot(&mut songs.clone());
        let second = sort_snapshot(&mut songs);
        assert_eq!(first, second);
        assert_eq!(first[0].0, None);
        assert_eq!(first[1].0, Some(0.0));
        assert_eq!(first[2].0, Some(60.0));
        assert_eq!(first[3].0, Some(120.0));
    }

    #[test]
    fn bitrate_is_size_in_bits_over_duration_in_milliseconds() {
        let s = song_with(Some(10.0), Some(1_250_000));
        assert!((song_bitrate_kbps(&s) - 1000.0).abs() < 1e-9);
        assert_eq!(song_bitrate_kbps(&song_with(Some(10.0), None)), 0.0);
        assert_eq!(song_bitrate_kbps(&song_with(Some(-1.0), Some(5))), 0.0);
    }

    #[test]
    fn without_a_column_songs_follow_the_canonical_listing_order() {
        let mut songs = vec![
            song(1, "b", "2000", "x", "1", "t"),
            song(2, "A", "2001", "y", "2", "t"),
            song(3, "a", "2001", "y", "1", "t"),
            song(4, "a", "1999", "z", "1", "t"),
        ];
        sort_songs(&mut songs, None, Some(false));
        assert_eq!(ids(&songs), vec![4, 3, 2, 1]);
    }

    #[test]
    fn descending_reverses_the_column_comparison() {
        let mut songs = vec![
            song(1, "a", "", "", "2", "B"),
            song(2, "a", "", "", "1", "a"),
            song(3, "a", "", "", "1", "c"),
        ];
        sort_songs(&mut songs, Some(SortColumn::Title), Some(true));
        assert_eq!(ids(&songs), vec![2, 1, 3]);
        sort_songs(&mut songs, Some(SortColumn::Title), Some(false));
        assert_eq!(ids(&songs), vec![3, 1, 2]);
    }

    #[test]
    fn a_missing_direction_sorts_ascending() {
        let mut songs = vec![song(1, "b", "", "", "", ""), song(2, "a", "", "", "", "")];
        sort_songs(&mut songs, Some(SortColumn::Artist), None);
        assert_eq!(ids(&songs), vec![2, 1]);
    }

    #[test]
    fn the_title_sort_breaks_ties_by_track_number() {
        let mut songs = vec![
            song(1, "a", "", "", "10", "Same"),
            song(2, "a", "", "", "2", "same"),
        ];
        sort_songs(&mut songs, Some(SortColumn::Title), Some(true));
        assert_eq!(ids(&songs), vec![2, 1]);
    }

    #[test]
    fn the_artist_sort_uses_the_album_artist_then_year_album_and_track() {
        let mut with_album_artist = song(1, "zz", "2000", "a", "1", "");
        with_album_artist.album_artist = Some(Arc::from("aa"));
        let mut songs = vec![
            song(2, "bb", "2000", "a", "1", ""),
            with_album_artist,
            song(3, "bb", "1990", "z", "1", ""),
            song(4, "bb", "2000", "a", "0", ""),
        ];
        sort_songs(&mut songs, Some(SortColumn::AlbumArtist), Some(true));
        assert_eq!(ids(&songs), vec![1, 3, 4, 2]);
    }

    #[test]
    fn the_album_genre_and_year_sorts_use_their_documented_tie_breakers() {
        let mut a = song(1, "b", "2000", "Same", "1", "");
        a.genre = Some(Arc::from("Rock"));
        let mut b = song(2, "a", "2001", "same", "2", "");
        b.genre = Some(Arc::from("rock"));
        let mut c = song(3, "a", "1999", "Other", "1", "");
        c.genre = Some(Arc::from("Jazz"));

        let mut songs = vec![a.clone(), b.clone(), c.clone()];
        sort_songs(&mut songs, Some(SortColumn::Album), Some(true));
        assert_eq!(ids(&songs), vec![3, 2, 1]);

        let mut songs = vec![a.clone(), b.clone(), c.clone()];
        sort_songs(&mut songs, Some(SortColumn::Genre), Some(true));
        assert_eq!(ids(&songs), vec![3, 2, 1]);

        let mut songs = vec![a, b, c];
        sort_songs(&mut songs, Some(SortColumn::Year), Some(true));
        assert_eq!(ids(&songs), vec![3, 1, 2]);
    }

    #[test]
    fn numeric_columns_treat_missing_values_as_zero() {
        let mut big = song(1, "", "", "", "", "");
        big.size = Some(10);
        big.sample_rate = Some(96_000);
        big.channels = Some(2);
        big.bit_depth = Some(24);
        big.duration_secs = Some(3.0);
        let missing = song(2, "", "", "", "", "");
        for column in [
            SortColumn::Size,
            SortColumn::SampleRate,
            SortColumn::Channels,
            SortColumn::BitDepth,
            SortColumn::Duration,
        ] {
            let mut songs = vec![big.clone(), missing.clone()];
            sort_songs(&mut songs, Some(column), Some(true));
            assert_eq!(ids(&songs), vec![2, 1], "column {column:?}");
        }
    }

    #[test]
    fn the_format_sort_ignores_case() {
        let mut flac = song(1, "", "", "", "", "");
        flac.format = Some(Arc::from("flac"));
        let mut aac = song(2, "", "", "", "", "");
        aac.format = Some(Arc::from("AAC"));
        let mut songs = vec![flac, aac];
        sort_songs(&mut songs, Some(SortColumn::Format), Some(true));
        assert_eq!(ids(&songs), vec![2, 1]);
    }

    #[test]
    fn the_card_columns_keep_the_current_order() {
        let mut songs = vec![song(2, "b", "", "", "", ""), song(1, "a", "", "", "", "")];
        sort_songs(&mut songs, Some(SortColumn::AlbumCard), Some(true));
        assert_eq!(ids(&songs), vec![2, 1]);
        sort_songs(&mut songs, Some(SortColumn::AlbumThumbnail), Some(false));
        assert_eq!(ids(&songs), vec![2, 1]);
    }

    #[test]
    fn without_a_column_albums_follow_artist_year_then_title() {
        let mut albums = vec![
            album("1", "B", "x", "", "2000"),
            album("2", "A", "x", "", "2000"),
            album("3", "Z", "x", "", "1990"),
            album("4", "A", "a", "", "2020"),
        ];
        sort_albums(&mut albums, None, Some(false));
        assert_eq!(album_ids(&albums), vec!["4", "3", "2", "1"]);
    }

    #[test]
    fn albums_sort_by_title_genre_or_year_with_their_tie_breakers() {
        let base = vec![
            album("1", "same", "b", "Rock", "2000"),
            album("2", "Same", "a", "rock", "2001"),
            album("3", "Other", "c", "Jazz", "1999"),
        ];
        let mut albums = base.clone();
        sort_albums(&mut albums, Some(SortColumn::Album), Some(true));
        assert_eq!(album_ids(&albums), vec!["3", "2", "1"]);

        let mut albums = base.clone();
        sort_albums(&mut albums, Some(SortColumn::Genre), Some(true));
        assert_eq!(album_ids(&albums), vec!["3", "2", "1"]);

        let mut albums = base.clone();
        sort_albums(&mut albums, Some(SortColumn::Year), Some(false));
        assert_eq!(album_ids(&albums), vec!["2", "1", "3"]);

        let mut albums = base;
        sort_albums(&mut albums, Some(SortColumn::Artist), Some(true));
        assert_eq!(album_ids(&albums), vec!["2", "1", "3"]);
    }

    #[test]
    fn a_column_that_does_not_apply_to_albums_keeps_their_order() {
        let mut albums = vec![album("2", "b", "", "", ""), album("1", "a", "", "", "")];
        sort_albums(&mut albums, Some(SortColumn::Bitrate), Some(false));
        assert_eq!(album_ids(&albums), vec!["2", "1"]);
    }

    #[test]
    fn clicking_another_column_sorts_it_ascending() {
        let state = SortState::default().after_header_click(SortColumn::Title);
        assert_eq!(
            state,
            SortState {
                column: Some(SortColumn::Title),
                ascending: Some(true)
            }
        );
        let state = state.after_header_click(SortColumn::Year);
        assert_eq!(state.column, Some(SortColumn::Year));
        assert_eq!(state.ascending, Some(true));
    }

    #[test]
    fn clicking_the_sorted_column_cycles_ascending_descending_then_off() {
        let ascending = SortState::default().after_header_click(SortColumn::Size);
        let descending = ascending.after_header_click(SortColumn::Size);
        assert_eq!(descending.column, Some(SortColumn::Size));
        assert_eq!(descending.ascending, Some(false));

        let off = descending.after_header_click(SortColumn::Size);
        assert_eq!(
            off,
            SortState {
                column: None,
                ascending: None
            }
        );

        let again = off.after_header_click(SortColumn::Size);
        assert_eq!(again.ascending, Some(true));
    }

    #[test]
    fn the_default_state_is_unsorted_and_ascending() {
        assert_eq!(
            SortState::default(),
            SortState {
                column: None,
                ascending: Some(true)
            }
        );
    }
}
