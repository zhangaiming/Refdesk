use crate::app::Desk;
use crate::builtin_diff::{self, Column, Issue, Side};
use crate::git::Version;
use crate::theme;
use crate::ui::{paint, small_button};
use eframe::egui::{self, FontId, Rect, RichText, Sense, Stroke, Vec2, pos2};

impl Desk {
    pub(crate) fn diff_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let colors = theme::palette(ctx);
        let path = self.file.as_ref().map(|file| file.path.as_str());
        let tool = path
            .and_then(|path| self.settings.matching_tool(path))
            .cloned();
        let format = self.comparison.as_ref().map(builtin_diff::registered);
        let mut invoke = false;
        #[cfg(windows)]
        let mut open = false;
        ui.horizontal(|ui| {
            ui.label(RichText::new("差异").size(17.0).strong().color(colors.text));
            if let Some(path) = path {
                ui.add(
                    egui::Label::new(RichText::new(path).size(12.0).color(colors.muted)).truncate(),
                )
                .on_hover_text(path);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                #[cfg(windows)]
                if self
                    .file
                    .as_ref()
                    .is_some_and(|file| file.code == 'U' && !file.is_gitlink())
                {
                    open = small_button(ui, "打开文件")
                        .on_hover_text("在记事本打开工作树文件")
                        .clicked();
                }
                if let Some(tool) = &tool {
                    let label = format
                        .and_then(|format| format.external_label)
                        .map_or_else(|| format!("{} ↗", tool.name), str::to_owned);
                    invoke = small_button(ui, &label)
                        .on_hover_text(&tool.command)
                        .clicked();
                }
            });
        });
        #[cfg(windows)]
        if open && let Some(path) = path.map(str::to_owned) {
            self.open_conflict_file(&path);
        }
        if invoke && let Some(tool) = tool {
            self.external(ctx, tool);
        }

        let (Some(pair), Some(session)) = (&self.comparison, &mut self.diff) else {
            ui.add_space(8.0);
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("从右侧文件列表选择一项来比较").color(colors.muted));
            });
            return;
        };

        diff_header(
            ui,
            &format!("旧版本  ·  {}", version_title(&pair.left)),
            &format!("新版本  ·  {}", version_title(&pair.right)),
        );
        let raw_button = format.filter(|format| format.raw_comparison).map(|_| {
            if self.force_text {
                "单元格对比"
            } else {
                "比较原始文件"
            }
        });
        let was_changes_only = self.settings.changes_only;
        let mut panel = Panel {
            changes_only: &mut self.settings.changes_only,
            full_value: &mut self.show_full_line,
            raw_button,
            toggle_raw: false,
        };
        session.show(ui, &mut panel);
        if panel.toggle_raw {
            self.force_text = !self.force_text;
            self.request_diff(ctx);
        }
        if was_changes_only != self.settings.changes_only {
            self.save();
        }
    }
}

fn version_title(version: &Version) -> &str {
    match version {
        Version::Empty => "空版本",
        version => version.title(),
    }
}

pub(crate) struct Panel<'a> {
    pub changes_only: &'a mut bool,
    pub full_value: &'a mut Option<String>,
    raw_button: Option<&'static str>,
    toggle_raw: bool,
}

impl Panel<'_> {
    pub fn controls(&mut self, ui: &mut egui::Ui, show: impl FnOnce(&mut egui::Ui, &mut bool)) {
        ui.horizontal(|ui| {
            if let Some(label) = self.raw_button {
                self.toggle_raw = small_button(ui, label).clicked();
            }
            show(ui, self.changes_only);
        });
    }

    pub fn body<T>(
        &mut self,
        ui: &mut egui::Ui,
        sides: &[Side<T>; 2],
        show: impl FnOnce(&mut egui::Ui, Rect) -> Option<String>,
    ) {
        let (body, _) = ui.allocate_exact_size(ui.available_size().max(Vec2::ZERO), Sense::hover());
        if !body.is_positive() {
            return;
        }
        let mut content = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(body)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        content.set_clip_rect(ui.clip_rect().intersect(body));
        if let Some(value) = show(&mut content, body) {
            *self.full_value = Some(value);
        }
        for column in Column::BOTH {
            let bounds = column_rect(body, column);
            match &sides[column.index()] {
                Side::Loading => paint_loading(ui, bounds),
                Side::Absent => paint_message(ui, bounds, "此版本无文件", false),
                Side::Failed(issue) => paint_message(ui, bounds, &issue_message(*issue), true),
                Side::Ready(_) => {}
            }
        }
        divider(ui, body);
    }
}

pub(crate) fn column_rect(rect: Rect, column: Column) -> Rect {
    let middle = rect.center().x;
    match column {
        Column::Left => Rect::from_min_max(rect.min, pos2(middle, rect.bottom())),
        Column::Right => Rect::from_min_max(pos2(middle, rect.top()), rect.max),
    }
}

fn divider(ui: &egui::Ui, rect: Rect) {
    ui.painter().vline(
        rect.center().x,
        rect.y_range(),
        Stroke::new(1.0_f32, theme::palette(ui.ctx()).divider),
    );
}

fn diff_header(ui: &mut egui::Ui, left: &str, right: &str) {
    let colors = theme::palette(ui.ctx());
    ui.add_space(3.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 25.0), Sense::hover());
    ui.painter().rect_filled(rect, 3.0, colors.control);
    for (column, title) in Column::BOTH.into_iter().zip([left, right]) {
        let bounds = column_rect(rect, column);
        paint(
            ui,
            bounds,
            bounds.left() + 10.0,
            title,
            FontId::proportional(12.0),
            colors.muted,
        );
    }
    divider(ui, rect);
}

pub(crate) fn paint_message(ui: &egui::Ui, bounds: Rect, text: &str, failed: bool) {
    let bounds = bounds.shrink(8.0);
    if !bounds.is_positive() {
        return;
    }
    let colors = theme::palette(ui.ctx());
    let color = if failed { colors.warning } else { colors.muted };
    let painter = ui.painter().with_clip_rect(bounds);
    let text = painter.layout(
        text.to_owned(),
        FontId::proportional(13.0),
        color,
        bounds.width(),
    );
    painter.galley(bounds.center() - text.size() * 0.5, text, color);
}

fn issue_message(issue: Issue) -> String {
    match issue {
        Issue::Read => "无法读取此版本的文件。",
        Issue::FileMissing => "此版本的文件已不存在。",
        Issue::AccessDenied => "没有读取此版本的权限。",
        Issue::Decode => "无法解析此版本的内容，可以使用外部 Diff 工具。",
        Issue::SnapshotLimit => "文件超过 64 MiB，可以使用外部 Diff 工具。",
        Issue::PreviewLimit => "内容超过内置预览上限，可以使用外部 Diff 工具。",
        Issue::Binary(size) => return format!("二进制文件，共 {size} 字节。"),
        Issue::ParserUnavailable => "无法运行内置解析器，可以使用外部 Diff 工具。",
        Issue::GraphicsLimit => "图片尺寸超过当前显卡的纹理上限。",
        Issue::LfsPointer => "无法解析 Git LFS 指针。",
        Issue::LfsCache => "无法读取本地 Git LFS 缓存。",
        Issue::LfsDownload => "无法下载 Git LFS 对象。",
        Issue::LfsMissing => "Git LFS 对象未下载，可能被仓库设置跳过。",
    }
    .to_owned()
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
