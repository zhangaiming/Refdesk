use crate::git::{self, Commit, History, HistoryScope, HistoryStart, LogReader, PAGE};
use crate::graph::{GraphBuilder, GraphRow};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::ops::Range;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tempfile::TempDir;

#[derive(Deserialize, Serialize)]
pub struct Row {
    pub commit: Commit,
    pub graph: GraphRow,
}

pub type Pages = BTreeMap<usize, Arc<Vec<Row>>>;

#[derive(Clone, Copy, Default)]
pub struct HistorySummary {
    pub loaded: usize,
    pub head_row: Option<usize>,
    pub lanes: usize,
    pub has_more: bool,
    pub elapsed_ms: u128,
}

impl HistorySummary {
    pub fn page_count(self) -> usize {
        self.loaded.div_ceil(PAGE)
    }
}

pub struct HistoryView {
    pub cache: Arc<HistoryCache>,
    pub summary: HistorySummary,
    pub pages: Pages,
    pub generation: u64,
}

impl HistoryView {
    pub fn ready_to_draw(&self) -> bool {
        self.summary.loaded == 0 || !self.pages.is_empty()
    }

    pub fn row(&self, index: usize) -> Option<&Row> {
        self.pages.get(&(index / PAGE))?.get(index % PAGE)
    }

    pub fn cached_rows(&self) -> impl Iterator<Item = (usize, &Row)> {
        self.pages.iter().flat_map(|(&page, rows)| {
            rows.iter()
                .enumerate()
                .map(move |(index, row)| (page * PAGE + index, row))
        })
    }
}

pub struct HistoryCache {
    pub info: History,
    dir: TempDir,
    start: HistoryStart,
    active: AtomicBool,
    progress: Mutex<Progress>,
    search: Mutex<Option<SearchIndex>>,
}

struct Progress {
    summary: HistorySummary,
    graph: GraphBuilder,
    reader: Option<LogReader>,
    lookahead: Option<Commit>,
    last_id: Option<String>,
}

#[derive(Clone, Copy)]
pub enum SearchDirection {
    Next,
    Previous,
}

pub struct SearchResult {
    pub count: usize,
    pub target: Option<usize>,
}

struct SearchIndex {
    query: String,
    file: File,
    indexed: usize,
    count: usize,
}

impl HistoryCache {
    pub fn open(root: &Path, scope: &HistoryScope, active: bool) -> Result<Arc<Self>> {
        let timer = Instant::now();
        let (info, start) = git::history_snapshot(root, scope)?;
        Ok(Arc::new(Self {
            info,
            dir: tempfile::tempdir().context("无法创建提交历史缓存")?,
            start,
            active: AtomicBool::new(active),
            progress: Mutex::new(Progress {
                summary: HistorySummary {
                    has_more: true,
                    elapsed_ms: timer.elapsed().as_millis(),
                    ..Default::default()
                },
                graph: GraphBuilder::default(),
                reader: None,
                lookahead: None,
                last_id: None,
            }),
            search: Mutex::new(None),
        }))
    }

    pub fn set_active(&self, active: bool) {
        self.active.store(active, Ordering::Release);
    }

    pub fn pause_if_inactive(&self) {
        let mut progress = self.progress.lock().expect("history cache");
        if !self.active.load(Ordering::Acquire) {
            progress.close_reader();
        }
    }

    pub fn load_through(&self, page_count: usize, canceled: &AtomicBool) -> Result<HistorySummary> {
        let mut progress = self.progress.lock().expect("history cache");
        let timer = Instant::now();
        let result = (|| {
            while progress.summary.has_more && progress.summary.page_count() < page_count {
                ensure!(!canceled.load(Ordering::Relaxed), "提交历史请求已取消");
                let commits = progress.read_page(&self.start)?;
                if commits.is_empty() {
                    progress.summary.has_more = false;
                    break;
                }
                let page = progress.summary.page_count();
                // Publish progress only after the page has been fully written.
                let mut graph = progress.graph.clone();
                let rows: Vec<_> = commits
                    .into_iter()
                    .map(|commit| Row {
                        graph: graph.push(&commit),
                        commit,
                    })
                    .collect();
                self.write_page(page, &rows)?;
                if progress.summary.head_row.is_none() {
                    progress.summary.head_row = rows
                        .iter()
                        .position(|row| self.info.head.as_ref() == Some(&row.commit.id))
                        .map(|index| page * PAGE + index);
                }
                progress.last_id = rows.last().map(|row| row.commit.id.clone());
                progress.summary.loaded += rows.len();
                progress.summary.lanes = graph.lanes;
                progress.summary.has_more = progress.lookahead.is_some();
                progress.graph = graph;
            }
            progress.summary.elapsed_ms += timer.elapsed().as_millis();
            Ok(progress.summary)
        })();
        if result.is_err() || !progress.summary.has_more || !self.active.load(Ordering::Acquire) {
            progress.close_reader();
        }
        result
    }

    pub fn window(
        &self,
        range: Range<usize>,
        summary: HistorySummary,
        mut pages: Pages,
    ) -> Result<Pages> {
        pages.retain(|page, _| range.contains(page));
        for page in range.start..range.end.min(summary.page_count()) {
            if let std::collections::btree_map::Entry::Vacant(entry) = pages.entry(page) {
                entry.insert(Arc::new(self.read_page(page)?));
            }
        }
        Ok(pages)
    }

    fn write_page(&self, page: usize, rows: &[Row]) -> Result<()> {
        let mut output = BufWriter::new(File::create(self.dir.path().join(page.to_string()))?);
        serde_json::to_writer(&mut output, rows)?;
        output.flush()?;
        Ok(())
    }

    fn read_page(&self, page: usize) -> Result<Vec<Row>> {
        let read = || -> Result<Vec<Row>> {
            let file = File::open(self.dir.path().join(page.to_string()))?;
            Ok(serde_json::from_reader(BufReader::new(file))?)
        };
        read().with_context(|| format!("无法读取第 {} 页提交历史", page + 1))
    }

    pub fn position(&self, loaded: usize, id: &str) -> Result<Option<usize>> {
        for page in 0..loaded.div_ceil(PAGE) {
            if let Some(index) = self
                .read_page(page)?
                .iter()
                .position(|row| row.commit.id == id)
            {
                return Ok(Some(page * PAGE + index));
            }
        }
        Ok(None)
    }

    pub fn search(
        &self,
        query: &str,
        loaded: usize,
        anchor: Option<usize>,
        direction: Option<SearchDirection>,
        canceled: &AtomicBool,
    ) -> Result<SearchResult> {
        let mut search = self.search.lock().expect("history search");
        if search
            .as_ref()
            .is_none_or(|index| index.query != query || index.indexed > loaded)
        {
            *search = Some(SearchIndex {
                query: query.into(),
                file: tempfile::tempfile()?,
                indexed: 0,
                count: 0,
            });
        }
        let index = search.as_mut().expect("search index");
        while index.indexed < loaded {
            ensure!(!canceled.load(Ordering::Relaxed), "提交搜索已取消");
            let rows = self.read_page(index.indexed / PAGE)?;
            let first = index.indexed % PAGE;
            let count = (rows.len() - first).min(loaded - index.indexed);
            ensure!(count > 0, "提交搜索缓存页不完整");
            let mut matching = Vec::new();
            for (offset, row) in rows[first..first + count].iter().enumerate() {
                if matches_search(&row.commit, &self.info, query) {
                    matching.extend_from_slice(&((index.indexed + offset) as u64).to_le_bytes());
                }
            }
            index.file.seek(SeekFrom::Start(index.count as u64 * 8))?;
            index.file.write_all(&matching)?;
            index.count += matching.len() / 8;
            index.indexed += count;
        }
        let target = match direction {
            _ if index.count == 0 => None,
            Some(SearchDirection::Next) => {
                let at = anchor.map_or(Ok(0), |row| index.bound(row, true))? % index.count;
                Some(index.row(at)?)
            }
            Some(SearchDirection::Previous) => {
                let at = anchor.map_or(Ok(0), |row| index.bound(row, false))?;
                Some(index.row(if at == 0 { index.count - 1 } else { at - 1 })?)
            }
            None => None,
        };
        Ok(SearchResult {
            count: index.count,
            target,
        })
    }
}

impl Progress {
    fn read_page(&mut self, start: &HistoryStart) -> Result<Vec<Commit>> {
        if self.reader.is_none() {
            let Some(mut reader) = start.open(self.summary.loaded.saturating_sub(1))? else {
                return Ok(Vec::new());
            };
            if let Some(last_id) = &self.last_id {
                let boundary = reader.next_commit()?;
                ensure!(
                    boundary.is_some_and(|commit| commit.id == *last_id),
                    "Git 历史快照已改变，请刷新"
                );
            }
            self.reader = Some(reader);
        }
        let reader = self.reader.as_mut().expect("history reader");
        let mut commits = Vec::with_capacity(PAGE);
        commits.extend(self.lookahead.take());
        while commits.len() < PAGE {
            let Some(commit) = reader.next_commit()? else {
                return Ok(commits);
            };
            commits.push(commit);
        }
        self.lookahead = reader.next_commit()?;
        Ok(commits)
    }

    fn close_reader(&mut self) {
        self.reader = None;
        self.lookahead = None;
    }
}

impl SearchIndex {
    fn row(&mut self, index: usize) -> Result<usize> {
        self.file.seek(SeekFrom::Start(index as u64 * 8))?;
        let mut bytes = [0; 8];
        self.file.read_exact(&mut bytes)?;
        Ok(u64::from_le_bytes(bytes) as usize)
    }

    fn bound(&mut self, row: usize, inclusive: bool) -> Result<usize> {
        let (mut start, mut end) = (0, self.count);
        while start < end {
            let mid = start + (end - start) / 2;
            let entry = self.row(mid)?;
            if entry < row || (inclusive && entry == row) {
                start = mid + 1;
            } else {
                end = mid;
            }
        }
        Ok(start)
    }
}

pub fn matches_search(commit: &Commit, history: &History, needle: &str) -> bool {
    commit.subject.to_lowercase().contains(needle)
        || commit.author.to_lowercase().contains(needle)
        || commit.id.contains(needle)
        || history.refs.iter().any(|reference| {
            reference.target == commit.id && reference.label().to_lowercase().contains(needle)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::Span;
    use std::process::{Command, Output, Stdio};

    const MAIN_COMMITS: usize = 3205;

    #[test]
    fn history_pages_continue_past_3000_and_restore_evicted_graph_rows() -> Result<()> {
        let repo = tempfile::tempdir()?;
        make_history(repo.path()).context("create paging fixture")?;
        let cache = HistoryCache::open(repo.path(), &HistoryScope::Head, true)?;
        let canceled = AtomicBool::new(false);
        let first = cache
            .load_through(1, &canceled)
            .context("load first page")?;
        assert_eq!(first.loaded, PAGE);
        assert_eq!(first.head_row, Some(0));
        assert!(first.has_more);
        let mut pages = cache.window(0..2, first, Pages::new())?;
        let merge = &pages[&0][0];
        let first_id = merge.commit.id.clone();
        let first_graph = merge.graph.clone();
        let merge_side = merge
            .graph
            .rails
            .iter()
            .filter(|rail| rail.start == Span::Middle)
            .nth(1)
            .unwrap();
        let (side_lane, side_color) = (merge_side.to, merge_side.color);

        // A failed page write leaves the preceding page readable and retryable.
        std::fs::create_dir(cache.dir.path().join("1"))?;
        assert!(cache.load_through(2, &canceled).is_err());
        assert_eq!(cache.progress.lock().unwrap().summary.loaded, PAGE);
        std::fs::remove_dir(cache.dir.path().join("1"))?;
        let second = cache
            .load_through(2, &canceled)
            .context("retry second page")?;
        pages = cache.window(0..3, second, pages)?;
        assert!(
            pages[&1][0]
                .graph
                .rails
                .iter()
                .any(|rail| rail.start == Span::Top
                    && rail.from == side_lane
                    && rail.color == side_color)
        );

        cache.set_active(false);
        cache.pause_if_inactive();
        assert!(cache.progress.lock().unwrap().reader.is_none());
        // Changing HEAD while inactive must not splice a different log into this snapshot.
        git(repo.path(), &["reset", "--soft", "HEAD^"])?;
        cache.set_active(true);
        let mut summary = second;
        while summary.has_more {
            let page = summary.page_count();
            summary = cache
                .load_through(page + 1, &canceled)
                .with_context(|| format!("load page {}", page + 1))?;
            pages = cache.window(page.saturating_sub(1)..page + 2, summary, pages)?;
            assert!(pages.len() <= 3);
        }
        assert_eq!(summary.loaded, MAIN_COMMITS + 1);
        assert_eq!(cache.load_through(100, &canceled)?.loaded, summary.loaded);
        assert!(!pages.contains_key(&0));

        let result = cache.search(
            "checkpoint",
            summary.loaded,
            None,
            Some(SearchDirection::Previous),
            &canceled,
        )?;
        assert_eq!(result.count, 2);
        assert_eq!(result.target, Some(MAIN_COMMITS));
        let result = cache.search(
            "checkpoint",
            summary.loaded,
            result.target,
            Some(SearchDirection::Next),
            &canceled,
        )?;
        assert_eq!(result.target, Some(0));
        let restored = cache.window(0..2, summary, pages)?;
        assert_eq!(restored.len(), 2);
        assert_eq!(restored[&0][0].commit.id, first_id);
        assert_eq!(restored[&0][0].graph, first_graph);

        let expected = git(
            repo.path(),
            &["log", "--date-order", "--format=%H", &first_id, "--"],
        )?;
        let expected = String::from_utf8(expected.stdout)?;
        let mut expected = expected.lines();
        for page in 0..summary.page_count() {
            for row in cache.read_page(page)? {
                assert_eq!(Some(row.commit.id.as_str()), expected.next());
            }
        }
        assert_eq!(expected.next(), None);

        let scope =
            HistoryScope::Selected(std::collections::BTreeSet::from(
                ["refs/heads/short".into()],
            ));
        let exact = HistoryCache::open(repo.path(), &scope, false)?.load_through(1, &canceled)?;
        assert_eq!(exact.loaded, PAGE);
        assert!(!exact.has_more);
        Ok(())
    }

    fn make_history(root: &Path) -> Result<()> {
        git(root, &["init", "--initial-branch=main"])?;
        let mut child = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["fast-import", "--quiet"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()?;
        {
            let mut input = BufWriter::new(child.stdin.take().unwrap());
            for index in 1..MAIN_COMMITS {
                let message = if index == 1 {
                    "checkpoint root"
                } else {
                    "main history"
                };
                writeln!(
                    input,
                    "commit refs/heads/main\nmark :{index}\ncommitter Test <git@example.invalid> {} +0000\ndata {}\n{message}\n",
                    1_700_000_000 + index * 10,
                    message.len()
                )?;
                if index == PAGE {
                    writeln!(input, "reset refs/heads/short\nfrom :{index}\n")?;
                }
            }
            write!(
                input,
                "commit refs/heads/side\nmark :4000\ncommitter Test <git@example.invalid> 1700028005 +0000\ndata 4\nside\nfrom :2800\n\n"
            )?;
            write!(
                input,
                "commit refs/heads/main\ncommitter Test <git@example.invalid> 1700032050 +0000\ndata 16\ncheckpoint merge\nmerge :4000\n\ndone\n"
            )?;
            input.flush()?;
        }
        checked(child.wait_with_output()?)?;
        Ok(())
    }

    fn git(root: &Path, args: &[&str]) -> Result<Output> {
        checked(
            Command::new("git")
                .arg("-C")
                .arg(root)
                .args(args)
                .output()?,
        )
    }

    fn checked(output: Output) -> Result<Output> {
        ensure!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(output)
    }
}
