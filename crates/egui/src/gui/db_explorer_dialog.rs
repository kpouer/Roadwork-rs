//! DB Explorer dialog: browse the cached services (Roadwork then Data) and,
//! in a secondary view, the raw SQLite tables, with pagination and deletion.
//! All data flows through the [`crate::db_rpc`] channel to the wasm worker that
//! owns the store.

use std::sync::{Arc, Mutex};

use egui::{Context, RichText, Ui};
use serde::Deserialize;
use wasm_bindgen::JsValue;

use crate::roadwork_app::spawn_task;

/// Mirrors `roadwork_db::ColumnInfo`.
#[derive(Debug, Clone, Deserialize)]
struct ColumnInfo {
    name: String,
    primary_key: bool,
}

/// Mirrors `roadwork_db::Cell`.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum Cell {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

impl Cell {
    fn display(&self) -> String {
        match self {
            Cell::Null => String::new(),
            Cell::Integer(i) => i.to_string(),
            Cell::Real(f) => {
                if f.fract() == 0.0 {
                    format!("{f:.1}")
                } else {
                    f.to_string()
                }
            }
            Cell::Text(s) => s.clone(),
            Cell::Blob(bytes) => String::from_utf8_lossy(bytes).into_owned(),
        }
    }
}

/// Mirrors `roadwork_db::DbTableData`.
#[derive(Debug, Clone, Deserialize)]
struct DbTableData {
    columns: Vec<ColumnInfo>,
    rows: Vec<Vec<Cell>>,
    total: i64,
}

/// Mirrors `roadwork_db::TableInfo`.
#[derive(Debug, Clone, Deserialize)]
struct TableInfo {
    name: String,
    count: i64,
}

/// Mirrors `roadwork_db::ServiceCount`.
#[derive(Debug, Clone, Deserialize)]
struct ServiceCount {
    service: String,
    count: i64,
}

/// Mirrors `roadwork_db::DbOverview`.
#[derive(Debug, Clone, Deserialize)]
struct DbOverview {
    tables: Vec<TableInfo>,
    size_bytes: i64,
    roadwork_by_service: Vec<ServiceCount>,
    opendata_by_service: Vec<ServiceCount>,
}

/// The current WME map viewport, returned by the `get_viewport_bounds` RPC.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ViewportBounds {
    lat_min: f64,
    lon_min: f64,
    lat_max: f64,
    lon_max: f64,
}

/// Fixed number of rows per page in the DB explorer.
const PAGE_SIZE: i64 = 100;

/// Columns of the `cache` table holding internal source data (the descriptor
/// blob and the enabled/visible flags). They are implementation details, hidden
/// from the raw-table view.
const HIDDEN_CACHE_COLUMNS: &[&str] = &["descriptor", "enabled", "visible"];

/// Removes the hidden `cache` columns from the given columns/rows, keeping the
/// cells aligned with the remaining columns. Other tables are returned as-is.
fn visible_columns(
    table: &str,
    columns: Vec<ColumnInfo>,
    rows: Vec<Vec<Cell>>,
) -> (Vec<ColumnInfo>, Vec<Vec<Cell>>) {
    if table != "cache" {
        return (columns, rows);
    }
    let hidden: Vec<bool> = columns
        .iter()
        .map(|column| HIDDEN_CACHE_COLUMNS.contains(&column.name.as_str()))
        .collect();
    let keep: Vec<usize> = hidden
        .iter()
        .enumerate()
        .filter_map(|(index, &is_hidden)| (!is_hidden).then_some(index))
        .collect();
    let columns = keep.iter().map(|&index| columns[index].clone()).collect();
    let rows = rows
        .into_iter()
        .map(|row| keep.iter().map(|&index| row[index].clone()).collect())
        .collect();
    (columns, rows)
}

/// What the top list shows: the cached services, or the raw SQLite tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum ViewMode {
    #[default]
    Services,
    Tables,
}

impl ViewMode {
    fn label(self) -> &'static str {
        match self {
            ViewMode::Services => "Services",
            ViewMode::Tables => "Tables brutes",
        }
    }
}

/// What the data panel currently displays.
#[derive(Debug, Clone)]
struct Selection {
    /// Underlying table to read (`roadwork`, `data` or a raw table name).
    table: String,
    /// Service filter, when browsing one service's data.
    service: Option<String>,
    /// Label shown in the toolbar.
    label: String,
}

#[derive(Default)]
struct ExplorerState {
    view_mode: ViewMode,
    tables: Vec<TableInfo>,
    roadwork_counts: Vec<ServiceCount>,
    opendata_counts: Vec<ServiceCount>,
    selection: Option<Selection>,
    columns: Vec<ColumnInfo>,
    rows: Vec<Vec<Cell>>,
    total: i64,
    page: i64,
    db_size: Option<i64>,
    loading_overview: bool,
    loading_data: bool,
    error: Option<String>,
    /// Set when the map viewport is required but unavailable.
    notice: Option<String>,
    /// When true, only the rows inside the current WME viewport are shown.
    only_visible: bool,
    needs_init: bool,
    /// Set when the selection changed outside a click (e.g. after reloading
    /// the overview) and its data still has to be loaded.
    data_needs_load: bool,
}

impl ExplorerState {
    fn page_count(&self) -> i64 {
        if self.total <= 0 {
            1
        } else {
            (self.total + PAGE_SIZE - 1) / PAGE_SIZE
        }
    }

    fn has_location(&self) -> bool {
        self.columns.iter().any(|c| c.name == "latitude")
            && self.columns.iter().any(|c| c.name == "longitude")
    }

    /// Whether `sel` still exists in the current view mode's data.
    fn selection_valid(&self, sel: &Selection) -> bool {
        match (self.view_mode, &sel.service) {
            (ViewMode::Services, Some(service)) => {
                if sel.table == "roadwork" {
                    self.roadwork_counts.iter().any(|s| &s.service == service)
                } else if sel.table == "data" {
                    self.opendata_counts.iter().any(|s| &s.service == service)
                } else {
                    false
                }
            }
            (ViewMode::Tables, None) => self.tables.iter().any(|t| t.name == sel.table),
            _ => false,
        }
    }

    /// The selection to fall back on for the current view mode, if any.
    fn default_selection(&self) -> Option<Selection> {
        match self.view_mode {
            ViewMode::Services => {
                if let Some(sc) = self.roadwork_counts.first() {
                    Some(Selection {
                        table: "roadwork".to_string(),
                        service: Some(sc.service.clone()),
                        label: format!("{} (Roadwork)", sc.service),
                    })
                } else {
                    self.opendata_counts.first().map(|sc| Selection {
                        table: "data".to_string(),
                        service: Some(sc.service.clone()),
                        label: format!("{} (Data)", sc.service),
                    })
                }
            }
            ViewMode::Tables => self.tables.first().map(|t| Selection {
                table: t.name.clone(),
                service: None,
                label: t.name.clone(),
            }),
        }
    }
}

struct PendingDelete {
    table: String,
    keys: Vec<(String, serde_json::Value)>,
    summary: String,
    /// Confirmation window message.
    title: String,
    /// Extra warning shown below the summary, if any.
    warning: Option<String>,
}

pub(crate) struct DbExplorerDialog {
    state: Arc<Mutex<ExplorerState>>,
    pending_delete: Option<PendingDelete>,
    was_open: bool,
    fullscreen_shown: bool,
    close_requested: bool,
}

impl Default for DbExplorerDialog {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(ExplorerState {
                only_visible: true,
                ..Default::default()
            })),
            pending_delete: None,
            was_open: false,
            fullscreen_shown: false,
            close_requested: false,
        }
    }
}

impl DbExplorerDialog {
    pub(crate) fn show(&mut self, ctx: &Context, open: &mut bool) {
        let state = Arc::clone(&self.state);
        {
            let mut st = state.lock().unwrap();
            if *open && !self.was_open {
                st.needs_init = true;
            }
        }
        self.was_open = *open;
        let needs_init = {
            let mut st = state.lock().unwrap();
            let init = st.needs_init;
            st.needs_init = false;
            init
        };
        if needs_init && *open {
            self.load_overview(ctx);
        }

        let screen = ctx.content_rect().size();
        let default_size = egui::vec2(
            (screen.x * 0.7).clamp(480.0, 1000.0),
            (screen.y * 0.7).clamp(360.0, 800.0),
        );

        egui::Window::new("DB Explorer")
            .id(egui::Id::new("db_explorer_window"))
            .open(open)
            .resizable(true)
            .default_size(default_size)
            .show(ctx, |ui| {
                self.show_content(ui, ctx, false);
            });

        if self.pending_delete.is_some() {
            self.show_delete_confirm(ctx);
        }
    }

    /// Renders the explorer filling the whole available area, used when the app
    /// is opened in DB-explorer-only mode (no overlay header, no map).
    pub(crate) fn show_fullscreen(&mut self, ui: &mut Ui) {
        if !self.fullscreen_shown {
            self.fullscreen_shown = true;
            self.state.lock().unwrap().needs_init = true;
        }
        let needs_init = {
            let mut st = self.state.lock().unwrap();
            let init = st.needs_init;
            st.needs_init = false;
            init
        };
        let ctx = ui.ctx().clone();
        if needs_init {
            self.load_overview(&ctx);
        }
        self.show_content(ui, &ctx, true);
        if self.pending_delete.is_some() {
            self.show_delete_confirm(&ctx);
        }
        if self.close_requested {
            self.close_requested = false;
            post_close_app();
        }
    }

    fn show_content(&mut self, ui: &mut Ui, ctx: &Context, show_close: bool) {
        self.show_toolbar(ui, ctx, show_close);
        ui.separator();
        let top_height = (ui.available_height() * 0.45).clamp(110.0, 320.0);
        egui::Panel::top("db_explorer_top_panel")
            .resizable(true)
            .min_size(100.0)
            .default_size(top_height)
            .show_inside(ui, |ui| {
                self.show_top_panel(ui, ctx);
            });
        egui::CentralPanel::default().show_inside(ui, |ui| {
            self.show_data_panel(ui, ctx);
        });

        let (selection, needs_load) = {
            let mut st = self.state.lock().unwrap();
            let needs_load = st.data_needs_load;
            if needs_load {
                st.data_needs_load = false;
            }
            (st.selection.clone(), needs_load)
        };
        if needs_load && let Some(sel) = selection {
            self.load_data(ctx, &sel);
        }
    }

    fn show_toolbar(&mut self, ui: &mut Ui, ctx: &Context, show_close: bool) {
        let mut refresh = false;
        let mut mode_changed = false;
        ui.horizontal(|ui| {
            let mut st = self.state.lock().unwrap();
            if let Some(sel) = &st.selection {
                ui.label(RichText::new(sel.label.clone()).strong());
            } else {
                ui.label("Aucune source sélectionnée");
            }
            ui.separator();
            let mut mode = st.view_mode;
            egui::ComboBox::from_id_salt("db_explorer_view_mode")
                .selected_text(mode.label())
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut mode, ViewMode::Services, "Services");
                    ui.selectable_value(&mut mode, ViewMode::Tables, "Tables brutes");
                });
            if mode != st.view_mode {
                st.view_mode = mode;
                st.page = 0;
                st.columns = Vec::new();
                st.rows = Vec::new();
                st.total = 0;
                st.notice = None;
                st.error = None;
                st.selection = st.default_selection();
                mode_changed = true;
            }
            if ui.button("Rafraîchir").clicked() {
                refresh = true;
            }
            if let Some(error) = &st.error {
                ui.separator();
                ui.colored_label(egui::Color32::RED, error);
            }
            if let Some(size) = st.db_size {
                ui.separator();
                ui.label(format!("Base : {}", format_bytes(size)));
            }
            if st.loading_overview || st.loading_data {
                ui.spinner();
            }
            if show_close {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Fermer").clicked() {
                        self.close_requested = true;
                    }
                });
            }
        });
        if mode_changed {
            let selection = { self.state.lock().unwrap().selection.clone() };
            if let Some(sel) = selection {
                self.load_data(ctx, &sel);
            }
        }
        if refresh {
            let selection = { self.state.lock().unwrap().selection.clone() };
            self.load_overview(ctx);
            if let Some(sel) = selection {
                self.load_data(ctx, &sel);
            }
        }
    }

    fn show_top_panel(&mut self, ui: &mut Ui, ctx: &Context) {
        let mut pending_select: Option<Selection> = None;
        let mut pending_delete_source: Option<(String, String)> = None;
        {
            let st = self.state.lock().unwrap();
            if st.loading_overview {
                ui.centered_and_justified(|ui| {
                    ui.spinner();
                });
            } else {
                let empty = match st.view_mode {
                    ViewMode::Services => {
                        st.roadwork_counts.is_empty() && st.opendata_counts.is_empty()
                    }
                    ViewMode::Tables => st.tables.is_empty(),
                };
                if empty {
                    ui.centered_and_justified(|ui| {
                        ui.label("Aucune donnée");
                    });
                } else {
                    match st.view_mode {
                        ViewMode::Services => Self::services_table(
                            ui,
                            &st,
                            &mut pending_select,
                            &mut pending_delete_source,
                        ),
                        ViewMode::Tables => Self::tables_list(ui, &st, &mut pending_select),
                    }
                }
            }
        }
        if let Some(sel) = pending_select {
            {
                let mut st = self.state.lock().unwrap();
                st.selection = Some(sel.clone());
                st.page = 0;
                st.error = None;
                st.notice = None;
            }
            self.load_data(ctx, &sel);
        }
        if let Some((service, cache_type)) = pending_delete_source {
            self.begin_delete_source(&service, &cache_type);
        }
    }

    /// Renders the services list (Roadwork group, then Data group) as a table.
    fn services_table(
        ui: &mut Ui,
        st: &ExplorerState,
        pending_select: &mut Option<Selection>,
        pending_delete_source: &mut Option<(String, String)>,
    ) {
        let selected = st.selection.clone();
        let roadwork = st.roadwork_counts.clone();
        let opendata = st.opendata_counts.clone();
        egui_extras::TableBuilder::new(ui)
            .striped(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(
                egui_extras::Column::initial(220.0)
                    .at_least(120.0)
                    .clip(true)
                    .resizable(true),
            )
            .column(
                egui_extras::Column::initial(60.0)
                    .at_least(40.0)
                    .resizable(true),
            )
            .column(
                egui_extras::Column::initial(90.0)
                    .at_least(70.0)
                    .resizable(true),
            )
            .header(22.0, |mut header| {
                header.col(|ui| {
                    ui.strong("Source");
                });
                header.col(|ui| {
                    ui.strong("Nb");
                });
                header.col(|_ui| {});
            })
            .body(|mut body| {
                Self::services_group(
                    &mut body,
                    "Roadwork",
                    &roadwork,
                    "roadwork",
                    "roadwork",
                    &selected,
                    pending_select,
                    pending_delete_source,
                );
                Self::services_group(
                    &mut body,
                    "Data",
                    &opendata,
                    "opendata",
                    "data",
                    &selected,
                    pending_select,
                    pending_delete_source,
                );
            });
    }

    #[allow(clippy::too_many_arguments)]
    fn services_group(
        body: &mut egui_extras::TableBody<'_>,
        title: &str,
        counts: &[ServiceCount],
        cache_type: &str,
        table: &str,
        selected: &Option<Selection>,
        pending_select: &mut Option<Selection>,
        pending_delete_source: &mut Option<(String, String)>,
    ) {
        body.row(22.0, |mut row| {
            row.set_selected(true);
            row.col(|ui| {
                ui.strong(title);
            });
            row.col(|_ui| {});
            row.col(|_ui| {});
        });
        if counts.is_empty() {
            body.row(22.0, |mut row| {
                row.col(|ui| {
                    ui.weak("Aucun");
                });
                row.col(|_ui| {});
                row.col(|_ui| {});
            });
            return;
        }
        for sc in counts {
            body.row(22.0, |mut row| {
                let is_selected = selected.as_ref().is_some_and(|s| {
                    s.service.as_deref() == Some(sc.service.as_str()) && s.table == table
                });
                row.col(|ui| {
                    if ui.selectable_label(is_selected, &sc.service).clicked() {
                        *pending_select = Some(Selection {
                            table: table.to_string(),
                            service: Some(sc.service.clone()),
                            label: format!("{} ({title})", sc.service),
                        });
                    }
                });
                row.col(|ui| {
                    ui.label(sc.count.to_string());
                });
                row.col(|ui| {
                    if ui.small_button("Supprimer").clicked() {
                        *pending_delete_source = Some((sc.service.clone(), cache_type.to_string()));
                    }
                });
            });
        }
    }

    /// Renders the raw tables list.
    fn tables_list(ui: &mut Ui, st: &ExplorerState, pending_select: &mut Option<Selection>) {
        let selected = st.selection.clone();
        let tables = st.tables.clone();
        egui_extras::TableBuilder::new(ui)
            .striped(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(
                egui_extras::Column::initial(220.0)
                    .at_least(120.0)
                    .clip(true)
                    .resizable(true),
            )
            .column(
                egui_extras::Column::initial(60.0)
                    .at_least(40.0)
                    .resizable(true),
            )
            .header(22.0, |mut header| {
                header.col(|ui| {
                    ui.strong("Table");
                });
                header.col(|ui| {
                    ui.strong("Nb");
                });
            })
            .body(|body| {
                body.rows(22.0, tables.len(), |mut row| {
                    let index = row.index();
                    let t = &tables[index];
                    let is_selected = selected
                        .as_ref()
                        .is_some_and(|s| s.table == t.name && s.service.is_none());
                    row.col(|ui| {
                        if ui.selectable_label(is_selected, &t.name).clicked() {
                            *pending_select = Some(Selection {
                                table: t.name.clone(),
                                service: None,
                                label: t.name.clone(),
                            });
                        }
                    });
                    row.col(|ui| {
                        ui.label(t.count.to_string());
                    });
                });
            });
    }

    fn show_data_panel(&mut self, ui: &mut Ui, ctx: &Context) {
        enum Action {
            Prev,
            Next,
        }
        let mut action: Option<Action> = None;
        let mut only_visible_toggled = false;
        {
            let mut st = self.state.lock().unwrap();
            if st.loading_data {
                ui.centered_and_justified(|ui| {
                    ui.label("Chargement…");
                });
                return;
            }
            let has_location = st.has_location();
            let enable_checkbox = has_location || st.notice.is_some();
            ui.horizontal(|ui| {
                ui.label(format!("{} ligne(s)", st.total));
                ui.separator();
                let page_count = st.page_count();
                if st.page > 0 && ui.button("‹ Précédent").clicked() {
                    action = Some(Action::Prev);
                }
                ui.label(format!("Page {} / {}", st.page + 1, page_count));
                if st.page + 1 < page_count && ui.button("Suivant ›").clicked() {
                    action = Some(Action::Next);
                }
                ui.separator();
                let mut only_visible = st.only_visible;
                let response = ui.add_enabled(
                    enable_checkbox,
                    egui::Checkbox::new(&mut only_visible, "Données visibles à l'écran"),
                );
                if !has_location && st.notice.is_none() {
                    response.clone().on_hover_text(
                        "Cette table n'a pas de coordonnées : toutes les lignes sont affichées.",
                    );
                }
                if response.changed() {
                    st.only_visible = only_visible;
                    st.page = 0;
                    only_visible_toggled = true;
                }
            });
            if let Some(notice) = &st.notice {
                ui.add_space(2.0);
                ui.colored_label(egui::Color32::from_rgb(220, 120, 0), notice);
            }
            ui.separator();
            if st.columns.is_empty() && !only_visible_toggled && st.notice.is_none() {
                if let Some(error) = &st.error {
                    ui.colored_label(egui::Color32::RED, error);
                } else {
                    let message = match st.view_mode {
                        ViewMode::Services => "Sélectionnez un service pour voir ses données.",
                        ViewMode::Tables => "Sélectionnez une table pour voir ses données.",
                    };
                    ui.label(message);
                }
            }
        }
        if action.is_some() || only_visible_toggled {
            match action {
                Some(Action::Prev) => {
                    self.state.lock().unwrap().page -= 1;
                }
                Some(Action::Next) => {
                    self.state.lock().unwrap().page += 1;
                }
                None => {}
            }
            let selection = { self.state.lock().unwrap().selection.clone() };
            if let Some(sel) = selection {
                self.load_data(ctx, &sel);
            }
            return;
        }

        let (selection, columns, rows) = {
            let st = self.state.lock().unwrap();
            (st.selection.clone(), st.columns.clone(), st.rows.clone())
        };
        let Some(selection) = selection else {
            return;
        };
        if columns.is_empty() {
            return;
        }
        // Row-level deletion only makes sense for raw tables, not for the
        // cached data of a service (sources are deleted from the top list).
        let show_delete = selection.service.is_none();

        let mut table = egui_extras::TableBuilder::new(ui)
            .striped(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center));
        if show_delete {
            table = table.column(egui_extras::Column::initial(90.0).clip(true));
        }
        table = table.column(
            egui_extras::Column::initial(180.0)
                .at_least(80.0)
                .clip(true)
                .resizable(true),
        );
        for _ in 1..columns.len() {
            table = table.column(
                egui_extras::Column::initial(160.0)
                    .at_least(80.0)
                    .clip(true)
                    .resizable(true),
            );
        }
        let table_name = selection.table.clone();
        table
            .header(24.0, |mut header| {
                if show_delete {
                    header.col(|ui| {
                        ui.strong("Actions");
                    });
                }
                for column in &columns {
                    header.col(|ui| {
                        if column.primary_key {
                            ui.strong(format!("{} (PK)", column.name));
                        } else {
                            ui.strong(&column.name);
                        }
                    });
                }
            })
            .body(|body| {
                body.rows(18.0, rows.len(), |mut row| {
                    let index = row.index();
                    if show_delete {
                        row.col(|ui| {
                            if ui.small_button("Supprimer").clicked() {
                                self.begin_delete(&table_name, &columns, &rows[index]);
                            }
                        });
                    }
                    for cell in &rows[index] {
                        row.col(|ui| {
                            ui.add(egui::Label::new(truncate(&cell.display(), 120)).truncate());
                        });
                    }
                });
            });
    }

    fn show_delete_confirm(&mut self, ctx: &Context) {
        let Some(pending) = &self.pending_delete else {
            return;
        };
        let title = pending.title.clone();
        let summary = pending.summary.clone();
        let warning = pending.warning.clone();
        let mut confirmed = false;
        let mut cancelled = false;
        egui::Window::new("Confirmer la suppression")
            .id(egui::Id::new("db_explorer_delete_confirm"))
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(title);
                ui.label(summary);
                if let Some(warning) = &warning {
                    ui.add_space(4.0);
                    ui.colored_label(egui::Color32::from_rgb(220, 120, 0), warning);
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Annuler").clicked() {
                        cancelled = true;
                    }
                    if ui.button("Supprimer").clicked() {
                        confirmed = true;
                    }
                });
            });
        if confirmed {
            let pending = self.pending_delete.take().unwrap();
            self.confirm_delete(pending, ctx);
        } else if cancelled {
            self.pending_delete = None;
        }
    }

    fn begin_delete(&mut self, table: &str, columns: &[ColumnInfo], row: &[Cell]) {
        let keys: Vec<(String, serde_json::Value)> = columns
            .iter()
            .enumerate()
            .filter(|(_, column)| column.primary_key)
            .map(|(i, column)| {
                let value = match &row[i] {
                    Cell::Null => serde_json::Value::Null,
                    Cell::Integer(v) => serde_json::json!(v),
                    Cell::Real(v) => serde_json::json!(v),
                    Cell::Text(s) => serde_json::json!(s),
                    Cell::Blob(_) => serde_json::Value::Null,
                };
                (column.name.clone(), value)
            })
            .collect();
        let summary = keys
            .iter()
            .map(|(key, value)| format!("{key}: {value}"))
            .collect::<Vec<_>>()
            .join(", ");
        let warning = if table == "cache" {
            Some(
                "Les données liées de ce service (table roadwork ou data) seront aussi supprimées."
                    .to_string(),
            )
        } else {
            None
        };
        self.pending_delete = Some(PendingDelete {
            table: table.to_string(),
            keys,
            summary,
            title: format!("Supprimer la ligne de la table \"{table}\" ?"),
            warning,
        });
    }

    /// Asks to delete a whole source: the `cache` row plus its linked data.
    fn begin_delete_source(&mut self, service: &str, cache_type: &str) {
        let kind = match cache_type {
            "roadwork" => "Roadwork",
            _ => "Data",
        };
        let keys: Vec<(String, serde_json::Value)> = vec![
            ("service".to_string(), serde_json::json!(service)),
            ("type".to_string(), serde_json::json!(cache_type)),
        ];
        self.pending_delete = Some(PendingDelete {
            table: "cache".to_string(),
            keys,
            summary: format!("{service} ({kind})"),
            title: format!("Supprimer la source « {service} » ?"),
            warning: Some(
                "Les données liées de ce service (table roadwork ou data) seront aussi supprimées."
                    .to_string(),
            ),
        });
    }

    fn confirm_delete(&mut self, pending: PendingDelete, ctx: &Context) {
        let keys_json = serde_json::to_string(&pending.keys).unwrap_or_else(|_| "[]".to_string());
        let state = Arc::clone(&self.state);
        let table = pending.table.clone();
        let ctx_clone = ctx.clone();
        spawn_task(async move {
            let result = crate::db_rpc::call(
                "delete_db_row",
                vec![JsValue::from_str(&table), JsValue::from_str(&keys_json)],
            )
            .await;
            {
                let mut st = state.lock().unwrap();
                match result {
                    Ok(_) => {
                        st.error = None;
                    }
                    Err(e) => {
                        st.error = Some(
                            e.as_string()
                                .unwrap_or_else(|| "Erreur de suppression".to_string()),
                        );
                    }
                }
            }
            ctx_clone.request_repaint();
        });
        self.load_overview(ctx);
    }

    fn load_overview(&mut self, ctx: &Context) {
        {
            let mut st = self.state.lock().unwrap();
            st.loading_overview = true;
            st.error = None;
        }
        let state = Arc::clone(&self.state);
        let ctx = ctx.clone();
        spawn_task(async move {
            let result = crate::db_rpc::rpc_json::<DbOverview>("get_db_overview", vec![]).await;
            {
                let mut st = state.lock().unwrap();
                st.loading_overview = false;
                match result {
                    Ok(overview) => {
                        st.tables = overview.tables;
                        st.db_size = Some(overview.size_bytes);
                        st.roadwork_counts = overview.roadwork_by_service;
                        st.opendata_counts = overview.opendata_by_service;
                        let keep = st.selection.clone().filter(|sel| st.selection_valid(sel));
                        st.selection = keep.or_else(|| st.default_selection());
                        st.data_needs_load = st.selection.is_some();
                    }
                    Err(e) => {
                        st.error = Some(e);
                    }
                }
            }
            ctx.request_repaint();
        });
    }

    fn load_data(&mut self, ctx: &Context, selection: &Selection) {
        {
            let mut st = self.state.lock().unwrap();
            st.loading_data = true;
            st.error = None;
            st.notice = None;
        }
        let state = Arc::clone(&self.state);
        let ctx = ctx.clone();
        let table = selection.table.clone();
        let service = selection.service.clone();
        spawn_task(async move {
            let (offset, only_visible) = {
                let st = state.lock().unwrap();
                (st.page * PAGE_SIZE, st.only_visible)
            };
            let apply = |result: Result<DbTableData, String>| {
                let mut st = state.lock().unwrap();
                st.loading_data = false;
                match result {
                    Ok(data) => {
                        if data.rows.is_empty() && data.total > 0 && st.page > 0 {
                            st.page -= 1;
                        }
                        let (columns, rows) = visible_columns(&table, data.columns, data.rows);
                        st.columns = columns;
                        st.rows = rows;
                        st.total = data.total;
                        st.error = None;
                        st.notice = None;
                    }
                    Err(e) => {
                        st.error = Some(e);
                    }
                }
                ctx.request_repaint();
            };
            let service_arg = match &service {
                Some(s) => JsValue::from_str(s),
                None => JsValue::NULL,
            };
            let args = if only_visible {
                let bounds = crate::db_rpc::rpc_json::<Option<ViewportBounds>>(
                    "get_viewport_bounds",
                    vec![],
                )
                .await;
                let Some(bounds) = bounds.ok().flatten() else {
                    let mut st = state.lock().unwrap();
                    st.loading_data = false;
                    st.rows = Vec::new();
                    st.total = 0;
                    st.notice = Some(
                        "Emprise de la carte WME indisponible : déplacez la carte puis cliquez \
                         sur Rafraîchir, ou décochez la case pour tout afficher."
                            .to_string(),
                    );
                    ctx.request_repaint();
                    return;
                };
                vec![
                    JsValue::from_str(&table),
                    JsValue::from_f64(offset as f64),
                    JsValue::from_f64(PAGE_SIZE as f64),
                    JsValue::from_f64(bounds.lat_min),
                    JsValue::from_f64(bounds.lon_min),
                    JsValue::from_f64(bounds.lat_max),
                    JsValue::from_f64(bounds.lon_max),
                    service_arg,
                ]
            } else {
                vec![
                    JsValue::from_str(&table),
                    JsValue::from_f64(offset as f64),
                    JsValue::from_f64(PAGE_SIZE as f64),
                    JsValue::NULL,
                    JsValue::NULL,
                    JsValue::NULL,
                    JsValue::NULL,
                    service_arg,
                ]
            };
            let result = crate::db_rpc::rpc_json::<DbTableData>("get_db_table", args).await;
            apply(result);
        });
    }
}

/// Asks the extension content script to close the app overlay.
fn post_close_app() {
    let obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(
        &obj,
        &js_sys::JsString::from("type"),
        &js_sys::JsString::from("ROADWORK_CLOSE_APP"),
    );
    if let Some(parent) = web_sys::window().and_then(|w| w.parent().ok().flatten()) {
        let _ = parent.post_message(&obj, "*");
    }
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let prefix: String = s.chars().take(max_chars).collect();
    format!("{prefix}…")
}

/// Formats a byte count in French units (o, Ko, Mo).
fn format_bytes(bytes: i64) -> String {
    let bytes = bytes.max(0) as f64;
    const KIB: f64 = 1024.0;
    if bytes < KIB {
        format!("{bytes:.0} o")
    } else if bytes < KIB * KIB {
        format!("{:.1} Ko", bytes / KIB)
    } else {
        format!("{:.1} Mo", bytes / (KIB * KIB))
    }
}
