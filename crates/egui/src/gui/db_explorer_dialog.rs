//! DB Explorer dialog: browse the extension's SQLite tables with pagination and
//! row deletion. All data flows through the [`crate::db_rpc`] channel to the
//! wasm worker that owns the store.

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

#[derive(Default)]
struct ExplorerState {
    tables: Vec<String>,
    selected: Option<String>,
    columns: Vec<ColumnInfo>,
    rows: Vec<Vec<Cell>>,
    total: i64,
    page: i64,
    page_size: i64,
    loading_tables: bool,
    loading_data: bool,
    error: Option<String>,
    needs_init: bool,
}

impl ExplorerState {
    fn page_count(&self) -> i64 {
        if self.total <= 0 {
            1
        } else {
            (self.total + self.page_size - 1) / self.page_size
        }
    }
}

struct PendingDelete {
    table: String,
    keys: Vec<(String, serde_json::Value)>,
    summary: String,
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
                page: 0,
                page_size: 50,
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
            self.load_tables(ctx);
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
            self.load_tables(&ctx);
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
        egui::Panel::left("db_explorer_tables_panel")
            .resizable(true)
            .default_size(180.0)
            .show_inside(ui, |ui| {
                self.show_tables_panel(ui, ctx);
            });
        egui::CentralPanel::default().show_inside(ui, |ui| {
            self.show_data_panel(ui, ctx);
        });
    }

    fn show_toolbar(&mut self, ui: &mut Ui, ctx: &Context, show_close: bool) {
        let mut refresh = false;
        ui.horizontal(|ui| {
            let st = self.state.lock().unwrap();
            if let Some(table) = &st.selected {
                ui.label(RichText::new(format!("Table: {table}")).strong());
            } else {
                ui.label("Aucune table sélectionnée");
            }
            if ui.button("Rafraîchir").clicked() {
                refresh = true;
            }
            if let Some(error) = &st.error {
                ui.separator();
                ui.colored_label(egui::Color32::RED, error);
            }
            if st.loading_tables || st.loading_data {
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
        if refresh {
            let selected = { self.state.lock().unwrap().selected.clone() };
            self.load_tables(ctx);
            if let Some(table) = selected {
                self.load_data(ctx, &table);
            }
        }
    }

    fn show_tables_panel(&mut self, ui: &mut Ui, ctx: &Context) {
        ui.add_space(4.0);
        let mut pending_select: Option<String> = None;
        {
            let mut st = self.state.lock().unwrap();
            if st.tables.is_empty() {
                if st.loading_tables {
                    ui.label("Chargement…");
                } else {
                    ui.label("Aucune table");
                }
                return;
            }
            egui::ScrollArea::vertical()
                .id_salt("db_explorer_tables_scroll")
                .show(ui, |ui| {
                    for table in st.tables.clone() {
                        let selected = st.selected.as_deref() == Some(table.as_str());
                        if ui.selectable_label(selected, &table).clicked() {
                            st.selected = Some(table.clone());
                            st.page = 0;
                            st.error = None;
                            pending_select = Some(table);
                        }
                    }
                });
        }
        if let Some(table) = pending_select {
            self.load_data(ctx, &table);
        }
    }

    fn show_data_panel(&mut self, ui: &mut Ui, ctx: &Context) {
        enum Action {
            Prev,
            Next,
            Resize(i64),
        }
        let mut action: Option<Action> = None;
        {
            let st = self.state.lock().unwrap();
            if st.loading_data {
                ui.centered_and_justified(|ui| {
                    ui.label("Chargement…");
                });
                return;
            }
            if st.columns.is_empty() {
                if let Some(error) = &st.error {
                    ui.colored_label(egui::Color32::RED, error);
                } else {
                    ui.label("Sélectionnez une table pour voir ses données.");
                }
                return;
            }
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
                ui.label("Taille:");
                let page_size = st.page_size;
                let new_size = egui::ComboBox::from_id_salt("db_explorer_page_size")
                    .selected_text(format!("{page_size}"))
                    .show_ui(ui, |ui| {
                        let mut chosen = page_size;
                        for size in [10i64, 25, 50, 100, 500] {
                            if ui
                                .selectable_value(&mut chosen, size, size.to_string())
                                .clicked()
                            {
                                break;
                            }
                        }
                        chosen
                    })
                    .inner
                    .unwrap_or(page_size);
                if new_size != page_size {
                    action = Some(Action::Resize(new_size));
                }
            });
            ui.separator();
        }
        match action {
            Some(Action::Prev) => {
                self.state.lock().unwrap().page -= 1;
            }
            Some(Action::Next) => {
                self.state.lock().unwrap().page += 1;
            }
            Some(Action::Resize(size)) => {
                let mut st = self.state.lock().unwrap();
                st.page_size = size;
                st.page = 0;
            }
            None => {}
        }
        if action.is_some() {
            let table_name = { self.state.lock().unwrap().selected.clone() };
            if let Some(table) = table_name {
                self.load_data(ctx, &table);
            }
            return;
        }

        let (table_name, columns, rows) = {
            let st = self.state.lock().unwrap();
            (st.selected.clone(), st.columns.clone(), st.rows.clone())
        };
        let Some(table_name) = table_name else {
            return;
        };

        let mut table = egui_extras::TableBuilder::new(ui)
            .striped(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(egui_extras::Column::initial(90.0).clip(true))
            .column(
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
        table
            .header(24.0, |mut header| {
                header.col(|ui| {
                    ui.strong("Actions");
                });
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
                    row.col(|ui| {
                        if ui.small_button("Supprimer").clicked() {
                            self.begin_delete(&table_name, &columns, &rows[index]);
                        }
                    });
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
        let is_cache = pending.table == "cache";
        let mut confirmed = false;
        let mut cancelled = false;
        egui::Window::new("Confirmer la suppression")
            .id(egui::Id::new("db_explorer_delete_confirm"))
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(format!(
                    "Supprimer la ligne de la table \"{}\" ?",
                    pending.table
                ));
                ui.label(pending.summary.clone());
                if is_cache {
                    ui.add_space(4.0);
                    ui.colored_label(
                        egui::Color32::from_rgb(220, 120, 0),
                        "Les données liées de ce service (table roadwork ou data) seront aussi supprimées.",
                    );
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
        self.pending_delete = Some(PendingDelete {
            table: table.to_string(),
            keys,
            summary,
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
        let table_name = { self.state.lock().unwrap().selected.clone() };
        if let Some(table) = table_name {
            self.load_data(ctx, &table);
        }
    }

    fn load_tables(&mut self, ctx: &Context) {
        {
            let mut st = self.state.lock().unwrap();
            st.loading_tables = true;
            st.error = None;
        }
        let state = Arc::clone(&self.state);
        let ctx = ctx.clone();
        spawn_task(async move {
            let result = rpc_json::<Vec<String>>("get_db_tables", vec![]).await;
            {
                let mut st = state.lock().unwrap();
                st.loading_tables = false;
                match result {
                    Ok(tables) => {
                        st.tables = tables;
                        let keep = st
                            .selected
                            .as_ref()
                            .filter(|s| st.tables.iter().any(|t| t == *s))
                            .cloned();
                        st.selected = keep.or_else(|| st.tables.first().cloned());
                    }
                    Err(e) => {
                        st.error = Some(e);
                    }
                }
            }
            ctx.request_repaint();
        });
    }

    fn load_data(&mut self, ctx: &Context, table: &str) {
        {
            let mut st = self.state.lock().unwrap();
            st.loading_data = true;
            st.error = None;
        }
        let state = Arc::clone(&self.state);
        let ctx = ctx.clone();
        let table = table.to_string();
        spawn_task(async move {
            let (offset, limit) = {
                let st = state.lock().unwrap();
                (st.page * st.page_size, st.page_size)
            };
            let result = rpc_json::<DbTableData>(
                "get_db_table",
                vec![
                    JsValue::from_str(&table),
                    JsValue::from_f64(offset as f64),
                    JsValue::from_f64(limit as f64),
                ],
            )
            .await;
            {
                let mut st = state.lock().unwrap();
                st.loading_data = false;
                match result {
                    Ok(data) => {
                        if data.rows.is_empty() && data.total > 0 && st.page > 0 {
                            st.page -= 1;
                        }
                        st.columns = data.columns;
                        st.rows = data.rows;
                        st.total = data.total;
                        st.error = None;
                    }
                    Err(e) => {
                        st.error = Some(e);
                    }
                }
            }
            ctx.request_repaint();
        });
    }
}

/// Calls an RPC and deserializes the result through `JSON.stringify`.
async fn rpc_json<T>(method: &str, args: Vec<JsValue>) -> Result<T, String>
where
    T: serde::de::DeserializeOwned,
{
    let value = crate::db_rpc::call(method, args).await.map_err(|e| {
        e.as_string()
            .unwrap_or_else(|| format!("Erreur RPC: {e:?}"))
    })?;
    let json =
        js_sys::JSON::stringify(&value).map_err(|e| format!("Erreur de sérialisation: {e:?}"))?;
    let s = json.as_string().unwrap_or_default();
    serde_json::from_str(&s).map_err(|e| format!("Erreur de décodage: {e}"))
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
