use crate::app::Desk;
use crate::diff::{Content, Kind, Line, is_workbook};
use crate::image_diff::{ImageDiff, ImageSide};
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
        let image_view_id = ui.id().with("image-diff-view");
        let mut image_view = ui
            .data(|data| data.get_temp::<ImageView>(image_view_id))
            .filter(|state| state.reset == self.diff_reset)
            .unwrap_or_default();
        if let Some(Content::Image(images)) = &mut self.content {
            images.left.upload(ctx);
            images.right.upload(ctx);
        }
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
                } else if matches!(self.content, Some(Content::LoadingWorkbook)) {
                    ui.add_enabled(false, compact_button("比较原始文件"));
                } else if workbook && self.force_text {
                    toggle = small_button(ui, "单元格对比").clicked();
                }
                if let Some(Content::Image(images)) = &self.content {
                    let (label, hint) = if image_view.actual_size {
                        ("适应窗格", "以相同比例显示两张完整图片")
                    } else {
                        ("100%", "查看原始像素尺寸，拖动图片可同步平移")
                    };
                    if ui
                        .add_enabled(
                            image_canvas_size(images) != Vec2::ZERO,
                            compact_button(label),
                        )
                        .on_hover_text(hint)
                        .clicked()
                    {
                        image_view.actual_size = !image_view.actual_size;
                        image_view.pan = Vec2::ZERO;
                    }
                }
                if matches!(
                    self.content,
                    Some(Content::Text { .. } | Content::LoadingText)
                ) {
                    save = ui
                        .add_enabled(
                            matches!(self.content, Some(Content::Text { .. })),
                            egui::Checkbox::new(&mut self.settings.changes_only, "仅显示变更"),
                        )
                        .changed();
                    if save {
                        navigation = TextDiffNavigation::default();
                    }
                    let (previous, next) = if let Some(Content::Text { lines, .. }) = &self.content
                    {
                        let (indices, changes) = text_diff_rows(lines, self.settings.changes_only);
                        text_rows = Some(indices);
                        navigation.targets(&changes)
                    } else {
                        (None, None)
                    };
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
        let Some(pair) = self.comparison.as_ref() else {
            return;
        };
        let mut full_line = None;
        if let Some(content) = self.content.as_ref() {
            match (content, text_rows) {
                (Content::LoadingText | Content::LoadingWorkbook, _) => {
                    let left = pair.left.title();
                    let right = pair.right.title();
                    if matches!(content, Content::LoadingWorkbook) {
                        crate::ui_workbook::pending_header(ui, left, right);
                    } else {
                        diff_header(
                            ui,
                            &format!("旧版本  ·  {left}"),
                            &format!("新版本  ·  {right}"),
                        );
                    }
                    if let Some(error) = &self.diff_error {
                        ui.colored_label(colors.removed.foreground, error);
                    } else {
                        let (rect, _) = ui.allocate_exact_size(
                            ui.available_size().max(Vec2::ZERO),
                            Sense::hover(),
                        );
                        paint_loading(ui, rect);
                    }
                }
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
                (Content::Image(images), _) => {
                    image_view.show(ui, images, pair.left.title(), pair.right.title());
                    image_view.reset = self.diff_reset;
                    ui.data_mut(|data| data.insert_temp(image_view_id, image_view));
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

#[derive(Clone, Copy, Default)]
struct ImageView {
    reset: u64,
    actual_size: bool,
    pan: Vec2,
}

impl ImageView {
    fn show(&mut self, ui: &mut egui::Ui, images: &ImageDiff, before: &str, after: &str) {
        let title = |side: &str, version: &str, image: &ImageSide| match image {
            ImageSide::Preview(texture) => {
                let [width, height] = texture.size();
                format!("{side}  ·  {version}     {width} × {height} px")
            }
            ImageSide::Absent => side.to_owned(),
            _ => format!("{side}  ·  {version}"),
        };
        diff_header(
            ui,
            &title("旧版本", before, &images.left),
            &title("新版本", after, &images.right),
        );

        let body = ui.available_size().max(Vec2::ZERO);
        let viewport = Vec2::new(body.x / 2.0 - 16.0, body.y - 16.0).max(Vec2::ZERO);
        let canvas = image_canvas_size(images);
        let pixels_per_point = ui.ctx().pixels_per_point();
        let mut scale = 1.0 / pixels_per_point;
        if !self.actual_size && canvas != Vec2::ZERO {
            scale = scale.min((viewport.x / canvas.x).min(viewport.y / canvas.y));
        }
        let canvas = canvas * scale;
        let overflow = (canvas - viewport).max(Vec2::ZERO);
        let can_pan = self.actual_size && overflow != Vec2::ZERO;
        let sense = if can_pan {
            Sense::drag()
        } else {
            Sense::hover()
        };
        let (rect, response) = ui.allocate_exact_size(body, sense);
        if can_pan {
            self.pan = (self.pan - response.drag_delta()).clamp(Vec2::ZERO, overflow);
            let cursor = if response.dragged() {
                egui::CursorIcon::Grabbing
            } else {
                egui::CursorIcon::Grab
            };
            response.on_hover_cursor(cursor);
        }

        let colors = theme::palette(ui.ctx());
        let (left, right) = halves(rect);
        ui.painter().vline(
            right.left(),
            rect.y_range(),
            Stroke::new(1.0_f32, colors.divider),
        );
        if viewport.x <= 0.0 || viewport.y <= 0.0 {
            return;
        }
        let inset = (viewport - canvas).max(Vec2::ZERO) * 0.5;
        for (column, image) in [(left, &images.left), (right, &images.right)] {
            let bounds = column.shrink(8.0);
            let origin = bounds.min + inset - self.pan;
            let aligned = pos2(
                (origin.x * pixels_per_point).round() / pixels_per_point,
                (origin.y * pixels_per_point).round() / pixels_per_point,
            );
            paint_image(ui, bounds, image, aligned, scale, &colors);
        }
    }
}

fn image_canvas_size(images: &ImageDiff) -> Vec2 {
    [&images.left, &images.right]
        .into_iter()
        .filter_map(|side| match side {
            ImageSide::Preview(texture) => Some(texture.size_vec2()),
            _ => None,
        })
        .fold(Vec2::ZERO, Vec2::max)
}

fn paint_image(
    ui: &egui::Ui,
    bounds: Rect,
    side: &ImageSide,
    origin: egui::Pos2,
    scale: f32,
    colors: &Palette,
) {
    let painter = ui.painter().with_clip_rect(bounds);
    let (message, color) = match side {
        ImageSide::Preview(texture) => {
            let rect = Rect::from_min_size(origin, texture.size_vec2() * scale);
            let visible = rect.intersect(painter.clip_rect());
            if visible.is_positive() {
                checkerboard(&painter, visible, origin, colors);
                let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
                painter.image(texture.id(), rect, uv, egui::Color32::WHITE);
            }
            return;
        }
        ImageSide::Loading => {
            paint_loading(ui, bounds);
            return;
        }
        ImageSide::Absent => ("此版本无图片", colors.muted),
        ImageSide::Unavailable(message) => (message.as_str(), colors.warning),
        ImageSide::Pixels(_) => return,
    };
    let text = painter.layout(
        message.to_owned(),
        FontId::proportional(13.0),
        color,
        bounds.width(),
    );
    painter.galley(bounds.center() - text.size() * 0.5, text, color);
}

fn paint_loading(ui: &egui::Ui, bounds: Rect) {
    let color = theme::palette(ui.ctx()).muted;
    let painter = ui.painter().with_clip_rect(bounds);
    let text = painter.layout_no_wrap("加载中…".into(), FontId::proportional(13.0), color);
    let mut position = bounds.center() - text.size() * 0.5;
    const SPINNER_SIZE: f32 = 14.0;
    const SPINNER_GAP: f32 = 7.0;
    if bounds.width() >= text.size().x + SPINNER_SIZE + SPINNER_GAP
        && bounds.height() >= SPINNER_SIZE
    {
        position.x += (SPINNER_SIZE + SPINNER_GAP) * 0.5;
        let center = pos2(
            position.x - SPINNER_GAP - SPINNER_SIZE * 0.5,
            bounds.center().y,
        );
        egui::Spinner::new().color(color).paint_at(
            ui,
            Rect::from_center_size(center, Vec2::splat(SPINNER_SIZE)),
        );
    }
    painter.galley(position, text, color);
}

fn checkerboard(painter: &egui::Painter, visible: Rect, origin: egui::Pos2, colors: &Palette) {
    const CELL: f32 = 8.0;
    let start = (visible.min - origin) / CELL;
    let end = (visible.max - origin) / CELL;
    let mut mesh = egui::Mesh::default();
    mesh.add_colored_rect(visible, colors.control);
    for row in start.y.floor() as i32..end.y.ceil() as i32 {
        for col in start.x.floor() as i32..end.x.ceil() as i32 {
            if (row + col) % 2 == 0 {
                let cell = origin + Vec2::new(col as f32, row as f32) * CELL;
                let tile = Rect::from_min_size(cell, Vec2::splat(CELL)).intersect(visible);
                mesh.add_colored_rect(tile, colors.border);
            }
        }
    }
    painter.add(egui::Shape::mesh(mesh));
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
