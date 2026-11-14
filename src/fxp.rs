use crate::theme;
use eframe::egui::{self, Align, Vec2};
use std::path::PathBuf;

#[derive(Clone, Debug, Default)]
pub struct FxSystem {
    pub name: String,
    pub filename: String,
    pub length: f32,
    pub playmode: u32,
    pub culldist: f32,
    pub texture: String,
    pub texture2: String,
    pub alpha_on: u32,
    pub src_blend: u32,
    pub dst_blend: u32,
    pub raw_lines: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct FxpDocument {
    pub header_lines: Vec<String>,
    pub systems: Vec<FxSystem>,
    pub path: Option<PathBuf>,
    pub dirty: bool,
}

impl FxpDocument {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut header_lines = Vec::new();
        let mut systems = Vec::new();
        let mut in_systems = false;
        let mut current_sys: Option<FxSystem> = None;

        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("FILENAME: ") {
                in_systems = true;
                if let Some(sys) = current_sys.take() {
                    systems.push(sys);
                }
                let mut sys = FxSystem::default();
                sys.filename = trimmed["FILENAME: ".len()..].trim().to_string();
                sys.raw_lines.push(line.to_string());
                current_sys = Some(sys);
                continue;
            }

            if !in_systems {
                header_lines.push(line.to_string());
                continue;
            }

            if let Some(sys) = current_sys.as_mut() {
                sys.raw_lines.push(line.to_string());
                if trimmed.starts_with("NAME: ") && sys.name.is_empty() {
                    sys.name = trimmed["NAME: ".len()..].trim().to_string();
                } else if trimmed.starts_with("LENGTH: ") && sys.length == 0.0 {
                    sys.length = trimmed["LENGTH: ".len()..].trim().parse().unwrap_or(1.0);
                } else if trimmed.starts_with("PLAYMODE: ") {
                    sys.playmode = trimmed["PLAYMODE: ".len()..].trim().parse().unwrap_or(0);
                } else if trimmed.starts_with("CULLDIST: ") {
                    sys.culldist = trimmed["CULLDIST: ".len()..].trim().parse().unwrap_or(50.0);
                } else if trimmed.starts_with("TEXTURE: ") && sys.texture.is_empty() {
                    sys.texture = trimmed["TEXTURE: ".len()..].trim().to_string();
                } else if trimmed.starts_with("TEXTURE2: ") {
                    sys.texture2 = trimmed["TEXTURE2: ".len()..].trim().to_string();
                } else if trimmed.starts_with("ALPHAON: ") {
                    sys.alpha_on = trimmed["ALPHAON: ".len()..].trim().parse().unwrap_or(1);
                } else if trimmed.starts_with("SRCBLENDID: ") {
                    sys.src_blend = trimmed["SRCBLENDID: ".len()..].trim().parse().unwrap_or(4);
                } else if trimmed.starts_with("DSTBLENDID: ") {
                    sys.dst_blend = trimmed["DSTBLENDID: ".len()..].trim().parse().unwrap_or(5);
                }
            }
        }

        if let Some(sys) = current_sys {
            systems.push(sys);
        }

        Ok(Self {
            header_lines,
            systems,
            path: None,
            dirty: false,
        })
    }

    pub fn to_string_data(&self) -> String {
        let mut out = String::with_capacity(512 * 1024);
        for line in &self.header_lines {
            out.push_str(line);
            out.push_str("\r\n");
        }
        for sys in &self.systems {
            for line in &sys.raw_lines {
                out.push_str(line);
                out.push_str("\r\n");
            }
        }
        out
    }

    pub fn save_to_disk(&mut self) -> Result<(), String> {
        let path = self.path.as_ref().ok_or("Путь к файлу не задан")?;
        let data = self.to_string_data();
        std::fs::write(path, data).map_err(|e| e.to_string())?;
        self.dirty = false;
        Ok(())
    }
}

pub struct FxpEditor {
    pub doc: FxpDocument,
    pub selected: usize,
    pub filter: String,
    pub raw_edit_buffer: String,
    pub raw_edit_active: bool,
    pub status: String,
}

impl FxpEditor {
    pub fn new(doc: FxpDocument) -> Self {
        let raw = doc
            .systems
            .first()
            .map(|s| s.raw_lines.join("\n"))
            .unwrap_or_default();
        Self {
            doc,
            selected: 0,
            filter: String::new(),
            raw_edit_buffer: raw,
            raw_edit_active: false,
            status: String::new(),
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        let dirty = self.doc.dirty;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            if theme::primary_button(
                ui,
                theme::Icon::Save,
                "Сохранить FXP",
                "Сохранить изменения в effects.fxp (Ctrl+S)",
                dirty,
            )
            .clicked()
            {
                match self.doc.save_to_disk() {
                    Ok(()) => self.status = "Файл effects.fxp успешно сохранён.".into(),
                    Err(e) => self.status = format!("Ошибка сохранения: {e}"),
                }
            }
            if !self.status.is_empty() {
                ui.label(
                    egui::RichText::new(&self.status)
                        .size(12.0)
                        .color(theme::TEXT_SECONDARY),
                );
            }
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format!("Эффектов: {}", self.doc.systems.len()))
                        .size(12.0)
                        .color(theme::TEXT_MUTED),
                );
            });
        });
        ui.add_space(4.0);
        theme::hline(ui);
        ui.add_space(4.0);

        let avail = ui.available_size();
        let list_w = (avail.x * 0.28).clamp(180.0, 240.0);

        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                Vec2::new(list_w, avail.y),
                egui::Layout::top_down(Align::Min),
                |ui| {
                    ui.set_width(list_w);
                    ui.add_sized(
                        [list_w, theme::CONTROL_H],
                        egui::TextEdit::singleline(&mut self.filter).hint_text("Поиск эффектов…"),
                    );
                    ui.add_space(4.0);
                    egui::ScrollArea::vertical()
                        .id_salt("fxp_list")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.set_width(list_w);
                            let item_w = ui.available_width();
                            for (idx, sys) in self.doc.systems.iter().enumerate() {
                                if !self.filter.is_empty()
                                    && !sys
                                        .name
                                        .to_lowercase()
                                        .contains(&self.filter.to_lowercase())
                                    && !sys
                                        .texture
                                        .to_lowercase()
                                        .contains(&self.filter.to_lowercase())
                                {
                                    continue;
                                }
                                let is_sel = idx == self.selected;
                                let resp = ui.add_sized(
                                    [item_w, 22.0],
                                    egui::SelectableLabel::new(is_sel, &sys.name),
                                );
                                if resp.clicked() && idx != self.selected {
                                    self.selected = idx;
                                    self.raw_edit_buffer = sys.raw_lines.join("\n");
                                }
                            }
                        });
                },
            );

            ui.separator();

            let right_w = ui.available_width().max(200.0);
            ui.allocate_ui_with_layout(
                Vec2::new(right_w, avail.y),
                egui::Layout::top_down(Align::Min),
                |ui| {
                    if let Some(sys) = self.doc.systems.get_mut(self.selected) {
                        ui.horizontal(|ui| {
                            ui.heading(&sys.name);
                            ui.label(
                                egui::RichText::new(format!("Текстура: {}", sys.texture))
                                    .color(theme::TEXT_SECONDARY),
                            );
                            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                ui.checkbox(&mut self.raw_edit_active, "Текстовый режим");
                            });
                        });
                        ui.add_space(4.0);
                        theme::hline(ui);
                        ui.add_space(4.0);

                        if self.raw_edit_active {
                            egui::ScrollArea::both()
                                .id_salt("fxp_raw_scroll")
                                .show(ui, |ui| {
                                    if ui
                                        .add(
                                            egui::TextEdit::multiline(&mut self.raw_edit_buffer)
                                                .font(egui::TextStyle::Monospace)
                                                .desired_width(f32::INFINITY)
                                                .desired_rows(24),
                                        )
                                        .changed()
                                    {
                                        sys.raw_lines = self
                                            .raw_edit_buffer
                                            .lines()
                                            .map(|s| s.to_string())
                                            .collect();
                                        self.doc.dirty = true;
                                    }
                                });
                        } else {
                            egui::Grid::new("fx_params_grid")
                                .num_columns(2)
                                .spacing([12.0, 6.0])
                                .show(ui, |ui| {
                                    ui.label("Имя эффекта:");
                                    ui.label(&sys.name);
                                    ui.end_row();

                                    ui.label("Основная текстура:");
                                    if ui.text_edit_singleline(&mut sys.texture).changed() {
                                        self.doc.dirty = true;
                                    }
                                    ui.end_row();

                                    ui.label("Доп. текстура 2:");
                                    if ui.text_edit_singleline(&mut sys.texture2).changed() {
                                        self.doc.dirty = true;
                                    }
                                    ui.end_row();

                                    ui.label("Длительность (сек):");
                                    if ui
                                        .add(egui::DragValue::new(&mut sys.length).speed(0.05))
                                        .changed()
                                    {
                                        self.doc.dirty = true;
                                    }
                                    ui.end_row();

                                    ui.label("Дистанция видимости:");
                                    if ui
                                        .add(egui::DragValue::new(&mut sys.culldist).speed(1.0))
                                        .changed()
                                    {
                                        self.doc.dirty = true;
                                    }
                                    ui.end_row();

                                    ui.label("Режим воспроизведения:");
                                    if ui.add(egui::DragValue::new(&mut sys.playmode)).changed() {
                                        self.doc.dirty = true;
                                    }
                                    ui.end_row();

                                    ui.label("Режимы наложения (Blend):");
                                    ui.horizontal(|ui| {
                                        ui.label(format!(
                                            "Src: {}, Dst: {}",
                                            sys.src_blend, sys.dst_blend
                                        ));
                                    });
                                    ui.end_row();
                                });

                            ui.add_space(10.0);
                            ui.label(
                                egui::RichText::new("Строки параметров блока:")
                                    .size(12.0)
                                    .color(theme::TEXT_SECONDARY),
                            );
                            ui.add_space(4.0);
                            egui::ScrollArea::vertical()
                                .id_salt("fx_lines_scroll")
                                .show(ui, |ui| {
                                    for line in &sys.raw_lines {
                                        ui.monospace(line);
                                    }
                                });
                        }
                    }
                },
            );
        });
    }
}
