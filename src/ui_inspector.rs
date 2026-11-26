use crate::app::{Desk, Selection};
use crate::git::{Area, Change, Operation};
use crate::theme;
use crate::ui::{
    ROW, centered_baseline, filter_field, fit_line, heading, paint_baseline, panel_divider,
    save_icon_button, split_panel_height,
};
use eframe::egui::{self, Align, FontId, Layout, Rect, RichText, Sense, UiBuilder, Vec2};
use std::collections::HashMap;

const COMMIT_PANEL_MIN_HEIGHT: f32 = 144.0;
const COMMIT_PANEL_MAX_HEIGHT: f32 = 320.0;
const FILES_PANEL_MIN_HEIGHT: f32 = 132.0;
const FILE_NAME_INSET: f32 = 24.0;
const FILE_RIGHT_INSET: f32 = 7.0;
const FILE_COLUMN_GAP: f32 = 8.0;
const FILE_GROUP_VERTICAL_INSET: f32 = 3.0;
const COPY_SHA_LABEL: &str = "复制 SHA";
const COPY_TOAST_DURATION: f32 = 1.1;

#[derive(Clone, Copy)]
struct CopyShaToast {
    pointer: egui::Pos2,
    started: f64,
}

fn copy_sha_button(ui: &mut egui::Ui, icon: &egui::TextureHandle, sha: &str, toast_id: egui::Id) {
    let colors = theme::palette(ui.ctx());
    let response = save_icon_button(ui, icon, COPY_SHA_LABEL, colors.subtle);
    if response.clicked() {
        ui.ctx().copy_text(sha.to_owned());
        let toast = CopyShaToast {
            pointer: response
                .interact_pointer_pos()
                .unwrap_or(response.rect.center()),
            started: ui.input(|input| input.time),
        };
        ui.data_mut(|data| data.insert_temp(toast_id, toast));
    }
    if response.hovered() && ui.data(|data| data.get_temp::<CopyShaToast>(toast_id).is_none()) {
        response.show_tooltip_text(COPY_SHA_LABEL);
    }
}

fn paint_copy_sha_toast(ctx: &egui::Context, toast_id: egui::Id) {
    let colors = theme::palette(ctx);
    let Some(toast) = ctx.data(|data| data.get_temp::<CopyShaToast>(toast_id)) else {
        return;
    };
    let elapsed = ctx.input(|input| (input.time - toast.started) as f32);
    if elapsed >= COPY_TOAST_DURATION {
        ctx.data_mut(|data| data.remove::<CopyShaToast>(toast_id));
        return;
    }
    let fade_in = (elapsed / 0.15).clamp(0.0, 1.0);
    let fade_out = ((COPY_TOAST_DURATION - elapsed) / 0.27).clamp(0.0, 1.0);
    let opacity = fade_in * fade_out;
    let drift = 6.0 * (1.0 - (1.0 - (elapsed / 0.28).clamp(0.0, 1.0)).powi(3));
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, toast_id));
    let text = painter.layout_no_wrap("已复制".into(), FontId::proportional(12.0), colors.text);
    let padding = Vec2::new(8.0, 4.0);
    let size = text.size() + padding * 2.0;
    let bounds = ctx.screen_rect().shrink(6.0);
    let target = toast.pointer + Vec2::new(12.0, 13.0);
    let position = egui::pos2(
        target.x.min(bounds.right() - size.x).max(bounds.left()),
        target
            .y
            .min(bounds.bottom() - size.y - 6.0)
            .max(bounds.top())
            + drift,
    );
    let rect = Rect::from_min_size(position, size);
    let visuals = ctx.style();
    let visuals = &visuals.visuals;
    painter.rect(
        rect,
        5.0,
        visuals.window_fill.gamma_multiply(opacity),
        egui::Stroke::new(1.0_f32, visuals.window_stroke.color.gamma_multiply(opacity)),
        egui::StrokeKind::Inside,
    );
    painter.galley_with_override_text_color(
        position + padding,
        text,
        visuals.text_color().gamma_multiply(opacity),
    );
    ctx.request_repaint();
}

enum FileRow<'a> {
    Group {
        name: &'static str,
        area: Area,
        count: usize,
    },
    File(&'a Change),
}

fn file_rows<'a>(
    files: &'a [Change],
    worktree: bool,
    query: &str,
    collapsed_groups: &[Area],
) -> Vec<FileRow<'a>> {
    let needle = query.trim().replace('\\', "/").to_lowercase();
    let matches = |file: &&Change| needle.is_empty() || file.path.to_lowercase().contains(&needle);
    let mut rows = Vec::with_capacity(files.len() + 2);
    if worktree {
        for (name, area) in [("已暂存", Area::Staged), ("未暂存", Area::Unstaged)] {
            let group_start = rows.len();
            rows.extend(
                files
                    .iter()
                    .filter(|file| file.area == area)
                    .filter(matches)
                    .map(FileRow::File),
            );
            let count = rows.len() - group_start;
            if count > 0 {
                if collapsed_groups.contains(&area) {
                    rows.truncate(group_start);
                }
                rows.insert(group_start, FileRow::Group { name, area, count });
            }
        }
    } else {
        rows.extend(files.iter().filter(matches).map(FileRow::File));
    }
    rows
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
    color: egui::Color32,
) -> std::sync::Arc<egui::Galley> {
    let font = FontId::proportional(12.0);
    let full = fit_line(painter, directory.to_owned(), font.clone(), color, width);
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
        color,
        width,
    )
}

impl Desk {
    pub(crate) fn inspector(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let colors = theme::palette(ctx);
        let toast_id = ui.id().with("copy-sha-toast");
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
                                .hint_text(
                                    RichText::new("写下本次提交的说明…").color(colors.subtle),
                                )
                                .desired_rows(1)
                                .desired_width(f32::INFINITY)
                                .min_size(Vec2::new(0.0, editor_height)),
                        );
                    });
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("{staged} 项已暂存"))
                            .small()
                            .color(colors.muted),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        commit = ui
                            .add_enabled(
                                staged > 0 && !self.editor.trim().is_empty() && !busy,
                                egui::Button::new(
                                    RichText::new("提交").color(colors.accent).strong(),
                                ),
                            )
                            .clicked();
                    });
                });
            } else if let Some(details) = &self.details {
                egui::ScrollArea::vertical()
                    .id_salt("commit-detail")
                    .max_height(ui.available_height().max(0.0))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(&details.title)
                                .size(14.0)
                                .strong()
                                .color(colors.text),
                        );
                        ui.label(
                            RichText::new(format!("{} · {}", details.author, details.date))
                                .size(11.0)
                                .color(colors.muted),
                        );
                        let (summary, rest) = details
                            .message
                            .split_once('\n')
                            .unwrap_or((&details.message, ""));
                        if details.title != summary {
                            ui.label(RichText::new(summary).small().color(colors.muted));
                        }
                        if !rest.trim().is_empty() {
                            ui.label(RichText::new(rest.trim()).size(12.0).color(colors.muted));
                        }
                        if let Selection::Commit { right, .. } = &self.selection {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 3.0;
                                ui.label(
                                    RichText::new(&right.short)
                                        .monospace()
                                        .small()
                                        .color(colors.subtle),
                                )
                                .on_hover_text(&right.id);
                                copy_sha_button(ui, &self.save_icon, &right.id, toast_id);
                            });
                        }
                    });
            } else if let Some(error) = &self.details_error {
                egui::ScrollArea::vertical()
                    .id_salt("commit-error")
                    .max_height(ui.available_height().max(0.0))
                    .show(ui, |ui| ui.colored_label(colors.removed.foreground, error));
            }
        }
        if commit && let Some(root) = active {
            self.operate(ctx, root, Operation::Commit(self.editor.trim().to_owned()));
        }
        panel_divider(ui, splitter_id);
        self.files(ui, ctx);
        paint_copy_sha_toast(ctx, toast_id);
    }

    fn files(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let colors = theme::palette(ctx);
        let worktree = matches!(self.selection, Selection::Worktree);
        let state = self.state();
        let status_busy = state.is_some_and(|state| state.status_busy);
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
            if worktree && status_busy {
                ui.spinner();
            }
        });
        let query_changed = ui
            .add(
                filter_field(
                    &mut self.file_query,
                    RichText::new("筛选文件…").color(colors.subtle),
                )
                .desired_width(f32::INFINITY),
            )
            .changed();
        let state = self.state();
        if worktree && let Some(error) = state.and_then(|state| state.status_error.as_ref()) {
            ui.colored_label(colors.removed.foreground, error);
        }
        let files = if worktree {
            state.map(|state| state.changes()).unwrap_or_default()
        } else {
            self.details
                .as_ref()
                .map(|details| details.files.as_slice())
                .unwrap_or_default()
        };
        let collapsed_groups_id = ui.id().with("collapsed-file-groups");
        let collapsed_groups = ctx
            .data(|data| data.get_temp::<Vec<Area>>(collapsed_groups_id))
            .unwrap_or_default();
        let entries = file_rows(files, worktree, &self.file_query, &collapsed_groups);
        if entries.is_empty() {
            if !files.is_empty() {
                ui.label(
                    RichText::new("未找到匹配的文件")
                        .color(colors.muted)
                        .small(),
                );
            } else if !status_busy && !self.details_busy {
                ui.label(
                    RichText::new(if worktree {
                        "没有待比较的文件"
                    } else {
                        "该提交没有文件变更"
                    })
                    .color(colors.muted)
                    .small(),
                );
            }
        }
        let mut directories_by_name: HashMap<&str, Vec<&str>> = HashMap::new();
        for file in files {
            let (name, directory) = file_name_and_directory(&file.path);
            directories_by_name.entry(name).or_default().push(directory);
        }
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let list = egui::ScrollArea::vertical().id_salt(("file-list", self.details_busy));
            let list = if query_changed {
                list.vertical_scroll_offset(0.0)
            } else {
                list
            };
            list.auto_shrink([false, false])
                .show_rows(ui, ROW, entries.len(), |ui, visible| {
                    let name_font = FontId::proportional(13.0);
                    let code_font = FontId::monospace(12.0);
                    let baseline_offset = centered_baseline(ui.painter(), name_font.clone());
                    for index in visible {
                        let (row_id, rect) =
                            ui.allocate_space(Vec2::new(ui.available_width(), ROW));
                        let painter = ui.painter();
                        let right = rect.right() - FILE_RIGHT_INSET;
                        let baseline = rect.center().y + baseline_offset;
                        let file = match entries[index] {
                            FileRow::Group { name, area, count } => {
                                let response = ui
                                    .interact(rect, collapsed_groups_id.with(name), Sense::click())
                                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                                response.widget_info(|| {
                                    egui::WidgetInfo::labeled(
                                        egui::WidgetType::CollapsingHeader,
                                        ui.is_enabled(),
                                        name,
                                    )
                                });
                                let collapsed = collapsed_groups.contains(&area);
                                if response.clicked() {
                                    ctx.data_mut(|data| {
                                        let groups = data.get_temp_mut_or_default::<Vec<Area>>(
                                            collapsed_groups_id,
                                        );
                                        if collapsed {
                                            groups.retain(|group| *group != area);
                                        } else {
                                            groups.push(area);
                                        }
                                    });
                                    ctx.request_repaint();
                                }
                                let fill = if response.hovered() || response.has_focus() {
                                    colors.hover
                                } else {
                                    colors.control
                                };
                                let background =
                                    rect.shrink2(Vec2::new(0.0, FILE_GROUP_VERTICAL_INSET));
                                painter.rect_filled(background, 4.0, fill);
                                let marker = painter.layout_no_wrap(
                                    if collapsed { "+" } else { "-" }.to_owned(),
                                    code_font.clone(),
                                    colors.muted,
                                );
                                let title = painter.layout_no_wrap(
                                    name.to_owned(),
                                    FontId::proportional(11.0),
                                    colors.muted,
                                );
                                let count = painter.layout_no_wrap(
                                    count.to_string(),
                                    code_font.clone(),
                                    colors.muted,
                                );
                                paint_baseline(
                                    painter,
                                    rect.left() + 6.0,
                                    baseline,
                                    marker,
                                    colors.muted,
                                );
                                paint_baseline(
                                    painter,
                                    rect.left() + FILE_NAME_INSET,
                                    baseline,
                                    title,
                                    colors.muted,
                                );
                                paint_baseline(
                                    painter,
                                    right - count.size().x,
                                    baseline,
                                    count,
                                    colors.muted,
                                );
                                continue;
                            }
                            FileRow::File(file) => file,
                        };
                        let response = ui.interact(rect, row_id, Sense::click());
                        let is_selected = self.file.as_ref().is_some_and(|current| {
                            current.path == file.path && current.area == file.area
                        });
                        if is_selected || response.hovered() {
                            ui.painter().rect_filled(
                                rect,
                                4.0,
                                if is_selected {
                                    colors.selection
                                } else {
                                    colors.hover
                                },
                            );
                        }
                        let color = match file.code {
                            'A' | '?' => colors.added.foreground,
                            'D' | 'U' => colors.removed.foreground,
                            'R' => colors.accent,
                            _ => colors.warning,
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
                                .layout_no_wrap(label.clone(), name_font.clone(), colors.text)
                                .size()
                                .x;
                            let directory_width =
                                text_width - natural_name_width.min(text_width * 0.60);
                            let directory_galley = file_directory(
                                painter,
                                directory,
                                &directories_by_name[short],
                                directory_width,
                                colors.subtle,
                            );
                            let directory_size = directory_galley.size().x;
                            paint_baseline(
                                painter,
                                right - directory_size,
                                baseline,
                                directory_galley,
                                colors.subtle,
                            );
                            name_width = text_width - directory_size;
                        }
                        let name =
                            fit_line(painter, label, name_font.clone(), colors.text, name_width);
                        paint_baseline(painter, name_x, baseline, name, colors.text);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn change(path: &str, area: Area) -> Change {
        Change {
            path: path.to_owned(),
            old_path: None,
            code: 'M',
            area,
            left_gitlink: false,
            right_gitlink: false,
        }
    }

    #[test]
    fn file_filter_matches_names_and_relative_paths() {
        let files = [
            change("文档/Sample Notes.md", Area::Commit),
            change("other/Sample Notes.md", Area::Commit),
        ];
        let rows = file_rows(&files, false, " 文档\\sAMPLE n ", &[]);
        let [FileRow::File(file)] = rows.as_slice() else {
            panic!("expected one matching file");
        };
        assert!(std::ptr::eq(*file, &files[0]));
        assert_eq!(file_rows(&files, false, "SAMPLE NOTES", &[]).len(), 2);
        assert_eq!(file_rows(&files, false, "  ", &[]).len(), 2);
        assert!(file_rows(&files, false, "missing", &[]).is_empty());
    }

    #[test]
    fn file_filter_counts_matching_rows_and_hides_empty_groups() {
        let files = [
            change("src/main.rs", Area::Staged),
            change("src/main.rs", Area::Unstaged),
            change("README.md", Area::Unstaged),
        ];
        let rows = file_rows(&files, true, "src/", &[]);
        let [
            FileRow::Group {
                area: Area::Staged,
                count: 1,
                ..
            },
            FileRow::File(staged),
            FileRow::Group {
                area: Area::Unstaged,
                count: 1,
                ..
            },
            FileRow::File(unstaged),
        ] = rows.as_slice()
        else {
            panic!("expected one matching file in each group");
        };
        assert_eq!(staged.area, Area::Staged);
        assert_eq!(unstaged.area, Area::Unstaged);

        let readme = file_rows(&files, true, "readme", &[]);
        assert!(matches!(
            readme.as_slice(),
            [FileRow::Group { area: Area::Unstaged, count: 1, .. }, FileRow::File(file)] if file.path == "README.md"
        ));
    }

    #[test]
    fn collapsed_group_keeps_matching_count_and_hides_only_its_files() {
        let files = [
            change("src/main.rs", Area::Staged),
            change("README.md", Area::Staged),
            change("src/lib.rs", Area::Unstaged),
        ];
        let rows = file_rows(&files, true, "src/", &[Area::Staged]);
        assert!(matches!(
            rows.as_slice(),
            [
                FileRow::Group { area: Area::Staged, count: 1, .. },
                FileRow::Group { area: Area::Unstaged, count: 1, .. },
                FileRow::File(file),
            ] if file.path == "src/lib.rs"
        ));
    }
}
