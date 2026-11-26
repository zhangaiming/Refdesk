use crate::logging;
use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};

type Task = Box<dyn FnOnce() + Send>;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ReadKind {
    History,
    HistorySearch,
    Status,
    Details,
    Diff,
}

impl ReadKind {
    fn name(self) -> &'static str {
        match self {
            Self::History => "history",
            Self::HistorySearch => "history_search",
            Self::Status => "status",
            Self::Details => "details",
            Self::Diff => "diff",
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct ReadKey {
    kind: ReadKind,
    root: PathBuf,
}

struct ReadTask {
    key: Option<ReadKey>,
    request: u64,
    task: Task,
}

#[derive(Default)]
struct Queue {
    active: Option<PathBuf>,
    reads: VecDeque<ReadTask>,
    running: HashSet<ReadKey>,
    writes: VecDeque<Task>,
    closed: bool,
}

impl Queue {
    fn pop_read(&mut self) -> Option<ReadTask> {
        let mut available = self.reads.iter().enumerate().filter(|(_, job)| {
            job.key
                .as_ref()
                .is_none_or(|key| !self.running.contains(key))
        });
        let first = available.next()?;
        let foreground = |job: &ReadTask| {
            job.key
                .as_ref()
                .is_none_or(|key| self.active.as_ref() == Some(&key.root))
        };
        let index = if foreground(first.1) {
            first.0
        } else {
            available
                .find(|(_, job)| foreground(job))
                .map_or(first.0, |(index, _)| index)
        };
        let job = self.reads.remove(index).expect("queued read");
        if let Some(key) = &job.key {
            self.running.insert(key.clone());
        }
        Some(job)
    }
}

#[derive(Default)]
struct Shared {
    queue: Mutex<Queue>,
    read_ready: Condvar,
    write_ready: Condvar,
}

pub struct Scheduler(Arc<Shared>);

impl Scheduler {
    pub fn new() -> Self {
        let shared = Arc::new(Shared::default());
        for index in 0..3 {
            let shared = Arc::clone(&shared);
            std::thread::Builder::new()
                .name(format!("refdesk-read-{index}"))
                .spawn(move || shared.read())
                .expect("read worker");
        }
        let writer = Arc::clone(&shared);
        std::thread::Builder::new()
            .name("refdesk-write".into())
            .spawn(move || writer.write())
            .expect("write worker");
        Self(shared)
    }

    pub fn set_active(&self, root: Option<PathBuf>) {
        let mut queue = self.0.queue.lock().expect("task queue");
        queue.active = root;
    }

    pub fn read(&self, task: Task) {
        let mut queue = self.0.queue.lock().expect("task queue");
        queue.reads.push_back(ReadTask {
            key: None,
            request: 0,
            task,
        });
        self.0.read_ready.notify_one();
    }

    pub fn latest(&self, kind: ReadKind, root: &Path, request: u64, task: Task) {
        let key = ReadKey {
            kind,
            root: root.to_owned(),
        };
        let mut queue = self.0.queue.lock().expect("task queue");
        queue.reads.retain(|job| {
            let replaced = job.key.as_ref() == Some(&key);
            if replaced {
                tracing::debug!(
                    kind = kind.name(),
                    worktree = logging::worktree_id(root),
                    request = job.request,
                    replacement = request,
                    "read_superseded"
                );
            }
            !replaced
        });
        queue.reads.push_back(ReadTask {
            key: Some(key),
            request,
            task,
        });
        self.0.read_ready.notify_one();
    }

    pub fn cancel(&self, root: &Path, kind: Option<ReadKind>) {
        let mut queue = self.0.queue.lock().expect("task queue");
        queue.reads.retain(|job| {
            let Some(key) = &job.key else {
                return true;
            };
            let canceled = key.root == root && kind.is_none_or(|kind| key.kind == kind);
            if canceled {
                tracing::debug!(
                    kind = key.kind.name(),
                    worktree = logging::worktree_id(root),
                    request = job.request,
                    "read_canceled"
                );
            }
            !canceled
        });
    }

    pub fn write(&self, task: Task) {
        let mut queue = self.0.queue.lock().expect("task queue");
        queue.writes.push_back(task);
        self.0.write_ready.notify_one();
    }
}

impl Drop for Scheduler {
    fn drop(&mut self) {
        let mut queue = self.0.queue.lock().expect("task queue");
        queue.closed = true;
        queue.reads.clear();
        self.0.read_ready.notify_all();
        self.0.write_ready.notify_one();
    }
}

impl Shared {
    fn read(&self) {
        loop {
            let mut queue = self.queue.lock().expect("task queue");
            let job = loop {
                if let Some(job) = queue.pop_read() {
                    break job;
                }
                if queue.closed {
                    return;
                }
                queue = self.read_ready.wait(queue).expect("task queue");
            };
            drop(queue);
            (job.task)();
            if let Some(key) = job.key {
                self.queue.lock().expect("task queue").running.remove(&key);
                self.read_ready.notify_one();
            }
        }
    }

    fn write(&self) {
        loop {
            let mut queue = self.queue.lock().expect("task queue");
            let task = loop {
                if let Some(task) = queue.writes.pop_front() {
                    break task;
                }
                if queue.closed {
                    return;
                }
                queue = self.write_ready.wait(queue).expect("task queue");
            };
            drop(queue);
            task();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::{bounded, unbounded};
    use std::time::Duration;

    #[test]
    fn writes_bypass_busy_readers_and_active_reads_replace_queued_work() {
        let scheduler = Scheduler::new();
        let (started, events) = unbounded();
        let (release, blocked) = bounded::<()>(0);
        let timeout = Duration::from_secs(5);

        for _ in 0..3 {
            let started = started.clone();
            let blocked = blocked.clone();
            scheduler.read(Box::new(move || {
                started.send("busy").unwrap();
                let _ = blocked.recv();
            }));
        }
        for _ in 0..3 {
            assert_eq!(events.recv_timeout(timeout).unwrap(), "busy");
        }

        let active = PathBuf::from("active");
        let background = Path::new("background");
        let enqueue = |kind, path: &Path, marker| {
            let started = started.clone();
            scheduler.latest(
                kind,
                path,
                0,
                Box::new(move || started.send(marker).unwrap()),
            );
        };
        enqueue(ReadKind::History, background, "prefetch");
        enqueue(ReadKind::Diff, &active, "superseded");
        enqueue(ReadKind::Diff, &active, "latest");
        enqueue(ReadKind::Details, &active, "cancelled");
        scheduler.cancel(&active, Some(ReadKind::Details));
        scheduler.set_active(Some(active));
        let writer = started.clone();
        scheduler.write(Box::new(move || writer.send("write").unwrap()));

        assert_eq!(events.recv_timeout(timeout).unwrap(), "write");
        release.send_timeout((), timeout).unwrap();
        assert_eq!(events.recv_timeout(timeout).unwrap(), "latest");
        assert_eq!(events.recv_timeout(timeout).unwrap(), "prefetch");
        assert!(events.try_recv().is_err());
    }
}
