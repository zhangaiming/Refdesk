use super::*;
use crate::git::{Operation, fixture_command};

fn git(root: &Path, args: &[&str]) -> Result<String> {
    let output = fixture_command(root).args(args).output()?;
    Ok(trim(&checked(output)?))
}

fn init(root: &Path) -> Result<()> {
    git(root, &["init", "-q", "-b", "main"])?;
    git(root, &["config", "user.name", "Refdesk Test"])?;
    git(root, &["config", "user.email", "refdesk@example.test"])?;
    Ok(())
}

fn commit(root: &Path, name: &str, files: &[(&str, &str)]) -> Result<String> {
    for (path, value) in files {
        std::fs::write(root.join(path), format!("{value}\n"))?;
    }
    git(root, &["add", "-A"])?;
    git(root, &["commit", "-qm", name])?;
    git(root, &["rev-parse", "HEAD"])
}

fn snapshot(root: &Path) -> Result<(Option<RecoveryState>, usize)> {
    let files = status(root)?;
    let state = recovery_state(root, &worktree_git_dir(root)?, files.head.as_deref())?;
    Ok((state, files.conflict_count()))
}

fn execute(root: &Path, state: &RecoveryState, action: RecoveryAction) -> Result<String> {
    Operation::Recover {
        expected: Box::new(state.clone()),
        action,
    }
    .execute(root)
}

fn conflicting_branches(root: &Path) -> Result<String> {
    init(root)?;
    commit(
        root,
        "base",
        &[("first.txt", "base first"), ("last.txt", "base last")],
    )?;
    git(root, &["switch", "-qc", "topic"])?;
    let topic = commit(root, "first topic", &[("first.txt", "topic first")])?;
    git(root, &["switch", "-q", "main"])?;
    commit(
        root,
        "main first and last",
        &[("first.txt", "main first"), ("last.txt", "main last")],
    )?;
    Ok(topic)
}

#[test]
fn recovers_merge_even_without_conflicts_and_preserves_parent_order() -> Result<()> {
    let fixture = tempfile::tempdir()?;
    let root = fixture.path();
    init(root)?;
    commit(root, "base", &[("one.txt", "base")])?;
    git(root, &["switch", "-qc", "topic"])?;
    let topic = commit(root, "topic", &[("two.txt", "topic")])?;
    git(root, &["switch", "-q", "main"])?;
    let main = commit(root, "main", &[("one.txt", "main")])?;
    for action in [RecoveryAction::Abort, RecoveryAction::Continue] {
        git(root, &["merge", "--no-ff", "--no-commit", "topic"])?;
        let (state, conflicts) = snapshot(root)?;
        let state = state.context("paused merge")?;
        assert_eq!(state.kind, RecoveryKind::Merge);
        assert_eq!(state.commit.as_deref(), Some(topic.as_str()));
        assert_eq!(conflicts, 0);
        assert!(!state.allows(RecoveryAction::Skip, conflicts));
        execute(root, &state, action)?;
        assert!(snapshot(root)?.0.is_none());
        if action == RecoveryAction::Continue {
            assert_eq!(
                git(root, &["show", "-s", "--format=%P", "HEAD"])?,
                format!("{main} {topic}")
            );
        } else {
            assert_eq!(git(root, &["rev-parse", "HEAD"])?, main);
        }
    }
    Ok(())
}

#[test]
fn single_cherry_pick_requires_staging_and_supports_all_three_actions() -> Result<()> {
    let fixture = tempfile::tempdir()?;
    let root = fixture.path();
    let topic = conflicting_branches(root)?;
    let before = git(root, &["rev-parse", "HEAD"])?;
    for action in [
        RecoveryAction::Abort,
        RecoveryAction::Skip,
        RecoveryAction::Continue,
    ] {
        assert!(git(root, &["cherry-pick", &topic]).is_err());
        let (state, conflicts) = snapshot(root)?;
        let state = state.context("paused cherry-pick")?;
        assert!(state.single_pick);
        assert_eq!(state.kind, RecoveryKind::CherryPick);
        assert_eq!(conflicts, 1);
        assert!(execute(root, &state, RecoveryAction::Continue).is_err());
        assert!(
            Operation::Commit("bypass recovery".into())
                .execute(root)
                .is_err()
        );
        if action == RecoveryAction::Continue {
            std::fs::write(root.join("first.txt"), "resolved\n")?;
            Operation::Stage(vec!["first.txt".into()]).execute(root)?;
            assert_eq!(snapshot(root)?, (Some(state.clone()), 0));
        }
        execute(root, &state, action)?;
        assert!(snapshot(root)?.0.is_none());
        assert_eq!(
            git(root, &["rev-parse", "HEAD"])? == before,
            action != RecoveryAction::Continue
        );
    }
    Ok(())
}

#[test]
fn skip_can_advance_to_another_conflict_and_invalidates_the_previous_step() -> Result<()> {
    let fixture = tempfile::tempdir()?;
    let root = fixture.path();
    let first = conflicting_branches(root)?;
    let before = git(root, &["rev-parse", "HEAD"])?;
    git(root, &["switch", "-q", "topic"])?;
    let middle = commit(root, "middle topic", &[("middle.txt", "topic middle")])?;
    let last = commit(root, "last topic", &[("last.txt", "topic last")])?;
    git(root, &["switch", "-q", "main"])?;
    let series = ["cherry-pick", &first, &middle, &last];
    assert!(git(root, &series).is_err());
    let old = snapshot(root)?.0.context("first pick")?;
    assert!(!old.single_pick);
    assert!(execute(root, &old, RecoveryAction::Skip).is_err());
    let (current, conflicts) = snapshot(root)?;
    let current = current.context("last pick")?;
    assert_eq!(conflicts, 1);
    assert_eq!(current.commit.as_deref(), Some(last.as_str()));
    assert_ne!(git(root, &["rev-parse", "HEAD"])?, before);
    assert_eq!(git(root, &["log", "-1", "--format=%s"])?, "middle topic");
    let stale = execute(root, &old, RecoveryAction::Abort).unwrap_err();
    assert!(stale.to_string().contains("Git 操作已变化"));
    assert_eq!(snapshot(root)?.0.as_ref(), Some(&current));
    execute(root, &current, RecoveryAction::Abort)?;

    assert!(git(root, &series).is_err());
    commit(root, "resolved manually", &[("first.txt", "resolved")])?;
    let manual = snapshot(root)?.0.context("sequence after manual commit")?;
    assert_eq!(manual.kind, RecoveryKind::CherryPick);
    assert!(manual.commit.is_none() && !manual.allows(RecoveryAction::Skip, 0));
    assert!(execute(root, &manual, RecoveryAction::Continue).is_err());
    let current = snapshot(root)?.0.context("next paused pick")?;
    assert_eq!(current.commit.as_deref(), Some(last.as_str()));
    execute(root, &current, RecoveryAction::Abort)?;
    Ok(())
}

#[test]
fn recovers_both_rebase_backends_in_linked_worktree() -> Result<()> {
    let fixture = tempfile::tempdir()?;
    let root = fixture.path().join("项目 root");
    std::fs::create_dir(&root)?;
    let topic = conflicting_branches(&root)?;
    let linked = fixture.path().join("linked 工作树");
    git(
        &root,
        &["worktree", "add", linked.to_str().unwrap(), "topic"],
    )?;
    let dir = worktree_git_dir(&linked)?.path;
    assert_ne!(dir, worktree_git_dir(&root)?.path);
    for (backend, action) in [
        ("--merge", RecoveryAction::Abort),
        ("--apply", RecoveryAction::Skip),
        ("--merge", RecoveryAction::Continue),
        ("--apply", RecoveryAction::Continue),
    ] {
        if git(&linked, &["rev-parse", "HEAD"])? != topic {
            git(&linked, &["reset", "--hard", &topic])?;
        }
        assert!(git(&linked, &["rebase", backend, "main"]).is_err());
        let (state, conflicts) = snapshot(&linked)?;
        let state = state.context("paused rebase")?;
        assert_eq!(state.kind, RecoveryKind::Rebase);
        assert_eq!(state.progress, Some((1, 1)));
        assert_eq!(state.branch.as_deref(), Some("topic"));
        assert_eq!(conflicts, 1);
        assert!(snapshot(&root)?.0.is_none());

        if backend == "--merge" {
            std::fs::write(dir.join("MERGE_HEAD"), &topic)?;
            assert_eq!(snapshot(&linked)?.0.unwrap().kind, RecoveryKind::Rebase);
            std::fs::remove_file(dir.join("MERGE_HEAD"))?;
        }
        if action == RecoveryAction::Continue {
            std::fs::write(linked.join("first.txt"), "resolved\n")?;
            Operation::Stage(vec!["first.txt".into()]).execute(&linked)?;
            assert_eq!(snapshot(&linked)?, (Some(state.clone()), 0));
        }
        execute(&linked, &state, action)?;
        std::fs::write(dir.join("REBASE_HEAD"), &topic)?;
        assert!(snapshot(&linked)?.0.is_none());
    }
    Ok(())
}

#[test]
fn submodule_recovery_is_separate_and_revert_uses_terminal_path() -> Result<()> {
    let fixture = tempfile::tempdir()?;
    let child = fixture.path().join("child");
    let parent = fixture.path().join("parent");
    std::fs::create_dir(&child)?;
    std::fs::create_dir(&parent)?;
    let topic = conflicting_branches(&child)?;
    init(&parent)?;
    git(
        &parent,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            child.to_str().unwrap(),
            "子 模块",
        ],
    )?;
    git(&parent, &["commit", "-qm", "submodule"])?;
    let module = parent.join("子 模块");
    git(&module, &["config", "user.name", "Refdesk Test"])?;
    git(&module, &["config", "user.email", "refdesk@example.test"])?;
    assert!(git(&module, &["cherry-pick", &topic]).is_err());
    let state = snapshot(&module)?.0.context("submodule operation")?;
    assert_eq!(state.kind, RecoveryKind::CherryPick);
    assert!(snapshot(&parent)?.0.is_none());
    execute(&module, &state, RecoveryAction::Abort)?;
    let base = git(&module, &["rev-parse", "HEAD~1"])?;
    assert!(git(&module, &["revert", "--no-edit", &topic]).is_err());
    assert_eq!(git(&module, &["rev-parse", "HEAD~1"])?, base);
    let state = snapshot(&module)?.0.context("submodule revert")?;
    assert_eq!(state.kind, RecoveryKind::Revert);
    assert!(state.terminal_only);
    Ok(())
}

#[test]
fn reads_single_cherry_pick_from_reftable() -> Result<()> {
    let fixture = tempfile::tempdir()?;
    let root = fixture.path();
    if let Err(error) = git(root, &["init", "-q", "--ref-format=reftable", "-b", "main"]) {
        let message = error.to_string();
        if message.contains("unknown option") && message.contains("ref-format") {
            return Ok(());
        }
        return Err(error);
    }
    git(root, &["config", "user.name", "Refdesk Test"])?;
    git(root, &["config", "user.email", "refdesk@example.test"])?;
    commit(root, "base", &[("initial.txt", "base")])?;
    git(root, &["switch", "-qc", "topic"])?;
    let target = commit(root, "topic", &[("initial.txt", "topic")])?;
    git(root, &["switch", "-q", "main"])?;
    commit(root, "main", &[("initial.txt", "main")])?;
    assert!(git(root, &["cherry-pick", &target]).is_err());
    let dir = worktree_git_dir(root)?;
    assert!(dir.reftable && !dir.path.join("CHERRY_PICK_HEAD").exists());
    let (state, conflicts) = snapshot(root)?;
    let state = state.context("paused cherry-pick in reftable")?;
    assert_eq!(conflicts, 1);
    assert!(state.single_pick);
    assert_eq!(state.commit.as_deref(), Some(target.as_str()));
    std::fs::write(root.join("initial.txt"), "resolved\n")?;
    git(root, &["add", "initial.txt"])?;
    execute(root, &state, RecoveryAction::Continue)?;
    assert!(snapshot(root)?.0.is_none());
    Ok(())
}

#[test]
fn conflicted_mail_patch_preserves_terminal_control() -> Result<()> {
    let fixture = tempfile::tempdir()?;
    let root = fixture.path().join("repo");
    std::fs::create_dir(&root)?;
    let topic = conflicting_branches(&root)?;
    let patch = fixture.path().join("topic.patch");
    std::fs::write(
        &patch,
        git(&root, &["format-patch", "--stdout", "-1", &topic])? + "\n",
    )?;
    assert!(git(&root, &["am", "--3way", patch.to_str().unwrap()]).is_err());
    let (state, conflicts) = snapshot(&root)?;
    let state = state.context("paused git am")?;
    assert_eq!(state.kind, RecoveryKind::ApplyMail);
    assert!(!state.allows(RecoveryAction::Abort, conflicts));
    assert!(execute(&root, &state, RecoveryAction::Abort).is_err());
    assert_eq!(snapshot(&root)?.0, Some(state));
    git(&root, &["am", "--abort"])?;
    Ok(())
}

#[test]
fn rebase_does_not_bypass_a_later_commit_message_edit() -> Result<()> {
    let fixture = tempfile::tempdir()?;
    let root = fixture.path();
    conflicting_branches(root)?;
    git(root, &["switch", "-q", "topic"])?;
    commit(root, "later commit", &[("later.txt", "later")])?;
    assert!(git(root, &["rebase", "--merge", "main"]).is_err());
    let original = snapshot(root)?.0.context("ordinary rebase")?;
    assert!(!original.terminal_only);

    let todo = worktree_git_dir(root)?
        .path
        .join("rebase-merge/git-rebase-todo");
    let plan = std::fs::read_to_string(&todo)?;
    let edited = plan.replacen("pick ", "reword ", 1);
    assert_ne!(plan, edited);
    std::fs::write(todo, edited)?;
    let (updated, conflicts) = snapshot(root)?;
    let updated = updated.context("rebase with pending reword")?;
    assert!(!updated.terminal_only);
    assert!(updated.allows(RecoveryAction::Skip, conflicts));
    assert!(
        execute(root, &updated, RecoveryAction::Skip)
            .unwrap_err()
            .to_string()
            .contains("需要提交说明编辑器")
    );
    assert!(execute(root, &original, RecoveryAction::Skip).is_err());
    git(root, &["rebase", "--abort"])?;
    Ok(())
}
