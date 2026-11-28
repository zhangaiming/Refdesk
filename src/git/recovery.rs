use super::{checked, command, oid, output, run, status, trim};
use anyhow::{Context, Result, ensure};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

mod editor;
pub(crate) use editor::{
    ARGUMENT as REBASE_EDITOR_ARGUMENT, RebaseMessageReply, RebaseMessageRequest,
    run_helper as run_rebase_editor,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RecoveryKind {
    CherryPick,
    Rebase,
    Merge,
    ApplyMail,
    Revert,
    UnknownSequence,
}

impl RecoveryKind {
    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::CherryPick => "摘取进行中",
            Self::Rebase => "变基进行中",
            Self::Merge => "合并进行中",
            Self::ApplyMail => "正在应用补丁",
            Self::Revert => "撤销提交进行中",
            Self::UnknownSequence => "Git 操作待确认",
        }
    }

    fn command(self) -> Option<&'static str> {
        match self {
            Self::CherryPick => Some("cherry-pick"),
            Self::Rebase => Some("rebase"),
            Self::Merge => Some("merge"),
            Self::ApplyMail | Self::Revert | Self::UnknownSequence => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RecoveryAction {
    Continue,
    Skip,
    Abort,
}

impl RecoveryAction {
    pub(crate) fn title(self, kind: RecoveryKind) -> &'static str {
        match (self, kind) {
            (Self::Continue, RecoveryKind::CherryPick) => "继续摘取",
            (Self::Continue, RecoveryKind::Rebase) => "继续变基",
            (Self::Continue, RecoveryKind::Merge) => "继续合并",
            (Self::Skip, _) => "跳过当前提交",
            (Self::Abort, RecoveryKind::CherryPick) => "中止摘取",
            (Self::Abort, RecoveryKind::Rebase) => "中止变基",
            (Self::Abort, RecoveryKind::Merge) => "中止合并",
            (Self::Continue, _) => "继续 Git 操作",
            (Self::Abort, _) => "中止 Git 操作",
        }
    }

    fn flag(self) -> &'static str {
        match self {
            Self::Continue => "--continue",
            Self::Skip => "--skip",
            Self::Abort => "--abort",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RecoveryState {
    pub kind: RecoveryKind,
    pub commit: Option<String>,
    pub branch: Option<String>,
    pub onto: Option<String>,
    pub progress: Option<(usize, usize)>,
    pub single_pick: bool,
    pub terminal_only: bool,
    skippable: bool,
    needs_message_editor: bool,
    head: Option<String>,
    // Persistent markers identify the exact step, including multi-target merges and
    // repeated occurrences of the same commit in a sequence.
    step: Vec<String>,
    started_at: Option<SystemTime>,
}

impl RecoveryState {
    fn new(kind: RecoveryKind, head: Option<&str>) -> Self {
        Self {
            kind,
            commit: None,
            branch: None,
            onto: None,
            progress: None,
            single_pick: false,
            terminal_only: kind.command().is_none(),
            skippable: false,
            needs_message_editor: false,
            head: head.map(str::to_owned),
            step: Vec::new(),
            started_at: None,
        }
    }

    pub(crate) fn allows(&self, action: RecoveryAction, conflicts: usize) -> bool {
        !self.terminal_only
            && match action {
                RecoveryAction::Continue => conflicts == 0,
                RecoveryAction::Skip => self.skippable,
                RecoveryAction::Abort => true,
            }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct WorktreeGitDir {
    pub(crate) path: PathBuf,
    reftable: bool,
}

pub(crate) fn worktree_git_dir(root: &Path) -> Result<WorktreeGitDir> {
    let result = trim(&run(
        root,
        &["rev-parse", "--absolute-git-dir", "--show-ref-format"],
    )?);
    let mut lines = result.lines();
    let path = PathBuf::from(lines.next().context("缺少工作树的 Git 目录")?);
    ensure!(path.is_absolute(), "无法识别工作树的 Git 目录");
    // Git versions predating reftable can echo the unrecognized option.
    Ok(WorktreeGitDir {
        path,
        reftable: lines.next() == Some("reftable"),
    })
}

fn read_root_ref(root: &Path, git: &WorktreeGitDir, name: &str) -> Result<Option<String>> {
    if !git.reftable {
        return read_marker(&git.path, name);
    }
    let result = output(root, &["rev-parse", "--verify", "--quiet", name], true)?;
    if result.status.code() == Some(1) && result.stderr.is_empty() {
        return Ok(None);
    }
    Ok(Some(trim(&checked(result)?)))
}

fn read_marker(dir: &Path, name: &str) -> Result<Option<String>> {
    match std::fs::read(dir.join(name)) {
        Ok(bytes) => Ok(Some(trim(&bytes))),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("无法读取 Git 操作状态（{name}）")),
    }
}

fn has_marker(dir: &Path, name: &str) -> Result<bool> {
    dir.join(name)
        .try_exists()
        .with_context(|| format!("无法确认 Git 操作状态（{name}）"))
}

fn marker_time(dir: &Path, name: &str) -> Result<Option<SystemTime>> {
    match dir.join(name).metadata() {
        Ok(metadata) => Ok(Some(metadata.modified()?)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).context("无法读取 Git 操作标记时间"),
    }
}

fn instruction_needs_editor(line: &str) -> bool {
    let mut words = line.split_ascii_whitespace();
    match words.next() {
        Some("reword" | "r" | "squash" | "s") => true,
        Some("fixup" | "f") => words.next() == Some("-c"),
        Some("merge" | "m") => words.next() != Some("-C"),
        _ => false,
    }
}

fn last_rebase_instruction(done: &str) -> Option<&str> {
    done.lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
}

fn editor_commit(dir: &Path) -> Option<String> {
    let done = read_marker(dir, "rebase-merge/done").ok()??;
    let mut words = last_rebase_instruction(&done)?.split_ascii_whitespace();
    words.next()?;
    let mut candidate = words.next()?;
    if matches!(candidate, "-c" | "-C") {
        candidate = words.next()?;
    }
    (candidate.len() >= 7 && candidate.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| candidate.to_owned())
}

fn rebase(
    root: &Path,
    git: &WorktreeGitDir,
    head: Option<&str>,
    apply: bool,
) -> Result<RecoveryState> {
    let dir = &git.path;
    let backend = if apply {
        "rebase-apply"
    } else {
        "rebase-merge"
    };
    let read = |name| read_marker(dir, &format!("{backend}/{name}"));
    let branch = read("head-name")?;
    let onto = read("onto")?;
    let position = read(if apply { "next" } else { "msgnum" })?;
    let total = read(if apply { "last" } else { "end" })?;
    let original = read("orig-head")?;
    let stopped = read(if apply {
        "original-commit"
    } else {
        "stopped-sha"
    })?;
    let commit = match stopped {
        Some(id) => Some(id),
        None => read_root_ref(root, git, "REBASE_HEAD")?,
    }
    .filter(|id| oid(id));
    let done = if apply { None } else { read("done")? };
    let todo = if apply {
        None
    } else {
        read("git-rebase-todo")?
    };
    let last = done.as_deref().and_then(last_rebase_instruction);
    let command = last.and_then(|line| line.split_ascii_whitespace().next());
    let mut state = RecoveryState::new(RecoveryKind::Rebase, head);
    state.started_at = marker_time(dir, &format!("{backend}/orig-head"))?;
    state.progress = position
        .as_deref()
        .and_then(|number| number.parse::<usize>().ok())
        .zip(
            total
                .as_deref()
                .and_then(|number| number.parse::<usize>().ok()),
        )
        .filter(|(current, total)| *current > 0 && current <= total);
    state.terminal_only = !original.as_deref().is_some_and(oid)
        || !onto.as_deref().is_some_and(oid)
        || (!apply
            && !matches!(
                command,
                Some(
                    "pick" | "p" | "reword" | "r" | "squash" | "s" | "fixup" | "f" | "merge" | "m"
                )
            ));
    state.needs_message_editor = last.is_some_and(instruction_needs_editor)
        || todo
            .as_deref()
            .is_some_and(|todo| todo.lines().any(instruction_needs_editor));
    state.skippable =
        !state.terminal_only && (commit.is_some() || (apply && state.progress.is_some()));
    state.branch = branch.and_then(|name| name.strip_prefix("refs/heads/").map(str::to_owned));
    state.onto = onto.filter(|id| oid(id));
    state.commit = commit;
    state.step = vec![
        backend.to_owned(),
        original.unwrap_or_default(),
        position.unwrap_or_default(),
        last.unwrap_or_default().to_owned(),
        todo.unwrap_or_default(),
    ];
    Ok(state)
}

pub(crate) fn recovery_state(
    root: &Path,
    git: &WorktreeGitDir,
    head: Option<&str>,
) -> Result<Option<RecoveryState>> {
    let dir = &git.path;
    ensure!(
        dir.metadata()
            .context("无法读取工作树的 Git 目录")?
            .is_dir(),
        "工作树的 Git 目录无效"
    );

    if has_marker(dir, "rebase-apply")? {
        return if has_marker(dir, "rebase-apply/applying")? {
            Ok(Some(RecoveryState::new(RecoveryKind::ApplyMail, head)))
        } else {
            rebase(root, git, head, true).map(Some)
        };
    }
    if has_marker(dir, "rebase-merge")? {
        return rebase(root, git, head, false).map(Some);
    }
    if let Some(targets) = read_marker(dir, "MERGE_HEAD")? {
        let mut state = RecoveryState::new(RecoveryKind::Merge, head);
        state.started_at = marker_time(dir, "MERGE_HEAD")?;
        state.step = targets.lines().map(str::to_owned).collect();
        state.terminal_only = state.step.is_empty() || state.step.iter().any(|id| !oid(id));
        state.commit = state.step.first().filter(|id| oid(id)).cloned();
        return Ok(Some(state));
    }

    let todo = read_marker(dir, "sequencer/todo")?;
    let next = todo.as_deref().and_then(|todo| {
        todo.lines()
            .map(str::trim)
            .find(|line| !line.is_empty() && !line.starts_with('#'))
    });
    let instruction = next.and_then(|line| line.split_ascii_whitespace().next());
    if let Some(commit) = read_root_ref(root, git, "CHERRY_PICK_HEAD")? {
        let mut state = RecoveryState::new(RecoveryKind::CherryPick, head);
        state.started_at = marker_time(
            dir,
            if git.reftable {
                "reftable/tables.list"
            } else {
                "CHERRY_PICK_HEAD"
            },
        )?;
        state.single_pick = todo.is_none();
        state.commit = Some(commit).filter(|id| oid(id));
        state.terminal_only =
            state.commit.is_none() || (todo.is_some() && instruction != Some("pick"));
        state.skippable = !state.terminal_only;
        state.step.extend(todo);
        return Ok(Some(state));
    }
    if instruction == Some("revert") || read_root_ref(root, git, "REVERT_HEAD")?.is_some() {
        return Ok(Some(RecoveryState::new(RecoveryKind::Revert, head)));
    }
    if instruction == Some("pick") {
        let mut state = RecoveryState::new(RecoveryKind::CherryPick, head);
        state.started_at = marker_time(dir, "sequencer/todo")?;
        // The previous pick can have been committed manually. Git still has a
        // sequence to continue, but that completed commit cannot be skipped.
        state.step.extend(todo);
        return Ok(Some(state));
    }
    Ok(todo.map(|_| RecoveryState::new(RecoveryKind::UnknownSequence, head)))
}

pub(super) fn recover(
    root: &Path,
    expected: &RecoveryState,
    action: RecoveryAction,
    editor: Option<&mut (dyn FnMut(RebaseMessageRequest) + Send)>,
) -> Result<String> {
    let dir = worktree_git_dir(root)?;
    let snapshot = status(root)?;
    let current = recovery_state(root, &dir, snapshot.head.as_deref())?;
    ensure!(
        current.as_ref() == Some(expected),
        "Git 操作已变化，请确认刷新后的状态"
    );
    ensure!(
        expected.allows(action, snapshot.conflict_count()),
        "当前 Git 状态不允许{}，请检查冲突或在终端处理",
        action.title(expected.kind)
    );
    let subcommand = expected.kind.command().context("请在终端处理此 Git 操作")?;
    let mut command = command(Some(root), false);
    command.args([subcommand, action.flag()]);
    let edits_message = expected.kind == RecoveryKind::Rebase && action != RecoveryAction::Abort;
    let git_error = || format!("无法执行 Git（{}）", root.display());
    let out = if let Some(editor) = editor.filter(|_| edits_message) {
        let (out, abort) = editor::run(&mut command, &dir.path, editor).with_context(git_error)?;
        if abort {
            let snapshot = status(root)?;
            let current = recovery_state(root, &dir, snapshot.head.as_deref())?;
            ensure!(
                current.as_ref().is_some_and(|current| {
                    current.kind == RecoveryKind::Rebase
                        && current.started_at.is_some()
                        && current.started_at == expected.started_at
                        && current.onto == expected.onto
                        && current.step.get(1) == expected.step.get(1)
                }),
                "变基现场已变化，请刷新后重新确认中止操作"
            );
            checked(output(root, &["rebase", "--abort"], false)?)?;
            return Ok("已中止变基".into());
        }
        out
    } else {
        ensure!(
            !edits_message || !expected.needs_message_editor,
            "当前变基需要提交说明编辑器"
        );
        command
            .env("GIT_EDITOR", "true")
            .output()
            .with_context(git_error)?
    };
    let stderr = trim(&out.stderr);
    let stdout = trim(&checked(out)?);
    Ok(if stdout.is_empty() { stderr } else { stdout })
}

#[cfg(test)]
mod tests;
