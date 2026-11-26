use crate::app::{Desk, Selection};
use crate::git::{Area, Change, Operation};
use crate::ui::{
    ACCENT, FAINT, GOLD, GREEN, HOVER, INK, MUTE, RED, ROW, SELECTION, SHADE, centered_baseline,
    fit_line, heading, paint_baseline, panel_divider, split_panel_height,
};
use eframe::egui::{self, Align, FontId, Layout, RichText, Sense, UiBuilder, Vec2};
use std::collections::HashMap;

const COMMIT_PANEL_MIN_HEIGHT: f32 = 144.0;
const COMMIT_PANEL_MAX_HEIGHT: f32 = 320.0;
const FILES_PANEL_MIN_HEIGHT: f32 = 132.0;
const FILE_NAME_INSET: f32 = 24.0;
const FILE_RIGHT_INSET: f32 = 7.0;
const FILE_COLUMN_GAP: f32 = 8.0;

enum FileRow<'a> {
    Group(&'static str, usize),
    File(&'a Change),
}

fn file_name_and_directory(path: &str) -> (&str, &str) {
    path.rsplit_once('/')
        .map_or((path, ""), |(directory, name)| (name, directory))
}

fn file_directory(
    painter: &egui::Painter,
    directory: &str,
    same_name_directories: &[&str],
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    let font = FontId::proportional(12.0);
    let full = fit_line(painter, directory.to_owned(), font.clone(), FAINT, width);
    if !full.elided {
        return full;
    }

    // A shared tail can hide the directory that distinguishes two files with the same name.
    let shared_tail = same_name_directories
        .iter()
        .copied()
        .filter(|other| *other != directory)
        .map(|other| {
            directory
                .rsplit('/')
                .zip(other.rsplit('/'))
                .take_while(|(left, right)| left == right)
                .count()
        })
        .max()
        .unwrap_or(0);
    let mut components = directory.rsplit('/');
    let Some(distinguishing) = components.nth(shared_tail) else {
        return full;
    };
    let prefix = if components.next().is_some() {
        "…/"
    } else {
        ""
    };
    let suffix = if shared_tail > 0 { "/…" } else { "" };
    fit_line(
        painter,
        format!("{prefix}{distinguishing}{suffix}"),
        font,
        FAINT,
        width,
    )
}

impl Desk {
    pub(crate) fn inspector(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let worktree = matches!(self.selection, Selection::Worktree);
        let active = self.active.clone();
        let busy = active
            .as_ref()
            .is_some_and(|root| self.operations.contains(root));
        let staged = self.state().map_or(0, |state| {
            state
                .changes()
                .iter()
                .filter(|file| file.area == Area::Staged)
                .count()
        });
        let splitter_id = ui.id().with("commit-files-splitter");
        let default_height = (ui.available_height() * 0.43).min(215.0);
        let height = split_panel_height(
            ui,
            splitter_id,
            default_height,
            (COMMIT_PANEL_MIN_HEIGHT..=COMMIT_PANEL_MAX_HEIGHT).into(),
            FILES_PANEL_MIN_HEIGHT,
        );
        let (_, section) = ui.allocate_space(Vec2::new(ui.available_width(), height));
        let mut commit = false;
        {
            let mut commit_panel = ui.new_child(
                UiBuilder::new()
                    .id_salt("commit-section")
                    .max_rect(section)
                    .layout(Layout::top_down(Align::Min)),
            );
            commit_panel.set_clip_rect(ui.clip_rect().intersect(section));
            let ui = &mut commit_panel;
            ui.horizontal(|ui| {
                heading(ui, if worktree { "提交" } else { "提交详情" }, None);
                if self.details_busy {
                    ui.spinner();
                }
            });
            ui.add_space(4.0);
            if worktree {
                let footer_height = ui.spacing().interact_size.y + ui.spacing().item_spacing.y;
                let editor_height = (ui.available_height() - footer_height).max(0.0);
                egui::ScrollArea::vertical()
                    .id_salt("commit-message")
                    .max_height(editor_height)
                    .min_scrolled_height(editor_height)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        // Keep the editor frame inside the scroll viewport in every state.
                        let widgets = &mut ui.visuals_mut().widgets;
                        widgets.hovered.expansion = 0.0;
                        widgets.active.expansion = 0.0;
                        ui.add_enabled(
                            active.is_some(),
                            egui::TextEdit::multiline(&mut self.editor)
                                .hint_text("写下本次提交的说明…")
                                .desired_rows(1)
                                .desired_width(f32::INFINITY)
                                .min_size(Vec2::new(0.0, editor_height)),
                        );
                    });
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
                egui::ScrollArea::vertical()
                    .id_salt("commit-detail")
                    .max_height(ui.available_height().max(0.0))
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
                            ui.label(RichText::new(&right.short).monospace().small().color(FAINT))
                                .on_hover_text(&right.id);
                        }
                    });
            } else if let Some(error) = &self.details_error {
                egui::ScrollArea::vertical()
                    .id_salt("commit-error")
                    .max_height(ui.available_height().max(0.0))
                    .show(ui, |ui| ui.colored_label(RED, error));
            }
        }
        if commit && let Some(root) = active {
            self.operate(ctx, root, Operation::Commit(self.editor.trim().to_owned()));
        }
        panel_divider(ui, splitter_id);
        self.files(ui, ctx);
    }

    fn files(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let worktree = matches!(self.selection, Selection::Worktree);
        let state = self.state();
        let count = if worktree {
            state.map_or(0, |state| state.changes().len())
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
            state.map(|state| state.changes()).unwrap_or_default()
        } else {
            self.details
                .as_ref()
                .map(|details| details.files.as_slice())
                .unwrap_or_default()
        };
        let mut entries = Vec::with_capacity(files.len() + 2);
        if worktree {
            for (name, area) in [("已暂存", Area::Staged), ("未暂存", Area::Unstaged)] {
                let group = files.iter().filter(|file| file.area == area);
                let count = group.clone().count();
                if count > 0 {
                    entries.push(FileRow::Group(name, count));
                    entries.extend(group.map(FileRow::File));
                }
            }
        } else {
            entries.extend(files.iter().map(FileRow::File));
        }
        let mut directories_by_name: HashMap<&str, Vec<&str>> = HashMap::new();
        for file in files {
            let (name, directory) = file_name_and_directory(&file.path);
            directories_by_name.entry(name).or_default().push(directory);
        }
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            egui::ScrollArea::vertical()
                .id_salt(("file-list", self.details_busy))
                .auto_shrink([false, false])
                .show_rows(ui, ROW, entries.len(), |ui, visible| {
                    let name_font = FontId::proportional(13.0);
                    let code_font = FontId::monospace(12.0);
                    let baseline_offset = centered_baseline(ui.painter(), name_font.clone());
                    for index in visible {
                        let (rect, response) = ui.allocate_exact_size(
                            Vec2::new(ui.available_width(), ROW),
                            Sense::click(),
                        );
                        let painter = ui.painter();
                        let right = rect.right() - FILE_RIGHT_INSET;
                        let baseline = rect.center().y + baseline_offset;
                        let file = match entries[index] {
                            FileRow::Group(name, count) => {
                                painter.rect_filled(rect, 4.0, SHADE);
                                let title = painter.layout_no_wrap(
                                    name.to_owned(),
                                    FontId::proportional(11.0),
                                    MUTE,
                                );
                                let count = painter.layout_no_wrap(
                                    count.to_string(),
                                    code_font.clone(),
                                    MUTE,
                                );
                                paint_baseline(painter, rect.left() + 7.0, baseline, title, MUTE);
                                paint_baseline(
                                    painter,
                                    right - count.size().x,
                                    baseline,
                                    count,
                                    MUTE,
                                );
                                continue;
                            }
                            FileRow::File(file) => file,
                        };
                        let is_selected = self.file.as_ref().is_some_and(|current| {
                            current.path == file.path && current.area == file.area
                        });
                        if is_selected || response.hovered() {
                            ui.painter().rect_filled(
                                rect,
                                4.0,
                                if is_selected { SELECTION } else { HOVER },
                            );
                        }
                        let color = match file.code {
                            'A' | '?' => GREEN,
                            'D' | 'U' => RED,
                            'R' => ACCENT,
                            _ => GOLD,
                        };
                        let code =
                            painter.layout_no_wrap(file.code.to_string(), code_font.clone(), color);
                        paint_baseline(painter, rect.left() + 6.0, baseline, code, color);
                        let (short, directory) = file_name_and_directory(&file.path);
                        let label = if file.is_gitlink() {
                            format!("{short} · Gitlink")
                        } else {
                            short.to_owned()
                        };
                        let name_x = rect.left() + FILE_NAME_INSET;
                        let mut name_width = (right - name_x).max(0.0);
                        if !directory.is_empty() {
                            let text_width = (name_width - FILE_COLUMN_GAP).max(0.0);
                            let natural_name_width = painter
                                .layout_no_wrap(label.clone(), name_font.clone(), INK)
                                .size()
                                .x;
                            let directory_width =
                                text_width - natural_name_width.min(text_width * 0.60);
                            let directory_galley = file_directory(
                                painter,
                                directory,
                                &directories_by_name[short],
                                directory_width,
                            );
                            let directory_size = directory_galley.size().x;
                            paint_baseline(
                                painter,
                                right - directory_size,
                                baseline,
                                directory_galley,
                                FAINT,
                            );
                            name_width = text_width - directory_size;
                        }
                        let name = fit_line(painter, label, name_font.clone(), INK, name_width);
                        paint_baseline(painter, name_x, baseline, name, INK);
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
        if let (Some(file), Some(root)) = (staging, self.active.clone())
            && let Some(operation) = file.index_operation()
        {
            self.operate(ctx, root, operation);
        }
    }
}
