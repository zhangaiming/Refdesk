use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output, Stdio};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum HistoryScope {
    #[default]
    Head,
    All,
    Selected(BTreeSet<String>),
}

impl HistoryScope {
    pub fn checks(&self) -> BTreeSet<String> {
        match self {
            Self::Head => BTreeSet::from(["HEAD".into()]),
            Self::All => BTreeSet::new(),
            Self::Selected(names) => names.clone(),
        }
    }
}

pub const PAGE: usize = 300;
const MAX_SNAPSHOT: u64 = 64 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct Commit {
    pub id: String,
    pub parents: Vec<String>,
    pub short: String,
    pub author: String,
    pub date: String,
    pub subject: String,
}

#[derive(Clone, Debug)]
pub struct Reference {
    pub name: String,
    pub target: String,
    pub upstream: Option<Upstream>,
    pub symbolic: bool,
}

impl Reference {
    pub fn label(&self) -> &str {
        short_ref(&self.name)
    }

    pub fn description(&self) -> String {
        self.upstream.as_ref().map_or_else(
            || self.name.clone(),
            |upstream| {
                format!(
                    "{}\n跟踪 {}  {}",
                    self.name,
                    upstream.label(),
                    upstream.state.caption()
                )
            },
        )
    }
}

#[derive(Clone, Debug)]
pub struct Upstream {
    pub name: String,
    pub state: TrackingState,
}

impl Upstream {
    pub fn label(&self) -> &str {
        short_ref(&self.name)
    }

    pub fn same_branch_remote(&self, local: &Reference) -> Option<&str> {
        self.name
            .strip_prefix("refs/remotes/")?
            .strip_suffix(local.label())?
            .strip_suffix('/')
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrackingState {
    Counts { ahead: u64, behind: u64 },
    Gone,
    Unknown,
}

impl TrackingState {
    fn parse(value: &str) -> Self {
        if value == "gone" {
            return Self::Gone;
        }
        let mut ahead = 0;
        let mut behind = 0;
        if !value.is_empty() {
            for entry in value.split(", ") {
                let Some((direction, count)) = entry.split_once(' ') else {
                    return Self::Unknown;
                };
                let Ok(count) = count.parse() else {
                    return Self::Unknown;
                };
                match direction {
                    "ahead" => ahead = count,
                    "behind" => behind = count,
                    _ => return Self::Unknown,
                }
            }
        }
        Self::Counts { ahead, behind }
    }

    pub fn caption(self) -> String {
        match self {
            Self::Counts {
                ahead: 0,
                behind: 0,
            } => "=".into(),
            Self::Counts { ahead, behind: 0 } => format!("+{ahead}"),
            Self::Counts { ahead: 0, behind } => format!("−{behind}"),
            Self::Counts { ahead, behind } => format!("+{ahead} −{behind}"),
            Self::Gone => "失效".into(),
            Self::Unknown => "?".into(),
        }
    }
}

fn short_ref(name: &str) -> &str {
    name.strip_prefix("refs/heads/")
        .or_else(|| name.strip_prefix("refs/remotes/"))
        .or_else(|| name.strip_prefix("refs/tags/"))
        .unwrap_or(name)
}

#[derive(Clone, Debug, Default)]
pub struct History {
    pub branch: String,
    pub head: Option<String>,
    pub commits: Vec<Commit>,
    pub refs: Vec<Reference>,
    pub has_more: bool,
    pub elapsed_ms: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Area {
    Staged,
    Unstaged,
    Commit,
}

#[derive(Clone, Debug)]
pub struct Change {
    pub path: String,
    pub old_path: Option<String>,
    pub code: char,
    pub area: Area,
}

#[derive(Clone, Debug)]
pub enum Version {
    Empty,
    Index,
    Worktree,
    Commit(String),
}

impl Version {
    pub fn title(&self) -> &str {
        match self {
            Self::Empty => "∅",
            Self::Index => "暂存区",
            Self::Worktree => "工作树",
            Self::Commit(id) if id == "HEAD" => "HEAD",
            Self::Commit(id) => id.get(..8).unwrap_or(id),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Compare {
    pub left: Version,
    pub right: Version,
    pub base: Version,
    pub left_path: String,
    pub right_path: String,
}

#[derive(Clone, Debug)]
pub struct Details {
    pub title: String,
    pub message: String,
    pub author: String,
    pub date: String,
    pub files: Vec<Change>,
    pub from: Version,
    pub to: Version,
    pub base: Version,
}

fn command(root: Option<&Path>, read_only: bool) -> Command {
    let mut cmd = Command::new("git");
    if let Some(root) = root {
        cmd.arg("-C").arg(root);
    }
    cmd.args([
        "--no-pager",
        "--literal-pathspecs",
        "-c",
        "core.quotePath=false",
    ])
    .env("GIT_TERMINAL_PROMPT", "0")
    .stdin(Stdio::null());
    if read_only {
        cmd.env("GIT_OPTIONAL_LOCKS", "0");
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    cmd
}

fn output(root: &Path, args: &[&str], read_only: bool) -> Result<Output> {
    command(Some(root), read_only)
        .args(args)
        .output()
        .with_context(|| format!("无法执行 Git（{}）", root.display()))
}

fn checked(result: Output) -> Result<Vec<u8>> {
    if !result.status.success() {
        let message = String::from_utf8_lossy(&result.stderr);
        let stdout = String::from_utf8_lossy(&result.stdout);
        bail!(
            "{}",
            if message.trim().is_empty() {
                stdout.trim()
            } else {
                message.trim()
            }
        );
    }
    Ok(result.stdout)
}

fn run(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    checked(output(root, args, true)?)
}

fn string(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}
fn trim(bytes: &[u8]) -> String {
    string(bytes).trim_end_matches(['\r', '\n']).to_owned()
}

pub fn root(path: &Path) -> Result<PathBuf> {
    let root = trim(&run(path, &["rev-parse", "--show-toplevel"])?);
    ensure!(!root.is_empty(), "此路径没有 Git 工作树");
    Ok(PathBuf::from(root))
}

pub fn worktrees(root: &Path) -> Result<Vec<PathBuf>> {
    let bytes = run(root, &["worktree", "list", "--porcelain", "-z"])?;
    let mut result = Vec::new();
    for field in bytes.split(|ch| *ch == 0) {
        if let Some(path) = field.strip_prefix(b"worktree ") {
            let path = PathBuf::from(string(path));
            if path.is_dir() {
                result.push(path);
            }
        }
    }
    Ok(result)
}

pub fn submodules(root: &Path) -> Result<Vec<PathBuf>> {
    if !root.join(".gitmodules").is_file() {
        return Ok(Vec::new());
    }
    let result = output(
        root,
        &[
            "config",
            "--null",
            "--file",
            ".gitmodules",
            "--get-regexp",
            "^submodule\\..*\\.path$",
        ],
        true,
    )?;
    if result.status.code() == Some(1) {
        return Ok(Vec::new());
    }
    let bytes = checked(result)?;
    Ok(bytes
        .split(|ch| *ch == 0)
        .filter_map(|line| {
            let position = line.iter().position(|ch| *ch == b'\n')?;
            let name = string(&line[position + 1..]);
            let path = safe_relative(&name).ok()?;
            let path = root.join(path);
            path.join(".git").exists().then_some(path)
        })
        .collect())
}

fn oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|ch| ch.is_ascii_hexdigit())
}

pub fn history(root: &Path, scope: &HistoryScope, limit: usize) -> Result<History> {
    let timer = std::time::Instant::now();
    let head_result = output(root, &["rev-parse", "--verify", "--quiet", "HEAD"], true)?;
    let head = head_result
        .status
        .success()
        .then(|| trim(&head_result.stdout));
    let branch_result = output(root, &["symbolic-ref", "--quiet", "--short", "HEAD"], true)?;
    let branch = if branch_result.status.success() {
        trim(&branch_result.stdout)
    } else {
        "Detached HEAD".to_owned()
    };
    let ref_bytes = checked(command(Some(root), true)
        .env("LC_ALL", "C")
        .args([
            "for-each-ref",
            "--sort=refname",
            "--format=%(refname)%00%(objectname)%00%(*objectname)%00%(upstream)%00%(upstream:track,nobracket)%00%(symref)",
            "refs/heads",
            "refs/remotes",
            "refs/tags",
        ])
        .output()
        .with_context(|| format!("无法读取 Git 引用（{}）", root.display()))?)?;
    let refs = ref_bytes
        .split(|ch| *ch == b'\n')
        .filter_map(parse_reference)
        .collect::<Vec<_>>();
    let count = format!("--max-count={}", limit + 1);
    let mut args = vec![
        "log",
        "--date-order",
        "--date=short",
        "-z",
        &count,
        "--format=%H%x00%P%x00%h%x00%an%x00%ad%x00%s",
    ];
    let mut commits = if let HistoryScope::Selected(filter) = scope {
        let mut targets: BTreeSet<&str> = refs
            .iter()
            .filter(|reference| filter.contains(&reference.name))
            .map(|reference| reference.target.as_str())
            .collect();
        if filter.contains("HEAD")
            && let Some(head) = &head
        {
            targets.insert(head);
        }
        if targets.is_empty() {
            Vec::new()
        } else {
            args.push("--stdin");
            let mut child = command(Some(root), true)
                .args(&args)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .context("无法读取所选分支的 Git 历史")?;
            let mut input = child.stdin.take().expect("piped stdin");
            let written = input.write_all(
                targets
                    .into_iter()
                    .collect::<Vec<_>>()
                    .join("\n")
                    .as_bytes(),
            );
            drop(input);
            let bytes = checked(child.wait_with_output()?)?;
            written?;
            parse_commits(&bytes)
        }
    } else if *scope == HistoryScope::Head {
        if head.is_some() {
            args.extend(["HEAD", "--"]);
            parse_commits(&run(root, &args)?)
        } else {
            Vec::new()
        }
    } else if head.is_none() && refs.is_empty() {
        Vec::new()
    } else {
        args.extend(["--branches", "--remotes", "--tags"]);
        if head.is_some() {
            args.push("HEAD");
        }
        args.push("--");
        parse_commits(&run(root, &args)?)
    };
    let has_more = commits.len() > limit;
    commits.truncate(limit);
    Ok(History {
        branch,
        head,
        commits,
        refs,
        has_more,
        elapsed_ms: timer.elapsed().as_millis(),
    })
}

fn parse_reference(line: &[u8]) -> Option<Reference> {
    let mut fields = line.split(|ch| *ch == 0);
    let name = string(fields.next()?);
    let direct = fields.next()?;
    let peeled = fields.next()?;
    let target = string(if peeled.is_empty() { direct } else { peeled });
    if name.is_empty() || !oid(&target) {
        return None;
    }

    let upstream = string(fields.next()?);
    let tracking = string(fields.next()?);
    let symbolic = !fields.next()?.is_empty();
    let upstream = (!upstream.is_empty()).then(|| Upstream {
        name: upstream,
        state: TrackingState::parse(&tracking),
    });
    Some(Reference {
        name,
        target,
        upstream,
        symbolic,
    })
}

fn parse_commits(bytes: &[u8]) -> Vec<Commit> {
    let fields: Vec<_> = bytes.split(|ch| *ch == 0).collect();
    fields
        .as_chunks::<6>()
        .0
        .iter()
        .filter_map(|row| {
            let id = string(row[0]);
            oid(&id).then(|| Commit {
                id,
                parents: string(row[1])
                    .split_whitespace()
                    .map(str::to_owned)
                    .collect(),
                short: string(row[2]),
                author: string(row[3]),
                date: string(row[4]),
                subject: string(row[5]),
            })
        })
        .collect()
}

pub fn status(root: &Path) -> Result<Vec<Change>> {
    let bytes = run(
        root,
        &[
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--ignore-submodules=dirty",
        ],
    )?;
    let mut fields = bytes.split(|ch| *ch == 0);
    let mut files = Vec::new();
    while let Some(field) = fields.next() {
        if field.len() < 4 {
            continue;
        }
        let path = string(&field[3..]);
        let x = field[0] as char;
        let y = field[1] as char;
        let old = if matches!(x, 'R' | 'C') || matches!(y, 'R' | 'C') {
            fields.next().map(string)
        } else {
            None
        };
        if matches!(
            (x, y),
            ('D', 'D')
                | ('A', 'U')
                | ('U', 'D')
                | ('U', 'A')
                | ('D', 'U')
                | ('A', 'A')
                | ('U', 'U')
        ) {
            files.push(Change {
                path,
                old_path: None,
                code: 'U',
                area: Area::Unstaged,
            });
            continue;
        }
        if !matches!(x, ' ' | '?' | '!') {
            files.push(Change {
                path: path.clone(),
                old_path: old.clone().filter(|_| matches!(x, 'R' | 'C')),
                code: x,
                area: Area::Staged,
            });
        }
        if !matches!(y, ' ' | '!') {
            files.push(Change {
                path,
                old_path: old.filter(|_| matches!(y, 'R' | 'C')),
                code: y,
                area: Area::Unstaged,
            });
        }
    }
    Ok(files)
}

fn named_changes(bytes: &[u8]) -> Vec<Change> {
    let mut fields = bytes.split(|ch| *ch == 0);
    let mut files = Vec::new();
    while let Some(field) = fields.next() {
        let Some(&code) = field.first() else { break };
        let Some(first) = fields.next() else { break };
        let code = code as char;
        let first = string(first);
        let (old_path, path) = if matches!(code, 'R' | 'C') {
            let Some(next) = fields.next() else { break };
            (Some(first), string(next))
        } else {
            (None, first)
        };
        files.push(Change {
            path,
            old_path,
            code,
            area: Area::Commit,
        });
    }
    files
}

pub fn details(root: &Path, item: &Commit, previous: Option<&Commit>) -> Result<Details> {
    ensure!(oid(&item.id), "无效的提交标识");
    let from_id = previous.map(|commit| &commit.id).or(item.parents.first());
    let to = Version::Commit(item.id.clone());
    let (from, files) = if let Some(from_id) = from_id {
        ensure!(oid(from_id), "无效的父提交标识");
        (
            Version::Commit(from_id.clone()),
            run(
                root,
                &[
                    "diff",
                    "--name-status",
                    "--find-renames",
                    "-z",
                    from_id,
                    &item.id,
                    "--",
                ],
            )?,
        )
    } else {
        (
            Version::Empty,
            run(
                root,
                &[
                    "diff-tree",
                    "--root",
                    "--no-commit-id",
                    "--name-status",
                    "--find-renames",
                    "-r",
                    "-z",
                    &item.id,
                    "--",
                ],
            )?,
        )
    };
    let base = if let Some(previous) = previous {
        let result = output(root, &["merge-base", &previous.id, &item.id], true)?;
        if result.status.success() {
            Version::Commit(trim(&result.stdout))
        } else {
            Version::Empty
        }
    } else {
        from.clone()
    };
    let message = trim(&run(root, &["show", "-s", "--format=%B", &item.id, "--"])?);
    let title = if let Some(previous) = previous {
        format!("{} → {}", previous.short, item.short)
    } else {
        item.subject.clone()
    };
    Ok(Details {
        title,
        message,
        author: item.author.clone(),
        date: item.date.clone(),
        files: named_changes(&files),
        from,
        to,
        base,
    })
}

pub fn compare(file: &Change, details: Option<&Details>, has_head: bool) -> Compare {
    let (mut left, mut right, base) = match file.area {
        Area::Commit => {
            let details = details.expect("commit changes require commit details");
            (
                details.from.clone(),
                details.to.clone(),
                details.base.clone(),
            )
        }
        Area::Staged => {
            let from = if has_head {
                Version::Commit("HEAD".into())
            } else {
                Version::Empty
            };
            (from.clone(), Version::Index, from)
        }
        Area::Unstaged => (Version::Index, Version::Worktree, Version::Index),
    };
    if matches!(file.code, 'A' | '?' | 'U') {
        left = Version::Empty;
    }
    if file.code == 'D' {
        right = Version::Empty;
    }
    Compare {
        left,
        right,
        base,
        left_path: file.old_path.clone().unwrap_or_else(|| file.path.clone()),
        right_path: file.path.clone(),
    }
}

pub fn safe_relative(path: &str) -> Result<&Path> {
    let path = Path::new(path);
    ensure!(
        !path.as_os_str().is_empty()
            && path
                .components()
                .all(|component| matches!(component, Component::Normal(_))),
        "路径不在工作树中"
    );
    Ok(path)
}

pub fn snapshot(root: &Path, version: &Version, path: &str, missing_ok: bool) -> Result<Vec<u8>> {
    let relative = safe_relative(path)?;
    match version {
        Version::Empty => Ok(Vec::new()),
        Version::Worktree => {
            let file = root.join(relative);
            let metadata = match std::fs::symlink_metadata(&file) {
                Err(error) if missing_ok && error.kind() == std::io::ErrorKind::NotFound => {
                    return Ok(Vec::new());
                }
                result => result?,
            };
            if metadata.file_type().is_symlink() {
                return Ok(std::fs::read_link(&file)?
                    .to_string_lossy()
                    .as_bytes()
                    .to_vec());
            }
            ensure!(metadata.is_file(), "此条目不是普通文件");
            ensure!(
                file.canonicalize()?.starts_with(root.canonicalize()?),
                "文件位于工作树之外"
            );
            ensure!(metadata.len() <= MAX_SNAPSHOT, "文件超过 64 MiB");
            let mut bytes = Vec::new();
            std::fs::File::open(file)?
                .take(MAX_SNAPSHOT + 1)
                .read_to_end(&mut bytes)?;
            ensure!(bytes.len() as u64 <= MAX_SNAPSHOT, "文件超过 64 MiB");
            Ok(bytes)
        }
        Version::Index | Version::Commit(_) => {
            let spec = match version {
                Version::Commit(id) => {
                    ensure!(id == "HEAD" || oid(id), "无效的提交标识");
                    format!("{id}:{path}")
                }
                _ => format!(":{path}"),
            };
            let size = output(root, &["cat-file", "-s", &spec], true)?;
            if missing_ok && !size.status.success() {
                return Ok(Vec::new());
            }
            let size: u64 = trim(&checked(size)?).parse().context("无法读取文件大小")?;
            ensure!(size <= MAX_SNAPSHOT, "文件超过 64 MiB");
            run(root, &["cat-file", "blob", &spec]).context("无法读取文件版本")
        }
    }
}

#[derive(Clone, Debug)]
pub enum Operation {
    Stage(String),
    Unstage(String),
    Commit(String),
    Switch(String),
    Branch(String),
    Fetch,
    Pull,
    Push,
}

impl Operation {
    pub fn execute(&self, root: &Path) -> Result<String> {
        let args = match self {
            Self::Stage(path) => {
                safe_relative(path)?;
                vec!["add", "--", path]
            }
            Self::Unstage(path) => {
                safe_relative(path)?;
                if output(root, &["rev-parse", "--verify", "--quiet", "HEAD"], true)?
                    .status
                    .success()
                {
                    vec!["restore", "--staged", "--", path]
                } else {
                    vec!["rm", "--cached", "--ignore-unmatch", "--", path]
                }
            }
            Self::Commit(message) => {
                ensure!(!message.trim().is_empty(), "请输入提交说明");
                vec!["commit", "-m", message]
            }
            Self::Switch(branch) => {
                validate_branch(root, branch)?;
                vec!["switch", "--", branch]
            }
            Self::Branch(branch) => {
                validate_branch(root, branch)?;
                vec!["switch", "-c", branch]
            }
            Self::Fetch => vec!["fetch", "--all"],
            Self::Pull => vec!["pull", "--ff-only"],
            Self::Push => vec!["push"],
        };
        let out = output(root, &args, false)?;
        let stderr = trim(&out.stderr);
        let stdout = trim(&checked(out)?);
        Ok(if stdout.is_empty() { stderr } else { stdout })
    }
}

fn validate_branch(root: &Path, branch: &str) -> Result<()> {
    ensure!(
        !branch.starts_with('-') && !branch.starts_with('@'),
        "请输入有效的分支名称"
    );
    run(root, &["check-ref-format", "--branch", branch])?;
    Ok(())
}

pub fn git_dir() -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?).find(|path| {
        path.join(if cfg!(windows) {
            OsStr::new("git.exe")
        } else {
            OsStr::new("git")
        })
        .is_file()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_nul_rename_paths_without_splitting_spaces() {
        let files = named_changes(b"R100\0before name.xlsx\0after name.xlsx\0M\0other.txt\0");
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].old_path.as_deref(), Some("before name.xlsx"));
        assert_eq!(files[0].path, "after name.xlsx");
    }

    #[test]
    fn rejects_paths_outside_the_worktree() {
        assert!(safe_relative("../outside").is_err());
        assert!(safe_relative("Assets/config.xlsx").is_ok());
    }

    #[test]
    fn git_tracks_each_branch_against_its_own_upstream() -> Result<()> {
        let sandbox = tempfile::tempdir()?;
        let root = sandbox.path();
        let git = |args: &[&str]| checked(output(root, args, false)?);
        git(&["init", "--initial-branch=main"])?;
        git(&["config", "user.name", "Refdesk test"])?;
        git(&["config", "user.email", "test@example.invalid"])?;
        git(&["config", "commit.gpgSign", "false"])?;
        git(&["config", "core.hooksPath", "disabled-hooks"])?;
        git(&["commit", "--allow-empty", "-m", "base"])?;
        git(&["branch", "behind"])?;
        git(&["branch", "review"])?;
        git(&["branch", "gone"])?;
        git(&["commit", "--allow-empty", "-m", "second"])?;
        git(&["commit", "--allow-empty", "-m", "third"])?;
        let main = trim(&git(&["rev-parse", "main"])?);
        git(&["branch", "synced"])?;
        git(&["switch", "review"])?;
        git(&["commit", "--allow-empty", "-m", "side"])?;
        git(&["remote", "add", "origin", "local-test-remote"])?;
        git(&["update-ref", "refs/remotes/origin/main", &main])?;
        git(&["update-ref", "refs/remotes/origin/behind", &main])?;
        git(&["update-ref", "refs/remotes/origin/synced", &main])?;
        git(&["update-ref", "refs/remotes/origin/old", &main])?;
        git(&["update-ref", "refs/remotes/origin/baseline", "main~2"])?;
        for (local, upstream) in [
            ("main", "origin/baseline"),
            ("behind", "origin/behind"),
            ("review", "origin/main"),
            ("synced", "origin/synced"),
            ("gone", "origin/old"),
        ] {
            git(&["branch", "--set-upstream-to", upstream, local])?;
        }
        git(&["update-ref", "-d", "refs/remotes/origin/old"])?;
        let snapshot = history(root, &HistoryScope::Head, PAGE)?;
        for (name, upstream, state) in [
            (
                "main",
                "origin/baseline",
                TrackingState::Counts {
                    ahead: 2,
                    behind: 0,
                },
            ),
            (
                "behind",
                "origin/behind",
                TrackingState::Counts {
                    ahead: 0,
                    behind: 2,
                },
            ),
            (
                "review",
                "origin/main",
                TrackingState::Counts {
                    ahead: 1,
                    behind: 2,
                },
            ),
            (
                "synced",
                "origin/synced",
                TrackingState::Counts {
                    ahead: 0,
                    behind: 0,
                },
            ),
            ("gone", "origin/old", TrackingState::Gone),
        ] {
            let reference = snapshot
                .refs
                .iter()
                .find(|reference| reference.name == format!("refs/heads/{name}"))
                .unwrap();
            let tracking = reference.upstream.as_ref().unwrap();
            assert_eq!(tracking.label(), upstream);
            assert_eq!(tracking.state, state);
        }
        Ok(())
    }

    #[test]
    fn real_git_history_worktree_and_commit_roundtrip() -> Result<()> {
        let sandbox = tempfile::tempdir()?;
        let root = sandbox.path().join("main repo");
        let linked = sandbox.path().join("linked worktree");
        std::fs::create_dir(&root)?;
        let git = |args: &[&str]| checked(output(&root, args, false)?);
        git(&["init", "--initial-branch=main"])?;
        git(&["config", "user.name", "Refdesk test"])?;
        git(&["config", "user.email", "test@example.invalid"])?;
        git(&["config", "commit.gpgSign", "false"])?;
        git(&["config", "core.autocrlf", "false"])?;
        git(&["config", "core.hooksPath", "disabled-hooks"])?;
        std::fs::write(root.join("notes with spaces.txt"), "original\n")?;
        let notes = "notes with spaces.txt".to_owned();
        Operation::Stage(notes.clone()).execute(&root)?;
        Operation::Unstage(notes.clone()).execute(&root)?;
        assert_eq!(status(&root)?[0].code, '?');
        Operation::Stage(notes.clone()).execute(&root)?;
        Operation::Commit("initial commit".into()).execute(&root)?;
        git(&["tag", "-a", "v1", "-m", "first release"])?;

        Operation::Branch("feature/forecast".into()).execute(&root)?;
        std::fs::write(root.join("forecast.txt"), "scattered clouds\n")?;
        Operation::Stage("forecast.txt".into()).execute(&root)?;
        Operation::Commit("add forecast".into()).execute(&root)?;
        Operation::Switch("main".into()).execute(&root)?;
        std::fs::write(root.join(&notes), "revised\n")?;
        Operation::Stage(notes.clone()).execute(&root)?;
        assert_eq!(status(&root)?[0].area, Area::Staged);
        Operation::Unstage(notes.clone()).execute(&root)?;
        assert_eq!(status(&root)?[0].area, Area::Unstaged);
        Operation::Stage(notes.clone()).execute(&root)?;
        Operation::Commit("revise notes".into()).execute(&root)?;
        git(&[
            "merge",
            "--no-ff",
            "feature/forecast",
            "-m",
            "merge forecast",
        ])?;
        git(&[
            "worktree",
            "add",
            "-b",
            "review",
            linked.to_str().expect("Unicode path"),
            "v1",
        ])?;
        assert_eq!(worktrees(&root)?.len(), 2);
        assert_eq!(history(&linked, &HistoryScope::All, PAGE)?.branch, "review");

        let history = history(&root, &HistoryScope::All, PAGE)?;
        assert_eq!(history.commits.len(), 4);
        let merge = &history.commits[0];
        assert_eq!(history.head.as_ref(), Some(&merge.id));
        assert_eq!(merge.parents.len(), 2);
        let base = history.commits.last().expect("root commit");
        assert!(
            history
                .refs
                .iter()
                .any(|reference| reference.label() == "v1" && reference.target == base.id)
        );
        let merge_details = details(&root, merge, None)?;
        assert_eq!(merge_details.files[0].path, "forecast.txt");
        let compared = details(&root, merge, Some(base))?;
        assert_eq!(compared.files.len(), 2);
        let change = compared
            .files
            .iter()
            .find(|change| change.path == notes)
            .expect("changed notes");
        let pair = compare(change, Some(&compared), true);
        assert_eq!(snapshot(&root, &pair.left, &notes, false)?, b"original\n");
        assert_eq!(snapshot(&root, &pair.right, &notes, false)?, b"revised\n");
        std::fs::write(linked.join("review-only.txt"), "review\n")?;
        Operation::Stage("review-only.txt".into()).execute(&linked)?;
        Operation::Commit("independent review".into()).execute(&linked)?;
        let review = HistoryScope::Selected(BTreeSet::from(["refs/heads/review".into()]));
        let joint = HistoryScope::Selected(BTreeSet::from([
            "HEAD".into(),
            "refs/heads/review".into(),
            "refs/heads/feature/forecast".into(),
            "refs/heads/ref-deleted".into(),
        ]));
        let joint_log = super::history(&root, &joint, PAGE)?;
        let head_log = super::history(&root, &HistoryScope::Head, PAGE)?;
        let review_log = super::history(&root, &review, PAGE)?;
        assert_eq!(head_log.commits.len(), 4);
        assert_eq!(review_log.commits.len(), 2);
        assert_eq!(joint_log.commits.len(), 5);
        let graph = crate::graph::Graph::build(&joint_log.commits);
        let merge_index = joint_log
            .commits
            .iter()
            .position(|item| item.id == merge.id)
            .unwrap();
        assert_eq!(
            graph.rows[merge_index]
                .rails
                .iter()
                .filter(|rail| rail.start == crate::graph::Span::Middle)
                .count(),
            2
        );
        assert!(
            super::history(&root, &HistoryScope::Selected(BTreeSet::new()), PAGE)?
                .commits
                .is_empty()
        );
        Ok(())
    }
}
