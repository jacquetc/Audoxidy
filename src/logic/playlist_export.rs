//! Playlist export: the M3U/M3U8 text and, for a portable export, the plan of
//! file copies into `<artist>/<album>/<file>` folders next to the list.
//!
//! The plan is pure; the caller performs the I/O in order: for each copy,
//! create its folder then copy the file, then write the list.

use crate::db::database::PlaylistSongRef;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Playlist file format. The Iced UI writes both identically (UTF-8 text);
/// the facade's export (`frontend::commands::playlist::export`) writes M3U in
/// Latin-1 and M3U8 in UTF-8 (Teksilo quirk fix 25).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// `.m3u`
    M3U,
    /// `.m3u8`
    M3U8,
}

impl ExportFormat {
    /// File extension, without the dot.
    pub fn extension(&self) -> &'static str {
        match self {
            ExportFormat::M3U => "m3u",
            ExportFormat::M3U8 => "m3u8",
        }
    }
}

/// What an export produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportMode {
    /// One list file with absolute paths, at the chosen file path.
    SingleFile,
    /// A folder holding copies of the songs and a list with relative paths.
    PortableFolder,
}

/// Playlist name used when the playlist cannot be read back.
pub const FALLBACK_PLAYLIST_NAME: &str = "Playlist_Exportada";

/// Replaces every character that is invalid in a file name on some system
/// (`/ \ ? % * : | " < >`) with `_`.
pub fn sanitize_name(s: &str) -> String {
    s.chars()
        .map(|c| if "/\\?%*:|\"<>".contains(c) { '_' } else { c })
        .collect()
}

/// One `#EXTINF` entry: the duration truncated to whole seconds, the title,
/// then the path on its own line.
pub fn m3u_entry(duration_secs: f64, title: &str, path: &str) -> String {
    format!("#EXTINF:{},{}\n{}\n", duration_secs as i32, title, path)
}

/// One file copy of a portable export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortableCopy {
    /// The song's file.
    pub source: String,
    /// Folder to create (`<target>/<artist>/<album>`).
    pub target_dir: PathBuf,
    /// Destination of the copy (`<target_dir>/<file name>`).
    pub target_file: PathBuf,
}

/// Everything an export writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportPlan {
    /// Where the list is written.
    pub playlist_path: PathBuf,
    /// The list's text.
    pub content: String,
    /// The copies to make first (portable export only, in playlist order).
    pub copies: Vec<PortableCopy>,
}

/// Plans an export of `songs` (in playlist order).
///
/// - Single file: the list goes to `target_path` and holds each song's path.
/// - Portable folder: each song is copied to
///   `<target_path>/<artist>/<album>/<file name>` (artist and album sanitised
///   with [`sanitize_name`]), the list holds `artist/album/file` relative
///   entries, and it is written to `<target_path>/<sanitised name>.<ext>`.
///   Destinations never overwrite each other: when `artist/album/file`
///   matches an earlier one, compared case-insensitively so that the copy is
///   also safe on case-insensitive file systems, the later file is renamed
///   with ` (2)`, ` (3)`, … before its extension (see [`claim_file_name`]),
///   and the list refers to the new name. A source path listed more than once
///   is copied once, and each of its entries points to that copy.
pub fn plan_export(
    songs: &[PlaylistSongRef],
    playlist_name: &str,
    target_path: &Path,
    format: ExportFormat,
    mode: ExportMode,
) -> ExportPlan {
    let is_portable = mode == ExportMode::PortableFolder;
    let mut content = String::from("#EXTM3U\n");
    let mut copies = Vec::new();
    // Portable export: destinations taken so far (lower-cased relative paths)
    // and the relative path each source was copied to.
    let mut taken = HashSet::new();
    let mut copied: HashMap<&str, String> = HashMap::new();

    for song in songs {
        let path_to_write = if is_portable {
            if let Some(relative_path) = copied.get(song.file_path.as_ref()) {
                relative_path.clone()
            } else {
                let artist_dir = sanitize_name(&song.artist_name);
                let album_dir = sanitize_name(&song.album_title);
                let song_file = Path::new(song.file_path.as_ref())
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy();
                let song_file = claim_file_name(
                    &mut taken,
                    &format!("{}/{}", artist_dir, album_dir),
                    &song_file,
                );

                let relative_path = format!("{}/{}/{}", artist_dir, album_dir, song_file);
                let target_dir = target_path.join(&artist_dir).join(&album_dir);
                let target_file = target_dir.join(&song_file);
                copies.push(PortableCopy {
                    source: song.file_path.to_string(),
                    target_dir,
                    target_file,
                });
                copied.insert(song.file_path.as_ref(), relative_path.clone());
                relative_path
            }
        } else {
            song.file_path.to_string()
        };

        content.push_str(&m3u_entry(song.duration, &song.title, &path_to_write));
    }

    let playlist_path = if is_portable {
        target_path.join(format!(
            "{}.{}",
            sanitize_name(playlist_name),
            format.extension()
        ))
    } else {
        target_path.to_path_buf()
    };

    ExportPlan {
        playlist_path,
        content,
        copies,
    }
}

/// Picks the file name of a portable copy into the folder `dir`
/// (`<artist>/<album>`, relative to the export folder) and records the
/// destination in `taken`, which holds the lower-cased `dir/name` of every
/// destination chosen so far. The name is `file_name` itself when that
/// destination is free; otherwise ` (2)`, ` (3)`, … is inserted before its
/// extension (`01.flac` → `01 (2).flac`, `notes` → `notes (2)`) until a free
/// one is found.
pub fn claim_file_name(taken: &mut HashSet<String>, dir: &str, file_name: &str) -> String {
    let key = |name: &str| format!("{}/{}", dir, name).to_lowercase();
    if taken.insert(key(file_name)) {
        return file_name.to_string();
    }
    let path = Path::new(file_name);
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let extension = path.extension().map(|e| e.to_string_lossy());
    let mut n: u64 = 2;
    loop {
        let candidate = match &extension {
            Some(ext) => format!("{} ({}).{}", stem, n, ext),
            None => format!("{} ({})", stem, n),
        };
        if taken.insert(key(&candidate)) {
            return candidate;
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(title: &str, artist: &str, album: &str, path: &str, duration: f64) -> PlaylistSongRef {
        PlaylistSongRef {
            item_id: 1,
            song_id: 1,
            sequence_order: 1.0,
            enabled: true,
            title: title.into(),
            artist_name: artist.into(),
            album_title: album.into(),
            album_artist_name: artist.into(),
            year: None,
            genre: None,
            track_number: None,
            duration,
            file_path: path.into(),
            folder_path: "/".into(),
            folder_name: "Root".into(),
            cover_path: None,
            cover_override: None,
        }
    }

    fn songs() -> Vec<PlaylistSongRef> {
        vec![
            song(
                "Heroes",
                "David Bowie",
                "\"Heroes\"",
                "/m/bowie/01 Heroes.flac",
                371.9,
            ),
            song("Low: A/B", "AC/DC", "Live?", "/m/acdc/02.mp3", 59.0),
        ]
    }

    #[test]
    fn format_extensions_are_m3u_and_m3u8() {
        assert_eq!(ExportFormat::M3U.extension(), "m3u");
        assert_eq!(ExportFormat::M3U8.extension(), "m3u8");
    }

    #[test]
    fn sanitising_replaces_every_reserved_character() {
        assert_eq!(
            sanitize_name("a/b\\c?d%e*f:g|h\"i<j>k"),
            "a_b_c_d_e_f_g_h_i_j_k"
        );
        assert_eq!(sanitize_name("Ñandú — ok"), "Ñandú — ok");
    }

    #[test]
    fn an_entry_truncates_the_duration_to_whole_seconds() {
        assert_eq!(
            m3u_entry(371.9, "Heroes", "/a.flac"),
            "#EXTINF:371,Heroes\n/a.flac\n"
        );
        assert_eq!(m3u_entry(-0.5, "T", "p"), "#EXTINF:0,T\np\n");
    }

    #[test]
    fn a_single_file_export_lists_absolute_paths_at_the_chosen_path() {
        let plan = plan_export(
            &songs(),
            "My list",
            Path::new("/out/list.m3u8"),
            ExportFormat::M3U8,
            ExportMode::SingleFile,
        );
        assert_eq!(plan.playlist_path, PathBuf::from("/out/list.m3u8"));
        assert!(plan.copies.is_empty());
        assert_eq!(
            plan.content,
            "#EXTM3U\n\
             #EXTINF:371,Heroes\n/m/bowie/01 Heroes.flac\n\
             #EXTINF:59,Low: A/B\n/m/acdc/02.mp3\n"
        );
    }

    #[test]
    fn m3u_and_m3u8_single_files_have_the_same_text() {
        let a = plan_export(
            &songs(),
            "x",
            Path::new("/o.m3u"),
            ExportFormat::M3U,
            ExportMode::SingleFile,
        );
        let b = plan_export(
            &songs(),
            "x",
            Path::new("/o.m3u"),
            ExportFormat::M3U8,
            ExportMode::SingleFile,
        );
        assert_eq!(a, b);
    }

    #[test]
    fn a_portable_export_copies_into_artist_and_album_folders() {
        let plan = plan_export(
            &songs(),
            "Best: of",
            Path::new("/out"),
            ExportFormat::M3U,
            ExportMode::PortableFolder,
        );
        assert_eq!(
            plan.copies,
            vec![
                PortableCopy {
                    source: "/m/bowie/01 Heroes.flac".into(),
                    target_dir: PathBuf::from("/out/David Bowie/_Heroes_"),
                    target_file: PathBuf::from("/out/David Bowie/_Heroes_/01 Heroes.flac"),
                },
                PortableCopy {
                    source: "/m/acdc/02.mp3".into(),
                    target_dir: PathBuf::from("/out/AC_DC/Live_"),
                    target_file: PathBuf::from("/out/AC_DC/Live_/02.mp3"),
                },
            ]
        );
    }

    #[test]
    fn a_portable_list_has_relative_entries_and_a_sanitised_name() {
        let plan = plan_export(
            &songs(),
            "Best: of",
            Path::new("/out"),
            ExportFormat::M3U8,
            ExportMode::PortableFolder,
        );
        assert_eq!(plan.playlist_path, PathBuf::from("/out/Best_ of.m3u8"));
        assert_eq!(
            plan.content,
            "#EXTM3U\n\
             #EXTINF:371,Heroes\nDavid Bowie/_Heroes_/01 Heroes.flac\n\
             #EXTINF:59,Low: A/B\nAC_DC/Live_/02.mp3\n"
        );
    }

    #[test]
    fn an_empty_playlist_exports_only_the_header() {
        let plan = plan_export(
            &[],
            "x",
            Path::new("/out"),
            ExportFormat::M3U,
            ExportMode::PortableFolder,
        );
        assert_eq!(plan.content, "#EXTM3U\n");
        assert_eq!(plan.playlist_path, PathBuf::from("/out/x.m3u"));
        assert!(plan.copies.is_empty());
    }

    fn portable(songs: &[PlaylistSongRef]) -> ExportPlan {
        plan_export(
            songs,
            "List",
            Path::new("/out"),
            ExportFormat::M3U8,
            ExportMode::PortableFolder,
        )
    }

    fn targets(plan: &ExportPlan) -> Vec<(&str, PathBuf)> {
        plan.copies
            .iter()
            .map(|c| (c.source.as_str(), c.target_file.clone()))
            .collect()
    }

    #[test]
    fn d3_tracks_with_the_same_destination_get_numbered_names() {
        // Before the fix, all three were copied to `A/Best/01.flac`.
        let plan = portable(&[
            song("One", "A", "Best", "/cd1/01.flac", 10.0),
            song("Two", "A", "Best", "/cd2/01.flac", 20.0),
            song("Three", "A", "Best", "/cd3/01.flac", 30.0),
        ]);
        assert_eq!(
            targets(&plan),
            vec![
                ("/cd1/01.flac", PathBuf::from("/out/A/Best/01.flac")),
                ("/cd2/01.flac", PathBuf::from("/out/A/Best/01 (2).flac")),
                ("/cd3/01.flac", PathBuf::from("/out/A/Best/01 (3).flac")),
            ]
        );
        assert!(
            plan.copies
                .iter()
                .all(|c| c.target_dir == Path::new("/out/A/Best"))
        );
        assert_eq!(
            plan.content,
            "#EXTM3U\n\
             #EXTINF:10,One\nA/Best/01.flac\n\
             #EXTINF:20,Two\nA/Best/01 (2).flac\n\
             #EXTINF:30,Three\nA/Best/01 (3).flac\n"
        );
    }

    #[test]
    fn d3_destinations_that_differ_only_by_case_collide() {
        let plan = portable(&[
            song("One", "Abba", "Gold", "/x/Track.FLAC", 1.0),
            song("Two", "ABBA", "gold", "/y/track.flac", 2.0),
            song("Three", "abba", "GOLD", "/z/notes", 3.0),
            song("Four", "Abba", "Gold", "/w/NOTES", 4.0),
        ]);
        assert_eq!(
            targets(&plan),
            vec![
                ("/x/Track.FLAC", PathBuf::from("/out/Abba/Gold/Track.FLAC")),
                (
                    "/y/track.flac",
                    PathBuf::from("/out/ABBA/gold/track (2).flac")
                ),
                ("/z/notes", PathBuf::from("/out/abba/GOLD/notes")),
                ("/w/NOTES", PathBuf::from("/out/Abba/Gold/NOTES (2)")),
            ]
        );
        assert!(plan.content.contains("\nABBA/gold/track (2).flac\n"));
        assert!(plan.content.contains("\nAbba/Gold/NOTES (2)\n"));
    }

    #[test]
    fn d3_a_generated_name_never_takes_an_existing_one() {
        let plan = portable(&[
            song("One", "A", "B", "/1/a.mp3", 1.0),
            song("Two", "A", "B", "/2/a (2).mp3", 2.0),
            song("Three", "A", "B", "/3/a.mp3", 3.0),
        ]);
        assert_eq!(
            targets(&plan),
            vec![
                ("/1/a.mp3", PathBuf::from("/out/A/B/a.mp3")),
                ("/2/a (2).mp3", PathBuf::from("/out/A/B/a (2).mp3")),
                ("/3/a.mp3", PathBuf::from("/out/A/B/a (3).mp3")),
            ]
        );
    }

    #[test]
    fn d3_the_same_source_listed_twice_is_copied_once() {
        let plan = portable(&[
            song("One", "A", "B", "/m/01.flac", 1.0),
            song("Other", "A", "B", "/n/01.flac", 2.0),
            song("One again", "A", "B", "/m/01.flac", 1.0),
        ]);
        assert_eq!(
            targets(&plan),
            vec![
                ("/m/01.flac", PathBuf::from("/out/A/B/01.flac")),
                ("/n/01.flac", PathBuf::from("/out/A/B/01 (2).flac")),
            ]
        );
        assert_eq!(
            plan.content,
            "#EXTM3U\n\
             #EXTINF:1,One\nA/B/01.flac\n\
             #EXTINF:2,Other\nA/B/01 (2).flac\n\
             #EXTINF:1,One again\nA/B/01.flac\n"
        );
    }

    #[test]
    fn d3_without_collisions_the_plan_is_unchanged() {
        // Same folder with different names, and the same name in different
        // folders or with another extension, keep their names.
        let plan = portable(&[
            song("One", "A", "B", "/m/01.flac", 1.0),
            song("Two", "A", "B", "/m/02.flac", 2.0),
            song("Three", "A", "C", "/n/01.flac", 3.0),
            song("Four", "D", "B", "/o/01.flac", 4.0),
            song("Five", "A", "B", "/p/01.mp3", 5.0),
        ]);
        assert_eq!(
            targets(&plan),
            vec![
                ("/m/01.flac", PathBuf::from("/out/A/B/01.flac")),
                ("/m/02.flac", PathBuf::from("/out/A/B/02.flac")),
                ("/n/01.flac", PathBuf::from("/out/A/C/01.flac")),
                ("/o/01.flac", PathBuf::from("/out/D/B/01.flac")),
                ("/p/01.mp3", PathBuf::from("/out/A/B/01.mp3")),
            ]
        );
        assert_eq!(
            plan.content,
            "#EXTM3U\n\
             #EXTINF:1,One\nA/B/01.flac\n\
             #EXTINF:2,Two\nA/B/02.flac\n\
             #EXTINF:3,Three\nA/C/01.flac\n\
             #EXTINF:4,Four\nD/B/01.flac\n\
             #EXTINF:5,Five\nA/B/01.mp3\n"
        );
    }

    #[test]
    fn a_single_file_export_keeps_repeated_names_and_sources() {
        let list = [
            song("One", "A", "B", "/m/01.flac", 1.0),
            song("Two", "A", "B", "/n/01.flac", 2.0),
            song("One", "A", "B", "/m/01.flac", 1.0),
        ];
        let plan = plan_export(
            &list,
            "x",
            Path::new("/o.m3u"),
            ExportFormat::M3U,
            ExportMode::SingleFile,
        );
        assert!(plan.copies.is_empty());
        assert_eq!(
            plan.content,
            "#EXTM3U\n\
             #EXTINF:1,One\n/m/01.flac\n\
             #EXTINF:2,Two\n/n/01.flac\n\
             #EXTINF:1,One\n/m/01.flac\n"
        );
    }
}
