use crate::builtin_diff::{Cancellation, Column, DiffType, Display, Issue, Side, Snapshot};
use crate::diff::{self, Kind, Line, TextChanges, TextDocument};
use crate::logging::ReadLog;
use crate::theme::{self, ChangeColors, Palette};
use crate::ui::{compact_button, paint};
use crate::ui_diff::{Panel, column_rect};
use eframe::egui::{self, FontId, Rect, RichText, Sense, Stroke, Vec2, pos2};
const TEXT_ROW_HEIGHT: f32 = 20.0;

#[derive(Clone, Copy, Default)]
struct TextDiffNavigation {
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

#[derive(Default)]
pub(crate) struct TextView {
    navigation: TextDiffNavigation,
    scroll_version: u64,
}

impl DiffType for TextView {
    type Data = TextDocument;
    type Changes = TextChanges;

    fn load(snapshot: Snapshot<'_>) -> Result<Self::Data, Issue> {
        diff::load_text(snapshot)
    }

    fn compare(
        sides: [Option<&Self::Data>; 2],
        log: &ReadLog,
        _: &Cancellation,
    ) -> Option<Self::Changes> {
        Some(diff::compare_text(sides, log))
    }

    fn show(
        &mut self,
        ui: &mut egui::Ui,
        display: Display<'_, Self::Data, Self::Changes>,
        panel: &mut Panel<'_>,
    ) {
        self.show_text(
            ui,
            display
                .sides
                .each_ref()
                .map(|side| side.map(|doc| doc.as_ref())),
            display.changes,
            panel,
        );
    }
}

impl TextView {
    pub fn show_text(
        &mut self,
        ui: &mut egui::Ui,
        sides: [Side<&TextDocument>; 2],
        changes: Option<&TextChanges>,
        panel: &mut Panel<'_>,
    ) {
        let mut jump = None;
        let mut row_indices = Vec::new();
        let colors = theme::palette(ui.ctx());
        panel.controls(ui, |ui, changes_only| {
            let mut targets = (None, None);
            if let Some(changes) = changes {
                let (indices, blocks) = text_diff_rows(&changes.lines, *changes_only);
                row_indices = indices;
                targets = self.navigation.targets(&blocks);
            }
            if ui
                .add_enabled(targets.0.is_some(), compact_button("↑"))
                .on_hover_text("上一处变更")
                .clicked()
            {
                jump = targets.0;
            }
            if ui
                .add_enabled(targets.1.is_some(), compact_button("↓"))
                .on_hover_text("下一处变更")
                .clicked()
            {
                jump = targets.1;
            }
            let response = ui.add_enabled(
                changes.is_some(),
                egui::Checkbox::new(changes_only, "仅显示变更"),
            );
            if response.changed() {
                self.navigation = TextDiffNavigation::default();
                self.scroll_version += 1;
                jump = None;
                if let Some(changes) = changes {
                    row_indices = text_diff_rows(&changes.lines, *changes_only).0;
                }
            }
            if let Some(changes) = changes {
                let summary = if changes.added + changes.removed == 0 {
                    "文本内容相同".to_owned()
                } else {
                    format!("+{}  −{}", changes.added, changes.removed)
                };
                ui.label(RichText::new(summary).small().color(colors.muted));
            }
        });

        panel.body(ui, &sides, |ui, body| {
            let raw_count = sides
                .iter()
                .filter_map(Side::ready)
                .map(|text| text.lines.len())
                .max()
                .unwrap_or_default();
            let count = changes.map_or(raw_count, |_| row_indices.len());
            let focus = jump.or(self.navigation.focus);
            let mut scroll = egui::ScrollArea::vertical()
                .id_salt(("text-diff", self.scroll_version))
                .auto_shrink([false, false]);
            if let Some(row) = jump {
                scroll = scroll.vertical_scroll_offset(row as f32 * TEXT_ROW_HEIGHT);
            }
            let mut value = None;
            ui.spacing_mut().item_spacing.y = 0.0;
            let output = scroll.show_rows(ui, TEXT_ROW_HEIGHT, count, |ui, visible| {
                for index in visible {
                    let line = changes.map(|changes| &changes.lines[row_indices[index]]);
                    let (rect, response) = ui.allocate_exact_size(
                        Vec2::new(ui.available_width(), TEXT_ROW_HEIGHT),
                        Sense::click(),
                    );
                    let row = Rect::from_min_max(
                        pos2(body.left(), rect.top()),
                        pos2(body.right(), rect.bottom()),
                    );
                    for column in Column::BOTH {
                        let bounds = column_rect(row, column);
                        let (entry, change) = match (line, column) {
                            (Some(line), Column::Left) => (
                                line.old
                                    .as_ref()
                                    .map(|(number, text)| (*number, text.as_str())),
                                matches!(line.kind, Kind::Remove | Kind::Replace)
                                    .then_some((colors.removed, '−')),
                            ),
                            (Some(line), Column::Right) => (
                                line.new
                                    .as_ref()
                                    .map(|(number, text)| (*number, text.as_str())),
                                matches!(line.kind, Kind::Add | Kind::Replace)
                                    .then_some((colors.added, '+')),
                            ),
                            (None, _) => (
                                sides[column.index()]
                                    .ready()
                                    .and_then(|doc| doc.lines.get(index))
                                    .map(|text| (index + 1, text.as_str())),
                                None,
                            ),
                        };
                        if let Some(entry) = entry {
                            code(ui, bounds, entry, &colors, change);
                            if response.clicked()
                                && response
                                    .interact_pointer_pos()
                                    .is_some_and(|pointer| bounds.contains(pointer))
                            {
                                value = Some(entry.1.to_owned());
                            }
                        }
                    }
                    if focus == Some(index) {
                        ui.painter().rect_stroke(
                            row.shrink(0.5),
                            0.0,
                            Stroke::new(1.0_f32, colors.accent),
                            egui::StrokeKind::Inside,
                        );
                    }
                    response.on_hover_text("单击查看完整行");
                }
            });
            let offset = output.state.offset.y;
            if jump.is_some() || self.navigation.offset != offset {
                ui.ctx().request_repaint();
            }
            self.navigation.update(offset, jump);
            value
        });
    }
}
fn code(
    ui: &egui::Ui,
    rect: Rect,
    (number, text): (usize, &str),
    colors: &Palette,
    change: Option<(ChangeColors, char)>,
) {
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
