use crate::app::{Desk, Dialog, Selection};
use crate::git::{Area, Commit, History, HistoryScope, Operation, PAGE, Reference, TrackingState};
use crate::graph::{GraphRow, Span};
use crate::history::{Row, SearchDirection, matches_search};
use crate::theme;
use crate::ui::{
    CONTROL_HEIGHT, DETAIL_SIZE, ROW, centered_baseline, filter_field, fit_line, fit_line_job,
    galley_baseline, heading, paint_baseline, small_button,
};
use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, TimeZone};
use eframe::egui::text::{LayoutJob, TextFormat};
use eframe::egui::{self, Align, Color32, FontId, Rect, RichText, Sense, Stroke, Vec2, pos2};
use std::path::Path;
use std::time::Duration;

const FULL_COMMIT_TIME: &str = "%Y-%m-%d %H:%M";

const SUBJECT_SIZE: f32 = 14.0;
const GRAPH_INSET: f32 = 9.0;
const HISTORY_PREFETCH_DISTANCE: f32 = 12.0 * ROW;

enum Click {
    Worktree,
    CommitWorktree,
    Commit(usize, Commit, bool),
    CopyId(String),
    CopyMessage(String),
    Dialog(Dialog),
}

fn confirm_operation(
    root: &Path,
    title: &'static str,
    question: String,
    operation: Operation,
) -> Click {
    Click::Dialog(Dialog::Operation {
        root: root.to_path_buf(),
        title,
        question,
        operation,
    })
}

impl Desk {
    pub(crate) fn history_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let colors = theme::palette(ctx);
        let id = ui.make_persistent_id("find-commit");
        let shortcut = self.dialog.is_none()
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::F));
        let working = self.state().map(|state| state.changes().len());
        let scope = self
            .state()
            .map(|state| &state.filter)
            .unwrap_or(&HistoryScope::Head);
        let filter_label = match scope {
            HistoryScope::Head => "当前 HEAD".into(),
            HistoryScope::All => "全部分支".into(),
            HistoryScope::Selected(names) => format!("已选 {} 项  ×", names.len()),
        };
        let filter_tip = match scope {
            HistoryScope::Selected(names) => {
                Some(names.iter().cloned().collect::<Vec<_>>().join("\n"))
            }
            _ => None,
        };
        let busy = self
            .state()
            .is_some_and(|state| state.history_busy && state.rebase_message.is_none());
        let mut clear_filter = false;
        let mut head = false;
        let mut next = false;
        let mut previous = false;
        ui.horizontal_wrapped(|ui| {
            heading(ui, "提交图", None);
            self.repository_toolbar(ui, ctx);
        });
        ui.horizontal(|ui| {
            if let Some(tip) = &filter_tip {
                clear_filter = small_button(ui, &filter_label).on_hover_text(tip).clicked();
            } else {
                ui.label(RichText::new(&filter_label).small().color(colors.muted));
            }
            if busy {
                ui.spinner();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if small_button(ui, "定位 HEAD")
                    .on_hover_text("滚动到图中当前分支的 HEAD")
                    .clicked()
                {
                    head = true;
                }
                let edit = ui
                    .add_sized(
                        [
                            (ui.available_width()
                                - if self.search.is_empty() { 0.0 } else { 60.0 })
                            .clamp(64.0, 176.0),
                            CONTROL_HEIGHT,
                        ],
                        filter_field(
                            &mut self.search,
                            RichText::new("筛选提交…").color(colors.subtle),
                        )
                        .id(id),
                    )
                    .on_hover_text("筛选提交（Ctrl+F）");
                if shortcut {
                    edit.request_focus();
                }
                if edit.changed() {
                    self.search_row = None;
                    next = true;
                }
                if !self.search.trim().is_empty() {
                    if small_button(ui, "↓").on_hover_text("下一条匹配").clicked() {
                        next = true;
                    }
                    if small_button(ui, "↑").on_hover_text("上一条匹配").clicked() {
                        previous = true;
                    }
                }
                if (edit.has_focus() || edit.lost_focus())
                    && ctx.input(|input| input.key_pressed(egui::Key::Enter))
                {
                    if ctx.input(|input| input.modifiers.shift) {
                        previous = true;
                    } else {
                        next = true;
                    }
                    edit.request_focus();
                }
            });
        });
        if clear_filter {
            self.filter(ctx, HistoryScope::Head);
        }
        let query = self.search.trim().to_lowercase();
        let direction = if previous {
            Some(SearchDirection::Previous)
        } else if next {
            Some(SearchDirection::Next)
        } else {
            None
        };
        self.request_history_search(ctx, &query, direction);
        ui.add_space(5.0);
        if let Some(error) = self.state().and_then(|state| state.history_error.as_ref()) {
            ui.colored_label(colors.removed.foreground, error);
        }
        let Some((root, state)) = self.active.as_deref().zip(self.state()) else {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("添加一个 Git 仓库以打开提交图").color(colors.muted));
            });
            return;
        };
        let writing = self.operations.contains(root);
        let normal_write = !writing && state.normal_writes_available();
        let history = state.history.as_ref();
        let overview = state.history_info();
        let summary = history.map_or_else(Default::default, |history| history.summary);
        let lanes = summary.lanes;
        let head_index = state.head_row();
        let scroll_to = if head { head_index } else { self.scroll_to };
        let width = ui.available_width() - ui.spacing().scroll.allocated_width();
        let graph_width = graph_width(width, lanes);
        table_header(ui, width, graph_width);
        let today = Local::now().date_naive();
        ctx.request_repaint_after(Duration::from_secs(60));
        let baseline_offset = centered_baseline(ui.painter(), FontId::proportional(SUBJECT_SIZE));
        let rows = summary.loaded;
        let mut click = None;
        let mut missing_rows = false;
        let scroll = ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let mut scroll = egui::ScrollArea::vertical()
                .id_salt(("commit-list", self.graph_reset))
                .max_height((ui.available_height() - 27.0).max(20.0))
                .auto_shrink([false, false]);
            if let Some(index) = scroll_to {
                scroll = scroll.vertical_scroll_offset(index as f32 * ROW);
            }
            scroll.show_rows(ui, ROW, rows + 1, |ui, visible| {
                let mut missing_label = false;
                for index in visible {
                    let cached = index.checked_sub(1).and_then(|row| history?.row(row));
                    let (id, rect) = ui.allocate_space(Vec2::new(width, ROW));
                    let id = cached.map_or(id, |cached| {
                        ui.make_persistent_id(("history-commit", &cached.commit.id))
                    });
                    let response = ui.interact(
                        rect,
                        id,
                        if index == 0 || cached.is_some() {
                            Sense::click()
                        } else {
                            Sense::hover()
                        },
                    );
                    if index == 0 {
                        let selected = matches!(self.selection, Selection::Worktree);
                        row_fill(ui, rect, selected, &response, false);
                        let dot = pos2(rect.left() + GRAPH_INSET, rect.center().y);
                        if head_index.is_some() {
                            worktree_link(ui, dot, pos2(dot.x, rect.bottom()));
                        }
                        ui.painter().rect_filled(
                            Rect::from_center_size(dot, Vec2::splat(8.0)),
                            2.0,
                            colors.accent,
                        );
                        let columns = HistoryColumns::new(rect, graph_width);
                        let baseline = rect.center().y + baseline_offset;
                        let label = ui.painter().layout_no_wrap(
                            "工作树 / 暂存区".into(),
                            FontId::proportional(SUBJECT_SIZE),
                            colors.text,
                        );
                        paint_baseline(
                            ui.painter(),
                            columns.subject.left(),
                            baseline,
                            label,
                            colors.text,
                        );
                        let summary = if state.status_busy && state.status.is_none() {
                            "读取状态…".into()
                        } else if let Some(0) = working {
                            "工作树干净".into()
                        } else {
                            format!("{} 条变更", working.unwrap_or_default())
                        };
                        let status = ui.painter().layout_no_wrap(
                            summary,
                            FontId::proportional(DETAIL_SIZE),
                            colors.muted,
                        );
                        paint_baseline(
                            ui.painter(),
                            rect.right() - HistoryColumns::END_INSET - status.size().x,
                            baseline,
                            status,
                            colors.muted,
                        );
                        ui.painter().hline(
                            rect.x_range(),
                            rect.bottom(),
                            Stroke::new(1.0_f32, colors.divider),
                        );
                        if response.clicked() {
                            click = Some(Click::Worktree);
                        }
                        response.context_menu(|ui| {
                            let changes = state.changes();
                            let can_stage = changes.iter().any(|file| file.area == Area::Unstaged);
                            let can_discard = changes.iter().any(|file| {
                                file.area == Area::Unstaged && file.code != '?' && !file.is_gitlink()
                            }) && changes.iter().all(|file| file.code != 'U');
                            if ui.add_enabled(normal_write, egui::Button::new("提交…")).clicked() {
                                click = Some(Click::CommitWorktree);
                                ui.close_menu();
                            }
                            if ui
                                .add_enabled(!writing && can_stage, egui::Button::new("暂存全部…"))
                                .clicked()
                            {
                                click = Some(confirm_operation(
                                    root,
                                    "暂存全部",
                                    "暂存此工作树的所有改动，包括未被忽略的新文件？".into(),
                                    Operation::StageAll,
                                ));
                                ui.close_menu();
                            }
                            if ui
                                .add_enabled(
                                    !writing && can_discard,
                                    egui::Button::new("丢弃未暂存改动…"),
                                )
                                .clicked()
                            {
                                click = Some(confirm_operation(
                                    root,
                                    "丢弃未暂存改动",
                                    "从暂存区恢复已跟踪的文件？此操作会丢弃这些文件的未暂存改动，保留暂存内容和未跟踪文件。".into(),
                                    Operation::DiscardUnstaged,
                                ));
                                ui.close_menu();
                            }
                        });
                        continue;
                    }
                    let Some((cached, history)) = cached.zip(overview) else {
                        if !missing_label && rect.intersects(ui.clip_rect()) {
                            missing_rows = true;
                            let label = if state.rebase_message.is_some() {
                                "变基继续后刷新历史"
                            } else if state.history_error.is_some() && !state.history_busy {
                                "历史页不可用"
                            } else {
                                "读取历史…"
                            };
                            ui.painter().text(
                                rect.left_center() + Vec2::new(graph_width + 4.0, 0.0),
                                egui::Align2::LEFT_CENTER,
                                label,
                                FontId::proportional(DETAIL_SIZE),
                                colors.muted,
                            );
                            missing_label = true;
                        }
                        continue;
                    };
                    let commit = &cached.commit;
                    let graph = &cached.graph;
                    let selected = match &self.selection {
                        Selection::Commit { right, left, .. } => {
                            commit.id == right.id
                                || left.as_ref().is_some_and(|left| commit.id == left.id)
                        }
                        Selection::Worktree => false,
                    };
                    let matched = !query.is_empty() && matches_search(commit, history, &query);
                    row_fill(ui, rect, selected, &response, matched);
                    if let Some(head) = head_index
                        && index <= head + 1
                    {
                        let x = rect.left() + GRAPH_INSET;
                        let end = if index == head + 1 {
                            pos2(
                                lane_x(rect, graph_width, lanes, graph.lane),
                                rect.center().y,
                            )
                        } else {
                            pos2(x, rect.bottom())
                        };
                        worktree_link(ui, pos2(x, rect.top()), end);
                    }
                    draw_rails(
                        ui,
                        rect,
                        graph_width,
                        lanes,
                        graph,
                        commit.parents.len() > 1,
                        history.head.as_ref() == Some(&commit.id),
                    );
                    draw_commit(
                        ui,
                        rect,
                        graph_width,
                        history,
                        cached,
                        today,
                        !query.is_empty() && !matched && !selected,
                    );
                    if response.clicked() {
                        click = Some(Click::Commit(
                            index - 1,
                            commit.clone(),
                            ctx.input(|input| input.modifiers.command),
                        ));
                    }
                    response.context_menu(|ui| {
                        if ui.button("打开提交详情").clicked() {
                            click = Some(Click::Commit(index - 1, commit.clone(), false));
                            ui.close_menu();
                        }
                        if matches!(self.selection, Selection::Commit { .. })
                            && ui.button("与选中提交比较").clicked()
                        {
                            click = Some(Click::Commit(index - 1, commit.clone(), true));
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("复制消息").clicked() {
                            click = Some(Click::CopyMessage(commit.id.clone()));
                            ui.close_menu();
                        }
                        if ui.button("复制 ID").on_hover_text("复制完整 SHA").clicked() {
                            click = Some(Click::CopyId(commit.id.clone()));
                            ui.close_menu();
                        }
                        ui.separator();
                        ui.add_enabled_ui(normal_write, |ui| {
                            if ui.button("检出提交…").clicked() {
                                click = Some(confirm_operation(
                                    root,
                                    "检出提交",
                                    format!("检出 {}？HEAD 将分离到该提交。", commit.short),
                                    Operation::CheckoutCommit(commit.id.clone()),
                                ));
                                ui.close_menu();
                            }
                            if ui.button("合并提交…").clicked() {
                                click = Some(confirm_operation(
                                    root,
                                    "合并提交",
                                    format!("将 {} 合并到当前 HEAD？", commit.short),
                                    Operation::MergeCommit(commit.id.clone()),
                                ));
                                ui.close_menu();
                            }
                            if ui.button("摘取提交…").clicked() {
                                click = Some(if commit.parents.len() > 1 {
                                    Click::Dialog(Dialog::CherryPickMerge {
                                        root: root.to_path_buf(),
                                        commit: Box::new(commit.clone()),
                                        mainline: None,
                                    })
                                } else {
                                    confirm_operation(
                                        root,
                                        "摘取提交",
                                        format!("将 {} 的改动摘取到当前 HEAD？", commit.short),
                                        Operation::CherryPick {
                                            commit: commit.id.clone(),
                                            mainline: None,
                                        },
                                    )
                                });
                                ui.close_menu();
                            }
                        });
                        ui.separator();
                        ui.add_enabled_ui(!writing, |ui| {
                            for (label, tag) in [("创建分支…", false), ("创建标签…", true)] {
                                if ui.button(label).clicked() {
                                    click = Some(Click::Dialog(Dialog::NewCommitReference {
                                        root: root.to_path_buf(),
                                        commit: Box::new(commit.clone()),
                                        name: String::new(),
                                        tag,
                                    }));
                                    ui.close_menu();
                                }
                            }
                        });
                    });
                    response.on_hover_ui(|ui| {
                        ui.label(&commit.subject);
                        let time = local_author_time(commit).map_or_else(
                            || commit.date.clone(),
                            |time| time.format(FULL_COMMIT_TIME).to_string(),
                        );
                        ui.label(format!("{} · {} · {}", commit.author, time, commit.short));
                        if commit.parents.len() > 1 {
                            let parents = commit
                                .parents
                                .iter()
                                .map(|id| id.get(..10).unwrap_or(id))
                                .collect::<Vec<_>>()
                                .join(" →  ");
                            ui.label(format!("父提交（第一父在前）  {parents}"));
                        }
                        for branch in graph_references_at(history, &commit.id) {
                            ui.label(branch.description());
                        }
                        let hint = egui::WidgetText::from(
                            RichText::new("Ctrl + 单击与当前提交比较").color(colors.subtle),
                        )
                        .into_galley(
                            ui,
                            None,
                            ui.available_width(),
                            egui::TextStyle::Small,
                        );
                        let width = ui.min_rect().width().max(hint.size().x);
                        ui.allocate_ui_with_layout(
                            egui::vec2(width, hint.size().y),
                            egui::Layout::top_down(Align::RIGHT),
                            |ui| ui.label(hint),
                        );
                    });
                }
            })
        });
        let scroll = scroll.inner;
        let bottom = scroll.state.offset.y + scroll.inner_rect.height();
        let remaining = scroll.content_size.y - bottom;
        let first = ((scroll.state.offset.y / ROW).floor() as usize).saturating_sub(1);
        let last = ((bottom / ROW).ceil() as usize)
            .saturating_sub(1)
            .max(first);
        let pages = (first / PAGE).saturating_sub(1)..(last / PAGE + 2);
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(if query.is_empty() {
                    format!("已加载 {rows} 条提交")
                } else {
                    match self.history_search_result() {
                        Some(Ok(count)) => format!("{count} / {rows} 条匹配"),
                        Some(Err(error)) => format!("搜索失败：{error}"),
                        None if rows == 0 => "0 / 0 条匹配".into(),
                        None => "搜索中…".into(),
                    }
                })
                .color(colors.muted)
                .small(),
            );
            if history.is_some() {
                ui.label(
                    RichText::new(format!("· {} ms", summary.elapsed_ms))
                        .color(colors.subtle)
                        .small(),
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Selection::Commit { left: Some(_), .. } = self.selection {
                    ui.label(RichText::new("两提交比较").small().color(colors.accent));
                }
            });
        });
        self.report_history_frame(
            history.map(|history| history.generation),
            rows,
            missing_rows,
        );
        self.scroll_to = None;
        self.request_history_window(ctx, pages, remaining <= HISTORY_PREFETCH_DISTANCE);
        match click {
            Some(Click::Worktree) => self.choose_worktree(),
            Some(Click::CommitWorktree) => self.focus_worktree_commit(ctx),
            Some(Click::Commit(row, commit, extend)) => {
                self.choose_commit(ctx, row, commit, extend)
            }
            Some(Click::CopyId(id)) => self.copy_commit_id(ctx, id),
            Some(Click::CopyMessage(id)) => self.copy_commit_message(ctx, id),
            Some(Click::Dialog(dialog)) => self.dialog = Some(dialog),
            None => {}
        }
    }
}

fn graph_width(width: f32, lanes: usize) -> f32 {
    (35.0 + lanes.max(1) as f32 * 12.0).clamp(47.0, (width * 0.28).max(47.0))
}

struct HistoryColumns {
    subject: Rect,
    author: Option<Rect>,
    time: Option<Rect>,
    sha: Option<Rect>,
}

impl HistoryColumns {
    const END_INSET: f32 = 10.0;

    fn new(rect: Rect, graph_width: f32) -> Self {
        let mut right = rect.right() - Self::END_INSET;
        let mut column = |width| {
            let bounds = Rect::from_x_y_ranges((right - width)..=right, rect.y_range());
            right -= width + 14.0;
            bounds
        };
        let sha = (rect.width() >= 660.0).then(|| column(72.0));
        let time = (rect.width() >= 465.0).then(|| column(124.0));
        let author = (rect.width() >= 550.0).then(|| column(102.0));
        let left = rect.left() + graph_width + 4.0;
        let subject = Rect::from_x_y_ranges(left..=right.max(left), rect.y_range());
        Self {
            subject,
            author,
            time,
            sha,
        }
    }
}

fn table_header(ui: &mut egui::Ui, width: f32, graph_width: f32) {
    let colors = theme::palette(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, ROW), Sense::hover());
    let columns = HistoryColumns::new(rect, graph_width);
    let painter = ui.painter();
    let font = FontId::proportional(DETAIL_SIZE);
    let baseline = rect.center().y + centered_baseline(painter, font.clone());
    painter.rect_filled(rect, 3.0, colors.control);
    let graph = Rect::from_min_max(pos2(rect.left() + GRAPH_INSET, rect.top()), rect.max);
    for (column, label, align, color) in [
        (Some(graph), "图", Align::Min, colors.muted),
        (Some(columns.subject), "提交消息", Align::Min, colors.text),
        (columns.author, "作者", Align::Min, colors.muted),
        (columns.time, "时间", Align::Max, colors.muted),
        (columns.sha, "SHA", Align::Max, colors.muted),
    ] {
        if let Some(column) = column {
            let galley = painter.layout_no_wrap(label.into(), font.clone(), color);
            let x = align
                .align_size_within_range(galley.size().x, column.x_range())
                .min;
            paint_baseline(painter, x, baseline, galley, color);
        }
    }
}

fn row_fill(ui: &egui::Ui, rect: Rect, selected: bool, response: &egui::Response, matched: bool) {
    let colors = theme::palette(ui.ctx());
    let painter = ui.painter();
    if selected || response.hovered() || matched {
        let fill = if selected {
            colors.selection
        } else if response.hovered() {
            colors.hover
        } else {
            colors.control
        };
        painter.rect_filled(rect, 3.0, fill);
    }
    if selected {
        let y = (rect.top() + 3.0)..=(rect.bottom() - 3.0);
        painter.vline(rect.left() + 1.0, y, Stroke::new(2.0_f32, colors.accent));
    }
    if response.has_focus() {
        painter.rect_stroke(
            rect.shrink(0.5),
            3.0,
            Stroke::new(1.0_f32, colors.accent),
            egui::StrokeKind::Inside,
        );
    }
}

fn lane_gap(width: f32, lanes: usize) -> f32 {
    ((width - 28.0) / lanes.max(1) as f32).min(12.0)
}

fn lane_x(rect: Rect, width: f32, lanes: usize, lane: usize) -> f32 {
    rect.left() + 21.0 + lane as f32 * lane_gap(width, lanes)
}

fn worktree_link(ui: &egui::Ui, a: egui::Pos2, b: egui::Pos2) {
    let colors = theme::palette(ui.ctx());
    let stroke = Stroke::new(1.0_f32, colors.accent.gamma_multiply(0.45));
    if a.x == b.x {
        ui.painter()
            .extend(egui::Shape::dashed_line(&[a, b], stroke, 3.5, 3.0));
    } else {
        ui.painter()
            .add(egui::epaint::CubicBezierShape::from_points_stroke(
                [a, pos2(a.x, b.y), pos2(b.x - 6.0, b.y), b],
                false,
                Color32::TRANSPARENT,
                stroke,
            ));
    }
}

fn draw_rails(
    ui: &egui::Ui,
    rect: Rect,
    width: f32,
    lanes: usize,
    graph: &GraphRow,
    merge: bool,
    head: bool,
) {
    let colors = theme::palette(ui.ctx());
    let gap = lane_gap(width, lanes);
    let x = |lane: usize| lane_x(rect, width, lanes, lane);
    let y = |span: Span| match span {
        Span::Top => rect.top(),
        Span::Middle => rect.center().y,
        Span::Bottom => rect.bottom(),
    };
    let painter = ui.painter().with_clip_rect(Rect::from_x_y_ranges(
        rect.left()..=(rect.left() + width),
        ui.clip_rect().y_range(),
    ));
    let stroke_width = (gap * 0.4).clamp(0.75, 1.6);
    for rail in &graph.rails {
        let a = pos2(x(rail.from), y(rail.start));
        let b = pos2(x(rail.to), y(rail.end));
        let color = colors.lane(rail.color).gamma_multiply(0.94);
        if rail.from == rail.to {
            painter.line_segment([a, b], Stroke::new(stroke_width, color));
        } else {
            let bend = (b.y - a.y) * 0.48;
            painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
                [
                    a,
                    pos2(a.x, a.y + bend * 0.65),
                    pos2(b.x, b.y - bend * 0.65),
                    b,
                ],
                false,
                Color32::TRANSPARENT,
                Stroke::new(stroke_width, color),
            ));
        }
    }
    let point = pos2(x(graph.lane), rect.center().y);
    let radius = (gap * 0.36).clamp(1.15, 3.25);
    let color = colors.lane(graph.color);
    if head {
        painter.circle_stroke(point, 6.0, Stroke::new(1.2_f32, colors.accent));
    }
    if merge {
        let t = radius + 1.0;
        painter.add(egui::Shape::convex_polygon(
            vec![
                point + Vec2::new(0.0, -t),
                point + Vec2::new(t, 0.0),
                point + Vec2::new(0.0, t),
                point + Vec2::new(-t, 0.0),
            ],
            color,
            Stroke::NONE,
        ));
    } else {
        let radius = if head { 3.6 } else { radius };
        painter.circle_filled(point, radius, colors.base);
        painter.circle_stroke(point, radius, Stroke::new(1.55_f32, color));
    }
}

fn graph_label(reference: &Reference) -> (String, Option<&str>) {
    let label = reference.label();
    let Some(upstream) = &reference.upstream else {
        return (label.into(), None);
    };
    let remote = upstream.same_branch_remote(reference);
    let relation = remote.map_or_else(
        || format!("{label} · {}", upstream.label()),
        |remote| format!("{remote} · {label}"),
    );
    (relation, remote)
}

#[derive(Clone, Copy)]
enum RefBadgeStyle {
    Head,
    Local,
    Remote,
    Tag,
}

fn ref_label_job(label: String, color: Color32, subtle: Color32, remote_first: bool) -> LayoutJob {
    let font = FontId::proportional(DETAIL_SIZE);
    let Some((first, second)) = label.split_once(" · ") else {
        return LayoutJob::simple_singleline(label, font, color);
    };
    let mut job = LayoutJob::default();
    let (first_color, second_color) = if remote_first {
        (subtle, color)
    } else {
        (color, subtle)
    };
    for (text, color) in [
        (first, first_color),
        (" · ", subtle),
        (second, second_color),
    ] {
        job.append(text, 0.0, TextFormat::simple(font.clone(), color));
    }
    job
}

fn ref_counts_job(state: TrackingState, colors: &theme::Palette) -> LayoutJob {
    let mut job = LayoutJob::default();
    let font = FontId::monospace(DETAIL_SIZE);
    let mut append = |text: &str, color| {
        job.append(text, 0.0, TextFormat::simple(font.clone(), color));
    };
    match state {
        TrackingState::Counts { ahead, behind } if ahead > 0 && behind > 0 => {
            append(&format!("+{ahead} "), colors.added.foreground);
            append(&format!("−{behind}"), colors.warning);
        }
        state => {
            let color = match state {
                TrackingState::Counts { ahead, .. } if ahead > 0 => colors.added.foreground,
                TrackingState::Counts { behind, .. } if behind > 0 => colors.warning,
                TrackingState::Gone => colors.warning,
                _ => colors.muted,
            };
            append(&state.caption(), color);
        }
    }
    job
}

fn graph_references_at<'a>(history: &'a History, target: &str) -> Vec<&'a Reference> {
    let mut refs: Vec<_> = history
        .refs
        .iter()
        .filter(|reference| reference.target == target)
        .filter(|reference| !(reference.symbolic && reference.name.ends_with("/HEAD")))
        .collect();
    refs.sort_by_key(|reference| match reference {
        reference
            if reference.name.starts_with("refs/heads/") && reference.label() == history.branch =>
        {
            0
        }
        reference if reference.name.starts_with("refs/heads/") => 1,
        reference if reference.name.starts_with("refs/remotes/") => 2,
        _ => 3,
    });
    let attached: std::collections::HashSet<_> = refs
        .iter()
        .filter_map(|reference| {
            reference
                .upstream
                .as_ref()
                .map(|upstream| upstream.name.as_str())
        })
        .collect();
    refs.retain(|reference| {
        !reference.name.starts_with("refs/remotes/") || !attached.contains(reference.name.as_str())
    });
    refs
}

fn local_author_time(commit: &Commit) -> Option<NaiveDateTime> {
    let time = Local.timestamp_opt(commit.author_timestamp?, 0).single()?;
    time.naive_utc().checked_add_offset(*time.offset())
}

fn format_commit_time(time: NaiveDateTime, today: NaiveDate) -> (Option<&'static str>, String) {
    let days_ago = (today - time.date()).num_days();
    let prefix = match days_ago {
        0 => None,
        1 => Some("昨天"),
        2..=6 => {
            let weekdays = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"];
            Some(weekdays[time.weekday().num_days_from_monday() as usize])
        }
        _ => return (None, time.format(FULL_COMMIT_TIME).to_string()),
    };
    (prefix, time.format("%H:%M").to_string())
}

fn draw_commit(
    ui: &egui::Ui,
    rect: Rect,
    graph_width: f32,
    history: &History,
    row: &Row,
    today: NaiveDate,
    dim: bool,
) {
    let commit = &row.commit;
    let colors = theme::palette(ui.ctx());
    let columns = HistoryColumns::new(rect, graph_width);
    let painter = ui
        .painter()
        .with_clip_rect(columns.subject.intersect(ui.clip_rect()));
    let head = history.head.as_ref() == Some(&commit.id);
    let branches = graph_references_at(history, &commit.id);
    let badges = head
        .then(|| ("HEAD".to_owned(), None, None, false, RefBadgeStyle::Head))
        .into_iter()
        .chain(branches.into_iter().take(3).map(|branch| {
            let (label, same_branch_remote) = graph_label(branch);
            let compact = same_branch_remote
                .zip(branch.label().rsplit_once('/'))
                .map(|(remote, (_, leaf))| format!("{remote} · {leaf}"));
            let style = if branch.name.starts_with("refs/heads/") {
                RefBadgeStyle::Local
            } else if branch.name.starts_with("refs/remotes/") {
                RefBadgeStyle::Remote
            } else {
                RefBadgeStyle::Tag
            };
            let tracking = branch.upstream.as_ref().map(|upstream| upstream.state);
            (
                label,
                compact,
                tracking,
                same_branch_remote.is_some(),
                style,
            )
        }));
    // The subject starts at a fixed column on every row. Ref labels use only
    // the space they need at its trailing edge, leaving room for the message.
    let subject_width = columns.subject.width();
    let label_fraction = if subject_width < 340.0 { 0.47 } else { 0.52 };
    let remaining = (subject_width * label_fraction).min(300.0);
    let badge_inset = 5.0;
    let badge_gap = 4.0;
    let suffix_gap = 5.0;
    let icon_size = 14.0;
    let icon_gap = 3.0;
    let mut used = 0.0;
    let mut labels = Vec::new();
    for (label, compact, tracking, remote_first, style) in badges {
        let color = match style {
            RefBadgeStyle::Head => colors.accent,
            RefBadgeStyle::Local => colors.text,
            RefBadgeStyle::Remote => colors.lane(row.graph.color),
            RefBadgeStyle::Tag => colors.lane(2),
        };
        let icon_width = if matches!(style, RefBadgeStyle::Remote) {
            icon_size + icon_gap
        } else {
            0.0
        };
        let suffix = tracking.map(|state| painter.layout_job(ref_counts_job(state, &colors)));
        let suffix_width = suffix
            .as_ref()
            .map_or(0.0, |galley| galley.size().x + suffix_gap);
        let gap = if labels.is_empty() { 0.0 } else { badge_gap };
        let width =
            (remaining - used - gap - 2.0 * badge_inset - suffix_width - icon_width).min(210.0);
        if width < 22.0 {
            break;
        }
        let layout_label = |label| {
            fit_line_job(
                &painter,
                ref_label_job(label, color, colors.muted, remote_first),
                width,
            )
        };
        let mut label = layout_label(label);
        if label.elided
            && let Some(compact) = compact
        {
            let shorter = layout_label(compact);
            if !shorter.elided {
                label = shorter;
            }
        }
        let badge_width = label.size().x + suffix_width + icon_width + 2.0 * badge_inset;
        used += gap + badge_width;
        labels.push((label, suffix, badge_width, color, style));
    }

    let subject_color = if dim { colors.subtle } else { colors.text };
    let subject = fit_line(
        &painter,
        commit.subject.clone(),
        FontId::proportional(SUBJECT_SIZE),
        subject_color,
        columns.subject.width() - used - 5.0,
    );
    let y = rect.center().y;
    let baseline = y + centered_baseline(&painter, FontId::proportional(SUBJECT_SIZE));
    paint_baseline(
        &painter,
        columns.subject.left(),
        baseline,
        subject,
        subject_color,
    );
    let mut x = columns.subject.right() - used;
    let vertical_inset = 3.0;
    let mut badge_top = y - 8.5;
    let mut badge_bottom = y + 8.5;
    for (label, suffix, _, _, _) in &labels {
        for galley in std::iter::once(label).chain(suffix.as_ref()) {
            let text_y = baseline - galley_baseline(galley);
            badge_top = badge_top.min(text_y + galley.mesh_bounds.top() - vertical_inset);
            badge_bottom = badge_bottom.max(text_y + galley.mesh_bounds.bottom() + vertical_inset);
        }
    }
    for (label, suffix, width, color, style) in labels {
        let badge = Rect::from_min_max(pos2(x, badge_top), pos2(x + width, badge_bottom));
        let (fill, stroke) = match style {
            RefBadgeStyle::Local => (colors.accent.gamma_multiply(0.24), Stroke::NONE),
            RefBadgeStyle::Remote => (
                Color32::TRANSPARENT,
                Stroke::new(0.7_f32, colors.border.lerp_to_gamma(color, 0.48)),
            ),
            RefBadgeStyle::Head | RefBadgeStyle::Tag => (
                color.gamma_multiply(0.11),
                Stroke::new(0.7_f32, color.gamma_multiply(0.42)),
            ),
        };
        painter.rect(badge, 3.0, fill, stroke, egui::StrokeKind::Inside);
        let mut label_x = x + badge_inset;
        if matches!(style, RefBadgeStyle::Remote) {
            let icon = Rect::from_center_size(
                pos2(label_x + icon_size / 2.0, badge.center().y),
                Vec2::splat(icon_size),
            );
            egui::Image::new(egui::include_image!("../assets/icons/remote-cloud.svg"))
                .tint(color)
                .paint_at(ui, icon);
            label_x += icon_size + icon_gap;
        }
        let suffix_x = label_x + label.size().x + suffix_gap;
        paint_baseline(&painter, label_x, baseline, label, color);
        if let Some(suffix) = suffix {
            paint_baseline(&painter, suffix_x, baseline, suffix, color);
        }
        x += width + badge_gap;
    }

    if let Some(author) = columns.author {
        let name = fit_line(
            ui.painter(),
            commit.author.clone(),
            FontId::proportional(DETAIL_SIZE),
            colors.muted,
            author.width(),
        );
        paint_baseline(ui.painter(), author.left(), baseline, name, colors.muted);
    }
    let time = columns
        .time
        .and_then(|_| local_author_time(commit))
        .map(|time| format_commit_time(time, today));
    let (prefix, time) = time
        .as_ref()
        .map_or((None, commit.date.as_str()), |(prefix, value)| {
            (*prefix, value.as_str())
        });
    for (column, prefix, value) in [
        (columns.time, prefix, time),
        (columns.sha, None, commit.short.as_str()),
    ] {
        if let Some(column) = column {
            let painter = ui
                .painter()
                .with_clip_rect(column.intersect(ui.clip_rect()));
            let value =
                painter.layout_no_wrap(value.into(), FontId::monospace(DETAIL_SIZE), colors.subtle);
            let x = column.right() - value.size().x;
            paint_baseline(&painter, x, baseline, value, colors.subtle);
            if let Some(prefix) = prefix {
                let prefix = painter.layout_no_wrap(
                    prefix.into(),
                    FontId::proportional(DETAIL_SIZE),
                    colors.subtle,
                );
                let prefix_x = x - 5.0 - prefix.size().x;
                paint_baseline(&painter, prefix_x, baseline, prefix, colors.subtle);
            }
        }
    }
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        Stroke::new(0.75_f32, colors.divider),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::Upstream;

    #[test]
    fn commit_time_uses_local_calendar_days_across_new_year() {
        let today = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        for (year, month, day, expected_prefix, expected_time) in [
            (2026, 1, 1, None, "00:05"),
            (2025, 12, 31, Some("昨天"), "00:05"),
            (2025, 12, 30, Some("周二"), "00:05"),
            (2025, 12, 26, Some("周五"), "00:05"),
            (2025, 12, 25, None, "2025-12-25 00:05"),
            (2026, 1, 2, None, "2026-01-02 00:05"),
        ] {
            let time = NaiveDate::from_ymd_opt(year, month, day)
                .unwrap()
                .and_hms_opt(0, 5, 0)
                .unwrap();
            let (prefix, value) = format_commit_time(time, today);
            assert_eq!(prefix, expected_prefix);
            assert_eq!(value, expected_time);
        }
    }

    #[test]
    fn graph_combines_coincident_upstream_labels_and_keeps_disjoint_tips() {
        let colors = theme::palette(&egui::Context::default());
        let tracked = |local: &str, remote: &str, target: &str, behind| Reference {
            name: format!("refs/heads/{local}"),
            target: target.into(),
            upstream: Some(Upstream {
                name: format!("refs/remotes/{remote}"),
                state: TrackingState::Counts { ahead: 0, behind },
            }),
            symbolic: false,
        };
        let remote = |name: &str, target: &str| Reference {
            name: format!("refs/remotes/{name}"),
            target: target.into(),
            upstream: None,
            symbolic: false,
        };
        let history = History {
            branch: "release/banshu".into(),
            refs: vec![
                remote("origin/release/banshu", "same"),
                tracked("release/banshu", "origin/release/banshu", "same", 0),
                remote("origin/main", "remote-only"),
                tracked("main", "origin/main", "local-only", 12),
            ],
            ..History::default()
        };
        let same = graph_references_at(&history, "same");
        assert_eq!(same.len(), 1);
        assert_eq!(
            graph_label(same[0]),
            ("origin · release/banshu".into(), Some("origin"))
        );
        assert_eq!(
            ref_counts_job(same[0].upstream.as_ref().unwrap().state, &colors).text,
            "="
        );
        let local = graph_references_at(&history, "local-only");
        assert_eq!(
            graph_label(local[0]),
            ("origin · main".into(), Some("origin"))
        );
        assert_eq!(
            ref_counts_job(local[0].upstream.as_ref().unwrap().state, &colors).text,
            "−12"
        );
        let remote = graph_references_at(&history, "remote-only");
        assert_eq!(graph_label(remote[0]), ("origin/main".into(), None));
    }
}
