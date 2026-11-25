use crate::app::{Desk, Dialog};
use crate::git::Operation;
use crate::settings::{self, Tool, ToolMode};
use crate::ui::{ACCENT, MUTE, small_button};
use eframe::egui::{self, Align2, RichText, Vec2};

impl Desk {
    pub(crate) fn dialogs(&mut self, ctx: &egui::Context) {
        if let Some(mut dialog) = self.dialog.take() {
            let mut keep = true;
            match &mut dialog {
                Dialog::NewBranch(name) => {
                    let mut open = true;
                    egui::Window::new("新建分支")
                        .collapsible(false)
                        .resizable(false)
                        .open(&mut open)
                        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                        .show(ctx, |ui| {
                            ui.text_edit_singleline(name);
                            ui.horizontal(|ui| {
                                if ui.button("取消").clicked() {
                                    keep = false;
                                }
                                if ui
                                    .add_enabled(
                                        !name.trim().is_empty(),
                                        egui::Button::new("创建并切换"),
                                    )
                                    .clicked()
                                    && let Some(root) = self.active.clone()
                                {
                                    self.operate(
                                        ctx,
                                        root,
                                        Operation::Branch(name.trim().to_owned()),
                                    );
                                    keep = false;
                                }
                            });
                        });
                    keep &= open;
                }
                Dialog::Operation {
                    root,
                    operation,
                    title,
                    question,
                } => {
                    let mut open = true;
                    egui::Window::new(*title)
                        .collapsible(false)
                        .resizable(false)
                        .open(&mut open)
                        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                        .show(ctx, |ui| {
                            ui.label(question.as_str());
                            ui.label(
                                RichText::new(root.display().to_string())
                                    .small()
                                    .color(MUTE),
                            );
                            ui.horizontal(|ui| {
                                if ui.button("取消").clicked() {
                                    keep = false;
                                }
                                if ui.button("继续").clicked() {
                                    self.operate(ctx, root.clone(), operation.clone());
                                    keep = false;
                                }
                            });
                        });
                    keep &= open;
                }
                Dialog::Forget(path) => {
                    let mut open = true;
                    egui::Window::new("移除仓库入口")
                        .collapsible(false)
                        .resizable(false)
                        .open(&mut open)
                        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                        .show(ctx, |ui| {
                            ui.label(path.display().to_string());
                            ui.horizontal(|ui| {
                                if ui.button("取消").clicked() {
                                    keep = false;
                                }
                                if ui.button("从列表移除").clicked() {
                                    self.forget(ctx, path);
                                    keep = false;
                                }
                            });
                        });
                    keep &= open;
                }
            }
            if keep {
                self.dialog = Some(dialog);
            }
        }
        if self.show_full_line.is_some() {
            let mut open = true;
            egui::Window::new("完整内容")
                .open(&mut open)
                .default_width(600.0)
                .show(ctx, |ui| {
                    if let Some(line) = &self.show_full_line {
                        egui::ScrollArea::both().max_height(240.0).show(ui, |ui| {
                            ui.label(RichText::new(line).monospace());
                        });
                        if ui.button("复制").clicked() {
                            ui.ctx().copy_text(line.clone());
                        }
                    }
                });
            if !open {
                self.show_full_line = None;
            }
        }
        if self.tools_open {
            self.tools_dialog(ctx);
        }
    }

    fn tools_dialog(&mut self, ctx: &egui::Context) {
        let mut open = self.tools_open;
        let mut import = None;
        let mut remove = None;
        let mut move_rule = None;
        let mut saved = false;
        egui::Window::new("Diff 工具设置")
            .open(&mut open)
            .default_width(640.0)
            .show(ctx, |ui| {
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
                        .color(MUTE)
                        .small(),
                );
                if !self.import_message.is_empty() {
                    ui.label(RichText::new(&self.import_message).color(ACCENT).small());
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
                                    if index > 0 && small_button(ui, "↑").clicked() {
                                        move_rule = Some((index, index - 1));
                                    }
                                    if index + 1 < count && small_button(ui, "↓").clicked() {
                                        move_rule = Some((index, index + 1));
                                    }
                                    if small_button(ui, "删除规则").clicked() {
                                        remove = Some(index);
                                    }
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
        self.tools_open = open;
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
