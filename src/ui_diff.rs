use crate::app::Desk;
use crate::diff::{Content, Kind, Line, is_workbook};
use crate::theme::{self, ChangeColors, Palette};
use crate::ui::{compact_button, paint, small_button};
use eframe::egui::{self, FontId, Rect, RichText, Sense, Stroke, Vec2, pos2};

const TEXT_ROW_HEIGHT: f32 = 20.0;

#[derive(Clone, Copy, Default)]
struct TextDiffNavigation {
    reset: u64,
    offset: f32,
    focus: Option<usize>,
}

impl TextDiffNavigation {
    fn targets(self, changes: &[usize]) -> (Option<usize>, Option<usize>) {
        let anchor = self
            .focus
            .unwrap_or_else(|| (self.offset / TEXT_ROW_HEIGHT) as usize);
        let previous = changes.partition_point(|&row| row < anchor);
        let next = previous + usize::from(self.focus.is_some());
        (
            previous.checked_sub(1).map(|index| changes[index]),
            changes.get(next).copied(),
        )
    }

    fn update(&mut self, offset: f32, jump: Option<usize>) {
        if jump.is_some() || self.offset != offset {
            self.focus = jump;
        }
        self.offset = offset;
    }
}

fn text_diff_rows(lines: &[Line], changes_only: bool) -> (Vec<usize>, Vec<usize>) {
    let mut indices = Vec::with_capacity(lines.len());
    let mut changes = Vec::new();
    let mut in_change = false;
    for (index, line) in lines.iter().enumerate() {
        let changed = line.kind != Kind::Equal;
        if changed && !in_change {
            changes.push(indices.len());
        }
        if changed || !changes_only {
            indices.push(index);
        }
        in_change = changed;
    }
    (indices, changes)
}

impl Desk {
    pub(crate) fn diff_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let colors = theme::palette(ctx);
        let path = self.file.as_ref().map(|file| file.path.clone());
        let selected_tool = path
            .as_ref()
            .and_then(|path| self.settings.matching_tool(path))
            .cloned();
        let mut invoke = false;
        let mut toggle = false;
        let mut save = false;
        let mut text_rows = None;
        let mut jump = None;
        let navigation_id = ui.id().with("text-diff-navigation");
        let mut navigation: TextDiffNavigation = ui
            .data(|data| data.get_temp(navigation_id))
            .filter(|state: &TextDiffNavigation| state.reset == self.diff_reset)
            .unwrap_or_default();
        let workbook = self
            .file
            .as_ref()
            .is_some_and(|file| !file.is_gitlink() && is_workbook(&file.path));
        ui.horizontal(|ui| {
            ui.label(RichText::new("差异").size(17.0).strong().color(colors.text));
            if let Some(path) = &path {
                ui.add(
                    egui::Label::new(RichText::new(path).size(12.0).color(colors.muted)).truncate(),
                )
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
                if let Some(Content::Text { lines, .. }) = &self.content {
                    save = ui
                        .checkbox(&mut self.settings.changes_only, "仅显示变更")
                        .changed();
                    if save {
                        navigation = TextDiffNavigation::default();
                    }
                    let (indices, changes) = text_diff_rows(lines, self.settings.changes_only);
                    let (previous, next) = navigation.targets(&changes);
                    if ui
                        .add_enabled(next.is_some(), compact_button("↓"))
                        .on_hover_text("下一处变更")
                        .clicked()
                    {
                        jump = next;
                    }
                    if ui
                        .add_enabled(previous.is_some(), compact_button("↑"))
                        .on_hover_text("上一处变更")
                        .clicked()
                    {
                        jump = previous;
                    }
                    text_rows = Some(indices);
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
                ui.label(RichText::new("从右侧文件列表选择一项来比较").color(colors.muted));
            });
            return;
        }
        if let Some(error) = &self.diff_error {
            ui.add_space(8.0);
            ui.colored_label(colors.removed.foreground, error);
            return;
        }
        let Some(pair) = self.comparison.as_ref() else {
            return;
        };
        let mut full_line = None;
        if let Some(content) = self.content.as_ref() {
            match (content, text_rows) {
                (
                    Content::Text {
                        lines,
                        added,
                        removed,
                    },
                    Some(row_indices),
                ) => {
                    let before = format!("旧版本  ·  {}", pair.left.title());
                    let after = format!(
                        "新版本  ·  {}     +{}  −{}",
                        pair.right.title(),
                        added,
                        removed
                    );
                    diff_header(ui, &before, &after);
                    if *added + *removed == 0 {
                        ui.label(RichText::new("文本内容相同").small().color(colors.muted));
                    }
                    let focus = jump.or(navigation.focus);
                    let mut scroll = egui::ScrollArea::vertical()
                        .id_salt(("text-diff", self.diff_reset))
                        .auto_shrink([false, false]);
                    if let Some(row) = jump {
                        scroll = scroll.vertical_scroll_offset(row as f32 * TEXT_ROW_HEIGHT);
                    }
                    let offset = ui
                        .scope(|ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;
                            let output = scroll.show_rows(
                                ui,
                                TEXT_ROW_HEIGHT,
                                row_indices.len(),
                                |ui, visible| {
                                    for index in visible {
                                        let line = &lines[row_indices[index]];
                                        let (rect, response) = ui.allocate_exact_size(
                                            Vec2::new(ui.available_width(), TEXT_ROW_HEIGHT),
                                            Sense::click(),
                                        );
                                        let (left, right) = halves(rect);
                                        code(
                                            ui,
                                            left,
                                            line.old.as_ref(),
                                            &colors,
                                            match line.kind {
                                                Kind::Remove | Kind::Replace => {
                                                    Some((colors.removed, '−'))
                                                }
                                                _ => None,
                                            },
                                        );
                                        code(
                                            ui,
                                            right,
                                            line.new.as_ref(),
                                            &colors,
                                            match line.kind {
                                                Kind::Add | Kind::Replace => {
                                                    Some((colors.added, '+'))
                                                }
                                                _ => None,
                                            },
                                        );
                                        ui.painter().vline(
                                            right.left(),
                                            rect.y_range(),
                                            Stroke::new(1.0_f32, colors.border),
                                        );
                                        if focus == Some(index) {
                                            ui.painter().rect_stroke(
                                                rect.shrink(0.5),
                                                0.0,
                                                Stroke::new(1.0_f32, colors.accent),
                                                egui::StrokeKind::Inside,
                                            );
                                        }
                                        if response.clicked()
                                            && let Some(pointer) = response.interact_pointer_pos()
                                        {
                                            let entry = if pointer.x < right.left() {
                                                &line.old
                                            } else {
                                                &line.new
                                            };
                                            full_line =
                                                entry.as_ref().map(|(_, text)| text.clone());
                                        }
                                        response.on_hover_text("单击查看完整行");
                                    }
                                },
                            );
                            output.state.offset.y
                        })
                        .inner;
                    if jump.is_some() || navigation.offset != offset {
                        ctx.request_repaint();
                    }
                    navigation.update(offset, jump);
                    navigation.reset = self.diff_reset;
                    ui.data_mut(|data| data.insert_temp(navigation_id, navigation));
                }
                (Content::Sheet(workbook), _) => {
                    full_line = self.sheet_view.show(
                        ui,
                        workbook,
                        self.diff_reset,
                        pair.left.title(),
                        pair.right.title(),
                    );
                }
                (Content::Binary { left, right }, _) => {
                    ui.label(
                        RichText::new(format!(
                            "二进制文件，旧版本 {left} 字节，新版本 {right} 字节。"
                        ))
                        .color(colors.muted),
                    );
                }
                (Content::Notice(message), _) => {
                    ui.label(RichText::new(message).color(colors.muted));
                }
                (Content::Text { .. }, None) => {}
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
    let colors = theme::palette(ui.ctx());
    ui.add_space(3.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 25.0), Sense::hover());
    ui.painter().rect_filled(rect, 3.0, colors.control);
    let (l, r) = halves(rect);
    paint(
        ui,
        l,
        l.left() + 10.0,
        left,
        FontId::proportional(12.0),
        colors.muted,
    );
    paint(
        ui,
        r,
        r.left() + 10.0,
        right,
        FontId::proportional(12.0),
        colors.muted,
    );
}

fn code(
    ui: &egui::Ui,
    rect: Rect,
    value: Option<&(usize, String)>,
    colors: &Palette,
    change: Option<(ChangeColors, char)>,
) {
    let Some((number, text)) = value else { return };
    let gutter = Rect::from_min_max(rect.min, pos2(rect.left() + 49.0, rect.bottom()));
    let (gutter_text, gutter_color) = if let Some((change, sign)) = change {
        ui.painter().rect_filled(rect, 0.0, change.background);
        ui.painter().vline(
            rect.left() + 47.0,
            rect.y_range(),
            Stroke::new(2.0_f32, change.border),
        );
        (format!("{sign}{number}"), change.foreground)
    } else {
        (number.to_string(), colors.subtle)
    };
    paint(
        ui,
        gutter,
        rect.left() + 8.0,
        &gutter_text,
        FontId::monospace(11.0),
        gutter_color,
    );
    paint(
        ui,
        rect,
        rect.left() + 54.0,
        text,
        FontId::monospace(12.0),
        colors.text,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hiding_context_preserves_separate_change_blocks() {
        let lines = [
            Kind::Equal,
            Kind::Replace,
            Kind::Add,
            Kind::Equal,
            Kind::Remove,
            Kind::Equal,
        ]
        .map(|kind| Line {
            old: None,
            new: None,
            kind,
        });
        assert_eq!(
            text_diff_rows(&lines, false),
            ((0..6).collect(), vec![1, 4])
        );
        assert_eq!(text_diff_rows(&lines, true), (vec![1, 2, 4], vec![0, 2]));
    }

    #[test]
    fn navigation_keeps_the_target_when_scroll_clamps_at_the_bottom() {
        let changes = [3, 20, 23];
        let mut navigation = TextDiffNavigation::default();
        assert_eq!(navigation.targets(&[]), (None, None));
        assert_eq!(navigation.targets(&changes), (None, Some(3)));

        navigation.update(3.0 * TEXT_ROW_HEIGHT, Some(3));
        assert_eq!(navigation.targets(&changes), (None, Some(20)));
        let bottom = 18.0 * TEXT_ROW_HEIGHT;
        navigation.update(bottom, Some(20));
        assert_eq!(navigation.targets(&changes), (Some(3), Some(23)));
        navigation.update(bottom, Some(23));
        assert_eq!(navigation.targets(&changes), (Some(20), None));
        navigation.update(bottom, Some(20));
        assert_eq!(navigation.targets(&changes), (Some(3), Some(23)));

        navigation.update(6.0 * TEXT_ROW_HEIGHT, None);
        assert_eq!(navigation.targets(&changes), (Some(3), Some(20)));
    }
}
