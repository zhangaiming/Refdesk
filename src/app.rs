use crate::diff::{self, Content};
use crate::git::{self, Change, Commit, Compare, Details, History, HistoryScope, Operation};
use crate::graph::Graph;
use crate::settings::{self, Settings, Tool};
use crate::timing::{FrameCapture, Timings};
use anyhow::Result;
use crossbeam_channel::{Receiver, Sender, unbounded};
use eframe::egui;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tempfile::TempDir;

type Task = Box<dyn FnOnce() + Send>;
type Outcome<T> = std::result::Result<T, String>;

enum Event {
    Added(Outcome<Vec<PathBuf>>, bool),
    History(PathBuf, u64, Outcome<(History, Graph)>),
    Status(PathBuf, u64, Outcome<Vec<Change>>),
    Details(u64, Outcome<Details>),
    Diff(u64, Outcome<Content>),
    Operation(PathBuf, Option<String>, Outcome<String>),
    External(Outcome<TempDir>),
}

#[derive(Default)]
pub(crate) struct Repository {
    pub history: Option<History>,
    pub graph: Graph,
    pub changes: Vec<Change>,
    pub has_status: bool,
    pub history_busy: bool,
    pub status_busy: bool,
    pub status_error: Option<String>,
    pub history_error: Option<String>,
    pub filter: HistoryScope,
    pub limit: usize,
    history_request: u64,
    draft: String,
    status_request: u64,
    status_checked: Option<Instant>,
}

#[derive(Clone, Debug)]
pub(crate) enum Selection {
    Worktree,
    Commit {
        right: Box<Commit>,
        left: Option<Box<Commit>>,
    },
}

pub(crate) enum Dialog {
    Operation {
        root: PathBuf,
        operation: Operation,
        title: &'static str,
        question: String,
    },
    NewBranch(String),
    Forget(PathBuf),
}

pub(crate) struct Desk {
    pub settings: Settings,
    pub active: Option<PathBuf>,
    pub repos: HashMap<PathBuf, Repository>,
    pub selection: Selection,
    pub details: Option<Details>,
    pub details_error: Option<String>,
    pub details_busy: bool,
    pub file: Option<Change>,
    pub comparison: Option<Compare>,
    pub content: Option<Content>,
    pub diff_error: Option<String>,
    pub diff_busy: bool,
    pub editor: String,
    pub search: String,
    pub branch_query: String,
    pub search_row: Option<usize>,
    pub scroll_to: Option<usize>,
    pub dialog: Option<Dialog>,
    pub tools_open: bool,
    pub import_message: String,
    pub status_message: String,
    pub status_is_error: bool,
    pub operations: HashSet<PathBuf>,
    pub sheet_filter: String,
    pub force_text: bool,
    pub show_full_line: Option<String>,
    pub diff_reset: u64,
    pub graph_reset: u64,
    pub focused: bool,
    writes_enabled: bool,
    requests: u64,
    details_request: u64,
    diff_request: u64,
    external_dirs: Vec<TempDir>,
    timings: Option<Timings>,
    frame_capture: Option<FrameCapture>,
    tasks: Sender<Task>,
    events: Sender<Event>,
    inbox: Receiver<Event>,
}

fn readable<T>(result: Result<T>) -> Outcome<T> {
    result.map_err(|error| format!("{error:#}"))
}

impl Desk {
    pub fn new(
        ctx: &egui::Context,
        paths: Vec<PathBuf>,
        import_tools: bool,
        started: Instant,
    ) -> Self {
        let (tasks, work) = unbounded::<Task>();
        let (events, inbox) = unbounded();
        for index in 0..4 {
            let work = work.clone();
            std::thread::Builder::new()
                .name(format!("refdesk-git-{index}"))
                .spawn(move || {
                    while let Ok(job) = work.recv() {
                        job();
                    }
                })
                .expect("worker thread");
        }

        let (mut settings, warning, writes_enabled) = match Settings::load() {
            Ok(settings) => (settings, None, true),
            Err(error) => (Settings::default(), Some(error.to_string()), false),
        };
        for path in paths {
            if !settings
                .repositories
                .iter()
                .any(|old| same_path(old, &path))
            {
                settings.repositories.push(path);
            }
        }
        let import_message = if import_tools {
            match settings::smartgit_preferences()
                .ok_or_else(|| anyhow::anyhow!("找不到 SmartGit preferences.yml"))
                .and_then(|path| settings.import_tools(&path))
            {
                Ok(count) => format!("已导入 {count} 条 SmartGit diff 规则"),
                Err(error) => error.to_string(),
            }
        } else {
            String::new()
        };
        let initial = settings
            .active
            .clone()
            .and_then(|active| {
                settings
                    .repositories
                    .iter()
                    .find(|path| same_path(path, &active))
                    .cloned()
            })
            .or_else(|| settings.repositories.first().cloned());
        let status_is_error = warning.is_some();
        let mut desk = Self {
            settings,
            active: None,
            repos: HashMap::new(),
            selection: Selection::Worktree,
            details: None,
            details_error: None,
            details_busy: false,
            file: None,
            comparison: None,
            content: None,
            diff_error: None,
            diff_busy: false,
            editor: String::new(),
            search: String::new(),
            branch_query: String::new(),
            search_row: None,
            scroll_to: None,
            dialog: None,
            tools_open: false,
            import_message,
            status_message: warning.unwrap_or_default(),
            status_is_error,
            writes_enabled,
            operations: HashSet::new(),
            sheet_filter: String::new(),
            force_text: false,
            show_full_line: None,
            diff_reset: 0,
            graph_reset: 0,
            focused: true,
            requests: 0,
            details_request: 0,
            diff_request: 0,
            external_dirs: Vec::new(),
            timings: Timings::open(started),
            frame_capture: FrameCapture::from_environment(),
            tasks,
            events,
            inbox,
        };
        if let Some(path) = initial {
            desk.activate(ctx, path);
        }
        desk
    }

    fn next(&mut self) -> u64 {
        self.requests += 1;
        self.requests
    }

    fn submit(&self, ctx: &egui::Context, task: impl FnOnce() -> Event + Send + 'static) {
        let events = self.events.clone();
        let ctx = ctx.clone();
        let _ = self.tasks.send(Box::new(move || {
            let _ = events.send(task());
            ctx.request_repaint();
        }));
    }

    pub fn tell(&mut self, message: impl Into<String>, error: bool) {
        self.status_message = message.into();
        self.status_is_error = error;
    }

    pub fn save(&mut self) {
        if self.writes_enabled
            && let Err(error) = self.settings.save()
        {
            self.tell(format!("保存配置失败：{error:#}"), true);
        }
    }

    pub fn state(&self) -> Option<&Repository> {
        self.active.as_ref().and_then(|path| self.repos.get(path))
    }

    pub fn choose_repository(&self, ctx: &egui::Context) {
        self.submit(ctx, || {
            Event::Added(
                readable((|| {
                    let Some(path) = rfd::FileDialog::new()
                        .set_title("选择 Git 工作树")
                        .pick_folder()
                    else {
                        return Ok(Vec::new());
                    };
                    Ok(vec![git::root(&path)?])
                })()),
                true,
            )
        });
    }

    pub fn discover(&self, ctx: &egui::Context, modules: bool) {
        let Some(root) = self.active.clone() else {
            return;
        };
        self.submit(ctx, move || {
            Event::Added(
                readable(if modules {
                    git::submodules(&root)
                } else {
                    git::worktrees(&root)
                }),
                false,
            )
        });
    }

    pub fn activate(&mut self, ctx: &egui::Context, path: PathBuf) {
        if self.active.as_ref() == Some(&path) {
            return;
        }
        if let Some(previous) = self
            .active
            .as_ref()
            .and_then(|path| self.repos.get_mut(path))
        {
            previous.draft = std::mem::take(&mut self.editor);
        }
        if let Some(timings) = &mut self.timings {
            timings.activate(&path);
        }
        self.active = Some(path.clone());
        self.settings.active = Some(path.clone());
        self.repos
            .entry(path.clone())
            .or_insert_with(|| Repository {
                filter: self
                    .settings
                    .branch_selection
                    .get(&path)
                    .cloned()
                    .unwrap_or_default(),
                ..Repository::default()
            });
        self.selection = Selection::Worktree;
        self.details_busy = false;
        self.details = None;
        self.details_error = None;
        self.details_request = self.next();
        self.clear_diff();
        self.search.clear();
        self.branch_query.clear();
        self.search_row = None;
        self.scroll_to = None;
        self.graph_reset += 1;
        self.editor =
            std::mem::take(&mut self.repos.get_mut(&path).expect("active repository").draft);

        if self.repos[&path].history.is_none() && !self.repos[&path].history_busy {
            self.request_history(ctx, &path);
        }
        self.request_status(ctx, &path, false);
        self.save();
    }

    pub fn forget(&mut self, ctx: &egui::Context, path: &Path) {
        if self.operations.contains(path) {
            return;
        }
        self.settings.repositories.retain(|entry| entry != path);
        self.repos.remove(path);
        self.settings.branch_selection.remove(path);
        if self.active.as_deref() == Some(path) {
            self.active = None;
            self.settings.active = None;
            self.selection = Selection::Worktree;
            self.details = None;
            self.details_busy = false;
            self.details_request = self.next();
            self.clear_diff();
            if let Some(next) = self.settings.repositories.first().cloned() {
                self.activate(ctx, next);
            }
        }
        self.save();
    }

    pub fn request_history(&mut self, ctx: &egui::Context, path: &Path) {
        let request = self.next();
        let state = self
            .repos
            .entry(path.to_owned())
            .or_insert_with(|| Repository {
                filter: self
                    .settings
                    .branch_selection
                    .get(path)
                    .cloned()
                    .unwrap_or_default(),
                ..Repository::default()
            });
        if state.limit == 0 {
            state.limit = git::PAGE;
        }
        state.history_request = request;
        state.history_busy = true;
        state.history_error = None;
        let root = path.to_owned();
        let filter = state.filter.clone();
        let limit = state.limit;
        self.submit(ctx, move || {
            let result = readable(git::history(&root, &filter, limit).map(|history| {
                let graph = Graph::build(&history.commits);
                (history, graph)
            }));
            Event::History(root, request, result)
        });
    }

    pub fn request_status(&mut self, ctx: &egui::Context, path: &Path, force: bool) {
        let request = self.next();
        let state = self.repos.entry(path.to_owned()).or_default();
        if state.status_busy && !force {
            return;
        }
        if !force
            && state
                .status_checked
                .is_some_and(|checked| checked.elapsed() < Duration::from_secs(25))
        {
            return;
        }
        state.status_request = request;
        state.status_busy = true;
        state.status_error = None;
        let root = path.to_owned();
        self.submit(ctx, move || {
            let result = readable(git::status(&root));
            Event::Status(root, request, result)
        });
    }

    pub fn refresh(&mut self, ctx: &egui::Context) {
        if let Some(path) = self.active.clone() {
            self.request_history(ctx, &path);
            self.request_status(ctx, &path, true);
        }
    }

    pub fn filter(&mut self, ctx: &egui::Context, filter: HistoryScope) {
        let Some(path) = self.active.clone() else {
            return;
        };
        let state = self.repos.entry(path.clone()).or_default();
        if state.filter == filter {
            return;
        }
        state.filter = filter.clone();
        self.settings.branch_selection.insert(path.clone(), filter);
        state.limit = git::PAGE;
        self.graph_reset += 1;
        self.search_row = None;
        self.request_history(ctx, &path);
        self.save();
    }

    pub fn more(&mut self, ctx: &egui::Context) {
        let Some(path) = self.active.clone() else {
            return;
        };
        let state = self.repos.entry(path.clone()).or_default();
        if state.history_busy {
            return;
        }
        state.limit = (state.limit + git::PAGE).min(3000);
        self.request_history(ctx, &path);
    }

    pub fn choose_worktree(&mut self) {
        self.selection = Selection::Worktree;
        self.details_request = self.next();
        self.details_busy = false;
        self.details_error = None;
        self.details = None;
        self.clear_diff();
    }

    pub fn choose_commit(&mut self, ctx: &egui::Context, item: Commit, extend: bool) {
        let Some(root) = self.active.clone() else {
            return;
        };
        let (left, right) = if extend {
            match &self.selection {
                Selection::Commit { right: current, .. } if current.id != item.id => {
                    let position = |id: &str| {
                        self.repos[&root].history.as_ref().and_then(|history| {
                            history.commits.iter().position(|commit| commit.id == id)
                        })
                    };
                    if position(&item.id) < position(&current.id) {
                        (Some(current.as_ref().clone()), item)
                    } else {
                        (Some(item), current.as_ref().clone())
                    }
                }
                _ => (None, item),
            }
        } else {
            (None, item)
        };
        self.selection = Selection::Commit {
            right: Box::new(right.clone()),
            left: left.clone().map(Box::new),
        };
        self.details_request = self.next();
        let request = self.details_request;
        self.details = None;
        self.details_error = None;
        self.details_busy = true;
        self.clear_diff();
        self.submit(ctx, move || {
            Event::Details(
                request,
                readable(git::details(&root, &right, left.as_ref())),
            )
        });
    }

    pub fn choose_file(&mut self, ctx: &egui::Context, file: Change) {
        self.force_text = false;
        let has_head = self
            .state()
            .and_then(|state| state.history.as_ref())
            .is_some_and(|history| history.head.is_some());
        let pair = git::compare(&file, self.details.as_ref(), has_head);
        self.file = Some(file);
        self.comparison = Some(pair);
        self.sheet_filter.clear();
        self.request_diff(ctx);
    }

    pub fn request_diff(&mut self, ctx: &egui::Context) {
        let (Some(root), Some(pair)) = (self.active.clone(), self.comparison.clone()) else {
            return;
        };
        let request = self.next();
        self.diff_request = request;
        self.content = None;
        self.diff_error = None;
        self.diff_busy = true;
        self.diff_reset = request;
        let force_text = self.force_text;
        self.submit(ctx, move || {
            Event::Diff(request, readable(diff::compare(&root, &pair, force_text)))
        });
    }

    pub fn clear_diff(&mut self) {
        self.diff_request = self.next();
        self.file = None;
        self.comparison = None;
        self.content = None;
        self.diff_error = None;
        self.diff_busy = false;
    }

    pub fn operate(&mut self, ctx: &egui::Context, root: PathBuf, operation: Operation) {
        if !self.operations.insert(root.clone()) {
            return;
        }
        let committed_message = match &operation {
            Operation::Commit(message) => Some(message.clone()),
            _ => None,
        };
        self.tell("正在运行 Git…", false);
        self.submit(ctx, move || {
            let result = readable(operation.execute(&root));
            Event::Operation(root, committed_message, result)
        });
    }

    pub fn external(&mut self, ctx: &egui::Context, tool: Tool) {
        let (Some(root), Some(pair)) = (self.active.clone(), self.comparison.clone()) else {
            return;
        };
        self.tell(format!("正在准备 {} 的两个版本…", tool.name), false);
        self.submit(ctx, move || {
            Event::External(readable((|| {
                let launch = settings::prepare_external(&root, &pair, &tool)?;
                for invocation in &launch.commands {
                    let mut process = settings::start_external(&root, invocation)?;
                    // Reap tools independently. A detached GUI may read its snapshot later.
                    std::thread::spawn(move || {
                        let _ = process.wait();
                    });
                }
                Ok(launch.dir)
            })()))
        });
    }

    pub fn events(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.inbox.try_recv() {
            match event {
                Event::Added(result, activate) => match result {
                    Ok(paths) => {
                        let first = paths.first().cloned();
                        let mut added = 0;
                        for path in paths {
                            if !self
                                .settings
                                .repositories
                                .iter()
                                .any(|old| same_path(&path, old))
                            {
                                self.settings.repositories.push(path);
                                added += 1;
                            }
                        }
                        if activate && let Some(first) = first {
                            let first = self
                                .settings
                                .repositories
                                .iter()
                                .find(|path| same_path(path, &first))
                                .cloned()
                                .unwrap_or(first);
                            self.activate(ctx, first);
                        }
                        if added > 0 {
                            self.tell(format!("已添加 {added} 个仓库"), false);
                            self.save();
                        } else if !activate {
                            self.tell("没有新的工作树可以添加", false);
                        }
                    }
                    Err(error) => self.tell(error, true),
                },
                Event::History(path, request, result) => {
                    let Some(state) = self.repos.get_mut(&path) else {
                        continue;
                    };
                    if state.history_request != request {
                        continue;
                    }
                    state.history_busy = false;
                    match result {
                        Ok((history, graph)) => {
                            state.graph = graph;

                            state.history = Some(history);
                        }
                        Err(error) => state.history_error = Some(error),
                    }
                }
                Event::Status(path, request, result) => {
                    let Some(state) = self.repos.get_mut(&path) else {
                        continue;
                    };
                    if state.status_request != request {
                        continue;
                    }
                    state.status_busy = false;
                    state.status_checked = Some(Instant::now());
                    match result {
                        Ok(files) => {
                            state.changes = files;
                            state.has_status = true;
                        }
                        Err(error) => state.status_error = Some(error),
                    }
                }
                Event::Details(request, result) if request == self.details_request => {
                    self.details_busy = false;
                    match result {
                        Ok(details) => self.details = Some(details),
                        Err(error) => self.details_error = Some(error),
                    }
                }
                Event::Diff(request, result) if request == self.diff_request => {
                    self.diff_busy = false;
                    match result {
                        Ok(content) => self.content = Some(content),
                        Err(error) => self.diff_error = Some(error),
                    }
                }
                Event::Operation(path, committed_message, result) => {
                    self.operations.remove(&path);
                    if result.is_ok()
                        && let Some(message) = committed_message
                    {
                        let draft = if self.active.as_ref() == Some(&path) {
                            Some(&mut self.editor)
                        } else {
                            self.repos.get_mut(&path).map(|repo| &mut repo.draft)
                        };
                        if let Some(draft) = draft
                            && draft.trim() == message
                        {
                            draft.clear();
                        }
                    }
                    match result {
                        Ok(message) => self.tell(
                            if message.is_empty() {
                                "Git 操作已完成".into()
                            } else {
                                message
                            },
                            false,
                        ),
                        Err(error) => self.tell(error, true),
                    }
                    if self.active.as_ref() == Some(&path) {
                        self.choose_worktree();
                    }
                    self.request_history(ctx, &path);
                    self.request_status(ctx, &path, true);
                }
                Event::External(result) => match result {
                    Ok(dir) => {
                        self.external_dirs.push(dir);
                        self.tell("外部 diff 工具已启动", false);
                    }
                    Err(error) => self.tell(error, true),
                },
                _ => {}
            }
        }
    }

    pub(crate) fn is_capturing(&self) -> bool {
        self.frame_capture.is_some()
    }

    pub fn position_capture(&mut self, ctx: &egui::Context) {
        if let Some(capture) = &mut self.frame_capture {
            capture.adjust_scale(ctx);
            if let Some(query) = capture.branch_query.take() {
                self.branch_query = query;
            }
        }
        let Some(target) = self
            .frame_capture
            .as_mut()
            .and_then(|capture| capture.commit.take())
        else {
            return;
        };
        let row = self
            .state()
            .and_then(|repo| repo.history.as_ref())
            .and_then(|history| {
                history
                    .commits
                    .iter()
                    .position(|commit| commit.id.starts_with(&target))
            });
        if let Some(row) = row {
            self.scroll_to = Some(row.saturating_sub(4));
        } else if !self.state().is_some_and(|repo| repo.history.is_some())
            && let Some(capture) = &mut self.frame_capture
        {
            capture.commit = Some(target);
        }
    }

    pub fn report_frame(&mut self, ctx: &egui::Context) {
        let capture_ready = self
            .state()
            .is_some_and(|repo| repo.history.is_some() && repo.has_status);
        if capture_ready && let Some(capture) = &mut self.frame_capture {
            if capture.save(ctx) {
                self.frame_capture = None;
            } else {
                capture.queue(ctx);
            }
        }
        if let Some(timings) = &mut self.timings
            && let Some(path) = &self.active
            && let Some(repo) = self.repos.get(path)
        {
            timings.presented(
                path,
                repo.history.is_some(),
                repo.has_status && !repo.status_busy,
            );
        }
    }

    pub fn prefetch(&mut self, ctx: &egui::Context) {
        if !self.settings.prefetch_history
            || !self.state().is_some_and(|state| state.history.is_some())
            || self.repos.values().any(|repo| repo.history_busy)
        {
            return;
        }
        let candidate = self.settings.repositories.iter().find(|path| {
            self.active.as_ref() != Some(*path)
                && self
                    .repos
                    .get(*path)
                    .is_none_or(|repo| repo.history.is_none() && repo.history_error.is_none())
        });
        if let Some(path) = candidate.cloned() {
            self.request_history(ctx, &path);
        }
    }

    pub fn background_status(&mut self, ctx: &egui::Context) {
        let focused = ctx.input(|input| input.viewport().focused).unwrap_or(true);
        if focused
            && !self.focused
            && let Some(path) = self.active.clone()
        {
            self.request_status(ctx, &path, false);
        }
        self.focused = focused;
    }
}

fn same_path(left: &Path, right: &Path) -> bool {
    if cfg!(windows) {
        left.to_string_lossy()
            .replace('\\', "/")
            .eq_ignore_ascii_case(&right.to_string_lossy().replace('\\', "/"))
    } else {
        left == right
    }
}
