use crate::builtin_diff::{self, ActiveDiff};
use crate::git::{
    self, AfterSuccess, Area, Change, Commit, Compare, ConfirmedChange, CurrentHead, Details,
    History, HistoryScope, ImmediateChange, Operation, RecoveryState, Reference, RefreshScope,
    Status, StatusScope, Submodule, TargetedRead, TrackingState,
};
use crate::history::{HistoryCache, HistoryView, SearchDirection, SearchResult};
use crate::logging::{self, Presentation, ReadLog};
use crate::settings::{self, Settings, Tool};
use crate::tasks::{ReadKind, Scheduler};
use crate::theme::{self, Theme};
use crate::timing::{FrameCapture, Timings};
use anyhow::Result;
use crossbeam_channel::{Receiver, Sender, unbounded};
use eframe::egui;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tempfile::TempDir;

type Outcome<T> = std::result::Result<T, String>;
const STATUS_REFRESH_INTERVAL: Duration = Duration::from_secs(25);
const INDEX_STATUS_COALESCE: Duration = Duration::from_millis(250);

enum Work<'a> {
    Read,
    Latest(ReadKind, &'a Path, u64),
    Write,
}

enum Event {
    Added(Outcome<Vec<PathBuf>>, bool),
    History(PathBuf, u64, Result<Option<HistoryView>>),
    HistorySearch(u64, u64, Outcome<SearchResult>),
    Status {
        path: PathBuf,
        request: u64,
        files: Result<Status>,
        submodules: Option<Result<Vec<Submodule>>>,
        recovery: Option<Result<(git::WorktreeGitDir, Option<RecoveryState>)>>,
    },
    Details(u64, u64, Selection, Outcome<Details>),
    CopyCommitMessage(PathBuf, u64, Outcome<String>),
    Operation(PathBuf, u64, Operation, Result<(String, AfterSuccess)>),
    OperationReadback(PathBuf, u64, Result<Option<ConfirmedChange>>),
    RebaseMessage(PathBuf, u64, git::RebaseMessageRequest),
    External(Outcome<TempDir>),
}

#[derive(Default)]
pub(crate) struct Repository {
    pub history: Option<HistoryView>,
    history_overlay: Option<History>,
    pub status: Option<Status>,
    git_directory: Option<git::WorktreeGitDir>,
    pub recovery: Option<RecoveryState>,
    pub recovery_verified: bool,
    pub rebase_message: Option<RebaseMessageEdit>,
    submodules: Option<Vec<Submodule>>,
    pub history_busy: bool,
    pub status_busy: bool,
    pub status_error: Option<String>,
    pub history_error: Option<String>,
    pub filter: HistoryScope,
    history_viewport: Range<usize>,
    history_pending_window: Option<Range<usize>>,
    history_failed_window: Option<Range<usize>>,
    history_cancel: Option<Arc<AtomicBool>>,
    history_request: u64,
    history_on_demand: bool,
    draft: String,
    status_request: u64,
    status_read_scope: Option<StatusScope>,
    operation: OperationCompletion,
    status_checked: Option<Instant>,
    visible_history_request: Option<u64>,
    visible_status_request: Option<u64>,
    status_warning: Option<Instant>,
    submodule_warning: Option<Instant>,
    refresh_on_focus: bool,
}

#[derive(Default)]
struct OperationCompletion {
    request: u64,
    pending: RefreshScope,
    after_frame: Followup,
    feedback: Option<(String, bool)>,
}

#[derive(Default)]
enum Followup {
    #[default]
    Idle,
    Read(TargetedRead),
    Refresh(Instant),
}

impl OperationCompletion {
    fn owns(&self, request: u64) -> bool {
        self.request == request
    }

    fn begin(&mut self, request: u64, refresh: RefreshScope) -> RefreshScope {
        self.request = request;
        self.after_frame = Followup::Idle;
        self.pending.union(refresh)
    }

    fn finish(&mut self, refresh: RefreshScope, readback: Option<TargetedRead>, now: Instant) {
        self.pending = self.pending.union(refresh);
        self.after_frame = if self.pending.is_empty() {
            Followup::Idle
        } else {
            readback.map_or_else(
                || {
                    let delay = if self.pending == RefreshScope::FILES {
                        INDEX_STATUS_COALESCE
                    } else {
                        Duration::ZERO
                    };
                    Followup::Refresh(now + delay)
                },
                Followup::Read,
            )
        };
    }

    fn ready_followup(&mut self, now: Instant) -> Result<Followup, Duration> {
        if let Followup::Refresh(deadline) = self.after_frame
            && deadline > now
        {
            return Err(deadline - now);
        }
        Ok(std::mem::take(&mut self.after_frame))
    }
}

impl Repository {
    pub(crate) fn normal_writes_available(&self) -> bool {
        self.recovery_verified && self.recovery.is_none()
    }

    pub(crate) fn conflicts(&self) -> usize {
        self.status.as_ref().map_or(0, Status::conflict_count)
    }

    pub fn changes(&self) -> &[Change] {
        self.status.as_ref().map_or(&[], |status| &status.files)
    }

    pub fn history_info(&self) -> Option<&History> {
        self.history_overlay
            .as_ref()
            .or_else(|| self.history.as_ref().map(|history| &history.cache.info))
    }

    pub fn head_row(&self) -> Option<usize> {
        let history = self.history.as_ref()?;
        let head = &self.history_info()?.head;
        if head == &history.cache.info.head {
            return history.summary.head_row;
        }
        let head = head.as_ref()?;
        history
            .cached_rows()
            .find_map(|(index, row)| (head == &row.commit.id).then_some(index))
    }

    fn update_reference(&mut self, reference: Reference) {
        if let Some(history) = self.history.as_ref() {
            let overview = self
                .history_overlay
                .get_or_insert_with(|| history.cache.info.clone());
            set_reference(overview, reference);
        }
    }

    fn update_head(&mut self, head: CurrentHead, switch_branch: bool) {
        let Some(history) = self.history.as_ref() else {
            return;
        };
        let located = history
            .cached_rows()
            .any(|(_, row)| row.commit.id == head.id && row.commit.parents == head.parents);
        if !located && !switch_branch {
            return;
        }
        let overview = self
            .history_overlay
            .get_or_insert_with(|| history.cache.info.clone());
        overview.branch = head.branch().to_owned();
        if let Some(name) = head.reference {
            let mut reference = overview
                .refs
                .iter()
                .find(|reference| reference.name == name)
                .cloned()
                .unwrap_or(Reference {
                    name,
                    target: head.id.clone(),
                    upstream: None,
                    symbolic: false,
                });
            if reference.target != head.id {
                reference.target.clone_from(&head.id);
                if let Some(upstream) = &mut reference.upstream {
                    upstream.state = TrackingState::Unknown;
                }
            }
            set_reference(overview, reference);
        }
        overview.head = located.then_some(head.id);
    }

    fn invalidate_operation_reads(&mut self, request: u64, scope: RefreshScope) {
        if scope.status.is_some() {
            self.status_request = request;
            self.status_busy = true;
        }
        if scope.history {
            self.history_request = request;
            self.history_busy = true;
            self.history_pending_window = None;
            if let Some(cancel) = self.history_cancel.take() {
                cancel.store(true, Ordering::Relaxed);
            }
        }
    }

    fn focus_refresh_delay(&self, now: Instant) -> Option<Duration> {
        (self.refresh_on_focus && !self.status_busy).then(|| {
            self.status_checked.map_or(Duration::ZERO, |checked| {
                STATUS_REFRESH_INTERVAL.saturating_sub(now.saturating_duration_since(checked))
            })
        })
    }

    fn cached_head_for_file_refresh(
        &self,
        scope: StatusScope,
        now: Instant,
    ) -> Option<Option<String>> {
        self.status
            .as_ref()
            .filter(|_| {
                scope == StatusScope::Files
                    && self.submodules.is_some()
                    && self.submodule_warning.is_none()
                    && self.status_checked.is_some_and(|checked| {
                        now.saturating_duration_since(checked) < STATUS_REFRESH_INTERVAL
                    })
            })
            .map(|status| status.head.clone())
    }
}

fn needs_submodule_refresh(cached_head: Option<&Option<String>>, result: &Result<Status>) -> bool {
    cached_head.is_none_or(|head| {
        result
            .as_ref()
            .is_ok_and(|status| &status.head != head || status.has_submodule_changes())
    })
}

fn set_reference(history: &mut History, reference: Reference) {
    match history
        .refs
        .binary_search_by(|entry| entry.name.cmp(&reference.name))
    {
        Ok(index) => history.refs[index] = reference,
        Err(index) => history.refs.insert(index, reference),
    }
}

fn index_destination(source: Area) -> Area {
    if source == Area::Staged {
        Area::Unstaged
    } else {
        Area::Staged
    }
}

pub(crate) struct RepositoryRow {
    pub path: PathBuf,
    pub depth: usize,
    pub submodule: Option<bool>,
}

#[derive(Clone, Debug)]
pub(crate) enum Selection {
    Worktree,
    Commit {
        right: Box<Commit>,
        left: Option<Box<Commit>>,
        right_position: Option<(u64, usize)>,
    },
}

struct HistorySearch {
    request: u64,
    generation: u64,
    loaded: usize,
    query: String,
    canceled: Arc<AtomicBool>,
    result: Option<Outcome<usize>>,
}

pub(crate) enum Dialog {
    Operation {
        root: PathBuf,
        operation: Operation,
        title: &'static str,
        question: String,
    },
    NewBranch(String),
    NewCommitReference {
        root: PathBuf,
        commit: Box<Commit>,
        name: String,
        tag: bool,
    },
    CherryPickMerge {
        root: PathBuf,
        commit: Box<Commit>,
        mainline: Option<usize>,
    },
    AbortRebaseMessage {
        root: PathBuf,
        editor: u64,
    },
    Forget(PathBuf),
}

pub(crate) struct RebaseMessageEdit {
    pub id: u64,
    pub commit: Option<String>,
    pub draft: String,
    pub visible: bool,
    pub focus: bool,
    original: String,
    reply: Sender<git::RebaseMessageReply>,
}

pub(crate) struct Desk {
    #[cfg(windows)]
    pub window_chrome: Option<crate::window_chrome::WindowChrome>,
    pub settings: Settings,
    pub themes: Vec<Theme>,
    pub active_theme: usize,
    pub active: Option<PathBuf>,
    pub repos: HashMap<PathBuf, Repository>,
    restore_active: Option<PathBuf>,
    pub selection: Selection,
    pub details: Option<Details>,
    pub details_error: Option<String>,
    pub details_busy: bool,
    pub save_icon: egui::TextureHandle,
    pub file: Option<Change>,
    pub stage_selection: BTreeSet<String>,
    pub comparison: Option<Compare>,
    pub diff: Option<Box<dyn ActiveDiff>>,
    pub editor: String,
    pub search: String,
    pub branch_query: String,
    pub file_query: String,
    pub reset_file_scroll: bool,
    pub reveal_conflict: bool,
    pub search_row: Option<usize>,
    history_search: Option<HistorySearch>,
    pub scroll_to: Option<usize>,
    pub dialog: Option<Dialog>,
    pub tools_open: bool,
    pub import_message: String,
    pub status_message: String,
    pub status_is_error: bool,
    status_from_operation: bool,
    pub operations: HashSet<PathBuf>,
    pub force_text: bool,
    pub show_full_line: Option<String>,
    pub graph_reset: u64,
    pub focused: bool,
    writes_enabled: bool,
    requests: u64,
    details_request: u64,
    copy_message_request: u64,
    external_dirs: Vec<TempDir>,
    timings: Option<Timings>,
    presentation: Option<Presentation>,
    frame_capture: Option<FrameCapture>,
    tasks: Scheduler,
    events: Sender<Event>,
    inbox: Receiver<Event>,
}

fn readable<T>(result: Result<T>) -> Outcome<T> {
    result.map_err(|error| format!("{error:#}"))
}

impl Desk {
    pub fn new(
        creation: &eframe::CreationContext<'_>,
        paths: Vec<PathBuf>,
        import_tools: bool,
        started: Instant,
    ) -> Self {
        let ctx = &creation.egui_ctx;
        let restoring = Instant::now();
        let (events, inbox) = unbounded();
        let (mut settings, mut warning, writes_enabled) = match Settings::load() {
            Ok(settings) => (settings, None, true),
            Err(error) => {
                tracing::warn!(action = "load", "settings_failed");
                (Settings::default(), Some(error.to_string()), false)
            }
        };
        normalize_repositories(&mut settings);
        for path in paths {
            let path = match git::root(&path) {
                Ok(root) => root,
                Err(error) => {
                    tracing::warn!(origin = "command_line", "repository_add_failed");
                    warning
                        .get_or_insert_with(|| format!("无法打开 {}：{error:#}", path.display()));
                    continue;
                }
            };
            if !settings
                .repositories
                .iter()
                .any(|old| same_path(old, &path))
            {
                tracing::info!(
                    worktree = logging::worktree_id(&path),
                    origin = "command_line",
                    "repository_added"
                );
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
            .as_deref()
            .and_then(|active| {
                settings
                    .repositories
                    .iter()
                    .filter(|path| same_path(path, active) || is_descendant(active, path))
                    .min_by_key(|path| path.components().count())
                    .cloned()
            })
            .or_else(|| settings.repositories.first().cloned());
        let restore_active = settings.active.clone().filter(|active| {
            initial
                .as_ref()
                .is_some_and(|path| is_descendant(active, path))
        });
        tracing::debug!(
            repositories = settings.repositories.len(),
            elapsed_ms = logging::millis(restoring.elapsed()),
            "session_restored"
        );
        let themes_path = settings::themes_path();
        if let Err(error) = theme::install_examples(&themes_path) {
            warning.get_or_insert_with(|| format!("写入示例主题失败：{error}"));
        }
        let status_is_error = warning.is_some();
        let themes = Theme::load(&themes_path);
        let active_theme = themes
            .iter()
            .position(|theme| theme.id.eq_ignore_ascii_case(&settings.theme))
            .unwrap_or_default();
        let theme = &themes[active_theme];
        theme.apply(ctx);
        let mut desk = Self {
            #[cfg(windows)]
            window_chrome: crate::window_chrome::WindowChrome::install(creation, theme),
            settings,
            themes,
            active_theme,
            active: None,
            repos: HashMap::new(),
            restore_active,
            selection: Selection::Worktree,
            details: None,
            details_error: None,
            details_busy: false,
            save_icon: crate::ui::load_save_icon(ctx),
            file: None,
            stage_selection: BTreeSet::new(),
            comparison: None,
            diff: None,
            editor: String::new(),
            search: String::new(),
            branch_query: String::new(),
            file_query: String::new(),
            reset_file_scroll: false,
            reveal_conflict: false,
            search_row: None,
            history_search: None,
            scroll_to: None,
            dialog: None,
            tools_open: false,
            import_message,
            status_message: warning.unwrap_or_default(),
            status_is_error,
            status_from_operation: false,
            writes_enabled,
            operations: HashSet::new(),
            force_text: false,
            show_full_line: None,
            graph_reset: 0,
            focused: true,
            requests: 0,
            copy_message_request: 0,
            details_request: 0,
            external_dirs: Vec::new(),
            timings: Timings::open(started),
            presentation: None,
            frame_capture: FrameCapture::from_environment(),
            tasks: Scheduler::new(),
            events,
            inbox,
        };
        if let Some(path) = initial {
            desk.activate_path(ctx, path);
        }
        desk.tasks.read(Box::new(git::log_version));
        desk
    }

    fn next(&mut self) -> u64 {
        self.requests += 1;
        self.requests
    }

    fn submit(
        &self,
        ctx: &egui::Context,
        work: Work<'_>,
        task: impl FnOnce() -> Event + Send + 'static,
    ) {
        let events = self.events.clone();
        let ctx = ctx.clone();
        let task = Box::new(move || {
            let _ = events.send(task());
            ctx.request_repaint();
        });
        match work {
            Work::Read => self.tasks.read(task),
            Work::Latest(kind, path, request) => self.tasks.latest(kind, path, request, task),
            Work::Write => self.tasks.write(task),
        }
    }

    pub fn tell(&mut self, message: impl Into<String>, error: bool) {
        self.status_message = message.into();
        self.status_is_error = error;
        self.status_from_operation = false;
    }

    fn tell_operation(&mut self, path: &Path, message: impl Into<String>, error: bool) {
        let message = message.into();
        if self.active.as_deref() == Some(path) {
            self.tell(message.clone(), error);
            self.status_from_operation = true;
        }
        if let Some(state) = self.repos.get_mut(path) {
            state.operation.feedback = Some((message, error));
        }
    }

    pub fn save(&mut self) {
        if self.writes_enabled
            && let Err(error) = self.settings.save()
        {
            tracing::warn!(action = "save", "settings_failed");
            self.tell(format!("保存配置失败：{error:#}"), true);
        }
    }

    pub fn state(&self) -> Option<&Repository> {
        self.active.as_ref().and_then(|path| self.repos.get(path))
    }

    pub fn repository_rows(&self) -> Vec<RepositoryRow> {
        let mut attached = HashSet::new();
        let mut visited = HashSet::new();
        let mut registered: Vec<_> = self.settings.repositories.iter().collect();
        // Visit parents first so an old standalone entry can be attached even
        // if it was saved before its parent.
        registered.sort_by_key(|path| path.components().count());
        for root in registered {
            if attached.contains(root) {
                continue;
            }
            let mut pending = vec![root];
            while let Some(path) = pending.pop() {
                if visited.insert(path) {
                    for module in self.submodules(path) {
                        attached.insert(&module.path);
                        if module.available {
                            pending.push(&module.path);
                        }
                    }
                }
            }
        }
        let mut pending: Vec<_> = self
            .settings
            .repositories
            .iter()
            .rev()
            .filter(|path| !attached.contains(path))
            .map(|path| (0, path, None))
            .collect();
        let mut rows = Vec::new();
        visited.clear();
        while let Some((depth, path, submodule)) = pending.pop() {
            if visited.insert(path) {
                rows.push(RepositoryRow {
                    path: path.clone(),
                    depth,
                    submodule,
                });
                if submodule != Some(false) {
                    pending.extend(
                        self.submodules(path)
                            .iter()
                            .rev()
                            .map(|module| (depth + 1, &module.path, Some(module.available))),
                    );
                }
            }
        }
        rows
    }

    fn submodules(&self, path: &Path) -> &[Submodule] {
        self.repos
            .get(path)
            .and_then(|state| state.submodules.as_deref())
            .unwrap_or_default()
    }

    fn restore_repository(&mut self, ctx: &egui::Context, parent: &Path) {
        let Some(target) = self.restore_active.take() else {
            return;
        };
        let module = self.submodules(parent).iter().find(|module| {
            same_path(&target, &module.path) || is_descendant(&target, &module.path)
        });
        if let Some(path) = module
            .filter(|module| module.available)
            .map(|module| module.path.clone())
        {
            if !same_path(&target, &path) {
                self.restore_active = Some(target);
            }
            self.activate_path(ctx, path);
        } else if module.is_none()
            && let Some(path) = self
                .settings
                .repositories
                .iter()
                .find(|path| same_path(path, &target))
        {
            self.activate_path(ctx, path.clone());
        } else {
            self.settings.active = self.active.clone();
            self.save();
        }
    }

    pub fn choose_repository(&self, ctx: &egui::Context) {
        self.submit(ctx, Work::Read, || {
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

    pub fn discover_worktrees(&self, ctx: &egui::Context) {
        let Some(root) = self.active.clone() else {
            return;
        };
        self.submit(ctx, Work::Read, move || {
            Event::Added(readable(git::worktrees(&root)), false)
        });
    }

    pub fn activate(&mut self, ctx: &egui::Context, path: PathBuf) {
        if self.restore_active.take().is_some() && self.active.as_ref() == Some(&path) {
            self.settings.active = Some(path);
            self.save();
            return;
        }
        self.activate_path(ctx, path);
    }

    fn activate_path(&mut self, ctx: &egui::Context, path: PathBuf) {
        if self.active.as_ref() == Some(&path) {
            return;
        }
        if let Some(previous) = self.active.clone() {
            self.deactivate_history(&previous);
            self.tasks.cancel(&previous, Some(ReadKind::Details));
            self.tasks.cancel(&previous, Some(ReadKind::Diff));
        }
        self.tasks.set_active(Some(path.clone()));
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
        self.settings.active = Some(self.restore_active.clone().unwrap_or_else(|| path.clone()));
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
        if let Some((message, error)) = self.repos[&path].operation.feedback.clone() {
            self.tell(message, error);
            self.status_from_operation = true;
        } else if self.status_from_operation {
            self.tell("", false);
        }
        self.selection = Selection::Worktree;
        self.details_busy = false;
        self.details = None;
        self.details_error = None;
        self.details_request = self.next();
        self.clear_diff();
        self.search.clear();
        self.branch_query.clear();
        self.file_query.clear();
        self.stage_selection.clear();
        self.cancel_history_search();
        self.search_row = None;
        self.scroll_to = None;
        self.graph_reset += 1;
        self.editor =
            std::mem::take(&mut self.repos.get_mut(&path).expect("active repository").draft);

        let activation = self.next();
        let state = self.repos.get_mut(&path).expect("active repository");
        self.presentation = Some(Presentation::activate(
            &path,
            activation,
            state.history.is_some(),
            state.status.is_some(),
        ));
        state.history_viewport = 0..2;
        if let Some(edit) = &mut state.rebase_message {
            edit.focus = true;
        }
        if let Some(history) = &state.history {
            history.cache.set_active(true);
        } else if !state.history_busy {
            self.request_history(ctx, &path);
        }
        self.request_status(ctx, &path, true, StatusScope::Files);
        self.save();
    }

    pub fn forget(&mut self, ctx: &egui::Context, path: &Path) {
        if self.operations.contains(path) {
            return;
        }
        self.settings.repositories.retain(|entry| entry != path);
        self.tasks.cancel(path, None);
        self.deactivate_history(path);
        if let Some(state) = self.repos.remove(path)
            && let Some(cancel) = state.history_cancel
        {
            cancel.store(true, Ordering::Relaxed);
        }
        self.settings.branch_selection.remove(path);
        if self.active.as_ref().is_some_and(|active| {
            active == path || !self.repository_rows().iter().any(|row| &row.path == active)
        }) {
            self.tasks.set_active(None);
            if let Some(active) = self.active.clone() {
                self.deactivate_history(&active);
                self.tasks.cancel(&active, Some(ReadKind::Details));
                self.tasks.cancel(&active, Some(ReadKind::Diff));
            }
            self.cancel_history_search();
            self.active = None;
            self.presentation = None;
            self.restore_active = None;
            self.settings.active = None;
            self.selection = Selection::Worktree;
            self.stage_selection.clear();
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
        self.queue_history(ctx, path, "request", false, None);
    }

    fn deactivate_history(&mut self, path: &Path) {
        if self.active.as_deref() == Some(path)
            && let Some(presentation) = &mut self.presentation
        {
            presentation.deactivate();
        }
        self.tasks.cancel(path, Some(ReadKind::HistorySearch));
        if let Some(history) = self
            .repos
            .get_mut(path)
            .and_then(|state| state.history.as_mut())
        {
            history.pages.clear();
            let cache = Arc::clone(&history.cache);
            cache.set_active(false);
            self.tasks.read(Box::new(move || cache.pause_if_inactive()));
        }
    }

    fn queue_history(
        &mut self,
        ctx: &egui::Context,
        path: &Path,
        source: &'static str,
        check_superproject: bool,
        filter_log: Option<(&'static str, usize)>,
    ) {
        let active = self.active.as_deref() == Some(path);
        if active {
            self.cancel_history_search();
            self.search_row = None;
        }
        let request = self.next();
        if active && let Some(presentation) = &mut self.presentation {
            let previous =
                presentation.change_history_filter(request, filter_log, self.timings.is_some());
            if let Some(timings) = &mut self.timings {
                if let Some(previous) = previous {
                    timings.history_filter_finished("history-filter-superseded", path, previous);
                }
                if let Some((_, checked_refs)) = filter_log {
                    timings.history_filter_requested(path, request, checked_refs);
                }
            }
        }
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
        if state.history_viewport.is_empty() {
            state.history_viewport = 0..2;
        }
        let canceled = Arc::new(AtomicBool::new(false));
        if let Some(previous) = state.history_cancel.replace(Arc::clone(&canceled)) {
            previous.store(true, Ordering::Relaxed);
        }
        if let Some(history) = &state.history {
            let cache = Arc::clone(&history.cache);
            cache.set_active(false);
            self.tasks.read(Box::new(move || cache.pause_if_inactive()));
        }
        state.history_pending_window = None;
        state.history_failed_window = None;
        state.history_request = request;
        state.history_busy = true;
        state.history_error = None;
        let root = path.to_owned();
        let filter = state.filter.clone();
        let range = state.history_viewport.clone();
        let log = ReadLog::queue("history", path, request);
        self.submit(
            ctx,
            Work::Latest(ReadKind::History, path, request),
            move || {
                let started = Instant::now();
                let result = (|| {
                    if check_superproject && git::has_superproject(&root)? {
                        return Ok(None);
                    }
                    let cache = HistoryCache::open(&root, &filter, active)?;
                    let summary =
                        cache.load_through(range.end.saturating_sub(1).max(1), &canceled)?;
                    let pages = if active {
                        cache.window(range, summary, Default::default())?
                    } else {
                        Default::default()
                    };
                    tracing::debug!(
                        worktree = log.worktree,
                        request,
                        source,
                        active,
                        commits = summary.loaded,
                        page_size = git::PAGE,
                        pages = pages.len(),
                        has_more = summary.has_more,
                        "history_built"
                    );
                    Ok(Some(HistoryView {
                        cache,
                        summary,
                        pages,
                        generation: request,
                    }))
                })();
                let outcome = match &result {
                    Ok(Some(_)) => "ok",
                    Ok(None) => "on_demand",
                    Err(_) if canceled.load(Ordering::Relaxed) => "canceled",
                    Err(_) => "failed",
                };
                log.complete(started, outcome);
                Event::History(root, request, result)
            },
        );
    }

    fn request_status(
        &mut self,
        ctx: &egui::Context,
        path: &Path,
        force: bool,
        scope: StatusScope,
    ) {
        let request = self.next();
        let state = self.repos.entry(path.to_owned()).or_default();
        let now = Instant::now();
        if state.operation.pending.status.is_some()
            && !matches!(state.operation.after_frame, Followup::Idle)
        {
            if force && scope == StatusScope::Full {
                state.operation.pending.status = Some(StatusScope::Full);
                if let Followup::Refresh(deadline) = &mut state.operation.after_frame {
                    *deadline = now;
                    ctx.request_repaint();
                }
            }
            return;
        }
        if state.status_busy && !force {
            return;
        }
        if !force
            && state
                .status_checked
                .is_some_and(|checked| checked.elapsed() < STATUS_REFRESH_INTERVAL)
        {
            return;
        }
        state.status_request = request;
        state.status_busy = true;
        state.status_error = None;
        let cached_head = state.cached_head_for_file_refresh(scope, now);
        let cached_git_dir = state.git_directory.clone();
        state.status_read_scope = Some(if cached_head.is_some() {
            StatusScope::Files
        } else {
            StatusScope::Full
        });
        if cached_head.is_none() {
            state.refresh_on_focus = false;
        }
        let root = path.to_owned();
        let log = ReadLog::queue("status", path, request);
        self.submit(
            ctx,
            Work::Latest(ReadKind::Status, path, request),
            move || {
                let started = Instant::now();
                let result = git::status(&root);
                let submodules = needs_submodule_refresh(cached_head.as_ref(), &result)
                    .then(|| git::submodules(&root));
                let outcome = if result.is_ok() && submodules.as_ref().is_none_or(Result::is_ok) {
                    "ok"
                } else {
                    "failed"
                };
                log.complete(started, outcome);
                let recovery = result.as_ref().ok().map(|status| {
                    let dir = match cached_git_dir.filter(|dir| dir.path.is_dir()) {
                        Some(dir) => dir,
                        None => git::worktree_git_dir(&root)?,
                    };
                    let recovery = git::recovery_state(&root, &dir, status.head.as_deref())?;
                    Ok((dir, recovery))
                });
                Event::Status {
                    path: root,
                    request,
                    files: result,
                    submodules,
                    recovery,
                }
            },
        );
    }

    pub(crate) fn request_operation_followups(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        let pending: Vec<_> = self
            .repos
            .iter_mut()
            .filter(|(path, _)| !self.operations.contains(*path))
            .filter_map(|(path, state)| {
                let step = match state.operation.ready_followup(now) {
                    Ok(step) => step,
                    Err(delay) => {
                        ctx.request_repaint_after(delay);
                        return None;
                    }
                };
                (!matches!(step, Followup::Idle)).then(|| {
                    (
                        path.clone(),
                        state.operation.request,
                        state.operation.pending,
                        step,
                    )
                })
            })
            .collect();
        for (path, request, scope, step) in pending {
            match step {
                Followup::Read(probe) => {
                    let root = path.clone();
                    let log = ReadLog::queue("operation_readback", &path, request);
                    self.submit(
                        ctx,
                        Work::Latest(ReadKind::OperationReadback, &path, request),
                        move || {
                            let started = Instant::now();
                            let result = probe.read(&root);
                            log.complete(started, if result.is_ok() { "ok" } else { "failed" });
                            Event::OperationReadback(root, request, result)
                        },
                    );
                }
                Followup::Refresh(_) => {
                    if scope.history {
                        self.request_history(ctx, &path);
                    }
                    if let Some(status) = scope.status {
                        self.request_status(ctx, &path, true, status);
                    }
                }
                Followup::Idle => {}
            }
        }
    }

    pub fn refresh(&mut self, ctx: &egui::Context) {
        if let Some(path) = self.active.clone() {
            self.request_history(ctx, &path);
            self.request_status(ctx, &path, true, StatusScope::Full);
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
        let filter_log = match &filter {
            HistoryScope::Head => ("head", 1),
            HistoryScope::All => ("all", 0),
            HistoryScope::Selected(refs) => ("selected", refs.len()),
        };
        self.settings.branch_selection.insert(path.clone(), filter);
        state.history_viewport = 0..2;
        self.graph_reset += 1;
        self.search_row = None;
        self.queue_history(ctx, &path, "filter", false, Some(filter_log));
        self.save();
    }

    pub fn request_history_window(
        &mut self,
        ctx: &egui::Context,
        range: Range<usize>,
        near_end: bool,
    ) {
        let Some(path) = self.active.clone() else {
            return;
        };
        let request = self.requests + 1;
        let state = self.repos.get_mut(&path).expect("active repository");
        state.history_viewport = range.clone();
        let Some(history) = &mut state.history else {
            return;
        };
        history.pages.retain(|page, _| range.contains(page));
        let summary = history.summary;
        let extend = near_end && summary.has_more && state.history_error.is_none();
        let missing = (range.start..range.end.min(summary.page_count()))
            .any(|page| !history.pages.contains_key(&page));
        if state.history_busy
            || state.history_failed_window.as_ref() == Some(&range)
            || !(extend || missing)
        {
            return;
        }
        self.requests = request;
        state.history_request = request;
        state.history_busy = true;
        state.history_pending_window = Some(range.clone());
        let canceled = Arc::new(AtomicBool::new(false));
        state.history_cancel = Some(Arc::clone(&canceled));
        let cache = Arc::clone(&history.cache);
        let pages = history.pages.clone();
        let generation = history.generation;
        let root = path.clone();
        let log = ReadLog::queue("history", &path, request);
        self.submit(
            ctx,
            Work::Latest(ReadKind::History, &path, request),
            move || {
                let started = Instant::now();
                let source = if extend { "more" } else { "cache_window" };
                let loaded_before = summary.loaded;
                let retained_pages = pages.len();
                let page_start = range.start;
                let page_end = range.end;
                let result = (|| {
                    let summary = if extend {
                        cache.load_through(summary.page_count() + 1, &canceled)?
                    } else {
                        summary
                    };
                    let pages = cache.window(range, summary, pages)?;
                    tracing::debug!(
                        worktree = log.worktree,
                        request,
                        generation,
                        source,
                        page_start,
                        page_end,
                        commits = summary.loaded,
                        commits_added = summary.loaded.saturating_sub(loaded_before),
                        pages = pages.len(),
                        cache_pages_read = pages.len().saturating_sub(retained_pages),
                        has_more = summary.has_more,
                        "history_window_loaded"
                    );
                    Ok(Some(HistoryView {
                        cache,
                        summary,
                        pages,
                        generation,
                    }))
                })();
                let outcome = if result.is_ok() {
                    "ok"
                } else if canceled.load(Ordering::Relaxed) {
                    "canceled"
                } else {
                    tracing::debug!(
                        worktree = log.worktree,
                        request,
                        generation,
                        source,
                        page_start,
                        page_end,
                        "history_window_failed"
                    );
                    "failed"
                };
                log.complete(started, outcome);
                Event::History(root, request, result)
            },
        );
        ctx.request_repaint();
    }

    fn cancel_history_search(&mut self) {
        if let Some(search) = self.history_search.take() {
            search.canceled.store(true, Ordering::Relaxed);
        }
    }

    pub fn request_history_search(
        &mut self,
        ctx: &egui::Context,
        query: &str,
        direction: Option<SearchDirection>,
    ) {
        if query.is_empty() {
            self.cancel_history_search();
            return;
        }
        let Some(path) = self.active.clone() else {
            return;
        };
        let state = &self.repos[&path];
        let Some(history) = &state.history else {
            return;
        };
        if state.history_busy && state.history_pending_window.is_none() {
            return;
        }
        let generation = history.generation;
        let loaded = history.summary.loaded;
        if direction.is_none()
            && self.history_search.as_ref().is_some_and(|search| {
                search.query == query
                    && search.generation == generation
                    && (search.loaded == loaded || search.result.is_none())
            })
        {
            return;
        }
        let cache = Arc::clone(&history.cache);
        self.cancel_history_search();
        let request = self.next();
        let canceled = Arc::new(AtomicBool::new(false));
        self.history_search = Some(HistorySearch {
            request,
            generation,
            loaded,
            query: query.into(),
            canceled: Arc::clone(&canceled),
            result: None,
        });
        let anchor = self.search_row;
        let query = query.to_owned();
        let log = ReadLog::queue("history_search", &path, request);
        self.submit(
            ctx,
            Work::Latest(ReadKind::HistorySearch, &path, request),
            move || {
                let started = Instant::now();
                let result = cache.search(&query, loaded, anchor, direction, &canceled);
                let outcome = match &result {
                    Ok(found) => {
                        tracing::debug!(
                            worktree = log.worktree,
                            request,
                            generation,
                            commits = loaded,
                            matches = found.count,
                            located = found.target.is_some(),
                            "history_search_finished"
                        );
                        "ok"
                    }
                    Err(_) if canceled.load(Ordering::Relaxed) => "canceled",
                    Err(error) => {
                        let (error_kind, exit_code) = git::error_kind(error);
                        tracing::warn!(
                            kind = "history_search",
                            worktree = log.worktree,
                            request,
                            generation,
                            error_kind,
                            exit_code,
                            "read_failed"
                        );
                        "failed"
                    }
                };
                log.complete(started, outcome);
                Event::HistorySearch(log.worktree, request, readable(result))
            },
        );
    }

    pub fn history_search_result(&self) -> Option<&Outcome<usize>> {
        self.history_search.as_ref()?.result.as_ref()
    }

    pub fn choose_worktree(&mut self) {
        if let Some(active) = &self.active {
            self.tasks.cancel(active, Some(ReadKind::Details));
        }
        self.selection = Selection::Worktree;
        self.stage_selection.clear();
        self.details_request = self.next();
        self.details_busy = false;
        self.details_error = None;
        self.details = None;
        self.clear_diff();
    }

    pub fn choose_commit(&mut self, ctx: &egui::Context, row: usize, item: Commit, extend: bool) {
        let Some(root) = self.active.clone() else {
            return;
        };
        let Some(history) = &self.repos[&root].history else {
            return;
        };
        if let Selection::Commit {
            right, left: None, ..
        } = &self.selection
            && right.id == item.id
            && (self.details_busy || self.details.is_some())
        {
            return;
        }
        let generation = history.generation;
        let loaded = history.summary.loaded;
        let cache = Arc::clone(&history.cache);
        let current = if extend
            && let Selection::Commit {
                right,
                right_position,
                ..
            } = &self.selection
            && right.id != item.id
        {
            Some((right.clone(), *right_position))
        } else {
            None
        };
        let item = Box::new(item);
        self.selection = Selection::Commit {
            right: item.clone(),
            left: current.as_ref().map(|(commit, _)| commit.clone()),
            right_position: Some((generation, row)),
        };
        self.stage_selection.clear();
        self.details_request = self.next();
        let request = self.details_request;
        self.details = None;
        self.details_error = None;
        self.details_busy = true;
        self.clear_diff();
        let path = root.clone();
        let log = ReadLog::queue("details", &path, request);
        self.submit(
            ctx,
            Work::Latest(ReadKind::Details, &path, request),
            move || {
                let started = Instant::now();
                let (left, right, right_position) = if let Some((commit, position)) = current {
                    let current_row = match position {
                        Some((snapshot, index)) if snapshot == generation => Some(index),
                        _ => match cache.position(loaded, &commit.id) {
                            Ok(index) => index,
                            Err(error) => {
                                let selection = Selection::Commit {
                                    right: item,
                                    left: Some(commit),
                                    right_position: Some((generation, row)),
                                };
                                log.complete(started, "failed");
                                return Event::Details(
                                    log.worktree,
                                    request,
                                    selection,
                                    Err(format!("{error:#}")),
                                );
                            }
                        },
                    };
                    if current_row.is_some_and(|index| row < index) {
                        (Some(commit), item, Some((generation, row)))
                    } else {
                        (
                            Some(item),
                            commit,
                            current_row.map(|index| (generation, index)),
                        )
                    }
                } else {
                    (None, item, Some((generation, row)))
                };
                let result = readable(git::details(&root, &right, left.as_deref()));
                log.complete(started, if result.is_ok() { "ok" } else { "failed" });
                Event::Details(
                    log.worktree,
                    request,
                    Selection::Commit {
                        right,
                        left,
                        right_position,
                    },
                    result,
                )
            },
        );
    }

    pub fn choose_file(&mut self, ctx: &egui::Context, file: Change) {
        self.force_text = false;
        let head = self
            .state()
            .and_then(|state| state.status.as_ref())
            .and_then(|status| status.head.as_deref());
        let pair = git::compare(&file, self.details.as_ref(), head);
        self.file = Some(file);
        self.comparison = Some(pair);
        self.request_diff(ctx);
    }

    fn reconcile_worktree_file(
        &mut self,
        ctx: &egui::Context,
        moved_to: Option<Area>,
        head_changed: bool,
    ) {
        if !matches!(self.selection, Selection::Worktree) {
            return;
        }
        let (Some(file), Some(state)) = (self.file.as_ref(), self.state()) else {
            return;
        };
        let preferred = moved_to.unwrap_or(file.area);
        let changes = state.changes();
        let next = changes
            .iter()
            .find(|change| change.path == file.path && change.area == preferred)
            .or_else(|| changes.iter().find(|change| change.path == file.path))
            .or_else(|| {
                changes.iter().find(|change| {
                    change.old_path.as_deref() == Some(file.path.as_str())
                        || file.old_path.as_deref() == Some(change.path.as_str())
                })
            });
        match next {
            Some(next) if moved_to.is_some() || head_changed || next != file => {
                self.choose_file(ctx, next.clone());
            }
            None => self.clear_diff(),
            Some(_) => {}
        }
    }

    pub fn request_diff(&mut self, ctx: &egui::Context) {
        let (Some(root), Some(pair)) = (self.active.clone(), self.comparison.clone()) else {
            return;
        };
        self.diff = None;
        self.show_full_line = None;
        let request = self.next();
        let registered = builtin_diff::registered(&pair);
        let format = if self.force_text {
            builtin_diff::text()
        } else {
            registered
        };
        let log = ReadLog::queue("diff", &root, request);
        let (session, job) = format.start(builtin_diff::Request {
            root: root.clone(),
            pair,
            log,
            ctx: ctx.clone(),
        });
        self.diff = Some(session);
        self.tasks.latest(ReadKind::Diff, &root, request, job);
        ctx.request_repaint();
    }

    pub fn clear_diff(&mut self) {
        self.diff = None;
        if let Some(active) = &self.active {
            self.tasks.cancel(active, Some(ReadKind::Diff));
        }
        self.file = None;
        self.comparison = None;
        self.show_full_line = None;
    }

    pub fn copy_commit_id(&mut self, ctx: &egui::Context, id: String) {
        self.copy_message_request = self.next();
        ctx.copy_text(id);
    }

    pub fn copy_commit_message(&mut self, ctx: &egui::Context, id: String) {
        let Some(root) = self.active.clone() else {
            return;
        };
        let request = self.next();
        self.copy_message_request = request;
        self.submit(ctx, Work::Read, move || {
            let result = readable(git::commit_message(&root, &id));
            Event::CopyCommitMessage(root, request, result)
        });
    }

    pub fn operate(&mut self, ctx: &egui::Context, root: PathBuf, operation: Operation) {
        let allowed = self.repos.get(&root).is_some_and(|state| match &operation {
            Operation::Recover { expected, action } => {
                self.active.as_ref() == Some(&root)
                    && state.recovery_verified
                    && !state.status_busy
                    && state.recovery.as_ref() == Some(expected.as_ref())
                    && expected.allows(*action, state.conflicts())
            }
            _ => !operation.requires_idle_worktree() || state.normal_writes_available(),
        });
        if !allowed {
            self.tell_operation(&root, "Git 操作状态待确认，请查看刷新后的工作树", true);
            self.request_status(ctx, &root, true, StatusScope::Full);
            return;
        }
        if !self.operations.insert(root.clone()) {
            return;
        }
        let request = self.next();
        let state = self.repos.entry(root.clone()).or_default();
        let mut after = operation.after_success(state.status.as_ref());
        if after.refresh.status.is_some()
            && state.status_busy
            && state.status_request != state.operation.request
        {
            state.operation.pending.status =
                state.operation.pending.status.max(state.status_read_scope);
        }
        if after.refresh.history && state.history_busy && state.history_pending_window.is_none() {
            state.operation.pending.history = true;
        }
        let invalidated = state.operation.begin(request, after.refresh);
        state.invalidate_operation_reads(request, invalidated);
        self.tasks.cancel(&root, Some(ReadKind::OperationReadback));
        let worktree = logging::worktree_id(&root);
        let kind = operation.kind();
        let queued = Instant::now();
        tracing::info!(kind, worktree, request, "git_operation_requested");
        self.tell_operation(&root, "正在运行 Git…", false);
        let events = self.events.clone();
        let repaint = ctx.clone();
        self.submit(ctx, Work::Write, move || {
            let started = Instant::now();
            let queue_ms = logging::millis(started.duration_since(queued));
            tracing::debug!(kind, worktree, request, queue_ms, "git_operation_started");
            if let Some(probe) = &mut after.readback {
                probe.prepare(&root);
            }
            let result = if matches!(&operation, Operation::Recover { expected, action } if expected.kind == git::RecoveryKind::Rebase && *action != git::RecoveryAction::Abort) {
                operation.execute_with_editor(&root, Some(&mut |edit| {
                    let _ = events.send(Event::RebaseMessage(root.clone(), request, edit));
                    repaint.request_repaint();
                }))
            } else {
                operation.execute(&root)
            };
            let run_ms = logging::millis(started.elapsed());
            match &result {
                Ok(_) => tracing::info!(
                    kind,
                    worktree,
                    request,
                    queue_ms,
                    run_ms,
                    exit_code = 0,
                    "git_operation_succeeded"
                ),
                Err(error) => {
                    let (error_kind, exit_code) = git::error_kind(error);
                    tracing::warn!(
                        kind,
                        worktree,
                        request,
                        queue_ms,
                        run_ms,
                        error_kind,
                        exit_code,
                        "git_operation_failed"
                    );
                }
            }
            Event::Operation(
                root,
                request,
                operation,
                result.map(|message| (message, after)),
            )
        });
    }

    fn finish_operation(
        &mut self,
        ctx: &egui::Context,
        path: &Path,
        request: u64,
        operation: Operation,
        result: Result<(String, AfterSuccess)>,
    ) {
        let Some(state) = self
            .repos
            .get_mut(path)
            .filter(|state| state.operation.owns(request))
        else {
            return;
        };
        state.rebase_message = None;
        self.operations.remove(path);
        let active = self.active.as_deref() == Some(path);
        match result {
            Ok((message, after)) => {
                if let Operation::Commit(message) = &operation {
                    let draft = if active {
                        &mut self.editor
                    } else {
                        &mut state.draft
                    };
                    if draft.trim() == message {
                        draft.clear();
                    }
                }
                let mut moved = None;
                match after.immediate {
                    ImmediateChange::None => {}
                    ImmediateChange::Reference(reference) => state.update_reference(reference),
                    ImmediateChange::Index {
                        source,
                        unconfirmed,
                    } => {
                        if let Some(status) = &mut state.status {
                            status.apply_index_operation(&operation, source, &unconfirmed);
                        }
                        if active
                            && self.file.as_ref().is_some_and(|file| {
                                operation.affects_index_path(&file.path)
                                    && !unconfirmed.contains(&file.path)
                            })
                        {
                            moved = Some(index_destination(source));
                        }
                    }
                }
                state
                    .operation
                    .finish(after.refresh, after.readback, Instant::now());
                state.invalidate_operation_reads(request, state.operation.pending);
                if let Some(destination) = moved {
                    self.reconcile_worktree_file(ctx, Some(destination), false);
                }
                let message = if message.is_empty() {
                    "Git 操作已完成".into()
                } else {
                    message
                };
                self.tell_operation(path, message, false);
            }
            Err(error) => {
                if state.history_request == request {
                    state.history_busy = false;
                }
                if state.status_request == request {
                    state.status_busy = false;
                }
                state
                    .operation
                    .finish(operation.refresh_after_failure(), None, Instant::now());
                state.invalidate_operation_reads(request, state.operation.pending);
                let message = if matches!(operation, Operation::Stage(_)) {
                    format!("暂存失败 · {error:#}")
                } else {
                    format!("{error:#}")
                };
                self.tell_operation(path, message, true);
            }
        }
    }

    pub(crate) fn show_rebase_message(&mut self) {
        if let Some(edit) = self
            .active
            .as_ref()
            .and_then(|root| self.repos.get_mut(root))
            .and_then(|state| state.rebase_message.as_mut())
        {
            edit.visible = true;
            edit.focus = true;
        }
    }

    pub(crate) fn respond_to_rebase_message(&mut self, save: bool) {
        let Some(root) = self.active.clone() else {
            return;
        };
        let Some(state) = self.repos.get_mut(&root) else {
            return;
        };
        if save {
            if let Some(edit) = state.rebase_message.take() {
                if edit
                    .reply
                    .send(git::RebaseMessageReply::Save(edit.draft))
                    .is_ok()
                {
                    self.tell_operation(&root, "正在运行 Git…", false);
                } else {
                    self.tell_operation(&root, "Git 已结束这次编辑请求", true);
                }
            }
        } else if let Some(edit) = &mut state.rebase_message {
            edit.draft.clone_from(&edit.original);
            edit.visible = false;
        }
    }

    pub(crate) fn abort_rebase_message(&mut self, root: &Path, id: u64) {
        let edit = self.repos.get_mut(root).and_then(|state| {
            if state
                .rebase_message
                .as_ref()
                .is_some_and(|edit| edit.id == id)
            {
                state.rebase_message.take()
            } else {
                None
            }
        });
        if edit.is_some_and(|edit| edit.reply.send(git::RebaseMessageReply::Abort).is_ok()) {
            self.tell_operation(root, "正在中止变基…", false);
        } else {
            self.tell_operation(root, "Git 已结束这次编辑请求", true);
        }
    }

    fn finish_operation_readback(
        &mut self,
        ctx: &egui::Context,
        path: &Path,
        request: u64,
        result: Result<Option<ConfirmedChange>>,
    ) {
        let Some(state) = self
            .repos
            .get_mut(path)
            .filter(|state| state.operation.owns(request))
        else {
            tracing::debug!(
                kind = "operation_readback",
                worktree = logging::worktree_id(path),
                request,
                "read_stale"
            );
            return;
        };
        state
            .operation
            .finish(RefreshScope::default(), None, Instant::now());
        let active = self.active.as_deref() == Some(path);
        let mut reconcile_file = false;
        let mut moved = None;
        match result {
            Ok(Some(change)) => {
                match change {
                    ConfirmedChange::Head {
                        head,
                        switch_branch,
                    } if state.history_request == request => {
                        state.update_head(head, switch_branch);
                    }
                    ConfirmedChange::Index {
                        source,
                        paths,
                        status,
                    } if state.status_request == request => {
                        if let Some(cached) = &mut state.status
                            && cached.head == status.head
                        {
                            cached.replace_index_paths(&paths, status);
                            if self
                                .file
                                .as_ref()
                                .is_some_and(|file| paths.contains(&file.path))
                            {
                                moved = Some(index_destination(source));
                            }
                            reconcile_file = true;
                        }
                    }
                    ConfirmedChange::Discard { paths, status }
                        if state.status_request == request =>
                    {
                        if let Some(cached) = &mut state.status
                            && cached.head == status.head
                        {
                            cached.remove_restored_files(paths, &status);
                            reconcile_file = true;
                        }
                    }
                    _ => {}
                }
                logging::read_applied("operation_readback", path, request);
            }
            Ok(None) => {}
            Err(error) => logging::read_failed("operation_readback", path, request, &error),
        }
        if active && reconcile_file {
            self.reconcile_worktree_file(ctx, moved, false);
        }
    }

    pub fn external(&mut self, ctx: &egui::Context, tool: Tool) {
        let (Some(root), Some(pair)) = (self.active.clone(), self.comparison.clone()) else {
            return;
        };
        let request = self.next();
        let worktree = logging::worktree_id(&root);
        let queued = Instant::now();
        self.tell(format!("正在准备 {} 的两个版本…", tool.name), false);
        self.submit(ctx, Work::Read, move || {
            let started = Instant::now();
            let mut stage = "prepare";
            let result = (|| {
                let launch = settings::prepare_external(&root, &pair, &tool)?;
                stage = "launch";
                for invocation in &launch.commands {
                    let mut process = settings::start_external(&root, invocation)?;
                    // Reap tools independently. A detached GUI may read its snapshot later.
                    std::thread::spawn(move || {
                        let _ = process.wait();
                    });
                }
                Ok(launch.dir)
            })();
            let queue_ms = logging::millis(started.duration_since(queued));
            let run_ms = logging::millis(started.elapsed());
            if result.is_ok() {
                tracing::debug!(worktree, request, queue_ms, run_ms, "external_diff_started");
            } else {
                tracing::warn!(
                    worktree,
                    request,
                    stage,
                    queue_ms,
                    run_ms,
                    "external_diff_failed"
                );
            }
            Event::External(readable(result))
        });
    }

    pub fn events(&mut self, ctx: &egui::Context) {
        if let Some(diff) = &mut self.diff {
            diff.poll(ctx);
        }
        while let Ok(event) = self.inbox.try_recv() {
            match event {
                Event::Added(result, activate) => match result {
                    Ok(mut paths) => {
                        let rows = self.repository_rows();
                        for path in &mut paths {
                            if let Some(row) = rows.iter().find(|row| same_path(&row.path, path)) {
                                *path = row.path.clone();
                            }
                        }
                        let first = paths.first().cloned();
                        let mut added = 0;
                        for path in paths {
                            if !self
                                .settings
                                .repositories
                                .iter()
                                .any(|old| same_path(&path, old))
                            {
                                tracing::info!(
                                    worktree = logging::worktree_id(&path),
                                    origin = if activate { "picker" } else { "discovery" },
                                    "repository_added"
                                );
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
                    Err(error) => {
                        tracing::warn!(
                            origin = if activate { "picker" } else { "discovery" },
                            "repository_add_failed"
                        );
                        self.tell(error, true);
                    }
                },
                Event::History(path, request, result) => {
                    let Some(state) = self
                        .repos
                        .get_mut(&path)
                        .filter(|state| state.history_request == request)
                    else {
                        tracing::debug!(
                            kind = "history",
                            worktree = logging::worktree_id(&path),
                            request,
                            "read_stale"
                        );
                        continue;
                    };
                    logging::read_applied("history", &path, request);
                    state.history_busy = false;
                    state.history_cancel = None;
                    let pending = state.history_pending_window.take();
                    if pending.is_none() {
                        state.operation.pending.history = false;
                    }
                    match result {
                        Ok(Some(mut history)) => {
                            if history.generation == request {
                                state.history_overlay = None;
                            }
                            let active = self.active.as_ref() == Some(&path);
                            history.cache.set_active(active);
                            if active {
                                history
                                    .pages
                                    .retain(|page, _| state.history_viewport.contains(page));
                            } else {
                                history.pages.clear();
                                let cache = Arc::clone(&history.cache);
                                self.tasks.read(Box::new(move || cache.pause_if_inactive()));
                            }
                            state.history = Some(history);
                            state.visible_history_request = Some(request);
                        }
                        Ok(None) => {
                            state.history_on_demand = true;
                            if self.active.as_ref() == Some(&path) {
                                self.request_history(ctx, &path);
                            }
                        }
                        Err(error) => {
                            logging::read_failed("history", &path, request, &error);
                            if self.active.as_ref() == Some(&path)
                                && let Some(presentation) = &mut self.presentation
                            {
                                presentation.history_filter_failed(request);
                            }
                            state.history_error = Some(format!("{error:#}"));
                            state.history_failed_window = pending;
                        }
                    }
                }
                Event::HistorySearch(worktree, request, result) => {
                    if let Some(search) = &mut self.history_search
                        && search.request == request
                    {
                        search.result = Some(result.map(|result| {
                            if let Some(row) = result.target {
                                self.search_row = Some(row);
                                self.scroll_to = Some(row);
                            }
                            result.count
                        }));
                        tracing::debug!(kind = "history_search", worktree, request, "read_applied");
                    } else {
                        tracing::debug!(kind = "history_search", worktree, request, "read_stale");
                    }
                }
                Event::Status {
                    path,
                    request,
                    files: result,
                    submodules,
                    recovery,
                } => {
                    let Some(state) = self
                        .repos
                        .get_mut(&path)
                        .filter(|state| state.status_request == request)
                    else {
                        tracing::debug!(
                            kind = "status",
                            worktree = logging::worktree_id(&path),
                            request,
                            "read_stale"
                        );
                        continue;
                    };
                    logging::read_applied("status", &path, request);
                    state.status_busy = false;
                    state.status_read_scope = None;
                    state.operation.pending.status = None;
                    if submodules.is_some() {
                        state.status_checked = Some(Instant::now());
                        state.refresh_on_focus = false;
                    }
                    state.recovery_verified = false;
                    match recovery {
                        Some(Ok((dir, recovery))) => {
                            state.git_directory = Some(dir);
                            state.recovery = recovery;
                            state.recovery_verified = true;
                        }
                        Some(Err(error)) => {
                            state.git_directory = None;
                            state.status_error = Some(format!("无法确认 Git 操作状态：{error:#}"));
                        }
                        None => {}
                    }
                    let status_updated = result.is_ok();
                    let mut head_changed = false;
                    match result {
                        Ok(status) => {
                            head_changed = state
                                .status
                                .as_ref()
                                .is_some_and(|previous| previous.head != status.head);
                            state.status = Some(status);
                            state.visible_status_request = Some(request);
                            state.status_warning = None;
                        }
                        Err(error) => {
                            if logging::warn_periodic(&mut state.status_warning) {
                                logging::read_failed("status", &path, request, &error);
                            }
                            state.status_error = Some(format!("{error:#}"));
                        }
                    }
                    let refresh_history = head_changed && !state.operation.pending.history;
                    let updated = match submodules {
                        Some(Ok(mut modules)) => {
                            for module in &mut modules {
                                if let Some(path) = self
                                    .settings
                                    .repositories
                                    .iter()
                                    .find(|path| same_path(path, &module.path))
                                {
                                    module.path = path.clone();
                                }
                            }
                            state.submodules = Some(modules);
                            state.submodule_warning = None;
                            true
                        }
                        Some(Err(error)) => {
                            if logging::warn_periodic(&mut state.submodule_warning) {
                                logging::read_failed("submodules", &path, request, &error);
                            }
                            self.tell(
                                format!("无法读取 {} 的子模块：{error:#}", path.display()),
                                true,
                            );
                            false
                        }
                        None => false,
                    };
                    if refresh_history {
                        self.request_history(ctx, &path);
                    }
                    if self.active.as_ref() == Some(&path) {
                        if status_updated {
                            self.reconcile_worktree_file(ctx, None, head_changed);
                        }
                        self.restore_repository(ctx, &path);
                    } else if updated
                        && self.active.as_ref().is_some_and(|active| {
                            is_descendant(active, &path)
                                && !self
                                    .repository_rows()
                                    .iter()
                                    .any(|row| &row.path == active && row.submodule != Some(false))
                        })
                    {
                        self.activate(ctx, path);
                    }
                }
                Event::Details(worktree, request, selection, result) => {
                    if request != self.details_request {
                        tracing::debug!(kind = "details", worktree, request, "read_stale");
                        continue;
                    }
                    self.selection = selection;
                    self.details_busy = false;
                    match result {
                        Ok(details) => {
                            let first = details
                                .files
                                .iter()
                                .find(crate::ui_inspector::file_filter(&self.file_query))
                                .cloned();
                            self.details = Some(details);
                            self.reset_file_scroll = true;
                            if let Some(file) = first {
                                self.choose_file(ctx, file);
                            }
                        }
                        Err(error) => {
                            tracing::warn!(kind = "details", worktree, request, "read_failed");
                            self.details_error = Some(error);
                        }
                    }
                }
                Event::CopyCommitMessage(root, request, result) => {
                    if self.copy_message_request == request && self.active.as_ref() == Some(&root) {
                        match result {
                            Ok(message) => ctx.copy_text(message),
                            Err(error) => self.tell(error, true),
                        }
                    }
                }
                Event::RebaseMessage(path, request, edit) => {
                    let id = self.next();
                    if let Some(state) = self
                        .repos
                        .get_mut(&path)
                        .filter(|state| state.operation.owns(request))
                        && self.operations.contains(&path)
                    {
                        state.rebase_message = Some(RebaseMessageEdit {
                            id,
                            commit: edit.commit,
                            draft: edit.message.clone(),
                            original: edit.message,
                            visible: true,
                            focus: true,
                            reply: edit.reply,
                        });
                        self.tell_operation(&path, "Git 正在等待提交说明", false);
                    }
                }
                Event::Operation(path, request, operation, result) => {
                    self.finish_operation(ctx, &path, request, operation, result);
                }
                Event::OperationReadback(path, request, result) => {
                    self.finish_operation_readback(ctx, &path, request, result);
                }
                Event::External(result) => match result {
                    Ok(dir) => {
                        self.external_dirs.push(dir);
                        self.tell("外部 diff 工具已启动", false);
                    }
                    Err(error) => self.tell(error, true),
                },
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
                    .cached_rows()
                    .find_map(|(index, row)| row.commit.id.starts_with(&target).then_some(index))
            });
        if let Some(row) = row {
            self.scroll_to = Some(row.saturating_sub(4));
        } else if !self.state().is_some_and(|repo| {
            repo.history
                .as_ref()
                .is_some_and(HistoryView::ready_to_draw)
        }) && let Some(capture) = &mut self.frame_capture
        {
            capture.commit = Some(target);
        }
    }

    pub fn report_frame(&mut self, ctx: &egui::Context) {
        let capture_ready = self.state().is_some_and(|repo| {
            repo.status.is_some()
                && repo
                    .history
                    .as_ref()
                    .is_some_and(HistoryView::ready_to_draw)
        });
        if capture_ready && let Some(capture) = &mut self.frame_capture {
            if capture.save(ctx) {
                self.frame_capture = None;
            } else {
                capture.queue(ctx);
            }
        }
        if let Some(path) = &self.active
            && let Some(repo) = self.repos.get(path)
        {
            let graph_ready = repo
                .history
                .as_ref()
                .is_some_and(HistoryView::ready_to_draw);
            let status_ready = repo.status.is_some() && !repo.status_busy;
            if let Some(timings) = &mut self.timings {
                timings.presented(path, graph_ready, status_ready);
            }
            if let Some(presentation) = &mut self.presentation {
                presentation.presented(
                    repo.visible_history_request.filter(|_| graph_ready),
                    repo.visible_status_request.filter(|_| status_ready),
                );
            }
        }
    }

    pub fn report_history_frame(&mut self, generation: Option<u64>, rows: usize, missing: bool) {
        if let Some(presentation) = &mut self.presentation
            && let Some(sample) = presentation.history_frame(generation, rows, missing)
            && let (Some(timings), Some(path)) = (&mut self.timings, &self.active)
        {
            timings.history_filter_finished("history-filter-ready", path, sample);
        }
    }

    pub fn prefetch(&mut self, ctx: &egui::Context) {
        if !self.settings.prefetch_history
            || !self
                .state()
                .and_then(|state| state.history.as_ref())
                .is_some_and(HistoryView::ready_to_draw)
            || self.repos.values().any(|repo| repo.history_busy)
        {
            return;
        }
        let candidate = self.settings.repositories.iter().find(|path| {
            self.active.as_ref() != Some(*path)
                && self.repos.get(*path).is_none_or(|repo| {
                    repo.history.is_none()
                        && repo.history_error.is_none()
                        && !repo.history_on_demand
                })
                && self.can_prefetch(path)
        });
        if let Some(path) = candidate.cloned() {
            let nested = self
                .settings
                .repositories
                .iter()
                .any(|parent| is_descendant(&path, parent));
            self.queue_history(ctx, &path, "prefetch", nested, None);
        }
    }

    fn can_prefetch(&self, path: &Path) -> bool {
        // A legacy entry can be a submodule of a registered parent that is
        // still loading. Wait for that scan before prefetching its descendants.
        !self.repos.iter().any(|(parent, state)| {
            (state.status_busy
                && self.active.as_ref() == Some(parent)
                && is_descendant(path, parent))
                || self
                    .submodules(parent)
                    .iter()
                    .any(|module| same_path(path, &module.path))
        })
    }

    pub fn background_status(&mut self, ctx: &egui::Context) {
        let focused = ctx.input(|input| input.viewport().focused).unwrap_or(true);
        let refocused = focused && !self.focused;
        self.focused = focused;
        if !focused {
            return;
        }
        let Some(path) = self.active.clone() else {
            return;
        };
        if let Some(state) = self.repos.get_mut(&path) {
            state.refresh_on_focus |= refocused;
            if let Some(delay) = state.focus_refresh_delay(Instant::now()) {
                if delay.is_zero() {
                    self.request_status(ctx, &path, true, StatusScope::Full);
                } else {
                    ctx.request_repaint_after(delay);
                }
            }
        }
    }
}

fn normalize_repositories(settings: &mut Settings) {
    for entry in std::mem::take(&mut settings.repositories) {
        let absolute = std::path::absolute(&entry).unwrap_or_else(|_| entry.clone());
        let was_active = settings
            .active
            .as_ref()
            .is_some_and(|path| same_path(path, &entry) || same_path(path, &absolute));
        let root = if absolute.is_dir()
            && !absolute.join(".git").exists()
            && let Ok(root) = git::root(&absolute)
            && !git::submodules(&root).is_ok_and(|modules| {
                modules
                    .iter()
                    .any(|module| same_path(&module.path, &absolute))
            }) {
            root
        } else {
            absolute
        };
        let registered = settings
            .repositories
            .iter()
            .find(|path| same_path(path, &root))
            .cloned()
            .unwrap_or_else(|| {
                settings.repositories.push(root.clone());
                root
            });
        if was_active {
            settings.active = Some(registered.clone());
        }
        if let Some(scope) = settings.branch_selection.remove(&entry) {
            settings.branch_selection.entry(registered).or_insert(scope);
        }
    }
    settings.active = settings
        .active
        .take()
        .map(|path| std::path::absolute(&path).unwrap_or(path));
}

fn is_descendant(path: &Path, parent: &Path) -> bool {
    path.ancestors()
        .skip(1)
        .any(|ancestor| same_path(ancestor, parent))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refocus_refresh_waits_until_the_throttle_expires() {
        let now = Instant::now();
        let mut repository = Repository {
            status_checked: Some(now),
            refresh_on_focus: true,
            ..Repository::default()
        };
        assert_eq!(
            repository.focus_refresh_delay(now),
            Some(STATUS_REFRESH_INTERVAL)
        );
        let deadline = now + STATUS_REFRESH_INTERVAL;
        assert_eq!(
            repository.focus_refresh_delay(deadline),
            Some(Duration::ZERO)
        );
        repository.status_busy = true;
        assert_eq!(repository.focus_refresh_delay(deadline), None);
        repository.status_busy = false;
        repository.refresh_on_focus = false;
        assert_eq!(repository.focus_refresh_delay(deadline), None);
    }

    #[test]
    fn newer_operations_keep_unfinished_calibration_scoped_to_their_worktree() {
        let now = Instant::now();
        let mut first = Repository::default();
        let mut second = Repository::default();
        let scope = first.operation.begin(1, RefreshScope::BOTH);
        first.invalidate_operation_reads(1, scope);
        first.operation.finish(
            RefreshScope::BOTH,
            Some(TargetedRead::Head(git::HeadUpdate::SwitchBranch)),
            now,
        );
        assert!(matches!(first.operation.after_frame, Followup::Read(_)));

        let second_scope = second.operation.begin(2, RefreshScope::HISTORY);
        second.invalidate_operation_reads(2, second_scope);
        second.operation.finish(RefreshScope::HISTORY, None, now);
        let new_scope = first.operation.begin(3, RefreshScope::STATUS);
        first.invalidate_operation_reads(3, new_scope);
        assert!(!first.operation.owns(1));
        assert!(!second.operation.owns(3));
        assert_eq!((first.history_request, first.status_request), (3, 3));
        assert_eq!((second.history_request, second.status_request), (2, 0));

        first.operation.finish(RefreshScope::default(), None, now);
        assert_eq!(first.operation.pending, RefreshScope::BOTH);
        assert!(matches!(first.operation.after_frame, Followup::Refresh(_)));
        assert_eq!(second.operation.pending, RefreshScope::HISTORY);
    }

    #[test]
    fn ordinary_index_operations_coalesce_only_final_status_reads() {
        let now = Instant::now();
        let mut operation = OperationCompletion::default();
        operation.begin(1, RefreshScope::FILES);
        operation.finish(RefreshScope::FILES, None, now);
        assert!(
            matches!(operation.ready_followup(now), Err(delay) if delay == INDEX_STATUS_COALESCE)
        );

        let next = now + INDEX_STATUS_COALESCE / 2;
        operation.begin(2, RefreshScope::FILES);
        operation.finish(RefreshScope::FILES, None, next);
        assert!(
            operation
                .ready_followup(now + INDEX_STATUS_COALESCE)
                .is_err()
        );
        assert!(matches!(
            operation.ready_followup(next + INDEX_STATUS_COALESCE),
            Ok(Followup::Refresh(_))
        ));
        assert!(matches!(
            operation.ready_followup(next + INDEX_STATUS_COALESCE),
            Ok(Followup::Idle)
        ));

        operation.finish(
            RefreshScope::FILES,
            Some(TargetedRead::Index {
                source: Area::Unstaged,
                paths: ["a".into()].into(),
            }),
            now,
        );
        assert!(matches!(
            operation.ready_followup(now),
            Ok(Followup::Read(_))
        ));
        operation.finish(RefreshScope::STATUS, None, now);
        assert_eq!(operation.pending, RefreshScope::STATUS);
        assert!(matches!(
            operation.ready_followup(now),
            Ok(Followup::Refresh(_))
        ));
    }

    #[test]
    fn file_refresh_reuses_only_a_recent_submodule_read_for_the_same_head() {
        let now = Instant::now();
        let status = || Status {
            head: Some("a".repeat(40)),
            files: vec![],
        };
        let mut repository = Repository {
            status: Some(status()),
            submodules: Some(vec![]),
            status_checked: Some(now),
            ..Repository::default()
        };
        let cached_head = repository.cached_head_for_file_refresh(StatusScope::Files, now);
        assert!(!needs_submodule_refresh(
            cached_head.as_ref(),
            &Ok(status())
        ));
        let mut changed = status();
        changed.head = Some("b".repeat(40));
        assert!(needs_submodule_refresh(cached_head.as_ref(), &Ok(changed)));
        let mut changed = status();
        changed.files.push(Change {
            path: ".gitmodules".into(),
            old_path: None,
            code: 'M',
            area: Area::Unstaged,
            left_gitlink: false,
            right_gitlink: false,
        });
        assert!(needs_submodule_refresh(cached_head.as_ref(), &Ok(changed)));
        assert!(
            repository
                .cached_head_for_file_refresh(StatusScope::Full, now)
                .is_none()
        );
        assert!(
            repository
                .cached_head_for_file_refresh(StatusScope::Files, now + STATUS_REFRESH_INTERVAL)
                .is_none()
        );
        repository.submodules = None;
        assert!(
            repository
                .cached_head_for_file_refresh(StatusScope::Files, now)
                .is_none()
        );
    }

    #[test]
    fn confirmed_head_updates_the_loaded_row_and_leaves_new_topology_to_history() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path();
        let git = |args: &[&str]| -> Result<()> {
            let output = git::fixture_command(root).args(args).output()?;
            anyhow::ensure!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            Ok(())
        };
        git(&["init", "--initial-branch=main"])?;
        git(&["config", "user.name", "Refdesk Test"])?;
        git(&["config", "user.email", "test@example.invalid"])?;
        git(&["commit", "--allow-empty", "-m", "base"])?;
        git(&["branch", "previous"])?;
        git(&["commit", "--allow-empty", "-m", "main"])?;
        let cache = HistoryCache::open(root, &HistoryScope::All, true)?;
        let summary = cache.load_through(1, &AtomicBool::new(false))?;
        let pages = cache.window(0..1, summary, Default::default())?;
        let mut repo = Repository {
            history: Some(HistoryView {
                cache,
                summary,
                pages,
                generation: 1,
            }),
            ..Repository::default()
        };
        assert_eq!(repo.head_row(), Some(0));
        Operation::Switch("previous".into()).execute(root)?;
        let change = TargetedRead::Head(git::HeadUpdate::SwitchBranch).read(root)?;
        let Some(ConfirmedChange::Head {
            head,
            switch_branch,
        }) = change
        else {
            panic!("expected current HEAD");
        };
        let previous = head.id.clone();
        repo.update_head(head, switch_branch);
        assert_eq!(repo.history_info().unwrap().branch, "previous");
        assert_eq!(repo.head_row(), Some(1));
        assert_eq!(repo.history.as_ref().unwrap().cache.info.branch, "main");

        git(&["commit", "--allow-empty", "-m", "new history"])?;
        let change = TargetedRead::Head(git::HeadUpdate::LocateCommit).read(root)?;
        let Some(ConfirmedChange::Head {
            head,
            switch_branch,
        }) = change
        else {
            panic!("expected new HEAD");
        };
        assert_eq!(head.parents.as_slice(), std::slice::from_ref(&previous));
        repo.update_head(head, switch_branch);
        assert_eq!(repo.history_info().unwrap().head.as_ref(), Some(&previous));
        assert_eq!(repo.history.as_ref().unwrap().summary.loaded, 2);
        Ok(())
    }
}
