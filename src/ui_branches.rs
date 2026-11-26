use crate::app::{Desk, Dialog};
use crate::git::{HistoryScope, Operation, Reference, TrackingState};
use crate::theme::{self, Palette};
use crate::ui::{
    DETAIL_SIZE, centered_baseline, compact_button, filter_field, fit_line, heading,
    paint_baseline, small_button,
};
use eframe::egui::{
    self, Align, Color32, FontId, Galley, Layout, Painter, RichText, Sense, Stroke, Vec2,
    collapsing_header::CollapsingState, pos2, text::LayoutJob,
};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

#[derive(Default)]
struct RefGroup<'a> {
    subgroups: BTreeMap<&'a str, RefGroup<'a>>,
    leaves: Vec<&'a Reference>,
    total: usize,
    checked: usize,
    contains_current: bool,
}

impl<'a> RefGroup<'a> {
    fn insert(&mut self, reference: &'a Reference, checked: bool, current: bool) {
        self.count(checked, current);
        let mut group = self;
        if let Some((path, _)) = reference.label().rsplit_once('/') {
            for name in path.split('/') {
                group = group.subgroups.entry(name).or_default();
                group.count(checked, current);
            }
        }
        group.leaves.push(reference);
    }

    fn count(&mut self, checked: bool, current: bool) {
        self.total += 1;
        self.checked += usize::from(checked);
        self.contains_current |= current;
    }

    fn set_checked(&self, selection: &mut BTreeSet<String>, checked: bool) {
        for reference in &self.leaves {
            if checked {
                selection.insert(reference.name.clone());
            } else {
                selection.remove(&reference.name);
            }
        }
        for subgroup in self.subgroups.values() {
            subgroup.set_checked(selection, checked);
        }
    }
}

const BRANCH_NAME_SIZE: f32 = 13.0;
const BRANCH_INSET: f32 = 3.0;
const UPSTREAM_GAP: f32 = 4.0;

const ROOTS: [(&str, &str, bool); 3] = [
    ("refs/heads/", "本地", true),
    ("refs/remotes/", "远程", false),
    ("refs/tags/", "标签", false),
];

fn build_groups<'a>(
    references: &'a [Reference],
    current: &str,
    needle: &str,
    selection: &BTreeSet<String>,
) -> [RefGroup<'a>; ROOTS.len()] {
    let mut groups = std::array::from_fn(|_| RefGroup::default());
    for reference in references {
        if !needle.is_empty() && !reference.label().to_lowercase().contains(needle) {
            continue;
        }
        if let Some(index) = ROOTS
            .iter()
            .position(|(prefix, _, _)| reference.name.starts_with(prefix))
        {
            let current = index == 0 && reference.label() == current;
            groups[index].insert(reference, selection.contains(&reference.name), current);
        }
    }
    groups
}

enum BranchAction {
    Filter(HistoryScope),
    Switch(String),
}

fn branch_checkbox(
    ui: &mut egui::Ui,
    checked: &mut bool,
    label: &str,
    indeterminate: bool,
) -> egui::Response {
    ui.scope(|ui| {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
        let widgets = &mut ui.visuals_mut().widgets;
        // Suppress egui's mark while retaining its box, text color and checkbox behavior.
        for visuals in [
            &mut widgets.noninteractive,
            &mut widgets.inactive,
            &mut widgets.hovered,
            &mut widgets.active,
            &mut widgets.open,
        ] {
            visuals.fg_stroke.width = 0.0;
        }
        let response = ui.add(egui::Checkbox::new(checked, label).indeterminate(indeterminate));
        if ui.is_rect_visible(response.rect) && (indeterminate || *checked) {
            let icon = if indeterminate {
                egui::include_image!("../assets/icons/branch-mixed.svg")
            } else {
                egui::include_image!("../assets/icons/branch-checked.svg")
            };
            let (_, icon_rect) = ui.spacing().icon_rectangles(response.rect);
            egui::Image::new(icon)
                .tint(ui.style().interact(&response).fg_stroke.color)
                .paint_at(ui, icon_rect);
        }
        response
    })
    .inner
}

struct BranchView<'a> {
    selection: &'a BTreeSet<String>,
    current: &'a str,
    searching: bool,
    action: Option<BranchAction>,
}

impl BranchView<'_> {
    fn toggle(&mut self, reference: &str) {
        let mut selection = self.selection.clone();
        if !selection.remove(reference) {
            selection.insert(reference.to_owned());
        }
        self.action = Some(BranchAction::Filter(HistoryScope::Selected(selection)));
    }

    fn only(&mut self, reference: &Reference) {
        let selection = BTreeSet::from([reference.name.clone()]);
        self.action = Some(BranchAction::Filter(HistoryScope::Selected(selection)));
    }

    fn group(
        &mut self,
        ui: &mut egui::Ui,
        key: &str,
        label: &str,
        group: &RefGroup<'_>,
        default_open: bool,
    ) {
        if group.total == 0 {
            return;
        }
        // Searching uses a separate widget ID so opening the matching paths does not
        // change the expansion state of the normal tree.
        let id = ui.make_persistent_id((key, self.searching));
        let mut state = CollapsingState::load_with_default_open(ui.ctx(), id, default_open);
        if self.searching {
            state.set_open(true);
        }

        let mut change_open = false;
        let mut header = state.show_header(ui, |ui| {
            let mut checked = group.checked == group.total;
            let hint = if self.searching {
                "勾选或取消勾选此组中匹配的引用"
            } else {
                "勾选或取消勾选此组及下级目录中的全部引用"
            };
            let indeterminate = group.checked > 0 && group.checked < group.total;
            if branch_checkbox(ui, &mut checked, "", indeterminate)
                .on_hover_text(hint)
                .changed()
            {
                let mut selection = self.selection.clone();
                group.set_checked(&mut selection, checked);
                self.action = Some(BranchAction::Filter(HistoryScope::Selected(selection)));
            }
            ui.add_space(BRANCH_INSET);
            let colors = theme::palette(ui.ctx());
            let font = FontId::proportional(BRANCH_NAME_SIZE);
            let directory = label.strip_suffix('/');
            let mut title = LayoutJob::simple_singleline(
                directory.unwrap_or(label).to_owned(),
                font.clone(),
                colors.text,
            );
            if directory.is_some() {
                title.append("/", 0.0, egui::TextFormat::simple(font, colors.subtle));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(BRANCH_INSET);
                ui.label(
                    RichText::new(group.total.to_string())
                        .size(BRANCH_NAME_SIZE)
                        .color(colors.subtle),
                );
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    change_open = ui
                        .add(egui::Label::new(title).truncate().sense(Sense::click()))
                        .clicked();
                });
            });
        });
        if self.searching {
            header.set_open(true);
        } else if change_open {
            header.toggle();
        }

        header.body(|ui| {
            for reference in &group.leaves {
                self.leaf(ui, reference);
            }
            for (&name, subgroup) in &group.subgroups {
                self.group(
                    ui,
                    name,
                    &format!("{name}/"),
                    subgroup,
                    subgroup.contains_current,
                );
            }
        });
    }

    fn leaf(&mut self, ui: &mut egui::Ui, reference: &Reference) {
        let colors = theme::palette(ui.ctx());
        let label = reference.label();
        let short = label.rsplit('/').next().unwrap_or(label);
        let local = reference.name.starts_with("refs/heads/");
        let current = local && label == self.current;
        let tooltip = reference.description();
        ui.push_id(&reference.name, |ui| {
            ui.horizontal(|ui| {
                // The current marker occupies the leaf's empty disclosure slot.
                let indent = ui.spacing().indent;
                let marker_x = ui.next_widget_position().x + indent / 2.0;
                ui.add_space(indent);
                let mut checked = self.selection.contains(&reference.name);
                let checkbox = branch_checkbox(ui, &mut checked, "", false).on_hover_text(&tooltip);
                if checkbox.changed() {
                    self.toggle(&reference.name);
                }
                if current {
                    ui.painter().circle_filled(
                        pos2(marker_x, checkbox.rect.center().y),
                        2.4,
                        colors.accent,
                    );
                }
                let response = branch_line(ui, reference, short, current);
                if response.clicked() {
                    if ui.input(|input| input.modifiers.command) {
                        self.toggle(&reference.name);
                    } else {
                        self.only(reference);
                    }
                }
                response.context_menu(|ui| {
                    if ui.button("只显示此引用的历史").clicked() {
                        self.only(reference);
                        ui.close_menu();
                    }
                    if local && !current && ui.button("切换到此分支…").clicked() {
                        self.action = Some(BranchAction::Switch(label.to_owned()));
                        ui.close_menu();
                    }
                    if ui.button("复制分支或标签名").clicked() {
                        ui.ctx().copy_text(label.to_owned());
                        ui.close_menu();
                    }
                });
                response.on_hover_text(&tooltip);
            });
        });
    }
}

fn branch_line(
    ui: &mut egui::Ui,
    reference: &Reference,
    name: &str,
    current: bool,
) -> egui::Response {
    let colors = theme::palette(ui.ctx());
    let width = ui.available_width();
    let height = ui.spacing().interact_size.y;
    let font = FontId::proportional(BRANCH_NAME_SIZE);
    let remote_font = FontId::proportional(DETAIL_SIZE);
    let color = if current { colors.accent } else { colors.text };
    let name_width = ui
        .painter()
        .layout_no_wrap(name.into(), font.clone(), color)
        .size()
        .x;
    let count_gap = 3.0;
    let mut text_width = width - 2.0 * BRANCH_INSET;
    let mut counts = Vec::new();
    let mut remote = None;
    let mut remote_on_second_line = false;
    if let Some(upstream) = &reference.upstream {
        let status = match upstream.state {
            TrackingState::Counts { ahead, behind } if ahead > 0 && behind > 0 => {
                vec![
                    (format!("+{ahead}"), colors.added.foreground),
                    (format!("−{behind}"), colors.warning),
                ]
            }
            state => vec![(state.caption(), tracking_color(state, colors))],
        };
        counts = status
            .into_iter()
            .map(|(caption, color)| {
                ui.painter()
                    .layout_no_wrap(caption, FontId::monospace(DETAIL_SIZE), color)
            })
            .collect();
        text_width -= counts
            .iter()
            .map(|count| count.size().x + count_gap)
            .sum::<f32>();
        let same = upstream.same_branch_remote(reference);
        let label = same.unwrap_or_else(|| upstream.label());
        let label_width = ui
            .painter()
            .layout_no_wrap(label.into(), remote_font.clone(), colors.subtle)
            .size()
            .x;
        // A short branch and its two counts remain readable even in the
        // smallest sidebar. An upstream with its own path can use a second line.
        remote_on_second_line = same.is_none()
            && name_width.min(52.0) + UPSTREAM_GAP + label_width.min(36.0) > text_width;
        remote = Some(label);
    }

    let extra = if remote_on_second_line { 16.0 } else { 0.0 };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height + extra), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), reference.label())
    });
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    if response.hovered() || current {
        painter.rect_filled(
            rect,
            3.0,
            if response.hovered() {
                colors.hover
            } else {
                colors.selection
            },
        );
    }
    if response.has_focus() {
        painter.rect_stroke(
            rect.shrink(0.5),
            3.0,
            Stroke::new(1.0_f32, colors.accent),
            egui::StrokeKind::Inside,
        );
    }

    let baseline = rect.top() + height / 2.0 + centered_baseline(&painter, font.clone());
    let left = rect.left() + BRANCH_INSET;
    let mut right = rect.right() - BRANCH_INSET;
    for count in counts.into_iter().rev() {
        right -= count.size().x;
        paint_baseline(&painter, right, baseline, count, colors.subtle);
        right -= count_gap;
    }
    let available = text_width.max(0.0);
    let inline_remote = remote.filter(|_| !remote_on_second_line && available - name_width >= 22.0);
    let branch = fit_line(&painter, name.into(), font, color, available);
    let branch_width = branch.size().x;
    paint_baseline(&painter, left, baseline, branch, color);
    if let Some(remote) = inline_remote {
        let start = left + branch_width + UPSTREAM_GAP;
        let label = fit_upstream(
            &painter,
            remote,
            remote_font.clone(),
            right - start,
            colors.subtle,
        );
        paint_baseline(&painter, start, baseline, label, colors.subtle);
    }
    if let Some(remote) = remote.filter(|_| remote_on_second_line) {
        let available = rect.right() - BRANCH_INSET - left;
        let label = fit_upstream(&painter, remote, remote_font, available, colors.subtle);
        painter.galley(
            pos2(left, rect.top() + height + (extra - label.size().y) / 2.0),
            label,
            colors.subtle,
        );
    }
    response
}

fn fit_upstream(
    painter: &Painter,
    upstream: &str,
    font: FontId,
    width: f32,
    color: Color32,
) -> Arc<Galley> {
    let full = fit_line(painter, upstream.into(), font.clone(), color, width);
    if !full.elided {
        return full;
    }
    if let Some((remote, path)) = upstream.split_once('/') {
        let leaf = path.rsplit('/').next().unwrap_or(path);
        for short in [
            format!("{remote}/…/{leaf}"),
            format!("{remote} · {leaf}"),
            format!("…/{leaf}"),
        ] {
            let label = fit_line(painter, short, font.clone(), color, width);
            if !label.elided {
                return label;
            }
        }
    }
    full
}

fn tracking_color(state: TrackingState, colors: &Palette) -> Color32 {
    match state {
        TrackingState::Counts { ahead, .. } if ahead > 0 => colors.added.foreground,
        TrackingState::Counts { behind, .. } if behind > 0 => colors.warning,
        TrackingState::Gone => colors.warning,
        _ => colors.subtle,
    }
}

impl Desk {
    pub(crate) fn branches(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let colors = theme::palette(ctx);
        let count = self
            .state()
            .and_then(|state| state.history.as_ref())
            .map(|history| history.cache.info.refs.len());
        let mut new = false;
        let mut action = None;
        ui.horizontal(|ui| {
            heading(ui, "分支", count);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                new = ui
                    .add_enabled(count.is_some(), compact_button("＋"))
                    .on_hover_text("新建并切换分支")
                    .clicked();
                if small_button(ui, "全部")
                    .on_hover_text("显示所有分支提交")
                    .clicked()
                {
                    action = Some(BranchAction::Filter(HistoryScope::All));
                }
            });
        });
        ui.add(
            filter_field(
                &mut self.branch_query,
                RichText::new("筛选分支和标签…").color(colors.subtle),
            )
            .desired_width(f32::INFINITY),
        );

        if let Some(state) = self.state()
            && let Some(history) = &state.history
        {
            let history = &history.cache.info;
            let needle = self.branch_query.trim().to_lowercase();
            let selection = state.filter.checks();
            let groups = build_groups(&history.refs, &history.branch, &needle, &selection);
            let mut view = BranchView {
                selection: &selection,
                current: &history.branch,
                searching: !needle.is_empty(),
                action,
            };
            let repository = self.active.as_ref();
            egui::ScrollArea::vertical()
                .id_salt(("branch-list", repository))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let spacing = ui.spacing_mut();
                    spacing.indent = 13.0;
                    spacing.interact_size.y = 21.0;
                    spacing.item_spacing = Vec2::new(6.0, 3.0);
                    spacing.button_padding.y = 2.0;
                    let mut head = selection.contains("HEAD");
                    let label = format!("HEAD · {}", history.branch);
                    if branch_checkbox(ui, &mut head, &label, false)
                        .on_hover_text(label)
                        .changed()
                    {
                        view.toggle("HEAD");
                    }
                    for ((key, label, open), group) in ROOTS.iter().zip(&groups) {
                        view.group(ui, key, label, group, *open);
                    }
                    if view.searching && groups.iter().all(|group| group.total == 0) {
                        ui.label(
                            RichText::new("没有匹配的分支或标签")
                                .small()
                                .color(colors.muted),
                        );
                    }
                });
            action = view.action;
        } else {
            let reason = if self.active.is_some() {
                "等待仓库历史"
            } else {
                "暂无分支"
            };
            ui.label(RichText::new(reason).color(colors.muted).small());
        }

        match action {
            Some(BranchAction::Filter(scope)) => self.filter(ctx, scope),
            Some(BranchAction::Switch(branch)) => {
                if let Some(root) = self.active.clone() {
                    self.dialog = Some(Dialog::Operation {
                        root,
                        question: format!("切换到 {branch}？Git 会检查当前工作树是否允许切换。"),
                        operation: Operation::Switch(branch),
                        title: "切换分支",
                    });
                }
            }
            None => {}
        }
        if new {
            self.dialog = Some(Dialog::NewBranch(String::new()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn references() -> Vec<Reference> {
        [
            "refs/heads/main",
            "refs/heads/release/banshu",
            "refs/heads/feature/client/ui",
            "refs/remotes/origin/feature/client/ui",
            "refs/remotes/origin/feature/client/audio",
            "refs/remotes/mirror/feature/client/ui",
            "refs/tags/release/v1",
        ]
        .map(|name| Reference {
            name: name.into(),
            target: String::new(),
            upstream: None,
            symbolic: false,
        })
        .to_vec()
    }

    #[test]
    fn retains_each_namespace_and_the_full_name_of_nested_refs() {
        let references = references();
        let selection = BTreeSet::from(["refs/heads/release/banshu".into()]);
        let [local, remote, tags] = build_groups(&references, "release/banshu", "", &selection);
        let release = &local.subgroups["release"];
        assert_eq!(local.leaves[0].label(), "main");
        assert!(release.contains_current);
        assert_eq!(release.checked, 1);
        assert_eq!(
            remote.subgroups["origin"].subgroups["feature"].subgroups["client"].total,
            2
        );
        assert_eq!(remote.subgroups["mirror"].total, 1);
        assert_eq!(
            tags.subgroups["release"].leaves[0].name,
            "refs/tags/release/v1"
        );
    }

    #[test]
    fn checking_a_filtered_subtree_only_changes_matching_descendants() {
        let references = references();
        let mut selection = BTreeSet::from([
            "HEAD".into(),
            "refs/remotes/origin/feature/client/audio".into(),
        ]);
        let [local, remote, _] = build_groups(&references, "main", "feature/client/ui", &selection);
        assert_eq!(local.total, 1);
        let origin = &remote.subgroups["origin"];
        assert_eq!(origin.total, 1);
        assert_eq!(origin.checked, 0);
        origin.set_checked(&mut selection, true);
        assert!(selection.contains("refs/remotes/origin/feature/client/ui"));
        assert!(!selection.contains("refs/remotes/mirror/feature/client/ui"));
        origin.set_checked(&mut selection, false);
        assert_eq!(
            selection,
            BTreeSet::from([
                "HEAD".into(),
                "refs/remotes/origin/feature/client/audio".into(),
            ])
        );
    }
}
