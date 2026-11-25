use crate::app::{Desk, Dialog};
use crate::git::Operation;
use eframe::egui::{
    self, Align, Align2, Color32, FontId, Frame, Galley, Layout, Margin, Painter, Rect, RichText,
    Sense, Stroke, Vec2, pos2, text::LayoutJob,
};
use std::path::PathBuf;

pub(crate) const BASE: Color32 = Color32::from_rgb(24, 24, 23);
pub(crate) const PANEL: Color32 = Color32::from_rgb(31, 31, 30);
pub(crate) const CHROME: Color32 = Color32::from_rgb(37, 36, 34);
pub(crate) const SHADE: Color32 = Color32::from_rgb(44, 43, 40);
pub(crate) const HOVER: Color32 = Color32::from_rgb(50, 49, 46);
pub(crate) const SELECTION: Color32 = Color32::from_rgb(52, 58, 49);
pub(crate) const BORDER: Color32 = Color32::from_rgb(62, 60, 55);
pub(crate) const DIVIDER: Color32 = Color32::from_rgb(37, 36, 34);
pub(crate) const ACCENT: Color32 = Color32::from_rgb(178, 205, 160);
pub(crate) const INK: Color32 = Color32::from_rgb(236, 233, 225);
pub(crate) const MUTE: Color32 = Color32::from_rgb(193, 187, 177);
pub(crate) const FAINT: Color32 = Color32::from_rgb(166, 161, 151);
pub(crate) const RED: Color32 = Color32::from_rgb(239, 149, 143);
pub(crate) const GREEN: Color32 = Color32::from_rgb(151, 208, 155);
pub(crate) const GOLD: Color32 = Color32::from_rgb(229, 183, 128);
pub(crate) const ROW: f32 = 25.0;
pub(crate) const DETAIL_SIZE: f32 = 12.0;
pub(crate) const CONTROL_HEIGHT: f32 = 26.0;

pub fn install_theme(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = Vec2::new(8.0, 5.0);
    style.spacing.button_padding = Vec2::new(9.0, 4.0);
    style.spacing.interact_size = Vec2::new(36.0, CONTROL_HEIGHT);
    style.visuals = egui::Visuals::dark();
    style.visuals.override_text_color = Some(INK);
    style.visuals.window_fill = PANEL;
    style.visuals.panel_fill = BASE;
    style.visuals.extreme_bg_color = BASE;
    style.visuals.faint_bg_color = SHADE;
    style.visuals.selection.bg_fill = SELECTION;
    style.visuals.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    style.visuals.widgets.inactive.bg_fill = SHADE;
    style.visuals.widgets.inactive.weak_bg_fill = SHADE;
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(0.6_f32, BORDER);
    style.visuals.widgets.hovered.bg_fill = HOVER;
    style.visuals.widgets.hovered.weak_bg_fill = HOVER;
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(0.8_f32, ACCENT.gamma_multiply(0.48));
    style.visuals.widgets.active.bg_fill = SELECTION;
    style.visuals.widgets.active.weak_bg_fill = SELECTION;
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    style.visuals.widgets.open.bg_fill = SELECTION;
    style.visuals.widgets.open.weak_bg_fill = SELECTION;
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    for (role, size) in [
        (egui::TextStyle::Body, 14.0),
        (egui::TextStyle::Button, 13.0),
        (egui::TextStyle::Small, 11.0),
        (egui::TextStyle::Heading, 21.0),
    ] {
        style.text_styles.insert(role, FontId::proportional(size));
    }
    ctx.set_style(style);
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
    let sample = painter.layout_no_wrap("Ag".into(), font, INK);
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

pub(crate) fn filter_field<'a>(value: &'a mut String, hint: &str) -> egui::TextEdit<'a> {
    egui::TextEdit::singleline(value)
        .hint_text(hint)
        .margin(Margin::symmetric(6, 3))
        .vertical_align(Align::Center)
        .min_size(Vec2::new(0.0, CONTROL_HEIGHT))
}

fn toolbar_context_label(
    ui: &mut egui::Ui,
    galley: std::sync::Arc<Galley>,
    baseline_offset: f32,
    color: Color32,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(galley.size().x, CONTROL_HEIGHT), Sense::hover());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Label, ui.is_enabled(), galley.text())
    });
    paint_baseline(
        ui.painter(),
        rect.left(),
        rect.center().y + baseline_offset,
        galley,
        color,
    );
    response
}

fn toolbar_button(ui: &mut egui::Ui, name: &str, enabled: bool) -> egui::Response {
    ui.scope(|ui| {
        let idle = &mut ui.visuals_mut().widgets.inactive;
        idle.weak_bg_fill = Color32::TRANSPARENT;
        idle.bg_stroke = Stroke::NONE;
        ui.add_enabled(enabled, compact_button(name))
    })
    .inner
}

fn panel(fill: Color32, horizontal: i8, vertical: i8) -> Frame {
    Frame::new()
        .fill(fill)
        .inner_margin(Margin::symmetric(horizontal, vertical))
        .stroke(Stroke::new(1.0_f32, BORDER))
}

pub(crate) fn heading(ui: &mut egui::Ui, name: &str, count: Option<usize>) {
    ui.label(RichText::new(name).size(15.0).strong().color(INK));
    if let Some(count) = count {
        ui.label(RichText::new(count.to_string()).color(FAINT).small());
    }
}

impl eframe::App for Desk {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, input: &mut egui::RawInput) {
        if self.is_capturing() {
            input.events.push(egui::Event::PointerGone);
        }
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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
        egui::TopBottomPanel::top("workbench")
            .frame(panel(CHROME, 14, 6))
            .show(ctx, |ui| self.toolbar(ui, ctx));
        egui::TopBottomPanel::bottom("status")
            .frame(panel(CHROME, 12, 2))
            .show(ctx, |ui| self.status_line(ui));
        egui::SidePanel::left("navigation")
            .resizable(true)
            .default_width(crate::timing::capture_sidebar().unwrap_or(221.0))
            .width_range(184.0..=navigation_limit)
            .frame(panel(PANEL, 11, 9))
            .show(ctx, |ui| self.navigation(ui, ctx));
        egui::TopBottomPanel::bottom("diff-view")
            .resizable(true)
            .default_height(diff_height)
            .height_range(135.0..=(ctx.screen_rect().height() * 0.57).max(135.0))
            .frame(panel(PANEL, 11, 8))
            .show(ctx, |ui| {
                ui.set_min_height(ui.available_height());
                self.diff_panel(ui, ctx);
            });
        egui::SidePanel::right("commit-and-files")
            .resizable(true)
            .default_width(inspector_width)
            .width_range(220.0..=inspector_limit)
            .frame(panel(PANEL, 11, 9))
            .show(ctx, |ui| self.inspector(ui, ctx));
        egui::CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(BASE)
                    .inner_margin(Margin::symmetric(11, 9)),
            )
            .show(ctx, |ui| self.history_panel(ui, ctx));
        self.dialogs(ctx);
        self.report_frame(ctx);
    }
}

impl Desk {
    fn toolbar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let active = self.active.clone();
        let can_run = active
            .as_ref()
            .is_some_and(|root| !self.operations.contains(root))
            && self.state().is_some_and(|state| state.history.is_some());
        let branch = self
            .state()
            .and_then(|state| state.history.as_ref())
            .map(|history| history.branch.clone());
        let mut action = None;
        ui.horizontal(|ui| {
            let (mark, _) = ui.allocate_exact_size(Vec2::new(24.0, 24.0), Sense::hover());
            ui.painter().rect_filled(mark, 5.0, SELECTION);
            let point = mark.center();
            for (a, b) in [
                (point + Vec2::new(-5.0, -6.0), point + Vec2::new(-5.0, 6.0)),
                (point + Vec2::new(-5.0, 2.0), point + Vec2::new(5.0, -5.0)),
            ] {
                ui.painter()
                    .line_segment([a, b], Stroke::new(1.6_f32, ACCENT));
                ui.painter().circle_filled(a, 2.0, ACCENT);
                ui.painter().circle_filled(b, 2.0, ACCENT);
            }
            ui.label(RichText::new("Refdesk").size(16.0).strong().color(INK));
            ui.separator();
            if let Some(root) = &active {
                let label = root
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("仓库");
                let font = FontId::proportional(14.0);
                let baseline = centered_baseline(ui.painter(), font.clone());
                let name = fit_line(ui.painter(), label.into(), font, INK, 160.0);
                toolbar_context_label(ui, name, baseline, INK)
                    .on_hover_text(root.display().to_string());
                if let Some(branch) = branch {
                    let font = FontId::monospace(11.5);
                    let label = fit_line(ui.painter(), branch.clone(), font, MUTE, 190.0);
                    toolbar_context_label(ui, label, baseline, MUTE).on_hover_text(branch);
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button(RichText::new("Diff 工具").size(12.0)).clicked() {
                    self.tools_open = true;
                }
                if toolbar_button(ui, "刷新", true)
                    .on_hover_text("刷新当前仓库（F5）")
                    .clicked()
                {
                    self.refresh(ctx);
                }
                ui.separator();
                for (title, operation) in [
                    ("推送", Operation::Push),
                    ("拉取", Operation::Pull),
                    ("获取", Operation::Fetch),
                ] {
                    if toolbar_button(ui, title, can_run).clicked() {
                        action = Some((title, operation));
                    }
                }
            });
        });
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
        ui.horizontal(|ui| {
            let busy = self
                .state()
                .is_some_and(|state| state.history_busy || state.status_busy)
                || self.details_busy
                || self.diff_busy
                || !self.operations.is_empty();
            let color = if self.status_is_error {
                RED
            } else if busy {
                GOLD
            } else {
                ACCENT
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
                        .color(if self.status_is_error { RED } else { MUTE })
                        .small(),
                )
                .truncate(),
            )
            .on_hover_text(&self.status_message);
        });
    }

    fn navigation(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        const REPO_ROW_HEIGHT: f32 = 23.0;
        const REPO_ICON_SIZE: f32 = 16.0;
        let mut select = None;
        let mut forget = None;
        let repositories = self.repository_rows();
        let repo_limit = (ui.available_height() * 0.40).clamp(84.0, 290.0);
        let repo_h = if repositories.is_empty() {
            42.0
        } else {
            let gaps = repositories.len() - 1;
            (repositories.len() as f32 * REPO_ROW_HEIGHT
                + gaps as f32 * ui.spacing().item_spacing.y)
                .min(repo_limit)
        };
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
        egui::ScrollArea::vertical()
            .id_salt("repositories")
            .max_height(repo_h)
            .min_scrolled_height(repo_h)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if repositories.is_empty() {
                    ui.add_space(7.0);
                    if small_button(ui, "选择仓库文件夹…").clicked() {
                        self.choose_repository(ctx);
                    }
                }
                for row in &repositories {
                    let path = &row.path;
                    let available = row.submodule != Some(false);
                    let indent = row.depth.min(4) as f32 * 14.0;
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
                        ui.painter()
                            .rect_filled(rect, 4.0, if active { SELECTION } else { HOVER });
                    }
                    if active {
                        let y = (rect.top() + 4.0)..=(rect.bottom() - 4.0);
                        ui.painter()
                            .vline(rect.left() + 1.0, y, Stroke::new(2.0_f32, ACCENT));
                    }
                    let symbol = Rect::from_center_size(
                        pos2(
                            rect.left() + 6.0 + indent + REPO_ICON_SIZE / 2.0,
                            rect.center().y,
                        ),
                        Vec2::splat(REPO_ICON_SIZE),
                    );
                    ui.painter()
                        .rect_filled(symbol, 4.0, if active { SELECTION } else { SHADE });
                    ui.painter().text(
                        symbol.center(),
                        Align2::CENTER_CENTER,
                        name.chars()
                            .next()
                            .unwrap_or('R')
                            .to_uppercase()
                            .collect::<String>(),
                        FontId::proportional(10.0),
                        if active { ACCENT } else { MUTE },
                    );
                    let name_color = if available { INK } else { MUTE };
                    let detail_color = if !available {
                        RED
                    } else if active {
                        ACCENT
                    } else {
                        MUTE
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
        ui.add_space(5.0);
        ui.separator();
        self.branches(ui, ctx);
    }
}
