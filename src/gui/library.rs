use crate::db::Database;
use crate::gui::app::Message;
use crate::gui::theme::*;
use iced::{
    Alignment, Color, Element, Length, Task, Theme,
    widget::{Space, button, column, container, mouse_area, row, text},
};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
// Import deleted since song row is injected and artist header is in universal_song_list
use crate::gui::widgets::{VuMeterWidget, standard_scrollable, standard_scrollbar};
use crate::logic::library_geometry::{self, ContentRect};
use crate::logic::library_groups;
use crate::logic::library_selection::{self, LibraryData, LibrarySelection};
use crate::logic::library_sort;
use crate::logic::selection_modifiers::SelectModifiers;
use crate::utils::{SortColumn, format_duration, format_size};

// Contadores de redraw para diagnóstico — se incrementan cada vez que la vista se reconstruye
static LIBRARY_VIEW_REDRAWS: AtomicU64 = AtomicU64::new(0);
pub fn get_library_redraw_count() -> u64 {
    LIBRARY_VIEW_REDRAWS.load(Ordering::Relaxed)
}
pub fn library_redraw_count_log() {
    let count = LIBRARY_VIEW_REDRAWS.fetch_add(1, Ordering::Relaxed) + 1;
    if count % 100 == 0 {
        tracing::info!("Library view redraws: {}", count);
    }
}

/// ID estático para el scrollable de la biblioteca — garantiza que view y update usan EXACTAMENTE el mismo ID
pub static LIBRARY_SCROLL_ID: std::sync::LazyLock<iced::widget::Id> =
    std::sync::LazyLock::new(|| iced::widget::Id::unique());
pub static GRID_ID_A: std::sync::LazyLock<iced::widget::Id> =
    std::sync::LazyLock::new(|| iced::widget::Id::new("grid_view_a"));
pub static GRID_ID_B: std::sync::LazyLock<iced::widget::Id> =
    std::sync::LazyLock::new(|| iced::widget::Id::new("grid_view_b"));
pub static LIBRARY_SEARCH_ID: std::sync::LazyLock<iced::widget::Id> =
    std::sync::LazyLock::new(|| iced::widget::Id::new("library_search_input"));

pub use crate::logic::library_geometry::{
    LibraryAlbumGroup, LibraryViewMode, LibraryVirtualArtist, LibraryVirtualRow,
};
pub use crate::logic::library_groups::{AlbumEntry, ArtistGroup, LibraryStats};
pub use crate::logic::library_selection::{LibraryListItem, LibraryNavDir};

/// Convierte los modificadores de Iced en los de la lógica de selección:
/// Ctrl/Cmd alterna, Shift selecciona por rango.
pub fn select_modifiers(modifiers: iced::keyboard::Modifiers) -> SelectModifiers {
    SelectModifiers {
        toggle: modifiers.command() || modifiers.control(),
        range: modifiers.shift(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LibrarySource {
    Local,
    Spotify,
    YouTube,
    Qobuz,
    Deezer,
    Tidal,
}

/// Datos de una fila calculada en la cuadrícula
#[derive(Debug, Clone)]
pub struct LibraryGridRowData {
    pub top_y: f32,
    pub height: f32,
    pub albums: Vec<AlbumEntry>,
    pub expanded_id: Option<String>,
}

/// Elementos visibles en la cuadrícula virtualizada
#[derive(Debug, Clone)]
pub enum LibraryGridElement {
    AlbumRow {
        albums: Vec<AlbumEntry>,
        active_expansion_id: Option<String>,
    },
    ExpandedSongs {
        album_id: String,
        songs: Vec<(std::sync::Arc<crate::db::database::SongData>, usize)>,
        top_space: f32,
        bottom_space: f32,
    },
}

pub struct LibraryManager {
    pub view_mode: LibraryViewMode,
    pub source: LibrarySource,
    pub sort_column: Option<SortColumn>,
    pub sort_ascending: Option<bool>,
    pub search_query: String,
    pub search_nonce: u32,
    pub expanded_album: Option<String>,
    pub expanded_album_songs: Option<Vec<std::sync::Arc<crate::db::database::SongData>>>,
    pub cached_albums: Option<Vec<AlbumEntry>>,
    pub filtered_albums: Option<Vec<AlbumEntry>>,
    pub cached_all_songs: Option<Vec<std::sync::Arc<crate::db::database::SongData>>>,
    pub filtered_songs: Option<Vec<std::sync::Arc<crate::db::database::SongData>>>,
    pub cached_folders: Option<Vec<(i64, String, String)>>,
    pub mount_points: Vec<(String, String)>,
    pub filter_artist: Option<String>,
    pub filter_album: Option<String>,
    pub filter_genre: Option<String>,
    pub filter_year: Option<String>,
    pub filter_folder_id: Option<i64>,
    pub artist_groups: Vec<ArtistGroup>,
    pub collapsed_artists: std::collections::HashSet<String>,
    pub collapsed_albums: std::collections::HashSet<String>,
    pub view_menu_open: bool,
    pub add_menu_open: bool,
    pub total_songs: usize,
    pub total_albums: usize,
    pub total_artists: usize,
    pub total_duration_secs: f64,
    pub total_size_bytes: f64,

    pub column_widths: std::collections::HashMap<SortColumn, u16>,
    pub resizing_column: Option<SortColumn>,
    pub resizing_start_x: f32,
    pub resizing_start_w: u16,
    pub hovered_column: Option<SortColumn>,

    // Selection & keyboard navigation
    pub selected_items: std::collections::HashSet<LibraryListItem>,
    pub focused_item: Option<LibraryListItem>,
    pub selection_pivot: Option<LibraryListItem>,
    pub selected_header: Option<String>,
    pub selected_album: Option<String>,
    pub selected_song_idx: Option<usize>,
    pub albums_per_row: std::cell::Cell<usize>,
    pub library_area_width: f32,
    pub last_selected_album: Option<String>,
    pub last_selected_song_idx: Option<usize>,
    pub selection_stats: Option<LibraryStats>,
    pub filter_stats: Option<LibraryStats>,
    pub artist_last_selection: std::collections::HashMap<String, usize>,
    pub selected_item_hint: Option<(f32, f32)>,
    pub last_viewport: Option<iced::Rectangle>,
    pub scroll_offset: iced::Vector,
    pub marquee_start: Option<iced::Point>,
    pub marquee_end: Option<iced::Point>,
    pub marquee_start_pos: Option<iced::Point>, // Posición inicial en pantalla para el umbral
    pub is_dragging: bool,

    // Caché de virtualización (Interior Mutability para permitir actualización en view)
    pub cached_detailed_view: std::cell::RefCell<Option<(f32, Vec<LibraryVirtualArtist>)>>,
    pub cached_visible_elements: std::cell::RefCell<
        Option<(
            f32,
            f32,
            f32,
            Vec<LibraryVirtualRow>,
            Option<(String, bool, usize, usize, f64)>,
        )>,
    >, // (top_space, bottom_space, total_h, rows, sticky_info)
    pub data_unloaded: bool,
    pub pending_reveal_path: Option<String>,
}

impl Default for LibraryManager {
    fn default() -> Self {
        let mut column_widths = std::collections::HashMap::new();
        column_widths.insert(SortColumn::TrackNumber, 47);
        column_widths.insert(SortColumn::Title, 275);
        column_widths.insert(SortColumn::Artist, 155);
        column_widths.insert(SortColumn::AlbumArtist, 135);
        column_widths.insert(SortColumn::Album, 165);
        column_widths.insert(SortColumn::Genre, 135);
        column_widths.insert(SortColumn::Year, 50);
        column_widths.insert(SortColumn::Duration, 53);
        column_widths.insert(SortColumn::Format, 53);
        column_widths.insert(SortColumn::SampleRate, 80);
        column_widths.insert(SortColumn::Channels, 43);
        column_widths.insert(SortColumn::BitDepth, 60);
        column_widths.insert(SortColumn::Bitrate, 90);
        column_widths.insert(SortColumn::Size, 85);
        column_widths.insert(SortColumn::AlbumCard, 250);
        column_widths.insert(SortColumn::AlbumThumbnail, 40);

        Self {
            view_mode: LibraryViewMode::Grid,
            source: LibrarySource::Local,
            sort_column: None,
            sort_ascending: Some(true),
            search_query: String::new(),
            search_nonce: 0,
            expanded_album: None,
            expanded_album_songs: None,
            cached_albums: None,
            filtered_albums: None,
            cached_all_songs: None,
            filtered_songs: None,
            cached_folders: None,
            mount_points: Vec::new(),
            filter_artist: None,
            filter_album: None,
            filter_genre: None,
            filter_year: None,
            filter_folder_id: None,
            artist_groups: Vec::new(),
            collapsed_artists: std::collections::HashSet::new(),
            collapsed_albums: std::collections::HashSet::new(),
            view_menu_open: false,
            add_menu_open: false,
            total_songs: 0,
            total_albums: 0,
            total_artists: 0,
            total_duration_secs: 0.0,
            total_size_bytes: 0.0,
            column_widths,
            resizing_column: None,
            resizing_start_x: 0.0,
            resizing_start_w: 0,
            hovered_column: None,
            selected_items: std::collections::HashSet::new(),
            focused_item: None,
            selection_pivot: None,
            selected_header: None,
            selected_album: None,
            selected_song_idx: None,
            albums_per_row: std::cell::Cell::new(5),
            library_area_width: 800.0,
            last_selected_album: None,
            last_selected_song_idx: None,
            last_viewport: None,
            selection_stats: None,
            filter_stats: None,
            artist_last_selection: std::collections::HashMap::new(),
            selected_item_hint: None,
            marquee_start: None,
            marquee_end: None,
            marquee_start_pos: None,
            is_dragging: false,
            scroll_offset: iced::Vector::new(0.0, 0.0),
            cached_detailed_view: std::cell::RefCell::new(None),
            cached_visible_elements: std::cell::RefCell::new(None),
            data_unloaded: false,
            pending_reveal_path: None,
        }
    }
}

pub const UNLOAD_SAFETY_LIMIT: usize = 60;

impl LibraryManager {
    /// Descarga los datos pesados de la RAM por inactividad, conservando un margen de seguridad
    /// Descarga los datos pesados de la RAM si no se están usando
    pub fn unload(&mut self, is_focused: bool) {
        if is_focused {
            // Si está enfocada, mantenemos los datos base (all_songs, albums) para que no desaparezca nada.
            // Solo invalidamos la caché de virtualización de la vista para liberar algo de RAM (se regenera al instante al mover el mouse/scroll)
            self.invalidate_cache();
            return;
        }

        // Condición inteligente: No descargar si la biblioteca es pequeña (menos de 250 álbumes y 2000 canciones)
        // Esto mantiene el reproductor "snappy" para usuarios con colecciones medianas/pequeñas.
        let song_count = self.cached_all_songs.as_ref().map(|s| s.len()).unwrap_or(0);
        let album_count = self.cached_albums.as_ref().map(|a| a.len()).unwrap_or(0);

        // Condición de protección: No descargar nada si la biblioteca es pequeña
        // (menos de 150 álbumes o menos de 700 canciones)
        if album_count < 150 || song_count < 700 {
            return;
        }

        // Si la biblioteca es grande y no está enfocada, realizamos una DESCARGA PARCIAL
        // En lugar de poner None, truncamos a un margen de seguridad (100 álbumes / 500 canciones)
        // Esto evita que la interfaz se vea vacía si el usuario vuelve rápido, pero libera la mayoría de la RAM.
        let mut cleared = false;

        if let Some(albums) = &mut self.cached_albums {
            if albums.len() > 100 {
                tracing::debug!("Audoxidy GC: Cleaning inactive albums from the library");
                albums.truncate(100);
                self.filtered_albums = None;
                cleared = true;
            }
        }

        if let Some(songs) = &mut self.cached_all_songs {
            if songs.len() > 500 {
                tracing::debug!("Audoxidy GC: Cleaning inactive songs from the library");
                songs.truncate(500);
                self.filtered_songs = None;
                cleared = true;
            }
        }

        if cleared {
            self.invalidate_cache();
            self.data_unloaded = true;
        }
    }

    pub fn invalidate_cache(&mut self) {
        self.cached_detailed_view.replace(None);
        self.cached_visible_elements.replace(None);
        // Al invalidar caché por cambio de estructura, el hint de scroll ya no es válido
        self.selected_item_hint = None;
    }

    pub fn invalidate_visible_cache(&mut self) {
        self.cached_visible_elements.replace(None);
    }

    /// Genera la estructura jerárquica para la vista de biblioteca
    /// Este proceso es O(N) y solo se ejecuta cuando cambian los filtros o expansiones.
    pub fn get_view_structure(&self) -> (f32, Vec<LibraryVirtualArtist>) {
        if let Some((h, ref v)) = *self.cached_detailed_view.borrow() {
            return (h, v.clone());
        }

        let (total_content_h, v_artists) = library_geometry::build_view_structure(
            &self.artist_groups,
            self.cached_albums.as_deref(),
            &self.collapsed_artists,
            &self.collapsed_albums,
            self.view_mode,
        );

        self.cached_detailed_view
            .replace(Some((total_content_h, v_artists.clone())));
        (total_content_h, v_artists)
    }

    /// Busca el ID compuesto de un álbum (Artist|AlbumID) para una canción dada,
    /// garantizando consistencia absoluta con la estructura visual actual.
    pub fn find_album_id_for_song(&self, song_id: i64) -> Option<String> {
        let (_, v_artists) = self.get_view_structure();
        library_geometry::album_id_for_song(&v_artists, song_id)
    }

    /// Cambia el estado de expansión de un álbum y actualiza el foco/selección.
    pub fn set_album_collapsed(&mut self, album_id: String, collapsed: bool) {
        if collapsed {
            self.collapsed_albums.insert(album_id.clone());
        } else {
            self.collapsed_albums.remove(&album_id);
        }

        // Sincronizar foco y selección
        let item = LibraryListItem::Album(album_id.clone());
        self.selected_items.clear();
        self.selected_items.insert(item.clone());
        self.focused_item = Some(item);

        self.selected_song_idx = None;
        self.selected_header = None;
        self.selected_album = Some(album_id);
        self.invalidate_cache();
    }

    /// Cambia el estado de expansión de un artista y actualiza el foco/selección.
    pub fn set_artist_collapsed(&mut self, artist_name: String, collapsed: bool) {
        if collapsed {
            self.collapsed_artists.insert(artist_name.clone());
        } else {
            self.collapsed_artists.remove(&artist_name);
        }

        // Sincronizar foco y selección
        let item = LibraryListItem::Artist(artist_name.clone());
        self.selected_items.clear();
        self.selected_items.insert(item.clone());
        self.focused_item = Some(item);

        self.selected_song_idx = None;
        self.selected_album = None;
        self.selected_header = Some(artist_name);
        self.invalidate_cache();
    }

    /// Obtiene los elementos visibles según el viewport actual (Virtualización)
    #[tracing::instrument(skip(self))]
    pub fn get_visible_elements(
        &self,
    ) -> (
        f32,
        f32,
        f32,
        Vec<LibraryVirtualRow>,
        Option<(String, bool, usize, usize, f64)>,
    ) {
        if let Some((ts, bs, th, ref rows, ref sticky)) = *self.cached_visible_elements.borrow() {
            return (ts, bs, th, rows.clone(), sticky.clone());
        }

        let (total_content_h, v_artists) = self.get_view_structure();
        let viewport_h = self
            .last_viewport
            .as_ref()
            .map(|v| v.height)
            .unwrap_or(800.0);

        // D-04: Margen dinámico = 50% del viewport (25% en low-resource)
        let margin = library_geometry::render_margin(viewport_h, crate::utils::is_low_resource());
        let visible = library_geometry::visible_rows(
            total_content_h,
            &v_artists,
            &self.collapsed_albums,
            self.view_mode,
            self.scroll_offset.y,
            viewport_h,
            margin,
        );
        let sticky_artist_info = visible.sticky.map(|s| {
            (
                s.name,
                s.is_collapsed,
                s.albums_count,
                s.songs_count,
                s.duration_secs,
            )
        });

        self.cached_visible_elements.replace(Some((
            visible.top_space,
            visible.bottom_space,
            visible.total_height,
            visible.rows.clone(),
            sticky_artist_info.clone(),
        )));
        (
            visible.top_space,
            visible.bottom_space,
            visible.total_height,
            visible.rows,
            sticky_artist_info,
        )
    }

    /// Calcula los elementos visibles en la cuadrícula para el modo Grid, con virtualización avanzada.
    #[tracing::instrument(skip(self))]
    pub fn get_visible_grid_elements(
        &self,
        columns: usize,
        viewport_y: f32,
        viewport_h: f32,
    ) -> (f32, f32, Vec<LibraryGridElement>) {
        let albums_to_show = self
            .filtered_albums
            .as_ref()
            .or(self.cached_albums.as_ref());

        let albums = if let Some(a) = albums_to_show {
            a
        } else {
            return (0.0, 0.0, Vec::new());
        };

        let mut total_height = 0.0;
        let mut rows_data = Vec::with_capacity(albums.len() / columns + 1);

        // Optimización: Desglosar el ID expandido para evitar format! en el bucle
        let expanded_parts = self
            .expanded_album
            .as_deref()
            .and_then(library_groups::parse_album_composite_id);
        let (exp_art, exp_alb) = match &expanded_parts {
            Some((artist, album)) => (Some(artist.as_str()), Some(album.as_str())),
            None => (None, None),
        };

        // 1. Calcular estructura completa (Altura de cada fila)
        for chunk in albums.chunks(columns) {
            let mut row_h = 252.0;
            let mut expanded_id = None;

            for album_entry in chunk {
                if exp_art == Some(album_entry.artist.as_str())
                    && exp_alb == Some(album_entry.id.as_str())
                {
                    let composite_id = self.expanded_album.clone().unwrap();
                    expanded_id = Some(composite_id);
                    let songs_count = if let Some(songs) = self.expanded_album_songs.as_ref() {
                        let query = self.search_query.trim();
                        if query.is_empty() {
                            songs.len()
                        } else {
                            songs
                                .iter()
                                .filter(|s| crate::utils::song_matches_search(s, query))
                                .count()
                        }
                    } else {
                        1 // "Cargando..."
                    };
                    // 60px de padding/headers + canciones (32px cada una)
                    row_h += 60.0 + (songs_count.max(1) as f32 * 32.0);
                    break;
                }
            }
            rows_data.push(LibraryGridRowData {
                top_y: total_height,
                height: row_h,
                albums: chunk.to_vec(),
                expanded_id: expanded_id,
            });
            total_height += row_h;
        }

        // D-04: Margen dinámico en grid = 50% del viewport (25% en low-resource)
        let grid_margin_ratio: f32 = if crate::utils::is_low_resource() {
            0.25
        } else {
            0.5
        };
        let lazy_margin = (viewport_h * grid_margin_ratio).max(100.0);
        let render_min = viewport_y - lazy_margin;
        let render_max = viewport_y + viewport_h + lazy_margin;

        let mut top_space = 0.0;
        let mut bottom_space = 0.0;
        let mut visible_elements = Vec::new();

        for row in rows_data {
            let row_end = row.top_y + row.height;
            if row_end < render_min {
                top_space += row.height;
            } else if row.top_y > render_max {
                bottom_space += row.height;
            } else {
                // Fila de álbumes
                visible_elements.push(LibraryGridElement::AlbumRow {
                    albums: row.albums,
                    active_expansion_id: row.expanded_id.clone(),
                });

                // Si esta fila tiene un álbum expandido, virtualizar las canciones dentro del bloque
                if let Some(exp_id) = row.expanded_id {
                    if let Some(songs) = self.expanded_album_songs.as_ref() {
                        let query = self.search_query.trim();

                        // El bloque de canciones empieza después de las tarjetas (252px) + header (32px)
                        let songs_block_start_y = row.top_y + 252.0 + 32.0;

                        let mut v_songs = Vec::new();
                        let mut songs_top_space = 0.0;
                        let mut songs_bottom_space = 0.0;

                        let mut current_song_y = songs_block_start_y;
                        for (idx, song) in songs.iter().enumerate() {
                            if !query.is_empty() && !crate::utils::song_matches_search(song, query)
                            {
                                continue;
                            }

                            if current_song_y + 32.0 >= render_min && current_song_y <= render_max {
                                v_songs.push((song.clone(), idx));
                            } else if current_song_y + 32.0 < render_min {
                                songs_top_space += 32.0;
                            } else {
                                songs_bottom_space += 32.0;
                            }
                            current_song_y += 32.0;
                        }

                        if !v_songs.is_empty() || songs_top_space > 0.0 || songs_bottom_space > 0.0
                        {
                            visible_elements.push(LibraryGridElement::ExpandedSongs {
                                album_id: exp_id,
                                songs: v_songs,
                                top_space: songs_top_space,
                                bottom_space: songs_bottom_space,
                            });
                        }
                    }
                }
            }
        }

        (top_space, bottom_space, visible_elements)
    }

    /// Obtiene el rango Y (y, height) de un ítem en la biblioteca virtualizada
    pub fn get_item_y_range(&self, item: &LibraryListItem) -> Option<(f32, f32)> {
        let (_total_h, v_artists) = self.get_view_structure();
        library_geometry::item_y_range(item, &v_artists, &self.collapsed_albums, self.view_mode)
    }

    pub fn sort_songs_static(
        songs: &mut [std::sync::Arc<crate::db::database::SongData>],
        sort_column: Option<SortColumn>,
        sort_ascending: Option<bool>,
    ) {
        library_sort::sort_songs(songs, sort_column, sort_ascending);
    }

    pub fn sort_songs(&self, songs: &mut [std::sync::Arc<crate::db::database::SongData>]) {
        Self::sort_songs_static(songs, self.sort_column, self.sort_ascending);
    }

    pub fn sort_albums_static(
        albums: &mut [AlbumEntry],
        sort_column: Option<SortColumn>,
        sort_ascending: Option<bool>,
    ) {
        library_sort::sort_albums(albums, sort_column, sort_ascending);
    }

    pub fn sort_albums(&self, albums: &mut [AlbumEntry]) {
        Self::sort_albums_static(albums, self.sort_column, self.sort_ascending);
    }

    /// Ordena todos los datos cargados en memoria basándose en el estado actual de ordenamiento.
    pub fn sort_all_data(&mut self) {
        let sc = self.sort_column;
        let sa = self.sort_ascending;

        if let Some(songs) = self.cached_all_songs.as_mut() {
            Self::sort_songs_static(songs, sc, sa);
        }
        if let Some(albums) = self.cached_albums.as_mut() {
            Self::sort_albums_static(albums, sc, sa);
        }
    }

    /// Actualiza los datos maestros de la biblioteca, re-ordenando y aplicando filtros.
    pub fn update_data(
        &mut self,
        songs: Vec<std::sync::Arc<crate::db::database::SongData>>,
        albums: Vec<AlbumEntry>,
    ) {
        self.cached_all_songs = Some(songs.clone());
        self.cached_albums = Some(albums);
        self.update_processed_data(songs);
    }

    /// Maneja la navegación por teclado unificada entre cabeceras y canciones.
    /// Devuelve (NuevaCabecera, NuevoIndiceCanción, FocoEnCanción).
    pub fn get_row_height(&self) -> f32 {
        library_geometry::ListMetrics::for_mode(self.view_mode).row_h
    }

    pub fn get_header_height(&self) -> f32 {
        library_geometry::ListMetrics::for_mode(self.view_mode).artist_header_h
    }

    pub fn is_list_mode(&self) -> bool {
        self.view_mode.is_list()
    }

    pub fn get_song_by_global_idx(
        &self,
        target_idx: usize,
    ) -> Option<std::sync::Arc<crate::db::database::SongData>> {
        let mut current_idx = 0;
        for group in &self.artist_groups {
            if current_idx + group.songs.len() > target_idx {
                return group.songs.get(target_idx - current_idx).cloned();
            }
            current_idx += group.songs.len();
        }
        None
    }

    /// Ítems visibles en orden de pantalla con su rectángulo en espacio de contenido.
    pub fn get_visible_items(&self) -> Vec<(LibraryListItem, ContentRect)> {
        let lib_width = self.library_area_width.max(100.0);

        if self.is_list_mode() {
            let (_total_h, v_artists) = self.get_view_structure();
            return library_geometry::list_item_rects(
                &v_artists,
                &self.collapsed_albums,
                self.view_mode,
                lib_width,
            );
        }

        // Modo Grid: Los elementos son álbumes y opcionalmente canciones si está expandido
        let mut items = Vec::new();
        let mut current_y = 0.0;
        let albums_to_show = self
            .filtered_albums
            .as_ref()
            .or(self.cached_albums.as_ref());

        if let Some(albums) = albums_to_show {
            let per_row = self.albums_per_row.get().max(1);
            let card_inner_width = 182.0;
            let spacing = 5.0;
            let x_start = 5.0; // El padding left: 5.0 definido en library.rs para content_with_deselection

            for chunk in albums.chunks(per_row) {
                let mut row_h = 252.0;
                let mut expanded_id = None;

                for (col, album_entry) in chunk.iter().enumerate() {
                    let composite_id =
                        library_groups::album_composite_id(&album_entry.artist, &album_entry.id);
                    let x = x_start + col as f32 * (card_inner_width + spacing);
                    // Usamos el ancho exacto de la tarjeta (182) para una selección precisa
                    items.push((
                        LibraryListItem::Album(composite_id.clone()),
                        ContentRect::new(x, current_y, card_inner_width, 252.0),
                    ));

                    if self.expanded_album.as_deref() == Some(composite_id.as_str()) {
                        expanded_id = Some(composite_id);
                    }
                }

                if let Some(_exp_id) = expanded_id {
                    if let Some(songs) = self.expanded_album_songs.as_ref() {
                        let query = self.search_query.trim();
                        let filtered = if query.is_empty() {
                            songs.clone()
                        } else {
                            songs
                                .iter()
                                .filter(|s| crate::utils::song_matches_search(s, query))
                                .cloned()
                                .collect()
                        };

                        let mut song_y = current_y + 252.0 + 32.0; // aprox padding
                        for song in filtered {
                            items.push((
                                LibraryListItem::Song(song.id),
                                ContentRect::new(0.0, song_y, lib_width, 32.0),
                            ));
                            song_y += 32.0;
                        }
                        row_h += 60.0 + (songs.len() as f32 * 32.0); // Ajuste de altura de fila para navegación
                    }
                }
                current_y += row_h;
            }
        }
        items
    }

    /// Obtiene todos los items actualmente visibles en la vista (respetando colapsos y filtros) sin calcular rectángulos.
    pub fn get_all_list_items(&self) -> Vec<LibraryListItem> {
        if self.is_list_mode() {
            let (_total_h, v_artists) = self.get_view_structure();
            return library_geometry::list_items(&v_artists, &self.collapsed_albums);
        }
        let mut items = Vec::new();
        let albums_to_show = self
            .filtered_albums
            .as_ref()
            .or(self.cached_albums.as_ref());
        if let Some(albums) = albums_to_show {
            for album_entry in albums {
                let composite_id =
                    library_groups::album_composite_id(&album_entry.artist, &album_entry.id);
                items.push(LibraryListItem::Album(composite_id.clone()));
                if self.expanded_album.as_deref() == Some(composite_id.as_str()) {
                    if let Some(songs) = &self.expanded_album_songs {
                        for song in songs {
                            items.push(LibraryListItem::Song(song.id));
                        }
                    }
                }
            }
        }
        items
    }

    /// Returns the new navigation item and its exact Y position from the items list.
    pub fn handle_key_nav(
        &mut self,
        dir: LibraryNavDir,
        modifiers: iced::keyboard::Modifiers,
    ) -> Option<(LibraryListItem, f32, f32)> {
        let items = self.get_visible_items();
        if items.is_empty() {
            return None;
        }

        // 1. Foco explícito; 2. fallback a campos legados; 3. primer ítem.
        let current_idx = library_selection::current_index(
            &items,
            self.focused_item.as_ref(),
            |item| match item {
                LibraryListItem::Song(id) => {
                    // Si tenemos un list mode con filtered_songs, buscamos el índice
                    if let Some(sel_idx) = self.selected_song_idx {
                        if let Some(songs) = self.filtered_songs.as_ref() {
                            songs.get(sel_idx).map(|s| s.id == *id).unwrap_or(false)
                        } else if let Some(songs) = self.expanded_album_songs.as_ref() {
                            songs.get(sel_idx).map(|s| s.id == *id).unwrap_or(false)
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                }
                LibraryListItem::Album(id) => Some(id) == self.selected_album.as_ref(),
                LibraryListItem::Artist(name) => Some(name) == self.selected_header.as_ref(),
            },
        );

        // Navegación en Grid vs Lista
        let next_idx = if self.view_mode == LibraryViewMode::Grid {
            let per_row = self.albums_per_row.get().max(1);
            let is_current_song =
                matches!(items.get(current_idx), Some((LibraryListItem::Song(_), _)));

            match dir {
                LibraryNavDir::Up => {
                    if is_current_song {
                        let prev = current_idx.saturating_sub(1);
                        if let Some((LibraryListItem::Song(_), _)) = items.get(prev) {
                            prev
                        } else {
                            // Al subir desde la primera canción, ir al álbum que está expandido
                            if let Some(exp_id) = &self.expanded_album {
                                items
                                    .iter()
                                    .position(|(it, _)| {
                                        if let LibraryListItem::Album(id) = it {
                                            id == exp_id
                                        } else {
                                            false
                                        }
                                    })
                                    .unwrap_or(prev)
                            } else {
                                prev
                            }
                        }
                    } else {
                        current_idx.saturating_sub(per_row)
                    }
                }
                LibraryNavDir::Down => {
                    if is_current_song {
                        let next = (current_idx + 1).min(items.len() - 1);
                        if let Some((LibraryListItem::Song(_), _)) = items.get(next) {
                            next
                        } else {
                            current_idx // Se DETIENE al final de la lista de canciones
                        }
                    } else {
                        let mut target = (current_idx + per_row).min(items.len() - 1);

                        // Si hay una lista de canciones expandida justo después de esta fila,
                        // entrar en ella por la primera canción.
                        for i in (current_idx + 1)..=target {
                            if let Some((LibraryListItem::Song(_), _)) = items.get(i) {
                                target = i;
                                break;
                            }
                        }
                        target
                    }
                }
                LibraryNavDir::Left => {
                    if is_current_song {
                        current_idx
                    } else {
                        current_idx.saturating_sub(1)
                    }
                }
                LibraryNavDir::Right => {
                    if is_current_song {
                        current_idx
                    } else {
                        (current_idx + 1).min(items.len() - 1)
                    }
                }
                _ => current_idx,
            }
        } else {
            library_selection::list_nav_target(current_idx, items.len(), dir)
        };

        let target = &items[next_idx.min(items.len() - 1)].1;
        let (y, h) = (target.y, target.height);

        let mut selection = self.take_selection();
        let new_item = selection.navigate(
            &items,
            current_idx,
            next_idx,
            select_modifiers(modifiers),
            &self.library_data(),
        );
        self.restore_selection(selection);
        let new_item = new_item?;

        self.selected_item_hint = Some((y, h));

        // Sincronizar legacy fields para compatibilidad con el resto de la app
        self.sync_legacy_selection(&new_item, next_idx);
        self.update_selection_stats();

        Some((new_item, y, h))
    }

    pub fn select_marquee(
        &mut self,
        marquee_rect: ContentRect,
        modifiers: iced::keyboard::Modifiers,
    ) {
        let items = self.get_visible_items();
        let mut selection = self.take_selection();
        selection.select_marquee(&items, marquee_rect, select_modifiers(modifiers));
        self.restore_selection(selection);
        self.update_selection_stats();
    }

    /// Maneja la selección por clic, soportando modificadores para selección múltiple o por rango.
    pub fn handle_click(&mut self, item: LibraryListItem, modifiers: iced::keyboard::Modifiers) {
        let mut selection = self.take_selection();
        selection.click(
            item,
            select_modifiers(modifiers),
            || self.get_visible_items(),
            &self.library_data(),
        );
        self.restore_selection(selection);
        self.update_selection_stats();
    }

    /// Selecciona todos los elementos visibles (incluyendo cabeceras colapsadas) según los filtros actuales.
    pub fn select_all(&mut self) {
        let items = self.get_all_list_items();
        let mut selection = self.take_selection();
        selection.select_all(items);
        self.restore_selection(selection);
        self.update_selection_stats();
    }

    pub fn select_range(&mut self, start_item: &LibraryListItem, end_item: &LibraryListItem) {
        let items = self.get_visible_items();
        let mut selection = self.take_selection();
        selection.select_range(&items, start_item, end_item, &self.library_data());
        self.restore_selection(selection);
    }

    pub fn get_songs_for_album_id(
        &self,
        composite_id: &str,
    ) -> Option<Vec<std::sync::Arc<crate::db::database::SongData>>> {
        // Priorizar canciones FILTRADAS (búsqueda activa) para que las estadísticas y selección sean coherentes
        let songs = self
            .filtered_songs
            .as_ref()
            .or(self.cached_all_songs.as_ref())?;
        let albums = self.cached_albums.as_ref()?;
        library_groups::songs_for_album_id(composite_id, songs, albums)
    }

    fn sync_legacy_selection(&mut self, item: &LibraryListItem, _current_idx_in_visible: usize) {
        match item {
            LibraryListItem::Artist(name) => {
                self.selected_header = Some(name.clone());
                self.selected_album = None;
                self.selected_song_idx = None;
            }
            LibraryListItem::Album(id) => {
                self.selected_header = None;
                self.selected_album = Some(id.clone());
                self.selected_song_idx = None;
            }
            LibraryListItem::Song(id) => {
                self.selected_header = None;
                if self.view_mode != LibraryViewMode::Grid {
                    self.selected_album = None;
                } else {
                    self.selected_album = self.expanded_album.clone();
                }
                // Encontrar el índice local en la vista actual
                let songs_list = if self.view_mode == LibraryViewMode::Grid {
                    self.expanded_album_songs.as_ref()
                } else {
                    self.filtered_songs.as_ref()
                };

                if let Some(songs) = songs_list {
                    self.selected_song_idx = songs.iter().position(|s| s.id == *id);
                }
            }
        }
    }

    pub fn update_selection_stats(&mut self) {
        self.selection_stats = library_groups::selection_stats(&self.get_selected_songs());
    }

    pub fn get_selected_songs(&self) -> Vec<Arc<crate::db::database::SongData>> {
        let Some(songs) = self
            .filtered_songs
            .as_ref()
            .or(self.cached_all_songs.as_ref())
        else {
            return Vec::new();
        };
        library_selection::selected_songs(&self.selected_items, &self.library_data(), songs)
    }

    /// Calcula la tarea de scroll para asegurar que el elemento seleccionado sea visible.
    /// Si 'force_top' es true, el elemento se moverá directamente a la parte superior.
    pub fn get_scroll_task<Message: 'static>(&self, force_top: bool) -> Task<Message> {
        let scroll_to_offset = |offset: Option<f32>| match offset {
            Some(y) => iced::widget::operation::scroll_to(
                LIBRARY_SCROLL_ID.clone(),
                iced::widget::operation::AbsoluteOffset { x: 0.0, y },
            ),
            None => Task::none(),
        };
        let viewport = self
            .last_viewport
            .as_ref()
            .map(|v| (self.scroll_offset.y, v.height));

        // FAST PATH: use the exact hint coordinates stored during keyboard navigation.
        // Solo los artistas (sticky headers) tienen margen 0; los álbumes y canciones
        // respetan el margen para no quedar ocultos bajo el sticky header del artista.
        if let Some((hint_y, hint_h)) = self.selected_item_hint {
            return scroll_to_offset(library_geometry::scroll_offset_to_reveal(
                hint_y,
                hint_h,
                self.focused_item.as_ref(),
                self.view_mode,
                force_top,
                viewport,
            ));
        }

        // FALLBACK: name-based search (used when clicking with mouse)
        let items = self.get_visible_items();
        for (item, bounds) in &items {
            let is_match = if let Some(focused) = &self.focused_item {
                item == focused
            } else {
                match item {
                    LibraryListItem::Song(id) => {
                        if let Some(idx) = self.selected_song_idx {
                            let list = if self.view_mode == LibraryViewMode::Grid {
                                self.expanded_album_songs.as_ref()
                            } else {
                                self.filtered_songs.as_ref()
                            };
                            list.and_then(|l| l.get(idx))
                                .map(|s| s.id == *id)
                                .unwrap_or(false)
                        } else {
                            false
                        }
                    }
                    LibraryListItem::Album(id) => Some(id) == self.selected_album.as_ref(),
                    LibraryListItem::Artist(name) => Some(name) == self.selected_header.as_ref(),
                }
            };

            if is_match {
                return scroll_to_offset(library_geometry::scroll_offset_to_reveal(
                    bounds.y,
                    bounds.height,
                    Some(item),
                    self.view_mode,
                    force_top,
                    viewport,
                ));
            }
        }
        Task::none()
    }

    /// Obtiene el nombre del artista que se debe contraer/expandir según la dirección de la tecla flecha.
    pub fn get_artist_to_toggle(&self, dir: LibraryNavDir) -> Option<String> {
        library_selection::artist_to_toggle(
            self.selected_header.as_deref(),
            self.legacy_selected_song(),
            dir,
            &self.library_data(),
        )
    }

    /// Obtiene listas de álbumes y artistas a contraer/expandir en bloque.
    pub fn get_bulk_toggles(&self, dir: LibraryNavDir) -> (Vec<String>, Vec<String>) {
        library_selection::bulk_toggles(
            &self.selected_items,
            self.focused_item.as_ref(),
            dir,
            &self.library_data(),
            |song_id| self.find_album_id_for_song(song_id),
        )
    }

    pub fn get_album_to_toggle(&self, dir: LibraryNavDir) -> Option<String> {
        library_selection::album_to_toggle(
            self.selected_album.as_deref(),
            self.legacy_selected_song(),
            self.cached_albums.as_deref(),
            dir,
            &self.library_data(),
        )
    }

    /// La canción de `selected_song_idx` en `filtered_songs`, si existe.
    fn legacy_selected_song(&self) -> Option<&crate::db::database::SongData> {
        let idx = self.selected_song_idx?;
        self.filtered_songs
            .as_ref()
            .and_then(|songs| songs.get(idx))
            .map(|s| &**s)
    }

    /// Vista de solo lectura de los datos de la biblioteca para la lógica de selección.
    fn library_data(&self) -> LibraryData<'_> {
        LibraryData {
            artist_groups: &self.artist_groups,
            songs: self
                .filtered_songs
                .as_deref()
                .or(self.cached_all_songs.as_deref()),
            cached_albums: self.cached_albums.as_deref(),
            collapsed_artists: &self.collapsed_artists,
            collapsed_albums: &self.collapsed_albums,
        }
    }

    /// Saca el estado de selección (selección, foco y pivote) para operar sobre él.
    fn take_selection(&mut self) -> LibrarySelection {
        LibrarySelection {
            selected: std::mem::take(&mut self.selected_items),
            focused: self.focused_item.take(),
            pivot: self.selection_pivot.take(),
        }
    }

    /// Devuelve el estado de selección sacado con `take_selection`.
    fn restore_selection(&mut self, selection: LibrarySelection) {
        self.selected_items = selection.selected;
        self.focused_item = selection.focused;
        self.selection_pivot = selection.pivot;
    }

    /// Actualiza la memoria de la última canción seleccionada para un artista.
    pub fn update_selection_memory(&mut self) {
        if let Some(song_idx) = self.selected_song_idx {
            if let Some(songs) = &self.filtered_songs {
                if let Some(song) = songs.get(song_idx) {
                    let artist = song
                        .artist
                        .as_ref()
                        .map(|s| s.to_string())
                        .or_else(|| song.album_artist.as_ref().map(|s| s.to_string()))
                        .unwrap_or_else(|| "Artista Desconocido".to_string());
                    self.artist_last_selection.insert(artist, song_idx);
                }
            }
        }
    }

    pub fn select_song(&mut self, idx: Option<usize>) {
        self.selected_song_idx = idx;
    }

    /// Genera los parámetros de búsqueda basados en el estado actual de los filtros.
    pub fn get_search_params(&self) -> crate::db::database::LibrarySearchParams {
        crate::db::database::LibrarySearchParams {
            query: if self.search_query.is_empty() {
                None
            } else {
                Some(self.search_query.clone())
            },
            artist: self.filter_artist.clone(),
            album: self.filter_album.clone(),
            genre: self.filter_genre.clone(),
            year: self.filter_year.clone(),
            folder_id: self.filter_folder_id,
        }
    }

    /// Prepara el estado para un nuevo filtrado. En el nuevo modelo Stateless,
    /// esto ya no filtra en RAM, sino que indica que se requiere una consulta a la DB.
    pub fn apply_filter(&mut self) {
        self.invalidate_cache();
        self.artist_last_selection.clear();
        self.search_nonce = self.search_nonce.wrapping_add(1);

        // El filtrado real ahora ocurre de forma asíncrona en App vía SQL.
        // Aquí solo podríamos activar un estado de "loading" si fuera necesario.
    }
    /// Procesa el nuevo vector de canciones filtradas (vía SQL) para reconstruir grupos y grid.
    pub fn update_processed_data(
        &mut self,
        songs: Vec<std::sync::Arc<crate::db::database::SongData>>,
    ) {
        self.invalidate_cache();
        let has_any_filter = !self.search_query.is_empty()
            || self.filter_artist.is_some()
            || self.filter_album.is_some()
            || self.filter_genre.is_some()
            || self.filter_year.is_some()
            || self.filter_folder_id.is_some();

        // 1. Agrupación por Artistas
        self.artist_groups = library_groups::group_by_artist(&songs);

        // 2. Sincronización del Grid (Álbumes)
        self.filtered_albums = if has_any_filter {
            Some(library_groups::filtered_albums(
                &self.artist_groups,
                self.cached_albums.as_deref().unwrap_or_default(),
                self.sort_column,
                self.sort_ascending,
            ))
        } else {
            None
        };

        // 3. Estadísticas
        self.filter_stats = Some(library_groups::compute_stats(&songs));
        self.filtered_songs = Some(songs);

        self.refresh_expanded_album_songs();
    }

    /// Refresca la lista de canciones del álbum expandido en vista Grid basándose en los datos actuales.
    pub fn refresh_expanded_album_songs(&mut self) {
        if self.view_mode != LibraryViewMode::Grid {
            return;
        }

        if let Some(album_id) = self.expanded_album.clone() {
            // Ahora buscamos en filtered_songs o disparamos consulta específica si fuera necesario.
            // Para simplicidad por ahora, si las canciones están en filtered_songs las usamos.
            if let Some(songs) = &self.filtered_songs {
                if let Some(album_entry) = self.cached_albums.as_ref().and_then(|all| {
                    all.iter().find(|a| {
                        (library_groups::album_composite_id(&a.artist, &a.id) == album_id)
                            || (a.id == album_id)
                    })
                }) {
                    let mut alb_songs: Vec<_> = songs
                        .iter()
                        .filter(|s| {
                            let s_alb = s.album.as_deref().unwrap_or("Desconocido");
                            let s_art = crate::utils::get_effective_artist(s);
                            s_alb == album_entry.title && s_art == album_entry.artist
                        })
                        .cloned()
                        .collect();

                    if !alb_songs.is_empty() {
                        self.sort_songs(&mut alb_songs);
                        self.expanded_album_songs = Some(alb_songs);
                    } else {
                        self.expanded_album = None;
                        self.expanded_album_songs = None;
                    }
                }
            }
        }
    }
}

pub fn view<'a>(
    manager: &'a LibraryManager,
    _database: &'a Arc<Mutex<Database>>,
    playing_path: &'a str,
    meter_state: &'a crate::gui::player::MeterUiState,
    clipping: bool,
    meter_enabled: bool,
) -> Element<'a, Message> {
    library_redraw_count_log();
    // Función auxiliar para iconos sin fondo (top y bottom bar) interactivos
    let icon_btn_size = |icon: &str, action: Message, size: f32| -> Element<'a, Message> {
        let content = iced::widget::svg(iced::widget::svg::Handle::from_path(format!(
            "assets/icons/{}",
            icon
        )))
        .width(Length::Fixed(size))
        .height(Length::Fixed(size));
        iced::widget::mouse_area(content)
            .on_press(action)
            .interaction(iced::mouse::Interaction::Pointer)
            .into()
    };

    let _icon_btn =
        |icon: &str, action: Message| -> Element<'a, Message> { icon_btn_size(icon, action, 20.0) };

    // Controles de ventana
    let win_action_btn = |icon: &str, action: Message, size: f32| -> Element<'a, Message> {
        let content = iced::widget::svg(iced::widget::svg::Handle::from_path(format!(
            "assets/icons/{}",
            icon
        )))
        .width(Length::Fixed(size))
        .height(Length::Fixed(size));

        iced::widget::mouse_area(content)
            .on_press(action)
            .interaction(iced::mouse::Interaction::Pointer)
            .into()
    };

    // --- BARRA SUPERIOR (40px) ---
    let create_source_tab =
        |label: &'a str, source: LibrarySource, current: LibrarySource| -> Element<'a, Message> {
            let is_active = source == current;
            let text_el = text(label)
                .size(15)
                .font(FONT_INTER_SANS_MEDIUM)
                .color(if is_active {
                    Color::WHITE
                } else {
                    COLOR_TEXT_SECONDARY
                });

            let source_color = match source {
                LibrarySource::Spotify => Color::from_rgb8(29, 185, 84),
                LibrarySource::YouTube => Color::from_rgb8(221, 36, 37),
                LibrarySource::Qobuz => Color::from_rgb8(48, 48, 48),
                LibrarySource::Deezer => Color::from_rgb8(162, 56, 255),
                LibrarySource::Tidal => Color::from_rgb8(68, 68, 68),
                LibrarySource::Local => COLOR_ACCENT,
            };

            let btn_container = container(text_el)
                .padding([2, 10]) // 3px arriba/abajo, 10px lados
                .style(move |_t: &Theme| {
                    if is_active {
                        container::Style::default()
                            .background(source_color)
                            .border(iced::Border {
                                radius: 14.0.into(),
                                ..Default::default()
                            })
                    } else {
                        container::Style::default()
                    }
                });

            iced::widget::mouse_area(btn_container)
                .on_press(Message::LibrarySourceSelected(source))
                .interaction(iced::mouse::Interaction::Pointer)
                .into()
        };

    let source_tabs = row![
        create_source_tab("Biblioteca", LibrarySource::Local, manager.source),
        create_source_tab("Spotify", LibrarySource::Spotify, manager.source),
        create_source_tab("YouTube", LibrarySource::YouTube, manager.source),
        create_source_tab("Qobuz", LibrarySource::Qobuz, manager.source),
        create_source_tab("Deezer", LibrarySource::Deezer, manager.source),
        create_source_tab("Tidal", LibrarySource::Tidal, manager.source),
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .height(Length::Fill);

    let top_bar = row![
        source_tabs,
        Space::new().width(Length::Fill),
        row![
            container(win_action_btn(
                "minimize.svg",
                Message::PlayerWindowAction(crate::gui::player::WindowAction::Minimize),
                30.0
            ))
            .padding(iced::Padding {
                top: 0.0,
                right: 0.0,
                bottom: 8.0,
                left: 0.0
            }),
            win_action_btn(
                "maximize.svg",
                Message::PlayerWindowAction(crate::gui::player::WindowAction::Maximize),
                30.0
            ),
            win_action_btn(
                "close.svg",
                Message::PlayerWindowAction(crate::gui::player::WindowAction::Close),
                34.0
            ),
        ]
        .spacing(5)
        .align_y(Alignment::Center)
    ]
    .align_y(Alignment::Center)
    .height(Length::Fill)
    .padding(iced::Padding {
        top: 0.0,
        right: 5.0,
        bottom: 0.0,
        left: 15.0,
    });

    let top_container = container(top_bar)
        .width(Length::Fill)
        .height(Length::Fixed(40.0))
        .style(|_t: &Theme| container::Style::default().background(COLOR_CONTRAST));

    // --- BARRA DE ORDENAMIENTO (30px) ---
    let columns = if manager.view_mode == LibraryViewMode::DetailedList {
        vec![
            SortColumn::AlbumCard,
            SortColumn::TrackNumber,
            SortColumn::Title,
            SortColumn::Artist,
            SortColumn::AlbumArtist,
            SortColumn::Duration,
            SortColumn::Format,
            SortColumn::SampleRate,
            SortColumn::BitDepth,
            SortColumn::Bitrate,
            SortColumn::Channels,
            SortColumn::Size,
        ]
    } else if manager.view_mode == LibraryViewMode::ThumbnailList {
        vec![
            SortColumn::AlbumThumbnail,
            SortColumn::TrackNumber,
            SortColumn::Title,
            SortColumn::Artist,
            SortColumn::AlbumArtist,
            SortColumn::Album,
            SortColumn::Genre,
            SortColumn::Year,
            SortColumn::Duration,
            SortColumn::Format,
            SortColumn::SampleRate,
            SortColumn::BitDepth,
            SortColumn::Bitrate,
            SortColumn::Channels,
            SortColumn::Size,
        ]
    } else {
        vec![
            SortColumn::TrackNumber,
            SortColumn::Title,
            SortColumn::Artist,
            SortColumn::AlbumArtist,
            SortColumn::Album,
            SortColumn::Genre,
            SortColumn::Year,
            SortColumn::Duration,
            SortColumn::Format,
            SortColumn::SampleRate,
            SortColumn::BitDepth,
            SortColumn::Bitrate,
            SortColumn::Channels,
            SortColumn::Size,
        ]
    };

    let sort_bar_pad = match manager.view_mode {
        LibraryViewMode::Grid => 19.0,
        LibraryViewMode::DetailedList => 29.0,
        LibraryViewMode::ThumbnailList => 31.0,
        LibraryViewMode::SimpleList => 29.0,
    };

    let sort_container = crate::gui::widgets::build_sort_bar(
        &columns,
        manager.sort_column,
        manager.sort_ascending,
        &manager.column_widths,
        manager.resizing_column,
        manager.hovered_column,
        sort_bar_pad,
        |col| Message::ColumnHover(col),
        |col| Message::StartColumnResize(col),
        |col| Message::LibrarySortChanged(col),
    );

    // --- CONTENIDO GRID / LISTA ---
    let content: Element<'a, Message> = match manager.view_mode {
        LibraryViewMode::Grid => {
            // Determinamos qué álbumes mostrar: filtrados o todos
            let albums_to_show = if let Some(filtered) = &manager.filtered_albums {
                filtered
            } else if let Some(cached) = &manager.cached_albums {
                cached
            } else {
                static EMPTY: Vec<AlbumEntry> = Vec::new();
                &EMPTY
            };

            if albums_to_show.is_empty() {
                container(
                    text(if manager.search_query.is_empty() {
                        "La biblioteca está vacía."
                    } else {
                        "No se encontraron álbumes."
                    })
                    .color(COLOR_TEXT_SECONDARY)
                    .font(FONT_INTER_SANS_MEDIUM),
                )
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into()
            } else {
                let res_grid = iced::widget::responsive(move |size| {
                    let card_w = 187.0;
                    let mut columns_count = (size.width / card_w).floor() as usize;
                    if columns_count < 2 {
                        columns_count = 2;
                    }
                    let max_cols = if crate::utils::is_low_resource() {
                        5
                    } else {
                        10
                    };
                    if columns_count > max_cols {
                        columns_count = max_cols;
                    }
                    manager.albums_per_row.set(columns_count);

                    let view_h = manager
                        .last_viewport
                        .as_ref()
                        .map(|v| v.height)
                        .unwrap_or(1000.0);
                    let (top_space, bottom_space, visible_elements) = manager
                        .get_visible_grid_elements(columns_count, manager.scroll_offset.y, view_h);

                    let mut grid_col: iced::widget::Column<'_, Message> = column![].spacing(0);

                    if top_space > 0.0 {
                        grid_col = grid_col.push(Space::new().height(Length::Fixed(top_space)));
                    }

                    for element in visible_elements {
                        match element {
                            LibraryGridElement::AlbumRow {
                                albums,
                                active_expansion_id,
                            } => {
                                let mut current_row = row![].spacing(5);

                                for album_entry in albums {
                                    let album_id = album_entry.id.clone();
                                    let album_title = album_entry.title.clone();
                                    let artist_name = album_entry.artist.clone();
                                    let genre_name = album_entry.genre.clone();
                                    let year_str = album_entry.year.clone();
                                    let cover_path = album_entry.cover_path.clone();

                                    let composite_id =
                                        library_groups::album_composite_id(&artist_name, &album_id);
                                    let is_expanded = active_expansion_id.as_deref()
                                        == Some(composite_id.as_str());
                                    let is_selected = manager
                                        .selected_items
                                        .contains(&LibraryListItem::Album(composite_id.clone()));

                                    let album_art = crate::gui::widgets::album_art_widget(
                                        cover_path.as_deref(),
                                        None,
                                        None,
                                        crate::gui::widgets::PlaceholderStyle::Large,
                                        Length::Fixed(152.0),
                                        8.0,
                                    );

                                    let info_col = column![
                                        crate::gui::widgets::smart_truncate_text(
                                            artist_name,
                                            12.0,
                                            FONT_INTER_SANS_MEDIUM,
                                            COLOR_TEXT_PRIMARY
                                        ),
                                        crate::gui::widgets::smart_truncate_text(
                                            album_title,
                                            12.0,
                                            FONT_INTER_SANS_MEDIUM,
                                            COLOR_TEXT_PRIMARY
                                        ),
                                        crate::gui::widgets::smart_truncate_text(
                                            genre_name,
                                            12.0,
                                            FONT_INTER_SANS_MEDIUM,
                                            COLOR_TEXT_PRIMARY
                                        ),
                                        text(year_str)
                                            .size(12)
                                            .color(COLOR_TEXT_PRIMARY)
                                            .font(FONT_INTER_SANS_MEDIUM)
                                            .line_height(iced::widget::text::LineHeight::Absolute(
                                                iced::Pixels(14.0)
                                            )),
                                    ]
                                    .spacing(2)
                                    .width(Length::Fill);

                                    let chevron_svg = if is_expanded {
                                        "arrow-up-chevron.svg"
                                    } else {
                                        "arrow-down-chevron.svg"
                                    };
                                    let chevron_btn = button(
                                        iced::widget::svg(iced::widget::svg::Handle::from_path(
                                            format!("assets/icons/{}", chevron_svg),
                                        ))
                                        .width(30)
                                        .height(30)
                                        .style(
                                            move |_t: &Theme, _s| iced::widget::svg::Style {
                                                color: Some(COLOR_TEXT_PRIMARY),
                                            },
                                        ),
                                    )
                                    .padding(0)
                                    .on_press(Message::ToggleAlbumExpansion(composite_id.clone()))
                                    .style(|_t, _s| {
                                        button::Style::default().with_background(Color::TRANSPARENT)
                                    });

                                    let card_bottom = row![info_col, chevron_btn]
                                        .align_y(Alignment::Center)
                                        .width(Length::Fill);

                                    let item_col = column![album_art, card_bottom].spacing(5);
                                    let card_wrapper = mouse_area(item_col)
                                        .on_press(Message::SelectAlbum(composite_id.clone()))
                                        .interaction(iced::mouse::Interaction::Pointer);

                                    let card_container = container(card_wrapper)
                                        .width(Length::Fixed(182.0))
                                        .padding(iced::Padding {
                                            top: 18.0,
                                            bottom: 15.0,
                                            left: 15.0,
                                            right: 15.0,
                                        })
                                        .style(move |_t: &Theme| {
                                            if is_expanded || is_selected {
                                                container::Style::default()
                                                    .background(COLOR_CONTRAST)
                                                    .border(iced::Border {
                                                        radius: 10.0.into(),
                                                        ..Default::default()
                                                    })
                                            } else {
                                                container::Style::default()
                                            }
                                        });

                                    current_row = current_row.push(card_container);
                                }
                                grid_col = grid_col.push(current_row);
                            }
                            LibraryGridElement::ExpandedSongs {
                                album_id: _,
                                songs,
                                top_space: s_top,
                                bottom_space: s_bottom,
                            } => {
                                let mut album_songs_col = column![].spacing(0).padding([32, 0]);

                                if s_top > 0.0 {
                                    album_songs_col = album_songs_col
                                        .push(Space::new().height(Length::Fixed(s_top)));
                                }

                                for (song, _song_i) in songs {
                                    let is_song_selected = manager
                                        .selected_items
                                        .contains(&LibraryListItem::Song(song.id));
                                    let columns = [
                                        SortColumn::TrackNumber,
                                        SortColumn::Title,
                                        SortColumn::Artist,
                                        SortColumn::AlbumArtist,
                                        SortColumn::Album,
                                        SortColumn::Genre,
                                        SortColumn::Year,
                                        SortColumn::Duration,
                                        SortColumn::Format,
                                        SortColumn::SampleRate,
                                        SortColumn::BitDepth,
                                        SortColumn::Bitrate,
                                        SortColumn::Channels,
                                        SortColumn::Size,
                                    ];

                                    let song_row = crate::gui::widgets::universal_song_row_widget(
                                        &song,
                                        0,
                                        is_song_selected,
                                        &columns,
                                        &manager.column_widths,
                                        Message::SelectSong(Some(song.id)),
                                        playing_path,
                                        32.0,
                                        false,
                                    );
                                    album_songs_col = album_songs_col.push(song_row);
                                }

                                if s_bottom > 0.0 {
                                    album_songs_col = album_songs_col
                                        .push(Space::new().height(Length::Fixed(s_bottom)));
                                }

                                grid_col = grid_col.push(
                                    container(album_songs_col)
                                        .width(Length::Fill)
                                        .padding(iced::Padding {
                                            top: 0.0,
                                            right: 10.0,
                                            bottom: 5.0,
                                            left: 5.0,
                                        })
                                        .style(|_t| {
                                            container::Style::default().background(COLOR_BG)
                                        }),
                                );
                            }
                        }
                    }

                    if bottom_space > 0.0 {
                        grid_col = grid_col.push(Space::new().height(Length::Fixed(bottom_space)));
                    }

                    grid_col.into()
                });

                // IMPORTANTE: NO usamos IDs dinámicos (A/B) basados en search_nonce a menos que sea estrictamente necesario.
                // Mantener el scroll_id constante permite que Iced mantenga el caché de carátulas y estado del scroll.
                let scroll_id = LIBRARY_SCROLL_ID.clone();
                let scrollable_grid = standard_scrollable(
                    scroll_id,
                    res_grid,
                    iced::widget::scrollable::Direction::Vertical(standard_scrollbar()),
                )
                .width(Length::Fill)
                .height(Length::Fill)
                .on_scroll(Message::LibraryScroll);

                container(scrollable_grid)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into()
            }
        }
        LibraryViewMode::SimpleList => {
            let cols = columns.clone();
            crate::gui::widgets::universal_song_list(
                manager,
                move |song, _song_idx, is_selected, playing_path| {
                    crate::gui::widgets::library_song_row_widget(
                        song,
                        0,
                        is_selected,
                        &cols,
                        &manager.column_widths,
                        Message::SelectSong(Some(song.id)),
                        playing_path,
                    )
                },
                32.0,
                playing_path,
            )
        }
        LibraryViewMode::DetailedList => {
            let cols = columns;
            crate::gui::widgets::detailed_song_list(
                manager,
                move |song, _song_idx, is_selected, playing_path| {
                    crate::gui::widgets::library_song_row_widget(
                        song,
                        0,
                        is_selected,
                        &cols,
                        &manager.column_widths,
                        Message::SelectSong(Some(song.id)),
                        playing_path,
                    )
                },
                32.0,
                playing_path,
            )
        }
        LibraryViewMode::ThumbnailList => {
            let cols = columns.clone();
            crate::gui::widgets::universal_song_list(
                manager,
                move |song, _song_idx, is_selected, playing_path| {
                    crate::gui::widgets::thumbnail_song_row_widget(
                        song,
                        0,
                        is_selected,
                        &cols,
                        &manager.column_widths,
                        Message::SelectSong(Some(song.id)),
                        playing_path,
                    )
                },
                42.0,
                playing_path,
            )
        }
    };

    // --- BARRA INFERIOR (40px) ---
    let search_input = crate::gui::widgets::standard_search_input(
        Some(LIBRARY_SEARCH_ID.clone()),
        "Buscar...",
        &manager.search_query,
        Message::LibrarySearchQueryChanged,
        Message::LibrarySearchQueryChanged(String::new()),
        Length::Fixed(180.0),
    );

    let (s_count, a_count, art_count, d_secs, s_bytes) = if let Some(sel) = &manager.selection_stats
    {
        (
            sel.songs,
            sel.albums,
            sel.artists,
            sel.duration_secs,
            sel.size_bytes,
        )
    } else if let Some(flt) = &manager.filter_stats {
        (
            flt.songs,
            flt.albums,
            flt.artists,
            flt.duration_secs,
            flt.size_bytes,
        )
    } else {
        (
            manager.total_songs as u64,
            manager.total_albums as u64,
            manager.total_artists as u64,
            manager.total_duration_secs,
            manager.total_size_bytes,
        )
    };

    let stats_text = if manager.view_mode == LibraryViewMode::Grid {
        format!(
            "{} Canciones | {} Álbumes | {} Artistas | {} | {}",
            s_count,
            a_count,
            art_count,
            format_duration(d_secs),
            format_size(s_bytes as i64)
        )
    } else {
        format!(
            "{} Canciones | {} Álbumes | {} Artistas | {} | {}",
            s_count,
            a_count,
            art_count,
            format_duration(d_secs),
            format_size(s_bytes as i64)
        )
    };

    let view_icon_str = match manager.view_mode {
        LibraryViewMode::Grid => "view-grid-outlined.svg",
        LibraryViewMode::ThumbnailList => "view-list-thumbnail-outlined.svg",
        LibraryViewMode::DetailedList => "view-list-thumbnail-fill.svg",
        LibraryViewMode::SimpleList => "view-list.svg",
    };

    let bottom_actions = row![
        icon_btn_size(
            "play-straight-outlined.svg",
            Message::PlayLibrarySelection,
            36.0
        ),
        Space::new().width(5.0),
        icon_btn_size("more-small.svg", Message::OpenFolderPicker, 31.0),
        Space::new().width(5.0),
        icon_btn_size("radio-button-on.svg", Message::LibraryShowPlaying, 30.0),
        Space::new().width(5.0),
        icon_btn_size(view_icon_str, Message::ToggleLibraryViewDropdown, 30.0),
    ]
    .align_y(Alignment::Center)
    .height(Length::Fill);

    let meter_widget = VuMeterWidget::new(
        meter_state.display_l,
        meter_state.smooth_peak_l,
        meter_state.peak_hold_l,
        meter_state.display_r,
        meter_state.smooth_peak_r,
        meter_state.peak_hold_r,
        clipping,
        meter_state.ms_mode,
        meter_state.phase_corr,
        meter_state.show_numeric,
        meter_state.crest_l,
        meter_state.crest_r,
    );

    // Envolver el medidor con mouse_area para capturar right-click
    let meter_with_rightclick: Element<'_, Message> = mouse_area(
        iced::widget::container(meter_widget)
            .padding(0)
            .width(Length::Shrink)
            .height(Length::Shrink),
    )
    .on_press(Message::NoOp)
    .on_right_press(Message::MeterRightClicked(iced::Point::ORIGIN))
    .into();

    let bottom_bar = if meter_enabled {
        row![
            search_input,
            Space::new().width(15.0),
            text(stats_text)
                .size(13)
                .color(COLOR_TEXT_SECONDARY)
                .font(FONT_INTER_SANS_MEDIUM),
            Space::new().width(10.0),
            meter_with_rightclick,
            Space::new().width(10.0),
            bottom_actions
        ]
        .padding([0, 15])
        .height(Length::Fill)
        .align_y(Alignment::Center)
    } else {
        row![
            search_input,
            Space::new().width(15.0),
            text(stats_text)
                .size(13)
                .color(COLOR_TEXT_SECONDARY)
                .font(FONT_INTER_SANS_MEDIUM),
            Space::new().width(Length::Fill),
            bottom_actions
        ]
        .padding([0, 15])
        .height(Length::Fill)
        .align_y(Alignment::Center)
    };

    let content_with_deselection = iced::widget::mouse_area(
        container(content)
            .padding(iced::Padding {
                top: 0.0,
                right: 0.0,
                bottom: 0.0,
                left: 5.0,
            })
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .on_press(Message::LibraryDeselect);

    let mut content_stack = iced::widget::Stack::new().push(content_with_deselection);

    if manager.is_dragging {
        if let (Some(start), Some(end)) = (manager.marquee_start, manager.marquee_end) {
            let dx = start.x - end.x;
            let dy = start.y - end.y;
            let dist_sq = dx * dx + dy * dy;

            if dist_sq > 9.0 {
                let scroll_y = manager.scroll_offset.y;
                let visual_start_y = start.y - scroll_y;
                let visual_end_y = end.y - scroll_y;

                // Calculamos los límites visuales reales (clamped al viewport del área de contenido)
                let y_min = visual_start_y.min(visual_end_y);
                let y_max = visual_start_y.max(visual_end_y);

                let x = start.x.min(end.x);
                let y = y_min.max(0.0);
                let w = dx.abs().max(1.0);
                let h = (y_max - y).max(0.0);

                if h > 0.1 && w > 0.1 {
                    let marquee_rect = container(
                        iced::widget::Space::new()
                            .width(Length::Fill)
                            .height(Length::Fill),
                    )
                    .width(Length::Fixed(w))
                    .height(Length::Fixed(h))
                    .style(move |_t| container::Style {
                        background: Some(COLOR_ACCENT.scale_alpha(0.3).into()),
                        border: iced::Border {
                            color: COLOR_ACCENT,
                            width: 1.0,
                            radius: 4.0.into(),
                        },
                        ..Default::default()
                    });

                    content_stack =
                        content_stack.push(container(marquee_rect).padding(iced::Padding {
                            top: y,
                            right: 0.0,
                            bottom: 0.0,
                            left: x,
                        }));
                }
            }
        }
    }

    let final_view = mouse_area(
        container(column![
            top_container,
            sort_container,
            content_stack,
            container(bottom_bar)
                .width(Length::Fill)
                .height(Length::Fixed(40.0))
                .style(|_t: &Theme| container::Style::default().background(COLOR_CONTRAST))
        ])
        .width(Length::Fill)
        .height(Length::Fill)
        .clip(true)
        .style(|_t: &Theme| container::Style::default().background(COLOR_BG)),
    )
    .on_press(Message::LibraryFocus);

    final_view.into()
}

#[cfg(test)]
mod library_tests {
    use super::*;
    use crate::db::database::SongData;
    use crate::utils::SortColumn;
    use std::sync::Arc;

    fn song_with(duration: Option<f64>, size: Option<i64>) -> Arc<SongData> {
        let mut song = SongData::default();
        song.duration_secs = duration;
        song.size = size;
        Arc::new(song)
    }

    /// Guarda de regresión: el comparador de bitrate calcula la tasa con
    /// `filter` en lugar de `unwrap`, de modo que una duración ausente (`None`)
    /// o cero no lo rompe. El orden debe ser estable entre llamadas.
    #[test]
    fn test_bitrate_sort_handles_missing_duration() {
        let mut songs = vec![
            song_with(None, Some(10_000)),
            song_with(Some(0.0), Some(10_000)),
            song_with(Some(120.0), Some(4_000_000)),
            song_with(Some(60.0), Some(500_000)),
        ];

        let sort_snapshot = |songs: &mut Vec<Arc<SongData>>| {
            LibraryManager::sort_songs_static(songs, Some(SortColumn::Bitrate), Some(true));
            songs
                .iter()
                .map(|s| (s.duration_secs, s.size))
                .collect::<Vec<_>>()
        };

        let first = sort_snapshot(&mut songs.clone());
        let second = sort_snapshot(&mut songs);
        assert_eq!(
            first, second,
            "el orden por bitrate con duración ausente/cero debe ser estable"
        );

        // Las entradas sin duración válida quedan al principio (tasa 0.0).
        assert_eq!(first[0].0, None);
        assert_eq!(first[1].0, Some(0.0));
    }
}
