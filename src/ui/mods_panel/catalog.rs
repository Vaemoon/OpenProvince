use crate::theme;
use eframe::egui::{self, Color32, Margin};
use province_mods::ModCategory;

impl crate::Browser {
    pub(super) fn catalog_ui(&mut self, ctx: &egui::Context) {
        let mut install = None;
        let mut refresh = false;
        egui::CentralPanel::default().frame(egui::Frame::new().fill(theme::BG_PANEL).inner_margin(Margin::same(20))).show(ctx, |ui| {
            ui.label(egui::RichText::new("Пакеты модификаций").size(18.0).strong());
            ui.add_space(6.0);
            ui.label(egui::RichText::new("Локальные .opmod и .zip рядом с программой или в папке OpenProvince/packages игры.").color(theme::TEXT_SECONDARY));
            ui.add_space(16.0);
            ui.horizontal_wrapped(|ui| {
                ui.add_sized([ui.available_width().min(320.0), theme::CONTROL_H], egui::TextEdit::singleline(&mut self.mod_manager.catalog_search).id_salt("catalog_search").hint_text("Найти пакет…"));
                egui::ComboBox::from_id_salt("catalog_category").selected_text(self.mod_manager.catalog_category.as_ref().map_or("Все категории", ModCategory::display_name)).show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.mod_manager.catalog_category, None, "Все категории");
                    for category in [ModCategory::Vehicles, ModCategory::Skins, ModCategory::Tuning, ModCategory::Weapons, ModCategory::Map, ModCategory::Audio, ModCategory::Other] {
                        let label = category.display_name(); ui.selectable_value(&mut self.mod_manager.catalog_category, Some(category), label);
                    }
                });
                refresh = theme::tool_button(ui, theme::Icon::Redo, "Обновить", "Перечитать список локальных пакетов", true).clicked();
            });
            ui.add_space(16.0);
            let query = self.mod_manager.catalog_search.trim().to_lowercase();
            let packages: Vec<_> = self.mod_manager.catalog_packages.iter().filter(|package| {
                let name = package.manifest.as_ref().map(|m| m.name.as_str()).unwrap_or_default();
                let category = package.manifest.as_ref().map(|m| &m.category);
                (self.mod_manager.catalog_category.as_ref().is_none_or(|c| Some(c) == category)) &&
                (query.is_empty() || name.to_lowercase().contains(&query) || package.path.file_name().unwrap_or_default().to_string_lossy().to_lowercase().contains(&query))
            }).collect();
            if packages.is_empty() {
                ui.label(if self.mod_manager.catalog_packages.is_empty() {"Локальных пакетов пока нет."} else {"По этому запросу пакеты не найдены."});
                ui.label(egui::RichText::new("Пакет из другой папки можно выбрать через «Установить…».").color(theme::TEXT_SECONDARY));
            }
            egui::ScrollArea::vertical().id_salt("local_mod_catalog").show(ui, |ui| {
                for package in packages {
                    ui.push_id(&package.path, |ui| {
                        let fallback = package.path.file_stem().unwrap_or_default().to_string_lossy();
                        let name = package.manifest.as_ref().map_or(fallback.as_ref(), |m| m.name.as_str());
                        ui.label(egui::RichText::new(name).strong());
                        if let Some(manifest) = &package.manifest {
                            ui.label(egui::RichText::new(format!("{} · {} · {}", manifest.category.display_name(), manifest.version, manifest.author)).small().color(theme::TEXT_SECONDARY));
                            if !manifest.description.is_empty() {ui.label(&manifest.description);}
                        }
                        ui.add(egui::Label::new(egui::RichText::new(package.path.file_name().unwrap_or_default().to_string_lossy()).small().color(theme::TEXT_SECONDARY)).truncate()).on_hover_text(package.path.display().to_string());
                        if let Some(error) = &package.error {
                            ui.label(egui::RichText::new(format!("Не удалось прочитать пакет: {error}")).color(Color32::from_rgb(239, 147, 147)));
                        } else {
                            let installed = package.manifest.as_ref().is_some_and(|m| self.mod_manager.registry.find_mod(&m.id).is_some());
                            if installed {ui.label(egui::RichText::new("Уже установлен").small().color(theme::TEXT_SECONDARY));}
                            else if theme::tool_button(ui, theme::Icon::Install, "Установить", "Установить файлы из этого пакета", !self.mod_manager.is_busy).clicked() {install = Some(package.path.clone());}
                        }
                        ui.add_space(12.0); theme::hline(ui); ui.add_space(12.0);
                    });
                }
            });
        });
        if refresh {
            self.mod_manager.reload();
        }
        if let Some(path) = install {
            match self.mod_manager.install_package(&path) {
                Ok(message) => {
                    self.mod_manager.banner_message = Some((message, false));
                    self.mod_manager.active_subtab = 0;
                }
                Err(error) => self.mod_manager.banner_message = Some((error, true)),
            }
        }
    }
}
