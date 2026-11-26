use crate::diff::{self, Content};
use crate::git::{
    self, Change, Commit, Compare, Details, HistoryScope, Operation, Status, Submodule,
};
use crate::history::{HistoryCache, HistoryView, SearchDirection, SearchResult};
use crate::settings::{self, Settings, Tool};
use crate::tasks::{ReadKind, Scheduler};
use crate::timing::{FrameCapture, Timings};
use anyhow::Result;
use crossbeam_channel::{Receiver, Sender, unbounded};
use eframe::egui;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tempfile::TempDir;

type Outcome<T> = std::result::Result<T, String>;
const STATUS_REFRESH_INTERVAL: Duration = Duration::from_secs(25);

enum Work<'a> {
    Read,
    Latest(ReadKind, &'a Path),
    Write,
}

enum Event {
    Added(Outcome<Vec<PathBuf>>, bool),
    History(PathBuf, u64, Outcome<Option<HistoryView>>),
    HistorySearch(u64, Outcome<SearchResult>),
    Status(PathBuf, u64, Outcome<Status>, Outcome<Vec<Submodule>>),
    Details(u64, Selection, Outcome<Details>),
    Diff(u64, Outcome<Content>),
    Operation(PathBuf, Option<String>, Outcome<String>),
    External(Outcome<TempDir>),
}

#[derive(Default)]
pub(crate) struct Repository {
    pub history: Option<HistoryView>,
    pub status: Option<Status>,
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
    status_checked: Option<Instant>,
    refresh_on_focus: bool,
}

impl Repository {
    pub fn changes(&self) -> &[Change] {
        self.status.as_ref().map_or(&[], |status| &status.files)
    }

    fn focus_refresh_delay(&self, now: Instant) -> Option<Duration> {
        (self.refresh_on_focus && !self.status_busy).then(|| {
            self.status_checked.map_or(Duration::ZERO, |checked| {
                STATUS_REFRESH_INTERVAL.saturating_sub(now.saturating_duration_since(checked))
            })
        })
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
    Forget(PathBuf),
}

pub(crate) struct Desk {
    #[cfg(windows)]
    pub window_chrome: Option<crate::window_chrome::WindowChrome>,
    pub settings: Settings,
    pub active: Option<PathBuf>,
    pub repos: HashMap<PathBuf, Repository>,
    restore_active: Option<PathBuf>,
    pub selection: Selection,
    pub details: Option<Details>,
    pub details_error: Option<String>,
    pub details_busy: bool,
    pub save_icon: egui::TextureHandle,
    pub file: Option<Change>,
    pub comparison: Option<Compare>,
    pub content: Option<Content>,
    pub diff_error: Option<String>,
    pub diff_busy: bool,
    pub editor: String,
    pub search: String,
    pub branch_query: String,
    pub search_row: Option<usize>,
    history_search: Option<HistorySearch>,
    pub scroll_to: Option<usize>,
    pub dialog: Option<Dialog>,
    pub tools_open: bool,
    pub import_message: String,
    pub status_message: String,
    pub status_is_error: bool,
    pub operations: HashSet<PathBuf>,
    pub sheet_view: crate::ui_workbook::SheetView,
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
        let (events, inbox) = unbounded();
        let (mut settings, mut warning, writes_enabled) = match Settings::load() {
            Ok(settings) => (settings, None, true),
            Err(error) => (Settings::default(), Some(error.to_string()), false),
        };
        normalize_repositories(&mut settings);
        for path in paths {
            let path = match git::root(&path) {
                Ok(root) => root,
                Err(error) => {
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
        let status_is_error = warning.is_some();
        settings.theme.apply(ctx);
        let mut desk = Self {
            #[cfg(windows)]
            window_chrome: crate::window_chrome::WindowChrome::install(creation, settings.theme),
            settings,
            active: None,
            repos: HashMap::new(),
            restore_active,
            selection: Selection::Worktree,
            details: None,
            details_error: None,
            details_busy: false,
            save_icon: crate::ui::load_save_icon(ctx),
            file: None,
            comparison: None,
            content: None,
            diff_error: None,
            diff_busy: false,
            editor: String::new(),
            search: String::new(),
            branch_query: String::new(),
            search_row: None,
            history_search: None,
            scroll_to: None,
            dialog: None,
            tools_open: false,
            import_message,
            status_message: warning.unwrap_or_default(),
            status_is_error,
            writes_enabled,
            operations: HashSet::new(),
            sheet_view: crate::ui_workbook::SheetView::default(),
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
            tasks: Scheduler::new(),
            events,
            inbox,
        };
        if let Some(path) = initial {
            desk.activate_path(ctx, path);
        }
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
            Work::Latest(kind, path) => self.tasks.latest(kind, path, task),
            Work::Write => self.tasks.write(task),
        }
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
        self.selection = Selection::Worktree;
        self.details_busy = false;
        self.details = None;
        self.details_error = None;
        self.details_request = self.next();
        self.clear_diff();
        self.search.clear();
        self.branch_query.clear();
        self.cancel_history_search();
        self.search_row = None;
        self.scroll_to = None;
        self.graph_reset += 1;
        self.editor =
            std::mem::take(&mut self.repos.get_mut(&path).expect("active repository").draft);

        let state = self.repos.get_mut(&path).expect("active repository");
        state.history_viewport = 0..2;
        if let Some(history) = &state.history {
            history.cache.set_active(true);
        } else if !state.history_busy {
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
            self.restore_active = None;
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
        self.queue_history(ctx, path, false, None);
    }

    fn deactivate_history(&mut self, path: &Path) {
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
        check_superproject: bool,
        checked_refs: Option<usize>,
    ) {
        let active = self.active.as_deref() == Some(path);
        if active {
            self.cancel_history_search();
            self.search_row = None;
        }
        let request = self.next();
        if active && let Some(timings) = &mut self.timings {
            timings.change_history_scope(path, request, checked_refs);
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
        self.submit(ctx, Work::Latest(ReadKind::History, path), move || {
            let result = readable((|| {
                if check_superproject && git::has_superproject(&root)? {
                    return Ok(None);
                }
                let cache = HistoryCache::open(&root, &filter, active)?;
                let summary = cache.load_through(range.end.saturating_sub(1).max(1), &canceled)?;
                let pages = if active {
                    cache.window(range, summary, Default::default())?
                } else {
                    Default::default()
                };
                Ok(Some(HistoryView {
                    cache,
                    summary,
                    pages,
                    generation: request,
                }))
            })());
            Event::History(root, request, result)
        });
    }

    fn request_status(&mut self, ctx: &egui::Context, path: &Path, force: bool) {
        let request = self.next();
        let state = self.repos.entry(path.to_owned()).or_default();
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
        state.refresh_on_focus = false;
        let root = path.to_owned();
        self.submit(ctx, Work::Latest(ReadKind::Status, path), move || {
            let result = readable(git::status(&root));
            let submodules = readable(git::submodules(&root));
            Event::Status(root, request, result, submodules)
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
        let checked_refs = match &filter {
            HistoryScope::Head => 1,
            HistoryScope::All => 0,
            HistoryScope::Selected(refs) => refs.len(),
        };
        self.settings.branch_selection.insert(path.clone(), filter);
        state.history_viewport = 0..2;
        self.graph_reset += 1;
        self.search_row = None;
        self.queue_history(ctx, &path, false, Some(checked_refs));
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
        self.submit(ctx, Work::Latest(ReadKind::History, &path), move || {
            let result = readable((|| {
                let summary = if extend {
                    cache.load_through(summary.page_count() + 1, &canceled)?
                } else {
                    summary
                };
                let pages = cache.window(range, summary, pages)?;
                Ok(Some(HistoryView {
                    cache,
                    summary,
                    pages,
                    generation,
                }))
            })());
            Event::History(root, request, result)
        });
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
        self.submit(
            ctx,
            Work::Latest(ReadKind::HistorySearch, &path),
            move || {
                Event::HistorySearch(
                    request,
                    readable(cache.search(&query, loaded, anchor, direction, &canceled)),
                )
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
        self.details_request = self.next();
        let request = self.details_request;
        self.details = None;
        self.details_error = None;
        self.details_busy = true;
        self.clear_diff();
        let path = root.clone();
        self.submit(ctx, Work::Latest(ReadKind::Details, &path), move || {
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
                            return Event::Details(request, selection, Err(format!("{error:#}")));
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
            Event::Details(
                request,
                Selection::Commit {
                    right,
                    left,
                    right_position,
                },
                result,
            )
        });
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
        self.sheet_view = crate::ui_workbook::SheetView::default();
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
        let path = root.clone();
        self.submit(ctx, Work::Latest(ReadKind::Diff, &path), move || {
            Event::Diff(request, readable(diff::compare(&root, &pair, force_text)))
        });
    }

    pub fn clear_diff(&mut self) {
        if let Some(active) = &self.active {
            self.tasks.cancel(active, Some(ReadKind::Diff));
        }
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
        self.submit(ctx, Work::Write, move || {
            let result = readable(operation.execute(&root));
            Event::Operation(root, committed_message, result)
        });
    }

    pub fn external(&mut self, ctx: &egui::Context, tool: Tool) {
        let (Some(root), Some(pair)) = (self.active.clone(), self.comparison.clone()) else {
            return;
        };
        self.tell(format!("正在准备 {} 的两个版本…", tool.name), false);
        self.submit(ctx, Work::Read, move || {
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
                    state.history_cancel = None;
                    let pending = state.history_pending_window.take();
                    match result {
                        Ok(Some(mut history)) => {
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
                        }
                        Ok(None) => {
                            state.history_on_demand = true;
                            if self.active.as_ref() == Some(&path) {
                                self.request_history(ctx, &path);
                            }
                        }
                        Err(error) => {
                            state.history_error = Some(error);
                            state.history_failed_window = pending;
                        }
                    }
                }
                Event::HistorySearch(request, result) => {
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
                    }
                }
                Event::Status(path, request, result, submodules) => {
                    let Some(state) = self.repos.get_mut(&path) else {
                        continue;
                    };
                    if state.status_request != request {
                        continue;
                    }
                    state.status_busy = false;
                    state.status_checked = Some(Instant::now());
                    match result {
                        Ok(status) => state.status = Some(status),
                        Err(error) => state.status_error = Some(error),
                    }
                    let updated = match submodules {
                        Ok(mut modules) => {
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
                            true
                        }
                        Err(error) => {
                            self.tell(
                                format!("无法读取 {} 的子模块：{error}", path.display()),
                                true,
                            );
                            false
                        }
                    };
                    if self.active.as_ref() == Some(&path) {
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
                Event::Details(request, selection, result) if request == self.details_request => {
                    self.selection = selection;
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
        if let Some(timings) = &mut self.timings
            && let Some(path) = &self.active
            && let Some(repo) = self.repos.get(path)
        {
            timings.presented(
                path,
                repo.history
                    .as_ref()
                    .is_some_and(HistoryView::ready_to_draw),
                repo.status.is_some() && !repo.status_busy,
            );
        }
    }

    pub fn report_history_frame(&mut self, generation: Option<u64>, rows: usize, missing: bool) {
        if let (Some(timings), Some(path)) = (&mut self.timings, &self.active) {
            timings.history_frame(path, generation, rows, missing);
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
            self.queue_history(ctx, &path, nested, None);
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
                    self.request_status(ctx, &path, true);
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
}
