use crate::{Browser, egui};
pub(crate) struct Capture {
    browser: Browser,
    output: String,
    frame: u32,
    requested: bool,
    #[cfg(feature = "mods")]
    stop_test_requested: bool,
    started: std::time::Instant,
}
impl Capture {
    pub(crate) fn new(browser: Browser, output: String, ctx: &egui::Context) -> Self {
        #[cfg(feature = "mods")]
        let mut browser = browser;
        #[cfg(feature = "mods")]
        match std::env::var("OPENPROVINCE_CAPTURE_MODE").as_deref() {
            Ok("mods") => browser.mode = crate::AppMode::Mods,
            Ok("catalog") => {
                browser.mode = crate::AppMode::Mods;
                browser.mod_manager.active_subtab = 1;
            }
            _ => {}
        }
        #[cfg(feature = "mods")]
        if let Ok(root) = std::env::var("OPENPROVINCE_CAPTURE_MODS_ROOT") {
            browser.mod_manager =
                crate::ui::mods_panel::ModManagerState::new(std::path::Path::new(&root));
        }
        #[cfg(feature = "mods")]
        if let Ok(executable) = std::env::var("OPENPROVINCE_CAPTURE_GAME") {
            let path = std::path::PathBuf::from(executable);
            if let Some(parent) = path.parent() {
                browser.mod_manager._game_root = parent.to_path_buf();
            }
            browser.custom_mta_exe = Some(path);
        }
        if let Ok(size) = std::env::var("OPENPROVINCE_CAPTURE_SIZE") {
            let dimensions: Vec<f32> = size.split('x').filter_map(|s| s.parse().ok()).collect();
            if dimensions.len() == 2 {
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                    dimensions[0],
                    dimensions[1],
                )));
            }
        }
        Self {
            browser,
            output,
            frame: 0,
            requested: false,
            #[cfg(feature = "mods")]
            stop_test_requested: false,
            started: std::time::Instant::now(),
        }
    }
}
impl eframe::App for Capture {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, input: &mut egui::RawInput) {
        let scenario = std::env::var("OPENPROVINCE_CAPTURE_ACTION").unwrap_or_default();
        let width = std::env::var("OPENPROVINCE_CAPTURE_SIZE")
            .unwrap_or_else(|_| "1280x820".into())
            .split('x')
            .next()
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(1280.0);
        let point = if scenario == "launch-menu" {
            egui::pos2(width - 221.0, 20.0)
        } else {
            egui::pos2(100.0, 98.0)
        };
        if !scenario.is_empty() && (self.frame == 4 || self.frame == 5) {
            input.events.push(egui::Event::PointerMoved(point));
            input.events.push(egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: self.frame == 4,
                modifiers: Default::default(),
            });
        }
        if self.frame == 6 && scenario == "noresults" {
            input
                .events
                .push(egui::Event::Text("no_matching_package".into()));
        }
        if self.frame == 6 && scenario == "search-focus" {
            input.events.push(egui::Event::Text("b".into()));
        }
        if self.frame == 8 && scenario == "search-focus" {
            input.events.push(egui::Event::Key {
                key: egui::Key::Backspace,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            });
        }
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        #[cfg(feature = "mods")]
        if std::env::var("OPENPROVINCE_CAPTURE_ACTION").as_deref() == Ok("game-stop")
            && self.started.elapsed() > std::time::Duration::from_millis(500)
            && self.browser.game_session.running()
            && !self.stop_test_requested
        {
            self.stop_test_requested = true;
            self.browser.stop_game();
        }
        if let Ok(query) = std::env::var("OPENPROVINCE_CAPTURE_FILTER")
            && self.browser.filter != query
        {
            self.browser.filter = query;
            self.browser.row_cache = None;
        }
        self.browser.update(ctx, frame);
        if let Some(model) = &mut self.browser.model {
            model.configure_capture();
        }
        if let Some(editor) = &mut self.browser.txd_editor {
            editor.configure_capture();
        }
        self.frame += 1;
        if !self.requested
            && self.started.elapsed() > std::time::Duration::from_secs(3)
            && self.frame >= 12
            && ((!self.browser.busy
                && !self.browser.loading_model
                && !self.browser.loading_textures)
                || self.started.elapsed() > std::time::Duration::from_secs(30))
        {
            self.requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        for event in ctx.input(|i| i.events.clone()) {
            if let egui::Event::Screenshot { image, .. } = event {
                let pixels: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                let _ = image::save_buffer(
                    &self.output,
                    &pixels,
                    image.size[0] as u32,
                    image.size[1] as u32,
                    image::ColorType::Rgba8,
                );
                #[cfg(feature = "mods")]
                {
                    let report = serde_json::json!({"search": self.browser.mod_manager.search_query, "focused": ctx.memory(|m| m.focused().is_some()), "installed": self.browser.mod_manager.registry.mods().len(), "packages": self.browser.mod_manager.catalog_packages.len()});
                    let _ = std::fs::write(
                        format!("{}.json", self.output),
                        serde_json::to_vec_pretty(&report).unwrap_or_default(),
                    );
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(40));
    }
}
