use crate::diff::{CellPosition, Kind, WorkbookDiff, WorksheetDiff, address, column_name};
use crate::theme;
use crate::ui::{fit_line, small_button};
use crate::ui_diff::diff_header;
use eframe::egui::{self, Align2, Color32, FontId, Rect, Sense, Stroke, Vec2, pos2, vec2};
use std::ops::Range;

const CELL_WIDTH: f32 = 144.0;
const ROW_HEIGHT: f32 = 22.0;
const ROW_HEADER: f32 = 47.0;
const COLUMN_HEADER: f32 = 22.0;
const PANE_GAP: f32 = 6.0;

#[derive(Default)]
pub(crate) struct SheetView {
    sheet: Option<usize>,
    change: usize,
}

impl SheetView {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        workbook: &WorkbookDiff,
        reset: u64,
        left: &str,
        right: &str,
    ) -> Option<String> {
        let colors = theme::palette(ui.ctx());
        if workbook.sheets.is_empty() {
            ui.label(egui::RichText::new("工作簿没有可显示的工作表").color(colors.muted));
            return None;
        }
        let mut selected = self
            .sheet
            .unwrap_or_else(|| {
                workbook
                    .sheets
                    .iter()
                    .position(|sheet| sheet.change_count() > 0 || sheet.kind != Kind::Equal)
                    .unwrap_or_default()
            })
            .min(workbook.sheets.len() - 1);
        let mut step = 0;
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("worksheet")
                .width(115.0)
                .selected_text(&workbook.sheets[selected].name)
                .show_ui(ui, |ui| {
                    for (index, sheet) in workbook.sheets.iter().enumerate() {
                        let label = match sheet.kind {
                            Kind::Add => format!("{} · 新增", sheet.name),
                            Kind::Remove => format!("{} · 删除", sheet.name),
                            _ => sheet.name.clone(),
                        };
                        ui.selectable_value(&mut selected, index, label);
                    }
                });
            let changes = workbook.sheets[selected].change_count();
            if changes > 0 {
                let current = if self.sheet == Some(selected) {
                    self.change
                } else {
                    0
                };
                let location = workbook.sheets[selected]
                    .change(current)
                    .map(|(row, column)| address(row, column))
                    .unwrap_or_default();
                if ui
                    .add_enabled(current > 0, crate::ui::compact_button("↑"))
                    .on_hover_text("上一处变化")
                    .clicked()
                {
                    step = -1;
                }
                if ui
                    .add_enabled(current + 1 < changes, crate::ui::compact_button("↓"))
                    .on_hover_text("下一处变化")
                    .clicked()
                {
                    step = 1;
                }
                if small_button(ui, &format!("{location}  {}/{changes}", current + 1))
                    .on_hover_text("定位当前变化")
                    .clicked()
                {
                    step = 2;
                }
            }
            ui.label(
                egui::RichText::new(format!("全簿 {} 处变化", workbook.total))
                    .small()
                    .color(colors.muted),
            );
            if let Some(status) = match workbook.sheets[selected].kind {
                Kind::Add => Some("工作表已新增"),
                Kind::Remove => Some("工作表已删除"),
                _ => None,
            } {
                ui.label(egui::RichText::new(status).small().color(colors.muted));
            }
        });

        let changed_sheet = self.sheet != Some(selected);
        self.sheet = Some(selected);
        if changed_sheet {
            self.change = 0;
        }
        match step {
            -1 => self.change = self.change.saturating_sub(1),
            1 => self.change += 1,
            _ => {}
        }
        let sheet = &workbook.sheets[selected];
        let focus = sheet.change(self.change);
        let jump = (changed_sheet || step != 0).then_some(focus).flatten();
        diff_header(ui, &format!("旧版 · {left}"), &format!("新版 · {right}"));
        render_grid(ui, sheet, reset, focus, jump)
    }
}

fn render_grid(
    ui: &mut egui::Ui,
    sheet: &WorksheetDiff,
    reset: u64,
    focus: Option<CellPosition>,
    jump: Option<CellPosition>,
) -> Option<String> {
    let mut scroll = egui::ScrollArea::both()
        .id_salt(("worksheet-grid", reset, &sheet.name))
        .auto_shrink([false, false])
        .min_scrolled_width(0.0)
        .min_scrolled_height(0.0);
    if let Some((row, col)) = jump {
        scroll = scroll.scroll_offset(vec2(
            col.saturating_sub(1) as f32 * CELL_WIDTH,
            row.saturating_sub(2) as f32 * ROW_HEIGHT,
        ));
    }
    scroll
        .show_viewport(ui, |ui, viewport| {
            let view = viewport.translate(ui.max_rect().min.to_vec2());
            let pane_width = ((view.width() - PANE_GAP) * 0.5).max(0.0);
            let data_width = (pane_width - ROW_HEADER).max(0.0);
            let body_height = (view.height() - COLUMN_HEADER).max(0.0);
            let extra_width = (sheet.columns as f32 * CELL_WIDTH - data_width).max(0.0);
            let extra_height = (sheet.rows as f32 * ROW_HEIGHT - body_height).max(0.0);
            ui.set_min_size(view.size() + vec2(extra_width, extra_height));

            let visible = GridViewport {
                offset: viewport.min.to_vec2(),
                rows: visible_range(viewport.min.y, body_height, ROW_HEIGHT),
                columns: visible_range(viewport.min.x, data_width, CELL_WIDTH),
            };
            let pane_size = vec2(pane_width, view.height());
            let left = Rect::from_min_size(view.min, pane_size);
            let right = Rect::from_min_size(pos2(view.right() - pane_width, view.top()), pane_size);
            for (side, pane) in [left, right].into_iter().enumerate() {
                paint_pane(ui, sheet, pane, &visible, side, focus);
            }

            let response = ui.interact(view, ui.id().with("worksheet-cell"), Sense::click());
            let pointer = response
                .interact_pointer_pos()
                .filter(|_| response.clicked())?;
            let pane = [left, right]
                .into_iter()
                .find(|pane| pane.contains(pointer))?;
            let cell = pointer - pane.min - vec2(ROW_HEADER, COLUMN_HEADER);
            if cell.x < 0.0 || cell.y < 0.0 {
                return None;
            }
            let position = (
                ((cell.y + visible.offset.y) / ROW_HEIGHT) as u32,
                ((cell.x + visible.offset.x) / CELL_WIDTH) as u32,
            );
            let (old, new) = sheet.cell(position)?.sides();
            Some(format!(
                "{} · {}\n旧：{}\n新：{}",
                sheet.name,
                address(position.0, position.1),
                old.unwrap_or_default(),
                new.unwrap_or_default(),
            ))
        })
        .inner
}

struct GridViewport {
    offset: Vec2,
    rows: Range<u32>,
    columns: Range<u32>,
}

fn visible_range(offset: f32, length: f32, cell_size: f32) -> Range<u32> {
    (offset / cell_size).floor() as u32..((offset + length) / cell_size).ceil() as u32
}

fn paint_pane(
    ui: &egui::Ui,
    sheet: &WorksheetDiff,
    pane: Rect,
    visible: &GridViewport,
    side: usize,
    focus: Option<CellPosition>,
) {
    let colors = theme::palette(ui.ctx());
    let root = ui.painter().with_clip_rect(ui.clip_rect().intersect(pane));
    let data_origin = pane.min + vec2(ROW_HEADER, COLUMN_HEADER);
    let body = Rect::from_min_max(data_origin, pane.max);
    let grid = root.with_clip_rect(body);
    let row_font = FontId::monospace(11.0);
    let line = Stroke::new(1.0_f32, colors.border);
    root.rect_filled(pane, 0.0, colors.base);
    root.rect_filled(
        Rect::from_min_max(pane.min, pos2(pane.right(), data_origin.y)),
        0.0,
        colors.control,
    );
    root.rect_filled(
        Rect::from_min_max(pane.min, pos2(data_origin.x, pane.bottom())),
        0.0,
        colors.control,
    );
    let row_gutter = root.with_clip_rect(Rect::from_min_max(
        pos2(pane.left(), data_origin.y),
        pos2(data_origin.x, pane.bottom()),
    ));
    let column_gutter = root.with_clip_rect(Rect::from_min_max(
        pos2(data_origin.x, pane.top()),
        pos2(pane.right(), data_origin.y),
    ));
    let origin = data_origin - visible.offset;
    let change = if side == 0 {
        colors.removed
    } else {
        colors.added
    };
    for column in visible.columns.clone() {
        let x = origin.x + column as f32 * CELL_WIDTH;
        column_gutter.text(
            pos2(x + CELL_WIDTH * 0.5, pane.top() + COLUMN_HEADER * 0.5),
            Align2::CENTER_CENTER,
            column_name(column),
            row_font.clone(),
            colors.muted,
        );
        column_gutter.vline(x + CELL_WIDTH, pane.top()..=data_origin.y, line);
    }
    for row in visible.rows.clone() {
        let y = origin.y + row as f32 * ROW_HEIGHT;
        row_gutter.text(
            pos2(data_origin.x - 7.0, y + ROW_HEIGHT * 0.5),
            Align2::RIGHT_CENTER,
            (u64::from(row) + 1).to_string(),
            row_font.clone(),
            colors.muted,
        );
        row_gutter.hline(pane.left()..=data_origin.x, y + ROW_HEIGHT, line);
        for col in visible.columns.clone() {
            let x = origin.x + col as f32 * CELL_WIDTH;
            let rect = Rect::from_min_size(pos2(x, y), vec2(CELL_WIDTH, ROW_HEIGHT));
            let position = (row, col);
            grid.hline(rect.x_range(), rect.bottom(), line);
            grid.vline(rect.right(), rect.y_range(), line);
            if let Some(values) = sheet.cell(position) {
                let (old, new) = values.sides();
                let value = [old, new][side];
                if let Some(value) = value {
                    let color = if values.is_changed() {
                        grid.rect_filled(rect, 0.0, change.background);
                        grid.rect_stroke(
                            rect,
                            0.0,
                            Stroke::new(1.0_f32, change.border),
                            egui::StrokeKind::Inside,
                        );
                        change.foreground
                    } else {
                        colors.text
                    };
                    paint_value(&grid, rect, value, color);
                }
            }
            if focus == Some(position) {
                grid.rect_stroke(
                    rect.shrink(0.5),
                    0.0,
                    Stroke::new(1.0_f32, colors.accent),
                    egui::StrokeKind::Inside,
                );
            }
        }
    }
    root.hline(pane.x_range(), data_origin.y, line);
    root.vline(data_origin.x, pane.y_range(), line);
}

fn paint_value(painter: &egui::Painter, rect: Rect, value: &str, color: Color32) {
    let line = value.lines().next().unwrap_or_default();
    let text = fit_line(
        painter,
        line.to_owned(),
        FontId::proportional(12.0),
        color,
        rect.width() - 12.0,
    );
    let y = rect.center().y - text.size().y * 0.5;
    painter
        .with_clip_rect(rect)
        .galley(pos2(rect.left() + 6.0, y), text, Color32::PLACEHOLDER);
}
