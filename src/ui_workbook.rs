use crate::builtin_diff::{Cancellation, Column, DiffType, Display, Issue, Side, Snapshot};
use crate::diff::{
    self, CellPosition, Kind, TextChanges, WorkbookDiff, WorkbookSide, Worksheet, WorksheetDiff,
    address, column_name,
};
use crate::logging::ReadLog;
use crate::theme;
use crate::ui::{compact_button, fit_line, paint, small_button};
use crate::ui_diff::{Panel, column_rect, paint_message};
use crate::ui_text::TextView;
use eframe::egui::{self, Align2, Color32, FontId, Rect, Sense, Stroke, Vec2, pos2, vec2};
use std::borrow::Cow;
use std::ops::Range;

const CELL_WIDTH: f32 = 144.0;
const ROW_HEIGHT: f32 = 22.0;
const ROW_HEADER: f32 = 47.0;
const COLUMN_HEADER: f32 = 22.0;

#[derive(Default)]
pub(crate) struct SheetView {
    sheet: Option<String>,
    change: usize,
    selected_by_user: bool,
    compared: bool,
    pointer_view: TextView,
}

pub(crate) enum Comparison {
    Cells(WorkbookDiff),
    Pointers(TextChanges),
}

impl DiffType for SheetView {
    type Data = WorkbookSide;
    type Changes = Comparison;

    fn load(snapshot: Snapshot<'_>) -> Result<Self::Data, Issue> {
        diff::load_workbook(snapshot)
    }

    fn compare(
        sides: [Option<&Self::Data>; 2],
        log: &ReadLog,
        cancel: &Cancellation,
    ) -> Option<Comparison> {
        if sides
            .iter()
            .flatten()
            .all(|side| matches!(side, WorkbookSide::Book(_)))
        {
            let books = sides.map(|side| match side {
                Some(WorkbookSide::Book(book)) => Some(book.as_slice()),
                _ => None,
            });
            diff::compare_workbooks(books, || cancel.requested()).map(Comparison::Cells)
        } else if sides
            .iter()
            .flatten()
            .all(|side| matches!(side, WorkbookSide::Pointer(_)))
        {
            Some(Comparison::Pointers(diff::compare_text(
                sides.map(|side| match side {
                    Some(WorkbookSide::Pointer(text)) => Some(text),
                    _ => None,
                }),
                log,
            )))
        } else {
            None
        }
    }

    fn show(
        &mut self,
        ui: &mut egui::Ui,
        display: Display<'_, Self::Data, Self::Changes>,
        panel: &mut Panel<'_>,
    ) {
        let sides = display
            .sides
            .each_ref()
            .map(|side| side.map(|workbook| workbook.as_ref()));
        let books = sides.each_ref().map(|side| match side.ready() {
            Some(WorkbookSide::Book(book)) => Some(book),
            _ => None,
        });
        if books.iter().all(Option::is_none)
            && sides
                .iter()
                .any(|side| matches!(side.ready(), Some(WorkbookSide::Pointer(_))))
        {
            let pointers = sides.each_ref().map(|side| match side {
                Side::Ready(WorkbookSide::Pointer(text)) => Side::Ready(text),
                Side::Ready(WorkbookSide::Book(_)) => unreachable!("no workbook grids"),
                Side::Absent => Side::Absent,
                Side::Loading => Side::Loading,
                Side::Failed(issue) => Side::Failed(*issue),
            });
            let changes = match display.changes {
                Some(Comparison::Pointers(changes)) => Some(changes),
                _ => None,
            };
            self.pointer_view.show_text(ui, pointers, changes, panel);
            return;
        }

        let changes = match display.changes {
            Some(Comparison::Cells(changes)) => Some(changes),
            _ => None,
        };
        let mut names = Vec::new();
        if let Some(changes) = changes {
            names.extend(
                changes
                    .sheets
                    .iter()
                    .map(|sheet| (sheet.name.as_str(), sheet.kind)),
            );
        } else {
            for book in books.iter().rev().flatten() {
                for sheet in *book {
                    if !names.iter().any(|&(name, _)| name == sheet.name) {
                        names.push((sheet.name.as_str(), Kind::Equal));
                    }
                }
            }
        }

        let default = changes
            .and_then(|changes| {
                changes
                    .sheets
                    .iter()
                    .position(|sheet| sheet.change_count() > 0 || sheet.kind != Kind::Equal)
            })
            .unwrap_or_default();
        let completed_now = changes.is_some() && !self.compared;
        self.compared = changes.is_some();
        let mut selected = self
            .sheet
            .as_deref()
            .and_then(|name| names.iter().position(|&(candidate, _)| candidate == name))
            .unwrap_or(default);
        if completed_now && !self.selected_by_user {
            selected = default;
        }

        let colors = theme::palette(ui.ctx());
        let mut step = 0;
        panel.controls(ui, |ui, _| {
            let Some(&(name, _)) = names.get(selected) else {
                ui.add_enabled_ui(false, |ui| {
                    worksheet_picker()
                        .selected_text("工作表")
                        .show_ui(ui, |_| {});
                });
                return;
            };
            worksheet_picker().selected_text(name).show_ui(ui, |ui| {
                for (index, &(name, kind)) in names.iter().enumerate() {
                    let label = match kind {
                        Kind::Add => format!("{name} · 新增"),
                        Kind::Remove => format!("{name} · 删除"),
                        _ => name.to_owned(),
                    };
                    self.selected_by_user |=
                        ui.selectable_value(&mut selected, index, label).clicked();
                }
            });
            if let Some(changes) = changes {
                let sheet = &changes.sheets[selected];
                let count = sheet.change_count();
                if count > 0 {
                    let current = if self.sheet.as_deref() == Some(&sheet.name) {
                        self.change
                    } else {
                        0
                    };
                    let location = sheet
                        .change(current)
                        .map(|(row, column)| address(row, column))
                        .unwrap_or_default();
                    if ui
                        .add_enabled(current > 0, compact_button("↑"))
                        .on_hover_text("上一处变化")
                        .clicked()
                    {
                        step = -1;
                    }
                    if ui
                        .add_enabled(current + 1 < count, compact_button("↓"))
                        .on_hover_text("下一处变化")
                        .clicked()
                    {
                        step = 1;
                    }
                    if small_button(ui, &format!("{location}  {}/{count}", current + 1))
                        .on_hover_text("定位当前变化")
                        .clicked()
                    {
                        step = 2;
                    }
                }
                ui.label(
                    egui::RichText::new(format!("全簿 {} 处变化", changes.total))
                        .small()
                        .color(colors.muted),
                );
            }
        });

        let name = names.get(selected).map(|&(name, _)| name);
        let changed_sheet = self.sheet.as_deref() != name;
        if changed_sheet {
            self.sheet = name.map(str::to_owned);
            self.change = 0;
        }
        if step == -1 {
            self.change = self.change.saturating_sub(1);
        } else if step == 1 {
            self.change += 1;
        }
        let sheets =
            books.map(|book| name.and_then(|name| book?.iter().find(|sheet| sheet.name == name)));
        let compared_sheet = changes.and_then(|changes| changes.sheets.get(selected));
        let focus = compared_sheet.and_then(|sheet| sheet.change(self.change));
        let jump = if completed_now || changed_sheet || step != 0 {
            focus.or_else(|| {
                sheets
                    .iter()
                    .flatten()
                    .filter_map(|sheet| sheet.first_cell())
                    .min()
            })
        } else {
            None
        };

        panel.body(ui, &sides, |ui, body| {
            let grid = if let Some(name) = name {
                Grid {
                    name,
                    raw: sheets,
                    changes: compared_sheet,
                }
                .show(ui, body, focus, jump)
            } else {
                None
            };
            for column in Column::BOTH {
                let index = column.index();
                let bounds = column_rect(body, column);
                match sides[index].ready() {
                    Some(WorkbookSide::Book(book)) if sheets[index].is_none() => {
                        let message = if book.is_empty() {
                            "工作簿没有可显示的工作表"
                        } else {
                            "此版本无此工作表"
                        };
                        paint_message(ui, bounds, message, false);
                    }
                    Some(WorkbookSide::Pointer(doc)) => {
                        for (index, line) in doc.lines.iter().take(8).enumerate() {
                            let row = Rect::from_min_size(
                                bounds.min + vec2(0.0, index as f32 * ROW_HEIGHT),
                                vec2(bounds.width(), ROW_HEIGHT),
                            )
                            .intersect(bounds);
                            if row.is_positive() {
                                paint(
                                    ui,
                                    row,
                                    row.left() + 8.0,
                                    line,
                                    FontId::monospace(12.0),
                                    colors.text,
                                );
                            }
                        }
                    }
                    _ => {}
                }
            }
            grid
        });
    }
}

fn worksheet_picker() -> egui::ComboBox {
    egui::ComboBox::from_id_salt("worksheet").width(115.0)
}

struct Grid<'a> {
    name: &'a str,
    raw: [Option<&'a Worksheet>; 2],
    changes: Option<&'a WorksheetDiff>,
}

impl Grid<'_> {
    fn dimensions(&self) -> (u32, u32) {
        self.raw
            .iter()
            .flatten()
            .fold((0, 0), |(rows, cols), sheet| {
                (rows.max(sheet.rows), cols.max(sheet.columns))
            })
    }

    fn value(&self, position: CellPosition, column: Column) -> Option<(Cow<'_, str>, bool)> {
        if let Some(sheet) = self.changes {
            let values = sheet.cell(position)?;
            let (old, new) = values.sides();
            [old, new][column.index()].map(|value| (Cow::Borrowed(value), values.is_changed()))
        } else {
            self.raw[column.index()]?
                .cell(position)
                .map(|value| (value, false))
        }
    }

    fn show(
        &self,
        ui: &mut egui::Ui,
        body: Rect,
        focus: Option<CellPosition>,
        jump: Option<CellPosition>,
    ) -> Option<String> {
        let mut scroll = egui::ScrollArea::both()
            .id_salt(("worksheet-grid", self.name))
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
                let viewport_rect = viewport.translate(ui.max_rect().min.to_vec2());
                let view = Rect::from_min_max(
                    pos2(body.left(), viewport_rect.top()),
                    pos2(body.right(), viewport_rect.bottom()),
                );
                let pane_width = view.width() * 0.5;
                let data_width = (pane_width - ROW_HEADER).max(0.0);
                let body_height = (view.height() - COLUMN_HEADER).max(0.0);
                let (rows, columns) = self.dimensions();
                let extra_width = (columns as f32 * CELL_WIDTH - data_width).max(0.0);
                let extra_height = (rows as f32 * ROW_HEIGHT - body_height).max(0.0);
                ui.set_min_size(viewport_rect.size() + vec2(extra_width, extra_height));

                let visible = GridViewport {
                    offset: viewport.min.to_vec2(),
                    rows: visible_range(viewport.min.y, body_height, ROW_HEIGHT),
                    columns: visible_range(viewport.min.x, data_width, CELL_WIDTH),
                };
                for column in Column::BOTH {
                    if self.raw[column.index()].is_some() {
                        paint_pane(ui, self, column_rect(view, column), &visible, column, focus);
                    }
                }

                let response = ui.interact(view, ui.id().with("worksheet-cell"), Sense::click());
                let pointer = response
                    .interact_pointer_pos()
                    .filter(|_| response.clicked())?;
                let column = if pointer.x < view.center().x {
                    Column::Left
                } else {
                    Column::Right
                };
                self.raw[column.index()]?;
                let pane = column_rect(view, column);
                let cell = pointer - pane.min - vec2(ROW_HEADER, COLUMN_HEADER);
                if cell.x < 0.0 || cell.y < 0.0 {
                    return None;
                }
                let position = (
                    ((cell.y + visible.offset.y) / ROW_HEIGHT) as u32,
                    ((cell.x + visible.offset.x) / CELL_WIDTH) as u32,
                );
                let [old, new] =
                    Column::BOTH.map(|column| self.value(position, column).map(|(value, _)| value));
                if old.is_none() && new.is_none() {
                    return None;
                }
                Some(format!(
                    "{} · {}\n旧：{}\n新：{}",
                    self.name,
                    address(position.0, position.1),
                    old.unwrap_or_default(),
                    new.unwrap_or_default(),
                ))
            })
            .inner
    }
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
    sheet: &Grid<'_>,
    pane: Rect,
    visible: &GridViewport,
    column: Column,
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
    let change = if column == Column::Left {
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
            if let Some((value, changed)) = sheet.value(position, column) {
                let color = if changed {
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
                paint_value(&grid, rect, &value, color);
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
