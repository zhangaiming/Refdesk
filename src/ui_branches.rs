use crate::app::{Desk, Dialog};
use crate::git::{HistoryScope, Operation, Reference};
use crate::ui::{ACCENT, FAINT, MUTE, heading, small_button};
use eframe::egui::{
    self, Align, FontId, Layout, RichText, TextFormat, collapsing_header::CollapsingState,
    text::LayoutJob,
};
use std::collections::{BTreeMap, BTreeSet};

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
            if ui
                .add(
                    egui::Checkbox::new(&mut checked, "")
                        .indeterminate(group.checked > 0 && group.checked < group.total),
                )
                .on_hover_text(hint)
                .changed()
            {
                let mut selection = self.selection.clone();
                group.set_checked(&mut selection, checked);
                self.action = Some(BranchAction::Filter(HistoryScope::Selected(selection)));
            }
            change_open = ui
                .add(
                    egui::Label::new(RichText::new(format!("{label}  {}", group.total)).size(12.0))
                        .truncate()
                        .sense(egui::Sense::click()),
                )
                .clicked();
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
        let label = reference.label();
        let short = label.rsplit('/').next().unwrap_or(label);
        let local = reference.name.starts_with("refs/heads/");
        let current = local && label == self.current;
        let caption = if current {
            format!("●  {short}")
        } else {
            short.into()
        };
        let tooltip = reference.description();
        ui.push_id(&reference.name, |ui| {
            ui.horizontal(|ui| {
                // Reserve the disclosure column so leaf choices line up with group choices.
                ui.add_space(ui.spacing().indent);
                let mut checked = self.selection.contains(&reference.name);
                if ui
                    .checkbox(&mut checked, "")
                    .on_hover_text(&tooltip)
                    .changed()
                {
                    self.toggle(&reference.name);
                }
                let font = FontId::proportional(12.0);
                let color = if current { ACCENT } else { MUTE };
                let mut row = LayoutJob::default();
                row.append(&caption, 0.0, TextFormat::simple(font.clone(), color));
                let mut counts = None;
                let mut width = ui.available_width();
                if let Some(upstream) = &reference.upstream {
                    let name = upstream
                        .same_branch_remote(reference)
                        .unwrap_or_else(|| upstream.label());
                    row.append(
                        &format!("  {name}"),
                        0.0,
                        TextFormat::simple(font.clone(), FAINT),
                    );
                    let status = upstream.state.caption();
                    let galley = ui.painter().layout_no_wrap(status, font, color);
                    width = (width - galley.size().x - ui.spacing().item_spacing.x).max(0.0);
                    counts = Some(galley);
                }
                let response = ui
                    .allocate_ui_with_layout(
                        egui::vec2(width, ui.spacing().interact_size.y),
                        Layout::left_to_right(Align::Center),
                        |ui| ui.add(egui::Label::new(row).truncate().sense(egui::Sense::click())),
                    )
                    .inner;
                if let Some(counts) = counts {
                    ui.label(counts).on_hover_text(&tooltip);
                }
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

impl Desk {
    pub(crate) fn branches(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let count = self
            .state()
            .and_then(|state| state.history.as_ref())
            .map(|history| history.refs.len());
        let mut new = false;
        let mut action = None;
        ui.horizontal(|ui| {
            heading(ui, "分支", count);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                new = ui
                    .add_enabled(count.is_some(), egui::Button::new("＋").small())
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
            egui::TextEdit::singleline(&mut self.branch_query)
                .hint_text("筛选分支和标签…")
                .desired_width(f32::INFINITY),
        );

        if let Some(state) = self.state()
            && let Some(history) = &state.history
        {
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
                    spacing.item_spacing.y = 3.0;
                    spacing.button_padding.y = 2.0;
                    let mut head = selection.contains("HEAD");
                    if ui
                        .checkbox(&mut head, format!("HEAD · {}", history.branch))
                        .changed()
                    {
                        view.toggle("HEAD");
                    }
                    for ((key, label, open), group) in ROOTS.iter().zip(&groups) {
                        view.group(ui, key, label, group, *open);
                    }
                    if view.searching && groups.iter().all(|group| group.total == 0) {
                        ui.label(RichText::new("没有匹配的分支或标签").small().color(MUTE));
                    }
                });
            action = view.action;
        } else {
            let reason = if self.active.is_some() {
                "等待仓库历史"
            } else {
                "暂无分支"
            };
            ui.label(RichText::new(reason).color(MUTE).small());
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
