use crate::builtin_diff::{Column, DiffType, Display, Issue, Side, Snapshot};
use crate::image_diff;
use crate::theme::{self, Palette};
use crate::ui::compact_button;
use crate::ui_diff::{Panel, column_rect};
use eframe::egui::{self, FontId, Rect, Sense, Vec2, pos2};
use std::sync::Arc;

#[derive(Default)]
pub(crate) struct ImageView {
    actual_size: bool,
    pan: Vec2,
    textures: [Option<egui::TextureHandle>; 2],
}

impl DiffType for ImageView {
    type Data = egui::ColorImage;
    type Changes = ();

    fn load(snapshot: Snapshot<'_>) -> Result<Self::Data, Issue> {
        image_diff::load(snapshot)
    }

    fn prepare(
        &mut self,
        ctx: &egui::Context,
        column: Column,
        pixels: &Arc<Self::Data>,
    ) -> Result<(), Issue> {
        let max_edge = ctx.input(|input| input.max_texture_side);
        if pixels.size.iter().any(|&size| size > max_edge) {
            return Err(Issue::GraphicsLimit);
        }
        self.textures[column.index()] = Some(ctx.load_texture(
            "image-diff",
            egui::ImageData::Color(Arc::clone(pixels)),
            egui::TextureOptions::LINEAR,
        ));
        Ok(())
    }

    fn show(
        &mut self,
        ui: &mut egui::Ui,
        display: Display<'_, Self::Data, Self::Changes>,
        panel: &mut Panel<'_>,
    ) {
        let colors = theme::palette(ui.ctx());
        panel.controls(ui, |ui, _| {
            let (label, hint) = if self.actual_size {
                ("适应窗格", "以相同比例显示两张完整图片")
            } else {
                ("100%", "查看原始像素尺寸，拖动图片可同步平移")
            };
            if ui
                .add_enabled(
                    self.textures.iter().any(Option::is_some),
                    compact_button(label),
                )
                .on_hover_text(hint)
                .clicked()
            {
                self.actual_size = !self.actual_size;
                self.pan = Vec2::ZERO;
            }
        });
        panel.body(ui, display.sides, |ui, body| {
            self.show_images(ui, body, display.sides, &colors);
            None
        });
    }
}

impl ImageView {
    fn show_images(
        &mut self,
        ui: &mut egui::Ui,
        body: Rect,
        sides: &[Side<Arc<egui::ColorImage>>; 2],
        colors: &Palette,
    ) {
        const INSET: f32 = 8.0;
        const SIZE_LABEL: f32 = 19.0;
        let viewport = Vec2::new(
            body.width() * 0.5 - INSET * 2.0,
            body.height() - SIZE_LABEL - INSET * 2.0,
        );
        if viewport.x <= 0.0 || viewport.y <= 0.0 {
            return;
        }
        let canvas = self
            .textures
            .iter()
            .flatten()
            .map(egui::TextureHandle::size_vec2)
            .fold(Vec2::ZERO, Vec2::max);
        let pixels_per_point = ui.ctx().pixels_per_point();
        let mut scale = 1.0 / pixels_per_point;
        if !self.actual_size && canvas != Vec2::ZERO {
            scale = scale.min((viewport.x / canvas.x).min(viewport.y / canvas.y));
        }
        let canvas = canvas * scale;
        let overflow = (canvas - viewport).max(Vec2::ZERO);
        self.pan = self.pan.clamp(Vec2::ZERO, overflow);
        let inset = (viewport - canvas).max(Vec2::ZERO) * 0.5;
        for column in Column::BOTH {
            let index = column.index();
            if let Some(pixels) = sides[index].ready() {
                let pane = column_rect(body, column);
                let mut bounds = pane.shrink(INSET);
                let [width, height] = pixels.size;
                let painter = ui.painter().with_clip_rect(pane);
                painter.text(
                    bounds.center_top(),
                    egui::Align2::CENTER_TOP,
                    format!("{width} × {height} px"),
                    FontId::proportional(12.0),
                    colors.muted,
                );
                bounds.min.y += SIZE_LABEL;
                if self.actual_size && overflow != Vec2::ZERO {
                    let response =
                        ui.interact(bounds, ui.id().with(("image-pan", index)), Sense::drag());
                    self.pan = (self.pan - response.drag_delta()).clamp(Vec2::ZERO, overflow);
                    let cursor = if response.dragged() {
                        egui::CursorIcon::Grabbing
                    } else {
                        egui::CursorIcon::Grab
                    };
                    response.on_hover_cursor(cursor);
                }
            }
        }
        for column in Column::BOTH {
            if let Some(texture) = &self.textures[column.index()] {
                let mut bounds = column_rect(body, column).shrink(INSET);
                bounds.min.y += SIZE_LABEL;
                let origin = bounds.min + inset - self.pan;
                let aligned = pos2(
                    (origin.x * pixels_per_point).round() / pixels_per_point,
                    (origin.y * pixels_per_point).round() / pixels_per_point,
                );
                let painter = ui.painter().with_clip_rect(bounds);
                let rect = Rect::from_min_size(aligned, texture.size_vec2() * scale);
                let visible = rect.intersect(painter.clip_rect());
                if visible.is_positive() {
                    checkerboard(&painter, visible, aligned, colors);
                    let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
                    painter.image(texture.id(), rect, uv, egui::Color32::WHITE);
                }
            }
        }
    }
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
