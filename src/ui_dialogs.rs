use crate::app::{Desk, Dialog, TrackingDialog};
use crate::git::Operation;
use crate::settings::{self, Tool, ToolMode};
use crate::theme;
use crate::ui::{
    Checkbox, TITLE_HEIGHT, WINDOW_BUTTON_WIDTH, compact_button, paint, small_button,
    window_close_button,
};
use eframe::egui::{
    self, Align, CornerRadius, Frame, Layout, Margin, Rect, RichText, Sense, Stroke, Vec2,
    emath::GuiRounding, pos2,
};

fn dialog_window(ctx: &egui::Context, title: &str) -> egui::Window<'static> {
    egui::Window::new(title)
        .title_bar(false)
        .frame(dialog_frame(ctx))
}

fn dialog_frame(ctx: &egui::Context) -> Frame {
    Frame::window(&ctx.style()).inner_margin(0)
}

fn dialog_title(ui: &mut egui::Ui, title: &str, focused: bool) -> bool {
    let colors = theme::palette(ui.ctx());
    let corners = ui.visuals().window_corner_radius;
    let (row, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), TITLE_HEIGHT),
        Sense::hover(),
    );
    let rounding = CornerRadius {
        sw: 0,
        se: 0,
        ..corners
    };
    ui.painter().rect_filled(row, rounding, colors.chrome);
    let close = Rect::from_min_max(pos2(row.right() - WINDOW_BUTTON_WIDTH, row.top()), row.max);
    let title_rect = Rect::from_min_max(row.min, pos2(close.left() - 8.0, row.bottom()));
    paint(
        ui,
        title_rect,
        row.left() + 14.0,
        title,
        egui::TextStyle::Body.resolve(ui.style()),
        colors.text,
    );
    let rounding = CornerRadius {
        ne: corners.ne,
        ..CornerRadius::ZERO
    };
    let closed = window_close_button(ui, close, focused, rounding).clicked();

    let scale = ui.ctx().pixels_per_point();
    let row = row.round_to_pixels(scale);
    let divider = Rect::from_min_max(pos2(row.left(), row.bottom() - 1.0 / scale), row.max);
    ui.painter().rect_filled(divider, 0.0, colors.border);
    closed
}

fn show_dialog(
    window: egui::Window<'_>,
    ctx: &egui::Context,
    title: &str,
    focused: bool,
    contents: impl FnOnce(&mut egui::Ui),
) -> bool {
    window
        .show(ctx, |ui| {
            let closed = dialog_title(ui, title, focused);
            dialog_body().show(ui, contents);
            closed
        })
        .and_then(|window| window.inner)
        .unwrap_or(false)
}

fn dialog_body() -> Frame {
    Frame::new().inner_margin(Margin::same(14))
}

fn danger_button(ui: &mut egui::Ui, button: egui::Button<'_>) -> egui::Response {
    let colors = theme::palette(ui.ctx());
    let danger = colors.removed;
    ui.scope(|ui| {
        let visuals = ui.visuals_mut();
        visuals.override_text_color = Some(danger.foreground);
        let widgets = &mut visuals.widgets;
        for (widget, emphasis, border) in [
            (&mut widgets.inactive, 0.0, danger.border),
            (&mut widgets.hovered, 0.06, danger.foreground),
            (&mut widgets.active, 0.10, colors.accent),
        ] {
            widget.weak_bg_fill = danger.background.lerp_to_gamma(danger.foreground, emphasis);
            widget.bg_stroke = Stroke::new(1.0_f32, border);
        }
        ui.add(button)
    })
    .inner
}

fn confirmation_buttons(
    ui: &mut egui::Ui,
    label: &str,
    enabled: bool,
    dangerous: bool,
    focus_cancel: bool,
    confirm: impl FnOnce(),
) -> bool {
    let cancel = "取消";
    let font = egui::TextStyle::Button.resolve(ui.style());
    let action_width = |text: &str| {
        let width = ui.fonts(|fonts| {
            fonts
                .layout_no_wrap(text.to_owned(), font.clone(), egui::Color32::PLACEHOLDER)
                .size()
                .x
        });
        (width + 2.0 * ui.spacing().button_padding.x).max(72.0)
    };
    let cancel_size = Vec2::new(action_width(cancel), 28.0);
    let confirm_size = Vec2::new(action_width(label), 28.0);
    let row_size = Vec2::new(
        cancel_size.x + ui.spacing().item_spacing.x + confirm_size.x,
        28.0,
    );
    ui.add_space(12.0);
    let (cancelled, confirmed) = ui
        .with_layout(Layout::top_down(Align::Max), |ui| {
            ui.allocate_ui_with_layout(row_size, Layout::left_to_right(Align::Center), |ui| {
                let cancel = ui.add(egui::Button::new(cancel).min_size(cancel_size));
                if focus_cancel {
                    cancel.request_focus();
                }
                let button = egui::Button::new(label).min_size(confirm_size);
                let action = if dangerous && enabled {
                    danger_button(ui, button)
                } else {
                    ui.add_enabled(enabled, button)
                };
                (cancel.clicked(), action.clicked())
            })
            .inner
        })
        .inner;
    if confirmed {
        confirm();
        true
    } else {
        cancelled
    }
}

fn tracking_picker<'a>(
    ui: &mut egui::Ui,
    title: &str,
    choices: &'a [String],
    selection: &mut Option<usize>,
    prefix: &str,
) -> Option<&'a str> {
    let label = ui.label(title);
    let display = |name: &'a str| name.strip_prefix(prefix).unwrap_or(name);
    egui::ComboBox::from_id_salt(title)
        .width(ui.available_width())
        .height(220.0)
        .truncate()
        .selected_text(selection.map_or("请选择", |index| display(&choices[index])))
        .show_ui(ui, |ui| {
            for (index, name) in choices.iter().enumerate() {
                let name = display(name);
                ui.selectable_value(selection, Some(index), name)
                    .on_hover_text(name);
            }
        })
        .response
        .labelled_by(label.id);
    selection.map(|index| choices[index].as_str())
}

fn tracking_controls(ui: &mut egui::Ui, dialog: &mut TrackingDialog) -> Option<Operation> {
    let colors = theme::palette(ui.ctx());
    let choices = match &dialog.options {
        None => {
            ui.label("正在读取跟踪分支和远端…");
            return None;
        }
        Some(Err(error)) => {
            ui.colored_label(colors.removed.foreground, error);
            return None;
        }
        Some(Ok(choices)) => choices,
    };
    let branch = &choices.branch;
    ui.label(if dialog.from_push {
        format!("本地分支 {branch} 尚未设置跟踪分支。")
    } else {
        format!("本地分支 · {branch}")
    });
    if dialog.from_push {
        ui.add_enabled(
            !choices.remotes.is_empty(),
            Checkbox::new(&mut dialog.publish, "本次推送并自动建立跟踪分支"),
        );
        if choices.remotes.is_empty() {
            ui.label(RichText::new("此仓库尚未配置远端，请先添加远端。").color(colors.muted));
            return None;
        }
    }
    ui.add_space(6.0);
    if dialog.publish {
        let remote = tracking_picker(
            ui,
            "推送到远端",
            &choices.remotes,
            &mut dialog.selected_remote,
            "",
        )?;
        ui.label(RichText::new(format!("目标分支 · {remote}/{branch}")).color(colors.muted));
        return Some(Operation::PushNewUpstream {
            branch: branch.clone(),
            remote: remote.to_owned(),
        });
    }
    if choices.remote_branches.is_empty() {
        ui.label(RichText::new("暂无可选的远端分支。可通过「获取」同步。").color(colors.muted));
        return None;
    }
    let upstream = tracking_picker(
        ui,
        "跟踪的远端分支",
        &choices.remote_branches,
        &mut dialog.selected_upstream,
        "refs/remotes/",
    );
    if dialog.from_push {
        ui.label(RichText::new("保存跟踪关系后，可再次点击「推送」。").color(colors.muted));
    }
    upstream.map(|upstream| Operation::SetUpstream {
        branch: branch.clone(),
        upstream: upstream.to_owned(),
    })
}

impl Desk {
    pub(crate) fn dialogs(&mut self, ctx: &egui::Context) {
        let colors = theme::palette(ctx);
        if let Some(mut dialog) = self.dialog.take() {
            let title = match &dialog {
                Dialog::NewBranch(_) => "新建分支",
                Dialog::NewCommitReference { tag: true, .. } => "创建标签",
                Dialog::NewCommitReference { .. } => "创建分支",
                Dialog::CherryPickMerge { .. } => "摘取提交",
                Dialog::Operation { title, .. } => *title,
                Dialog::Tracking(dialog) if dialog.from_push => "推送",
                Dialog::Tracking(_) => "设置跟踪分支",
                Dialog::Forget(_) => "移除仓库入口",
                Dialog::AbortRebaseMessage { .. } => "中止变基",
            };
            let modal_id = if matches!(
                &dialog,
                Dialog::Tracking(TrackingDialog { options: None, .. })
            ) {
                egui::Id::new((title, "tracking_loading"))
            } else {
                egui::Id::new(title)
            };
            let modal = egui::Modal::new(modal_id).frame(dialog_frame(ctx));
            let opening = ctx.memory(|memory| memory.top_modal_layer() != Some(modal.area.layer()));
            let modal = modal.show(ctx, |ui| {
                ui.set_width(420.0_f32.min(ctx.screen_rect().width()));
                let closed = dialog_title(ui, title, self.focused);
                let finished = dialog_body()
                    .show(ui, |ui| match &mut dialog {
                        Dialog::NewBranch(name) => {
                            let label = ui.label("分支名称");
                            let input = ui
                                .add(egui::TextEdit::singleline(name).desired_width(f32::INFINITY))
                                .labelled_by(label.id);
                            if opening {
                                input.request_focus();
                            }
                            let root = self.active.clone();
                            let enabled = !name.trim().is_empty() && root.is_some();
                            confirmation_buttons(
                                ui,
                                "创建并切换",
                                enabled,
                                false,
                                false,
                                || {
                                    if let Some(root) = root {
                                        self.operate(
                                            ctx,
                                            root,
                                            Operation::Branch(name.trim().to_owned()),
                                        );
                                    }
                                },
                            )
                        }
                        Dialog::NewCommitReference {
                            root,
                            commit,
                            name,
                            tag,
                        } => {
                            ui.label(format!("起点提交  {}", commit.short))
                                .on_hover_text(&commit.id);
                            let label =
                                ui.label(if *tag { "标签名称" } else { "分支名称" });
                            let input = ui
                                .add(egui::TextEdit::singleline(name).desired_width(f32::INFINITY))
                                .labelled_by(label.id);
                            if opening {
                                input.request_focus();
                            }
                            confirmation_buttons(
                                ui,
                                title,
                                !name.trim().is_empty() && !self.operations.contains(root),
                                false,
                                false,
                                || {
                                    let name = name.trim().to_owned();
                                    let commit = commit.id.clone();
                                    let operation = if *tag {
                                        Operation::TagAt { name, commit }
                                    } else {
                                        Operation::BranchAt { name, commit }
                                    };
                                    self.operate(ctx, root.clone(), operation);
                                },
                            )
                        }
                        Dialog::CherryPickMerge {
                            root,
                            commit,
                            mainline,
                        } => {
                            ui.label(format!(
                                "将合并提交 {} 的改动摘取到当前 HEAD？",
                                commit.short
                            ))
                            .on_hover_text(&commit.id);
                            ui.label("选择用于计算改动的主线父提交");
                            for (index, parent) in commit.parents.iter().enumerate() {
                                let short = parent.get(..12).unwrap_or(parent);
                                ui.radio_value(
                                    mainline,
                                    Some(index + 1),
                                    format!("父提交 {}  {short}", index + 1),
                                )
                                .on_hover_text(parent);
                            }
                            ui.label(
                                RichText::new(root.display().to_string())
                                    .small()
                                    .color(colors.muted),
                            );
                            confirmation_buttons(
                                ui,
                                title,
                                mainline.is_some() && !self.operations.contains(root),
                                false,
                                opening,
                                || {
                                    self.operate(
                                        ctx,
                                        root.clone(),
                                        Operation::CherryPick {
                                            commit: commit.id.clone(),
                                            mainline: *mainline,
                                        },
                                    );
                                },
                            )
                        }
                        Dialog::Operation {
                            root,
                            operation,
                            question,
                            ..
                        } => {
                            ui.label(question.as_str());
                            ui.label(
                                RichText::new(root.display().to_string())
                                    .small()
                                    .color(colors.muted),
                            );
                            confirmation_buttons(
                                ui,
                                title,
                                !self.operations.contains(root)
                                    && (!operation.requires_idle_worktree()
                                        || self
                                            .repos
                                            .get(root)
                                            .is_some_and(|state| state.normal_writes_available())),
                                matches!(
                                    operation,
                                    Operation::Push
                                        | Operation::DiscardUnstaged
                                        | Operation::Recover { .. }
                                ),
                                opening,
                                || {
                                    self.operate(ctx, root.clone(), operation.clone());
                                },
                            )
                        }
                        Dialog::Tracking(dialog) => {
                            let operation = tracking_controls(ui, dialog);
                            let can_write = self.active.as_ref() == Some(&dialog.root)
                                && !self.operations.contains(&dialog.root)
                                && self
                                    .state()
                                    .is_some_and(|state| state.normal_writes_available());
                            let action = if dialog.publish {
                                "推送并设置跟踪"
                            } else if dialog.from_push && !matches!(dialog.options, Some(Ok(_))) {
                                "推送"
                            } else {
                                "保存跟踪分支"
                            };
                            confirmation_buttons(
                                ui,
                                action,
                                can_write && operation.is_some(),
                                dialog.publish,
                                opening,
                                || {
                                    if let Some(operation) = operation {
                                        self.operate(ctx, dialog.root.clone(), operation);
                                    }
                                },
                            )
                        }
                        Dialog::AbortRebaseMessage { root, editor } => {
                            ui.label("将放弃这次提交说明编辑，并尝试恢复变基前的状态。");
                            ui.label("本次变基的改动和冲突解决结果可能丢失。");
                            ui.label(
                                RichText::new(root.display().to_string())
                                    .small()
                                    .color(colors.muted),
                            );
                            confirmation_buttons(ui, title, true, true, opening, || {
                                self.abort_rebase_message(root, *editor)
                            })
                        }
                        Dialog::Forget(path) => {
                            ui.label("从 Refdesk 的仓库列表移除此入口。");
                            ui.label(
                                RichText::new(path.display().to_string())
                                    .small()
                                    .color(colors.muted),
                            );
                            confirmation_buttons(ui, "从列表移除", true, true, opening, || {
                                self.forget(ctx, path)
                            })
                        }
                    })
                    .inner;
                closed || finished
            });
            let escaped = modal.is_top_modal
                && !modal.any_popup_open
                && ctx
                    .input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
            if !modal.inner && !escaped {
                self.dialog = Some(dialog);
            }
        }
        if let Some(line) = &self.show_full_line {
            let title = "完整内容";
            let window = dialog_window(ctx, title).default_width(600.0);
            let closed = show_dialog(window, ctx, title, self.focused, |ui| {
                egui::ScrollArea::both().max_height(240.0).show(ui, |ui| {
                    ui.add(egui::Label::new(RichText::new(line).monospace()).extend());
                });
                ui.add_space(12.0);
                ui.with_layout(Layout::top_down(Align::Max), |ui| {
                    if ui.button("复制").clicked() {
                        ui.ctx().copy_text(line.clone());
                    }
                });
            });
            if closed {
                self.show_full_line = None;
            }
        }
        if self.tools_open {
            self.tools_dialog(ctx);
        }
    }

    fn tools_dialog(&mut self, ctx: &egui::Context) {
        let colors = theme::palette(ctx);
        let title = "Diff 工具设置";
        let mut import = None;
        let mut remove = None;
        let mut move_rule = None;
        let mut saved = false;
        let window = dialog_window(ctx, title)
            .default_width(640.0)
            .min_width(520.0);
        let closed = show_dialog(window, ctx, title, self.focused, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("导入本机 SmartGit").clicked() {
                    import = Some(settings::smartgit_preferences());
                }
                if ui.button("选择 preferences.yml…").clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter("YAML", &["yml", "yaml"])
                        .pick_file()
                {
                    import = Some(Some(path));
                }
                if ui.button("＋ 新规则").clicked() {
                    self.settings.tools.push(Tool::default());
                    saved = true;
                }
            });
            ui.label(
                RichText::new("按从上到下的顺序匹配文件名。")
                    .color(colors.muted)
                    .small(),
            );
            if !self.import_message.is_empty() {
                ui.label(
                    RichText::new(&self.import_message)
                        .color(colors.accent)
                        .small(),
                );
            }
            egui::ScrollArea::vertical()
                .max_height(440.0)
                .show(ui, |ui| {
                    let count = self.settings.tools.len();
                    for (index, tool) in self.settings.tools.iter_mut().enumerate() {
                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                saved |= ui
                                    .add(
                                        egui::TextEdit::singleline(&mut tool.name)
                                            .desired_width(160.0),
                                    )
                                    .changed();
                                let mode = tool.mode;
                                egui::ComboBox::from_id_salt(("mode", index))
                                    .selected_text(mode_name(tool.mode))
                                    .show_ui(ui, |ui| {
                                        for mode in [
                                            ToolMode::External,
                                            ToolMode::BuiltIn,
                                            ToolMode::Viewer,
                                        ] {
                                            ui.selectable_value(
                                                &mut tool.mode,
                                                mode,
                                                mode_name(mode),
                                            );
                                        }
                                    });
                                saved |= tool.mode != mode;
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if danger_button(ui, compact_button("删除规则")).clicked() {
                                        remove = Some(index);
                                    }
                                    if index + 1 < count && small_button(ui, "↓").clicked() {
                                        move_rule = Some((index, index + 1));
                                    }
                                    if index > 0 && small_button(ui, "↑").clicked() {
                                        move_rule = Some((index, index - 1));
                                    }
                                });
                            });
                            ui.horizontal(|ui| {
                                ui.label("匹配规则");
                                saved |= ui
                                    .add(
                                        egui::TextEdit::singleline(&mut tool.pattern)
                                            .desired_width(f32::INFINITY)
                                            .hint_text("*.xlsx, *.xlsm"),
                                    )
                                    .changed();
                            });
                            if tool.mode != ToolMode::BuiltIn {
                                ui.horizontal(|ui| {
                                    ui.label("程序路径");
                                    saved |= ui
                                        .add(
                                            egui::TextEdit::singleline(&mut tool.command)
                                                .desired_width(f32::INFINITY),
                                        )
                                        .changed();
                                });
                                ui.horizontal(|ui| {
                                    ui.label("启动参数");
                                    saved |= ui
                                        .add(
                                            egui::TextEdit::multiline(&mut tool.arguments)
                                                .desired_rows(2)
                                                .desired_width(f32::INFINITY),
                                        )
                                        .changed();
                                });
                            }
                        });
                    }
                });
        });
        if closed {
            self.tools_open = false;
        }
        if let Some(index) = remove {
            self.settings.tools.remove(index);
            saved = true;
        } else if let Some((a, b)) = move_rule {
            self.settings.tools.swap(a, b);
            saved = true;
        }
        if let Some(import) = import {
            self.import_message = match import {
                Some(path) => match self.settings.import_tools(&path) {
                    Ok(count) => {
                        saved = true;
                        format!("已导入 {count} 条规则")
                    }
                    Err(error) => error.to_string(),
                },
                None => "找不到 SmartGit preferences.yml".into(),
            };
        }
        if saved {
            self.save();
        }
    }
}

fn mode_name(mode: ToolMode) -> &'static str {
    match mode {
        ToolMode::BuiltIn => "内置比较",
        ToolMode::External => "外部 diff",
        ToolMode::Viewer => "独立查看器",
    }
}
