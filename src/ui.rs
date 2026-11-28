use crate::app::{Desk, Dialog};
use crate::git::Operation;
use crate::theme::{self, Theme};
use eframe::egui::{
    self, Align, Align2, Color32, FontId, Frame, Galley, Layout, Margin, Painter, Rect, RichText,
    Sense, Stroke, UiBuilder, Vec2, emath::GuiRounding, pos2, text::LayoutJob,
};
use std::path::PathBuf;

#[cfg(target_os = "macos")]
mod macos_fonts;

pub(crate) const ROW: f32 = 25.0;
pub(crate) const DETAIL_SIZE: f32 = 12.0;
pub(crate) const CONTROL_HEIGHT: f32 = 26.0;
const TOOLBAR_HEIGHT: f32 = 40.0;
pub(crate) const TITLE_HEIGHT: f32 = 34.0;
const TITLE_BUTTON_HEIGHT: f32 = 24.0;
const TITLE_BUTTON_MIN_WIDTH: f32 = 25.0;
pub(crate) const WINDOW_BUTTON_WIDTH: f32 = 46.0;
const SPLITTER_HEIGHT: f32 = 8.0;

pub(crate) struct WindowButtons {
    pub maximized: bool,
    pub maximize_hovered: bool,
    pub maximize_pressed: bool,
}

pub(crate) struct ToolbarRegions {
    pub drag: Rect,
    pub controls: [Rect; 3],
    pub maximize: Option<Rect>,
}

pub fn install_style(ctx: &egui::Context) {
    egui_extras::install_image_loaders(ctx);
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = Vec2::new(8.0, 5.0);
        style.spacing.button_padding = Vec2::new(9.0, 4.0);
        style.spacing.interact_size = Vec2::new(36.0, CONTROL_HEIGHT);
        for (role, size) in [
            (egui::TextStyle::Body, 14.0),
            (egui::TextStyle::Button, 13.0),
            (egui::TextStyle::Small, 11.0),
            (egui::TextStyle::Heading, 21.0),
        ] {
            style.text_styles.insert(role, FontId::proportional(size));
        }
    });
    let mut fonts = egui::FontDefinitions::default();
    if let Some(windows) = std::env::var_os("WINDIR") {
        let path = PathBuf::from(windows).join("Fonts");
        for (name, file, family, first) in [
            ("segoe", "segoeui.ttf", egui::FontFamily::Proportional, true),
            ("consolas", "consola.ttf", egui::FontFamily::Monospace, true),
        ] {
            if let Ok(bytes) = std::fs::read(path.join(file)) {
                fonts
                    .font_data
                    .insert(name.into(), egui::FontData::from_owned(bytes).into());
                if let Some(list) = fonts.families.get_mut(&family) {
                    if first {
                        list.insert(0, name.into());
                    } else {
                        list.push(name.into());
                    }
                }
            }
        }
        if let Ok(bytes) = std::fs::read(path.join("msyh.ttc")) {
            fonts
                .font_data
                .insert("yahei".into(), egui::FontData::from_owned(bytes).into());
            for list in fonts.families.values_mut() {
                list.push("yahei".into());
            }
        }
    }
    #[cfg(target_os = "macos")]
    macos_fonts::install(&mut fonts);
    ctx.set_fonts(fonts);
}

pub(crate) fn paint(ui: &egui::Ui, rect: Rect, x: f32, text: &str, font: FontId, color: Color32) {
    ui.painter()
        .with_clip_rect(rect.intersect(ui.clip_rect()))
        .text(
            pos2(x, rect.center().y),
            Align2::LEFT_CENTER,
            text,
            font,
            color,
        );
}

pub(crate) fn fit_line(
    painter: &egui::Painter,
    text: String,
    font: FontId,
    color: Color32,
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    fit_line_job(
        painter,
        LayoutJob::simple_singleline(text, font, color),
        width,
    )
}

pub(crate) fn fit_line_job(
    painter: &egui::Painter,
    mut job: LayoutJob,
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    job.break_on_newline = false;
    job.wrap.max_width = width.max(0.0);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    painter.layout_job(job)
}

pub(crate) fn centered_baseline(painter: &Painter, font: FontId) -> f32 {
    let sample = painter.layout_no_wrap("Ag".into(), font, Color32::PLACEHOLDER);
    galley_baseline(&sample) - sample.size().y / 2.0
}

pub(crate) fn galley_baseline(galley: &Galley) -> f32 {
    galley
        .rows
        .first()
        .and_then(|row| row.glyphs.first())
        .map_or(0.0, |glyph| glyph.pos.y)
}

pub(crate) fn paint_baseline(
    painter: &Painter,
    x: f32,
    baseline: f32,
    galley: std::sync::Arc<Galley>,
    color: Color32,
) {
    painter.galley(pos2(x, baseline - galley_baseline(&galley)), galley, color);
}

pub(crate) fn compact_button(text: &str) -> egui::Button<'_> {
    egui::Button::new(RichText::new(text).size(DETAIL_SIZE))
        .min_size(Vec2::new(25.0, CONTROL_HEIGHT))
}

pub(crate) fn small_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(compact_button(text))
}

pub(crate) struct Checkbox<'a> {
    checked: &'a mut bool,
    label: egui::WidgetText,
    indeterminate: bool,
}

impl<'a> Checkbox<'a> {
    pub(crate) fn new(checked: &'a mut bool, label: impl Into<egui::WidgetText>) -> Self {
        Self {
            checked,
            label: label.into(),
            indeterminate: false,
        }
    }

    pub(crate) fn indeterminate(mut self, indeterminate: bool) -> Self {
        self.indeterminate = indeterminate;
        self
    }
}

impl egui::Widget for Checkbox<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        ui.scope(|ui| {
            let widgets = &mut ui.visuals_mut().widgets;
            // Suppress egui's mark while retaining its box, text color and checkbox behavior.
            for visuals in [
                &mut widgets.noninteractive,
                &mut widgets.inactive,
                &mut widgets.hovered,
                &mut widgets.active,
                &mut widgets.open,
            ] {
                visuals.fg_stroke.width = 0.0;
            }
            let response = ui.add(
                egui::Checkbox::new(self.checked, self.label).indeterminate(self.indeterminate),
            );
            if ui.is_rect_visible(response.rect) && (self.indeterminate || *self.checked) {
                let icon = if self.indeterminate {
                    egui::include_image!("../assets/icons/branch-mixed.svg")
                } else {
                    egui::include_image!("../assets/icons/branch-checked.svg")
                };
                let (_, icon_rect) = ui.spacing().icon_rectangles(response.rect);
                egui::Image::new(icon)
                    .tint(ui.style().interact(&response).fg_stroke.color)
                    .paint_at(ui, icon_rect);
            }
            response
        })
        .inner
    }
}

pub(crate) fn load_save_icon(ctx: &egui::Context) -> egui::TextureHandle {
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icons/save.png"))
        .expect("embedded save icon must be a valid PNG");
    ctx.load_texture(
        "save-icon",
        egui::ColorImage::from_rgba_unmultiplied(
            [icon.width as usize, icon.height as usize],
            &icon.rgba,
        ),
        egui::TextureOptions::LINEAR,
    )
}

pub(crate) fn save_icon_button(
    ui: &mut egui::Ui,
    icon: &egui::TextureHandle,
    label: &str,
    color: Color32,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let painter = ui.painter();
    let widgets = ui.style().interact(&response);
    if response.hovered() || response.is_pointer_button_down_on() || response.has_focus() {
        painter.rect_filled(rect, widgets.corner_radius, widgets.weak_bg_fill);
    }
    if response.has_focus() {
        painter.rect_stroke(
            rect,
            widgets.corner_radius,
            ui.visuals().selection.stroke,
            egui::StrokeKind::Inside,
        );
    }
    painter.image(
        icon.id(),
        Rect::from_center_size(rect.center(), Vec2::splat(15.0)),
        Rect::from_min_max(egui::Pos2::ZERO, pos2(1.0, 1.0)),
        color,
    );
    response
}

pub(crate) fn filter_field(
    value: &mut String,
    hint: impl Into<egui::WidgetText>,
) -> egui::TextEdit<'_> {
    egui::TextEdit::singleline(value)
        .hint_text(hint)
        .margin(Margin::symmetric(6, 3))
        .vertical_align(Align::Center)
        .min_size(Vec2::new(0.0, CONTROL_HEIGHT))
}

fn toolbar_mark(ui: &mut egui::Ui) {
    let colors = theme::palette(ui.ctx());
    let (mark, _) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::hover());
    ui.painter().rect_filled(mark, 4.0, colors.selection);
    let point = mark.center();
    for (a, b) in [
        (point + Vec2::new(-4.0, -5.0), point + Vec2::new(-4.0, 5.0)),
        (point + Vec2::new(-4.0, 2.0), point + Vec2::new(4.0, -4.0)),
    ] {
        ui.painter()
            .line_segment([a, b], Stroke::new(1.5_f32, colors.accent));
        ui.painter().circle_filled(a, 1.7, colors.accent);
        ui.painter().circle_filled(b, 1.7, colors.accent);
    }
}

fn title_button(text: &str) -> egui::Button<'_> {
    compact_button(text).min_size(Vec2::new(TITLE_BUTTON_MIN_WIDTH, TITLE_BUTTON_HEIGHT))
}

enum AppearanceAction {
    Select(usize),
    #[cfg(windows)]
    OpenDirectory,
}

fn appearance_menu(
    ui: &mut egui::Ui,
    themes: &[Theme],
    current: usize,
) -> egui::InnerResponse<Option<AppearanceAction>> {
    let menu = egui::menu::menu_custom_button(ui, title_button("外观"), |ui| {
        let colors = theme::palette(ui.ctx());
        ui.set_min_width(106.0);
        let selected = egui::ScrollArea::vertical()
            .max_height(320.0)
            .show(ui, |ui| {
                let mut selected = None;
                for (mode, label) in [(egui::Theme::Light, "亮色"), (egui::Theme::Dark, "暗色")]
                {
                    if mode == egui::Theme::Dark {
                        ui.separator();
                    }
                    ui.label(RichText::new(label).small().color(colors.muted));
                    for (index, theme) in themes.iter().enumerate().filter(|(_, t)| t.mode == mode)
                    {
                        if ui.selectable_label(index == current, &theme.name).clicked() {
                            selected = Some(index);
                            ui.close_menu();
                        }
                    }
                }
                selected
            })
            .inner;
        #[cfg(windows)]
        {
            ui.separator();
            if ui.button("自定义主题...").clicked() {
                ui.close_menu();
                return Some(AppearanceAction::OpenDirectory);
            }
        }
        selected.map(AppearanceAction::Select)
    });
    egui::InnerResponse::new(menu.inner.flatten(), menu.response)
}

fn window_buttons(ui: &mut egui::Ui, area: Rect, buttons: WindowButtons, focused: bool) -> Rect {
    let colors = theme::palette(ui.ctx());
    use egui::{ViewportCommand, WidgetInfo, WidgetType};

    let maximize_label = if buttons.maximized {
        "还原"
    } else {
        "最大化"
    };
    let items = [
        ("最小化", ViewportCommand::Minimized(true)),
        (
            maximize_label,
            ViewportCommand::Maximized(!buttons.maximized),
        ),
    ];
    let mut maximize_rect = Rect::NOTHING;
    for (index, (label, command)) in items.into_iter().enumerate() {
        let rect = Rect::from_min_size(
            pos2(area.left() + index as f32 * WINDOW_BUTTON_WIDTH, area.top()),
            Vec2::new(WINDOW_BUTTON_WIDTH, area.height()),
        );
        if index == 1 {
            maximize_rect = rect;
        }
        let response = ui.interact(
            rect,
            ui.id().with(("window-controls", index)),
            Sense::click(),
        );
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, ui.is_enabled(), label));
        if response.clicked() {
            ui.ctx().send_viewport_cmd(command);
        }
        let pressed =
            response.is_pointer_button_down_on() || (index == 1 && buttons.maximize_pressed);
        let hovered = response.hovered() || (index == 1 && buttons.maximize_hovered);
        let painter = ui.painter();
        if hovered || pressed {
            let fill = if pressed {
                colors.selection
            } else {
                colors.hover
            };
            painter.rect_filled(rect, 0.0, fill);
        }
        paint_caption_focus(ui, &response);
        let ink = if focused || hovered {
            colors.text
        } else {
            colors.muted
        };
        let stroke = Stroke::new(1.0_f32, ink);
        let center = rect.center();
        let glyph = Rect::from_center_size(center, Vec2::splat(10.0));
        match index {
            0 => {
                painter.hline(glyph.x_range(), center.y, stroke);
            }
            1 if buttons.maximized => {
                let front = Rect::from_center_size(center + Vec2::new(-1.0, 1.0), Vec2::splat(8.0));
                let back = front.translate(Vec2::new(2.0, -2.0));
                painter.add(egui::Shape::line(
                    vec![
                        pos2(back.left(), front.top()),
                        back.left_top(),
                        back.right_top(),
                        back.right_bottom(),
                        pos2(front.right(), back.bottom()),
                    ],
                    stroke,
                ));
                painter.rect_stroke(front, 0.0, stroke, egui::StrokeKind::Inside);
            }
            _ => {
                painter.rect_stroke(glyph, 0.0, stroke, egui::StrokeKind::Inside);
            }
        }
        if index != 1 {
            response.on_hover_text(label);
        }
    }
    let close_rect = Rect::from_min_max(
        pos2(area.right() - WINDOW_BUTTON_WIDTH, area.top()),
        area.max,
    );
    if window_close_button(ui, close_rect, focused, egui::CornerRadius::ZERO).clicked() {
        ui.ctx().send_viewport_cmd(ViewportCommand::Close);
    }
    maximize_rect
}

pub(crate) fn window_close_button(
    ui: &mut egui::Ui,
    rect: Rect,
    focused: bool,
    rounding: egui::CornerRadius,
) -> egui::Response {
    let colors = theme::palette(ui.ctx());
    let response = ui.interact(rect, ui.id().with("window-close"), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), "关闭")
    });
    let pressed = response.is_pointer_button_down_on();
    let hovered = response.hovered();
    let painter = ui.painter();
    let ink = if pressed || hovered {
        let fill = if pressed {
            colors.close_pressed
        } else {
            colors.close_hover
        };
        painter.rect_filled(rect, rounding, fill);
        colors.close_foreground
    } else if focused {
        colors.text
    } else {
        colors.muted
    };
    paint_caption_focus(ui, &response);
    let glyph = Rect::from_center_size(rect.center(), Vec2::splat(10.0));
    let stroke = Stroke::new(1.0_f32, ink);
    painter.line_segment([glyph.left_top(), glyph.right_bottom()], stroke);
    painter.line_segment([glyph.left_bottom(), glyph.right_top()], stroke);
    response.on_hover_text("关闭")
}

fn paint_caption_focus(ui: &egui::Ui, response: &egui::Response) {
    if response.has_focus() {
        let colors = theme::palette(ui.ctx());
        ui.painter().rect_stroke(
            response.rect.shrink(2.0),
            2.0,
            Stroke::new(1.0_f32, colors.accent),
            egui::StrokeKind::Inside,
        );
    }
}

fn panel(fill: Color32, horizontal: i8, vertical: i8) -> Frame {
    Frame::new()
        .fill(fill)
        .inner_margin(Margin::symmetric(horizontal, vertical))
}

fn panel_separator_color(ctx: &egui::Context, panel_id: egui::Id) -> Color32 {
    let colors = theme::palette(ctx);
    // egui's SidePanel and TopBottomPanel use this ID for their resize handle.
    if ctx
        .read_response(panel_id.with("__resize"))
        .is_some_and(|handle| handle.hovered() || handle.dragged())
    {
        colors.splitter_hover
    } else {
        colors.border
    }
}

pub(crate) fn split_panel_height(
    ui: &egui::Ui,
    splitter_id: egui::Id,
    default_height: f32,
    upper_bounds: egui::Rangef,
    lower_min: f32,
) -> f32 {
    let gap = ui.spacing().item_spacing.y;
    let maximum = (ui.available_height() - lower_min - 2.0 * gap - SPLITTER_HEIGHT)
        .clamp(0.0, upper_bounds.max);
    let minimum = upper_bounds.min.min(maximum);
    // Read the interaction before laying out the list, as egui's panels do.
    let button = egui::PointerButton::Primary;
    if let Some(resize) = ui.ctx().read_response(splitter_id)
        && (resize.dragged_by(button) || resize.drag_stopped_by(button))
        && let Some(pointer) = resize.interact_pointer_pos()
    {
        let height = pointer.y - ui.next_widget_position().y - gap - SPLITTER_HEIGHT / 2.0;
        ui.data_mut(|data| data.insert_temp(splitter_id, height.clamp(minimum, maximum)));
    }
    ui.data(|data| data.get_temp::<f32>(splitter_id))
        .unwrap_or(default_height)
        .clamp(minimum, maximum)
}

pub(crate) fn panel_divider(ui: &mut egui::Ui, splitter_id: egui::Id) -> egui::Response {
    let colors = theme::palette(ui.ctx());
    let (_, divider) = ui.allocate_space(Vec2::new(ui.available_width(), SPLITTER_HEIGHT));
    let splitter = ui
        .interact(divider, splitter_id, Sense::drag())
        .on_hover_and_drag_cursor(egui::CursorIcon::ResizeVertical);
    let line = if splitter.hovered() || splitter.dragged() {
        colors.splitter_hover
    } else {
        colors.border
    };
    ui.painter().hline(
        divider.x_range(),
        divider.center().y,
        Stroke::new(1.0_f32, line),
    );
    splitter
}

pub(crate) fn heading(ui: &mut egui::Ui, name: &str, count: Option<usize>) {
    let colors = theme::palette(ui.ctx());
    ui.label(RichText::new(name).size(15.0).strong().color(colors.text));
    if let Some(count) = count {
        ui.label(
            RichText::new(count.to_string())
                .color(colors.subtle)
                .small(),
        );
    }
}

impl eframe::App for Desk {
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        visuals.panel_fill.to_normalized_gamma_f32()
    }

    fn raw_input_hook(&mut self, _ctx: &egui::Context, input: &mut egui::RawInput) {
        if self.is_capturing() {
            input.events.push(egui::Event::PointerGone);
        }
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let colors = theme::palette(ctx);
        self.events(ctx);
        self.position_capture(ctx);
        self.background_status(ctx);
        self.prefetch(ctx);
        if self.dialog.is_none()
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::F5))
        {
            self.refresh(ctx);
        }
        let viewport = ctx.screen_rect().size();
        let navigation_limit = (viewport.x * 0.27).clamp(221.0, 370.0);
        let inspector_limit = (viewport.x * 0.33).clamp(240.0, 475.0);
        let inspector_width = if viewport.x < 1080.0 { 240.0 } else { 278.0 };
        let diff_height = (viewport.y * 0.30).clamp(165.0, 259.0);
        #[cfg(windows)]
        let caption = self.window_chrome.as_ref().map(|chrome| chrome.buttons());
        #[cfg(not(windows))]
        let caption: Option<WindowButtons> = None;
        let title_height = if caption.is_some() {
            TITLE_HEIGHT
        } else {
            TOOLBAR_HEIGHT
        };
        let title = egui::TopBottomPanel::top("workbench")
            .exact_height(title_height)
            .show_separator_line(false)
            .frame(Frame::new())
            .show(ctx, |ui| self.title_bar(ui, title_height, caption));
        let (title_regions, next_theme) = title.inner;
        #[cfg(windows)]
        if let Some(chrome) = &self.window_chrome {
            chrome.set_toolbar_regions(title_regions, ctx.pixels_per_point());
        }
        #[cfg(not(windows))]
        let _ = title_regions;
        let status = egui::TopBottomPanel::bottom("status")
            .show_separator_line(false)
            .frame(panel(colors.chrome, 13, 3))
            .show(ctx, |ui| self.status_line(ui));

        let navigation_id = egui::Id::new("navigation");
        let diff_id = egui::Id::new("diff-view");
        let inspector_id = egui::Id::new("commit-and-files");
        let (navigation, diff, inspector) = egui::CentralPanel::default()
            .frame(Frame::NONE)
            .show(ctx, |ui| {
                let content_style = ui.style().clone();
                // egui draws its own hover and drag strokes even with separators disabled.
                let widgets = &mut ui.visuals_mut().widgets;
                widgets.hovered.fg_stroke = Stroke::NONE;
                widgets.active.fg_stroke = Stroke::NONE;

                let navigation = egui::SidePanel::left(navigation_id)
                    .show_separator_line(false)
                    .default_width(crate::timing::capture_sidebar().unwrap_or(221.0))
                    .width_range(184.0..=navigation_limit)
                    .frame(panel(colors.panel, 12, 10))
                    .show_inside(ui, |ui| {
                        ui.set_style(content_style.clone());
                        self.navigation(ui, ctx);
                    });
                let diff = egui::TopBottomPanel::bottom(diff_id)
                    .resizable(true)
                    .show_separator_line(false)
                    .default_height(diff_height)
                    .height_range(135.0..=(viewport.y * 0.57).max(135.0))
                    .frame(panel(colors.panel, 12, 9))
                    .show_inside(ui, |ui| {
                        ui.set_style(content_style.clone());
                        ui.set_min_height(ui.available_height());
                        self.diff_panel(ui, ctx);
                    });
                let inspector = egui::SidePanel::right(inspector_id)
                    .show_separator_line(false)
                    .default_width(inspector_width)
                    .width_range(220.0..=inspector_limit)
                    .frame(panel(colors.panel, 12, 10))
                    .show_inside(ui, |ui| {
                        ui.set_style(content_style.clone());
                        self.inspector(ui, ctx);
                    });
                ui.set_style(content_style);
                egui::CentralPanel::default()
                    .frame(panel(colors.base, 11, 10))
                    .show_inside(ui, |ui| self.history_panel(ui, ctx));
                (
                    navigation.response.rect,
                    diff.response.rect,
                    inspector.response.rect,
                )
            })
            .inner;

        let scale = ctx.pixels_per_point();
        let pixel = 1.0 / scale;
        let title = title.response.rect.round_to_pixels(scale);
        let status = status.response.rect.round_to_pixels(scale);
        let navigation = navigation.round_to_pixels(scale);
        let diff = diff.round_to_pixels(scale);
        let inspector = inspector.round_to_pixels(scale);
        // A mesh keeps one-pixel rectangles sharp, including where the edges meet.
        let mut separators = egui::Mesh::default();
        for (rect, color) in [
            (
                Rect::from_min_max(
                    pos2(title.left(), title.bottom() - pixel),
                    title.right_bottom(),
                ),
                colors.border,
            ),
            (
                Rect::from_min_size(status.min, Vec2::new(status.width(), pixel)),
                colors.border,
            ),
            (
                Rect::from_min_max(
                    pos2(navigation.right() - pixel, title.bottom()),
                    pos2(navigation.right(), status.top()),
                ),
                panel_separator_color(ctx, navigation_id),
            ),
            (
                Rect::from_min_size(diff.min, Vec2::new(diff.width(), pixel)),
                panel_separator_color(ctx, diff_id),
            ),
            (
                Rect::from_min_max(inspector.min, pos2(inspector.left() + pixel, diff.top())),
                panel_separator_color(ctx, inspector_id),
            ),
        ] {
            separators.add_colored_rect(rect, color);
        }
        ctx.layer_painter(egui::LayerId::background())
            .add(separators);
        self.dialogs(ctx);
        self.report_frame(ctx);
        self.request_operation_followups(ctx);
        match next_theme {
            Some(AppearanceAction::Select(index)) => {
                self.active_theme = index;
                let theme = &self.themes[index];
                self.settings.theme.clone_from(&theme.id);
                theme.apply(ctx);
                #[cfg(windows)]
                if let Some(chrome) = &self.window_chrome {
                    chrome.apply_theme(theme);
                }
                self.save();
            }
            #[cfg(windows)]
            Some(AppearanceAction::OpenDirectory) => {
                if let Err(error) = theme::open_directory(&crate::settings::themes_path()) {
                    self.tell(format!("无法打开自定义主题目录：{error}"), true);
                }
            }
            None => {}
        }
    }
}

impl Desk {
    fn title_bar(
        &mut self,
        ui: &mut egui::Ui,
        height: f32,
        caption: Option<WindowButtons>,
    ) -> (ToolbarRegions, Option<AppearanceAction>) {
        let colors = theme::palette(ui.ctx());
        let spacing = ui.spacing_mut();
        spacing.interact_size.y = TITLE_BUTTON_HEIGHT;
        spacing.button_padding.y = 2.0;
        let (row, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
        ui.painter().rect_filled(row, 0.0, colors.chrome);

        let start = row.left() + 14.0;
        let caption_x = row.right()
            - if caption.is_some() {
                WINDOW_BUTTON_WIDTH * 3.0
            } else {
                0.0
            };
        let end = caption_x - if caption.is_some() { 8.0 } else { 14.0 };
        let control_half_height = TITLE_BUTTON_HEIGHT / 2.0;
        let center_y = row.center().y;
        let content = Rect::from_min_max(
            pos2(start, center_y - control_half_height),
            pos2(end, center_y + control_half_height),
        );
        let mut mark = ui.new_child(
            UiBuilder::new()
                .id_salt("title-mark")
                .max_rect(content)
                .layout(Layout::left_to_right(Align::Center)),
        );
        toolbar_mark(&mut mark);

        let mut tools = ui.new_child(
            UiBuilder::new()
                .id_salt("title-tools")
                .max_rect(content)
                .layout(Layout::right_to_left(Align::Center)),
        );
        let diff_tools = tools.add(title_button("Diff 工具"));
        if diff_tools.clicked() {
            self.tools_open = true;
        }
        let appearance = appearance_menu(&mut tools, &self.themes, self.active_theme);
        let tools_bounds = diff_tools.rect.union(appearance.response.rect);
        let caption_area = Rect::from_min_max(pos2(caption_x, row.top()), row.max);
        let maximize =
            caption.map(|buttons| window_buttons(ui, caption_area, buttons, self.focused));
        let mark_bounds = mark.min_rect();
        let regions = ToolbarRegions {
            drag: Rect::from_min_max(
                pos2(mark_bounds.right() + 8.0, row.top()),
                pos2(tools_bounds.left() - 8.0, row.bottom() - 1.0),
            ),
            controls: [mark_bounds, tools_bounds, caption_area],
            maximize,
        };
        (regions, appearance.inner)
    }

    pub(crate) fn repository_toolbar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let active = self.active.clone();
        let can_run = active
            .as_ref()
            .is_some_and(|root| !self.operations.contains(root))
            && self.state().is_some_and(|state| state.history.is_some());
        let operations = [
            ("获取", Operation::Fetch),
            ("拉取", Operation::Pull),
            ("推送", Operation::Push),
        ];
        let refresh_label = "刷新";
        let gap = 4.0;
        let padding = ui.spacing().button_padding.x;
        let width = ui.fonts(|fonts| {
            operations
                .iter()
                .map(|(label, _)| *label)
                .chain([refresh_label])
                .map(|label| {
                    let text = fonts.layout_no_wrap(
                        label.into(),
                        FontId::proportional(DETAIL_SIZE),
                        Color32::PLACEHOLDER,
                    );
                    (text.size().x + 2.0 * padding).max(TITLE_BUTTON_MIN_WIDTH)
                })
                .sum::<f32>()
        }) + (operations.len() + 1) as f32 * gap;
        ui.add_space((ui.available_size_before_wrap().x - width).max(0.0));

        let mut action = None;
        ui.allocate_ui_with_layout(
            Vec2::new(width, TITLE_BUTTON_HEIGHT),
            Layout::left_to_right(Align::Center),
            |ui| {
                let spacing = ui.spacing_mut();
                spacing.interact_size.y = TITLE_BUTTON_HEIGHT;
                spacing.button_padding.y = 2.0;
                spacing.item_spacing.x = gap;
                for (title, operation) in operations {
                    if ui.add_enabled(can_run, title_button(title)).clicked() {
                        action = Some((title, operation));
                    }
                }
                ui.add_space(gap);
                if ui
                    .add(title_button(refresh_label))
                    .on_hover_text("刷新当前仓库（F5）")
                    .clicked()
                {
                    self.refresh(ctx);
                }
            },
        );
        if let (Some(root), Some((title, operation))) = (active, action) {
            match operation {
                Operation::Fetch => self.operate(ctx, root, operation),
                Operation::Pull => {
                    self.dialog = Some(Dialog::Operation {
                        root,
                        operation,
                        title,
                        question: "获取远程变更并仅允许快进更新当前分支。".into(),
                    })
                }
                Operation::Push => {
                    self.dialog = Some(Dialog::Operation {
                        root,
                        operation,
                        title,
                        question: "使用当前仓库的 Git 推送配置发送提交。".into(),
                    })
                }
                _ => {}
            }
        }
    }

    fn status_line(&self, ui: &mut egui::Ui) {
        let colors = theme::palette(ui.ctx());
        ui.horizontal(|ui| {
            let busy = self
                .state()
                .is_some_and(|state| state.history_busy || state.status_busy)
                || self.details_busy
                || self.diff.as_ref().is_some_and(|diff| diff.busy())
                || !self.operations.is_empty();
            let color = if self.status_is_error {
                colors.removed.foreground
            } else if busy {
                colors.warning
            } else {
                colors.accent
            };
            ui.label(RichText::new("●").size(9.0).color(color));
            let message = if self.status_message.is_empty() {
                if busy {
                    "读取 Git 数据…"
                } else {
                    "就绪"
                }
            } else {
                self.status_message.lines().next().unwrap_or_default()
            };
            ui.add(
                egui::Label::new(
                    RichText::new(message)
                        .color(if self.status_is_error {
                            colors.removed.foreground
                        } else {
                            colors.muted
                        })
                        .small(),
                )
                .truncate(),
            )
            .on_hover_text(&self.status_message);
        });
    }

    fn navigation(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let colors = theme::palette(ctx);
        const REPO_ROW_HEIGHT: f32 = 23.0;
        const ACTIVE_REPO_ROW_HEIGHT: f32 = 35.0;
        const REPO_ROW_GAP: f32 = 2.0;
        const REPO_GROUP_GAP: f32 = 6.0;
        const MIN_REPO_HEIGHT: f32 = 123.5;
        const MAX_REPO_HEIGHT: f32 = 359.0;
        const MIN_REPO_ROWS: usize = 4;
        const MAX_DEFAULT_REPO_ROWS: usize = 6;
        const MIN_BRANCH_HEIGHT: f32 = 180.0;
        const REPO_INSET: f32 = 8.0;
        const REPO_INDENT: f32 = 14.0;
        const REPO_ICON_SIZE: f32 = 16.0;
        const REPO_ICON_GAP: f32 = 6.0;
        const MAX_REPO_DEPTH: usize = 4;
        let mut select = None;
        let mut forget = None;
        let repositories = self.repository_rows();
        let row_height = |active| {
            if active {
                ACTIVE_REPO_ROW_HEIGHT
            } else {
                REPO_ROW_HEIGHT
            }
        };
        let group_spacing = |index, depth| {
            if index > 0 && depth == 0 {
                REPO_GROUP_GAP - REPO_ROW_GAP
            } else {
                0.0
            }
        };
        let fit_rows = ((ui.available_height() * 0.40 - REPO_ROW_HEIGHT / 2.0)
            / (REPO_ROW_HEIGHT + REPO_GROUP_GAP)) as usize;
        let full_rows = fit_rows.clamp(MIN_REPO_ROWS, MAX_DEFAULT_REPO_ROWS);
        let default_height = repositories
            .iter()
            .take(full_rows + 1)
            .enumerate()
            .map(|(index, row)| {
                let spacing = if index == 0 { 0.0 } else { REPO_ROW_GAP };
                let height = row_height(self.active.as_ref() == Some(&row.path));
                let visible = if index == full_rows {
                    height / 2.0
                } else {
                    height
                };
                spacing + group_spacing(index, row.depth) + visible
            })
            .sum::<f32>()
            .max(MIN_REPO_HEIGHT);
        ui.horizontal(|ui| {
            heading(ui, "仓库", Some(repositories.len()));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.menu_button("···", |ui| {
                    if ui
                        .add(Checkbox::new(
                            &mut self.settings.prefetch_history,
                            "后台预读其他仓库的提交图",
                        ))
                        .on_hover_text(
                            "当前仓库优先。其他已登记仓库逐个预读日志，子模块在打开时加载。",
                        )
                        .changed()
                    {
                        self.save();
                    }
                    ui.separator();
                    if ui.button("添加当前仓库的 worktree").clicked() {
                        self.discover_worktrees(ctx);
                        ui.close_menu();
                    }
                });
                if small_button(ui, "＋")
                    .on_hover_text("添加 Git 仓库")
                    .clicked()
                {
                    self.choose_repository(ctx);
                }
            });
        });
        let splitter_id = ui.id().with("repository-splitter");
        let reveal_id = splitter_id.with("reveal-repository");
        let reveal = ui.data_mut(|data| data.remove_temp::<PathBuf>(reveal_id));
        let repo_h = split_panel_height(
            ui,
            splitter_id,
            default_height,
            (MIN_REPO_HEIGHT..=MAX_REPO_HEIGHT).into(),
            MIN_BRANCH_HEIGHT,
        );
        egui::ScrollArea::vertical()
            .id_salt("repositories")
            .max_height(repo_h)
            .min_scrolled_height(repo_h)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = REPO_ROW_GAP;
                let guide_stroke = Stroke::new(1.0_f32, colors.tree_guide);
                let mut guide_starts = [0.0; MAX_REPO_DEPTH];
                if repositories.is_empty() {
                    ui.add_space(7.0);
                    if small_button(ui, "选择仓库文件夹…").clicked() {
                        self.choose_repository(ctx);
                    }
                }
                for (index, row) in repositories.iter().enumerate() {
                    ui.add_space(group_spacing(index, row.depth));
                    let path = &row.path;
                    let available = row.submodule != Some(false);
                    let indent = row.depth.min(MAX_REPO_DEPTH) as f32 * REPO_INDENT;
                    let active = self.active.as_ref() == Some(path);
                    let name = path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("仓库");
                    let detail = if available {
                        self.repos
                            .get(path)
                            .and_then(|repo| repo.history_info())
                            .map(|history| history.branch.as_str())
                            .unwrap_or(if active {
                                "分支未读取"
                            } else if row.submodule.is_some() {
                                "子模块"
                            } else {
                                "工作树"
                            })
                    } else {
                        "子模块未检出"
                    };
                    let (_, rect) =
                        ui.allocate_space(Vec2::new(ui.available_width(), row_height(active)));
                    let response = ui.interact(rect, ui.id().with(path), Sense::click());
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::Button, available, name)
                    });
                    if reveal.as_ref() == Some(path) || response.gained_focus() {
                        response.scroll_to_me(None);
                    }
                    if active || response.hovered() {
                        ui.painter().rect_filled(
                            rect,
                            4.0,
                            if active {
                                colors.selection
                            } else {
                                colors.hover
                            },
                        );
                    }
                    if active {
                        let y = (rect.top() + 4.0)..=(rect.bottom() - 4.0);
                        ui.painter().vline(
                            rect.left() + 1.0,
                            y,
                            Stroke::new(2.0_f32, colors.accent),
                        );
                    }
                    if response.has_focus() {
                        ui.painter().rect_stroke(
                            rect.shrink(0.5),
                            4.0,
                            Stroke::new(1.0_f32, colors.accent),
                            egui::StrokeKind::Inside,
                        );
                    }
                    let icon_left = rect.left() + REPO_INSET + indent;
                    let name_y = rect.top() + REPO_ROW_HEIGHT / 2.0;
                    if (1..=MAX_REPO_DEPTH).contains(&row.depth) {
                        let start = &mut guide_starts[row.depth - 1];
                        let x = icon_left - REPO_INDENT + 4.0;
                        ui.painter().vline(x, *start..=name_y, guide_stroke);
                        ui.painter()
                            .hline(x..=(icon_left - 2.0), name_y, guide_stroke);
                        // The next sibling continues from this elbow across any descendants.
                        *start = name_y;
                    }
                    if row.depth < MAX_REPO_DEPTH {
                        guide_starts[row.depth] = rect.bottom() - 2.0;
                    }
                    let icon_rect = Rect::from_min_size(
                        pos2(icon_left, name_y - REPO_ICON_SIZE / 2.0),
                        Vec2::splat(REPO_ICON_SIZE),
                    );
                    if ui.is_rect_visible(icon_rect) {
                        let icon_color = if active {
                            colors.accent
                        } else if available {
                            colors.subtle
                        } else {
                            colors.subtle.gamma_multiply(0.55)
                        };
                        egui::Image::new(egui::include_image!(
                            "../assets/icons/repository-cube.svg"
                        ))
                        .tint(icon_color)
                        .paint_at(ui, icon_rect);
                    }
                    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
                    let text_left = icon_rect.right() + REPO_ICON_GAP;
                    let text_width = rect.right() - REPO_INSET - text_left;
                    let draw_text = |text: &str, size, y, color| {
                        let galley = fit_line(
                            &painter,
                            text.into(),
                            FontId::proportional(size),
                            color,
                            text_width,
                        );
                        painter.galley(pos2(text_left, y - galley.size().y / 2.0), galley, color);
                    };
                    let name_color = if available { colors.text } else { colors.muted };
                    draw_text(name, 13.0, name_y, name_color);
                    if active {
                        draw_text(detail, 11.0, name_y + 14.0, colors.muted);
                    }
                    if available && response.clicked() {
                        select = Some(path.clone());
                    }
                    response.context_menu(|ui| {
                        if ui.button("复制仓库路径").clicked() {
                            ui.ctx().copy_text(path.display().to_string());
                            ui.close_menu();
                        }
                        if self.settings.repositories.contains(path)
                            && ui.button("从 Refdesk 列表移除…").clicked()
                        {
                            forget = Some(path.clone());
                            ui.close_menu();
                        }
                    });
                    response.on_hover_ui(|ui| {
                        ui.label(name);
                        if available {
                            ui.label(detail);
                        } else {
                            ui.colored_label(colors.removed.foreground, detail);
                        }
                        ui.label(path.display().to_string());
                    });
                }
            });
        if let Some(path) = select {
            // The activity switch adds a second line on the next frame.
            ui.data_mut(|data| data.insert_temp(reveal_id, path.clone()));
            self.activate(ctx, path);
            ctx.request_repaint();
        }
        if let Some(path) = forget {
            self.dialog = Some(Dialog::Forget(path));
        }
        panel_divider(ui, splitter_id);
        self.branches(ui, ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, Modifiers, PointerButton, RawInput};

    #[test]
    fn panel_splitter_uses_release_position_and_clamps_height() {
        check_splitter_bounds(179.5, 123.5, 359.0, 180.0);
        check_splitter_bounds(215.0, 144.0, 320.0, 132.0);
    }

    fn check_splitter_bounds(default_height: f32, min: f32, max: f32, lower_min: f32) {
        let ctx = egui::Context::default();
        let splitter = egui::Id::new("panel-divider-test");
        let frame = |events, screen_height| {
            let mut result = (0.0, Rect::ZERO, 0.0);
            let _ = ctx.run(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        pos2(0.0, 0.0),
                        Vec2::new(500.0, screen_height),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let height = split_panel_height(
                            ui,
                            splitter,
                            default_height,
                            (min..=max).into(),
                            lower_min,
                        );
                        ui.allocate_space(Vec2::new(ui.available_width(), height));
                        let divider = panel_divider(ui, splitter).rect;
                        result = (height, divider, ui.available_height());
                    });
                },
            );
            result
        };
        let mouse_button = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let (initial_height, divider, _) = frame(vec![], 800.0);
        assert_eq!(initial_height, default_height);

        let start = divider.center();
        frame(
            vec![Event::PointerMoved(start), mouse_button(start, true)],
            800.0,
        );
        let end = pos2(start.x, 650.0);
        let (height, _, _) = frame(
            vec![Event::PointerMoved(end), mouse_button(end, false)],
            800.0,
        );
        assert_eq!(height, max);
        let (height, _, remaining) = frame(vec![], 440.0);
        assert!(height < max);
        assert_eq!(remaining, lower_min);

        let (height, divider, _) = frame(vec![], 800.0);
        assert_eq!(height, max);
        let start = divider.center();
        frame(
            vec![Event::PointerMoved(start), mouse_button(start, true)],
            800.0,
        );
        let end = pos2(start.x, 60.0);
        let (height, _, _) = frame(
            vec![Event::PointerMoved(end), mouse_button(end, false)],
            800.0,
        );
        assert_eq!(height, min);
    }
}
