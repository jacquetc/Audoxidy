//! Playlist export: the M3U/M3U8 text and, for a portable export, the plan of
//! file copies into `<artist>/<album>/<file>` folders next to the list.
//!
//! The plan is pure; the caller performs the I/O in order: for each copy,
//! create its folder then copy the file, then write the list.

use crate::db::database::PlaylistSongRef;
use std::path::{Path, PathBuf};

/// Playlist file format. Both formats are written identically (UTF-8 text);
/// only the file extension differs.
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

    for song in songs {
        let path_to_write = if is_portable {
            let artist_dir = sanitize_name(&song.artist_name);
            let album_dir = sanitize_name(&song.album_title);
            let song_file = Path::new(song.file_path.as_ref())
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();

            let relative_path = format!("{}/{}/{}", artist_dir, album_dir, song_file);
            let target_dir = target_path.join(&artist_dir).join(&album_dir);
            let target_file = target_dir.join(song_file.as_ref());
            copies.push(PortableCopy {
                source: song.file_path.to_string(),
                target_dir,
                target_file,
            });
            relative_path
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
}
