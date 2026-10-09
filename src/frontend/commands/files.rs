//! Revealing a file in the system file manager.

#![warn(missing_docs)]

use std::path::Path;

use crate::frontend::error::FrontendError;

/// The operating systems whose file-manager commands are known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileManagerOs {
    /// Linux and the other `xdg-open` desktops.
    Linux,
    /// Windows (Explorer).
    Windows,
    /// macOS (Finder).
    MacOs,
}

impl FileManagerOs {
    /// The operating system this build runs on.
    pub fn current() -> Self {
        if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Linux
        }
    }
}

/// The program and arguments that reveal `path` in the file manager of `os`.
///
/// - Linux: `xdg-open <parent directory>`; `None` when the path has no parent.
/// - Windows: `explorer /select,"<path>"`.
/// - macOS: `open -R <path>`.
///
/// This is a pure function so that every OS' command line can be tested on any
/// host.
pub fn file_location_command(os: FileManagerOs, path: &str) -> Option<(String, Vec<String>)> {
    // Iced only reveals a file whose path has a parent; keep that rule.
    let parent = Path::new(path).parent()?;
    Some(match os {
        FileManagerOs::Linux => (
            "xdg-open".to_string(),
            vec![parent.to_string_lossy().into_owned()],
        ),
        FileManagerOs::Windows => ("explorer".to_string(), vec![format!("/select,\"{path}\"")]),
        FileManagerOs::MacOs => ("open".to_string(), vec!["-R".to_string(), path.to_string()]),
    })
}

/// Reveals `path` in the system file manager, without waiting for it.
///
/// A path with no parent directory does nothing and succeeds, as in the Iced
/// UI. Fails with [`FrontendError::Io`] when the program cannot be started.
pub fn show_file_location(path: &str) -> Result<(), FrontendError> {
    let Some((program, args)) = file_location_command(FileManagerOs::current(), path) else {
        return Ok(());
    };
    let mut child = std::process::Command::new(program).args(args).spawn()?;
    // Reap the file manager launcher in the background so it does not stay a
    // zombie; the caller never waits for it.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_opens_the_parent_directory() {
        let (program, args) =
            file_location_command(FileManagerOs::Linux, "/music/Album/01 - Song.flac").unwrap();
        assert_eq!(program, "xdg-open");
        assert_eq!(args, vec!["/music/Album".to_string()]);
    }

    #[test]
    fn windows_selects_the_file_in_explorer() {
        let (program, args) =
            file_location_command(FileManagerOs::Windows, "/music/a b/song.flac").unwrap();
        assert_eq!(program, "explorer");
        assert_eq!(args, vec!["/select,\"/music/a b/song.flac\"".to_string()]);
    }

    #[test]
    fn macos_reveals_the_file_in_finder() {
        let (program, args) =
            file_location_command(FileManagerOs::MacOs, "/music/song.flac").unwrap();
        assert_eq!(program, "open");
        assert_eq!(args, vec!["-R".to_string(), "/music/song.flac".to_string()]);
    }

    #[test]
    fn a_path_without_a_parent_has_no_command() {
        assert!(file_location_command(FileManagerOs::Linux, "/").is_none());
        assert!(file_location_command(FileManagerOs::MacOs, "").is_none());
    }

    #[test]
    fn showing_a_path_without_a_parent_succeeds_without_spawning() {
        assert!(show_file_location("/").is_ok());
    }
}
