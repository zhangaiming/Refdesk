use crate::app::{Desk, Selection};
use crate::git::{Area, Operation};
use crate::ui::{ACCENT, FAINT, GOLD, GREEN, HOVER, INK, MUTE, RED, SHADE, heading, paint};
use eframe::egui::{self, Align, FontId, Layout, RichText, Sense, Vec2};
use std::path::Path;

impl Desk {
    pub(crate) fn inspector(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let worktree = matches!(self.selection, Selection::Worktree);
        let active = self.active.clone();
        let busy = active
            .as_ref()
            .is_some_and(|root| self.operations.contains(root));
        let staged = self.state().map_or(0, |state| {
            state
                .changes
                .iter()
                .filter(|file| file.area == Area::Staged)
                .count()
        });
        let mut commit = false;
        let height = (ui.available_height() * 0.43).clamp(104.0, 215.0);
        ui.allocate_ui_with_layout(
            Vec2::new(ui.available_width(), height),
            Layout::top_down(Align::Min),
            |ui| {
                ui.set_min_height(height);
                ui.horizontal(|ui| {
                    heading(ui, if worktree { "提交" } else { "提交详情" }, None);
                    if self.details_busy {
                        ui.spinner();
                    }
                });
                ui.add_space(4.0);
                if worktree {
                    ui.add_enabled(
                        active.is_some(),
                        egui::TextEdit::multiline(&mut self.editor)
                            .hint_text("写下本次提交的说明…")
                            .desired_rows(3)
                            .desired_width(f32::INFINITY),
                    );
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("{staged} 项已暂存"))
                                .small()
                                .color(MUTE),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            commit = ui
                                .add_enabled(
                                    staged > 0 && !self.editor.trim().is_empty() && !busy,
                                    egui::Button::new(RichText::new("提交").color(ACCENT).strong()),
                                )
                                .clicked();
                        });
                    });
                } else if let Some(details) = &self.details {
                    let body_height = (height - 42.0).max(45.0);
                    egui::ScrollArea::vertical()
                        .id_salt("commit-detail")
                        .max_height(body_height)
                        .show(ui, |ui| {
                            ui.label(RichText::new(&details.title).size(14.0).strong().color(INK));
                            ui.label(
                                RichText::new(format!("{} · {}", details.author, details.date))
                                    .size(11.0)
                                    .color(MUTE),
                            );
                            let (summary, rest) = details
                                .message
                                .split_once('\n')
                                .unwrap_or((&details.message, ""));
                            if details.title != summary {
                                ui.label(RichText::new(summary).small().color(MUTE));
                            }
                            if !rest.trim().is_empty() {
                                ui.label(RichText::new(rest.trim()).size(12.0).color(MUTE));
                            }
                            if let Selection::Commit { right, .. } = &self.selection {
                                ui.label(
                                    RichText::new(&right.short).monospace().small().color(FAINT),
                                )
                                .on_hover_text(&right.id);
                            }
                        });
                } else if let Some(error) = &self.details_error {
                    ui.colored_label(RED, error);
                }
            },
        );
        if commit && let Some(root) = active {
            self.operate(ctx, root, Operation::Commit(self.editor.trim().to_owned()));
        }
        ui.separator();
        self.files(ui, ctx);
    }

    fn files(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let worktree = matches!(self.selection, Selection::Worktree);
        let state = self.state();
        let count = if worktree {
            state.map_or(0, |state| state.changes.len())
        } else {
            self.details
                .as_ref()
                .map_or(0, |details| details.files.len())
        };
        let can_stage = self
            .active
            .as_ref()
            .is_some_and(|root| !self.operations.contains(root));
        let mut chosen = None;
        let mut external = false;
        let mut staging = None;
        ui.horizontal(|ui| {
            heading(ui, "文件", Some(count));
            if worktree && state.is_some_and(|state| state.status_busy) {
                ui.spinner();
            }
        });
        if worktree && let Some(error) = state.and_then(|state| state.status_error.as_ref()) {
            ui.colored_label(RED, error);
        }
        if count == 0 && !state.is_some_and(|state| state.status_busy) && !self.details_busy {
            ui.label(
                RichText::new(if worktree {
                    "没有待比较的文件"
                } else {
                    "该提交没有文件变更"
                })
                .color(MUTE)
                .small(),
            );
        }
        let files = if worktree {
            state
                .map(|state| state.changes.as_slice())
                .unwrap_or_default()
        } else {
            self.details
                .as_ref()
                .map(|details| details.files.as_slice())
                .unwrap_or_default()
        };
        let entries: Vec<_> = if worktree {
            [("已暂存", Area::Staged), ("未暂存", Area::Unstaged)]
                .into_iter()
                .flat_map(|(name, area)| {
                    std::iter::once((Some(name), None)).chain(
                        files
                            .iter()
                            .filter(move |file| file.area == area)
                            .map(|file| (None, Some(file))),
                    )
                })
                .collect()
        } else {
            files.iter().map(|file| (None, Some(file))).collect()
        };
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            egui::ScrollArea::vertical()
                .id_salt(("file-list", self.details_busy))
                .auto_shrink([false, false])
                .show_rows(ui, 25.0, entries.len(), |ui, visible| {
                    for index in visible {
                        let (heading, file) = entries[index];
                        let (rect, response) = ui.allocate_exact_size(
                            Vec2::new(ui.available_width(), 25.0),
                            Sense::click(),
                        );
                        if let Some(heading) = heading {
                            ui.painter().rect_filled(rect, 4.0, SHADE);
                            paint(
                                ui,
                                rect,
                                rect.left() + 7.0,
                                heading,
                                FontId::proportional(11.0),
                                MUTE,
                            );
                        }
                        let Some(file) = file else {
                            continue;
                        };
                        let is_selected = self.file.as_ref().is_some_and(|current| {
                            current.path == file.path && current.area == file.area
                        });
                        if is_selected || response.hovered() {
                            ui.painter().rect_filled(
                                rect,
                                4.0,
                                if is_selected { HOVER } else { SHADE },
                            );
                        }
                        let color = match file.code {
                            'A' | '?' => GREEN,
                            'D' | 'U' => RED,
                            'R' => ACCENT,
                            _ => GOLD,
                        };
                        paint(
                            ui,
                            rect,
                            rect.left() + 6.0,
                            &file.code.to_string(),
                            FontId::monospace(12.0),
                            color,
                        );
                        let short = Path::new(&file.path)
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or(&file.path);
                        paint(
                            ui,
                            rect,
                            rect.left() + 24.0,
                            short,
                            FontId::proportional(13.0),
                            INK,
                        );
                        if response.clicked() {
                            chosen = Some(file.clone());
                        }
                        if response.double_clicked()
                            && self.settings.matching_tool(&file.path).is_some()
                        {
                            chosen = Some(file.clone());
                            external = true;
                        }
                        response.context_menu(|ui| {
                            if ui.button("复制路径").clicked() {
                                ui.ctx().copy_text(file.path.clone());
                                ui.close_menu();
                            }
                            if can_stage && file.area != Area::Commit {
                                let label = if file.area == Area::Staged {
                                    "取消暂存"
                                } else {
                                    "暂存"
                                };
                                if ui.button(label).clicked() {
                                    staging = Some(file.clone());
                                    ui.close_menu();
                                }
                            }
                            if self.settings.matching_tool(&file.path).is_some()
                                && ui.button("用外部工具比较").clicked()
                            {
                                chosen = Some(file.clone());
                                external = true;
                                ui.close_menu();
                            }
                        });
                        response.on_hover_text(if let Some(old) = &file.old_path {
                            format!("{old}  →  {}", file.path)
                        } else {
                            file.path.clone()
                        });
                    }
                });
        });
        if let Some(file) = chosen {
            let tool = external
                .then(|| self.settings.matching_tool(&file.path).cloned())
                .flatten();
            self.choose_file(ctx, file);
            if let Some(tool) = tool {
                self.external(ctx, tool);
            }
        }
        if let (Some(file), Some(root)) = (staging, self.active.clone()) {
            self.operate(
                ctx,
                root,
                if file.area == Area::Staged {
                    Operation::Unstage(file.path)
                } else {
                    Operation::Stage(file.path)
                },
            );
        }
    }
}
