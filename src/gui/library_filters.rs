use crate::gui::app::Message;
use crate::gui::theme::*;
use crate::gui::widgets::{chevron_btn, standard_scrollable, standard_scrollbar};
use crate::logic::filter_index::{self, FilterIndex};
use iced::{
    Alignment, Color, Element, Length, Theme,
    widget::{Space, button, column, container, mouse_area, row, text},
};
use std::cell::RefCell;
use std::collections::HashSet;

pub use crate::logic::filter_index::{FilterType, TreeNode, VisibleTreeItem};

impl FilterType {
    pub fn label(&self) -> &'static str {
        match self {
            FilterType::Folder => "Carpeta",
            FilterType::Artist => "Artista",
            FilterType::Album => "Álbum",
            FilterType::Genre => "Género",
            FilterType::Year => "Año",
        }
    }
}

pub struct LibraryFiltersManager {
    pub current_filter: FilterType,
    pub menu_open: bool,
    pub selected_subfilter: Option<String>,
    pub expanded_nodes: HashSet<String>,
    pub search_query: String, // internal search specific to this module

    // Índice pre-calculado (se construye una vez con build_filter_index)
    filter_index: Option<FilterIndex>,

    // Datos visibles para la UI (se actualizan con apply_view)
    pub active_subfilters: Vec<String>, // available characters e.g. ["#", "·", "A", "C", "R"]
    pub tree_data: Vec<TreeNode>,
    pub selected_tree_node: Option<String>,

    // Virtualización
    pub scroll_offset: iced::Vector,
    pub last_viewport: Option<iced::Rectangle>,
    pub cached_flattened_tree: RefCell<Option<Vec<VisibleTreeItem>>>,
}

pub static FILTERS_SCROLL_ID: std::sync::LazyLock<iced::widget::Id> =
    std::sync::LazyLock::new(iced::widget::Id::unique);

impl Default for LibraryFiltersManager {
    fn default() -> Self {
        Self {
            current_filter: FilterType::Genre,
            menu_open: false,
            selected_subfilter: None,
            expanded_nodes: HashSet::new(),
            search_query: String::new(),
            filter_index: None,
            active_subfilters: Vec::new(),
            tree_data: Vec::new(),
            selected_tree_node: None,
            scroll_offset: iced::Vector::new(0.0, 0.0),
            last_viewport: None,
            cached_flattened_tree: RefCell::new(None),
        }
    }
}

impl LibraryFiltersManager {
    /// Invalida el caché del árbol aplanado para liberar memoria
    pub fn invalidate_cache(&mut self) {
        *self.cached_flattened_tree.borrow_mut() = None;
    }

    /// Construye el índice de filtros UNA sola vez. Llamar al cargar canciones o cuando el escáner detecta cambios.
    pub fn build_filter_index(
        &mut self,
        db_m: &std::sync::Arc<std::sync::Mutex<crate::db::database::Database>>,
    ) {
        let all_types = [
            FilterType::Genre,
            FilterType::Artist,
            FilterType::Album,
            FilterType::Year,
            FilterType::Folder,
        ];

        let mut index = FilterIndex::new();

        // db-lock: solo lectura — refresco best-effort, se omite si el lock falla
        if let Ok(db) = db_m.lock() {
            for ft in all_types {
                if let Ok(hierarchy) = db.get_filter_hierarchy(ft.name()) {
                    index.insert(ft, hierarchy);
                }
            }
        }

        self.filter_index = Some(index);

        // Actualizar la vista inmediatamente
        self.apply_view();
    }

    pub fn refresh_flattened_tree(&self) {
        let flattened = filter_index::flatten_tree(&self.tree_data, &self.expanded_nodes);
        *self.cached_flattened_tree.borrow_mut() = Some(flattened);
    }

    pub fn get_visible_tree_items(&self) -> (f32, f32, Vec<VisibleTreeItem>) {
        if self.cached_flattened_tree.borrow().is_none() {
            self.refresh_flattened_tree();
        }

        let flattened_borrow = self.cached_flattened_tree.borrow();
        let flattened = flattened_borrow.as_deref().unwrap_or_default();

        let viewport_height = self.last_viewport.map(|v| v.height).unwrap_or(800.0);
        // D-05: Margen dinámico = max(5, total_visible_items / 2)
        let (start_index, end_index, top_space, bottom_space) =
            filter_index::tree_window(flattened.len(), self.scroll_offset.y, viewport_height);

        let visible_items = flattened[start_index..end_index].to_vec();

        (top_space, bottom_space, visible_items)
    }

    /// Actualiza tree_data y active_subfilters desde el índice pre-calculado.
    /// Operación instantánea (<1ms). Llamar al cambiar filtro general, subfiltro o búsqueda.
    pub fn apply_view(&mut self) {
        self.tree_data.clear();
        self.active_subfilters.clear();

        let index = match &self.filter_index {
            Some(idx) => idx,
            None => {
                self.refresh_flattened_tree();
                return;
            }
        };

        // 1. Subfilters (letras del abecedario) - directo de la caché
        if let Some(chars) = index.subfilters(self.current_filter) {
            self.active_subfilters = chars.to_vec();
        }

        // 2. Árbol filtrado (subfiltro por letra y búsqueda de texto), en paralelo por nodo raíz
        if let Some(full_tree) = index.tree(self.current_filter) {
            let subfilter = self.selected_subfilter.as_deref();
            let query = self.search_query.as_str();

            use rayon::prelude::*;
            self.tree_data = full_tree
                .par_iter()
                .filter_map(|node| filter_index::filter_root(node, subfilter, query))
                .collect();
        }
        self.refresh_flattened_tree();
    }
}

pub fn view<'a>(
    manager: &'a LibraryFiltersManager,
) -> (Element<'a, Message>, Option<Element<'a, Message>>) {
    // --- 1. TOP BAR (40px) ---
    let top_bar = container(
        text("AuDoxiDY")
            .font(FONT_STAGE_WANDER)
            .size(19)
            .color(COLOR_TEXT_PRIMARY),
    )
    .width(Length::Fill)
    .height(Length::Fixed(40.0))
    .align_x(iced::alignment::Horizontal::Center)
    .padding(iced::Padding {
        top: 9.0,
        bottom: 0.0,
        left: 0.0,
        right: 0.0,
    })
    .style(|_t: &Theme| container::Style::default().background(COLOR_CONTRAST));

    // --- 2. MIDDLE SECTION ---
    let mut middle_content = column![].spacing(0);

    // Dropdown Header
    let drop_icon = if manager.menu_open {
        "arrow-up-chevron.svg"
    } else {
        "arrow-down-chevron.svg"
    };
    let dropdown_header = button(
        row![
            text(manager.current_filter.label())
                .size(14)
                .font(FONT_INTER_SANS_MEDIUM),
            Space::new().width(Length::Fill),
            iced::widget::svg(iced::widget::svg::Handle::from_path(format!(
                "assets/icons/{}",
                drop_icon
            )))
            .width(28)
            .height(28)
            .style(
                |_t: &Theme, _s: iced::widget::svg::Status| iced::widget::svg::Style {
                    color: Some(COLOR_TEXT_SECONDARY)
                }
            )
        ]
        .align_y(Alignment::Center)
        .padding(iced::Padding {
            top: 0.0,
            bottom: 0.0,
            left: 15.0,
            right: 7.0,
        }),
    )
    .width(Length::Fill)
    .height(Length::Fixed(28.0))
    .padding(0)
    .style(move |_t: &Theme, _s: button::Status| {
        let mut st = button::Style::default().with_background(COLOR_BG);
        st.border.radius = 0.0.into();
        st.text_color = COLOR_TEXT_SECONDARY;
        st
    })
    .on_press(Message::ToggleFilterMenu);

    middle_content = middle_content.push(column![
        dropdown_header,
        container(iced::widget::Space::new().height(2.0))
            .width(Length::Fill)
            .style(|_t: &Theme| container::Style::default().background(COLOR_CONTRAST))
    ]);

    // Build Menu Options layout (to be overlayed)
    let menu_options_container = if manager.menu_open {
        let all_filters = vec![
            FilterType::Folder,
            FilterType::Artist,
            FilterType::Album,
            FilterType::Genre,
            FilterType::Year,
        ];

        let mut content = column![].spacing(0).width(Length::Fixed(172.0));

        for f in all_filters {
            let is_selected = manager.current_filter == f;

            let icon_path = match f {
                FilterType::Folder => "folder-open-outlined.svg",
                FilterType::Artist => "artist-outlined-straight.svg",
                FilterType::Album => "album.svg",
                FilterType::Genre => "music-note-2-straight.svg",
                FilterType::Year => "calendar-rounded.svg",
            };

            let btn = button(
                container(
                    row![
                        container(
                            iced::widget::svg(iced::widget::svg::Handle::from_path(format!(
                                "assets/icons/{}",
                                icon_path
                            )))
                            .width(Length::Fixed(18.0))
                            .height(Length::Fixed(18.0))
                            .style(|_t, _s| {
                                iced::widget::svg::Style {
                                    color: Some(COLOR_TEXT_PRIMARY),
                                }
                            })
                        )
                        .width(Length::Fixed(18.0)),
                        Space::new().width(Length::Fixed(15.0)),
                        text(f.label())
                            .size(14)
                            .font(FONT_INTER_SANS_NORMAL)
                            .wrapping(iced::widget::text::Wrapping::None)
                    ]
                    .align_y(Alignment::Center),
                )
                .width(Length::Fill)
                .height(Length::Fixed(32.0))
                .padding(iced::Padding {
                    left: 15.0,
                    right: 10.0,
                    ..Default::default()
                })
                .align_y(iced::alignment::Vertical::Center),
            )
            .on_press(Message::ChangeGeneralFilter(f))
            .padding(0)
            .style(move |_t: &Theme, status: iced::widget::button::Status| {
                let is_hovered = matches!(status, iced::widget::button::Status::Hovered);

                button::Style {
                    background: if is_hovered {
                        Some(COLOR_CONTRAST.into())
                    } else {
                        None
                    },
                    text_color: if is_selected {
                        COLOR_ACCENT
                    } else {
                        COLOR_TEXT_PRIMARY
                    },
                    border: iced::Border {
                        radius: 0.0.into(),
                        width: 0.0,
                        color: Color::TRANSPARENT,
                    },
                    ..Default::default()
                }
            });

            content = content.push(btn);
        }

        Some(
            container(content)
                .width(Length::Fixed(172.0))
                .style(|_t: &Theme| {
                    container::Style::default()
                        .background(COLOR_BG)
                        .border(iced::Border {
                            color: COLOR_TEXT_SECONDARY,
                            width: 1.0,
                            radius: 8.0.into(),
                        })
                        .shadow(iced::Shadow {
                            offset: iced::Vector::new(0.0, 4.0),
                            blur_radius: 10.0,
                            color: Color::from_rgba8(0, 0, 0, 0.5),
                        })
                })
                .padding(iced::Padding {
                    top: 5.0,
                    bottom: 5.0,
                    left: 1.0,
                    right: 1.0,
                }),
        )
    } else {
        None
    };

    // Subfilters (Alfabeto/Numeros/Signos)
    if !manager.active_subfilters.is_empty() {
        let mut rows_of_chars = column![].spacing(4).padding(iced::Padding {
            top: 10.0,
            bottom: 10.0,
            left: 15.0,
            right: 15.0,
        });

        let mut line_width = 0.0;
        let max_w = 202.0 - 30.0; // 202 total - 15 padding L/R
        let mut current_row = iced::widget::Row::new().spacing(0);

        let restore_icon = container(
            iced::widget::svg(iced::widget::svg::Handle::from_path(
                "assets/icons/restore-straight.svg",
            ))
            .width(14)
            .height(14)
            .style(move |_t, s: iced::widget::svg::Status| {
                if s == iced::widget::svg::Status::Hovered {
                    iced::widget::svg::Style {
                        color: Some(COLOR_TEXT_PRIMARY),
                    }
                } else {
                    iced::widget::svg::Style {
                        color: Some(COLOR_TEXT_SECONDARY),
                    }
                }
            }),
        )
        .width(Length::Fixed(16.0))
        .height(Length::Fixed(16.0))
        .center_x(Length::Fill)
        .center_y(Length::Fill);

        let restore_btn = button(restore_icon)
            .width(Length::Fixed(16.0))
            .height(Length::Fixed(16.0))
            .padding(0)
            .style(move |_t: &Theme, _s: button::Status| {
                let mut st = button::Style::default().with_background(Color::TRANSPARENT);
                if _s == button::Status::Pressed {
                    st.background = Some(iced::Background::Color(COLOR_ACCENT));
                    st.text_color = COLOR_TEXT_PRIMARY;
                    st.border.radius = 18.0.into();
                } else {
                    st.text_color = COLOR_TEXT_SECONDARY;
                }
                st
            })
            .on_press(Message::SelectSubfilter(None));

        current_row = current_row.push(restore_btn);
        line_width += 16.0;

        for c in &manager.active_subfilters {
            let is_selected = manager.selected_subfilter.as_deref() == Some(c.as_str());
            let clr = if is_selected {
                COLOR_TEXT_PRIMARY
            } else {
                COLOR_TEXT_SECONDARY
            };
            let size = 12.0; // 13px typopgraphy
            let char_width = 16.0; // exact layout width

            if line_width + char_width > max_w {
                rows_of_chars =
                    rows_of_chars.push(current_row.align_y(iced::alignment::Vertical::Center));
                current_row = iced::widget::Row::new().spacing(0);
                line_width = 0.0;
            }

            let char_content = container(
                text(c)
                    .size(size as f32)
                    .color(clr)
                    .font(FONT_INTER_SANS_MEDIUM),
            )
            .width(Length::Fixed(16.0))
            .height(Length::Fixed(16.0))
            .center_x(Length::Fill)
            .center_y(Length::Fill);

            let btn = button(char_content)
                .width(Length::Fixed(char_width))
                .height(Length::Fixed(char_width))
                .padding(0)
                .style(move |_t: &Theme, _s: button::Status| {
                    let mut st = button::Style::default();
                    if is_selected {
                        st.background = Some(iced::Background::Color(COLOR_ACCENT));
                        st.border.radius = 18.0.into();
                    } else {
                        st.background = Some(iced::Background::Color(Color::TRANSPARENT));
                    }
                    st
                })
                .on_press(Message::SelectSubfilter(Some(c.clone())));

            current_row = current_row.push(btn);
            line_width += char_width; // no spacing
        }
        if line_width > 0.0 {
            rows_of_chars =
                rows_of_chars.push(current_row.align_y(iced::alignment::Vertical::Center));
        }

        middle_content = middle_content.push(rows_of_chars);
    }

    let mut tree_col_content = column![].spacing(0);

    if !manager.active_subfilters.is_empty() {
        let (top, bottom, visible_items) = manager.get_visible_tree_items();

        if top > 0.0 {
            tree_col_content = tree_col_content.push(Space::new().height(Length::Fixed(top)));
        }

        for item in visible_items {
            let icon = if item.is_expanded {
                "arrow-down-chevron.svg"
            } else {
                "arrow-right-chevron.svg"
            };
            let padding_left = item.depth as f32 * 12.0;

            let t_label = crate::gui::widgets::smart_truncate_text(
                item.label,
                13.0,
                FONT_INTER_SANS_MEDIUM,
                COLOR_TEXT_SECONDARY,
            );

            let row_content = if item.has_children {
                row![
                    chevron_btn(icon, Message::ToggleTreeNode(item.id.clone()), 22.0, 22.0),
                    Space::new().width(0.0),
                    t_label,
                ]
            } else {
                row![Space::new().width(16.0), Space::new().width(0.0), t_label,]
            };

            let is_active = manager.selected_tree_node.as_deref() == Some(item.id.as_str());

            let interactable = button(
                container(row_content.align_y(Alignment::Center))
                    .padding(iced::Padding {
                        top: 0.0,
                        bottom: 0.0,
                        left: padding_left,
                        right: 15.0,
                    })
                    .height(Length::Fixed(32.0))
                    .center_y(Length::Fill),
            )
            .width(Length::Fill)
            .style(move |_t: &Theme, _s: button::Status| {
                let mut st = button::Style::default();
                if is_active || _s == button::Status::Hovered {
                    st.background = Some(iced::Background::Color(COLOR_CONTRAST));
                } else {
                    st.background = Some(iced::Background::Color(Color::TRANSPARENT));
                }
                st
            })
            .on_press(Message::SelectTreeNode(item.id.clone()));

            tree_col_content = tree_col_content.push(interactable);
        }

        if bottom > 0.0 {
            tree_col_content = tree_col_content.push(Space::new().height(Length::Fixed(bottom)));
        }
    }

    let scrollable_tree = standard_scrollable(
        FILTERS_SCROLL_ID.clone(),
        tree_col_content,
        iced::widget::scrollable::Direction::Vertical(standard_scrollbar()),
    )
    .on_scroll(Message::FilterScroll);

    // --- 3. BOTTOM BAR (40px) ---
    let search_input = crate::gui::widgets::standard_search_input(
        None,
        "Buscar...",
        &manager.search_query,
        Message::FilterSearchChanged,
        Message::FilterSearchChanged(String::new()),
        Length::Fixed(180.0),
    );

    let bottom_bar = container(search_input)
        .width(Length::Fill)
        .height(Length::Fixed(40.0))
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Center)
        .style(|_t: &Theme| container::Style::default().background(COLOR_CONTRAST));

    // Container base de contenido central inferior a The Top Bar
    let base_middle = column![
        middle_content, // header and subfilters
        container(scrollable_tree)
            .width(Length::Fill)
            .height(Length::Fill),
    ];

    let main_col = column![top_bar, base_middle, bottom_bar,];

    let base_view = container(main_col)
        .width(Length::Fixed(202.0))
        .height(Length::Fill)
        .style(|_t: &Theme| container::Style::default().background(COLOR_BG));

    let overlay = if let Some(menu) = menu_options_container {
        Some(
            container(mouse_area(menu).on_press(Message::NoOp))
                .width(Length::Fixed(202.0))
                .height(Length::Fill)
                .padding(iced::Padding {
                    top: 70.0,
                    bottom: 0.0,
                    left: 0.0,
                    right: 0.0,
                })
                .align_x(iced::alignment::Horizontal::Center)
                .align_y(iced::alignment::Vertical::Top)
                .into(),
        )
    } else {
        None
    };

    (base_view.into(), overlay)
}
