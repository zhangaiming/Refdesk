use crate::app::Desk;
use crate::diff::{Content, Kind, is_workbook};
use crate::ui::{BORDER, FAINT, GREEN, INK, MUTE, RED, SHADE, paint, small_button};
use eframe::egui::{self, Color32, FontId, Rect, RichText, Sense, Stroke, Vec2, pos2};

impl Desk {
    pub(crate) fn diff_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let path = self.file.as_ref().map(|file| file.path.clone());
        let selected_tool = path
            .as_ref()
            .and_then(|path| self.settings.matching_tool(path))
            .cloned();
        let mut invoke = false;
        let mut toggle = false;
        let mut save = false;
        let workbook = self
            .file
            .as_ref()
            .is_some_and(|file| !file.is_gitlink() && is_workbook(&file.path));
        ui.horizontal(|ui| {
            ui.label(RichText::new("差异").size(17.0).strong().color(INK));
            if let Some(path) = &path {
                ui.add(egui::Label::new(RichText::new(path).size(12.0).color(MUTE)).truncate())
                    .on_hover_text(path);
            }
            if self.diff_busy {
                ui.spinner();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(tool) = &selected_tool {
                    let text = if workbook {
                        "Excel Diff ↗".to_owned()
                    } else {
                        format!("{} ↗", tool.name)
                    };
                    invoke = small_button(ui, &text)
                        .on_hover_text(&tool.command)
                        .clicked();
                }
                if workbook && let Some(Content::Sheet(_)) = self.content {
                    toggle = small_button(ui, "比较原始文件").clicked();
                } else if workbook && self.force_text {
                    toggle = small_button(ui, "单元格对比").clicked();
                }
                if matches!(self.content, Some(Content::Text { .. })) {
                    save = ui
                        .checkbox(&mut self.settings.changes_only, "仅显示变更")
                        .changed();
                }
            });
        });
        if save {
            self.diff_reset += 1;
            self.save();
        }
        if invoke && let Some(tool) = selected_tool {
            self.external(ctx, tool);
        }
        if toggle {
            self.force_text = !self.force_text;
            self.sheet_view = crate::ui_workbook::SheetView::default();
            self.request_diff(ctx);
        }

        if self.file.is_none() {
            ui.add_space(8.0);
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("从右侧文件列表选择一项来比较").color(MUTE));
            });
            return;
        }
        if let Some(error) = &self.diff_error {
            ui.add_space(8.0);
            ui.colored_label(RED, error);
            return;
        }
        let Some(pair) = self.comparison.as_ref() else {
            return;
        };
        let mut full_line = None;
        if let Some(content) = self.content.as_ref() {
            match content {
                Content::Text {
                    lines,
                    added,
                    removed,
                } => {
                    let before = format!("旧版本  ·  {}", pair.left.title());
                    let after = format!(
                        "新版本  ·  {}     +{}  −{}",
                        pair.right.title(),
                        added,
                        removed
                    );
                    diff_header(ui, &before, &after);
                    let row_indices: Vec<_> = (0..lines.len())
                        .filter(|index| {
                            !self.settings.changes_only || lines[*index].kind != Kind::Equal
                        })
                        .collect();
                    if *added + *removed == 0 {
                        ui.label(RichText::new("文本内容相同").small().color(MUTE));
                    }
                    ui.scope(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        egui::ScrollArea::vertical()
                            .id_salt(("text-diff", self.diff_reset))
                            .auto_shrink([false, false])
                            .show_rows(ui, 20.0, row_indices.len(), |ui, visible| {
                                for index in visible {
                                    let line = &lines[row_indices[index]];
                                    let (rect, response) = ui.allocate_exact_size(
                                        Vec2::new(ui.available_width(), 20.0),
                                        Sense::click(),
                                    );
                                    let (left, right) = halves(rect);
                                    code(
                                        ui,
                                        left,
                                        line.old.as_ref(),
                                        match line.kind {
                                            Kind::Remove | Kind::Replace => Some(RED),
                                            _ => None,
                                        },
                                    );
                                    code(
                                        ui,
                                        right,
                                        line.new.as_ref(),
                                        match line.kind {
                                            Kind::Add | Kind::Replace => Some(GREEN),
                                            _ => None,
                                        },
                                    );
                                    ui.painter().vline(
                                        right.left(),
                                        rect.y_range(),
                                        Stroke::new(1.0_f32, BORDER),
                                    );
                                    if response.clicked()
                                        && let Some(pointer) = response.interact_pointer_pos()
                                    {
                                        let entry = if pointer.x < right.left() {
                                            &line.old
                                        } else {
                                            &line.new
                                        };
                                        full_line = entry.as_ref().map(|(_, text)| text.clone());
                                    }
                                    response.on_hover_text("单击查看完整行");
                                }
                            });
                    });
                }
                Content::Sheet(workbook) => {
                    full_line = self.sheet_view.show(
                        ui,
                        workbook,
                        self.diff_reset,
                        pair.left.title(),
                        pair.right.title(),
                    );
                }
                Content::Binary { left, right } => {
                    ui.label(
                        RichText::new(format!(
                            "二进制文件，旧版本 {left} 字节，新版本 {right} 字节。"
                        ))
                        .color(MUTE),
                    );
                }
                Content::Notice(message) => {
                    ui.label(RichText::new(message).color(MUTE));
                }
            }
        }
        if let Some(value) = full_line {
            self.show_full_line = Some(value);
        }
    }
}

fn halves(rect: Rect) -> (Rect, Rect) {
    let middle = rect.center().x;
    (
        Rect::from_min_max(rect.min, pos2(middle, rect.bottom())),
        Rect::from_min_max(pos2(middle, rect.top()), rect.max),
    )
}

pub(crate) fn diff_header(ui: &mut egui::Ui, left: &str, right: &str) {
    ui.add_space(3.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 25.0), Sense::hover());
    ui.painter().rect_filled(rect, 3.0, SHADE);
    let (l, r) = halves(rect);
    paint(
        ui,
        l,
        l.left() + 10.0,
        left,
        FontId::proportional(12.0),
        MUTE,
    );
    paint(
        ui,
        r,
        r.left() + 10.0,
        right,
        FontId::proportional(12.0),
        MUTE,
    );
}

fn code(ui: &egui::Ui, rect: Rect, value: Option<&(usize, String)>, color: Option<Color32>) {
    if let Some(color) = color {
        ui.painter()
            .rect_filled(rect, 0.0, color.gamma_multiply(0.13));
    }
    let Some((number, text)) = value else { return };
    let gutter = Rect::from_min_max(rect.min, pos2(rect.left() + 49.0, rect.bottom()));
    paint(
        ui,
        gutter,
        rect.left() + 8.0,
        &number.to_string(),
        FontId::monospace(11.0),
        FAINT,
    );
    if let Some(color) = color {
        ui.painter().vline(
            rect.left() + 47.0,
            rect.y_range(),
            Stroke::new(2.0_f32, color),
        );
    }
    paint(
        ui,
        rect,
        rect.left() + 54.0,
        text,
        FontId::monospace(12.0),
        INK,
    );
}
