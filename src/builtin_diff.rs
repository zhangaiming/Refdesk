use crate::git::{self, Compare, Version};
use crate::logging::ReadLog;
use crate::ui_diff::Panel;
use crossbeam_channel::{Receiver, Sender, TryRecvError, bounded, unbounded};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Column {
    Left,
    Right,
}

impl Column {
    pub const BOTH: [Self; 2] = [Self::Left, Self::Right];

    pub fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub(crate) enum Issue {
    Read,
    FileMissing,
    AccessDenied,
    Decode,
    SnapshotLimit,
    PreviewLimit,
    Binary(usize),
    ParserUnavailable,
    GraphicsLimit,
    LfsPointer,
    LfsCache,
    LfsDownload,
    LfsMissing,
}

pub(crate) enum Side<T> {
    Loading,
    Absent,
    Ready(T),
    Failed(Issue),
}

impl<T> Side<T> {
    pub fn ready(&self) -> Option<&T> {
        match self {
            Self::Ready(value) => Some(value),
            _ => None,
        }
    }

    pub fn map<'a, U>(&'a self, map: impl FnOnce(&'a T) -> U) -> Side<U> {
        match self {
            Self::Ready(value) => Side::Ready(map(value)),
            Self::Loading => Side::Loading,
            Self::Absent => Side::Absent,
            Self::Failed(issue) => Side::Failed(*issue),
        }
    }
}

#[derive(Clone)]
pub(crate) struct Cancellation(Receiver<()>);

impl Cancellation {
    pub fn new() -> (Sender<()>, Self) {
        let (sender, receiver) = bounded(0);
        (sender, Self(receiver))
    }

    pub fn requested(&self) -> bool {
        self.0.try_recv() == Err(TryRecvError::Disconnected)
    }

    #[cfg(any(test, windows))]
    pub fn signal(&self) -> &Receiver<()> {
        &self.0
    }
}

pub(crate) struct Snapshot<'a> {
    pub root: &'a Path,
    pub path: &'a str,
    pub version: &'a Version,
    gitlink: bool,
    pub log: &'a ReadLog,
    pub cancel: &'a Cancellation,
}

impl<'a> Snapshot<'a> {
    pub fn new(
        root: &'a Path,
        pair: &'a Compare,
        column: Column,
        log: &'a ReadLog,
        cancel: &'a Cancellation,
    ) -> Self {
        let (version, path, gitlink) = match column {
            Column::Left => (&pair.left, pair.left_path.as_str(), pair.left_gitlink),
            Column::Right => (&pair.right, pair.right_path.as_str(), pair.right_gitlink),
        };
        Self {
            root,
            path,
            version,
            gitlink,
            log,
            cancel,
        }
    }

    pub fn read(&self) -> Result<Vec<u8>, Issue> {
        git::diff_snapshot_cancellable(
            self.root,
            self.version,
            self.path,
            self.gitlink,
            self.cancel,
        )
        .map_err(|error| {
            let ReadLog {
                worktree, request, ..
            } = *self.log;
            let (error_kind, exit_code) = git::error_kind(&error);
            if !self.cancel.requested() {
                tracing::warn!(
                    worktree,
                    request,
                    error_kind,
                    exit_code,
                    "diff_snapshot_failed"
                );
            }
            match error
                .downcast_ref::<std::io::Error>()
                .map(std::io::Error::kind)
            {
                Some(std::io::ErrorKind::NotFound) if matches!(self.version, Version::Worktree) => {
                    Issue::FileMissing
                }
                Some(std::io::ErrorKind::PermissionDenied) => Issue::AccessDenied,
                _ if error.is::<git::SnapshotLimit>() => Issue::SnapshotLimit,
                _ => Issue::Read,
            }
        })
    }
}

pub(crate) struct Display<'a, T, C> {
    pub sides: &'a [Side<Arc<T>>; 2],
    pub changes: Option<&'a C>,
}

pub(crate) trait DiffType: Default + 'static {
    type Data: Send + Sync + 'static;
    type Changes: Send + 'static;

    fn load(snapshot: Snapshot<'_>) -> Result<Self::Data, Issue>;

    fn compare(
        _sides: [Option<&Self::Data>; 2],
        _log: &ReadLog,
        _cancel: &Cancellation,
    ) -> Option<Self::Changes> {
        None
    }

    fn prepare(
        &mut self,
        _ctx: &egui::Context,
        _column: Column,
        _data: &Arc<Self::Data>,
    ) -> Result<(), Issue> {
        Ok(())
    }

    fn show(
        &mut self,
        ui: &mut egui::Ui,
        display: Display<'_, Self::Data, Self::Changes>,
        panel: &mut Panel<'_>,
    );
}

pub(crate) struct Registration {
    pub matches: fn(&Compare) -> bool,
    pub external_label: Option<&'static str>,
    pub raw_comparison: bool,
    start: fn(Request) -> (Box<dyn ActiveDiff>, Job),
}

impl Registration {
    const fn new<T: DiffType>(
        matches: fn(&Compare) -> bool,
        external_label: Option<&'static str>,
        raw_comparison: bool,
    ) -> Self {
        Self {
            matches,
            external_label,
            raw_comparison,
            start: start::<T>,
        }
    }

    pub fn start(&self, request: Request) -> (Box<dyn ActiveDiff>, Job) {
        (self.start)(request)
    }
}

static FORMATS: &[Registration] = &[
    Registration::new::<crate::ui_image::ImageView>(crate::image_diff::supports_pair, None, false),
    Registration::new::<crate::ui_workbook::SheetView>(
        crate::diff::is_workbook_pair,
        Some("Excel Diff ↗"),
        true,
    ),
];
static TEXT: Registration = Registration::new::<crate::ui_text::TextView>(|_| true, None, false);

pub(crate) fn registered(pair: &Compare) -> &'static Registration {
    FORMATS
        .iter()
        .find(|format| (format.matches)(pair))
        .unwrap_or(&TEXT)
}

pub(crate) fn text() -> &'static Registration {
    &TEXT
}

pub(crate) struct Request {
    pub root: PathBuf,
    pub pair: Compare,
    pub log: ReadLog,
    pub ctx: egui::Context,
}

type Job = Box<dyn FnOnce() + Send>;

pub(crate) trait ActiveDiff {
    fn poll(&mut self, ctx: &egui::Context);
    fn show(&mut self, ui: &mut egui::Ui, panel: &mut Panel<'_>);
    fn busy(&self) -> bool;
}

enum Update<T, C> {
    Side(Column, Side<Arc<T>>),
    Compared(Option<C>),
}

struct Delivery<T, C> {
    worktree: u64,
    request: u64,
    update: Update<T, C>,
}

struct Session<T: DiffType> {
    format: T,
    sides: [Side<Arc<T::Data>>; 2],
    changes: Option<T::Changes>,
    inbox: Receiver<Delivery<T::Data, T::Changes>>,
    _cancel_on_drop: Sender<()>,
    log: ReadLog,
    busy: bool,
}

impl<T: DiffType> ActiveDiff for Session<T> {
    fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(delivery) = self.inbox.try_recv() {
            self.apply(ctx, delivery);
        }
    }

    fn show(&mut self, ui: &mut egui::Ui, panel: &mut Panel<'_>) {
        ui.push_id(self.log.request, |ui| {
            self.format.show(
                ui,
                Display {
                    sides: &self.sides,
                    changes: self.changes.as_ref(),
                },
                panel,
            );
        });
    }

    fn busy(&self) -> bool {
        self.busy
    }
}

impl<T: DiffType> Session<T> {
    fn apply(&mut self, ctx: &egui::Context, delivery: Delivery<T::Data, T::Changes>) {
        let Delivery {
            worktree,
            request,
            update,
        } = delivery;
        if (worktree, request) != (self.log.worktree, self.log.request) {
            tracing::debug!(kind = "diff", worktree, request, "read_stale");
            return;
        }
        match update {
            Update::Side(column, mut side) => {
                if let Side::Ready(data) = &side
                    && let Err(issue) = self.format.prepare(ctx, column, data)
                {
                    tracing::warn!(worktree, request, ?column, ?issue, "diff_display_failed");
                    side = Side::Failed(issue);
                }
                self.sides[column.index()] = side;
            }
            Update::Compared(changes) => {
                self.changes = changes;
                self.busy = false;
                tracing::debug!(kind = "diff", worktree, request, "read_applied");
            }
        }
    }
}

fn start<T: DiffType>(request: Request) -> (Box<dyn ActiveDiff>, Job) {
    let (session, job) = create::<T>(request);
    (Box::new(session), job)
}

fn create<T: DiffType>(request: Request) -> (Session<T>, Job) {
    let (publish, inbox) = unbounded();
    let (cancel_tx, cancel) = Cancellation::new();
    let session = Session::<T> {
        format: T::default(),
        sides: [&request.pair.left, &request.pair.right].map(|version| {
            if matches!(version, Version::Empty) {
                Side::Absent
            } else {
                Side::Loading
            }
        }),
        changes: None,
        inbox,
        _cancel_on_drop: cancel_tx,
        log: request.log,
        busy: true,
    };
    let job = move || {
        let ctx = request.ctx.clone();
        let ReadLog {
            worktree,
            request: generation,
            ..
        } = request.log;
        run::<T>(&request, &cancel, |update| {
            if publish
                .send(Delivery {
                    worktree,
                    request: generation,
                    update,
                })
                .is_ok()
            {
                ctx.request_repaint();
            }
        });
    };
    (session, Box::new(job))
}

fn run<T: DiffType>(
    request: &Request,
    cancel: &Cancellation,
    publish: impl Fn(Update<T::Data, T::Changes>) + Sync,
) {
    let Request {
        root, pair, log, ..
    } = request;
    let started = Instant::now();
    let load = |column: Column| {
        if cancel.requested() {
            return Side::Loading;
        }
        let snapshot = Snapshot::new(root, pair, column, log, cancel);
        let side = if matches!(snapshot.version, Version::Empty) {
            Side::Absent
        } else {
            match T::load(snapshot) {
                Ok(data) => Side::Ready(Arc::new(data)),
                Err(issue) => {
                    if !cancel.requested() && !matches!(issue, Issue::Binary(_)) {
                        let ReadLog {
                            worktree, request, ..
                        } = *log;
                        tracing::warn!(worktree, request, ?column, ?issue, "diff_side_failed");
                    }
                    Side::Failed(issue)
                }
            }
        };
        if !cancel.requested() {
            publish(Update::Side(column, side.map(Arc::clone)));
        }
        side
    };
    let sides = std::thread::scope(|scope| {
        let left = scope.spawn(|| load(Column::Left));
        let right = load(Column::Right);
        [left.join().expect("left diff loader"), right]
    });
    if cancel.requested() {
        log.complete(started, "canceled");
        return;
    }
    let failed = sides.iter().any(|side| matches!(side, Side::Failed(_)));
    let changes = if failed {
        None
    } else {
        T::compare(
            sides.each_ref().map(|side| side.ready().map(Arc::as_ref)),
            log,
            cancel,
        )
    };
    if !cancel.requested() {
        publish(Update::Compared(changes));
    }
    let result = if cancel.requested() {
        "canceled"
    } else if failed {
        "failed"
    } else {
        "ok"
    };
    log.complete(started, result);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::{self, Kind, TextChanges, TextDocument};
    use crate::tasks::{ReadKind, Scheduler};
    use crate::ui_text::TextView;
    use std::sync::OnceLock;
    use std::time::Duration;

    const DEADLINE: Duration = Duration::from_secs(5);
    const COMPARE: usize = 2;

    struct Control {
        entered: Sender<usize>,
        release: [Receiver<()>; 3],
    }

    static CONTROL: OnceLock<Control> = OnceLock::new();

    fn gate(stage: usize, cancel: &Cancellation) -> Result<(), Issue> {
        let control = CONTROL.get().unwrap();
        control.entered.send(stage).unwrap();
        crossbeam_channel::select! {
            recv(control.release[stage]) -> signal => signal.map_err(|_| Issue::Read),
            recv(cancel.signal()) -> _ => Err(Issue::Read),
            default(DEADLINE) => panic!("diff stage {stage} was not released"),
        }
    }

    #[derive(Default)]
    struct GatedText;

    impl DiffType for GatedText {
        type Data = TextDocument;
        type Changes = TextChanges;

        fn load(snapshot: Snapshot<'_>) -> Result<Self::Data, Issue> {
            let column = if snapshot.path == "left.txt" {
                Column::Left
            } else {
                Column::Right
            };
            gate(column.index(), snapshot.cancel)?;
            diff::load_text(snapshot)
        }

        fn compare(
            sides: [Option<&Self::Data>; 2],
            log: &ReadLog,
            cancel: &Cancellation,
        ) -> Option<Self::Changes> {
            gate(COMPARE, cancel).ok()?;
            Some(diff::compare_text(sides, log))
        }

        fn show(
            &mut self,
            _: &mut egui::Ui,
            _: Display<'_, Self::Data, Self::Changes>,
            _: &mut Panel<'_>,
        ) {
        }
    }

    fn request(root: &Path, generation: u64) -> Request {
        Request {
            root: root.to_owned(),
            pair: Compare {
                left: Version::Worktree,
                right: Version::Worktree,
                base: Version::Empty,
                left_path: "left.txt".into(),
                right_path: "right.txt".into(),
                left_gitlink: false,
                right_gitlink: false,
            },
            log: ReadLog::queue("diff", root, generation),
            ctx: egui::Context::default(),
        }
    }

    fn schedule(scheduler: &Scheduler, root: &Path, generation: u64, job: Job) -> Receiver<()> {
        let (done, finished) = bounded(1);
        scheduler.latest(
            ReadKind::Diff,
            root,
            generation,
            Box::new(move || {
                job();
                done.send(()).unwrap();
            }),
        );
        finished
    }

    fn receive<T: DiffType>(session: &mut Session<T>) {
        let delivery = session.inbox.recv_timeout(DEADLINE).unwrap();
        session.apply(&egui::Context::default(), delivery);
    }

    #[test]
    fn builtin_diff_keeps_ready_sides_and_discards_superseded_generations() -> anyhow::Result<()> {
        let (entered, arrivals) = unbounded();
        let [left, right, compare] = core::array::from_fn(|_| bounded(1));
        let release = [left.0, right.0, compare.0];
        assert!(
            CONTROL
                .set(Control {
                    entered,
                    release: [left.1, right.1, compare.1]
                })
                .is_ok()
        );
        let loaders_started = || {
            let mut stages = [
                arrivals.recv_timeout(DEADLINE).unwrap(),
                arrivals.recv_timeout(DEADLINE).unwrap(),
            ];
            stages.sort();
            assert_eq!(stages, [Column::Left.index(), Column::Right.index()]);
        };

        let sandbox = tempfile::tempdir()?;
        let root = sandbox.path();
        std::fs::write(root.join("left.txt"), "context\nold\n")?;
        std::fs::write(root.join("right.txt"), "context\nnew\n")?;
        let scheduler = Scheduler::new();
        for (generation, first) in Column::BOTH.into_iter().enumerate() {
            let generation = generation as u64 + 1;
            let (mut session, job) = create::<GatedText>(request(root, generation));
            let finished = schedule(&scheduler, root, generation, job);
            loaders_started();
            let first = first.index();
            let other = 1 - first;
            release[first].send(())?;
            receive(&mut session);
            assert_eq!(session.sides[first].ready().unwrap().lines[0], "context");
            assert!(matches!(session.sides[other], Side::Loading));
            assert!(session.changes.is_none() && session.busy());

            release[other].send(())?;
            receive(&mut session);
            assert_eq!(arrivals.recv_timeout(DEADLINE)?, COMPARE);
            assert_eq!(session.sides[0].ready().unwrap().lines[1], "old");
            assert_eq!(session.sides[1].ready().unwrap().lines[1], "new");
            assert!(session.changes.is_none() && session.busy());

            release[COMPARE].send(())?;
            receive(&mut session);
            let changes = session.changes.as_ref().unwrap();
            assert_eq!((changes.added, changes.removed), (1, 1));
            assert_eq!(changes.lines[1].kind, Kind::Replace);
            assert!(!session.busy());
            finished.recv_timeout(DEADLINE)?;
        }

        std::fs::remove_file(root.join("left.txt"))?;
        let (mut failed, job) = create::<GatedText>(request(root, 3));
        let finished = schedule(&scheduler, root, 3, job);
        loaders_started();
        release[0].send(())?;
        receive(&mut failed);
        assert!(matches!(failed.sides[0], Side::Failed(Issue::FileMissing)));
        release[1].send(())?;
        receive(&mut failed);
        assert_eq!(failed.sides[1].ready().unwrap().lines[1], "new");
        receive(&mut failed);
        assert!(failed.changes.is_none() && !failed.busy());
        finished.recv_timeout(DEADLINE)?;

        let (pending, job) = create::<GatedText>(request(root, 4));
        let old_inbox = pending.inbox.clone();
        let finished = schedule(&scheduler, root, 4, job);
        loaders_started();
        drop(pending);
        let mut next = request(root, 5);
        next.pair.left = Version::Empty;
        let (mut current, job) = create::<TextView>(next);
        assert!(matches!(current.sides[0], Side::Absent));
        let current_done = schedule(&scheduler, root, 5, job);
        finished.recv_timeout(DEADLINE)?;
        assert_eq!(old_inbox.try_recv().err(), Some(TryRecvError::Disconnected));
        current_done.recv_timeout(DEADLINE)?;
        current.poll(&egui::Context::default());
        assert_eq!(current.changes.as_ref().unwrap().added, 2);
        assert_eq!(current.sides[1].ready().unwrap().lines[1], "new");

        std::fs::write(root.join("left.txt"), "old\n")?;
        let (mut comparing, job) = create::<GatedText>(request(root, 6));
        let old_inbox = comparing.inbox.clone();
        let finished = schedule(&scheduler, root, 6, job);
        loaders_started();
        release[0].send(())?;
        release[1].send(())?;
        receive(&mut comparing);
        receive(&mut comparing);
        assert_eq!(arrivals.recv_timeout(DEADLINE)?, COMPARE);
        let old_log = comparing.log;
        drop(comparing);

        let other_root = tempfile::tempdir()?;
        std::fs::write(other_root.path().join("left.txt"), "other\n")?;
        std::fs::write(other_root.path().join("right.txt"), "other\n")?;
        let (mut switched, job) = create::<TextView>(request(other_root.path(), 7));
        let switched_done = schedule(&scheduler, other_root.path(), 7, job);
        finished.recv_timeout(DEADLINE)?;
        assert_eq!(old_inbox.try_recv().err(), Some(TryRecvError::Disconnected));
        for (worktree, request) in [
            (switched.log.worktree, old_log.request),
            (old_log.worktree, switched.log.request),
        ] {
            switched.apply(
                &egui::Context::default(),
                Delivery {
                    worktree,
                    request,
                    update: Update::Side(Column::Left, Side::Failed(Issue::Decode)),
                },
            );
            switched.apply(
                &egui::Context::default(),
                Delivery {
                    worktree,
                    request,
                    update: Update::Compared(None),
                },
            );
        }
        assert!(matches!(switched.sides[0], Side::Loading) && switched.busy());
        switched_done.recv_timeout(DEADLINE)?;
        switched.poll(&egui::Context::default());
        assert_eq!(switched.sides[0].ready().unwrap().lines[0], "other");
        assert_eq!(switched.changes.as_ref().unwrap().added, 0);
        Ok(())
    }
}
