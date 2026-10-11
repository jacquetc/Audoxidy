//! `res!()` SVG factories for the icons in `assets/icons/`, grouped by where
//! they appear. Pure asset lookups: no state, no logic.
//!
//! `res!` embeds each file at compile time (the Iced UI loads them from the
//! working directory at run time) and needs a literal path per call site, so
//! there is one function per icon. The SVGs are white single-colour Material
//! icons; Teksilo tints them, so a factory sets no colour: the caller picks a
//! role, by default [`style::ICON_ROLE`](super::style::ICON_ROLE). Factories
//! set no size either: the Iced sizes differ per call site.
//!
//! The submodules follow the Iced UI's screens: [`window`] (title bar),
//! [`player`], [`playlist`], [`library`], [`filters`], [`audio_center`] and
//! [`app`] (the context menus the app shell builds). [`shared`] holds an icon
//! that two or more of those areas use, so each icon lives in exactly one
//! module. A factory is named after what it shows, in snake_case, without the
//! `-straight` marker of the Iced file names (every icon the Iced UI uses is
//! straight); `-fill`, `-outlined` and `-rounded` stay in the name.
//!
//! [`ALL`] lists every `(file name, factory)` pair. The tests check it against
//! the `.svg` literals in `src/gui`, so an icon the Iced UI starts using
//! without a factory fails the build's tests.

use teksilo::res;
use teksilo::widgets::IconWidget;

/// The window controls in the title bar (`src/gui/library.rs:1361`).
pub mod window {
    use super::{IconWidget, res};

    /// Minimize (`minimize.svg`): the title bar's minimize button,
    /// `src/gui/library.rs:1363`.
    pub fn minimize() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/minimize.svg"))
    }

    /// Maximize (`maximize.svg`): the title bar's maximize button, shown in
    /// both window states, `src/gui/library.rs:1374`.
    pub fn maximize() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/maximize.svg"))
    }

    /// Close (`close.svg`): the title bar's close button,
    /// `src/gui/library.rs:1379`.
    pub fn close() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/close.svg"))
    }
}

/// The player panel: the 400 × 400 cover with its menu, transport and volume
/// overlay, at the top of the left column (`src/gui/player.rs`).
pub mod player {
    use super::{IconWidget, res};

    /// Menu (`menu.svg`): the player's menu button, `src/gui/player.rs:125`.
    pub fn menu() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/menu.svg"))
    }

    /// Pause (`pause-straight-fill.svg`): the play/pause button while a track
    /// plays, `src/gui/player.rs:294`, also `:301` and `:340`.
    pub fn pause_fill() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/pause-straight-fill.svg"))
    }

    /// Play (`play-straight-fill.svg`): the play/pause button while playback
    /// is stopped, `src/gui/player.rs:342`.
    pub fn play_fill() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/play-straight-fill.svg"))
    }

    /// Previous track (`skip-previous-straight-fill.svg`),
    /// `src/gui/player.rs:370`.
    pub fn skip_previous_fill() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/skip-previous-straight-fill.svg"))
    }

    /// Next track (`skip-next-straight-fill.svg`), `src/gui/player.rs:380`.
    pub fn skip_next_fill() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/skip-next-straight-fill.svg"))
    }

    /// Volume, muted (`volume-off-straight-fill.svg`),
    /// `src/gui/player.rs:391`.
    pub fn volume_off_fill() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/volume-off-straight-fill.svg"))
    }

    /// Volume, low (`volume-down-straight-fill.svg`), `src/gui/player.rs:393`.
    pub fn volume_down_fill() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/volume-down-straight-fill.svg"))
    }

    /// Volume, high (`volume-up-straight-fill.svg`), `src/gui/player.rs:395`.
    pub fn volume_up_fill() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/volume-up-straight-fill.svg"))
    }
}

/// The playlist tab bar, the playlist track list and its toolbar and context
/// menus (`src/gui/playlist.rs`).
pub mod playlist {
    use super::{IconWidget, res};

    /// Play (`playlist-play-straight.svg`): the tab menu's "Reproducir" entry,
    /// `src/gui/playlist.rs:472`.
    pub fn playlist_play() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/playlist-play-straight.svg"))
    }

    /// Rename (`playlist-edit-straight.svg`): the tab menu's "Renombrar lista"
    /// entry, `src/gui/playlist.rs:487`.
    pub fn playlist_edit() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/playlist-edit-straight.svg"))
    }

    /// Delete playlist (`playlist-remove-straight.svg`): the tab menu's
    /// "Eliminar lista" entry, `src/gui/playlist.rs:498`.
    pub fn playlist_remove() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/playlist-remove-straight.svg"))
    }

    /// Save playlist (`playlist-add-check-straight.svg`): the tab menu's
    /// "Guardar lista" entry, `src/gui/playlist.rs:513`.
    pub fn playlist_add_check() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/playlist-add-check-straight.svg"))
    }

    /// Import playlist (`import-straight.svg`): the tab menu's "Importar lista"
    /// entry and the toolbar's import entry, `src/gui/playlist.rs:525`, `:594`.
    pub fn import() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/import-straight.svg"))
    }

    /// Export playlist (`export-straight.svg`): the tab menu's "Exportar lista"
    /// entry, `src/gui/playlist.rs:530`.
    pub fn export() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/export-straight.svg"))
    }

    /// Play the track (`play-circle-straight-outlined.svg`): the track menu's
    /// "Reproducir" entry, `src/gui/playlist.rs:1041`.
    pub fn play_circle_outlined() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/play-circle-straight-outlined.svg"))
    }

    /// Add files (`audio-file-outlined-straight.svg`): the "Agregar archivo"
    /// entry of the track menu and of the toolbar, `src/gui/playlist.rs:1051`,
    /// `:1129`.
    pub fn audio_file_outlined() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/audio-file-outlined-straight.svg"))
    }

    /// Add folder (`folder-add-outlined.svg`): the "Agregar carpeta" entry of
    /// the track menu and of the toolbar, `src/gui/playlist.rs:1056`, `:1134`.
    pub fn folder_add_outlined() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/folder-add-outlined.svg"))
    }

    /// Information (`info-outlined-straight.svg`): the track menu's
    /// "Información" entry, `src/gui/playlist.rs:1066`.
    pub fn info_outlined() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/info-outlined-straight.svg"))
    }

    /// Show the file's location (`show-library-outlined-straight.svg`): the
    /// track menu's "Mostrar en la biblioteca" entry,
    /// `src/gui/playlist.rs:1076`.
    pub fn show_library_outlined() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/show-library-outlined-straight.svg"))
    }

    /// Send to another playlist (`send-straight.svg`): the track menu's
    /// "Enviar a otra lista" entry, `src/gui/playlist.rs:1086`.
    pub fn send() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/send-straight.svg"))
    }

    /// Tag editor (`edit-tag.svg`): the track menu's "Editor de Etiquetas"
    /// entry, `src/gui/playlist.rs:1091`.
    pub fn edit_tag() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/edit-tag.svg"))
    }

    /// Audio converter (`convert.svg`): the track menu's "Convertidor de audio"
    /// entry, `src/gui/playlist.rs:1096`.
    pub fn convert() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/convert.svg"))
    }

    /// Enable or disable (`indicator-outlined.svg`): the track menu's
    /// "Activar | Desactivar" entry, `src/gui/playlist.rs:1106`.
    pub fn indicator_outlined() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/indicator-outlined.svg"))
    }

    /// Collapse or expand all folders (`expand-collapse-straight.svg`): the
    /// track menu's "Plegar | Desplegar" entry, `src/gui/playlist.rs:1111`.
    pub fn expand_collapse() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/expand-collapse-straight.svg"))
    }

    /// Remove from the playlist (`delete-oulined.svg`, sic: the file name is
    /// misspelt): the track menu's "Eliminar de la lista" entry,
    /// `src/gui/playlist.rs:1116`.
    pub fn delete_outlined() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/delete-oulined.svg"))
    }

    /// Shuffle (`shuffle-straight.svg`): the toolbar's shuffle toggle,
    /// `src/gui/playlist.rs:1160`.
    pub fn shuffle() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/shuffle-straight.svg"))
    }

    /// Repeat one (`repeat-one-straight-outlined.svg`): the toolbar's repeat
    /// button in the repeat-one mode, `src/gui/playlist.rs:1165`.
    pub fn repeat_one_outlined() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/repeat-one-straight-outlined.svg"))
    }

    /// Repeat (`repeat-straight-outlined.svg`): the toolbar's repeat button in
    /// the repeat-all and repeat-off modes, `src/gui/playlist.rs:1167`.
    pub fn repeat_outlined() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/repeat-straight-outlined.svg"))
    }

    /// Lyrics (`lyrics-straight-outlined.svg`): the toolbar's lyrics toggle,
    /// `src/gui/playlist.rs:1170`.
    pub fn lyrics_outlined() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/lyrics-straight-outlined.svg"))
    }
}

/// The library screen's title bar actions and view switcher
/// (`src/gui/library.rs`).
pub mod library {
    use super::{IconWidget, res};

    /// Grid view (`view-grid-outlined.svg`): the view-mode button while the
    /// library shows a grid, `src/gui/library.rs:1865`.
    pub fn view_grid_outlined() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/view-grid-outlined.svg"))
    }

    /// Thumbnail list, outlined (`view-list-thumbnail-outlined.svg`): a view
    /// mode of the view-mode button, `src/gui/library.rs:1866`.
    pub fn view_list_thumbnail_outlined() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/view-list-thumbnail-outlined.svg"))
    }

    /// Thumbnail list, filled (`view-list-thumbnail-fill.svg`): a view mode of
    /// the view-mode button, `src/gui/library.rs:1867`.
    pub fn view_list_thumbnail_fill() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/view-list-thumbnail-fill.svg"))
    }

    /// List (`view-list.svg`): a view mode of the view-mode button,
    /// `src/gui/library.rs:1868`.
    pub fn view_list() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/view-list.svg"))
    }

    /// Play the selection (`play-straight-outlined.svg`): the bottom action
    /// that plays the library selection, `src/gui/library.rs:1873`.
    pub fn play_outlined() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/play-straight-outlined.svg"))
    }

    /// More (`more-small.svg`): the bottom action that opens the folder picker,
    /// `src/gui/library.rs:1878`.
    pub fn more_small() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/more-small.svg"))
    }

    /// Show what is playing (`radio-button-on.svg`): the bottom action that
    /// scrolls to the playing track, `src/gui/library.rs:1880`.
    pub fn radio_button_on() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/radio-button-on.svg"))
    }
}

/// The library's filter panel (`src/gui/library_filters.rs`).
pub mod filters {
    use super::{IconWidget, res};

    /// Collapsed tree item (`arrow-right-chevron.svg`): a filter value that has
    /// children and is folded, `src/gui/library_filters.rs:473`.
    pub fn arrow_right_chevron() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/arrow-right-chevron.svg"))
    }

    /// Artist (`artist-outlined-straight.svg`): the artist filter's icon,
    /// `src/gui/library_filters.rs:253`.
    pub fn artist_outlined() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/artist-outlined-straight.svg"))
    }

    /// Genre (`music-note-2-straight.svg`): the genre filter's icon,
    /// `src/gui/library_filters.rs:255`.
    pub fn music_note_2() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/music-note-2-straight.svg"))
    }

    /// Year (`calendar-rounded.svg`): the year filter's icon,
    /// `src/gui/library_filters.rs:256`.
    pub fn calendar_rounded() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/calendar-rounded.svg"))
    }
}

/// The audio center window (`src/gui/audio_center.rs`).
pub mod audio_center {
    use super::{IconWidget, res};

    /// Close (`close-big.svg`): the audio center's 24 px close button,
    /// `src/gui/audio_center.rs:1851`.
    pub fn close_big() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/close-big.svg"))
    }

    /// Save preset (`save-outlined-straight.svg`): the equalizer preset row's
    /// save button, `src/gui/audio_center.rs:2848`.
    pub fn save_outlined() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/save-outlined-straight.svg"))
    }
}

/// The context menus the app shell builds (`src/gui/app.rs`).
pub mod app {
    use super::{IconWidget, res};

    /// Back (`double-arrow-left.svg`): the context menu's "Atrás" entry,
    /// `src/gui/app.rs:4005`.
    pub fn double_arrow_left() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/double-arrow-left.svg"))
    }

    /// A playlist (`playlist-music.svg`): one entry per playlist in the
    /// "send to playlist" menu, `src/gui/app.rs:2717`, `:4053`.
    pub fn playlist_music() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/playlist-music.svg"))
    }
}

/// Icons two or more of the areas above use (`src/gui/widgets.rs` and the
/// screens that share it).
pub mod shared {
    use super::{IconWidget, res};

    /// Album (`album.svg`): the album filter's icon (`src/gui/library_filters.rs:254`)
    /// and the cover placeholder (`src/gui/widgets.rs:1478`, `:1506`).
    pub fn album() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/album.svg"))
    }

    /// Up chevron (`arrow-up-chevron.svg`): the sort direction
    /// (`src/gui/widgets.rs:1713`, `:2325`), an open dropdown
    /// (`src/gui/library.rs:1597`, `src/gui/library_filters.rs:188`,
    /// `src/gui/playlist.rs:616`) and an expanded section (`src/gui/widgets.rs:2244`).
    pub fn arrow_up_chevron() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/arrow-up-chevron.svg"))
    }

    /// Down chevron (`arrow-down-chevron.svg`): the counterpart of
    /// [`arrow_up_chevron`], `src/gui/widgets.rs:1715`, `:2242`, `:2327`,
    /// `src/gui/library.rs:1599`, `src/gui/library_filters.rs:190`, `:471`,
    /// `src/gui/playlist.rs:618`.
    pub fn arrow_down_chevron() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/arrow-down-chevron.svg"))
    }

    /// New playlist (`playlist-add-straight.svg`): the "Nueva lista" entry of
    /// the playlist tab menu and of the context menus, `src/gui/playlist.rs:477`,
    /// `:584`, `src/gui/app.rs:2680`, `:4017`.
    pub fn playlist_add() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/playlist-add-straight.svg"))
    }

    /// Folder (`folder-open-outlined.svg`): the folder filter's icon
    /// (`src/gui/library_filters.rs:252`) and the track menu's "Ubicación del
    /// archivo" entry (`src/gui/playlist.rs:1071`).
    pub fn folder_open_outlined() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/folder-open-outlined.svg"))
    }

    /// Equalizer (`equalizer-straight.svg`): the playlist toolbar's equalizer
    /// button (`src/gui/playlist.rs:1155`) and the equalizer preset row
    /// (`src/gui/audio_center.rs:2841`).
    pub fn equalizer() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/equalizer-straight.svg"))
    }

    /// Restore (`restore-straight.svg`): the filter panel's restore button
    /// (`src/gui/library_filters.rs:364`) and the equalizer's reset button
    /// (`src/gui/audio_center.rs:2855`).
    pub fn restore() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/restore-straight.svg"))
    }
}

/// A factory of one icon, as listed in [`ALL`].
pub type IconFactory = fn() -> IconWidget;

/// Every icon the Iced UI uses: `(file name, factory)`.
pub const ALL: &[(&str, IconFactory)] = &[
    ("minimize.svg", window::minimize),
    ("maximize.svg", window::maximize),
    ("close.svg", window::close),
    ("menu.svg", player::menu),
    ("pause-straight-fill.svg", player::pause_fill),
    ("play-straight-fill.svg", player::play_fill),
    (
        "skip-previous-straight-fill.svg",
        player::skip_previous_fill,
    ),
    ("skip-next-straight-fill.svg", player::skip_next_fill),
    ("volume-off-straight-fill.svg", player::volume_off_fill),
    ("volume-down-straight-fill.svg", player::volume_down_fill),
    ("volume-up-straight-fill.svg", player::volume_up_fill),
    ("playlist-play-straight.svg", playlist::playlist_play),
    ("playlist-edit-straight.svg", playlist::playlist_edit),
    ("playlist-remove-straight.svg", playlist::playlist_remove),
    (
        "playlist-add-check-straight.svg",
        playlist::playlist_add_check,
    ),
    ("import-straight.svg", playlist::import),
    ("export-straight.svg", playlist::export),
    (
        "play-circle-straight-outlined.svg",
        playlist::play_circle_outlined,
    ),
    (
        "audio-file-outlined-straight.svg",
        playlist::audio_file_outlined,
    ),
    ("folder-add-outlined.svg", playlist::folder_add_outlined),
    ("info-outlined-straight.svg", playlist::info_outlined),
    (
        "show-library-outlined-straight.svg",
        playlist::show_library_outlined,
    ),
    ("send-straight.svg", playlist::send),
    ("edit-tag.svg", playlist::edit_tag),
    ("convert.svg", playlist::convert),
    ("indicator-outlined.svg", playlist::indicator_outlined),
    ("expand-collapse-straight.svg", playlist::expand_collapse),
    ("delete-oulined.svg", playlist::delete_outlined),
    ("shuffle-straight.svg", playlist::shuffle),
    (
        "repeat-one-straight-outlined.svg",
        playlist::repeat_one_outlined,
    ),
    ("repeat-straight-outlined.svg", playlist::repeat_outlined),
    ("lyrics-straight-outlined.svg", playlist::lyrics_outlined),
    ("view-grid-outlined.svg", library::view_grid_outlined),
    (
        "view-list-thumbnail-outlined.svg",
        library::view_list_thumbnail_outlined,
    ),
    (
        "view-list-thumbnail-fill.svg",
        library::view_list_thumbnail_fill,
    ),
    ("view-list.svg", library::view_list),
    ("play-straight-outlined.svg", library::play_outlined),
    ("more-small.svg", library::more_small),
    ("radio-button-on.svg", library::radio_button_on),
    ("arrow-right-chevron.svg", filters::arrow_right_chevron),
    ("artist-outlined-straight.svg", filters::artist_outlined),
    ("music-note-2-straight.svg", filters::music_note_2),
    ("calendar-rounded.svg", filters::calendar_rounded),
    ("close-big.svg", audio_center::close_big),
    ("save-outlined-straight.svg", audio_center::save_outlined),
    ("double-arrow-left.svg", app::double_arrow_left),
    ("playlist-music.svg", app::playlist_music),
    ("album.svg", shared::album),
    ("arrow-up-chevron.svg", shared::arrow_up_chevron),
    ("arrow-down-chevron.svg", shared::arrow_down_chevron),
    ("playlist-add-straight.svg", shared::playlist_add),
    ("folder-open-outlined.svg", shared::folder_open_outlined),
    ("equalizer-straight.svg", shared::equalizer),
    ("restore-straight.svg", shared::restore),
];

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::Path;

    use teksilo::core::WidgetTree;
    use teksilo::widgets::{FixedSize, HStack, Spacer};

    use super::ALL;
    use crate::teksilo_ui::test_support::{first_of_type, layout};

    /// The `.svg` string literals in `src/gui/*.rs`, by file name. A literal is
    /// the text between two double quotes, so the scan reads the odd-numbered
    /// pieces of each file split on `"`.
    fn svg_names_used_by_the_iced_ui() -> BTreeSet<String> {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/gui");
        let mut names = BTreeSet::new();
        for entry in std::fs::read_dir(&dir).expect("src/gui is readable") {
            let path = entry.expect("a directory entry in src/gui").path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("a Rust source in src/gui");
            for literal in text.split('"').skip(1).step_by(2) {
                if literal.ends_with(".svg") {
                    let name = literal.rsplit('/').next().unwrap_or(literal);
                    names.insert(name.to_string());
                }
            }
        }
        names
    }

    #[test]
    fn every_icon_lays_out_at_the_size_it_is_given() {
        for &(name, factory) in ALL {
            let mut tree = WidgetTree::new();
            let square = tree.add(
                FixedSize::new()
                    .width(24.0)
                    .height(24.0)
                    .child(factory().icon_size(24.0)),
            );
            tree.add(HStack::new().child(square).child(Spacer::new()));
            layout(&mut tree);
            let icon = first_of_type(&tree, square, "IconWidget")
                .unwrap_or_else(|| panic!("{name}: the icon is in the tree"));
            let bounds = tree.bounds(icon);
            assert_eq!(
                (bounds.width, bounds.height),
                (24.0, 24.0),
                "{name} lays out at the 24 px it is given"
            );
        }
    }

    #[test]
    fn every_svg_the_iced_ui_references_has_a_factory() {
        let used = svg_names_used_by_the_iced_ui();
        let factories: BTreeSet<String> = ALL.iter().map(|(name, _)| name.to_string()).collect();
        let missing: Vec<&String> = used.difference(&factories).collect();
        let extra: Vec<&String> = factories.difference(&used).collect();
        assert!(
            missing.is_empty() && extra.is_empty(),
            "icons used by src/gui without a factory: {missing:?}; \
             factories whose file src/gui no longer uses: {extra:?}"
        );
    }

    #[test]
    fn the_factories_are_listed_once() {
        let names: BTreeSet<&str> = ALL.iter().map(|(name, _)| *name).collect();
        assert_eq!(names.len(), ALL.len(), "no file name is listed twice");
        let factories: BTreeSet<usize> = ALL.iter().map(|(_, factory)| *factory as usize).collect();
        assert_eq!(factories.len(), ALL.len(), "no factory is listed twice");
    }
}
