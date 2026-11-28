use crate::app::{Desk, Dialog};
use crate::git::{Operation, RecoveryAction, RecoveryKind};
use crate::theme;
use crate::ui::{CONTROL_HEIGHT, DETAIL_SIZE, compact_button};
use eframe::egui::{self, Align, Color32, Layout, RichText, Sense, Stroke, Vec2};

impl Desk {
    pub(crate) fn recovery_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let Some((root, state)) = self.active.as_ref().zip(self.state()) else {
            return;
        };
        let Some(recovery) = &state.recovery else {
            return;
        };
        let colors = theme::palette(ctx);
        let conflicts = state.conflicts();
        let busy = self.operations.contains(root);
        let message_id = state.rebase_message.as_ref().map(|message| message.id);
        let waiting_for_message = message_id.is_some();
        let ready = !busy && !state.status_busy && state.recovery_verified;
        let supported = !recovery.terminal_only;
        let can_skip = recovery.allows(RecoveryAction::Skip, conflicts);
        let phase = if waiting_for_message {
            "等待编辑提交说明".to_owned()
        } else if busy {
            "正在运行 Git…".to_owned()
        } else if state.status_busy {
            "正在核对 Git 状态…".to_owned()
        } else if !state.recovery_verified {
            "无法确认 Git 操作状态，请刷新".to_owned()
        } else if !supported {
            if conflicts > 0 {
                format!("{conflicts} 个文件冲突，请在终端处理")
            } else {
                "请在终端处理当前步骤".to_owned()
            }
        } else if conflicts > 0 {
            format!("{conflicts} 个文件冲突，解决后暂存")
        } else if recovery.kind == RecoveryKind::Merge {
            "等待创建合并提交".to_owned()
        } else {
            format!("可以{}", RecoveryAction::Continue.title(recovery.kind))
        };
        let detail = match recovery.progress {
            Some((step, total)) if ready => {
                format!("第 {step} / {total} 步  ·  {phase}")
            }
            _ => phase,
        };
        let warning = conflicts > 0 || !state.recovery_verified || !supported;
        let width = ui.available_width();
        let compact = width < 750.0;
        let buttons = if waiting_for_message {
            200.0
        } else {
            (if conflicts > 0 { 86.0 } else { 0.0 })
                + if !supported {
                    0.0
                } else if compact && can_skip {
                    160.0
                } else if can_skip {
                    293.0
                } else {
                    166.0
                }
        };
        let mut action = None;
        let mut show_conflicts = false;

        let (row, _) = ui.allocate_exact_size(Vec2::new(width, 44.0), Sense::hover());
        let text_right = (row.right() - buttons).max(row.left());
        let text_color = if warning {
            colors.warning
        } else {
            colors.muted
        };
        let heading = egui::Rect::from_min_max(row.min, egui::pos2(text_right, row.top() + 21.0));
        let mut heading = ui.new_child(
            egui::UiBuilder::new()
                .id_salt("heading")
                .max_rect(heading)
                .layout(Layout::left_to_right(Align::Center)),
        );
        recovery_icon(&mut heading, recovery.kind, colors.accent, colors.chrome);
        heading.add(
            egui::Label::new(RichText::new(recovery.kind.title()).size(14.0).strong()).truncate(),
        );
        if !waiting_for_message && (busy || state.status_busy) {
            heading.add(egui::Spinner::new().size(12.0));
        }

        let detail_rect = egui::Rect::from_min_max(
            egui::pos2((row.left() + 22.0).min(text_right), row.top() + 21.0),
            egui::pos2(text_right, row.bottom()),
        );
        ui.new_child(
            egui::UiBuilder::new()
                .id_salt("detail")
                .max_rect(detail_rect)
                .layout(Layout::left_to_right(Align::Center)),
        )
        .add(
            egui::Label::new(RichText::new(&detail).size(DETAIL_SIZE).color(text_color)).truncate(),
        )
        .on_hover_text(&detail);

        ui.painter().hline(
            (row.left() - 13.0)..=(row.right() + 13.0),
            row.bottom() + 6.0,
            Stroke::new(1.0_f32, colors.border),
        );
        if waiting_for_message || supported || conflicts > 0 {
            let action_rect =
                egui::Rect::from_min_max(egui::pos2(text_right, row.top()), row.right_bottom());
            let mut ui = ui.new_child(
                egui::UiBuilder::new()
                    .id_salt("actions")
                    .max_rect(action_rect)
                    .layout(Layout::right_to_left(Align::Center)),
            );
            if let Some(editor) = message_id {
                let reopen = ui.add(compact_button("编辑提交说明")).clicked();
                if ui.add(compact_button("中止…")).clicked() {
                    self.dialog = Some(Dialog::AbortRebaseMessage {
                        root: root.clone(),
                        editor,
                    });
                } else if reopen {
                    self.show_rebase_message();
                }
                return;
            }
            if supported {
                let can_continue = ready && recovery.allows(RecoveryAction::Continue, conflicts);
                let response = ui.add_enabled(
                    can_continue,
                    egui::Button::new(
                        RichText::new(RecoveryAction::Continue.title(recovery.kind))
                            .size(DETAIL_SIZE)
                            .color(if can_continue {
                                colors.accent
                            } else {
                                colors.subtle
                            })
                            .strong(),
                    )
                    .min_size(Vec2::new(76.0, CONTROL_HEIGHT)),
                );
                if response.clicked() {
                    action = Some(RecoveryAction::Continue);
                } else if conflicts > 0 {
                    response.on_disabled_hover_text("先解决并暂存所有冲突文件");
                }
                if compact && can_skip {
                    ui.add_enabled_ui(ready, |ui| {
                        ui.menu_button("更多", |ui| {
                            for (label, choice) in [
                                ("跳过当前提交…", RecoveryAction::Skip),
                                ("中止…", RecoveryAction::Abort),
                            ] {
                                if ui.button(label).clicked() {
                                    action = Some(choice);
                                    ui.close_menu();
                                }
                            }
                        });
                    });
                } else {
                    if ui.add_enabled(ready, compact_button("中止…")).clicked() {
                        action = Some(RecoveryAction::Abort);
                    }
                    if can_skip
                        && ui
                            .add_enabled(ready, compact_button("跳过当前提交…"))
                            .clicked()
                    {
                        action = Some(RecoveryAction::Skip);
                    }
                }
            }
            if conflicts > 0 {
                show_conflicts = ui.add(compact_button("查看冲突")).clicked();
            }
        }

        if let Some(action) = action {
            let title = action.title(recovery.kind);
            let question = match action {
                RecoveryAction::Abort => format!(
                    "将尝试恢复操作前的状态。\n本次操作的改动和冲突解决结果可能丢失。{}",
                    recovery
                        .branch
                        .as_ref()
                        .map_or(String::new(), |branch| format!("\n分支  {branch}"))
                ),
                RecoveryAction::Skip => {
                    let scope = if recovery.single_pick {
                        "跳过后本次摘取将结束。"
                    } else {
                        "Git 将跳过当前步骤并处理后续步骤。"
                    };
                    recovery.commit.as_deref().map_or_else(
                        || format!("{scope}\n本步改动和冲突解决结果可能丢失。"),
                        |commit| {
                            format!(
                                "当前提交  {}\n{scope}\n本步改动和冲突解决结果可能丢失。",
                                short_id(commit)
                            )
                        },
                    )
                }
                RecoveryAction::Continue => String::new(),
            };
            let root = root.clone();
            let operation = Operation::Recover {
                expected: Box::new(recovery.clone()),
                action,
            };
            if action == RecoveryAction::Continue {
                self.operate(ctx, root, operation);
            } else {
                self.dialog = Some(Dialog::Operation {
                    root,
                    operation,
                    title,
                    question,
                });
            }
        } else if show_conflicts {
            self.view_conflicts(ctx);
        }
    }

    #[cfg(windows)]
    pub(crate) fn open_conflict_file(&mut self, path: &str) {
        let result = (|| -> anyhow::Result<()> {
            let root = self
                .active
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("未选中工作树"))?;
            let file = root.join(crate::git::safe_relative(path)?);
            anyhow::ensure!(file.is_file(), "工作树中没有可编辑的文件");
            anyhow::ensure!(
                file.canonicalize()?.starts_with(root.canonicalize()?),
                "文件位于工作树之外"
            );
            let system = std::env::var_os("SystemRoot")
                .ok_or_else(|| anyhow::anyhow!("找不到 Windows 系统目录"))?;
            std::process::Command::new(
                std::path::PathBuf::from(system).join("System32/notepad.exe"),
            )
            .arg(file)
            .spawn()?;
            Ok(())
        })();
        if let Err(error) = result {
            self.tell(format!("无法打开工作树文件：{error:#}"), true);
        }
    }

    fn view_conflicts(&mut self, ctx: &egui::Context) {
        let Some(file) = self
            .state()
            .and_then(|state| {
                state
                    .changes()
                    .iter()
                    .filter(|file| file.code == 'U')
                    .min_by_key(|file| &file.path)
            })
            .cloned()
        else {
            return;
        };
        self.choose_worktree();
        if !crate::ui_inspector::file_filter(&self.file_query)(&&file) {
            self.file_query.clear();
        }
        self.stage_selection.insert(file.path.clone());
        self.reveal_conflict = true;
        self.choose_file(ctx, file);
    }
}

pub(crate) fn short_id(id: &str) -> &str {
    id.get(..8).unwrap_or(id)
}

fn recovery_icon(ui: &mut egui::Ui, kind: RecoveryKind, color: Color32, background: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
    let p = |x, y| rect.min + Vec2::new(x, y);
    let stroke = Stroke::new(1.3_f32, color);
    let line = |points: &[(f32, f32)]| {
        ui.painter().add(egui::Shape::line(
            points.iter().map(|&(x, y)| p(x, y)).collect(),
            stroke,
        ));
    };
    let node = |x, y| {
        ui.painter().circle(p(x, y), 1.55, background, stroke);
    };
    match kind {
        RecoveryKind::CherryPick => {
            line(&[(1.0, 13.0), (15.0, 13.0)]);
            line(&[
                (5.5, 3.0),
                (8.0, 3.0),
                (10.5, 4.2),
                (12.0, 6.4),
                (12.0, 9.0),
            ]);
            line(&[(10.0, 7.3), (12.0, 9.3), (14.0, 7.3)]);
            for (x, y) in [(4.5, 3.0), (3.0, 13.0), (13.0, 13.0)] {
                node(x, y);
            }
        }
        RecoveryKind::Rebase => {
            line(&[(1.0, 4.0), (10.0, 4.0), (10.0, 8.5)]);
            line(&[(8.0, 7.0), (10.0, 9.0), (12.0, 7.0)]);
            line(&[(3.0, 13.0), (15.0, 13.0)]);
            for (x, y) in [(3.0, 4.0), (8.0, 4.0), (5.0, 13.0), (13.0, 13.0)] {
                node(x, y);
            }
        }
        RecoveryKind::Merge => {
            line(&[(2.0, 3.5), (6.0, 3.5), (10.0, 8.0), (14.0, 8.0)]);
            line(&[(2.0, 12.5), (6.0, 12.5), (10.0, 8.0)]);
            for (x, y) in [(2.0, 3.5), (2.0, 12.5), (14.0, 8.0)] {
                node(x, y);
            }
        }
        RecoveryKind::ApplyMail | RecoveryKind::Revert | RecoveryKind::UnknownSequence => {
            ui.painter().rect_stroke(
                egui::Rect::from_min_max(p(0.5, 1.5), p(15.5, 14.5)),
                2.0,
                stroke,
                egui::StrokeKind::Inside,
            );
            line(&[(4.0, 5.5), (6.5, 8.0), (4.0, 10.5)]);
            line(&[(8.5, 10.5), (12.0, 10.5)]);
        }
    }
}
