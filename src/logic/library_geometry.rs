//! Layout of the library list views (Simple, Thumbnail and Detailed).
//!
//! Extracted from the Iced `LibraryManager::get_view_structure`,
//! `get_visible_elements`, `get_visible_items` (list modes), `get_item_y_range`,
//! `find_album_id_for_song` and `get_scroll_task`. All coordinates are in
//! content space: y = 0 is the top of the first artist header, before
//! scrolling. The grid layout is not handled here.
//!
//! Row model: every artist starts with a header row. Unless the artist is
//! collapsed, its songs follow, grouped into albums of consecutive songs with
//! the same album tag. In Simple and Thumbnail modes an album is a header row
//! followed (when expanded) by one row per song. In Detailed mode an album is a
//! single block: a 323 px cover card next to the album header and its songs,
//! plus a 10 px gap.

use crate::db::database::SongData;
use crate::logic::id_codec;
use crate::logic::library_groups::{AlbumEntry, ArtistGroup, UNKNOWN_ALBUM, album_composite_id};
use crate::logic::library_selection::LibraryListItem;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// How the library is displayed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LibraryViewMode {
    /// Album cards in a grid.
    Grid,
    /// Artist headers, then album blocks with a cover card.
    DetailedList,
    /// Artist and album headers, then song rows with a thumbnail (42 px rows).
    ThumbnailList,
    /// Artist and album headers, then song rows (32 px rows).
    SimpleList,
}

impl LibraryViewMode {
    /// Whether the mode is one of the three list modes.
    pub fn is_list(self) -> bool {
        self != LibraryViewMode::Grid
    }
}

/// Height of the cover card of an expanded album block in Detailed mode.
pub const DETAILED_CARD_HEIGHT: f32 = 323.0;

/// Gap after each album block in Detailed mode.
pub const DETAILED_BLOCK_GAP: f32 = 10.0;

/// Squared distance (px²) a mouse drag must exceed to count as a marquee.
pub const MARQUEE_DRAG_THRESHOLD_SQ: f32 = 9.0;

/// Row heights of a list mode.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ListMetrics {
    /// Height of an artist header row.
    pub artist_header_h: f32,
    /// Height of an album header row.
    pub album_header_h: f32,
    /// Height of a song row.
    pub row_h: f32,
}

impl ListMetrics {
    /// The heights of `mode`: 42 px in Thumbnail mode, 32 px otherwise.
    pub fn for_mode(mode: LibraryViewMode) -> Self {
        let h = if mode == LibraryViewMode::ThumbnailList {
            42.0
        } else {
            32.0
        };
        Self {
            artist_header_h: h,
            album_header_h: h,
            row_h: h,
        }
    }
}

/// A run of consecutive songs of one artist with the same album tag.
#[derive(Debug, Clone)]
pub struct LibraryAlbumGroup {
    /// The album tag ([`UNKNOWN_ALBUM`] when missing).
    pub album_name: String,
    /// The cached album id for (artist, album), or the album name when not cached.
    pub album_hash: String,
    /// The songs with their index across all groups (in display order).
    pub songs: Vec<(Arc<SongData>, usize)>,
    /// Total duration in seconds.
    pub duration_secs: f64,
}

/// An artist of the list views with its albums and its vertical extent.
#[derive(Debug, Clone)]
pub struct LibraryVirtualArtist {
    /// The artist group name.
    pub artist_name: String,
    /// Distinct album titles of the group.
    pub albums_count: usize,
    /// Songs of the group.
    pub songs_count: usize,
    /// Total duration in seconds.
    pub duration_secs: f64,
    /// Whether the artist header is collapsed.
    pub is_collapsed: bool,
    /// Top of the artist header in content space.
    pub top_y: f32,
    /// Height of the header plus, when expanded, all its album blocks.
    pub height: f32,
    /// The album runs of the artist.
    pub albums: Vec<LibraryAlbumGroup>,
}

/// A row of the virtualized list, as the Iced list widgets render it.
#[derive(Debug, Clone)]
pub enum LibraryVirtualRow {
    /// An artist header.
    ArtistHeader {
        /// Artist group name.
        name: String,
        /// Whether the header is collapsed.
        is_collapsed: bool,
        /// Distinct albums of the artist.
        albums_count: usize,
        /// Songs of the artist.
        songs_count: usize,
        /// Total duration in seconds.
        duration_secs: f64,
    },
    /// An album header (Simple/Thumbnail) or a whole album block (Detailed).
    AlbumBlock {
        /// The album tag.
        album_name: String,
        /// The album hash of the composite id.
        album_hash: String,
        /// The artist group name.
        artist_name: String,
        /// Genre of the first song (Detailed mode only; empty otherwise).
        genre: String,
        /// Year of the first song (Detailed mode only; empty otherwise).
        year: String,
        /// Whether the album is expanded.
        is_expanded: bool,
        /// The songs of the block (Detailed mode only; empty otherwise).
        songs: Vec<(Arc<SongData>, usize)>,
        /// Number of songs of the album.
        songs_count: usize,
        /// Total duration in seconds.
        duration_secs: f64,
    },
    /// A song row (Simple/Thumbnail).
    SimpleSong {
        /// The song.
        song: Arc<SongData>,
        /// Its index across all groups.
        global_idx: usize,
    },
}

/// The artist whose header sticks to the top of the viewport.
#[derive(Debug, Clone, PartialEq)]
pub struct StickyArtist {
    /// Artist group name.
    pub name: String,
    /// Whether the header is collapsed.
    pub is_collapsed: bool,
    /// Distinct albums of the artist.
    pub albums_count: usize,
    /// Songs of the artist.
    pub songs_count: usize,
    /// Total duration in seconds.
    pub duration_secs: f64,
}

/// The rows to render for a viewport, with the space taken by culled rows.
#[derive(Debug, Clone)]
pub struct VisibleRows {
    /// Height of the culled rows above the rendered ones.
    pub top_space: f32,
    /// Height of the culled rows below the rendered ones.
    pub bottom_space: f32,
    /// Height of the whole list.
    pub total_height: f32,
    /// The rendered rows, in order.
    pub rows: Vec<LibraryVirtualRow>,
    /// The artist header to pin at the top of the viewport.
    pub sticky: Option<StickyArtist>,
}

/// What a [`ListRow`] shows; the indices point into the artists and albums of
/// [`build_view_structure`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListRowKind {
    /// The header of `artists[artist]`.
    ArtistHeader {
        /// Artist index.
        artist: usize,
    },
    /// The header row of an album (Simple/Thumbnail).
    AlbumHeader {
        /// Artist index.
        artist: usize,
        /// Album index within the artist.
        album: usize,
    },
    /// A song row (Simple/Thumbnail).
    Song {
        /// Artist index.
        artist: usize,
        /// Album index within the artist.
        album: usize,
        /// Song index within the album.
        song: usize,
    },
    /// A whole album block (Detailed).
    AlbumBlock {
        /// Artist index.
        artist: usize,
        /// Album index within the artist.
        album: usize,
    },
}

/// A row of the flat list model, with its position in content space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ListRow {
    /// What the row shows.
    pub kind: ListRowKind,
    /// Top of the row.
    pub top_y: f32,
    /// Height of the row.
    pub height: f32,
}

/// An axis-aligned rectangle in content space.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ContentRect {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width.
    pub width: f32,
    /// Height.
    pub height: f32,
}

impl ContentRect {
    /// A rectangle from its top-left corner and size.
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Whether the two rectangles overlap with a positive area (touching edges do not count).
    pub fn intersects(&self, other: &ContentRect) -> bool {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let lower_right_x = (self.x + self.width).min(other.x + other.width);
        let lower_right_y = (self.y + self.height).min(other.y + other.height);
        lower_right_x - x > 0.0 && lower_right_y - y > 0.0
    }
}

/// The marquee rectangle of a drag from `start` to `end` (content space), or
/// `None` when the drag is too short to be a marquee
/// ([`MARQUEE_DRAG_THRESHOLD_SQ`]). Each side is at least 1 px.
pub fn marquee_rect(start: (f32, f32), end: (f32, f32)) -> Option<ContentRect> {
    let dx = start.0 - end.0;
    let dy = start.1 - end.1;
    if dx * dx + dy * dy <= MARQUEE_DRAG_THRESHOLD_SQ {
        return None;
    }
    Some(ContentRect::new(
        start.0.min(end.0),
        start.1.min(end.1),
        dx.abs().max(1.0),
        dy.abs().max(1.0),
    ))
}

/// Height of one album in the list views.
///
/// Simple/Thumbnail: the album header, plus one row per song when expanded.
/// Detailed: `max(323, header + songs * 32) + 10` when expanded, `32 + 10`
/// when collapsed.
pub fn album_block_height(mode: LibraryViewMode, is_expanded: bool, song_count: usize) -> f32 {
    let metrics = ListMetrics::for_mode(mode);
    let right_h = if is_expanded {
        metrics.album_header_h + song_count as f32 * metrics.row_h
    } else {
        metrics.album_header_h
    };
    if mode == LibraryViewMode::DetailedList {
        let card_h: f32 = if is_expanded {
            DETAILED_CARD_HEIGHT
        } else {
            0.0
        };
        card_h.max(right_h) + DETAILED_BLOCK_GAP
    } else {
        right_h
    }
}

/// Builds the artists of the list views and the total content height.
///
/// Each group's songs are split into runs of consecutive songs with the same
/// album tag. A run's album hash is the id of the cached album with the same
/// artist and title (when several cached albums match, the last one wins), or
/// the album name. Song indices count across all groups in order. An artist's
/// height is its header plus, unless collapsed, its album blocks
/// ([`album_block_height`]; an album is expanded unless its composite id is in
/// `collapsed_albums`).
pub fn build_view_structure(
    groups: &[ArtistGroup],
    cached_albums: Option<&[AlbumEntry]>,
    collapsed_artists: &HashSet<String>,
    collapsed_albums: &HashSet<String>,
    mode: LibraryViewMode,
) -> (f32, Vec<LibraryVirtualArtist>) {
    let mut album_hash_map: HashMap<(&str, &str), &str> = HashMap::new();
    for album in cached_albums.unwrap_or_default() {
        album_hash_map.insert(
            (album.artist.as_str(), album.title.as_str()),
            album.id.as_str(),
        );
    }

    let header_h = ListMetrics::for_mode(mode).artist_header_h;
    let mut v_artists = Vec::with_capacity(groups.len());
    let mut total_content_h = 0.0;
    let mut global_song_idx = 0;

    for group in groups {
        let is_artist_collapsed = collapsed_artists.contains(&group.name);
        let mut albums: Vec<LibraryAlbumGroup> = Vec::new();

        for song in &group.songs {
            let album_name = song.album.as_deref().unwrap_or(UNKNOWN_ALBUM);
            if let Some(last) = albums.last_mut()
                && last.album_name == album_name
            {
                last.duration_secs += song.duration_secs.unwrap_or(0.0);
                last.songs.push((song.clone(), global_song_idx));
                global_song_idx += 1;
                continue;
            }
            let album_hash = album_hash_map
                .get(&(group.name.as_str(), album_name))
                .map_or_else(|| album_name.to_string(), |id| id.to_string());
            albums.push(LibraryAlbumGroup {
                album_name: album_name.to_string(),
                album_hash,
                songs: vec![(song.clone(), global_song_idx)],
                duration_secs: song.duration_secs.unwrap_or(0.0),
            });
            global_song_idx += 1;
        }

        let mut artist_h = header_h;
        if !is_artist_collapsed {
            for album in &albums {
                let composite_id = album_composite_id(&group.name, &album.album_hash);
                let is_expanded = !collapsed_albums.contains(&composite_id);
                artist_h += album_block_height(mode, is_expanded, album.songs.len());
            }
        }

        v_artists.push(LibraryVirtualArtist {
            artist_name: group.name.clone(),
            albums_count: group.albums_count,
            songs_count: group.songs_count,
            duration_secs: group.duration_secs,
            is_collapsed: is_artist_collapsed,
            top_y: total_content_h,
            height: artist_h,
            albums,
        });
        total_content_h += artist_h;
    }

    (total_content_h, v_artists)
}

/// Whether the album `album` of `artist` is expanded.
fn is_album_expanded(
    artist: &LibraryVirtualArtist,
    album: &LibraryAlbumGroup,
    collapsed_albums: &HashSet<String>,
) -> bool {
    !collapsed_albums.contains(&album_composite_id(&artist.artist_name, &album.album_hash))
}

/// Extra space rendered above and below the viewport: half the viewport
/// (a quarter in low-resource mode), at least 50 px.
pub fn render_margin(viewport_h: f32, low_resource: bool) -> f32 {
    let ratio: f32 = if low_resource { 0.25 } else { 0.5 };
    (viewport_h * ratio).max(50.0)
}

/// The scroll offset clamped to the scrollable range `[.., total - viewport]`.
pub fn clamped_scroll(total_h: f32, viewport_h: f32, scroll_y: f32) -> f32 {
    scroll_y.min((total_h - viewport_h).max(0.0))
}

/// Index of the artist whose header sticks to the top of the viewport: the
/// last artist whose header top is at or above `scroll_y + header_h - 1`.
pub fn sticky_artist_index(
    artists: &[LibraryVirtualArtist],
    scroll_y: f32,
    mode: LibraryViewMode,
) -> Option<usize> {
    let threshold = ListMetrics::for_mode(mode).artist_header_h - 1.0;
    let mut sticky = None;
    for (i, artist) in artists.iter().enumerate() {
        if artist.top_y <= scroll_y + threshold {
            sticky = Some(i);
        }
    }
    sticky
}

/// The rows to render for a viewport (the Iced list virtualization).
///
/// `scroll_y` is clamped with [`clamped_scroll`]; rows overlapping
/// `[scroll - margin, scroll + viewport + margin]` are rendered and the others
/// are summed into `top_space` / `bottom_space`. In Simple/Thumbnail modes
/// artist headers, album headers and song rows are culled one by one; in
/// Detailed mode whole album blocks are. The sticky artist is resolved with
/// [`sticky_artist_index`] on the clamped scroll offset.
pub fn visible_rows(
    total_h: f32,
    artists: &[LibraryVirtualArtist],
    collapsed_albums: &HashSet<String>,
    mode: LibraryViewMode,
    scroll_y: f32,
    viewport_h: f32,
    margin: f32,
) -> VisibleRows {
    let metrics = ListMetrics::for_mode(mode);
    let header_h = metrics.artist_header_h;
    let album_header_h = metrics.album_header_h;
    let row_height = metrics.row_h;

    let view_min = clamped_scroll(total_h, viewport_h, scroll_y);
    let view_max = view_min + viewport_h;
    let render_min = view_min - margin;
    let render_max = view_max + margin;

    let mut top_space = 0.0;
    let mut bottom_space = 0.0;
    let mut rows = Vec::new();

    for va in artists {
        let a_end = va.top_y + va.height;
        if a_end < render_min {
            top_space += va.height;
            continue;
        }
        if va.top_y > render_max {
            bottom_space += va.height;
            continue;
        }

        let ah_end = va.top_y + header_h;
        if ah_end >= render_min && va.top_y <= render_max {
            rows.push(LibraryVirtualRow::ArtistHeader {
                name: va.artist_name.clone(),
                is_collapsed: va.is_collapsed,
                albums_count: va.albums_count,
                songs_count: va.songs_count,
                duration_secs: va.duration_secs,
            });
        } else if ah_end < render_min {
            top_space += header_h;
        } else {
            bottom_space += header_h;
        }

        if va.is_collapsed {
            continue;
        }
        let mut current_y = va.top_y + header_h;
        for alb in &va.albums {
            let is_expanded = is_album_expanded(va, alb, collapsed_albums);
            let block_h = album_block_height(mode, is_expanded, alb.songs.len());
            let block_end = current_y + block_h;

            if block_end >= render_min && current_y <= render_max {
                if mode != LibraryViewMode::DetailedList {
                    if current_y + album_header_h >= render_min && current_y <= render_max {
                        rows.push(LibraryVirtualRow::AlbumBlock {
                            album_name: alb.album_name.clone(),
                            album_hash: alb.album_hash.clone(),
                            artist_name: va.artist_name.clone(),
                            genre: String::new(),
                            year: String::new(),
                            is_expanded,
                            songs: Vec::new(),
                            songs_count: alb.songs.len(),
                            duration_secs: alb.duration_secs,
                        });
                    } else if current_y + album_header_h < render_min {
                        top_space += album_header_h;
                    } else {
                        bottom_space += album_header_h;
                    }

                    if is_expanded {
                        let mut song_y = current_y + album_header_h;
                        for (song, song_i) in &alb.songs {
                            if song_y + row_height >= render_min && song_y <= render_max {
                                rows.push(LibraryVirtualRow::SimpleSong {
                                    song: song.clone(),
                                    global_idx: *song_i,
                                });
                            } else if song_y + row_height < render_min {
                                top_space += row_height;
                            } else {
                                bottom_space += row_height;
                            }
                            song_y += row_height;
                        }
                    }
                } else {
                    let first = alb.songs.first().map(|(s, _)| s);
                    rows.push(LibraryVirtualRow::AlbumBlock {
                        album_name: alb.album_name.clone(),
                        album_hash: alb.album_hash.clone(),
                        artist_name: va.artist_name.clone(),
                        genre: first
                            .and_then(|s| s.genre.as_deref())
                            .map(str::to_string)
                            .unwrap_or_default(),
                        year: first
                            .and_then(|s| s.release_year.as_deref())
                            .map(str::to_string)
                            .unwrap_or_default(),
                        is_expanded,
                        songs: alb.songs.clone(),
                        songs_count: alb.songs.len(),
                        duration_secs: alb.duration_secs,
                    });
                }
            } else if block_end < render_min {
                top_space += block_h;
            } else {
                bottom_space += block_h;
            }
            current_y += block_h;
        }
    }

    let sticky = sticky_artist_index(artists, view_min, mode).map(|i| {
        let va = &artists[i];
        StickyArtist {
            name: va.artist_name.clone(),
            is_collapsed: va.is_collapsed,
            albums_count: va.albums_count,
            songs_count: va.songs_count,
            duration_secs: va.duration_secs,
        }
    });

    VisibleRows {
        top_space,
        bottom_space,
        total_height: total_h,
        rows,
        sticky,
    }
}

/// The flat row model of the list views, each row with its `top_y`.
///
/// Simple/Thumbnail: artist header, then (unless the artist is collapsed) per
/// album a header row and, when expanded, one row per song. Detailed: artist
/// header, then one block per album. The `top_y` of each row is the prefix sum
/// of the heights before it, so the last row ends at the total height of
/// [`build_view_structure`].
pub fn list_rows(
    artists: &[LibraryVirtualArtist],
    collapsed_albums: &HashSet<String>,
    mode: LibraryViewMode,
) -> Vec<ListRow> {
    let metrics = ListMetrics::for_mode(mode);
    let mut rows = Vec::new();
    let mut y = 0.0;
    let mut push = |kind: ListRowKind, height: f32| {
        rows.push(ListRow {
            kind,
            top_y: y,
            height,
        });
        y += height;
    };

    for (ai, va) in artists.iter().enumerate() {
        push(
            ListRowKind::ArtistHeader { artist: ai },
            metrics.artist_header_h,
        );
        if va.is_collapsed {
            continue;
        }
        for (bi, alb) in va.albums.iter().enumerate() {
            let is_expanded = is_album_expanded(va, alb, collapsed_albums);
            if mode == LibraryViewMode::DetailedList {
                push(
                    ListRowKind::AlbumBlock {
                        artist: ai,
                        album: bi,
                    },
                    album_block_height(mode, is_expanded, alb.songs.len()),
                );
                continue;
            }
            push(
                ListRowKind::AlbumHeader {
                    artist: ai,
                    album: bi,
                },
                metrics.album_header_h,
            );
            if is_expanded {
                for si in 0..alb.songs.len() {
                    push(
                        ListRowKind::Song {
                            artist: ai,
                            album: bi,
                            song: si,
                        },
                        metrics.row_h,
                    );
                }
            }
        }
    }
    rows
}

/// Indices of the rows of `rows` (sorted by `top_y`) that overlap the
/// content-space span `[from_y, to_y]`.
pub fn visible_row_range(rows: &[ListRow], from_y: f32, to_y: f32) -> std::ops::Range<usize> {
    let start = rows.partition_point(|r| r.top_y + r.height < from_y);
    let end = rows.partition_point(|r| r.top_y <= to_y);
    start..end.max(start)
}

/// The list items in display order with their rectangles in content space,
/// for marquee hit-testing and keyboard navigation (list modes).
///
/// Artist and album items span the header row; song items span their row. In
/// Detailed mode album and song rectangles sit inside the album block (the
/// header at the block top, songs below it). All rectangles start at x = 0 and
/// are `width` wide.
pub fn list_item_rects(
    artists: &[LibraryVirtualArtist],
    collapsed_albums: &HashSet<String>,
    mode: LibraryViewMode,
    width: f32,
) -> Vec<(LibraryListItem, ContentRect)> {
    let metrics = ListMetrics::for_mode(mode);
    let mut items = Vec::new();
    for va in artists {
        items.push((
            LibraryListItem::Artist(va.artist_name.clone()),
            ContentRect::new(0.0, va.top_y, width, metrics.artist_header_h),
        ));
        if va.is_collapsed {
            continue;
        }
        let mut current_y = va.top_y + metrics.artist_header_h;
        for alb in &va.albums {
            let composite_id = album_composite_id(&va.artist_name, &alb.album_hash);
            let is_expanded = !collapsed_albums.contains(&composite_id);
            let block_h = album_block_height(mode, is_expanded, alb.songs.len());
            items.push((
                LibraryListItem::Album(composite_id),
                ContentRect::new(0.0, current_y, width, metrics.album_header_h),
            ));
            if is_expanded {
                let mut song_y = current_y + metrics.album_header_h;
                for (song, _) in &alb.songs {
                    items.push((
                        LibraryListItem::Song(song.id),
                        ContentRect::new(0.0, song_y, width, metrics.row_h),
                    ));
                    song_y += metrics.row_h;
                }
            }
            current_y += block_h;
        }
    }
    items
}

/// The list items in display order, without geometry (list modes): artists,
/// then for expanded artists their albums and, for expanded albums, their songs.
pub fn list_items(
    artists: &[LibraryVirtualArtist],
    collapsed_albums: &HashSet<String>,
) -> Vec<LibraryListItem> {
    let mut items = Vec::new();
    for va in artists {
        items.push(LibraryListItem::Artist(va.artist_name.clone()));
        if va.is_collapsed {
            continue;
        }
        for alb in &va.albums {
            let composite_id = album_composite_id(&va.artist_name, &alb.album_hash);
            let is_expanded = !collapsed_albums.contains(&composite_id);
            items.push(LibraryListItem::Album(composite_id));
            if is_expanded {
                items.extend(alb.songs.iter().map(|(s, _)| LibraryListItem::Song(s.id)));
            }
        }
    }
    items
}

/// The vertical span `(top_y, height)` of an item in content space (list modes).
///
/// - Artist: the header of the first artist with that name.
/// - Album: the header of the first album with that hash in the first artist
///   with the id's artist name. The id is split into its parts with
///   [`id_codec::split_parts`]; the first two are used. The album is found
///   even when its artist is collapsed.
/// - Song: the first row of that song in an expanded album of an expanded artist.
pub fn item_y_range(
    item: &LibraryListItem,
    artists: &[LibraryVirtualArtist],
    collapsed_albums: &HashSet<String>,
    mode: LibraryViewMode,
) -> Option<(f32, f32)> {
    let metrics = ListMetrics::for_mode(mode);
    match item {
        LibraryListItem::Artist(name) => artists
            .iter()
            .find(|va| va.artist_name == *name)
            .map(|va| (va.top_y, metrics.artist_header_h)),
        LibraryListItem::Album(composite_id) => {
            let mut parts = id_codec::split_parts(composite_id).into_iter();
            let (artist_name, album_hash) = (parts.next()?, parts.next()?);
            let va = artists.iter().find(|va| va.artist_name == artist_name)?;
            let mut current_y = va.top_y + metrics.artist_header_h;
            for alb in &va.albums {
                if alb.album_hash == album_hash {
                    return Some((current_y, metrics.album_header_h));
                }
                let is_expanded = is_album_expanded(va, alb, collapsed_albums);
                current_y += album_block_height(mode, is_expanded, alb.songs.len());
            }
            None
        }
        LibraryListItem::Song(song_id) => {
            for va in artists.iter().filter(|va| !va.is_collapsed) {
                let mut current_y = va.top_y + metrics.artist_header_h;
                for alb in &va.albums {
                    let is_expanded = is_album_expanded(va, alb, collapsed_albums);
                    if is_expanded {
                        let mut song_y = current_y + metrics.album_header_h;
                        for (song, _) in &alb.songs {
                            if song.id == *song_id {
                                return Some((song_y, metrics.row_h));
                            }
                            song_y += metrics.row_h;
                        }
                    }
                    current_y += album_block_height(mode, is_expanded, alb.songs.len());
                }
            }
            None
        }
    }
}

/// The composite id of the first album (in display order) that contains the song.
pub fn album_id_for_song(artists: &[LibraryVirtualArtist], song_id: i64) -> Option<String> {
    artists.iter().find_map(|va| {
        va.albums
            .iter()
            .find(|alb| alb.songs.iter().any(|(s, _)| s.id == song_id))
            .map(|alb| album_composite_id(&va.artist_name, &alb.album_hash))
    })
}

/// [`build_view_structure`] for the Teksilo UI, with defects D10 and D13 fixed
/// (`docs/teksilo-port-iced-defects.md`; quirk fixes 38 and 41).
///
/// - **D10:** when several cached albums share an artist and a title, the run
///   takes the hash of the **first** one, as `library_groups::filtered_albums`,
///   `album_composite_id_for_song` and the cover lookups do. The original keeps
///   the last one, so the same album resolved to two composite ids.
/// - **D13:** an artist has **one** run per album tag. Songs of an album that
///   are not consecutive (the list is sorted by another column than the album)
///   join the run their album opened first, in input order, instead of opening
///   a second run with the same composite id and a shared collapsed state.
///   Runs keep the order of their first song; song indices count across the
///   runs in display order.
///
/// Everything else (heights, collapse state, the untagged album name) is as
/// in [`build_view_structure`]. The Iced UI keeps calling the original.
pub fn build_view_structure_fixed(
    groups: &[ArtistGroup],
    cached_albums: Option<&[AlbumEntry]>,
    collapsed_artists: &HashSet<String>,
    collapsed_albums: &HashSet<String>,
    mode: LibraryViewMode,
) -> (f32, Vec<LibraryVirtualArtist>) {
    let mut album_hash_map: HashMap<(&str, &str), &str> = HashMap::new();
    for album in cached_albums.unwrap_or_default() {
        album_hash_map
            .entry((album.artist.as_str(), album.title.as_str()))
            .or_insert(album.id.as_str());
    }

    let header_h = ListMetrics::for_mode(mode).artist_header_h;
    let mut v_artists = Vec::with_capacity(groups.len());
    let mut total_content_h = 0.0;
    let mut global_song_idx = 0;

    for group in groups {
        let is_artist_collapsed = collapsed_artists.contains(&group.name);
        let mut albums: Vec<LibraryAlbumGroup> = Vec::new();
        let mut run_of_album: HashMap<&str, usize> = HashMap::new();

        for song in &group.songs {
            let album_name = song.album.as_deref().unwrap_or(UNKNOWN_ALBUM);
            let duration = song.duration_secs.unwrap_or(0.0);
            if let Some(&run) = run_of_album.get(album_name) {
                albums[run].duration_secs += duration;
                albums[run].songs.push((song.clone(), 0));
                continue;
            }
            let album_hash = album_hash_map
                .get(&(group.name.as_str(), album_name))
                .map_or_else(|| album_name.to_string(), |id| id.to_string());
            run_of_album.insert(album_name, albums.len());
            albums.push(LibraryAlbumGroup {
                album_name: album_name.to_string(),
                album_hash,
                songs: vec![(song.clone(), 0)],
                duration_secs: duration,
            });
        }
        for album in &mut albums {
            for (_, index) in &mut album.songs {
                *index = global_song_idx;
                global_song_idx += 1;
            }
        }

        let mut artist_h = header_h;
        if !is_artist_collapsed {
            for album in &albums {
                let composite_id = album_composite_id(&group.name, &album.album_hash);
                let is_expanded = !collapsed_albums.contains(&composite_id);
                artist_h += album_block_height(mode, is_expanded, album.songs.len());
            }
        }

        v_artists.push(LibraryVirtualArtist {
            artist_name: group.name.clone(),
            albums_count: group.albums_count,
            songs_count: group.songs_count,
            duration_secs: group.duration_secs,
            is_collapsed: is_artist_collapsed,
            top_y: total_content_h,
            height: artist_h,
            albums,
        });
        total_content_h += artist_h;
    }

    (total_content_h, v_artists)
}

/// [`item_y_range`] for the Teksilo UI, with defect D12 fixed
/// (`docs/teksilo-port-iced-defects.md`; quirk fix 40).
///
/// An album of a **collapsed** artist has no row, so it has no span: the
/// original returns the y its header would have if the artist were expanded,
/// and a reveal scrolled to a row that is not there. The album id must also be
/// a well-formed composite id (exactly two parts, see
/// `library_groups::parse_album_composite_id`); the original reads the first
/// two parts of anything. Artists and songs are as in [`item_y_range`].
pub fn item_y_range_fixed(
    item: &LibraryListItem,
    artists: &[LibraryVirtualArtist],
    collapsed_albums: &HashSet<String>,
    mode: LibraryViewMode,
) -> Option<(f32, f32)> {
    let LibraryListItem::Album(composite_id) = item else {
        return item_y_range(item, artists, collapsed_albums, mode);
    };
    let (artist_name, album_hash) =
        crate::logic::library_groups::parse_album_composite_id(composite_id)?;
    let va = artists.iter().find(|va| va.artist_name == artist_name)?;
    if va.is_collapsed {
        return None;
    }
    let metrics = ListMetrics::for_mode(mode);
    let mut current_y = va.top_y + metrics.artist_header_h;
    for alb in &va.albums {
        if alb.album_hash == album_hash {
            return Some((current_y, metrics.album_header_h));
        }
        let is_expanded = is_album_expanded(va, alb, collapsed_albums);
        current_y += album_block_height(mode, is_expanded, alb.songs.len());
    }
    None
}

/// The scroll offset that reveals an item spanning `item_y..item_y + item_h`,
/// or `None` when it is already fully visible.
///
/// A top margin keeps albums and songs from hiding under the sticky artist
/// header: 32 px (42 px in Thumbnail mode, none in Grid mode); none for artist
/// headers and for songs in Grid mode. `item` is the item being revealed, when
/// known (an unknown item gets the margin). With `force_top` the item is scrolled to the top
/// (minus the margin). `viewport` is `(scroll_y, height)` when known; without
/// it the item is scrolled to the top. Above the viewport (or the margin) the
/// item is aligned to the top; below it, to the bottom. Offsets are never
/// negative.
pub fn scroll_offset_to_reveal(
    item_y: f32,
    item_h: f32,
    item: Option<&LibraryListItem>,
    mode: LibraryViewMode,
    force_top: bool,
    viewport: Option<(f32, f32)>,
) -> Option<f32> {
    let is_artist = matches!(item, Some(LibraryListItem::Artist(_)));
    let is_song = matches!(item, Some(LibraryListItem::Song(_)));
    let margin_top = match mode {
        LibraryViewMode::ThumbnailList => 42.0_f32,
        LibraryViewMode::Grid => 0.0,
        LibraryViewMode::DetailedList | LibraryViewMode::SimpleList => 32.0,
    };
    let margin_top = if (mode == LibraryViewMode::Grid && is_song) || is_artist {
        0.0
    } else {
        margin_top
    };
    let to_top = (item_y - margin_top).max(0.0);

    if force_top {
        return Some(to_top);
    }
    let Some((scroll_y, viewport_h)) = viewport else {
        return Some(to_top);
    };
    let bottom = item_y + item_h;
    if item_y < scroll_y + margin_top {
        Some(to_top)
    } else if bottom > scroll_y + viewport_h {
        Some((bottom - viewport_h).max(0.0))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logic::library_groups::group_by_artist;

    fn song(id: i64, artist: &str, album: Option<&str>) -> Arc<SongData> {
        Arc::new(SongData {
            id,
            artist: Some(Arc::from(artist)),
            album: album.map(Arc::from),
            duration_secs: Some(10.0),
            genre: Some(Arc::from(format!("genre{id}"))),
            release_year: Some(Arc::from(format!("{}", 1990 + id))),
            ..SongData::default()
        })
    }

    fn cached(id: &str, title: &str, artist: &str) -> AlbumEntry {
        AlbumEntry {
            id: id.to_string(),
            title: title.to_string(),
            artist: artist.to_string(),
            genre: String::new(),
            year: String::new(),
            cover_path: None,
        }
    }

    /// A: albums X (songs 1, 2) and Y (song 3); B: album Z (songs 4, 5, 6).
    fn library() -> (Vec<ArtistGroup>, Vec<AlbumEntry>) {
        let songs = vec![
            song(1, "A", Some("X")),
            song(2, "A", Some("X")),
            song(3, "A", Some("Y")),
            song(4, "B", Some("Z")),
            song(5, "B", Some("Z")),
            song(6, "B", Some("Z")),
        ];
        let albums = vec![cached("hx", "X", "A"), cached("hz", "Z", "B")];
        (group_by_artist(&songs), albums)
    }

    fn structure(
        mode: LibraryViewMode,
        collapsed_artists: &[&str],
        collapsed_albums: &[&str],
    ) -> (f32, Vec<LibraryVirtualArtist>, HashSet<String>) {
        let (groups, albums) = library();
        let ca: HashSet<String> = collapsed_artists.iter().map(|s| s.to_string()).collect();
        let cb: HashSet<String> = collapsed_albums.iter().map(|s| s.to_string()).collect();
        let (total, artists) = build_view_structure(&groups, Some(&albums), &ca, &cb, mode);
        (total, artists, cb)
    }

    #[test]
    fn row_heights_are_42_px_in_thumbnail_mode_and_32_px_otherwise() {
        assert_eq!(
            ListMetrics::for_mode(LibraryViewMode::ThumbnailList).row_h,
            42.0
        );
        for mode in [
            LibraryViewMode::SimpleList,
            LibraryViewMode::DetailedList,
            LibraryViewMode::Grid,
        ] {
            let m = ListMetrics::for_mode(mode);
            assert_eq!(
                (m.artist_header_h, m.album_header_h, m.row_h),
                (32.0, 32.0, 32.0)
            );
        }
        assert!(LibraryViewMode::SimpleList.is_list());
        assert!(!LibraryViewMode::Grid.is_list());
    }

    #[test]
    fn album_block_heights_follow_each_list_mode() {
        assert_eq!(
            album_block_height(LibraryViewMode::SimpleList, true, 3),
            128.0
        );
        assert_eq!(
            album_block_height(LibraryViewMode::SimpleList, false, 3),
            32.0
        );
        assert_eq!(
            album_block_height(LibraryViewMode::ThumbnailList, true, 2),
            126.0
        );
        assert_eq!(
            album_block_height(LibraryViewMode::DetailedList, true, 3),
            333.0
        );
        assert_eq!(
            album_block_height(LibraryViewMode::DetailedList, true, 12),
            426.0
        );
        assert_eq!(
            album_block_height(LibraryViewMode::DetailedList, false, 12),
            42.0
        );
    }

    #[test]
    fn consecutive_songs_of_the_same_album_form_one_run_with_the_cached_hash() {
        let (total, artists, _) = structure(LibraryViewMode::SimpleList, &[], &[]);
        assert_eq!(artists.len(), 2);
        let a = &artists[0];
        let runs: Vec<(&str, &str, usize)> = a
            .albums
            .iter()
            .map(|r| (r.album_name.as_str(), r.album_hash.as_str(), r.songs.len()))
            .collect();
        assert_eq!(runs, vec![("X", "hx", 2), ("Y", "Y", 1)]);
        assert_eq!(a.albums[0].duration_secs, 20.0);
        let global: Vec<usize> = artists
            .iter()
            .flat_map(|va| {
                va.albums
                    .iter()
                    .flat_map(|r| r.songs.iter().map(|(_, i)| *i))
            })
            .collect();
        assert_eq!(global, vec![0, 1, 2, 3, 4, 5]);

        // A: 32 + (32 + 64) + (32 + 32) = 192; B: 32 + 32 + 96 = 160.
        assert_eq!((a.top_y, a.height), (0.0, 192.0));
        assert_eq!((artists[1].top_y, artists[1].height), (192.0, 160.0));
        assert_eq!(total, 352.0);
    }

    #[test]
    fn non_consecutive_songs_of_one_album_form_separate_runs() {
        let songs = vec![
            song(1, "A", Some("X")),
            song(2, "A", Some("Y")),
            song(3, "A", Some("X")),
            song(4, "A", None),
        ];
        let groups = group_by_artist(&songs);
        let (_, artists) = build_view_structure(
            &groups,
            None,
            &HashSet::new(),
            &HashSet::new(),
            LibraryViewMode::SimpleList,
        );
        let names: Vec<&str> = artists[0]
            .albums
            .iter()
            .map(|r| r.album_name.as_str())
            .collect();
        assert_eq!(names, vec!["X", "Y", "X", "Desconocido"]);
    }

    #[test]
    fn the_last_cached_album_with_the_same_artist_and_title_gives_the_hash() {
        let songs = vec![song(1, "A", Some("X"))];
        let groups = group_by_artist(&songs);
        let albums = vec![cached("first", "X", "A"), cached("last", "X", "A")];
        let (_, artists) = build_view_structure(
            &groups,
            Some(&albums),
            &HashSet::new(),
            &HashSet::new(),
            LibraryViewMode::SimpleList,
        );
        assert_eq!(artists[0].albums[0].album_hash, "last");
    }

    #[test]
    fn collapsed_artists_and_albums_shrink_the_layout() {
        let (total, artists, _) = structure(LibraryViewMode::SimpleList, &["B"], &["A|hx"]);
        assert_eq!(artists[0].height, 32.0 + 32.0 + 64.0);
        assert!(artists[1].is_collapsed);
        assert_eq!(artists[1].height, 32.0);
        assert_eq!(total, 160.0);
    }

    #[test]
    fn detailed_mode_uses_album_blocks() {
        let (total, artists, _) = structure(LibraryViewMode::DetailedList, &[], &["A|Y"]);
        // A: 32 + 333 + 42; B: 32 + 333.
        assert_eq!(artists[0].height, 407.0);
        assert_eq!(artists[1].height, 365.0);
        assert_eq!(total, 772.0);
    }

    #[test]
    fn the_flat_rows_are_prefix_sums_that_end_at_the_total_height() {
        for mode in [
            LibraryViewMode::SimpleList,
            LibraryViewMode::ThumbnailList,
            LibraryViewMode::DetailedList,
        ] {
            let (total, artists, collapsed) = structure(mode, &[], &["A|Y"]);
            let rows = list_rows(&artists, &collapsed, mode);
            let mut y = 0.0;
            for row in &rows {
                assert_eq!(row.top_y, y, "{mode:?}");
                y += row.height;
            }
            assert_eq!(y, total, "{mode:?}");
        }

        let (_, artists, collapsed) = structure(LibraryViewMode::SimpleList, &["B"], &["A|Y"]);
        let kinds: Vec<ListRowKind> = list_rows(&artists, &collapsed, LibraryViewMode::SimpleList)
            .iter()
            .map(|r| r.kind)
            .collect();
        assert_eq!(
            kinds,
            vec![
                ListRowKind::ArtistHeader { artist: 0 },
                ListRowKind::AlbumHeader {
                    artist: 0,
                    album: 0
                },
                ListRowKind::Song {
                    artist: 0,
                    album: 0,
                    song: 0
                },
                ListRowKind::Song {
                    artist: 0,
                    album: 0,
                    song: 1
                },
                ListRowKind::AlbumHeader {
                    artist: 0,
                    album: 1
                },
                ListRowKind::ArtistHeader { artist: 1 },
            ]
        );
    }

    #[test]
    fn the_visible_row_range_covers_rows_overlapping_the_span() {
        let (_, artists, collapsed) = structure(LibraryViewMode::SimpleList, &[], &[]);
        let rows = list_rows(&artists, &collapsed, LibraryViewMode::SimpleList);
        assert_eq!(rows.len(), 11);
        assert_eq!(visible_row_range(&rows, 0.0, 31.0), 0..1);
        assert_eq!(visible_row_range(&rows, 40.0, 70.0), 1..3);
        assert_eq!(visible_row_range(&rows, 32.0, 64.0), 0..3);
        assert_eq!(visible_row_range(&rows, 1000.0, 2000.0), 11..11);
    }

    #[test]
    fn the_render_margin_is_half_the_viewport_and_at_least_50_px() {
        assert_eq!(render_margin(800.0, false), 400.0);
        assert_eq!(render_margin(800.0, true), 200.0);
        assert_eq!(render_margin(60.0, false), 50.0);
    }

    #[test]
    fn the_scroll_offset_is_clamped_to_the_content() {
        assert_eq!(clamped_scroll(1000.0, 400.0, 900.0), 600.0);
        assert_eq!(clamped_scroll(300.0, 400.0, 50.0), 0.0);
        assert_eq!(clamped_scroll(1000.0, 400.0, 100.0), 100.0);
    }

    #[test]
    fn the_sticky_artist_is_the_last_header_at_or_above_the_threshold() {
        let (_, artists, _) = structure(LibraryViewMode::SimpleList, &[], &[]);
        assert_eq!(
            sticky_artist_index(&artists, 0.0, LibraryViewMode::SimpleList),
            Some(0)
        );
        // B starts at 192; threshold is scroll + 31.
        assert_eq!(
            sticky_artist_index(&artists, 160.0, LibraryViewMode::SimpleList),
            Some(0)
        );
        assert_eq!(
            sticky_artist_index(&artists, 161.0, LibraryViewMode::SimpleList),
            Some(1)
        );
        assert_eq!(
            sticky_artist_index(&[], 0.0, LibraryViewMode::SimpleList),
            None
        );
    }

    #[test]
    fn visible_rows_cull_individual_rows_in_simple_mode() {
        let (total, artists, collapsed) = structure(LibraryViewMode::SimpleList, &[], &[]);
        // Viewport 64 px at scroll 96 with no margin: rows overlapping [96, 160].
        let v = visible_rows(
            total,
            &artists,
            &collapsed,
            LibraryViewMode::SimpleList,
            96.0,
            64.0,
            0.0,
        );
        let kinds: Vec<String> = v
            .rows
            .iter()
            .map(|r| match r {
                LibraryVirtualRow::ArtistHeader { name, .. } => format!("artist {name}"),
                LibraryVirtualRow::AlbumBlock { album_name, .. } => format!("album {album_name}"),
                LibraryVirtualRow::SimpleSong { song, .. } => format!("song {}", song.id),
            })
            .collect();
        // Song 1 (64..96) touches the span and is kept; the A and X headers are culled.
        assert_eq!(kinds, vec!["song 1", "song 2", "album Y", "song 3"]);
        assert_eq!(v.top_space, 64.0);
        assert_eq!(v.bottom_space, 160.0);
        assert_eq!(v.total_height, total);
        assert_eq!(v.top_space + v.bottom_space + 4.0 * 32.0, total);
        assert_eq!(v.sticky.as_ref().map(|s| s.name.as_str()), Some("A"));
    }

    #[test]
    fn visible_rows_render_whole_album_blocks_in_detailed_mode() {
        let (total, artists, collapsed) = structure(LibraryViewMode::DetailedList, &[], &[]);
        let v = visible_rows(
            total,
            &artists,
            &collapsed,
            LibraryViewMode::DetailedList,
            0.0,
            100.0,
            0.0,
        );
        assert_eq!(v.rows.len(), 2);
        match &v.rows[1] {
            LibraryVirtualRow::AlbumBlock {
                album_name,
                genre,
                year,
                songs,
                songs_count,
                is_expanded,
                ..
            } => {
                assert_eq!(album_name, "X");
                assert_eq!(genre, "genre1");
                assert_eq!(year, "1991");
                assert_eq!(songs.len(), 2);
                assert_eq!(*songs_count, 2);
                assert!(*is_expanded);
            }
            other => panic!("expected an album block, got {other:?}"),
        }
        assert_eq!(v.top_space, 0.0);
        assert_eq!(v.bottom_space, total - 32.0 - 333.0);
    }

    #[test]
    fn visible_rows_clamp_the_scroll_offset_for_culling_and_the_sticky_header() {
        let (total, artists, collapsed) = structure(LibraryViewMode::SimpleList, &[], &[]);
        let v = visible_rows(
            total,
            &artists,
            &collapsed,
            LibraryViewMode::SimpleList,
            10_000.0,
            100.0,
            0.0,
        );
        assert_eq!(v.sticky.as_ref().map(|s| s.name.as_str()), Some("B"));
        assert!(!v.rows.is_empty(), "the last screen stays rendered");
    }

    #[test]
    fn item_rects_follow_the_display_order_and_rows() {
        let (_, artists, collapsed) = structure(LibraryViewMode::SimpleList, &[], &["A|Y"]);
        let rects = list_item_rects(&artists, &collapsed, LibraryViewMode::SimpleList, 500.0);
        let summary: Vec<(LibraryListItem, f32, f32)> = rects
            .iter()
            .map(|(it, r)| (it.clone(), r.y, r.height))
            .collect();
        assert_eq!(
            summary,
            vec![
                (LibraryListItem::Artist("A".into()), 0.0, 32.0),
                (LibraryListItem::Album("A|hx".into()), 32.0, 32.0),
                (LibraryListItem::Song(1), 64.0, 32.0),
                (LibraryListItem::Song(2), 96.0, 32.0),
                (LibraryListItem::Album("A|Y".into()), 128.0, 32.0),
                (LibraryListItem::Artist("B".into()), 160.0, 32.0),
                (LibraryListItem::Album("B|hz".into()), 192.0, 32.0),
                (LibraryListItem::Song(4), 224.0, 32.0),
                (LibraryListItem::Song(5), 256.0, 32.0),
                (LibraryListItem::Song(6), 288.0, 32.0),
            ]
        );
        assert!(rects.iter().all(|(_, r)| r.x == 0.0 && r.width == 500.0));
        let ids: Vec<LibraryListItem> = rects.into_iter().map(|(it, _)| it).collect();
        assert_eq!(list_items(&artists, &collapsed), ids);
    }

    #[test]
    fn detailed_item_rects_place_songs_under_the_album_header_inside_the_block() {
        let (_, artists, collapsed) = structure(LibraryViewMode::DetailedList, &[], &[]);
        let rects = list_item_rects(&artists, &collapsed, LibraryViewMode::DetailedList, 500.0);
        let ys: Vec<f32> = rects.iter().map(|(_, r)| r.y).collect();
        // A header 0; album X block at 32 (songs 64, 96), block 333 → Y at 365 (song 397).
        assert_eq!(&ys[..6], &[0.0, 32.0, 64.0, 96.0, 365.0, 397.0]);
    }

    #[test]
    fn collapsed_artists_list_only_their_header() {
        let (_, artists, collapsed) = structure(LibraryViewMode::SimpleList, &["A"], &[]);
        let items = list_items(&artists, &collapsed);
        assert_eq!(items[0], LibraryListItem::Artist("A".into()));
        assert_eq!(items[1], LibraryListItem::Artist("B".into()));
    }

    #[test]
    fn item_y_ranges_match_the_item_rectangles() {
        let (_, artists, collapsed) = structure(LibraryViewMode::ThumbnailList, &[], &["A|Y"]);
        for (item, rect) in
            list_item_rects(&artists, &collapsed, LibraryViewMode::ThumbnailList, 1.0)
        {
            assert_eq!(
                item_y_range(&item, &artists, &collapsed, LibraryViewMode::ThumbnailList),
                Some((rect.y, rect.height)),
                "{item:?}"
            );
        }
        assert_eq!(
            item_y_range(
                &LibraryListItem::Song(3),
                &artists,
                &collapsed,
                LibraryViewMode::ThumbnailList
            ),
            None,
            "a song of a collapsed album has no row"
        );
    }

    #[test]
    fn an_album_y_range_is_found_even_inside_a_collapsed_artist() {
        let (_, artists, collapsed) = structure(LibraryViewMode::SimpleList, &["B"], &[]);
        let mode = LibraryViewMode::SimpleList;
        assert_eq!(
            item_y_range(
                &LibraryListItem::Album("B|hz".into()),
                &artists,
                &collapsed,
                mode
            ),
            Some((224.0, 32.0))
        );
        assert_eq!(
            item_y_range(&LibraryListItem::Song(4), &artists, &collapsed, mode),
            None
        );
        assert_eq!(
            item_y_range(
                &LibraryListItem::Album("B".into()),
                &artists,
                &collapsed,
                mode
            ),
            None
        );
        assert_eq!(
            item_y_range(
                &LibraryListItem::Album("B|hz|x".into()),
                &artists,
                &collapsed,
                mode
            ),
            Some((224.0, 32.0)),
            "only the first two parts of the id are used"
        );
    }

    #[test]
    fn d4_album_ids_with_pipes_in_the_artist_or_album_stay_distinct() {
        // Without escaping, artist "A|B" + album "X" and artist "A" + album
        // "B|X" both gave the id `A|B|X`, and the y lookup split it wrongly.
        let songs = vec![
            song(1, "A|B", Some("X")),
            song(2, "A|B", Some("Y|Z")),
            song(3, "A", Some("B|X")),
        ];
        let albums = vec![cached("X", "X", "A|B")];
        let groups = group_by_artist(&songs);
        let mode = LibraryViewMode::SimpleList;
        let a_bx = album_composite_id("A", "B|X");
        let ab_x = album_composite_id("A|B", "X");
        assert_ne!(a_bx, ab_x);
        let collapsed: HashSet<String> = [a_bx.clone()].into_iter().collect();
        let (_, artists) =
            build_view_structure(&groups, Some(&albums), &HashSet::new(), &collapsed, mode);

        assert_eq!(album_id_for_song(&artists, 3), Some(a_bx.clone()));
        assert_eq!(album_id_for_song(&artists, 1), Some(ab_x.clone()));
        assert_eq!(
            album_id_for_song(&artists, 2),
            Some(album_composite_id("A|B", "Y|Z"))
        );
        // Only the album collapsed by its id is collapsed.
        let rects = list_item_rects(&artists, &collapsed, mode, 1.0);
        assert!(
            !rects
                .iter()
                .any(|(item, _)| *item == LibraryListItem::Song(3))
        );
        assert!(
            rects
                .iter()
                .any(|(item, _)| *item == LibraryListItem::Song(1))
        );
        for (item, rect) in rects {
            assert_eq!(
                item_y_range(&item, &artists, &collapsed, mode),
                Some((rect.y, rect.height)),
                "{item:?}"
            );
        }
    }

    #[test]
    fn the_album_of_a_song_is_its_first_run() {
        let (_, artists, _) = structure(LibraryViewMode::SimpleList, &["A"], &[]);
        assert_eq!(album_id_for_song(&artists, 3), Some("A|Y".to_string()));
        assert_eq!(album_id_for_song(&artists, 5), Some("B|hz".to_string()));
        assert_eq!(album_id_for_song(&artists, 99), None);
    }

    #[test]
    fn rectangles_intersect_only_with_a_positive_overlap() {
        let a = ContentRect::new(0.0, 0.0, 10.0, 10.0);
        assert!(a.intersects(&ContentRect::new(5.0, 5.0, 10.0, 10.0)));
        assert!(!a.intersects(&ContentRect::new(10.0, 0.0, 5.0, 5.0)));
        assert!(!a.intersects(&ContentRect::new(0.0, 10.0, 5.0, 5.0)));
        assert!(a.intersects(&ContentRect::new(2.0, 2.0, 1.0, 1.0)));
    }

    #[test]
    fn a_marquee_needs_a_drag_longer_than_three_pixels() {
        assert_eq!(marquee_rect((0.0, 0.0), (3.0, 0.0)), None);
        assert_eq!(marquee_rect((0.0, 0.0), (2.0, 2.0)), None);
        assert_eq!(
            marquee_rect((10.0, 50.0), (2.0, 50.0)),
            Some(ContentRect::new(2.0, 50.0, 8.0, 1.0))
        );
        assert_eq!(
            marquee_rect((5.0, 5.0), (8.0, 30.0)),
            Some(ContentRect::new(5.0, 5.0, 3.0, 25.0))
        );
    }

    #[test]
    fn revealing_an_item_respects_the_sticky_header_margin() {
        let song_item = LibraryListItem::Song(1);
        let song = Some(&song_item);
        let artist_item = LibraryListItem::Artist("A".into());
        let artist = Some(&artist_item);
        let simple = LibraryViewMode::SimpleList;
        // Above the viewport: aligned to the top minus the 32 px margin.
        assert_eq!(
            scroll_offset_to_reveal(100.0, 32.0, song, simple, false, Some((90.0, 300.0))),
            Some(68.0)
        );
        // Artists have no margin.
        assert_eq!(
            scroll_offset_to_reveal(100.0, 32.0, artist, simple, false, Some((120.0, 300.0))),
            Some(100.0)
        );
        // Below the viewport: aligned to the bottom.
        assert_eq!(
            scroll_offset_to_reveal(500.0, 32.0, song, simple, false, Some((0.0, 300.0))),
            Some(232.0)
        );
        // Fully visible: no scroll.
        assert_eq!(
            scroll_offset_to_reveal(100.0, 32.0, song, simple, false, Some((0.0, 300.0))),
            None
        );
        // Forced or without a viewport: to the top, never negative.
        assert_eq!(
            scroll_offset_to_reveal(10.0, 32.0, song, simple, true, Some((0.0, 300.0))),
            Some(0.0)
        );
        assert_eq!(
            scroll_offset_to_reveal(
                100.0,
                32.0,
                song,
                LibraryViewMode::ThumbnailList,
                false,
                None
            ),
            Some(58.0)
        );
        assert_eq!(
            scroll_offset_to_reveal(100.0, 32.0, song, LibraryViewMode::Grid, true, None),
            Some(100.0)
        );
        assert_eq!(
            scroll_offset_to_reveal(
                100.0,
                32.0,
                Some(&LibraryListItem::Album("A|x".into())),
                LibraryViewMode::Grid,
                true,
                None
            ),
            Some(100.0)
        );
        // Without a known item, the list margin applies.
        assert_eq!(
            scroll_offset_to_reveal(100.0, 32.0, None, simple, true, None),
            Some(68.0)
        );
    }

    fn fixed_structure(
        songs: &[Arc<SongData>],
        albums: &[AlbumEntry],
        collapsed_artists: &[&str],
        mode: LibraryViewMode,
    ) -> Vec<LibraryVirtualArtist> {
        let ca: HashSet<String> = collapsed_artists.iter().map(|s| s.to_string()).collect();
        build_view_structure_fixed(
            &group_by_artist(songs),
            Some(albums),
            &ca,
            &HashSet::new(),
            mode,
        )
        .1
    }

    #[test]
    fn d10_duplicate_albums_resolve_to_the_first_cached_hash_like_the_rest_of_the_library() {
        let songs = vec![song(1, "A", Some("X"))];
        let albums = vec![cached("first", "X", "A"), cached("last", "X", "A")];
        let groups = group_by_artist(&songs);
        let none = HashSet::new();
        let mode = LibraryViewMode::SimpleList;
        let (_, original) = build_view_structure(&groups, Some(&albums), &none, &none, mode);
        assert_eq!(
            original[0].albums[0].album_hash, "last",
            "the original keeps the last"
        );

        let fixed = fixed_structure(&songs, &albums, &[], mode);
        assert_eq!(fixed[0].albums[0].album_hash, "first");
        // The same id the song's own lookup and the filtered grid give.
        assert_eq!(
            album_id_for_song(&fixed, 1),
            Some(crate::logic::library_groups::album_composite_id_for_song(
                &songs[0],
                Some(&albums)
            ))
        );
        let grid = crate::logic::library_groups::filtered_albums(&groups, &albums, None, None);
        assert_eq!(grid[0].id, fixed[0].albums[0].album_hash);
    }

    #[test]
    fn d13_an_album_whose_songs_are_not_consecutive_forms_one_run() {
        let songs = vec![
            song(1, "A", Some("X")),
            song(2, "A", Some("Y")),
            song(3, "A", Some("X")),
            song(4, "A", None),
        ];
        let groups = group_by_artist(&songs);
        let none = HashSet::new();
        let mode = LibraryViewMode::SimpleList;
        let (_, original) = build_view_structure(&groups, None, &none, &none, mode);
        let original_items = list_items(&original, &none);
        let x = LibraryListItem::Album(album_composite_id("A", "X"));
        assert_eq!(
            original_items.iter().filter(|it| **it == x).count(),
            2,
            "the original lists album X twice"
        );

        let (total, fixed) = build_view_structure_fixed(&groups, None, &none, &none, mode);
        let runs: Vec<(&str, Vec<i64>)> = fixed[0]
            .albums
            .iter()
            .map(|r| {
                (
                    r.album_name.as_str(),
                    r.songs.iter().map(|(s, _)| s.id).collect(),
                )
            })
            .collect();
        assert_eq!(
            runs,
            vec![("X", vec![1, 3]), ("Y", vec![2]), ("Desconocido", vec![4])]
        );
        assert_eq!(fixed[0].albums[0].duration_secs, 20.0);
        let indices: Vec<usize> = fixed[0]
            .albums
            .iter()
            .flat_map(|r| r.songs.iter().map(|(_, i)| *i))
            .collect();
        assert_eq!(
            indices,
            vec![0, 1, 2, 3],
            "indices follow the display order"
        );
        let items = list_items(&fixed, &none);
        assert_eq!(items.iter().filter(|it| **it == x).count(), 1);
        // Same height as the original: one header per run less.
        assert_eq!(total, 32.0 + 3.0 * 32.0 + 4.0 * 32.0);
    }

    #[test]
    fn d13_collapsing_the_album_hides_all_its_songs() {
        let songs = vec![
            song(1, "A", Some("X")),
            song(2, "A", Some("Y")),
            song(3, "A", Some("X")),
        ];
        let groups = group_by_artist(&songs);
        let collapsed: HashSet<String> = [album_composite_id("A", "X")].into_iter().collect();
        let mode = LibraryViewMode::SimpleList;
        let (_, fixed) =
            build_view_structure_fixed(&groups, None, &HashSet::new(), &collapsed, mode);
        let items = list_items(&fixed, &collapsed);
        assert!(!items.contains(&LibraryListItem::Song(1)));
        assert!(!items.contains(&LibraryListItem::Song(3)));
        assert!(items.contains(&LibraryListItem::Song(2)));
    }

    #[test]
    fn the_fixed_structure_matches_the_original_without_duplicates() {
        for mode in [
            LibraryViewMode::SimpleList,
            LibraryViewMode::ThumbnailList,
            LibraryViewMode::DetailedList,
        ] {
            let (groups, albums) = library();
            let ca: HashSet<String> = ["B".to_string()].into_iter().collect();
            let cb: HashSet<String> = [album_composite_id("A", "Y")].into_iter().collect();
            let (total, original) = build_view_structure(&groups, Some(&albums), &ca, &cb, mode);
            let (fixed_total, fixed) =
                build_view_structure_fixed(&groups, Some(&albums), &ca, &cb, mode);
            assert_eq!(total, fixed_total, "{mode:?}");
            assert_eq!(
                list_item_rects(&original, &cb, mode, 100.0),
                list_item_rects(&fixed, &cb, mode, 100.0),
                "{mode:?}"
            );
        }
    }

    #[test]
    fn d12_an_album_of_a_collapsed_artist_has_no_span() {
        let (_, artists, collapsed) = structure(LibraryViewMode::SimpleList, &["B"], &[]);
        let mode = LibraryViewMode::SimpleList;
        let hz = LibraryListItem::Album("B|hz".into());
        assert_eq!(
            item_y_range(&hz, &artists, &collapsed, mode),
            Some((224.0, 32.0)),
            "the original gives the y of a hidden header"
        );
        assert_eq!(item_y_range_fixed(&hz, &artists, &collapsed, mode), None);

        let (_, expanded, collapsed) = structure(mode, &[], &[]);
        assert_eq!(
            item_y_range_fixed(&hz, &expanded, &collapsed, mode),
            item_y_range(&hz, &expanded, &collapsed, mode)
        );
    }

    #[test]
    fn d12_the_fixed_span_matches_every_visible_rectangle_and_rejects_malformed_ids() {
        let mode = LibraryViewMode::ThumbnailList;
        let (_, artists, collapsed) = structure(mode, &["B"], &["A|Y"]);
        for (item, rect) in list_item_rects(&artists, &collapsed, mode, 1.0) {
            assert_eq!(
                item_y_range_fixed(&item, &artists, &collapsed, mode),
                Some((rect.y, rect.height)),
                "{item:?}"
            );
        }
        let malformed = LibraryListItem::Album("A|hx|extra".into());
        assert!(item_y_range(&malformed, &artists, &collapsed, mode).is_some());
        assert_eq!(
            item_y_range_fixed(&malformed, &artists, &collapsed, mode),
            None
        );
    }
}
