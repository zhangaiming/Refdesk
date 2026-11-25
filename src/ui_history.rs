use crate::app::{Desk, Selection};
use crate::git::{Commit, History, HistoryScope, Reference};
use crate::graph::{GraphRow, Span};
use crate::ui::{ACCENT, BASE, BORDER, FAINT, HOVER, INK, MUTE, ROW, SHADE, paint, small_button};
use eframe::egui::{
    self, Align2, Color32, FontId, Rect, RichText, Sense, Stroke, Vec2, pos2, text::LayoutJob,
};

const PALETTE: [Color32; 8] = [
    Color32::from_rgb(85, 207, 169),
    Color32::from_rgb(117, 153, 246),
    Color32::from_rgb(227, 151, 89),
    Color32::from_rgb(190, 130, 226),
    Color32::from_rgb(245, 196, 91),
    Color32::from_rgb(118, 201, 226),
    Color32::from_rgb(236, 132, 152),
    Color32::from_rgb(176, 203, 104),
];

enum Click {
    Worktree,
    Commit(Commit, bool),
}

impl Desk {
    pub(crate) fn history_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let id = ui.make_persistent_id("find-commit");
        let shortcut =
            ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::F));
        let history = self.state().and_then(|state| state.history.as_ref());
        let count = history.map_or(0, |history| history.commits.len());
        let working = self.state().map(|state| state.changes.len());
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
        let busy = self.state().is_some_and(|state| state.history_busy);
        let mut clear_filter = false;
        let mut head = false;
        let mut next = false;
        let mut previous = false;
        ui.horizontal(|ui| {
            ui.label(RichText::new("提交图").color(INK).size(17.0).strong());
            if let Some(tip) = &filter_tip {
                clear_filter = small_button(ui, &filter_label).on_hover_text(tip).clicked();
            } else {
                ui.label(RichText::new(&filter_label).small().color(MUTE));
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
                let edit = ui.add_sized(
                    [166.0, 26.0],
                    egui::TextEdit::singleline(&mut self.search)
                        .id(id)
                        .hint_text("筛选提交…  Ctrl+F"),
                );
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
        ui.add_space(5.0);
        if let Some(error) = self.state().and_then(|state| state.history_error.as_ref()) {
            ui.colored_label(crate::ui::RED, error);
        }
        let Some(state) = self.state() else {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("添加一个 Git 仓库以打开提交图").color(MUTE));
            });
            return;
        };
        let query = self.search.trim().to_lowercase();
        // Keep commit positions stable while searching, so edges still point to their parents.
        let hits = state
            .history
            .as_ref()
            .map(|history| {
                history
                    .commits
                    .iter()
                    .enumerate()
                    .filter_map(|(index, commit)| {
                        (!query.is_empty() && matches_search(commit, history, &query))
                            .then_some(index)
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut search_row = self.search_row;
        let mut scroll_to = self.scroll_to;

        if head {
            scroll_to = state.history.as_ref().and_then(|history| {
                history
                    .head
                    .as_ref()
                    .and_then(|head| history.commits.iter().position(|commit| &commit.id == head))
            });
        } else if !hits.is_empty() && (next || previous) {
            let current = search_row.and_then(|index| hits.iter().position(|hit| *hit == index));
            let next = if previous {
                current.map_or(hits.len() - 1, |index| {
                    (index + hits.len() - 1) % hits.len()
                })
            } else {
                current.map_or(0, |index| (index + 1) % hits.len())
            };
            search_row = Some(hits[next]);
            scroll_to = search_row;
        }
        let graph_width = graph_width(ui.available_width(), state.graph.lanes);
        table_header(ui, graph_width);
        let has_more = state
            .history
            .as_ref()
            .is_some_and(|history| history.has_more);
        let limited = state.limit >= 3000;
        let rows = state
            .history
            .as_ref()
            .map_or(0, |history| history.commits.len());
        let head_index = state.history.as_ref().and_then(|history| {
            history
                .head
                .as_ref()
                .and_then(|head| history.commits.iter().position(|commit| &commit.id == head))
        });
        let mut click = None;
        let mut more = false;
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let mut scroll = egui::ScrollArea::vertical()
                .id_salt(("commit-list", self.graph_reset))
                .max_height((ui.available_height() - 27.0).max(20.0))
                .auto_shrink([false, false]);
            if let Some(index) = scroll_to {
                scroll = scroll.vertical_scroll_offset(index as f32 * ROW);
            }
            scroll.show_rows(ui, ROW, rows + 1, |ui, visible| {
                for index in visible {
                    let (rect, response) = ui
                        .allocate_exact_size(Vec2::new(ui.available_width(), ROW), Sense::click());
                    if index == 0 {
                        let selected = matches!(self.selection, Selection::Worktree);
                        row_fill(ui, rect, selected, response.hovered(), false);
                        let dot = pos2(rect.left() + 9.0, rect.center().y);
                        if head_index.is_some() {
                            worktree_link(ui, dot, pos2(dot.x, rect.bottom()));
                        }
                        ui.painter().rect_filled(
                            Rect::from_center_size(dot, Vec2::splat(8.0)),
                            2.0,
                            ACCENT,
                        );
                        paint(
                            ui,
                            rect,
                            rect.left() + graph_width + 4.0,
                            "工作树 / 暂存区",
                            FontId::proportional(14.0),
                            INK,
                        );
                        let summary = if state.status_busy && !state.has_status {
                            "读取状态…".into()
                        } else if let Some(0) = working {
                            "工作树干净".into()
                        } else {
                            format!("{} 条变更", working.unwrap_or_default())
                        };
                        paint(
                            ui,
                            rect,
                            rect.right() - 130.0,
                            &summary,
                            FontId::proportional(12.0),
                            MUTE,
                        );
                        if response.clicked() {
                            click = Some(Click::Worktree);
                        }
                        continue;
                    }
                    let Some(history) = state.history.as_ref() else {
                        continue;
                    };
                    let commit = &history.commits[index - 1];
                    let selected = match &self.selection {
                        Selection::Commit { right, left } => {
                            commit.id == right.id
                                || left.as_ref().is_some_and(|left| commit.id == left.id)
                        }
                        Selection::Worktree => false,
                    };
                    let matched = hits.binary_search(&(index - 1)).is_ok();
                    row_fill(ui, rect, selected, response.hovered(), matched);
                    if let Some(graph) = state.graph.rows.get(index - 1) {
                        if let Some(head) = head_index
                            && index <= head + 1
                        {
                            let x = rect.left() + 9.0;
                            let end = if index == head + 1 {
                                pos2(
                                    lane_x(rect, graph_width, state.graph.lanes, graph.lane),
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
                            state.graph.lanes,
                            graph,
                            commit.parents.len() > 1,
                            history.head.as_ref() == Some(&commit.id),
                        );
                    }
                    let lane_color = state
                        .graph
                        .rows
                        .get(index - 1)
                        .map(|row| PALETTE[row.color % PALETTE.len()]);
                    draw_commit(
                        ui,
                        rect,
                        graph_width,
                        history,
                        commit,
                        !query.is_empty() && !matched && !selected,
                        lane_color,
                    );
                    if response.clicked() {
                        click = Some(Click::Commit(
                            commit.clone(),
                            ctx.input(|input| input.modifiers.command),
                        ));
                    }
                    response.context_menu(|ui| {
                        if ui.button("复制完整 SHA").clicked() {
                            ui.ctx().copy_text(commit.id.clone());
                            ui.close_menu();
                        }
                        if ui.button("打开提交详情").clicked() {
                            click = Some(Click::Commit(commit.clone(), false));
                            ui.close_menu();
                        }
                        if matches!(self.selection, Selection::Commit { .. })
                            && ui.button("与选中提交比较").clicked()
                        {
                            click = Some(Click::Commit(commit.clone(), true));
                            ui.close_menu();
                        }
                    });
                    response.on_hover_ui(|ui| {
                        ui.label(&commit.subject);
                        ui.label(format!(
                            "{} · {} · {}",
                            commit.author, commit.date, commit.short
                        ));
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
                        ui.label("Ctrl + 单击与当前提交比较");
                    });
                }
            });
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(if query.is_empty() {
                    format!("{count} 条提交")
                } else {
                    format!("{} / {count} 条匹配", hits.len())
                })
                .color(MUTE)
                .small(),
            );
            if let Some(history) = state.history.as_ref() {
                ui.label(
                    RichText::new(format!("· {} ms", history.elapsed_ms))
                        .color(FAINT)
                        .small(),
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if has_more && !limited {
                    more = small_button(ui, "加载更多").clicked();
                } else if limited {
                    ui.label(RichText::new("已显示最近 3,000 条").small().color(MUTE));
                }
                if let Selection::Commit { left: Some(_), .. } = self.selection {
                    ui.label(RichText::new("两提交比较").small().color(ACCENT));
                }
            });
        });
        self.search_row = search_row;
        self.scroll_to = None;
        if more {
            self.more(ctx);
        }
        match click {
            Some(Click::Worktree) => self.choose_worktree(),
            Some(Click::Commit(commit, extend)) => self.choose_commit(ctx, commit, extend),
            None => {}
        }
    }
}

fn matches_search(commit: &Commit, history: &History, needle: &str) -> bool {
    commit.subject.to_lowercase().contains(needle)
        || commit.author.to_lowercase().contains(needle)
        || commit.id.contains(needle)
        || history.refs.iter().any(|reference| {
            reference.target == commit.id && reference.label().to_lowercase().contains(needle)
        })
}

fn graph_width(width: f32, lanes: usize) -> f32 {
    (31.0 + lanes as f32 * 13.0).clamp(53.0, (width * 0.38).max(53.0))
}

fn table_header(ui: &mut egui::Ui, graph_width: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 26.0), Sense::hover());
    ui.painter().rect_filled(rect, 4.0, SHADE);
    paint(
        ui,
        rect,
        rect.left() + 10.0,
        "DAG",
        FontId::monospace(10.0),
        MUTE,
    );
    paint(
        ui,
        rect,
        rect.left() + graph_width + 4.0,
        "提交消息",
        FontId::proportional(12.0),
        MUTE,
    );
    if rect.width() >= 630.0 {
        for (offset, name) in [(238.0, "作者"), (115.0, "日期"), (65.0, "SHA")] {
            paint(
                ui,
                rect,
                rect.right() - offset,
                name,
                FontId::proportional(11.0),
                MUTE,
            );
        }
    } else if rect.width() >= 420.0 {
        paint(
            ui,
            rect,
            rect.right() - 65.0,
            "SHA",
            FontId::proportional(11.0),
            MUTE,
        );
    }
}

fn row_fill(ui: &egui::Ui, rect: Rect, selected: bool, hovered: bool, matched: bool) {
    if selected || hovered || matched {
        let color = if selected {
            HOVER
        } else if hovered {
            SHADE
        } else {
            Color32::from_rgb(37, 53, 57)
        };
        ui.painter().rect_filled(rect, 3.0, color);
    }
    if selected {
        ui.painter()
            .vline(rect.left(), rect.y_range(), Stroke::new(2.0_f32, ACCENT));
    }
}

fn lane_x(rect: Rect, width: f32, lanes: usize, lane: usize) -> f32 {
    let gap = ((width - 34.0) / lanes.max(1) as f32).clamp(2.0, 13.0);
    rect.left() + 23.0 + lane as f32 * gap
}

fn worktree_link(ui: &egui::Ui, a: egui::Pos2, b: egui::Pos2) {
    let stroke = Stroke::new(1.0_f32, ACCENT.gamma_multiply(0.45));
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
    let x = |lane: usize| lane_x(rect, width, lanes, lane);
    let y = |span: Span| match span {
        Span::Top => rect.top(),
        Span::Middle => rect.center().y,
        Span::Bottom => rect.bottom(),
    };
    let painter = ui
        .painter()
        .with_clip_rect(rect.intersect(Rect::from_min_max(
            rect.min,
            pos2(rect.left() + width, rect.bottom()),
        )));
    for rail in &graph.rails {
        let a = pos2(x(rail.from), y(rail.start));
        let b = pos2(x(rail.to), y(rail.end));
        let color = PALETTE[rail.color % PALETTE.len()].gamma_multiply(0.94);
        if rail.from == rail.to {
            painter.line_segment([a, b], Stroke::new(1.75_f32, color));
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
                Stroke::new(1.75_f32, color),
            ));
        }
    }
    let point = pos2(x(graph.lane), rect.center().y);
    let color = PALETTE[graph.color % PALETTE.len()];
    if head {
        painter.circle_stroke(point, 6.0, Stroke::new(1.2_f32, ACCENT));
    }
    if merge {
        let t = 4.3;
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
        let radius = if head { 3.6 } else { 3.2 };
        painter.circle_filled(point, radius, BASE);
        painter.circle_stroke(point, radius, Stroke::new(1.55_f32, color));
    }
}

fn graph_label(reference: &Reference) -> (String, String) {
    let label = reference.label();
    let Some(upstream) = &reference.upstream else {
        return (label.into(), String::new());
    };
    let relation = if let Some(remote) = upstream.same_branch_remote(reference) {
        format!("{remote} · {label}")
    } else {
        format!("{label} · {}", upstream.label())
    };
    (relation, upstream.state.caption())
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

fn draw_commit(
    ui: &egui::Ui,
    rect: Rect,
    graph_width: f32,
    history: &History,
    commit: &Commit,
    dim: bool,
    lane_color: Option<Color32>,
) {
    let mut text_x = rect.left() + graph_width + 4.0;
    let subject_end = if rect.width() >= 630.0 {
        rect.right() - 249.0
    } else if rect.width() >= 420.0 {
        rect.right() - 75.0
    } else {
        rect.right() - 5.0
    };
    let badge_limit = (text_x + 285.0).min(subject_end - 95.0);
    let text_clip = Rect::from_min_max(
        pos2(text_x, rect.top()),
        pos2(subject_end.max(text_x), rect.bottom()),
    );
    let painter = ui
        .painter()
        .with_clip_rect(text_clip.intersect(ui.clip_rect()));
    let head = history.head.as_ref() == Some(&commit.id);
    let branches = graph_references_at(history, &commit.id);
    let badges = head
        .then(|| ("HEAD".to_owned(), String::new(), ACCENT))
        .into_iter()
        .chain(branches.into_iter().take(3).map(|branch| {
            let (label, counts) = graph_label(branch);
            let color = if branch.name.starts_with("refs/tags/") {
                PALETTE[2]
            } else {
                lane_color.unwrap_or(MUTE)
            };
            (label, counts, color)
        }));
    let font = FontId::proportional(11.0);
    for (label, counts, color) in badges {
        let suffix =
            (!counts.is_empty()).then(|| painter.layout_no_wrap(counts, font.clone(), color));
        let suffix_width = suffix.as_ref().map_or(0.0, |galley| galley.size().x + 6.0);
        let label_width = ((badge_limit - text_x - 10.0) - suffix_width).min(210.0);
        if label_width < 22.0 {
            break;
        }
        let mut job = LayoutJob::simple_singleline(label, font.clone(), color);
        job.wrap.max_width = label_width;
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        let galley = painter.layout_job(job);
        let width = galley.size().x;
        let badge = Rect::from_min_size(
            pos2(text_x, rect.center().y - 8.5),
            Vec2::new(width + suffix_width + 10.0, 17.0),
        );
        painter.rect_filled(badge, 4.0, color.gamma_multiply(0.13));
        let label_pos = pos2(badge.left() + 5.0, badge.center().y - galley.size().y / 2.0);
        painter.galley(label_pos, galley, color);
        if let Some(suffix) = suffix {
            let pos = pos2(
                label_pos.x + width + 6.0,
                badge.center().y - suffix.size().y / 2.0,
            );
            painter.galley(pos, suffix, color);
        }
        text_x = badge.right() + 5.0;
    }
    painter.text(
        pos2(text_x, rect.center().y),
        Align2::LEFT_CENTER,
        &commit.subject,
        FontId::proportional(14.0),
        if dim { FAINT } else { INK },
    );
    if rect.width() >= 630.0 {
        let author = Rect::from_min_max(
            pos2(rect.right() - 238.0, rect.top()),
            pos2(rect.right() - 123.0, rect.bottom()),
        );
        paint(
            ui,
            author,
            author.left(),
            &commit.author,
            FontId::proportional(12.0),
            MUTE,
        );
        paint(
            ui,
            rect,
            rect.right() - 115.0,
            commit.date.get(5..).unwrap_or(&commit.date),
            FontId::proportional(11.0),
            MUTE,
        );
    }
    if rect.width() >= 420.0 {
        paint(
            ui,
            rect,
            rect.right() - 70.0,
            &commit.short,
            FontId::monospace(10.5),
            FAINT,
        );
    }
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        Stroke::new(0.4_f32, BORDER.gamma_multiply(0.55)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::{TrackingState, Upstream};

    #[test]
    fn graph_combines_coincident_upstream_labels_and_keeps_disjoint_tips() {
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
            ("origin · release/banshu".into(), "=".into())
        );
        let local = graph_references_at(&history, "local-only");
        assert_eq!(
            graph_label(local[0]),
            ("origin · main".into(), "−12".into())
        );
        let remote = graph_references_at(&history, "remote-only");
        assert_eq!(
            graph_label(remote[0]),
            ("origin/main".into(), String::new())
        );
    }
}
