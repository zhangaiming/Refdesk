use crate::app::{Desk, Dialog};
use crate::git::Operation;
use eframe::egui::{
    self, Align, Align2, Color32, FontId, Frame, Layout, Margin, Rect, RichText, Sense, Stroke,
    Vec2, pos2,
};
use std::path::PathBuf;

pub(crate) const BASE: Color32 = Color32::from_rgb(17, 22, 30);
pub(crate) const PANEL: Color32 = Color32::from_rgb(20, 27, 37);
pub(crate) const SHADE: Color32 = Color32::from_rgb(28, 37, 50);
pub(crate) const HOVER: Color32 = Color32::from_rgb(33, 58, 67);
pub(crate) const BORDER: Color32 = Color32::from_rgb(42, 54, 70);
pub(crate) const ACCENT: Color32 = Color32::from_rgb(104, 219, 188);
pub(crate) const INK: Color32 = Color32::from_rgb(228, 235, 244);
pub(crate) const MUTE: Color32 = Color32::from_rgb(151, 171, 194);
pub(crate) const FAINT: Color32 = Color32::from_rgb(111, 135, 161);
pub(crate) const RED: Color32 = Color32::from_rgb(244, 135, 145);
pub(crate) const GREEN: Color32 = Color32::from_rgb(120, 225, 154);
pub(crate) const GOLD: Color32 = Color32::from_rgb(237, 179, 103);
pub(crate) const ROW: f32 = 27.0;

pub fn install_theme(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = Vec2::new(8.0, 5.0);
    style.spacing.button_padding = Vec2::new(9.0, 4.0);
    style.spacing.interact_size = Vec2::new(36.0, 26.0);
    style.visuals = egui::Visuals::dark();
    style.visuals.override_text_color = Some(INK);
    style.visuals.window_fill = PANEL;
    style.visuals.panel_fill = BASE;
    style.visuals.extreme_bg_color = BASE;
    style.visuals.faint_bg_color = SHADE;
    style.visuals.selection.bg_fill = HOVER;
    style.visuals.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    style.visuals.widgets.inactive.bg_fill = SHADE;
    style.visuals.widgets.inactive.weak_bg_fill = SHADE;
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(0.6_f32, BORDER);
    style.visuals.widgets.hovered.bg_fill = HOVER;
    style.visuals.widgets.hovered.weak_bg_fill = HOVER;
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(0.8_f32, ACCENT.gamma_multiply(0.48));
    style.visuals.widgets.active.bg_fill = HOVER;
    style.visuals.widgets.active.weak_bg_fill = HOVER;
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

pub(crate) fn small_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(egui::Button::new(RichText::new(text).size(12.0)).min_size(Vec2::new(25.0, 24.0)))
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
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.events(ctx);
        self.position_capture();
        self.background_status(ctx);
        self.prefetch(ctx);
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::F5)) {
            self.refresh(ctx);
        }
        egui::TopBottomPanel::top("workbench")
            .frame(panel(PANEL, 14, 8))
            .show(ctx, |ui| self.toolbar(ui, ctx));
        egui::TopBottomPanel::bottom("status")
            .frame(panel(PANEL, 12, 2))
            .show(ctx, |ui| self.status_line(ui));
        egui::SidePanel::left("navigation")
            .resizable(true)
            .default_width(221.0)
            .width_range(184.0..=370.0)
            .frame(panel(PANEL, 11, 9))
            .show(ctx, |ui| self.navigation(ui, ctx));
        egui::TopBottomPanel::bottom("diff-view")
            .resizable(true)
            .default_height(259.0)
            .height_range(135.0..=(ctx.screen_rect().height() * 0.57).max(135.0))
            .frame(panel(BASE, 11, 8))
            .show(ctx, |ui| {
                ui.set_min_height(ui.available_height());
                self.diff_panel(ui, ctx);
            });
        egui::SidePanel::right("commit-and-files")
            .resizable(true)
            .default_width(278.0)
            .width_range(220.0..=475.0)
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
            let (mark, _) = ui.allocate_exact_size(Vec2::new(28.0, 28.0), Sense::hover());
            ui.painter()
                .rect_filled(mark, 7.0, Color32::from_rgb(31, 70, 69));
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
            ui.label(RichText::new("Refdesk").size(19.0).strong().color(INK));
            ui.separator();
            if let Some(root) = &active {
                let label = root
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("仓库");
                ui.add(egui::Label::new(RichText::new(label).strong().color(INK)).truncate())
                    .on_hover_text(root.display().to_string());
            }
            if let Some(branch) = branch {
                ui.add(egui::Label::new(RichText::new(branch).color(ACCENT).size(12.0)).truncate());
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if small_button(ui, "Diff 工具").clicked() {
                    self.tools_open = true;
                }
                if small_button(ui, "刷新")
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
                    if ui
                        .add_enabled(can_run, egui::Button::new(RichText::new(title).size(12.0)))
                        .clicked()
                    {
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
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new("Git · 本地工作区").color(FAINT).small());
            });
        });
    }

    fn navigation(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let mut select = None;
        let mut forget = None;
        let repo_h = (ui.available_height() * 0.38).clamp(102.0, 290.0);
        ui.horizontal(|ui| {
            heading(ui, "仓库", Some(self.settings.repositories.len()));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.menu_button("···", |ui| {
                    if ui
                        .checkbox(
                            &mut self.settings.prefetch_history,
                            "后台预读其他仓库的提交图",
                        )
                        .on_hover_text(
                            "当前仓库优先。其余仓库逐个预读日志，首次打开时读取工作树状态。",
                        )
                        .changed()
                    {
                        self.save();
                    }
                    ui.separator();
                    if ui.button("添加当前仓库的 worktree").clicked() {
                        self.discover(ctx, false);
                        ui.close_menu();
                    }
                    if ui.button("添加已初始化的子模块").clicked() {
                        self.discover(ctx, true);
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
                if self.settings.repositories.is_empty() {
                    ui.add_space(7.0);
                    if small_button(ui, "选择仓库文件夹…").clicked() {
                        self.choose_repository(ctx);
                    }
                }
                let mut paths: Vec<(usize, &std::path::PathBuf, usize)> = self
                    .settings
                    .repositories
                    .iter()
                    .enumerate()
                    .map(|(index, path)| {
                        let mut anchor = index;
                        let mut depth = 0;
                        for (other_index, parent) in self.settings.repositories.iter().enumerate() {
                            if path != parent && path.starts_with(parent) {
                                depth += 1;
                                anchor = anchor.min(other_index);
                            }
                        }
                        (anchor, path, depth)
                    })
                    .collect();
                paths.sort_by_key(|(index, _, depth)| (*index, *depth));
                for (_, path, depth) in paths {
                    let indent = depth.min(4) as f32 * 14.0;
                    let active = self.active.as_ref() == Some(path);
                    let name = path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("仓库");
                    let detail = self
                        .repos
                        .get(path)
                        .and_then(|repo| repo.history.as_ref())
                        .map(|history| history.branch.as_str())
                        .unwrap_or("工作树");
                    let (rect, response) = ui
                        .allocate_exact_size(Vec2::new(ui.available_width(), 40.0), Sense::click());
                    if active || response.hovered() {
                        ui.painter()
                            .rect_filled(rect, 5.0, if active { HOVER } else { SHADE });
                    }
                    let symbol = Rect::from_min_size(
                        rect.left_top() + Vec2::new(8.0 + indent, 9.0),
                        Vec2::splat(22.0),
                    );
                    ui.painter().rect_filled(
                        symbol,
                        5.0,
                        if active {
                            Color32::from_rgb(45, 93, 90)
                        } else {
                            SHADE
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
                        FontId::proportional(11.0),
                        if active { ACCENT } else { MUTE },
                    );
                    let name_rect =
                        Rect::from_min_max(rect.min, pos2(rect.right() - 4.0, rect.top() + 21.0));
                    let detail_rect = Rect::from_min_max(
                        pos2(rect.left(), rect.top() + 19.0),
                        rect.max - Vec2::new(4.0, 1.0),
                    );
                    paint(
                        ui,
                        name_rect,
                        rect.left() + 37.0 + indent,
                        name,
                        FontId::proportional(13.0),
                        INK,
                    );
                    paint(
                        ui,
                        detail_rect,
                        rect.left() + 37.0 + indent,
                        detail,
                        FontId::proportional(11.0),
                        if active { ACCENT } else { MUTE },
                    );
                    if response.clicked() {
                        select = Some(path.clone());
                    }
                    response.context_menu(|ui| {
                        if ui.button("复制仓库路径").clicked() {
                            ui.ctx().copy_text(path.display().to_string());
                            ui.close_menu();
                        }
                        if ui.button("从 Refdesk 列表移除…").clicked() {
                            forget = Some(path.clone());
                            ui.close_menu();
                        }
                    });
                    response.on_hover_text(path.display().to_string());
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
