use anyhow::{Context, Result};
use std::collections::hash_map::RandomState;
use std::fs;
use std::hash::BuildHasher;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::level_filters::LevelFilter;
use tracing_appender::non_blocking::{ErrorCounter, NonBlockingBuilder, WorkerGuard};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::filter::Targets;
use tracing_subscriber::fmt::time::UtcTime;
use tracing_subscriber::layer::SubscriberExt;

const RETENTION: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const CLEANUP_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const PREFIX: &str = "refdesk-";
static WORKTREE_IDS: LazyLock<RandomState> = LazyLock::new(RandomState::new);

pub(crate) struct RuntimeLog {
    dropped: ErrorCounter,
    _guard: WorkerGuard,
}

impl RuntimeLog {
    pub fn open() -> Result<Option<Self>> {
        let level = parse_level(std::env::var("REFDESK_LOG_LEVEL").ok().as_deref());
        if level == LevelFilter::OFF {
            return Ok(None);
        }
        let directory = std::env::var_os("REFDESK_LOG_DIR")
            .filter(|path| !path.is_empty())
            .map(PathBuf::from)
            .or_else(|| dirs::data_local_dir().map(|path| path.join("Refdesk").join("logs")))
            .context("无法定位本机应用数据目录")?;
        let pid = std::process::id();
        let started = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
        let session = format!("{started}-{pid}");
        let files = RollingFileAppender::builder()
            .rotation(Rotation::DAILY)
            .filename_prefix(format!("{PREFIX}{session}"))
            .filename_suffix("log")
            .build(&directory)?;
        let (writer, guard) = NonBlockingBuilder::default()
            .buffered_lines_limit(4_096)
            .lossy(true)
            .thread_name("refdesk-log")
            .finish(LogFiles {
                files,
                directory,
                cleaned: None,
            });
        let dropped = writer.error_counter();
        let output = tracing_subscriber::fmt::layer()
            .with_timer(UtcTime::rfc_3339())
            .with_ansi(false)
            .with_writer(writer);
        let subscriber = tracing_subscriber::registry()
            .with(output)
            .with(Targets::new().with_target("refdesk", level));
        tracing::subscriber::set_global_default(subscriber)?;
        install_panic_hook();
        tracing::info!(
            session,
            version = env!("CARGO_PKG_VERSION"),
            pid,
            "app_started"
        );
        Ok(Some(Self {
            dropped,
            _guard: guard,
        }))
    }

    pub fn record_exit(&self, elapsed: Duration) {
        let dropped_lines = self.dropped.dropped_lines();
        if dropped_lines != 0 {
            tracing::warn!(dropped_lines, "log_queue_dropped");
        }
        tracing::info!(elapsed_ms = millis(elapsed), dropped_lines, "app_exited");
    }
}

fn parse_level(value: Option<&str>) -> LevelFilter {
    value
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(LevelFilter::INFO)
}

fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some(location) = info.location() {
            // A panic payload or a build-machine path can contain private data.
            let line = location.line();
            tracing::error!(line, "app_panic");
        } else {
            tracing::error!("app_panic");
        }
        previous(info);
    }));
}

pub(crate) fn worktree_id(path: &Path) -> u64 {
    WORKTREE_IDS.hash_one(path)
}

pub(crate) fn millis(duration: Duration) -> u64 {
    duration.as_millis() as u64
}

#[derive(Clone, Copy)]
pub(crate) struct ReadLog {
    kind: &'static str,
    pub worktree: u64,
    pub request: u64,
    queued: Instant,
}

impl ReadLog {
    pub fn queue(kind: &'static str, path: &Path, request: u64) -> Self {
        let queued = Instant::now();
        let worktree = worktree_id(path);
        tracing::debug!(kind, worktree, request, "read_queued");
        Self {
            kind,
            worktree,
            request,
            queued,
        }
    }

    pub fn complete(&self, started: Instant, outcome: &'static str) {
        let Self {
            kind,
            worktree,
            request,
            queued,
        } = *self;
        let queue_ms = millis(started.duration_since(queued));
        let run_ms = millis(started.elapsed());
        if matches!(outcome, "ok" | "on_demand") && (queue_ms >= 750 || run_ms >= 750) {
            tracing::warn!(
                kind,
                worktree,
                request,
                outcome,
                queue_ms,
                run_ms,
                "read_slow"
            );
        } else {
            tracing::debug!(
                kind,
                worktree,
                request,
                outcome,
                queue_ms,
                run_ms,
                "read_finished"
            );
        }
    }
}

pub(crate) fn read_applied(kind: &'static str, path: &Path, request: u64) {
    tracing::debug!(kind, worktree = worktree_id(path), request, "read_applied");
}

pub(crate) fn read_failed(kind: &'static str, path: &Path, request: u64, error: &anyhow::Error) {
    let (error_kind, exit_code) = crate::git::error_kind(error);
    tracing::warn!(
        kind,
        worktree = worktree_id(path),
        request,
        error_kind,
        exit_code,
        "read_failed"
    );
}

pub(crate) fn warn_periodic(last: &mut Option<Instant>) -> bool {
    if last.is_some_and(|last| last.elapsed() < Duration::from_secs(60)) {
        return false;
    }
    *last = Some(Instant::now());
    true
}

pub(crate) struct Presentation {
    worktree: u64,
    activation: u64,
    started: Instant,
    graph: bool,
    status: bool,
    history_filter: Option<HistoryFilterTiming>,
    drawn_history: Option<u64>,
}

struct HistoryFilterTiming {
    request: u64,
    started: Instant,
    last_frame: Instant,
    frames: u32,
    max_gap: Duration,
    missing_frames: u32,
    intermediate_graphs: u32,
}

pub(crate) struct HistoryFilterSample {
    pub request: u64,
    pub elapsed_ms: u64,
    pub commits: usize,
    pub frames: u32,
    pub max_gap_ms: u64,
    pub missing_frames: u32,
    pub intermediate_graphs: u32,
}

impl Presentation {
    pub fn activate(
        path: &Path,
        activation: u64,
        cached_history: bool,
        cached_status: bool,
    ) -> Self {
        let worktree = worktree_id(path);
        tracing::info!(worktree, activation, "repository_activated");
        tracing::debug!(
            worktree,
            activation,
            cached_history,
            cached_status,
            "activation_cache"
        );
        Self {
            worktree,
            activation,
            started: Instant::now(),
            graph: false,
            status: false,
            history_filter: None,
            drawn_history: None,
        }
    }

    pub fn presented(&mut self, history: Option<u64>, status: Option<u64>) {
        if let Some(request) = history
            && !self.graph
        {
            self.graph = true;
            tracing::info!(
                worktree = self.worktree,
                activation = self.activation,
                request,
                elapsed_ms = millis(self.started.elapsed()),
                "graph_ready"
            );
        }
        if let Some(request) = status
            && !self.status
        {
            self.status = true;
            tracing::info!(
                worktree = self.worktree,
                activation = self.activation,
                request,
                elapsed_ms = millis(self.started.elapsed()),
                "status_ready"
            );
        }
    }

    pub fn change_history_filter(
        &mut self,
        request: u64,
        scope: Option<(&'static str, usize)>,
        record_timings: bool,
    ) -> Option<HistoryFilterSample> {
        let previous = self.finish_history_filter("superseded", 0);
        if let Some((mode, checked_refs)) = scope
            && (record_timings || tracing::enabled!(tracing::Level::INFO))
        {
            let started = Instant::now();
            self.history_filter = Some(HistoryFilterTiming {
                request,
                started,
                last_frame: started,
                frames: 0,
                max_gap: Duration::ZERO,
                missing_frames: 0,
                intermediate_graphs: 0,
            });
            tracing::debug!(
                worktree = self.worktree,
                request,
                mode,
                checked_refs,
                "history_filter_requested"
            );
        }
        previous
    }

    pub fn history_frame(
        &mut self,
        generation: Option<u64>,
        commits: usize,
        missing: bool,
    ) -> Option<HistoryFilterSample> {
        let previous = std::mem::replace(&mut self.drawn_history, generation);
        let pending = self.history_filter.as_mut()?;
        let now = Instant::now();
        pending.max_gap = pending.max_gap.max(now.duration_since(pending.last_frame));
        pending.last_frame = now;
        pending.frames += 1;
        pending.missing_frames += u32::from(generation.is_none() || missing);
        if generation == Some(pending.request) && !missing {
            return self.finish_history_filter("ready", commits);
        }
        if generation.is_some() && generation != previous && generation != Some(pending.request) {
            pending.intermediate_graphs += 1;
        }
        None
    }

    pub fn history_filter_failed(&mut self, request: u64) {
        if self
            .history_filter
            .as_ref()
            .is_some_and(|pending| pending.request == request)
        {
            self.finish_history_filter("failed", 0);
        }
    }

    pub fn deactivate(&mut self) {
        self.finish_history_filter("deactivated", 0);
    }

    fn finish_history_filter(
        &mut self,
        outcome: &'static str,
        commits: usize,
    ) -> Option<HistoryFilterSample> {
        let pending = self.history_filter.take()?;
        let sample = HistoryFilterSample {
            request: pending.request,
            elapsed_ms: millis(pending.started.elapsed()),
            commits,
            frames: pending.frames,
            max_gap_ms: millis(pending.max_gap),
            missing_frames: pending.missing_frames,
            intermediate_graphs: pending.intermediate_graphs,
        };
        tracing::info!(
            worktree = self.worktree,
            request = sample.request,
            outcome,
            elapsed_ms = sample.elapsed_ms,
            commits,
            frames = sample.frames,
            max_gap_ms = sample.max_gap_ms,
            missing_frames = sample.missing_frames,
            intermediate_graphs = sample.intermediate_graphs,
            "history_filter_finished"
        );
        Some(sample)
    }
}

struct LogFiles {
    files: RollingFileAppender,
    directory: PathBuf,
    cleaned: Option<Instant>,
}

impl Write for LogFiles {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let written = self.files.write(buf)?;
        if self
            .cleaned
            .is_none_or(|cleaned| cleaned.elapsed() >= CLEANUP_INTERVAL)
        {
            self.cleaned = Some(Instant::now());
            prune_expired(&self.directory, SystemTime::now());
        }
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.files.flush()
    }
}

fn prune_expired(directory: &Path, now: SystemTime) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        if !entry.file_name().to_str().is_some_and(is_runtime_log_name)
            || !entry.file_type().is_ok_and(|kind| kind.is_file())
        {
            continue;
        }
        if entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .is_ok_and(|time| now.duration_since(time).is_ok_and(|age| age > RETENTION))
        {
            remove_closed_log(&entry.path());
        }
    }
}

fn is_runtime_log_name(name: &str) -> bool {
    let Some((session, date)) = name
        .strip_prefix(PREFIX)
        .and_then(|name| name.strip_suffix(".log"))
        .and_then(|name| name.split_once('.'))
    else {
        return false;
    };
    let digits = |value: &str| !value.is_empty() && value.bytes().all(|c| c.is_ascii_digit());
    session
        .split_once('-')
        .is_some_and(|(started, pid)| digits(started) && digits(pid))
        && date.len() == 10
        && date.as_bytes()[4] == b'-'
        && date.as_bytes()[7] == b'-'
        && date.split('-').all(digits)
}

fn remove_closed_log(path: &Path) {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // A currently open appender prevents an exclusive write handle.
        if fs::OpenOptions::new()
            .write(true)
            .share_mode(0)
            .open(path)
            .is_err()
        {
            return;
        }
    }
    let _ = fs::remove_file(path);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Seek};

    #[test]
    fn log_level_defaults_to_info() {
        assert_eq!(parse_level(None), LevelFilter::INFO);
        assert_eq!(parse_level(Some("missing")), LevelFilter::INFO);
        assert_eq!(parse_level(Some(" debug ")), LevelFilter::DEBUG);
        assert_eq!(parse_level(Some("OFF")), LevelFilter::OFF);
    }

    #[test]
    fn history_filter_logs_one_anonymized_result_per_request() -> Result<()> {
        let mut output = tempfile::tempfile()?;
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .without_time()
            .with_ansi(false)
            .with_writer(output.try_clone()?)
            .finish();
        let path = Path::new("C:/test/private-worktree");
        tracing::subscriber::with_default(subscriber, || {
            let mut view = Presentation::activate(path, 1, false, false);
            assert!(view.history_frame(Some(7), 32, false).is_none());
            assert!(
                view.change_history_filter(8, Some(("selected", 2)), false)
                    .is_none()
            );
            assert!(view.history_frame(Some(7), 32, false).is_none());
            assert!(view.history_frame(Some(6), 32, false).is_none());
            assert!(view.history_frame(Some(6), 32, true).is_none());
            let replaced = view
                .change_history_filter(9, Some(("head", 1)), false)
                .unwrap();
            assert_eq!(
                (
                    replaced.request,
                    replaced.frames,
                    replaced.missing_frames,
                    replaced.intermediate_graphs
                ),
                (8, 3, 1, 1)
            );
            view.history_filter_failed(8);
            let ready = view.history_frame(Some(9), 32, false).unwrap();
            assert_eq!((ready.request, ready.commits), (9, 32));
            assert!(view.history_frame(Some(9), 32, false).is_none());
            view.change_history_filter(10, Some(("selected", 2)), false);
            view.history_filter_failed(10);
            assert!(view.history_frame(Some(10), 32, false).is_none());
        });

        output.rewind()?;
        let mut contents = String::new();
        output.read_to_string(&mut contents)?;
        let results: Vec<_> = contents
            .lines()
            .filter(|line| line.contains("history_filter_finished"))
            .collect();
        assert_eq!(results.len(), 3);
        for (line, (id, outcome)) in
            results
                .iter()
                .zip([(8, "superseded"), (9, "ready"), (10, "failed")])
        {
            assert!(line.contains(&format!("worktree={} request={id}", worktree_id(path))));
            assert!(line.contains(&format!("outcome=\"{outcome}\"")));
        }
        assert!(!contents.contains("private-worktree"));
        Ok(())
    }

    #[test]
    fn cleanup_only_removes_expired_refdesk_logs() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let now = SystemTime::now();
        let old = now - RETENTION - Duration::from_secs(60);
        let expired = directory.path().join("refdesk-1000-1.2026-01-02.log");
        let unrelated = directory.path().join("refdesk-example.log");
        let recent = directory.path().join("refdesk-2000-4.2026-01-03.log");

        fs::File::create(&expired)?.set_modified(old)?;
        fs::File::create(&unrelated)?.set_modified(old)?;
        fs::File::create(&recent)?;

        #[cfg(windows)]
        let active = {
            let path = directory.path().join("refdesk-3000-5.2026-01-01.log");
            let file = fs::File::create(&path)?;
            file.set_modified(old)?;
            (path, file)
        };

        prune_expired(directory.path(), now);
        assert!(!expired.exists());
        assert!(unrelated.exists());
        assert!(recent.exists());

        #[cfg(windows)]
        {
            let (path, file) = active;
            assert!(path.exists());
            drop(file);
            prune_expired(directory.path(), now);
            assert!(!path.exists());
        }
        Ok(())
    }
}
