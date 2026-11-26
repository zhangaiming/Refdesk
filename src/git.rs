use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::io::{BufRead, BufReader, Read, Seek, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Output, Stdio};
use std::time::{Duration, Instant};

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

pub const PAGE: usize = 32;
pub(crate) const MAX_SNAPSHOT: u64 = 64 * 1024 * 1024;

#[derive(Debug)]
pub(crate) struct SnapshotLimit;

impl std::fmt::Display for SnapshotLimit {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("文件超过 64 MiB")
    }
}

impl std::error::Error for SnapshotLimit {}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Commit {
    pub id: String,
    pub parents: Vec<String>,
    pub short: String,
    pub author: String,
    pub date: String,
    pub author_timestamp: Option<i64>,
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
    pub refs: Vec<Reference>,
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
    pub left_gitlink: bool,
    pub right_gitlink: bool,
}

impl Change {
    pub fn index_operation(&self) -> Option<Operation> {
        let mut paths = vec![self.path.clone()];
        if self.code == 'R'
            && let Some(old_path) = &self.old_path
        {
            paths.push(old_path.clone());
        }
        match self.area {
            Area::Staged => Some(Operation::Unstage(paths)),
            Area::Unstaged => Some(Operation::Stage(paths)),
            Area::Commit => None,
        }
    }

    pub fn is_gitlink(&self) -> bool {
        self.left_gitlink || self.right_gitlink
    }
}

#[derive(Debug)]
pub struct Status {
    pub head: Option<String>,
    pub files: Vec<Change>,
}

#[derive(Debug)]
pub struct Submodule {
    pub path: PathBuf,
    pub available: bool,
}

#[derive(Clone, Debug)]
pub enum Version {
    Empty,
    Index,
    Worktree,
    Head(String),
    Commit(String),
}

impl Version {
    pub fn title(&self) -> &str {
        match self {
            Self::Empty => "∅",
            Self::Index => "暂存区",
            Self::Worktree => "工作树",
            Self::Head(_) => "HEAD",
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
    pub left_gitlink: bool,
    pub right_gitlink: bool,
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
    #[cfg(test)]
    isolate_test_environment(&mut cmd);

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

#[cfg(test)]
fn isolate_test_environment(cmd: &mut Command) {
    const NULL_DEVICE: &str = if cfg!(windows) { "NUL" } else { "/dev/null" };
    // Inherited Git variables can redirect a fixture command to a different repository.
    for (name, _) in std::env::vars_os() {
        if name
            .to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("GIT_")
        {
            cmd.env_remove(name);
        }
    }
    cmd.env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", NULL_DEVICE)
        .env("GIT_ATTR_NOSYSTEM", "1")
        .env("GIT_TEMPLATE_DIR", "")
        .env("LC_ALL", "C")
        .args([
            "-c",
            &format!("core.attributesFile={NULL_DEVICE}"),
            "-c",
            &format!("core.excludesFile={NULL_DEVICE}"),
        ]);
}

#[cfg(test)]
pub(crate) fn fixture_command(root: &Path) -> Command {
    command(Some(root), false)
}

fn output(root: &Path, args: &[&str], read_only: bool) -> Result<Output> {
    command(Some(root), read_only)
        .args(args)
        .output()
        .with_context(|| format!("无法执行 Git（{}）", root.display()))
}

#[derive(Debug)]
struct GitExit {
    code: Option<i32>,
    message: String,
}

impl std::fmt::Display for GitExit {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for GitExit {}

pub(crate) fn error_kind(error: &anyhow::Error) -> (&'static str, Option<i32>) {
    if let Some(exit) = error.downcast_ref::<GitExit>() {
        ("git_exit", exit.code)
    } else if error.downcast_ref::<std::io::Error>().is_some() {
        ("io", None)
    } else {
        ("validation_or_parse", None)
    }
}

pub(crate) fn log_version() {
    if !tracing::enabled!(tracing::Level::INFO) {
        return;
    }
    let version = command(None, true)
        .arg("--version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| {
            let output = std::str::from_utf8(&output.stdout).ok()?;
            let version = output
                .strip_prefix("git version ")?
                .split_whitespace()
                .next()?;
            let digits = |part: &str| !part.is_empty() && part.bytes().all(|c| c.is_ascii_digit());
            let (numeric, windows_build) = version
                .split_once(".windows.")
                .map_or((version, None), |(numeric, build)| (numeric, Some(build)));
            if version.len() <= 32
                && numeric.split('.').count() >= 2
                && numeric.split('.').all(digits)
                && windows_build.is_none_or(digits)
            {
                Some(version.to_owned())
            } else {
                None
            }
        });
    if let Some(version) = version {
        tracing::info!(version, "git_version");
    } else {
        tracing::warn!("git_version_unavailable");
    }
}

fn checked(result: Output) -> Result<Vec<u8>> {
    if !result.status.success() {
        let message = String::from_utf8_lossy(&result.stderr);
        let stdout = String::from_utf8_lossy(&result.stdout);
        let message = if message.trim().is_empty() {
            stdout.trim()
        } else {
            message.trim()
        };
        return Err(GitExit {
            code: result.status.code(),
            message: message.to_owned(),
        }
        .into());
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

pub fn has_superproject(root: &Path) -> Result<bool> {
    let output = run(root, &["rev-parse", "--show-superproject-working-tree"])?;
    Ok(!trim(&output).is_empty())
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

pub fn submodules(root: &Path) -> Result<Vec<Submodule>> {
    let index = run(root, &["ls-files", "--stage", "-z"])?;
    // The staged diff also exposes HEAD-only gitlinks (for example, a staged removal).
    let staged = run(
        root,
        &[
            "diff",
            "--cached",
            "--raw",
            "-z",
            "--no-renames",
            "--no-ext-diff",
            "--no-textconv",
            "--ignore-submodules=none",
        ],
    )?;
    let mut paths = BTreeSet::new();
    for entry in index.split(|byte| *byte == 0) {
        if entry.starts_with(b"160000 ")
            && let Some(tab) = entry.iter().position(|byte| *byte == b'\t')
        {
            paths.insert(&entry[tab + 1..]);
        }
    }
    let mut entries = staged.split(|byte| *byte == 0);
    while let (Some(header), Some(path)) = (entries.next(), entries.next()) {
        if header.starts_with(b":160000 ") {
            paths.insert(path);
        }
    }
    Ok(paths
        .into_iter()
        .filter_map(|entry| {
            let name = std::str::from_utf8(entry).ok()?;
            let path = root.join(safe_relative(name).ok()?);
            let available = path.join(".git").exists()
                && run(&path, &["rev-parse", "--show-prefix"])
                    .is_ok_and(|prefix| trim(&prefix).is_empty());
            Some(Submodule { path, available })
        })
        .collect())
}

fn oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|ch| ch.is_ascii_hexdigit())
}

pub fn history_snapshot(root: &Path, scope: &HistoryScope) -> Result<(History, HistoryStart)> {
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
    let targets = match scope {
        HistoryScope::Head => head.iter().cloned().collect(),
        HistoryScope::All => refs
            .iter()
            .map(|reference| reference.target.clone())
            .chain(head.iter().cloned())
            .collect(),
        HistoryScope::Selected(filter) => {
            let mut targets: BTreeSet<_> = refs
                .iter()
                .filter(|reference| filter.contains(&reference.name))
                .map(|reference| reference.target.clone())
                .collect();
            if filter.contains("HEAD")
                && let Some(head) = &head
            {
                targets.insert(head.clone());
            }
            targets.into_iter().collect()
        }
    };
    Ok((
        History { branch, head, refs },
        HistoryStart {
            root: root.to_owned(),
            targets,
        },
    ))
}

pub struct HistoryStart {
    root: PathBuf,
    targets: Vec<String>,
}

impl HistoryStart {
    pub fn open(&self, skip: usize) -> Result<Option<LogReader>> {
        if self.targets.is_empty() {
            return Ok(None);
        }
        let stderr = tempfile::tempfile()?;
        let skip = format!("--skip={skip}");
        let mut child = command(Some(&self.root), true)
            .args([
                "log",
                "--date-order",
                "--date=short",
                "-z",
                "--format=%H%x00%P%x00%h%x00%an%x00%ad%x00%at%x00%s",
                &skip,
                "--stdin",
                "--",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(stderr.try_clone()?)
            .spawn()
            .context("无法启动 Git 历史读取")?;
        let input = child.stdin.take().expect("piped stdin");
        let stdout = BufReader::new(child.stdout.take().expect("piped stdout"));
        let reader = LogReader {
            child,
            stdout,
            stderr,
        };
        {
            let mut input = std::io::BufWriter::new(input);
            for target in &self.targets {
                writeln!(input, "{target}")?;
            }
            input.flush()?;
        }
        Ok(Some(reader))
    }
}

pub struct LogReader {
    child: Child,
    stdout: BufReader<ChildStdout>,
    stderr: std::fs::File,
}

impl LogReader {
    pub fn next_commit(&mut self) -> Result<Option<Commit>> {
        let mut bytes = Vec::new();
        for field in 0..7 {
            if self.stdout.read_until(0, &mut bytes)? == 0 || bytes.last() != Some(&0) {
                if !self.child.wait()?.success() {
                    self.stderr.rewind()?;
                    let mut message = Vec::new();
                    (&mut self.stderr)
                        .take(16 * 1024)
                        .read_to_end(&mut message)?;
                    bail!("无法读取 Git 历史：{}", string(&message).trim());
                }
                ensure!(field == 0 && bytes.is_empty(), "Git 历史记录不完整");
                return Ok(None);
            }
        }
        let mut fields = bytes.split(|byte| *byte == 0).map(string);
        let id = fields.next().expect("commit id");
        ensure!(oid(&id), "无法识别 Git 历史提交：{id}");
        Ok(Some(Commit {
            id,
            parents: fields
                .next()
                .expect("commit parents")
                .split_whitespace()
                .map(str::to_owned)
                .collect(),
            short: fields.next().expect("short id"),
            author: fields.next().expect("author"),
            date: fields.next().expect("date"),
            author_timestamp: fields.next().expect("author timestamp").parse().ok(),
            subject: fields.next().expect("subject"),
        }))
    }
}

impl Drop for LogReader {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
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

#[cfg(test)]
struct HistorySample {
    info: History,
    commits: Vec<Commit>,
}

#[cfg(test)]
fn history(root: &Path, scope: &HistoryScope, limit: usize) -> Result<HistorySample> {
    let (info, source) = history_snapshot(root, scope)?;
    let mut commits = Vec::new();
    if let Some(mut reader) = source.open(0)? {
        for _ in 0..limit {
            let Some(commit) = reader.next_commit()? else {
                break;
            };
            commits.push(commit);
        }
    }
    Ok(HistorySample { info, commits })
}

pub fn status(root: &Path) -> Result<Status> {
    let bytes = run(
        root,
        &[
            "status",
            "--porcelain=v2",
            "--branch",
            "-z",
            "--untracked-files=all",
            "--ignore-submodules=dirty",
        ],
    )?;
    let mut fields = bytes.split(|&byte| byte == 0);
    let mut head = None;
    let mut files = Vec::new();
    while let Some(record) = fields.next() {
        if let Some(value) = record.strip_prefix(b"# branch.oid ") {
            let value = string(value);
            ensure!(value == "(initial)" || oid(&value), "无法识别工作树的 HEAD");
            head = Some((value != "(initial)").then_some(value));
        } else if let Some(path) = record.strip_prefix(b"? ") {
            files.push(Change {
                path: string(path),
                old_path: None,
                code: '?',
                area: Area::Unstaged,
                left_gitlink: false,
                right_gitlink: false,
            });
        } else if record.starts_with(b"u ") {
            let parts: Vec<_> = record.splitn(11, |&byte| byte == b' ').collect();
            ensure!(parts.len() == 11, "无法读取冲突文件状态");
            files.push(Change {
                path: string(parts[10]),
                old_path: None,
                code: 'U',
                area: Area::Unstaged,
                left_gitlink: false,
                right_gitlink: parts[6] == b"160000",
            });
        } else if record.starts_with(b"1 ") || record.starts_with(b"2 ") {
            let renamed = record[0] == b'2';
            let field_count = if renamed { 10 } else { 9 };
            let parts: Vec<_> = record.splitn(field_count, |&byte| byte == b' ').collect();
            ensure!(
                parts.len() == field_count && parts[1].len() == 2,
                "无法读取文件状态"
            );
            let path = string(parts[field_count - 1]);
            let old = if renamed {
                Some(string(fields.next().context("重命名记录缺少原路径")?))
            } else {
                None
            };
            for (index, area) in [Area::Staged, Area::Unstaged].into_iter().enumerate() {
                let code = parts[1][index] as char;
                if code != '.' {
                    files.push(Change {
                        path: path.clone(),
                        old_path: old.clone().filter(|_| matches!(code, 'R' | 'C')),
                        code,
                        area,
                        left_gitlink: parts[3 + index] == b"160000",
                        right_gitlink: parts[4 + index] == b"160000",
                    });
                }
            }
        }
    }
    Ok(Status {
        head: head.context("工作树状态缺少 HEAD")?,
        files,
    })
}

fn raw_changes(bytes: &[u8]) -> Result<Vec<Change>> {
    let mut fields = bytes.split(|ch| *ch == 0);
    let mut files = Vec::new();
    while let Some(header) = fields.next().filter(|header| !header.is_empty()) {
        let header = header.strip_prefix(b":").context("无效的文件差异记录")?;
        let parts: Vec<_> = header.split(|&byte| byte == b' ').collect();
        ensure!(parts.len() == 5, "无效的文件差异记录");
        let code = *parts[4].first().context("文件差异缺少状态")? as char;
        let first = string(fields.next().context("文件差异缺少路径")?);
        let (old_path, path) = if matches!(code, 'R' | 'C') {
            (
                Some(first),
                string(fields.next().context("重命名记录缺少目标路径")?),
            )
        } else {
            (None, first)
        };
        files.push(Change {
            path,
            old_path,
            code,
            area: Area::Commit,
            left_gitlink: parts[0] == b"160000",
            right_gitlink: parts[1] == b"160000",
        });
    }
    Ok(files)
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
                    "--raw",
                    "--ignore-submodules=none",
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
                    "--raw",
                    "--ignore-submodules=none",
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
        files: raw_changes(&files)?,
        from,
        to,
        base,
    })
}

pub fn compare(file: &Change, details: Option<&Details>, head: Option<&str>) -> Compare {
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
            let from = head.map_or(Version::Empty, |id| Version::Head(id.to_owned()));
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
        left_gitlink: file.left_gitlink,
        right_gitlink: file.right_gitlink,
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
    snapshot_with(root, version, path, missing_ok, &|root, args| {
        output(root, args, true)
    })
}

fn snapshot_with(
    root: &Path,
    version: &Version,
    path: &str,
    missing_ok: bool,
    read: &impl Fn(&Path, &[&str]) -> Result<Output>,
) -> Result<Vec<u8>> {
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
            ensure!(metadata.len() <= MAX_SNAPSHOT, SnapshotLimit);
            let mut bytes = Vec::new();
            std::fs::File::open(file)?
                .take(MAX_SNAPSHOT + 1)
                .read_to_end(&mut bytes)?;
            ensure!(bytes.len() as u64 <= MAX_SNAPSHOT, SnapshotLimit);
            Ok(bytes)
        }
        Version::Index | Version::Head(_) | Version::Commit(_) => {
            let spec = match version {
                Version::Head(id) | Version::Commit(id) => {
                    ensure!(oid(id), "无效的提交标识");
                    format!("{id}:{path}")
                }
                _ => format!(":{path}"),
            };
            let size = read(root, &["cat-file", "-s", &spec])?;
            if missing_ok && !size.status.success() {
                return Ok(Vec::new());
            }
            let size: u64 = trim(&checked(size)?).parse().context("无法读取文件大小")?;
            ensure!(size <= MAX_SNAPSHOT, SnapshotLimit);
            checked(read(root, &["cat-file", "blob", &spec])?).context("无法读取文件版本")
        }
    }
}

pub fn diff_snapshot(
    worktree: &Path,
    version: &Version,
    path: &str,
    gitlink: bool,
) -> Result<Vec<u8>> {
    diff_snapshot_with(worktree, version, path, gitlink, &|root, args| {
        output(root, args, true)
    })
}

pub(crate) fn diff_snapshot_cancellable(
    root: &Path,
    version: &Version,
    path: &str,
    gitlink: bool,
    cancel: &crate::builtin_diff::Cancellation,
) -> Result<Vec<u8>> {
    ensure!(!cancel.requested(), "读取已取消");
    diff_snapshot_with(root, version, path, gitlink, &|root, args| {
        ensure!(!cancel.requested(), "读取已取消");
        #[cfg(windows)]
        {
            let mut child = command(Some(root), true)
                .args(args)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?;
            match crate::window_job::assign(&child, Default::default()) {
                Ok(job) => crate::window_job::cancel_with(job, cancel.signal(), || {
                    Ok(child.wait_with_output()?)
                }),
                Err(error) => {
                    if child.try_wait()?.is_some() {
                        return Ok(child.wait_with_output()?);
                    }
                    let _ = child.kill();
                    let _ = child.wait();
                    Err(error.into())
                }
            }
        }
        #[cfg(not(windows))]
        output(root, args, true)
    })
}

fn diff_snapshot_with(
    worktree: &Path,
    version: &Version,
    path: &str,
    gitlink: bool,
    read: &impl Fn(&Path, &[&str]) -> Result<Output>,
) -> Result<Vec<u8>> {
    if !gitlink {
        return snapshot_with(worktree, version, path, false, read);
    }
    let run = |root: &Path, args: &[&str]| checked(read(root, args)?);
    let relative = safe_relative(path)?;
    let id = match version {
        Version::Index => trim(&run(
            worktree,
            &["rev-parse", "--verify", &format!(":{path}")],
        )?),
        Version::Head(id) | Version::Commit(id) => {
            ensure!(oid(id), "无效的提交标识");
            trim(&run(
                worktree,
                &["rev-parse", "--verify", &format!("{id}:{path}")],
            )?)
        }
        Version::Worktree => {
            let module = worktree.join(relative);
            ensure!(module.join(".git").exists(), "子模块未检出");
            let top = run(&module, &["rev-parse", "--show-toplevel"])?;
            ensure!(
                PathBuf::from(trim(&top)).canonicalize()? == module.canonicalize()?,
                "子模块未检出"
            );
            trim(&run(&module, &["rev-parse", "--verify", "HEAD"])?)
        }
        Version::Empty => return Ok(Vec::new()),
    };
    ensure!(oid(&id), "无效的子模块提交标识");
    Ok(format!("Subproject commit {id}\n").into_bytes())
}

pub(crate) struct LfsPointer<'a> {
    oid: &'a str,
    size: u64,
}

impl<'a> LfsPointer<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Option<Self>> {
        const VERSION: &str = "version https://git-lfs.github.com/spec/v1";
        if !bytes.starts_with(VERSION.as_bytes()) {
            return Ok(None);
        }
        ensure!(bytes.len() <= 1024, "无效的 Git LFS 指针");
        let mut lines = std::str::from_utf8(bytes)
            .context("无效的 Git LFS 指针")?
            .lines();
        ensure!(lines.next() == Some(VERSION), "无效的 Git LFS 指针");
        let oid_line = lines.find(|line| !line.starts_with("ext-"));
        let oid = oid_line
            .and_then(|line| line.strip_prefix("oid sha256:"))
            .filter(|oid| {
                oid.len() == 64
                    && oid
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            })
            .context("无效的 Git LFS 对象标识")?;
        let size = lines
            .next()
            .and_then(|line| line.strip_prefix("size "))
            .and_then(|size| size.parse().ok())
            .context("无效的 Git LFS 对象大小")?;
        ensure!(lines.next().is_none(), "无效的 Git LFS 指针");
        Ok(Some(Self { oid, size }))
    }

    pub fn read(&self, dir: &Path) -> Result<Option<Vec<u8>>> {
        ensure!(self.size <= MAX_SNAPSHOT, SnapshotLimit);
        let oid = self.oid;
        let path = dir.join(&oid[..2]).join(&oid[2..4]).join(oid);
        let metadata = match std::fs::metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(None);
            }
            result => result.context("无法读取 Git LFS 对象信息")?,
        };
        ensure!(
            metadata.is_file() && metadata.len() == self.size,
            "本地 Git LFS 对象大小不符"
        );
        let mut bytes = Vec::new();
        std::fs::File::open(path)
            .and_then(|file| file.take(self.size + 1).read_to_end(&mut bytes))
            .context("无法读取 Git LFS 对象")?;
        ensure!(bytes.len() as u64 == self.size, "本地 Git LFS 对象大小不符");
        Ok(Some(bytes))
    }
}

pub(crate) fn lfs_object_dir(root: &Path) -> Result<PathBuf> {
    let common_dir = run(
        root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    let config = output(root, &["config", "--null", "--get", "lfs.storage"], true)?;
    let storage = if config.status.code() == Some(1) {
        String::new()
    } else {
        string(&checked(config)?).trim_end_matches('\0').to_owned()
    };
    let storage = if storage.is_empty() { "lfs" } else { &storage };
    Ok(PathBuf::from(trim(&common_dir))
        .join(storage)
        .join("objects"))
}

pub(crate) fn download_lfs_object(
    root: &Path,
    path: &str,
    pointer: &[u8],
    canceled: impl Fn() -> bool,
) -> Result<()> {
    const TIMEOUT: Duration = Duration::from_secs(15);
    const CANCELED: &str = "Git LFS 下载已取消";
    const FAILED: &str = "无法下载 Git LFS 对象";
    ensure!(!canceled(), CANCELED);
    // Git LFS smudge also attempts to install hooks. Keep those inside the temporary directory.
    let hooks = tempfile::tempdir().context(FAILED)?;
    let mut hooks_config = OsString::from("core.hooksPath=");
    hooks_config.push(hooks.path());
    let mut child = command(Some(root), true)
        .arg("-c")
        .arg(hooks_config)
        .args(["lfs", "smudge", "--", path])
        .env("GCM_INTERACTIVE", "never")
        .env("SSH_ASKPASS_REQUIRE", "never")
        .env_remove("GIT_LFS_PROGRESS")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context(FAILED)?;
    let started = Instant::now();
    let result = (|| {
        #[cfg(windows)]
        let _job = crate::window_job::assign(&child, Default::default()).context(FAILED)?;
        child
            .stdin
            .take()
            .expect("piped stdin")
            .write_all(pointer)
            .context(FAILED)?;
        loop {
            ensure!(!canceled(), CANCELED);
            if let Some(status) = child.try_wait().context(FAILED)? {
                ensure!(status.success(), FAILED);
                return Ok(());
            }
            ensure!(started.elapsed() < TIMEOUT, "Git LFS 下载超时");
            std::thread::sleep(Duration::from_millis(25));
        }
    })();
    if result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    result
}

pub fn base_snapshot(root: &Path, pair: &Compare) -> Result<Vec<u8>> {
    let version = &pair.base;
    let path = &pair.left_path;
    let gitlink = match version {
        Version::Head(id) | Version::Commit(id) if pair.left_gitlink || pair.right_gitlink => {
            run(root, &["ls-tree", "-z", id, "--", path])?.starts_with(b"160000 ")
        }
        Version::Index => pair.left_gitlink,
        _ => false,
    };
    if gitlink {
        diff_snapshot(root, version, path, true)
    } else {
        snapshot(root, version, path, true)
    }
}

#[derive(Clone, Debug)]
pub enum Operation {
    Stage(Vec<String>),
    Unstage(Vec<String>),
    Commit(String),
    Switch(String),
    Branch(String),
    Fetch,
    Pull,
    Push,
}

impl Operation {
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Self::Stage(_) => "stage",
            Self::Unstage(_) => "unstage",
            Self::Commit(_) => "commit",
            Self::Switch(_) => "switch",
            Self::Branch(_) => "branch_create",
            Self::Fetch => "fetch",
            Self::Pull => "pull",
            Self::Push => "push",
        }
    }

    pub fn execute(&self, worktree: &Path) -> Result<String> {
        ensure!(
            root(worktree)?.canonicalize()? == worktree.canonicalize()?,
            "Git 操作需要工作树根目录"
        );
        let root = worktree;
        let args = match self {
            Self::Stage(paths) | Self::Unstage(paths) => {
                ensure!(!paths.is_empty(), "没有选中的文件");
                for path in paths {
                    safe_relative(path)?;
                }
                let mut args = if matches!(self, Self::Stage(_)) {
                    vec!["add", "--"]
                } else if output(root, &["rev-parse", "--verify", "--quiet", "HEAD"], true)?
                    .status
                    .success()
                {
                    vec!["restore", "--staged", "--"]
                } else {
                    vec!["rm", "-f", "--cached", "--ignore-unmatch", "--"]
                };
                args.extend(paths.iter().map(String::as_str));
                args
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
        let files = raw_changes(
            b":100644 100644 0123 0123 R100\0before name.xlsx\0after name.xlsx\0:100644 100644 0123 4567 M\0other.txt\0",
        )
        .unwrap();
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
    fn index_operations_use_the_worktree_root_and_both_rename_paths() -> Result<()> {
        let sandbox = tempfile::tempdir()?;
        let worktree = sandbox.path().join("中文 worktree");
        let nested = worktree.join("nested");
        std::fs::create_dir_all(&nested)?;
        let git = |args: &[&str]| checked(output(&worktree, args, false)?);
        git(&["init", "--initial-branch=main"])?;
        git(&["config", "user.name", "Refdesk test"])?;
        git(&["config", "user.email", "test@example.invalid"])?;
        git(&["config", "commit.gpgSign", "false"])?;
        git(&["config", "core.hooksPath", "disabled-hooks"])?;
        git(&["config", "core.autocrlf", "false"])?;
        git(&["config", "status.renames", "true"])?;

        let name = "同名 文件.txt";
        let stage = Operation::Stage(vec![name.into()]);
        std::fs::write(worktree.join(name), "original\n")?;
        stage.execute(&worktree)?;
        std::fs::write(worktree.join(name), "edited after staging\n")?;
        let initial = status(&worktree)?;
        assert!(initial.head.is_none());
        initial.files[0]
            .index_operation()
            .unwrap()
            .execute(&worktree)?;
        assert_eq!(status(&worktree)?.files[0].code, '?');
        assert_eq!(
            std::fs::read(worktree.join(name))?,
            b"edited after staging\n"
        );

        std::fs::write(nested.join(name), "different original\n")?;
        git(&["add", "--", "."])?;
        Operation::Commit("baseline".into()).execute(&worktree)?;
        std::fs::write(worktree.join(name), "root edit\n")?;
        std::fs::write(nested.join(name), "nested edit\n")?;
        assert!(stage.execute(&nested).is_err());
        assert!(run(&worktree, &["diff", "--cached", "--name-only"])?.is_empty());
        assert_eq!(root(&nested)?.canonicalize()?, worktree.canonicalize()?);
        stage.execute(&root(&nested)?)?;
        let state = status(&worktree)?;
        let staged = state
            .files
            .iter()
            .find(|file| file.area == Area::Staged)
            .unwrap();
        assert_eq!(staged.path, name);
        let pair = compare(staged, None, state.head.as_deref());
        assert_eq!(
            snapshot(&worktree, &pair.left, name, false)?,
            b"edited after staging\n"
        );
        assert_eq!(
            snapshot(&worktree, &pair.right, name, false)?,
            b"root edit\n"
        );
        Operation::Commit("changed head".into()).execute(&worktree)?;
        assert_eq!(
            snapshot(&worktree, &pair.left, name, false)?,
            b"edited after staging\n"
        );

        let renamed = "新 文件.txt";
        git(&["mv", "--", name, renamed])?;
        let state = status(&worktree)?;
        let change = state.files.iter().find(|file| file.code == 'R').unwrap();
        assert_eq!(change.old_path.as_deref(), Some(name));
        assert_eq!(change.path, renamed);
        change.index_operation().unwrap().execute(&worktree)?;
        assert!(run(&worktree, &["diff", "--cached", "--name-only"])?.is_empty());
        assert_eq!(std::fs::read(worktree.join(renamed))?, b"root edit\n");
        assert!(!worktree.join(name).exists());
        Ok(())
    }

    #[test]
    fn discovers_gitlinks_and_checks_for_a_local_submodule_worktree() -> Result<()> {
        let sandbox = tempfile::tempdir()?;
        let parent = sandbox.path().join("parent");
        let source = sandbox.path().join("source");
        let git = |root: &Path, args: &[&str]| checked(output(root, args, false)?);
        for repo in [&parent, &source] {
            std::fs::create_dir(repo)?;
            git(repo, &["init", "--initial-branch=main"])?;
            git(repo, &["config", "user.name", "Refdesk test"])?;
            git(repo, &["config", "user.email", "test@example.invalid"])?;
            git(repo, &["config", "commit.gpgSign", "false"])?;
            git(repo, &["config", "core.hooksPath", "disabled-hooks"])?;
        }
        let local = "deps/共用 库";
        git(&source, &["commit", "--allow-empty", "-m", "child"])?;
        let oid = trim(&run(&source, &["rev-parse", "HEAD"])?);
        git(
            &parent,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                "--",
                source.to_str().unwrap(),
                local,
            ],
        )?;
        git(
            &parent,
            &[
                "update-index",
                "--add",
                "--cacheinfo",
                "160000",
                &oid,
                "unpopulated",
            ],
        )?;
        git(
            &parent,
            &[
                "config",
                "--file",
                ".gitmodules",
                "submodule.fake.path",
                "fake",
            ],
        )?;
        std::fs::create_dir(parent.join("fake"))?;
        git(&parent.join("fake"), &["init"])?;
        assert_eq!(
            submodules(&parent)?
                .into_iter()
                .map(|module| (module.path, module.available))
                .collect::<Vec<_>>(),
            vec![
                (parent.join(local), true),
                (parent.join("unpopulated"), false)
            ],
        );

        let state = status(&parent)?;
        let added = state.files.iter().find(|file| file.path == local).unwrap();
        let pair = compare(added, None, state.head.as_deref());
        assert!(matches!(pair.left, Version::Empty));
        assert_eq!(
            diff_snapshot(&parent, &pair.right, local, pair.right_gitlink)?,
            format!("Subproject commit {oid}\n").as_bytes()
        );
        git(&parent, &["commit", "-m", "parent"])?;
        let parent_history = history(&parent, &HistoryScope::Head, PAGE)?;
        let commit_details = details(&parent, &parent_history.commits[0], None)?;
        let added = commit_details
            .files
            .iter()
            .find(|file| file.path == local)
            .unwrap();
        let pair = compare(added, Some(&commit_details), None);
        assert_eq!(
            diff_snapshot(&parent, &pair.right, local, pair.right_gitlink)?,
            format!("Subproject commit {oid}\n").as_bytes()
        );

        let child = parent.join(local);
        git(
            &child,
            &[
                "-c",
                "user.name=Refdesk test",
                "-c",
                "user.email=test@example.invalid",
                "-c",
                "commit.gpgSign=false",
                "-c",
                "core.hooksPath=disabled-hooks",
                "commit",
                "--allow-empty",
                "-m",
                "new child tip",
            ],
        )?;
        let tip = trim(&run(&child, &["rev-parse", "HEAD"])?);
        let state = status(&parent)?;
        let moved = state.files.iter().find(|file| file.path == local).unwrap();
        assert_eq!(moved.area, Area::Unstaged);
        let pair = compare(moved, None, state.head.as_deref());
        assert_eq!(
            diff_snapshot(&parent, &pair.left, local, pair.left_gitlink)?,
            format!("Subproject commit {oid}\n").as_bytes()
        );
        assert_eq!(
            diff_snapshot(&parent, &pair.right, local, pair.right_gitlink)?,
            format!("Subproject commit {tip}\n").as_bytes()
        );

        git(&parent, &["update-index", "--force-remove", "--", local])?;
        let state = status(&parent)?;
        let removed = state
            .files
            .iter()
            .find(|file| file.path == local && file.area == Area::Staged)
            .unwrap();
        let pair = compare(removed, None, state.head.as_deref());
        assert_eq!(
            diff_snapshot(&parent, &pair.left, local, pair.left_gitlink)?,
            format!("Subproject commit {oid}\n").as_bytes()
        );
        assert!(matches!(pair.right, Version::Empty));
        assert_eq!(submodules(&parent)?.len(), 2);
        git(&parent, &["commit", "-m", "remove link"])?;
        let modules = submodules(&parent)?;
        assert_eq!(modules.len(), 1);
        assert_eq!(modules[0].path, parent.join("unpopulated"));
        assert!(!modules[0].available);
        Ok(())
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
                .info
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
    fn pull_fast_forwards_and_rejects_divergence() -> Result<()> {
        let sandbox = tempfile::tempdir()?;
        let root = sandbox.path();
        let sender = root.join("sender");
        let receiver = root.join("receiver with spaces");
        let git = |worktree: &Path, args: &[&str]| checked(output(worktree, args, false)?);

        git(
            root,
            &["init", "--bare", "--initial-branch=main", "origin.git"],
        )?;
        git(root, &["clone", "--", "origin.git", "sender"])?;
        git(&sender, &["config", "user.name", "Refdesk test"])?;
        git(&sender, &["config", "user.email", "test@example.invalid"])?;
        git(&sender, &["commit", "--allow-empty", "-m", "base"])?;
        git(&sender, &["push", "origin", "main"])?;
        git(root, &["clone", "--", "origin.git", "receiver with spaces"])?;
        git(&receiver, &["config", "user.name", "Refdesk test"])?;
        git(&receiver, &["config", "user.email", "test@example.invalid"])?;
        // Give plain git pull a merge policy so a missing --ff-only cannot pass accidentally.
        git(&receiver, &["config", "pull.rebase", "false"])?;

        git(&sender, &["commit", "--allow-empty", "-m", "remote update"])?;
        git(&sender, &["push", "origin", "main"])?;
        Operation::Pull.execute(&receiver)?;
        let head = |worktree: &Path| run(worktree, &["rev-parse", "HEAD"]);
        assert_eq!(head(&receiver)?, head(&sender)?);

        git(
            &receiver,
            &["commit", "--allow-empty", "-m", "local branch tip"],
        )?;
        let local_head = head(&receiver)?;
        git(
            &sender,
            &["commit", "--allow-empty", "-m", "remote branch tip"],
        )?;
        git(&sender, &["push", "origin", "main"])?;

        let error = Operation::Pull.execute(&receiver).unwrap_err();
        assert!(error.to_string().contains("fast-forward"), "{error:#}");
        assert_eq!(head(&receiver)?, local_head);
        assert_eq!(
            run(&receiver, &["rev-parse", "origin/main"])?,
            head(&sender)?
        );
        assert!(run(&receiver, &["status", "--porcelain"])?.is_empty());
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
        Operation::Stage(vec![notes.clone()]).execute(&root)?;
        Operation::Unstage(vec![notes.clone()]).execute(&root)?;
        assert_eq!(status(&root)?.files[0].code, '?');
        Operation::Stage(vec![notes.clone()]).execute(&root)?;
        git(&[
            "commit",
            "--date=2000-01-01T00:30:00+08:00",
            "-m",
            "initial commit",
        ])?;
        git(&["tag", "-a", "v1", "-m", "first release"])?;

        Operation::Branch("feature/forecast".into()).execute(&root)?;
        std::fs::write(root.join("forecast.txt"), "scattered clouds\n")?;
        Operation::Stage(vec!["forecast.txt".into()]).execute(&root)?;
        Operation::Commit("add forecast".into()).execute(&root)?;
        Operation::Switch("main".into()).execute(&root)?;
        std::fs::write(root.join(&notes), "revised\n")?;
        Operation::Stage(vec![notes.clone()]).execute(&root)?;
        assert_eq!(status(&root)?.files[0].area, Area::Staged);
        Operation::Unstage(vec![notes.clone()]).execute(&root)?;
        assert_eq!(status(&root)?.files[0].area, Area::Unstaged);
        Operation::Stage(vec![notes.clone()]).execute(&root)?;
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
        assert_eq!(
            history(&linked, &HistoryScope::All, PAGE)?.info.branch,
            "review"
        );

        let history = history(&root, &HistoryScope::All, PAGE)?;
        assert_eq!(history.commits.len(), 4);
        let merge = &history.commits[0];
        assert_eq!(history.info.head.as_ref(), Some(&merge.id));
        assert_eq!(merge.parents.len(), 2);
        let base = history.commits.last().expect("root commit");
        assert_eq!(base.author_timestamp, Some(946_657_800));
        assert_eq!(base.date, "2000-01-01");
        assert!(
            history
                .info
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
        let pair = compare(change, Some(&compared), None);
        assert_eq!(snapshot(&root, &pair.left, &notes, false)?, b"original\n");
        assert_eq!(snapshot(&root, &pair.right, &notes, false)?, b"revised\n");
        std::fs::write(linked.join("review-only.txt"), "review\n")?;
        Operation::Stage(vec!["review-only.txt".into()]).execute(&linked)?;
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
