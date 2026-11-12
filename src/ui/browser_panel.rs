use super::{Command, RowItem, file_icon, menu, search_field};
use crate::{Browser, EntryRef, theme};
use eframe::egui::{self, Align, Color32, Margin, Sense, Stroke, Vec2};
use egui_extras::{Column, TableBuilder};
use std::sync::Arc;

impl Browser {
    pub(super) fn browser_panel_ui(
        &mut self,
        ctx: &egui::Context,
        command: &mut Option<Command>,
        menu_jump_to: Option<EntryRef>,
    ) {
        let has_archives = !self.archives.is_empty();
        let has_multi_archives = self.archives.len() > 1;
        let multi = has_multi_archives && self.all_tabs;
        let mut clicked = None;
        let mut double = false;
        let mut rendered_rows = Arc::new(Vec::new());

        egui::SidePanel::left("resource_files")
            .resizable(true)
            .default_width(540.0)
            .min_width(380.0)
            .max_width(960.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::BG_PANEL)
                    .inner_margin(Margin {
                        left: 0,
                        right: 6,
                        top: 0,
                        bottom: 0,
                    }),
            )
            .show(ctx, |ui| {
                let search_resp = search_field(
                    ui,
                    &mut self.filter,
                    &mut self.group_by_archive,
                    has_archives && multi,
                );
                if self.focus_search {
                    search_resp.request_focus();
                    self.focus_search = false;
                }
                if search_resp.has_focus()
                    && !self.filter.is_empty()
                    && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
                {
                    self.filter.clear();
                }

                let rows = self.filtered_rows();
                let visible_entries = rows
                    .iter()
                    .filter(|r| matches!(r, RowItem::Entry(_)))
                    .count();
                if has_archives {
                    egui::Frame::new()
                        .inner_margin(Margin::symmetric(8, 2))
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;
                                for (mode, label) in
                                    [(0u8, "Все"), (1, "DFF"), (2, "TXD"), (3, "Прочие")]
                                {
                                    if theme::tab_toggle(ui, self.type_filter == mode, label)
                                        .clicked()
                                    {
                                        self.type_filter = mode;
                                        self.row_cache = None;
                                    }
                                }
                                if !self.edits.is_empty()
                                    && theme::tab_toggle(
                                        ui,
                                        self.type_filter == 4,
                                        &format!("Правки ({})", self.edits.len()),
                                    )
                                    .clicked()
                                {
                                    self.type_filter = if self.type_filter == 4 { 0 } else { 4 };
                                    self.row_cache = None;
                                }
                                if has_multi_archives {
                                    ui.separator();
                                    if theme::tab_toggle(ui, self.all_tabs, "Все вкладки")
                                        .on_hover_text(
                                            "Искать и показывать файлы из всех открытых IMG",
                                        )
                                        .clicked()
                                    {
                                        self.all_tabs = !self.all_tabs;
                                        self.row_cache = None;
                                    }
                                }
                                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                    ui.add_space(6.0);
                                    ui.label(
                                        egui::RichText::new(theme::format_thousands(
                                            visible_entries as u64,
                                        ))
                                        .size(11.5)
                                        .color(theme::TEXT_MUTED),
                                    );
                                });
                            });
                        });
                    ui.add_space(2.0);
                }
                theme::hline(ui);

                let rows = self.filtered_rows();
                rendered_rows = rows.clone();
                let mut arrow = None;
                if !ctx.wants_keyboard_input() {
                    if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                        arrow = Some(1isize);
                    }
                    if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                        arrow = Some(-1isize);
                    }
                }
                let selectable_indices: Vec<usize> = rows
                    .iter()
                    .enumerate()
                    .filter_map(|(i, item)| matches!(item, RowItem::Entry(_)).then_some(i))
                    .collect();
                if let Some(delta) = arrow
                    && !selectable_indices.is_empty()
                {
                    let cur_pos = self
                        .selected
                        .and_then(|sel| {
                            selectable_indices.iter().position(
                                |&ri| matches!(&rows[ri], RowItem::Entry(e) if e.target == sel),
                            )
                        })
                        .unwrap_or(0);
                    let next_pos = cur_pos
                        .saturating_add_signed(delta)
                        .min(selectable_indices.len() - 1);
                    if let RowItem::Entry(e) = &rows[selectable_indices[next_pos]] {
                        clicked = Some(e.target);
                        self.scroll_selection = true;
                    }
                }
                let scroll = self
                    .scroll_selection
                    .then(|| {
                        self.selected.and_then(|sel| {
                            rows.iter()
                                .position(|r| matches!(r, RowItem::Entry(e) if e.target == sel))
                        })
                    })
                    .flatten();
                self.scroll_selection = false;
                let table_top = ui.cursor().top();
                let table_left = ui.cursor().left();
                let spacing_x = ui.spacing().item_spacing.x;
                let cols_count = if multi { 4 } else { 3 };
                let gaps = spacing_x * (cols_count - 1) as f32;
                let scroll_w = ui.spacing().scroll.allocated_width();
                let total_cols_w = (ui.available_width() - scroll_w - gaps).max(210.0);

                let right_mins: &[f32] = if multi {
                    &[64.0, 44.0, 62.0]
                } else {
                    &[44.0, 62.0]
                };
                let right_maxs: &[f32] = if multi {
                    &[240.0, 120.0, 160.0]
                } else {
                    &[120.0, 160.0]
                };
                let right_defs: &[f32] = if multi {
                    &[108.0, 56.0, 84.0]
                } else {
                    &[56.0, 84.0]
                };
                let mut right_w: Vec<f32> = if multi {
                    vec![self.col_widths[0], self.col_widths[1], self.col_widths[2]]
                } else {
                    vec![self.col_widths[1], self.col_widths[2]]
                };
                for (i, w) in right_w.iter_mut().enumerate() {
                    *w = w.clamp(right_mins[i], right_maxs[i]);
                }
                let min_name = 110.0_f32;
                let right_sum: f32 = right_w.iter().sum();
                if right_sum > total_cols_w - min_name {
                    let excess = right_sum - (total_cols_w - min_name);
                    let shrinkable: f32 = right_w
                        .iter()
                        .zip(right_mins)
                        .map(|(w, m)| (*w - *m).max(0.0))
                        .sum();
                    if shrinkable > 0.5 {
                        let factor = (excess / shrinkable).min(1.0);
                        for (w, &m) in right_w.iter_mut().zip(right_mins) {
                            *w -= (*w - m) * factor;
                        }
                    }
                }
                let w_name = (total_cols_w - right_w.iter().sum::<f32>()).max(min_name);
                let mut widths = Vec::with_capacity(cols_count);
                widths.push(w_name);
                widths.extend_from_slice(&right_w);

                let table = TableBuilder::new(ui).striped(false);
                let table = if let Some(row) = scroll {
                    table.scroll_to_row(row, Some(Align::Center))
                } else {
                    table
                };
                let mut table = table
                    .striped(false)
                    .resizable(false)
                    .cell_layout(egui::Layout::left_to_right(Align::Center))
                    .sense(Sense::click());
                for &w in &widths {
                    table = table.column(Column::exact(w).clip(true).resizable(false));
                }

                let mut cols: Vec<(&str, usize)> = vec![("Имя", 0)];
                if multi {
                    cols.push(("Архив", 1));
                }
                cols.extend([("Тип", 2), ("Размер", 3)]);

                table
                    .header(theme::HEADER_H, |mut header| {
                        for &(label, col_id) in &cols {
                            header.col(|ui| {
                                let title = if col_id < 10 && self.sort == col_id {
                                    if self.descending {
                                        format!("{label} ▼")
                                    } else {
                                        format!("{label} ▲")
                                    }
                                } else {
                                    label.to_owned()
                                };
                                let btn_fn = |ui: &mut egui::Ui| {
                                    let resp = ui.add(
                                        egui::Button::new(
                                            egui::RichText::new(title).size(12.0).color(
                                                if self.sort == col_id {
                                                    theme::TEXT_PRIMARY
                                                } else {
                                                    theme::TEXT_SECONDARY
                                                },
                                            ),
                                        )
                                        .frame(false)
                                        .min_size(Vec2::new(0.0, theme::HEADER_H)),
                                    );
                                    if resp.clicked() && col_id < 10 {
                                        if self.sort == col_id {
                                            self.descending = !self.descending;
                                        } else {
                                            self.sort = col_id;
                                            self.descending = false;
                                        }
                                        self.row_cache = None;
                                    }
                                };
                                if col_id == 3 {
                                    ui.with_layout(
                                        egui::Layout::right_to_left(Align::Center),
                                        |ui| {
                                            ui.add_space(12.0);
                                            btn_fn(ui);
                                        },
                                    );
                                } else {
                                    if col_id == 0 {
                                        ui.add_space(6.0);
                                    }
                                    btn_fn(ui);
                                }
                            });
                        }
                    })
                    .body(|mut body| {
                        if rows.is_empty() {
                            body.row(64.0, |mut row| {
                                row.col(|ui| {
                                    ui.add_space(10.0);
                                    ui.weak(if self.filter.is_empty() && self.type_filter == 0 {
                                        "В этой папке пока нет элементов"
                                    } else {
                                        "Ничего не найдено по текущему фильтру"
                                    });
                                });
                            });
                            return;
                        }
                        body.rows(theme::ROW_H, rows.len(), |mut row| match &rows[row.index()] {
                            RowItem::Section { title, count } => {
                                row.col(|ui| {
                                    theme::section_bar(ui, title, *count);
                                });
                                for _ in 0..if multi { 3 } else { 2 } {
                                    row.col(|ui| {
                                        theme::section_bar_fill(ui);
                                    });
                                }
                            }
                            RowItem::Entry(item) => {
                                let is_selected = self.selection.contains(&item.target);
                                row.set_selected(is_selected);
                                let edit_tag = self.pending_edit_tag(item.archive_idx, &item.name);
                                row.col(|ui| {
                                    ui.add_space(6.0);
                                    file_icon(ui, item.directory, &item.kind);
                                    let name_color = if let Some((_, color)) = edit_tag {
                                        color
                                    } else if is_selected {
                                        Color32::WHITE
                                    } else {
                                        theme::TEXT_PRIMARY
                                    };
                                    let label_resp = ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(&item.name).color(name_color),
                                        )
                                        .truncate(),
                                    );
                                    if item.name.len() > 22 {
                                        label_resp.on_hover_text(&item.name);
                                    }
                                    if let Some((tag, color)) = edit_tag {
                                        let suffix = if tag == "ЗАМ" { "[зам]" } else { "[удал]" };
                                        ui.label(egui::RichText::new(suffix).size(11.0).color(color));
                                    }
                                });
                                if multi {
                                    row.col(|ui| {
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(&item.archive_stem)
                                                    .size(12.0)
                                                    .color(theme::TEXT_SECONDARY),
                                            )
                                            .truncate(),
                                        )
                                        .on_hover_text(self.archive_label(item.archive_idx));
                                    });
                                }
                                row.col(|ui| {
                                    ui.label(
                                        egui::RichText::new(if item.directory {
                                            "Папка"
                                        } else {
                                            &item.kind
                                        })
                                        .size(11.5)
                                        .color(theme::TEXT_SECONDARY),
                                    );
                                });
                                row.col(|ui| {
                                    ui.with_layout(
                                        egui::Layout::right_to_left(Align::Center),
                                        |ui| {
                                            ui.add_space(12.0);
                                            if !item.directory {
                                                ui.label(
                                                    egui::RichText::new(theme::format_size(
                                                        item.size,
                                                    ))
                                                    .size(12.0)
                                                    .color(theme::TEXT_SECONDARY),
                                                )
                                                .on_hover_text(format!(
                                                    "{} Б",
                                                    theme::format_thousands(item.size)
                                                ));
                                            }
                                        },
                                    );
                                });
                                let response = row.response();
                                if response.clicked() || response.double_clicked() {
                                    clicked = Some(item.target);
                                    double = response.double_clicked();
                                }
                                let paired = self.find_paired_asset(item.target);
                                response.context_menu(|ui| {
                                    if !self.selection.contains(&item.target) {
                                        self.selection.clear();
                                        self.selection.insert(item.target);
                                        self.selected = Some(item.target);
                                    }
                                    menu(ui, "Просмотр    F3", true, Command::Preview, command);
                                    if let Some((paired_target, paired_name)) = &paired
                                        && ui.button(format!("Перейти к {paired_name}")).clicked()
                                    {
                                        clicked = Some(*paired_target);
                                        self.scroll_selection = true;
                                        ui.close_menu();
                                    }
                                    if multi
                                        && item.archive_idx != self.active_archive
                                        && ui
                                            .button(format!(
                                                "Открыть во вкладке {}",
                                                item.archive_stem
                                            ))
                                            .clicked()
                                    {
                                        self.active_archive = item.archive_idx;
                                        self.all_tabs = false;
                                        self.row_cache = None;
                                        clicked = Some(item.target);
                                        self.scroll_selection = true;
                                        ui.close_menu();
                                    }
                                    ui.separator();
                                    menu(
                                        ui,
                                        "Извлечь…    Ctrl+E",
                                        has_archives,
                                        Command::Extract,
                                        command,
                                    );
                                    menu(
                                        ui,
                                        "Заменить…",
                                        has_archives && self.selection.len() == 1,
                                        Command::Replace,
                                        command,
                                    );
                                    menu(
                                        ui,
                                        "Удалить    Delete",
                                        has_archives,
                                        Command::Remove,
                                        command,
                                    );
                                });
                            }
                        });
                    });
                let table_bottom = ui.min_rect().bottom().min(ui.clip_rect().bottom());
                let header_bottom = table_top + theme::HEADER_H;
                let mut sep_x = table_left - spacing_x * 0.5;
                for k in 0..cols_count - 1 {
                    sep_x += widths[k] + spacing_x;
                    let hit_rect = egui::Rect::from_min_max(
                        egui::pos2(sep_x - 4.0, table_top),
                        egui::pos2(sep_x + 4.0, header_bottom),
                    );
                    let sep_id = ui.id().with(("col_sep", cols_count, k));
                    let resp = ui.interact(hit_rect, sep_id, egui::Sense::click_and_drag());
                    if resp.double_clicked() {
                        self.col_widths = [108.0, 56.0, 84.0];
                    } else if resp.dragged()
                        && let Some(ptr) = ctx.pointer_latest_pos()
                    {
                        let dx = ptr.x - sep_x;
                        let right_offset = if multi { 0 } else { 1 };
                        if dx > 0.5 {
                            let max_give = if k == 0 {
                                (widths[0] + right_w.iter().sum::<f32>() - min_name).max(0.0)
                            } else {
                                (right_maxs[k - 1] - right_w[k - 1]).max(0.0)
                            };
                            let mut rem = dx.min(max_give);
                            let mut taken = 0.0_f32;
                            for r in k..right_w.len() {
                                if rem <= 0.0 {
                                    break;
                                }
                                let can_take = (right_w[r] - right_mins[r]).max(0.0);
                                let d = rem.min(can_take);
                                right_w[r] -= d;
                                rem -= d;
                                taken += d;
                            }
                            if k > 0
                                && taken > 0.0
                                && !(k == right_w.len() - 1 && right_w[k - 1] >= right_defs[k - 1])
                            {
                                right_w[k - 1] += taken;
                            }
                        } else if dx < -0.5 {
                            let mut need = (-dx).min((right_maxs[k] - right_w[k]).max(0.0));
                            let name_avail = (widths[0] - min_name).max(0.0);
                            if k == 0 {
                                right_w[0] += need.min(name_avail);
                            } else {
                                let d_name = need.min(name_avail);
                                right_w[k] += d_name;
                                need -= d_name;
                                if need > 0.0 {
                                    let d_left =
                                        need.min((right_w[k - 1] - right_mins[k - 1]).max(0.0));
                                    right_w[k - 1] -= d_left;
                                    right_w[k] += d_left;
                                }
                            }
                        }
                        for (i, w) in right_w.iter().enumerate() {
                            self.col_widths[right_offset + i] = *w;
                        }
                    }
                    let hovering = resp.hovered() || resp.dragged();
                    if hovering {
                        ctx.set_cursor_icon(egui::CursorIcon::ResizeColumn);
                    }
                    let line_color = if resp.dragged() {
                        theme::ACCENT
                    } else if hovering {
                        theme::BORDER_HOVER
                    } else {
                        theme::BORDER_LINE
                    };
                    let y_end = if resp.dragged() {
                        table_bottom
                    } else {
                        header_bottom
                    };
                    ui.painter().line_segment(
                        [egui::pos2(sep_x, table_top), egui::pos2(sep_x, y_end)],
                        Stroke::new(1.0_f32, line_color),
                    );
                }
            });

        let rows = rendered_rows;
        let selectable_indices: Vec<usize> = rows
            .iter()
            .enumerate()
            .filter_map(|(i, r)| matches!(r, RowItem::Entry(_)).then_some(i))
            .collect();
        if let Some(target) = menu_jump_to {
            self.selection.clear();
            self.selection.insert(target);
            self.scroll_selection = true;
            self.select(target, ctx);
        } else if let Some(target) = clicked {
            let modifiers = ctx.input(|i| i.modifiers);
            if modifiers.shift {
                let anchor = self
                    .selected
                    .and_then(|sel| {
                        selectable_indices.iter().position(
                            |&ri| matches!(&rows[ri], RowItem::Entry(e) if e.target == sel),
                        )
                    })
                    .unwrap_or(0);
                let position = selectable_indices
                    .iter()
                    .position(|&ri| matches!(&rows[ri], RowItem::Entry(e) if e.target == target))
                    .unwrap_or(0);
                if !modifiers.ctrl {
                    self.selection.clear();
                }
                for &ri in &selectable_indices[anchor.min(position)..=anchor.max(position)] {
                    if let RowItem::Entry(e) = &rows[ri] {
                        self.selection.insert(e.target);
                    }
                }
            } else if modifiers.ctrl {
                if !self.selection.remove(&target) {
                    self.selection.insert(target);
                }
            } else {
                self.selection.clear();
                self.selection.insert(target);
            }
            self.selected = Some(target);
            if double {
                *command = Some(Command::Preview);
            } else if self.preview_open {
                if has_archives {
                    self.select(target, ctx);
                } else if let Some(file) = self.folder_files.get(target.index)
                    && !file.directory
                    && file
                        .path
                        .extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("txd"))
                {
                    let path = file.path.clone();
                    self.open_txd_file(path, ctx);
                }
            }
        }
    }
}
