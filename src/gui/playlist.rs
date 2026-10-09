use crate::audio::AudioManager;
use crate::db::database::{PlaylistFolderGroup, PlaylistSongRef, ShuffleSession};
use crate::gui::app::Message;
use crate::gui::theme::*;
use crate::gui::widgets::{standard_scrollable, standard_scrollbar};
use crate::utils::format_duration;
use iced::{
    Alignment, Background, Color, Element, Length, Padding, Theme,
    widget::{Space, button, column, container, mouse_area, row, svg, text},
};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

// Contadores de redraw para diagnóstico
static PLAYLIST_VIEW_REDRAWS: AtomicU64 = AtomicU64::new(0);

// ============================================================
// Constantes y IDs
// ============================================================

pub static PLAYLIST_SCROLL_ID: std::sync::LazyLock<iced::widget::Id> =
    std::sync::LazyLock::new(iced::widget::Id::unique);
pub static PLAYLIST_TABS_SCROLL_ID: std::sync::LazyLock<iced::widget::Id> =
    std::sync::LazyLock::new(iced::widget::Id::unique);
pub static PLAYLIST_SEARCH_ID: std::sync::LazyLock<iced::widget::Id> =
    std::sync::LazyLock::new(|| iced::widget::Id::new("playlist_search_input"));

// ============================================================
// Tipos auxiliares
// ============================================================

pub use crate::logic::playlist_nav::PlaylistItemType;
use crate::logic::playlist_nav::{self as nav, PlaylistSelection, RepeatMode};
use crate::logic::selection_modifiers::SelectModifiers;

// ============================================================
// Estado del PlaylistManager
// ============================================================

#[derive(Clone)]
pub struct PlaylistManager {
    /// ID de la playlist activa (referencia a BD)
    pub active_playlist_id: i64,

    /// Canciones de la playlist activa, agrupadas por carpeta
    pub groups: Vec<PlaylistFolderGroup>,

    /// Índice de la canción que se está reproduciendo (dentro del grupo aplanado)
    pub playing_song_idx: Option<usize>,

    /// Selected items in the current playlist
    pub selected_idxs: std::collections::HashSet<usize>,

    /// Currently focused item for keyboard navigation
    pub focused_idx: Option<usize>,

    /// Selection pivot for Shift-range selection
    pub selection_pivot: Option<usize>,

    /// Estado shuffle/repeat
    pub shuffle_active: bool,
    pub repeat_mode: u8, // 0=Off, 1=All, 2=One

    /// Búsqueda local dentro de la playlist
    pub search_query: String,
    pub filtered_groups: Option<Vec<PlaylistFolderGroup>>,

    /// Estado de la barra de pestañas
    pub show_tab_dropdown: bool,

    /// IDs de pestañas visibles (para overflow management)
    pub visible_tabs_count: Option<usize>,

    /// Sesión de shuffle activa
    pub shuffle_session: Option<ShuffleSession>,

    /// Lista de todas las playlists disponibles (para la barra de pestañas)
    pub playlists: Vec<crate::db::database::PlaylistData>,

    /// Carpetas plegadas (IDs de la primera canción del grupo como anchor)
    pub collapsed_groups: std::collections::HashSet<i64>,

    /// Información del último clic para detectar doble clic (linear_idx, instant)
    pub last_click_info: Option<(usize, std::time::Instant)>,

    /// Guardar el último viewport (ventana visible) para autoscroll
    pub last_viewport: Option<iced::Rectangle>,

    /// Guardar el último viewport de la barra de pestañas
    pub tabs_viewport: Option<iced::Rectangle>,

    /// Caché de ítems visibles (aplanados y filtrados por colapso) compartida vía Arc
    pub cached_visible_items: std::cell::RefCell<Option<Arc<Vec<(PlaylistItemType, usize)>>>>,

    /// Indica si los datos pesados han sido descargados de RAM por inactividad
    pub data_unloaded: bool,
}

impl Default for PlaylistManager {
    fn default() -> Self {
        Self {
            active_playlist_id: 1,
            groups: Vec::new(),
            playing_song_idx: None,
            selected_idxs: std::collections::HashSet::new(),
            focused_idx: None,
            selection_pivot: None,
            shuffle_active: false,
            repeat_mode: 0,
            search_query: String::new(),
            filtered_groups: None,
            show_tab_dropdown: false,
            visible_tabs_count: None,
            shuffle_session: None,
            playlists: Vec::new(),
            collapsed_groups: std::collections::HashSet::new(),
            last_click_info: None,
            last_viewport: None,
            tabs_viewport: None,
            cached_visible_items: std::cell::RefCell::new(None),
            data_unloaded: false,
        }
    }
}

/// Contar items lineales (separadores + canciones)
pub use crate::logic::playlist_nav::count_linear_items;
/// Aplanar todos los grupos en un solo índice lineal
pub use crate::logic::playlist_nav::flatten_groups;

/// Traduce los modificadores de Iced a los de `logic` (Ctrl/Cmd alterna,
/// Shift selecciona rango).
fn select_modifiers(modifiers: iced::keyboard::Modifiers) -> SelectModifiers {
    SelectModifiers {
        toggle: modifiers.command(),
        range: modifiers.shift(),
    }
}

impl PlaylistManager {
    pub fn apply_filter(&mut self) {
        self.filtered_groups = nav::filter_groups(&self.groups, &self.search_query);
        self.invalidate_cache();
    }

    /// Obtiene la siguiente canción para precarga de carátulas y audio.
    ///
    /// Usa la misma decisión que `play_next` (`logic::playlist_nav::peek_next`,
    /// incluido el salto de canciones deshabilitadas), de modo que la pre-carga
    /// nunca carga una canción distinta a la que realmente se reproducirá.
    pub fn get_next_song_ref(&self) -> Option<&PlaylistSongRef> {
        nav::next_song_ref(
            self.active_groups(),
            self.playing_song_idx,
            RepeatMode::from_u8(self.repeat_mode),
            self.shuffle_active,
            self.shuffle_session.as_ref(),
        )
    }

    /// Obtener los grupos activos (filtrados o no)
    pub fn active_groups(&self) -> &[PlaylistFolderGroup] {
        self.filtered_groups.as_ref().unwrap_or(&self.groups)
    }

    /// Establece los grupos de la playlist e invalida el caché
    pub fn set_groups(&mut self, groups: Vec<PlaylistFolderGroup>) {
        self.groups = groups;
        self.invalidate_cache();
    }

    /// Limpia los grupos e invalida el caché
    pub fn clear_groups(&mut self) {
        self.groups.clear();
        self.filtered_groups = None;
        self.invalidate_cache();
    }

    /// Descarga los datos pesados de la RAM si no se están usando
    pub fn unload(&mut self, is_focused: bool, is_playing: bool, is_active_tab: bool) {
        if is_focused || is_playing || is_active_tab {
            return;
        }

        if !self.groups.is_empty() {
            tracing::debug!("Audoxidy GC: Unloading inactive Playlist data.");
            self.groups.clear();
            self.groups.shrink_to_fit();
            self.filtered_groups = None;
            self.cached_visible_items.borrow_mut().take();
            self.data_unloaded = true;
        }
    }

    /// Obtener la canción en un índice lineal
    pub fn get_song_at_linear_index(&self, linear_idx: usize) -> Option<&PlaylistSongRef> {
        nav::song_at(self.active_groups(), linear_idx)
    }

    /// Invalida el caché de ítems visibles
    pub fn invalidate_cache(&mut self) {
        self.cached_visible_items.replace(None);
    }

    /// Obtener los ítems que son actualmente visibles (no están dentro de grupos plegados)
    /// Retorna una copia compartida (Arc) de los ítems visibles (Tipo, GlobalLinearIndex)
    /// Es O(1) y evita clonar el vector completo.
    pub fn get_visible_items(&self) -> Arc<Vec<(PlaylistItemType, usize)>> {
        let mut cache = self.cached_visible_items.borrow_mut();
        cache
            .get_or_insert_with(|| {
                Arc::new(nav::visible_items(
                    self.active_groups(),
                    &self.collapsed_groups,
                ))
            })
            .clone()
    }

    /// Alternar expansion de grupo
    pub fn toggle_group_expansion(&mut self, anchor: i64) {
        nav::toggle_group_collapsed(&mut self.collapsed_groups, anchor);
        self.invalidate_cache();
    }

    /// Obtiene información de un item en base a su índice lineal (Canónico)
    pub fn get_item_info_at_linear_index(&self, linear_idx: usize) -> Option<PlaylistItemType> {
        nav::item_at(self.active_groups(), linear_idx)
    }

    /// Buscar el índice lineal del separador para un anchor (first_item_id)
    pub fn find_separator_index_for_anchor(&self, anchor: i64) -> Option<usize> {
        nav::separator_index_for_anchor(self.active_groups(), anchor)
    }

    /// Mueve la selección (selected_idxs, focused_idx, selection_pivot) a un
    /// `PlaylistSelection` de `logic`, aplica `f` y la devuelve a sus campos.
    fn with_selection<R>(&mut self, f: impl FnOnce(&mut Self, &mut PlaylistSelection) -> R) -> R {
        let mut selection = PlaylistSelection {
            selected: std::mem::take(&mut self.selected_idxs),
            focused: self.focused_idx,
            pivot: self.selection_pivot,
        };
        let result = f(self, &mut selection);
        self.selected_idxs = selection.selected;
        self.focused_idx = selection.focused;
        self.selection_pivot = selection.pivot;
        result
    }

    /// Multi-selection: handles keyboard navigation with modifiers (Shift for range)
    pub fn handle_key_nav(
        &mut self,
        dir: crate::gui::library::LibraryNavDir,
        modifiers: iced::keyboard::Modifiers,
    ) {
        let visible_items = self.get_visible_items();
        if visible_items.is_empty() {
            return;
        }

        match dir {
            crate::gui::library::LibraryNavDir::Up => self.with_selection(|_, sel| {
                sel.navigate(nav::VerticalDir::Up, modifiers.shift(), &visible_items)
            }),
            crate::gui::library::LibraryNavDir::Down => self.with_selection(|_, sel| {
                sel.navigate(nav::VerticalDir::Down, modifiers.shift(), &visible_items)
            }),
            crate::gui::library::LibraryNavDir::Left => {
                let changed = self.with_selection(|pm, sel| {
                    let groups = pm.filtered_groups.as_deref().unwrap_or(&pm.groups);
                    nav::collapse_selected_groups(groups, &mut pm.collapsed_groups, sel)
                });
                if changed {
                    self.invalidate_cache();
                }
            }
            crate::gui::library::LibraryNavDir::Right => {
                let changed = self.with_selection(|pm, sel| {
                    let groups = pm.filtered_groups.as_deref().unwrap_or(&pm.groups);
                    nav::expand_selected_groups(groups, &mut pm.collapsed_groups, sel)
                });
                if changed {
                    self.invalidate_cache();
                }
            }
            _ => {}
        }
    }

    pub fn select_all(&mut self) {
        let visible_items = self.get_visible_items();
        self.with_selection(|_, sel| sel.select_all(&visible_items));
    }

    pub fn handle_click(&mut self, linear_idx: usize, modifiers: iced::keyboard::Modifiers) {
        let visible_items = self.get_visible_items();
        self.with_selection(|_, sel| {
            sel.click(linear_idx, select_modifiers(modifiers), &visible_items)
        });
    }

    /// Obtener canción en un índice lineal (mutable)
    pub fn get_song_at_linear_index_mut(
        &mut self,
        linear_idx: usize,
    ) -> Option<&mut PlaylistSongRef> {
        let groups = match self.filtered_groups.as_mut() {
            Some(filtered) => filtered,
            None => &mut self.groups,
        };
        nav::song_at_mut(groups, linear_idx)
    }

    /// Obtener información del grupo en un índice lineal
    pub fn get_group_info_at_linear_index(&self, linear_idx: usize) -> Option<(usize, bool)> {
        nav::group_info_at(self.active_groups(), linear_idx)
    }

    /// Toggle enabled de una canción por índice lineal
    pub fn toggle_song_enabled_at_linear_index(&mut self, linear_idx: usize) -> Option<bool> {
        nav::toggle_song_enabled(
            &mut self.groups,
            self.filtered_groups.as_deref_mut(),
            linear_idx,
        )
    }

    pub fn set_song_enabled_at_linear_index(&mut self, linear_idx: usize, enabled: bool) {
        nav::set_song_enabled(
            &mut self.groups,
            self.filtered_groups.as_deref_mut(),
            linear_idx,
            enabled,
        );
    }

    /// Toggle habilitado de todo un grupo
    pub fn toggle_group_enabled_at_linear_index(&mut self, linear_idx: usize) {
        nav::toggle_group_enabled(
            &mut self.groups,
            self.filtered_groups.as_deref_mut(),
            linear_idx,
        );
    }

    pub fn set_group_enabled_at_linear_index(&mut self, linear_idx: usize, enabled: bool) {
        nav::set_group_enabled(
            &mut self.groups,
            self.filtered_groups.as_deref_mut(),
            linear_idx,
            enabled,
        );
    }

    pub fn get_first_enabled_song_idx(&self) -> Option<usize> {
        nav::find_next_enabled(
            self.active_groups(),
            None,
            RepeatMode::from_u8(self.repeat_mode) == RepeatMode::All,
        )
    }

    /// Returns the list of song IDs currently selected (including songs within selected folders)
    /// Preserves the order in which they appear in the playlist.
    pub fn get_selected_song_ids(&self) -> Vec<i64> {
        nav::selected_song_ids(self.active_groups(), &self.selected_idxs)
    }

    /// Obtiene el índice lineal de una canción por su song_id de base de datos.
    pub fn get_linear_index_by_song_id(&self, song_id: i64) -> Option<usize> {
        nav::linear_index_of_song_id(self.active_groups(), song_id)
    }

    /// Activa el modo shuffle generando una nueva sesión aleatoria.
    pub fn activate_shuffle(&mut self) {
        let groups = self.filtered_groups.as_deref().unwrap_or(&self.groups);
        nav::activate_shuffle(
            groups,
            self.playing_song_idx,
            &mut self.shuffle_active,
            &mut self.shuffle_session,
            &mut fastrand::Rng::new(),
        );
    }

    /// Desactiva el modo shuffle.
    pub fn deactivate_shuffle(&mut self) {
        self.shuffle_active = false;
        self.shuffle_session = None;
    }

    /// Calcula el rango [top, bottom] en píxeles de un ítem basado en su índice lineal.
    /// Si una canción está en una carpeta plegada, devuelve la posición del separador.
    pub fn get_item_y_bounds(&self, target_idx: usize) -> (f32, f32) {
        nav::item_y_bounds(self.active_groups(), &self.collapsed_groups, target_idx)
    }
}

// ============================================================
// Vista principal del módulo de Playlist
// ============================================================

const TABS_BAR_HEIGHT: f32 = 40.0;
const BOTTOM_BAR_HEIGHT: f32 = 40.0;
const TABS_PADDING_H: u16 = 7;

pub fn view<'a>(
    manager: &'a PlaylistManager,
    _audio_manager: &AudioManager,
) -> Element<'a, Message> {
    let count = PLAYLIST_VIEW_REDRAWS.fetch_add(1, Ordering::Relaxed) + 1;
    if count % 100 == 0 {
        tracing::info!("Playlist view redraws: {}", count);
    }
    let tabs_container = build_tabs_bar(manager);
    let playlist_scroll = build_song_list(manager);
    let bottom_container = build_bottom_bar(manager);

    mouse_area(
        container(column![tabs_container, playlist_scroll, bottom_container])
            .width(Length::Fixed(400.0))
            .height(Length::Fill)
            .style(|_t: &Theme| container::Style::default().background(COLOR_BG)),
    )
    .on_press(Message::PlaylistDeselect)
    .into()
}

// ============================================================
// Barra superior de pestañas
// ============================================================

fn build_tabs_bar<'a>(manager: &'a PlaylistManager) -> Element<'a, Message> {
    let playlists = &manager.playlists;

    // Use zero padding internally so the scrollable bounds determine visibility
    let mut tabs_row = row![].spacing(5).padding([0, 0]).align_y(Alignment::Center);

    let mut total_tabs_width: f32 = 0.0;

    for p_data in playlists.iter() {
        let name = &p_data.name;
        let id = p_data.id;

        let is_active = id == manager.active_playlist_id;
        let tab_color = if is_active {
            COLOR_TEXT_PRIMARY
        } else {
            COLOR_TEXT_SECONDARY
        };
        let tab = text(name.clone())
            .size(14)
            .color(tab_color)
            .font(FONT_INTER_SANS_MEDIUM);

        let tab_btn = button(tab)
            .padding([5, 5])
            .style(|_t: &Theme, _s| button::Style::default().with_background(Color::TRANSPARENT))
            .on_press(Message::SwitchPlaylist(id, false));

        let tab_entries = vec![
            crate::gui::widgets::ContextMenuEntry {
                label: "Reproducir".to_string(),
                icon: Some("playlist-play-straight.svg".to_string()),
                action: Some(Message::SwitchPlaylist(id, true)),
            },
            crate::gui::widgets::ContextMenuEntry {
                label: "Nueva lista".to_string(),
                icon: Some("playlist-add-straight.svg".to_string()),
                action: Some(Message::OpenDialog(
                    crate::gui::app::ActiveDialog::CreatePlaylist {
                        name: "".into(),
                        pending_add_songs: Vec::new(),
                    },
                )),
            },
            crate::gui::widgets::ContextMenuEntry {
                label: "Renombrar lista".to_string(),
                icon: Some("playlist-edit-straight.svg".to_string()),
                action: Some(Message::OpenDialog(
                    crate::gui::app::ActiveDialog::RenamePlaylist {
                        id,
                        current_name: name.clone(),
                        new_name: name.clone(),
                    },
                )),
            },
            crate::gui::widgets::ContextMenuEntry {
                label: "Eliminar lista".to_string(),
                icon: Some("playlist-remove-straight.svg".to_string()),
                action: Some(Message::OpenDialog(
                    crate::gui::app::ActiveDialog::DeleteConfirm {
                        id,
                        name: name.clone(),
                    },
                )),
            },
            crate::gui::widgets::ContextMenuEntry {
                label: "".to_string(), // Divisor
                icon: None,
                action: None,
            },
            crate::gui::widgets::ContextMenuEntry {
                label: "Guardar lista".to_string(),
                icon: Some("playlist-add-check-straight.svg".to_string()),
                action: Some(Message::OpenDialog(
                    crate::gui::app::ActiveDialog::ExportConfirm {
                        id,
                        name: name.clone(),
                        format: crate::gui::app::ExportFormat::M3U8,
                        mode: crate::gui::app::ExportMode::SingleFile,
                    },
                )),
            },
            crate::gui::widgets::ContextMenuEntry {
                label: "Importar lista".to_string(),
                icon: Some("import-straight.svg".to_string()),
                action: Some(Message::OpenPlaylistFilePicker),
            },
            crate::gui::widgets::ContextMenuEntry {
                label: "Exportar lista".to_string(),
                icon: Some("export-straight.svg".to_string()),
                action: Some(Message::OpenDialog(
                    crate::gui::app::ActiveDialog::ExportConfirm {
                        id,
                        name: name.clone(),
                        format: crate::gui::app::ExportFormat::M3U8,
                        mode: crate::gui::app::ExportMode::PortableFolder,
                    },
                )),
            },
        ];

        tabs_row = tabs_row.push(
            mouse_area(tab_btn).on_right_press(Message::RequestContextMenu(tab_entries.clone())),
        );
        total_tabs_width += estimate_tab_width(name) + 5.0;
    }

    // Usamos padding 12 a la izquierda para el corte visual, y el limite real a la derecha
    let bar_width = 380.0;
    let has_overflow = total_tabs_width > bar_width;

    let scrollable_tabs = iced::widget::scrollable(tabs_row)
        .id(PLAYLIST_TABS_SCROLL_ID.clone())
        .direction(iced::widget::scrollable::Direction::Horizontal(
            iced::widget::scrollable::Scrollbar::new()
                .width(0)
                .scroller_width(0)
                .margin(0),
        ))
        .on_scroll(Message::PlaylistTabsScrolled);

    let mut bar_content =
        row![
            container(scrollable_tabs)
                .width(Length::Fill)
                .padding(iced::Padding {
                    top: 0.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 12.0
                }),
        ]
        .align_y(Alignment::Center);

    if has_overflow {
        bar_content = bar_content.push(build_tab_dropdown_button(manager));
    }

    let content = column![bar_content];

    let bar_entries = vec![
        crate::gui::widgets::ContextMenuEntry {
            label: "Nueva lista".to_string(),
            icon: Some("playlist-add-straight.svg".to_string()),
            action: Some(Message::OpenDialog(
                crate::gui::app::ActiveDialog::CreatePlaylist {
                    name: "".into(),
                    pending_add_songs: Vec::new(),
                },
            )),
        },
        crate::gui::widgets::ContextMenuEntry {
            label: "Importar lista".to_string(),
            icon: Some("import-straight.svg".to_string()),
            action: Some(Message::OpenPlaylistFilePicker),
        },
    ];

    mouse_area(
        container(content)
            .width(Length::Fill)
            .height(Length::Fixed(TABS_BAR_HEIGHT))
            .align_y(iced::alignment::Vertical::Center)
            .style(|_t: &Theme| container::Style::default().background(COLOR_CONTRAST)),
    )
    .on_right_press(Message::RequestContextMenu(bar_entries))
    .into()
}

pub fn estimate_tab_width(name: &str) -> f32 {
    name.chars().count() as f32 * 7.5 + 10.0
}

fn build_tab_dropdown_button<'a>(manager: &'a PlaylistManager) -> Element<'a, Message> {
    let icon_path = if manager.show_tab_dropdown {
        "assets/icons/arrow-up-chevron.svg"
    } else {
        "assets/icons/arrow-down-chevron.svg"
    };

    let chevron = svg(svg::Handle::from_path(icon_path))
        .width(28)
        .height(28)
        .style(|_t: &Theme, _s| svg::Style {
            color: Some(COLOR_TEXT_SECONDARY),
        });

    button(chevron)
        .padding(4)
        .style(|_t: &Theme, _s| button::Style::default().with_background(Color::TRANSPARENT))
        .on_press(Message::ToggleTabDropdown)
        .into()
}

fn build_dropdown_menu<'a>(
    playlists: &[crate::db::database::PlaylistData],
    visible_count: usize,
) -> Element<'a, Message> {
    let mut items = column![].padding([5, TABS_PADDING_H as u16]).spacing(4);

    for p_data in playlists.iter().skip(visible_count) {
        let name = &p_data.name;
        let id = p_data.id;

        let item = text(name.clone())
            .size(14)
            .color(COLOR_TEXT_SECONDARY)
            .font(FONT_INTER_SANS_MEDIUM);

        let item_btn = button(item)
            .width(Length::Fill)
            .padding([5, 5])
            .style(|_t: &Theme, _s| button::Style::default().with_background(COLOR_BG))
            .on_press(Message::SwitchPlaylist(id, false));

        items = items.push(item_btn);
    }

    container(items)
        .width(Length::Fill)
        .style(|_t: &Theme| container::Style {
            background: Some(Background::Color(COLOR_CONTRAST)),
            ..container::Style::default()
        })
        .into()
}

// ============================================================
// Lista de canciones con separadores de carpeta
// ============================================================

use crate::logic::playlist_nav::{FOLDER_SEPARATOR_HEIGHT, SONG_ROW_HEIGHT};

fn build_song_list<'a>(manager: &'a PlaylistManager) -> Element<'a, Message> {
    let groups = manager.active_groups();
    if groups.is_empty() {
        return mouse_area(
            container(
                text("No hay canciones en esta lista")
                    .size(16)
                    .color(COLOR_TEXT_SECONDARY)
                    .font(FONT_INTER_SANS_MEDIUM),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .padding(15),
        )
        .on_enter(Message::PlaylistMouseOver(true))
        .on_exit(Message::PlaylistMouseOver(false))
        .on_press(Message::PlaylistDeselect)
        .on_right_press(Message::RequestContextMenu(
            get_empty_playlist_context_menu_entries(),
        ))
        .into();
    }

    // 1. Obtener ítems expandidos (visibles en la estructura lógica)
    let visible_items = manager.get_visible_items();
    let total_items = visible_items.len();

    // 2. Parámetros de virtualización
    let viewport = manager.last_viewport.unwrap_or(iced::Rectangle {
        x: 0.0,
        y: 0.0,
        width: 800.0,
        height: 800.0,
    });
    let view_min = viewport.y;
    let view_max = view_min + viewport.height;

    // Margen de renderizado reducido para menos redraws
    let margin = if crate::utils::is_low_resource() {
        50.0
    } else {
        150.0
    };
    let render_min = (view_min - margin).max(0.0);
    let render_max = view_max + margin;

    let item_h = SONG_ROW_HEIGHT; // Ambas son 46.0

    // 3. Identificar rango de índices a renderizar
    let start_idx = (render_min / item_h).floor() as usize;
    let end_idx = (render_max / item_h).ceil() as usize;

    let start_idx = start_idx.min(total_items);
    let end_idx = end_idx.min(total_items);

    let top_space = start_idx as f32 * item_h;
    let bottom_space = (total_items.saturating_sub(end_idx)) as f32 * item_h;

    let mut songs_col = column![].spacing(0).padding(Padding {
        top: 0.0,
        right: 9.0,
        bottom: 0.0,
        left: 0.0,
    });

    // Espaciador superior
    if top_space > 0.0 {
        songs_col = songs_col.push(Space::new().height(Length::Fixed(top_space)));
    }

    // Renderizar solo los ítems en el rango visible
    for (item_type, linear_idx) in &visible_items[start_idx..end_idx] {
        match item_type {
            PlaylistItemType::Separator(g_idx, _) => {
                if let Some(group) = groups.get(*g_idx) {
                    let is_selected = manager.selected_idxs.contains(linear_idx);
                    let any_song_enabled = group.songs.iter().any(|s| s.enabled);
                    let total_dur = format_duration(group.total_duration);

                    songs_col = songs_col.push(build_folder_separator(
                        &group.folder_name,
                        group.songs.len(),
                        total_dur,
                        *linear_idx,
                        is_selected,
                        any_song_enabled,
                    ));
                }
            }
            PlaylistItemType::Song(g_idx, s_idx, _) => {
                if let Some(group) = groups.get(*g_idx) {
                    if let Some(song) = group.songs.get(*s_idx) {
                        let is_playing = manager.playing_song_idx == Some(*linear_idx);
                        let is_selected = manager.selected_idxs.contains(linear_idx);

                        // Un ítem de canción se considera "dentro de grupo seleccionado" si el separador del grupo lo está
                        let sep_idx = manager
                            .find_separator_index_for_anchor(group.first_item_id)
                            .unwrap_or(0);
                        let is_group_selected = manager.selected_idxs.contains(&sep_idx);

                        songs_col = songs_col.push(build_song_row(
                            song,
                            *linear_idx,
                            is_playing,
                            is_selected,
                            is_group_selected,
                        ));
                    }
                }
            }
        }
    }

    // Espaciador inferior
    if bottom_space > 0.0 {
        songs_col = songs_col.push(Space::new().height(Length::Fixed(bottom_space)));
    }

    let scroll = standard_scrollable(
        PLAYLIST_SCROLL_ID.clone(),
        songs_col,
        iced::widget::scrollable::Direction::Vertical(standard_scrollbar()),
    )
    .height(Length::Fill)
    .on_scroll(Message::PlaylistScrolled);

    mouse_area(scroll)
        .on_enter(Message::PlaylistMouseOver(true))
        .on_exit(Message::PlaylistMouseOver(false))
        .on_press(Message::PlaylistDeselect)
        .on_right_press(Message::RequestContextMenu(
            get_empty_playlist_context_menu_entries(),
        ))
        .into()
}

fn build_folder_separator<'a>(
    folder_name: &'a str,
    song_count: usize,
    total_duration: String,
    linear_idx: usize,
    is_selected: bool,
    is_enabled: bool,
) -> Element<'a, Message> {
    let indicator_color = if is_enabled {
        COLOR_TEXT_SECONDARY
    } else {
        Color::TRANSPARENT
    };

    let indicator = text("•").size(19).color(indicator_color);

    let name_text = crate::gui::widgets::smart_truncate_text(
        folder_name.to_string(),
        14.0,
        FONT_INTER_SANS_MEDIUM,
        COLOR_TEXT_SECONDARY,
    );

    let stats_text = text(format!("{} | {}", song_count, total_duration))
        .size(13)
        .color(COLOR_TEXT_SECONDARY)
        .font(FONT_INTER_SANS_MEDIUM)
        .wrapping(iced::widget::text::Wrapping::None);

    let content = row![
        mouse_area(
            container(indicator)
                .width(Length::Fixed(12.0))
                .height(Length::Fixed(FOLDER_SEPARATOR_HEIGHT))
                .align_y(Alignment::Center)
        )
        .on_press(Message::ToggleGroupEnabled(linear_idx, None)),
        Space::new().width(Length::Fixed(5.0)),
        name_text,
        Space::new().width(Length::Shrink),
        stats_text
    ]
    .spacing(0)
    .align_y(Alignment::Center);

    let bg_color = if is_selected {
        COLOR_CONTRAST
    } else {
        Color::TRANSPARENT
    };

    mouse_area(
        container(column![
            container(content)
                .width(Length::Fill)
                .height(Length::Fixed(FOLDER_SEPARATOR_HEIGHT - 11.0))
                .align_y(iced::alignment::Vertical::Center),
            container(Space::new().width(Length::Fill).height(1.0)).style(move |_t: &Theme| {
                let col = if is_selected {
                    COLOR_TEXT_PRIMARY
                } else {
                    COLOR_TEXT_SECONDARY
                };
                container::Style::default().background(col)
            }),
            Space::new().height(10.0)
        ])
        .width(Length::Fill)
        .padding(Padding {
            top: 0.0,
            right: 4.0,
            bottom: 0.0,
            left: 10.0,
        })
        .style(move |_t: &Theme| container::Style {
            background: Some(Background::Color(bg_color)),
            ..container::Style::default()
        }),
    )
    .on_press(Message::TogglePlaylistFolder(linear_idx))
    .on_right_press(Message::RequestContextMenu(get_item_context_menu_entries(
        linear_idx,
    )))
    .interaction(iced::mouse::Interaction::Pointer)
    .into()
}

fn build_song_row<'a>(
    song: &'a PlaylistSongRef,
    linear_idx: usize,
    is_playing: bool,
    is_selected: bool,
    is_group_selected: bool,
) -> Element<'a, Message> {
    let text_color = if is_playing {
        COLOR_TEXT_PRIMARY
    } else {
        COLOR_TEXT_SECONDARY
    };
    let duration_color = if is_playing {
        COLOR_TEXT_PRIMARY
    } else {
        COLOR_TEXT_SECONDARY
    };

    // Indicador •
    let indicator_color = if !song.enabled {
        Color::TRANSPARENT
    } else if is_playing {
        COLOR_TEXT_PRIMARY
    } else {
        COLOR_TEXT_SECONDARY
    };
    let indicator = text("•").size(17).color(indicator_color);

    // Fila 1: "Nº. Nombre de canción .... Duración"
    let track_prefix = if let Some(ref tn) = song.track_number {
        format!("{}.", tn)
    } else {
        format!("{:02}.", linear_idx)
    };

    let prefix_width = 20.0;
    let prefix_spacing = 5.0;

    let prefix_container = container(
        text(track_prefix)
            .size(13)
            .color(text_color)
            .font(FONT_INTER_SANS_MEDIUM),
    )
    .width(Length::Fixed(prefix_width))
    .align_x(Alignment::End);

    let title_text = crate::gui::widgets::smart_truncate_text(
        song.title.to_string(),
        13.0,
        FONT_INTER_SANS_MEDIUM,
        text_color,
    );

    let duration_text = text(format_duration(song.duration))
        .size(13)
        .color(duration_color)
        .font(FONT_INTER_SANS_MEDIUM)
        .wrapping(iced::widget::text::Wrapping::None);

    let row1 = row![prefix_container, title_text, duration_text]
        .spacing(prefix_spacing)
        .align_y(Alignment::Center);

    // Fila 2: "Artista - Álbum :: Año" (Alineada con el Título de la Fila 1)
    let year_str = song.year.as_deref().unwrap_or("-");
    let row2_text = format!(
        "{} - {} :: {}",
        song.artist_name, song.album_title, year_str
    );

    let row2 = row![
        Space::new().width(Length::Fixed(prefix_width + prefix_spacing)),
        crate::gui::widgets::smart_truncate_text(
            row2_text,
            13.0,
            FONT_INTER_SANS_MEDIUM,
            text_color
        )
    ]
    .spacing(0)
    .align_y(Alignment::Center);

    let content = column![row1, row2].spacing(2).padding([0, 0]);

    let bg_color = if is_selected || is_playing {
        COLOR_CONTRAST
    } else if is_group_selected {
        COLOR_CONTRAST
    } else {
        Color::TRANSPARENT
    };

    let song_row_inner = row![
        mouse_area(
            container(indicator)
                .width(Length::Fixed(12.0))
                .height(Length::Fixed(SONG_ROW_HEIGHT))
                .align_y(Alignment::Center)
        )
        .on_press(Message::ToggleSongEnabled(linear_idx, None)),
        Space::new().width(Length::Fixed(0.0)),
        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_y(iced::alignment::Vertical::Center)
    ]
    .spacing(0)
    .align_y(Alignment::Center);

    mouse_area(
        container(song_row_inner)
            .width(Length::Fill)
            .height(Length::Fixed(SONG_ROW_HEIGHT))
            .padding(Padding {
                top: 0.0,
                right: 4.0,
                bottom: 0.0,
                left: 15.0,
            })
            .style(move |_t: &Theme| container::Style {
                background: Some(Background::Color(bg_color)),
                ..container::Style::default()
            }),
    )
    .on_press(Message::PlaySongIndex(linear_idx))
    .on_right_press(Message::RequestContextMenu(get_item_context_menu_entries(
        linear_idx,
    )))
    .interaction(iced::mouse::Interaction::Pointer)
    .into()
}

/// Genera las opciones del menú contextual para canciones y carpetas en la lista de reproducción.
pub fn get_item_context_menu_entries(
    linear_idx: usize,
) -> Vec<crate::gui::widgets::ContextMenuEntry<Message>> {
    use crate::gui::widgets::ContextMenuEntry;
    vec![
        ContextMenuEntry {
            label: "Reproducir".to_string(),
            icon: Some("play-circle-straight-outlined.svg".to_string()),
            action: Some(Message::PlaylistContextMenuPlay(linear_idx)),
        },
        ContextMenuEntry {
            label: "".to_string(),
            icon: None,
            action: None,
        },
        ContextMenuEntry {
            label: "Agregar archivo".to_string(),
            icon: Some("audio-file-outlined-straight.svg".to_string()),
            action: Some(Message::PlaylistAddFiles),
        },
        ContextMenuEntry {
            label: "Agregar carpeta".to_string(),
            icon: Some("folder-add-outlined.svg".to_string()),
            action: Some(Message::PlaylistAddFolder),
        },
        ContextMenuEntry {
            label: "".to_string(),
            icon: None,
            action: None,
        },
        ContextMenuEntry {
            label: "Información".to_string(),
            icon: Some("info-outlined-straight.svg".to_string()),
            action: Some(Message::NoOp),
        },
        ContextMenuEntry {
            label: "Ubicación del archivo".to_string(),
            icon: Some("folder-open-outlined.svg".to_string()),
            action: Some(Message::PlaylistShowFileLocation(linear_idx)),
        },
        ContextMenuEntry {
            label: "Mostrar en la biblioteca".to_string(),
            icon: Some("show-library-outlined-straight.svg".to_string()),
            action: Some(Message::PlaylistShowInLibrary(linear_idx)),
        },
        ContextMenuEntry {
            label: "".to_string(),
            icon: None,
            action: None,
        },
        ContextMenuEntry {
            label: "Enviar a otra lista".to_string(),
            icon: Some("send-straight.svg".to_string()),
            action: Some(Message::PlaylistRequestSubMenu(linear_idx)),
        },
        ContextMenuEntry {
            label: "Editor de Etiquetas".to_string(),
            icon: Some("edit-tag.svg".to_string()),
            action: Some(Message::NoOp),
        },
        ContextMenuEntry {
            label: "Convertidor de audio".to_string(),
            icon: Some("convert.svg".to_string()),
            action: Some(Message::NoOp),
        },
        ContextMenuEntry {
            label: "".to_string(),
            icon: None,
            action: None,
        },
        ContextMenuEntry {
            label: "Activar | Desactivar".to_string(),
            icon: Some("indicator-outlined.svg".to_string()),
            action: Some(Message::PlaylistToggleEnabled(linear_idx)),
        },
        ContextMenuEntry {
            label: "Plegar | Desplegar".to_string(),
            icon: Some("expand-collapse-straight.svg".to_string()),
            action: Some(Message::PlaylistToggleAllFolders),
        },
        ContextMenuEntry {
            label: "Eliminar de la lista".to_string(),
            icon: Some("delete-oulined.svg".to_string()),
            action: Some(Message::PlaylistDeleteSelection(linear_idx)),
        },
    ]
}

/// Genera las opciones del menú contextual para el área vacía de la lista de reproducción.
fn get_empty_playlist_context_menu_entries() -> Vec<crate::gui::widgets::ContextMenuEntry<Message>>
{
    use crate::gui::widgets::ContextMenuEntry;
    vec![
        ContextMenuEntry {
            label: "Agregar archivo".to_string(),
            icon: Some("audio-file-outlined-straight.svg".to_string()),
            action: Some(Message::PlaylistAddFiles),
        },
        ContextMenuEntry {
            label: "Agregar carpeta".to_string(),
            icon: Some("folder-add-outlined.svg".to_string()),
            action: Some(Message::PlaylistAddFolder),
        },
    ]
}

// ============================================================
// Barra inferior
// ============================================================

fn build_bottom_bar<'a>(manager: &'a PlaylistManager) -> Element<'a, Message> {
    let search_box = crate::gui::widgets::standard_search_input(
        Some(PLAYLIST_SEARCH_ID.clone()),
        "Buscar...",
        &manager.search_query,
        Message::PlaylistSearchChanged,
        Message::PlaylistSearchChanged(String::new()),
        Length::Fixed(180.0),
    );

    let eq_btn = icon_button(
        "equalizer-straight.svg",
        false,
        Message::ToggleAudioCenter(Some(1)),
    );
    let shuffle_btn = icon_button(
        "shuffle-straight.svg",
        manager.shuffle_active,
        Message::ToggleShuffle,
    );
    let (repeat_icon, repeat_active) = if manager.repeat_mode == 2 {
        ("repeat-one-straight-outlined.svg", true)
    } else {
        ("repeat-straight-outlined.svg", manager.repeat_mode > 0)
    };
    let repeat_btn = icon_button(repeat_icon, repeat_active, Message::ToggleRepeat);
    let lyrics_icon = icon_button("lyrics-straight-outlined.svg", false, Message::ToggleLyrics);

    let icons_group = row![
        container(lyrics_icon).padding(Padding {
            top: 3.0,
            right: 0.0,
            bottom: 0.0,
            left: 0.0
        }),
        repeat_btn,
        shuffle_btn,
        eq_btn
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    let bar_content = row![
        container(search_box).center_y(Length::Fill),
        Space::new().width(Length::Fill),
        container(icons_group).center_y(Length::Fill)
    ]
    .padding([0, 15])
    .spacing(10)
    .align_y(Alignment::Center);

    container(bar_content)
        .width(Length::Fill)
        .height(Length::Fixed(BOTTOM_BAR_HEIGHT))
        .style(|_t: &Theme| container::Style::default().background(COLOR_CONTRAST))
        .into()
}

fn icon_button<'a>(icon_name: &str, active: bool, msg: Message) -> Element<'a, Message> {
    let icon_color = if active {
        COLOR_ACCENT
    } else {
        COLOR_TEXT_SECONDARY
    };

    let icon = svg(svg::Handle::from_path(format!(
        "assets/icons/{}",
        icon_name
    )))
    .width(24)
    .height(24)
    .style(move |_t: &Theme, _s| svg::Style {
        color: Some(icon_color),
    });

    button(icon)
        .padding(2)
        .style(|_t: &Theme, _s| button::Style::default().with_background(Color::TRANSPARENT))
        .on_press(msg)
        .into()
}

// ============================================================
// Compatibilidad temporal con app.rs existente
// ============================================================

impl PlaylistManager {
    /// Carga y reproduce la canción en `idx` (sin ganancias explícitas).
    fn load_and_play(&self, idx: usize, audio_manager: &AudioManager) {
        if let Some(song) = self.get_song_at_linear_index(idx) {
            let _ = audio_manager.load_file(
                &song.file_path,
                song.title.to_string(),
                song.artist_name.to_string(),
                song.album_title.to_string(),
                song.cover_path.as_deref().map(str::to_string),
                None,
                None,
            );
            audio_manager.play();
        }
    }

    /// Avanza a la siguiente canción (auto-advance EOF).
    ///
    /// La decisión (repeat one, shuffle, secuencial, re-barajado) es la de
    /// `logic::playlist_nav::advance_next`, la misma que usa
    /// `get_next_song_ref` para la pre-carga.
    pub fn play_next(&mut self, audio_manager: &AudioManager) {
        let groups = self.filtered_groups.as_deref().unwrap_or(&self.groups);
        let target = nav::advance_next(
            groups,
            &mut self.playing_song_idx,
            RepeatMode::from_u8(self.repeat_mode),
            &mut self.shuffle_active,
            &mut self.shuffle_session,
            &mut fastrand::Rng::new(),
        );
        if let Some(idx) = target {
            self.load_and_play(idx, audio_manager);
        }
    }

    /// Retrocede a la canción anterior.
    pub fn play_prev(&mut self, audio_manager: &AudioManager) {
        let groups = self.filtered_groups.as_deref().unwrap_or(&self.groups);
        let target = nav::step_prev(
            groups,
            self.playing_song_idx,
            RepeatMode::from_u8(self.repeat_mode),
            self.shuffle_active,
            self.shuffle_session.as_mut(),
        );
        if let Some(idx) = target {
            self.playing_song_idx = Some(idx);
            self.load_and_play(idx, audio_manager);
        }
    }

    /// Notifica que una canción fue reproducida manualmente (por doble clic o enter).
    /// Esto es vital para el modo Shuffle para evitar repeticiones y mantener el historial.
    pub fn notify_manual_play(&mut self, song_id: i64) {
        if self.shuffle_active
            && let Some(session) = &mut self.shuffle_session
        {
            nav::record_manual_play(session, song_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::database::{PlaylistFolderGroup, PlaylistSongRef};

    fn make_song(song_id: i64, path: &str) -> PlaylistSongRef {
        PlaylistSongRef {
            item_id: song_id,
            song_id,
            sequence_order: song_id as f64,
            enabled: true,
            title: format!("Título {}", song_id).into(),
            artist_name: "Artista".into(),
            album_title: "Álbum".into(),
            album_artist_name: "Artista".into(),
            year: None,
            genre: None,
            track_number: None,
            duration: 180.0,
            file_path: path.into(),
            folder_path: "/".into(),
            folder_name: "Raíz".into(),
            cover_path: None,
            cover_override: None,
        }
    }

    fn manager_with_songs(songs: Vec<PlaylistSongRef>) -> PlaylistManager {
        let mut pm = PlaylistManager::default();
        pm.set_groups(vec![PlaylistFolderGroup {
            folder_path: "/".into(),
            folder_name: "Raíz".into(),
            songs,
            total_duration: 0.0,
            first_item_id: 1,
        }]);
        pm
    }

    #[test]
    fn test_get_next_song_ref_sequential_skips_disabled() {
        let mut songs = vec![make_song(1, "/a.flac"), make_song(2, "/b.flac")];
        songs[0].enabled = false; // La actual está deshabilitada
        let pm = manager_with_songs(songs);
        // playing_song_idx apunta a la canción 1 (índice lineal 1: separador + 0)
        let pm2 = PlaylistManager {
            playing_song_idx: Some(1),
            ..pm
        };
        let next = pm2.get_next_song_ref();
        assert!(next.is_some());
        assert_eq!(next.unwrap().song_id, 2, "Debe saltar la deshabilitada");
    }

    #[test]
    fn test_get_next_song_ref_shuffle_skips_disabled() {
        // Orden shuffle: [1 (deshabilitada), 3, 2]
        let mut songs = vec![
            make_song(1, "/a.flac"),
            make_song(2, "/b.flac"),
            make_song(3, "/c.flac"),
        ];
        songs[0].enabled = false;
        let mut pm = manager_with_songs(songs);
        pm.shuffle_active = true;
        pm.shuffle_session = Some(ShuffleSession {
            session_id: "s1".into(),
            shuffle_order: vec![1, 3, 2],
            current_position: 0,
            history: vec![4], // canción previa
        });
        pm.playing_song_idx = Some(1); // La canción 1 (índice lineal 1)
        let next = pm.get_next_song_ref();
        assert!(next.is_some());
        assert_eq!(
            next.unwrap().song_id,
            3,
            "En shuffle debe saltar la deshabilitada y tomar la siguiente del orden"
        );
    }

    #[test]
    fn test_get_next_song_ref_shuffle_end_returns_none() {
        let songs = vec![make_song(1, "/a.flac")];
        let mut pm = manager_with_songs(songs);
        pm.shuffle_active = true;
        pm.shuffle_session = Some(ShuffleSession {
            session_id: "s1".into(),
            shuffle_order: vec![1],
            current_position: 1, // Ya al final del orden
            history: vec![1],
        });
        pm.playing_song_idx = Some(1);
        assert!(pm.get_next_song_ref().is_none());
    }
}
