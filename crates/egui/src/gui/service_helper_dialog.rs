use egui::{Context, RichText, Ui};
use roadwork_core::opendata::json::model::service_descriptor::ServiceDescriptor;
use roadwork_core::opendata::json::opendata_service::OpendataService;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub(crate) struct ServiceHelperDialog {
    service: String,
    descriptor: Option<ServiceDescriptor>,
    descriptor_json: String,
    form_mode: bool,
    url_params: Vec<(String, String)>,
    url: String,
    raw_json: String,
    result_json: String,
    error: Option<String>,
    dirty: bool,
    pending_fetch: Arc<Mutex<Option<Result<String, String>>>>,
}

impl ServiceHelperDialog {
    pub(crate) fn new(service: &str) -> Self {
        let mut dialog = Self {
            service: service.to_string(),
            descriptor: None,
            descriptor_json: String::new(),
            form_mode: true,
            url_params: Vec::new(),
            url: String::new(),
            raw_json: String::new(),
            result_json: String::new(),
            error: None,
            dirty: false,
            pending_fetch: Arc::new(Mutex::new(None)),
        };
        dialog.reload();
        dialog
    }

    pub(crate) fn show(&mut self, ctx: &Context, open: &mut bool, service: &str) {
        self.service = service.to_string();
        if let Some(result) = self.pending_fetch.lock().unwrap().take() {
            match result {
                Ok(text) => {
                    self.raw_json = text;
                    self.error = None;
                    self.dirty = true;
                }
                Err(e) => {
                    self.error = Some(e.clone());
                    self.result_json = format!("Fetch error: {e}");
                }
            }
        }
        self.recompute_if_dirty();

        let screen = ctx.content_rect().size();
        let max = egui::vec2(screen.x * 0.98, screen.y * 0.98);
        egui::Window::new("Service helper")
            .open(open)
            .resizable(true)
            .default_size([900.0, 600.0])
            .max_size(max)
            .show(ctx, |ui| {
                self.show_content(ui);
            });
    }

    fn show_content(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.form_mode, "Form mode");
            ui.separator();
            if ui.button("Reload").clicked() {
                self.reload();
            }
        });
        ui.separator();
        let available_height = ui.available_height();
        egui::Panel::left("helper_left")
            .resizable(true)
            .default_size(460.0)
            .size_range(280.0..=1200.0)
            .show_inside(ui, |ui| {
                if self.form_mode {
                    ui.label(RichText::new("Service descriptor (form)").strong());
                    self.show_form(ui, available_height);
                } else {
                    ui.label(RichText::new("Service descriptor (JSON)").strong());
                    egui::ScrollArea::vertical()
                        .id_salt("descriptor_scroll")
                        .max_height(available_height - 24.0)
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut self.descriptor_json)
                                    .code_editor()
                                    .interactive(false)
                                    .desired_width(f32::INFINITY)
                                    .desired_rows(30),
                            );
                        });
                }
            });
        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.label(RichText::new("URL").strong());
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.url)
                        .hint_text("https://...")
                        .desired_width(f32::INFINITY),
                );
                if ui.button("Fetch").clicked() {
                    self.fetch(ui.ctx().clone());
                }
            });
            if let Some(error) = &self.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }

            ui.label(RichText::new("Fetched JSON").strong());
            egui::ScrollArea::vertical()
                .id_salt("raw_json_scroll")
                .max_height((available_height - 100.0) * 0.55)
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut self.raw_json)
                            .code_editor()
                            .interactive(false)
                            .desired_width(f32::INFINITY)
                            .desired_rows(15),
                    );
                });

            ui.label(RichText::new("Result JSON").strong());
            egui::ScrollArea::vertical()
                .id_salt("result_json_scroll")
                .max_height((available_height - 100.0) * 0.4)
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut self.result_json)
                            .code_editor()
                            .interactive(false)
                            .desired_width(f32::INFINITY)
                            .desired_rows(15),
                    );
                });
        });
    }

    fn show_form(&mut self, ui: &mut Ui, available_height: f32) {
        let Self {
            descriptor,
            descriptor_json,
            dirty,
            url_params,
            ..
        } = self;
        egui::ScrollArea::vertical()
            .id_salt("descriptor_form_scroll")
            .max_height(available_height - 24.0)
            .show(ui, |ui| match descriptor {
                Some(descriptor) => {
                    let changed = crate::gui::service_helper_form::show(ui, descriptor, url_params);
                    if changed {
                        let params: HashMap<String, String> = url_params
                            .iter()
                            .filter(|(key, _)| !key.trim().is_empty())
                            .map(|(key, value)| (key.trim().to_string(), value.clone()))
                            .collect();
                        descriptor.metadata.url_params = if params.is_empty() {
                            None
                        } else {
                            Some(params)
                        };
                        *dirty = true;
                        *descriptor_json =
                            serde_json::to_string_pretty(descriptor).unwrap_or_default();
                    }
                }
                None => {
                    ui.colored_label(ui.visuals().error_fg_color, "No descriptor available");
                }
            });
    }

    fn reload(&mut self) {
        self.descriptor = roadwork_service::get_descriptor(&self.service);
        self.descriptor_json = match &self.descriptor {
            Some(descriptor) => {
                self.url = descriptor.metadata.url.clone();
                self.url_params = url_params_to_vec(&descriptor.metadata.url_params);
                serde_json::to_string_pretty(descriptor).unwrap_or_default()
            }
            None => String::new(),
        };
        self.dirty = true;
    }

    fn fetch(&mut self, ctx: Context) {
        let url = self.url.clone();
        let pending_fetch = Arc::clone(&self.pending_fetch);
        crate::roadwork_app::spawn_task(async move {
            let result = match reqwest::get(&url).await {
                Ok(response) => match response.text().await {
                    Ok(text) => Ok(text),
                    Err(e) => Err(e.to_string()),
                },
                Err(e) => Err(e.to_string()),
            };
            *pending_fetch.lock().unwrap() = Some(result);
            ctx.request_repaint();
        });
    }

    fn recompute_if_dirty(&mut self) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        if self.raw_json.is_empty() {
            self.result_json.clear();
            return;
        }
        match &self.descriptor {
            Some(descriptor) => {
                let ods = OpendataService::new("Service helper".to_string(), descriptor.clone());
                match ods.parse_json(&self.raw_json) {
                    Ok(data) => {
                        self.result_json = serde_json::to_string_pretty(&data).unwrap_or_default();
                    }
                    Err(e) => self.result_json = format!("Parse error: {e}"),
                }
            }
            None => self.result_json = "No descriptor".to_string(),
        }
    }
}

fn url_params_to_vec(params: &Option<HashMap<String, String>>) -> Vec<(String, String)> {
    let mut vec: Vec<(String, String)> = params
        .iter()
        .flat_map(|map| map.iter())
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    vec.sort_by(|a, b| a.0.cmp(&b.0));
    vec
}
