use crate::app::{Desk, Dialog};
use crate::git::Operation;
use crate::theme::{self, ThemeId};
use eframe::egui::{
    self, Align, Align2, Color32, FontId, Frame, Galley, Layout, Margin, Painter, Rect, RichText,
    Sense, Stroke, UiBuilder, Vec2, pos2, text::LayoutJob,
};
use std::path::PathBuf;

pub(crate) const ROW: f32 = 25.0;
pub(crate) const DETAIL_SIZE: f32 = 12.0;
pub(crate) const CONTROL_HEIGHT: f32 = 26.0;
const TOOLBAR_HEIGHT: f32 = 40.0;
const TITLE_HEIGHT: f32 = 34.0;
const TITLE_BUTTON_HEIGHT: f32 = 24.0;
const WINDOW_BUTTON_WIDTH: f32 = 46.0;
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
    let mut job = LayoutJob::simple_singleline(text, font, color);
    job.wrap.max_width = width.max(0.0);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    painter.layout_job(job)
}

pub(crate) fn centered_baseline(painter: &Painter, font: FontId) -> f32 {
    let sample = painter.layout_no_wrap("Ag".into(), font, Color32::PLACEHOLDER);
    galley_baseline(&sample) - sample.size().y / 2.0
}

fn galley_baseline(galley: &Galley) -> f32 {
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

fn toolbar_button(ui: &mut egui::Ui, name: &str, enabled: bool) -> egui::Response {
    ui.scope(|ui| {
        let idle = &mut ui.visuals_mut().widgets.inactive;
        idle.weak_bg_fill = Color32::TRANSPARENT;
        idle.bg_stroke = Stroke::NONE;
        ui.add_enabled(enabled, title_button(name))
    })
    .inner
}

fn title_button(text: &str) -> egui::Button<'_> {
    compact_button(text).min_size(Vec2::new(25.0, TITLE_BUTTON_HEIGHT))
}

fn appearance_menu(ui: &mut egui::Ui, current: ThemeId) -> egui::InnerResponse<Option<ThemeId>> {
    let menu = egui::menu::menu_custom_button(ui, title_button("外观"), |ui| {
        let colors = theme::palette(ui.ctx());
        ui.set_min_width(106.0);
        let mut selected = current;
        for (index, (label, themes)) in ThemeId::GROUPS.into_iter().enumerate() {
            if index > 0 {
                ui.separator();
            }
            ui.label(RichText::new(label).small().color(colors.muted));
            for theme in themes {
                if ui
                    .selectable_value(&mut selected, theme, theme.label())
                    .clicked()
                {
                    ui.close_menu();
                }
            }
        }
        selected
    });
    egui::InnerResponse::new(menu.inner.filter(|theme| *theme != current), menu.response)
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
        ("关闭", ViewportCommand::Close),
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
        let close = index == 2;
        let painter = ui.painter();
        if hovered || pressed {
            let fill = match (close, pressed) {
                (true, true) => colors.close_pressed,
                (true, false) => colors.close_hover,
                (false, true) => colors.selection,
                (false, false) => colors.hover,
            };
            painter.rect_filled(rect, 0.0, fill);
        }
        if response.has_focus() {
            painter.rect_stroke(
                rect.shrink(2.0),
                2.0,
                Stroke::new(1.0_f32, colors.accent),
                egui::StrokeKind::Inside,
            );
        }
        let ink = if close && (hovered || pressed) {
            colors.close_foreground
        } else if focused || hovered {
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
            1 => {
                painter.rect_stroke(glyph, 0.0, stroke, egui::StrokeKind::Inside);
            }
            _ => {
                painter.line_segment([glyph.left_top(), glyph.right_bottom()], stroke);
                painter.line_segment([glyph.left_bottom(), glyph.right_top()], stroke);
            }
        }
        if index != 1 {
            response.on_hover_text(label);
        }
    }
    maximize_rect
}

fn panel(fill: Color32, border: Color32, horizontal: i8, vertical: i8) -> Frame {
    Frame::new()
        .fill(fill)
        .inner_margin(Margin::symmetric(horizontal, vertical))
        .stroke(Stroke::new(1.0_f32, border))
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
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::F5)) {
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
        let toolbar_height = if caption.is_some() {
            TITLE_HEIGHT
        } else {
            TOOLBAR_HEIGHT
        };
        let (toolbar, next_theme) = egui::TopBottomPanel::top("workbench")
            .exact_height(toolbar_height)
            .show_separator_line(false)
            .frame(Frame::new())
            .show(ctx, |ui| self.toolbar(ui, ctx, toolbar_height, caption))
            .inner;
        #[cfg(windows)]
        if let Some(chrome) = &self.window_chrome {
            chrome.set_toolbar_regions(toolbar, ctx.pixels_per_point());
        }
        #[cfg(not(windows))]
        let _ = toolbar;
        egui::TopBottomPanel::bottom("status")
            .frame(panel(colors.chrome, colors.border, 12, 2))
            .show(ctx, |ui| self.status_line(ui));
        egui::SidePanel::left("navigation")
            .resizable(true)
            .default_width(crate::timing::capture_sidebar().unwrap_or(221.0))
            .width_range(184.0..=navigation_limit)
            .frame(panel(colors.panel, colors.border, 11, 9))
            .show(ctx, |ui| self.navigation(ui, ctx));
        egui::TopBottomPanel::bottom("diff-view")
            .resizable(true)
            .default_height(diff_height)
            .height_range(135.0..=(ctx.screen_rect().height() * 0.57).max(135.0))
            .frame(panel(colors.panel, colors.border, 11, 8))
            .show(ctx, |ui| {
                ui.set_min_height(ui.available_height());
                self.diff_panel(ui, ctx);
            });
        egui::SidePanel::right("commit-and-files")
            .resizable(true)
            .default_width(inspector_width)
            .width_range(220.0..=inspector_limit)
            .frame(panel(colors.panel, colors.border, 11, 9))
            .show(ctx, |ui| self.inspector(ui, ctx));
        egui::CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(colors.base)
                    .inner_margin(Margin::symmetric(11, 9)),
            )
            .show(ctx, |ui| self.history_panel(ui, ctx));
        self.dialogs(ctx);
        self.report_frame(ctx);
        if let Some(theme) = next_theme {
            self.settings.theme = theme;
            theme.apply(ctx);
            #[cfg(windows)]
            if let Some(chrome) = &self.window_chrome {
                chrome.apply_theme(theme);
            }
            self.save();
        }
    }
}

impl Desk {
    fn toolbar(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        height: f32,
        caption: Option<WindowButtons>,
    ) -> (ToolbarRegions, Option<ThemeId>) {
        let colors = theme::palette(ctx);
        let active = self.active.clone();
        let can_run = active
            .as_ref()
            .is_some_and(|root| !self.operations.contains(root))
            && self.state().is_some_and(|state| state.history.is_some());
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
        let mut actions = ui.new_child(
            UiBuilder::new()
                .id_salt("toolbar-actions")
                .max_rect(content)
                .layout(Layout::left_to_right(Align::Center)),
        );
        actions.spacing_mut().item_spacing.x = 4.0;
        toolbar_mark(&mut actions);
        actions.add_space(8.0);
        let mut action = None;
        for (title, operation) in [
            ("获取", Operation::Fetch),
            ("拉取", Operation::Pull),
            ("推送", Operation::Push),
        ] {
            if toolbar_button(&mut actions, title, can_run).clicked() {
                action = Some((title, operation));
            }
        }
        if toolbar_button(&mut actions, "刷新", true)
            .on_hover_text("刷新当前仓库（F5）")
            .clicked()
        {
            self.refresh(ctx);
        }

        let mut tools = ui.new_child(
            UiBuilder::new()
                .id_salt("toolbar-tools")
                .max_rect(content)
                .layout(Layout::right_to_left(Align::Center)),
        );
        let diff_tools = tools.add(title_button("Diff 工具"));
        if diff_tools.clicked() {
            self.tools_open = true;
        }
        let appearance = appearance_menu(&mut tools, self.settings.theme);
        let tools_bounds = diff_tools.rect.union(appearance.response.rect);
        let caption_area = Rect::from_min_max(pos2(caption_x, row.top()), row.max);
        let maximize =
            caption.map(|buttons| window_buttons(ui, caption_area, buttons, self.focused));
        ui.painter().hline(
            row.x_range(),
            row.bottom() - 0.5,
            Stroke::new(1.0_f32, colors.border),
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
        let action_bounds = actions.min_rect();
        let regions = ToolbarRegions {
            drag: Rect::from_min_max(
                pos2(action_bounds.right() + 8.0, row.top()),
                pos2(tools_bounds.left() - 8.0, row.bottom() - 1.0),
            ),
            controls: [action_bounds, tools_bounds, caption_area],
            maximize,
        };
        (regions, appearance.inner)
    }

    fn status_line(&self, ui: &mut egui::Ui) {
        let colors = theme::palette(ui.ctx());
        ui.horizontal(|ui| {
            let busy = self
                .state()
                .is_some_and(|state| state.history_busy || state.status_busy)
                || self.details_busy
                || self.diff_busy
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
        const MIN_REPO_ROWS: usize = 4;
        const MAX_DEFAULT_REPO_ROWS: usize = 6;
        const MIN_BRANCH_HEIGHT: f32 = 180.0;
        const REPO_ICON_SIZE: f32 = 16.0;
        const REPO_INDENT: f32 = 14.0;
        const MAX_REPO_DEPTH: usize = 4;
        let mut select = None;
        let mut forget = None;
        let repositories = self.repository_rows();
        let gap = ui.spacing().item_spacing.y;
        let row_stride = REPO_ROW_HEIGHT + gap;
        let half_row = REPO_ROW_HEIGHT / 2.0;
        let fit_rows = ((ui.available_height() * 0.40 - half_row) / row_stride).floor() as usize;
        let default_limit =
            fit_rows.clamp(MIN_REPO_ROWS, MAX_DEFAULT_REPO_ROWS) as f32 * row_stride + half_row;
        let preferred_min = MIN_REPO_ROWS as f32 * row_stride + half_row;
        let content_height = repositories.len() as f32 * row_stride - gap;
        let default_height = content_height.clamp(preferred_min, default_limit);
        ui.horizontal(|ui| {
            heading(ui, "仓库", Some(repositories.len()));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.menu_button("···", |ui| {
                    if ui
                        .checkbox(
                            &mut self.settings.prefetch_history,
                            "后台预读其他仓库的提交图",
                        )
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
        let preferred_max = 2.0 * (MAX_DEFAULT_REPO_ROWS as f32 * row_stride + half_row);
        let repo_h = split_panel_height(
            ui,
            splitter_id,
            default_height,
            (preferred_min..=preferred_max).into(),
            MIN_BRANCH_HEIGHT,
        );
        egui::ScrollArea::vertical()
            .id_salt("repositories")
            .max_height(repo_h)
            .min_scrolled_height(repo_h)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let guide_stroke = Stroke::new(1.0_f32, colors.tree_guide);
                let mut guide_starts = [0.0; MAX_REPO_DEPTH];
                if repositories.is_empty() {
                    ui.add_space(7.0);
                    if small_button(ui, "选择仓库文件夹…").clicked() {
                        self.choose_repository(ctx);
                    }
                }
                for row in &repositories {
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
                            .and_then(|repo| repo.history.as_ref())
                            .map(|history| history.branch.as_str())
                            .unwrap_or(if row.submodule.is_some() {
                                "子模块"
                            } else {
                                "工作树"
                            })
                    } else {
                        "子模块未检出"
                    };
                    let (rect, response) = ui.allocate_exact_size(
                        Vec2::new(ui.available_width(), REPO_ROW_HEIGHT),
                        Sense::click(),
                    );
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
                    let symbol = Rect::from_center_size(
                        pos2(
                            rect.left() + 6.0 + indent + REPO_ICON_SIZE / 2.0,
                            rect.center().y,
                        ),
                        Vec2::splat(REPO_ICON_SIZE),
                    );
                    if (1..=MAX_REPO_DEPTH).contains(&row.depth) {
                        let start = &mut guide_starts[row.depth - 1];
                        let x = symbol.center().x - REPO_INDENT;
                        let y = symbol.center().y;
                        ui.painter().vline(x, *start..=y, guide_stroke);
                        ui.painter()
                            .hline(x..=(symbol.left() - 1.5), y, guide_stroke);
                        // The next sibling continues from this elbow across any descendants.
                        *start = y;
                    }
                    if row.depth < MAX_REPO_DEPTH {
                        guide_starts[row.depth] = symbol.bottom() + 1.5;
                    }
                    ui.painter().rect_filled(
                        symbol,
                        4.0,
                        if active {
                            colors.selection
                        } else {
                            colors.control
                        },
                    );
                    ui.painter().text(
                        symbol.center(),
                        Align2::CENTER_CENTER,
                        name.chars()
                            .next()
                            .unwrap_or('R')
                            .to_uppercase()
                            .collect::<String>(),
                        FontId::proportional(10.0),
                        if active { colors.accent } else { colors.muted },
                    );
                    let name_color = if available { colors.text } else { colors.muted };
                    let detail_color = if !available {
                        colors.removed.foreground
                    } else if active {
                        colors.accent
                    } else {
                        colors.muted
                    };
                    let text_rect = Rect::from_min_max(
                        pos2(symbol.right() + 6.0, rect.top()),
                        pos2(rect.right() - 4.0, rect.bottom()),
                    );
                    let mut label = LayoutJob::simple_singleline(
                        name.to_owned(),
                        FontId::proportional(13.0),
                        name_color,
                    );
                    label.append(
                        &format!(" ({detail})"),
                        0.0,
                        egui::TextFormat::simple(FontId::proportional(11.0), detail_color),
                    );
                    label.wrap.max_width = text_rect.width().max(0.0);
                    label.wrap.max_rows = 1;
                    label.wrap.break_anywhere = true;
                    let painter = ui.painter().with_clip_rect(text_rect);
                    let galley = painter.layout_job(label);
                    let text_pos = Align2::LEFT_CENTER
                        .anchor_size(pos2(text_rect.left(), rect.center().y), galley.size())
                        .min;
                    painter.galley(text_pos, galley, name_color);
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
                    response.on_hover_text(format!("{name} ({detail})\n{}", path.display()));
                }
            });
        if let Some(path) = select {
            self.activate(ctx, path);
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
