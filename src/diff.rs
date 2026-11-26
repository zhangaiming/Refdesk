use anyhow::{Context, Result, ensure};
use calamine::{Data, DataRef, Reader, Sheets, open_workbook_auto_from_rs};
use serde::{Deserialize, Serialize};
use similar::{Algorithm, DiffTag, TextDiff};
use std::collections::BTreeMap;
use std::io::Cursor;
use std::ops::Range;
use std::path::Path;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use crate::builtin_diff::{Cancellation, Issue, Snapshot};
use crate::git::Compare;
use crate::logging::{self, ReadLog};
use crate::syntax::{self, Highlight};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum Kind {
    Equal,
    Add,
    Remove,
    Replace,
}

#[derive(Debug)]
pub struct Line {
    pub old: Option<(usize, String)>,
    pub new: Option<(usize, String)>,
    pub kind: Kind,
}

#[derive(Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum CellValues {
    Same(String),
    Added(String),
    Removed(String),
    Changed(String, String),
}

impl CellValues {
    pub fn sides(&self) -> (Option<&str>, Option<&str>) {
        match self {
            Self::Same(value) => (Some(value), Some(value)),
            Self::Added(value) => (None, Some(value)),
            Self::Removed(value) => (Some(value), None),
            Self::Changed(old, new) => (Some(old), Some(new)),
        }
    }

    pub fn is_changed(&self) -> bool {
        !matches!(self, Self::Same(_))
    }
}

pub type CellPosition = (u32, u32);

#[derive(Debug, Deserialize, Serialize)]
pub struct WorksheetDiff {
    pub name: String,
    pub rows: u32,
    pub columns: u32,
    pub kind: Kind,
    cells: Vec<(CellPosition, CellValues)>,
    changes: Vec<usize>,
}

impl WorksheetDiff {
    pub fn cell(&self, position: CellPosition) -> Option<&CellValues> {
        self.cells
            .binary_search_by_key(&position, |(position, _)| *position)
            .ok()
            .map(|index| &self.cells[index].1)
    }

    pub fn change(&self, index: usize) -> Option<CellPosition> {
        self.changes.get(index).map(|&index| self.cells[index].0)
    }

    pub fn change_count(&self) -> usize {
        self.changes.len()
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WorkbookDiff {
    pub sheets: Vec<WorksheetDiff>,
    pub total: usize,
}

pub(crate) struct TextDocument {
    text: String,
    pub lines: Vec<String>,
    pub syntax: OnceLock<Highlight>,
}

impl TextDocument {
    pub fn parse(bytes: &[u8]) -> std::result::Result<Self, Issue> {
        let text = decode(bytes).ok_or(Issue::Binary(bytes.len()))?;
        if bytes.len() > 2 * 1024 * 1024 {
            return Err(Issue::PreviewLimit);
        }
        let text = text.replace("\r\n", "\n");
        let source_lines = text.split_inclusive(['\n', '\r']);
        if source_lines.clone().count() > 30_000 {
            return Err(Issue::PreviewLimit);
        }
        let lines = source_lines.map(display_line).collect();
        Ok(Self {
            text,
            lines,
            syntax: OnceLock::new(),
        })
    }

    pub fn highlight(&self, path: &Path, cancel: &Cancellation) -> bool {
        syntax::highlight(path, &self.text, || cancel.requested())
            .is_some_and(|syntax| !cancel.requested() && self.syntax.set(syntax).is_ok())
    }
}

type InlineChanges = [Vec<Range<usize>>; 2];

pub(crate) struct TextChanges {
    pub lines: Vec<Line>,
    pub added: usize,
    pub removed: usize,
    pub inline: BTreeMap<usize, InlineChanges>,
}

pub(crate) fn load_text(snapshot: Snapshot<'_>) -> std::result::Result<TextDocument, Issue> {
    TextDocument::parse(&snapshot.read()?)
}

fn display_line(text: &str) -> String {
    text.trim_end_matches(['\n', '\r']).replace('\t', "    ")
}

pub fn is_workbook(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "xlsx" | "xlsm" | "xls" | "xlsb" | "ods"
            )
        })
}

pub(crate) fn is_workbook_pair(pair: &Compare) -> bool {
    !pair.left_gitlink && !pair.right_gitlink && is_workbook(&pair.right_path)
}

pub(crate) fn compare_text(
    [left, right]: [Option<&TextDocument>; 2],
    log: &ReadLog,
    cancel: &Cancellation,
) -> TextChanges {
    let (worktree, request) = (log.worktree, log.request);
    let old_text = left.map_or("", |document| document.text.as_str());
    let new_text = right.map_or("", |document| document.text.as_str());
    let budget = Duration::from_millis(750);
    let started = Instant::now();
    let result = TextDiff::configure()
        .algorithm(Algorithm::Patience)
        .timeout(budget)
        .diff_lines(old_text, new_text);
    let elapsed = started.elapsed();
    if elapsed >= budget {
        // similar returns a diff even when it reaches its internal deadline.
        tracing::warn!(
            worktree,
            request,
            elapsed_ms = logging::millis(elapsed),
            "text_diff_budget_reached"
        );
    }
    let mut lines = Vec::new();
    let mut added = 0;
    let mut removed = 0;
    let old = result.old_slices();
    let new = result.new_slices();
    for operation in result.ops() {
        let before = operation.old_range();
        let after = operation.new_range();
        let kind = match operation.tag() {
            DiffTag::Equal => Kind::Equal,
            DiffTag::Insert => Kind::Add,
            DiffTag::Delete => Kind::Remove,
            DiffTag::Replace => Kind::Replace,
        };
        if kind != Kind::Equal {
            added += after.len();
            removed += before.len();
        }
        for offset in 0..before.len().max(after.len()) {
            let left = (offset < before.len()).then(|| {
                let index = before.start + offset;
                (index + 1, display_line(old[index]))
            });
            let right = (offset < after.len()).then(|| {
                let index = after.start + offset;
                (index + 1, display_line(new[index]))
            });
            let kind = match (&left, &right) {
                (None, Some(_)) => Kind::Add,
                (Some(_), None) => Kind::Remove,
                _ => kind,
            };
            lines.push(Line {
                old: left,
                new: right,
                kind,
            });
        }
    }
    let inline = inline_changes(&lines, Instant::now() + Duration::from_millis(80), cancel);
    TextChanges {
        lines,
        added,
        removed,
        inline,
    }
}

const MAX_INLINE_LINE_BYTES: usize = 1024;

fn inline_changes(
    lines: &[Line],
    deadline: Instant,
    cancel: &Cancellation,
) -> BTreeMap<usize, InlineChanges> {
    let mut inline = BTreeMap::new();
    for (index, line) in lines.iter().enumerate() {
        if line.kind != Kind::Replace {
            continue;
        }
        if cancel.requested() || Instant::now() >= deadline {
            break;
        }
        if let (Some((_, old)), Some((_, new))) = (&line.old, &line.new)
            && let Some(changes) = inline_line(old, new, deadline)
            && Instant::now() < deadline
        {
            inline.insert(index, changes);
        }
    }
    inline
}

fn inline_line(old: &str, new: &str, deadline: Instant) -> Option<InlineChanges> {
    if old == new || old.len().max(new.len()) > MAX_INLINE_LINE_BYTES {
        return None;
    }
    let diff = TextDiff::configure()
        .deadline(deadline)
        .diff_graphemes(old, new);
    let mut changes: InlineChanges = Default::default();
    let mut offsets = [0; 2];
    let mut common_text = false;
    let slices = [diff.old_slices(), diff.new_slices()];
    for op in diff.ops() {
        for (side, range) in [op.old_range(), op.new_range()].into_iter().enumerate() {
            let parts = &slices[side][range];
            let start = offsets[side];
            let end = start + parts.iter().map(|part| part.len()).sum::<usize>();
            offsets[side] = end;
            if op.tag() == DiffTag::Equal {
                common_text |= parts
                    .iter()
                    .any(|part| part.chars().any(|ch| !ch.is_whitespace()));
            } else if start < end {
                changes[side].push(start..end);
            }
        }
    }
    common_text.then_some(changes)
}

pub fn encoding(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(&[0xff, 0xfe]) {
        "UTF-16LE"
    } else if bytes.starts_with(&[0xfe, 0xff]) {
        "UTF-16BE"
    } else {
        "UTF-8"
    }
}

fn decode(bytes: &[u8]) -> Option<String> {
    if let Some(rest) = bytes.strip_prefix(&[0xff, 0xfe]) {
        return Some(String::from_utf16_lossy(
            &rest
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect::<Vec<_>>(),
        ));
    }
    if let Some(rest) = bytes.strip_prefix(&[0xfe, 0xff]) {
        return Some(String::from_utf16_lossy(
            &rest
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
                .collect::<Vec<_>>(),
        ));
    }
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
    if bytes
        .iter()
        .take(8192)
        .any(|byte| matches!(*byte, 0..=8 | 14..=31))
    {
        return None;
    }
    std::str::from_utf8(bytes).ok().map(str::to_owned)
}

#[derive(Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
struct Cell {
    value: String,
    typed: String,
    formula: String,
}

impl Cell {
    fn display(&self) -> std::borrow::Cow<'_, str> {
        if self.formula.is_empty() {
            return std::borrow::Cow::Borrowed(&self.value);
        }
        if self.value.is_empty() {
            return format!("={}", self.formula).into();
        }
        format!("={}  →  {}", self.formula, self.value).into()
    }
}

const MAX_USED_CELLS: usize = 300_000;
type WorksheetCells = BTreeMap<CellPosition, Cell>;

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct Worksheet {
    pub name: String,
    pub rows: u32,
    pub columns: u32,
    cells: Vec<(CellPosition, Cell)>,
}

impl Worksheet {
    fn new(name: String, cells: WorksheetCells) -> Self {
        let rows = cells
            .last_key_value()
            .map_or(0, |(&(row, _), _)| row.saturating_add(1));
        let columns = cells
            .keys()
            .map(|&(_, col)| col.saturating_add(1))
            .max()
            .unwrap_or_default();
        Self {
            name,
            rows,
            columns,
            cells: cells.into_iter().collect(),
        }
    }

    pub fn cell(&self, position: CellPosition) -> Option<std::borrow::Cow<'_, str>> {
        self.cells
            .binary_search_by_key(&position, |(position, _)| *position)
            .ok()
            .map(|index| self.cells[index].1.display())
    }

    pub fn first_cell(&self) -> Option<CellPosition> {
        self.cells.first().map(|(position, _)| *position)
    }
}

pub(crate) type Workbook = Vec<Worksheet>;

pub(crate) enum WorkbookSide {
    Book(Workbook),
    Pointer(TextDocument),
}

pub(crate) fn load_workbook(snapshot: Snapshot<'_>) -> std::result::Result<WorkbookSide, Issue> {
    let bytes = snapshot.read()?;
    if bytes.starts_with(b"version https://git-lfs.github.com/spec/v1") {
        return TextDocument::parse(&bytes).map(WorkbookSide::Pointer);
    }
    #[cfg(windows)]
    let parsed = crate::workbook_worker::parse(&bytes, snapshot.log, snapshot.cancel);
    #[cfg(not(windows))]
    let parsed = parse_workbook(&bytes);
    parsed.map(WorkbookSide::Book)
}

#[derive(Debug)]
struct WorkbookLimit;

impl std::fmt::Display for WorkbookLimit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("工作簿超过 300,000 个已用单元格")
    }
}

impl std::error::Error for WorkbookLimit {}

#[derive(Default)]
struct SheetCells {
    cells: WorksheetCells,
    used: usize,
}

impl SheetCells {
    fn at(&mut self, position: (u32, u32)) -> Result<&mut Cell> {
        use std::collections::btree_map::Entry;
        match self.cells.entry(position) {
            Entry::Occupied(entry) => Ok(entry.into_mut()),
            Entry::Vacant(entry) => {
                ensure!(self.used < MAX_USED_CELLS, WorkbookLimit);
                self.used += 1;
                Ok(entry.insert(Cell::default()))
            }
        }
    }

    fn value(&mut self, position: (u32, u32), value: &Data) -> Result<()> {
        let cell = self.at(position)?;
        cell.value = value.to_string();
        cell.typed = format!("{value:?}");
        Ok(())
    }

    fn sparse_values<'a>(
        &mut self,
        mut next: impl FnMut() -> Result<Option<calamine::Cell<DataRef<'a>>>>,
    ) -> Result<()> {
        while let Some(cell) = next()? {
            if !matches!(cell.get_value(), DataRef::Empty) {
                self.value(cell.get_position(), &Data::from(cell.get_value().clone()))?;
            }
        }
        Ok(())
    }

    fn sparse_formulas(
        &mut self,
        mut next: impl FnMut() -> Result<Option<calamine::Cell<String>>>,
    ) -> Result<()> {
        while let Some(cell) = next()? {
            if !cell.get_value().is_empty() {
                self.at(cell.get_position())?
                    .formula
                    .clone_from(cell.get_value());
            }
        }
        Ok(())
    }

    fn dense(&mut self, workbook: &mut Sheets<Cursor<&[u8]>>, name: &str) -> Result<()> {
        let values = workbook.worksheet_range(name)?;
        let origin = values.start().unwrap_or((0, 0));
        for (row, col, value) in values.used_cells() {
            self.value((origin.0 + row as u32, origin.1 + col as u32), value)?;
        }
        if let Ok(formulas) = workbook.worksheet_formula(name) {
            let origin = formulas.start().unwrap_or((0, 0));
            for (row, col, formula) in formulas
                .used_cells()
                .filter(|(_, _, text)| !text.is_empty())
            {
                self.at((origin.0 + row as u32, origin.1 + col as u32))?
                    .formula
                    .clone_from(formula);
            }
        }
        Ok(())
    }
}

pub(crate) fn parse_workbook(bytes: &[u8]) -> std::result::Result<Workbook, Issue> {
    read_workbook(bytes).map_err(|error| {
        if error.is::<WorkbookLimit>() {
            Issue::PreviewLimit
        } else {
            Issue::Decode
        }
    })
}

fn read_workbook(bytes: &[u8]) -> Result<Workbook> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    let mut workbook =
        open_workbook_auto_from_rs(Cursor::new(bytes)).context("无法读取电子表格")?;
    let mut sheets = Vec::new();
    let mut cells_seen = 0;
    for name in workbook.sheet_names() {
        let mut sheet = SheetCells {
            used: cells_seen,
            ..SheetCells::default()
        };
        (|| -> Result<()> {
            match &mut workbook {
                Sheets::Xlsx(workbook) => {
                    let mut reader = workbook.worksheet_cells_reader(&name)?;
                    sheet.sparse_values(|| Ok(reader.next_cell()?))?;
                    drop(reader);
                    let mut reader = workbook.worksheet_cells_reader(&name)?;
                    sheet.sparse_formulas(|| Ok(reader.next_formula()?))?;
                }
                Sheets::Xlsb(workbook) => {
                    let mut reader = workbook.worksheet_cells_reader(&name)?;
                    sheet.sparse_values(|| Ok(reader.next_cell()?))?;
                    drop(reader);
                    let mut reader = workbook.worksheet_cells_reader(&name)?;
                    sheet.sparse_formulas(|| Ok(reader.next_formula()?))?;
                }
                _ => sheet.dense(&mut workbook, &name)?,
            }
            Ok(())
        })()
        .with_context(|| format!("无法读取工作表 {name}"))?;
        cells_seen = sheet.used;
        sheets.push(Worksheet::new(name, sheet.cells));
    }
    Ok(sheets)
}

pub(crate) fn compare_workbooks(
    [left, right]: [Option<&[Worksheet]>; 2],
    cancel: impl Fn() -> bool,
) -> Option<WorkbookDiff> {
    let left = left.unwrap_or_default();
    let right = right.unwrap_or_default();
    let mut remaining: BTreeMap<_, _> = left
        .iter()
        .map(|sheet| (sheet.name.as_str(), sheet))
        .collect();
    let mut sheets = Vec::new();
    for after in right {
        let before = remaining.remove(after.name.as_str());
        sheets.push(compare_worksheet(
            &after.name,
            before,
            Some(after),
            &cancel,
        )?);
    }
    for before in left {
        if remaining.contains_key(before.name.as_str()) {
            sheets.push(compare_worksheet(
                &before.name,
                Some(before),
                None,
                &cancel,
            )?);
        }
    }
    let total = sheets
        .iter()
        .map(|sheet| sheet.change_count() + usize::from(sheet.kind != Kind::Equal))
        .sum();
    Some(WorkbookDiff { sheets, total })
}

fn compare_worksheet(
    name: &str,
    before: Option<&Worksheet>,
    after: Option<&Worksheet>,
    cancel: impl Fn() -> bool,
) -> Option<WorksheetDiff> {
    let kind = match (&before, &after) {
        (None, _) => Kind::Add,
        (_, None) => Kind::Remove,
        _ => Kind::Equal,
    };
    let mut left = before.into_iter().flat_map(|sheet| &sheet.cells).peekable();
    let mut right = after.into_iter().flat_map(|sheet| &sheet.cells).peekable();
    let mut diff = WorksheetDiff {
        name: name.to_owned(),
        rows: 0,
        columns: 0,
        kind,
        cells: Vec::new(),
        changes: Vec::new(),
    };
    while let Some(position) = left
        .peek()
        .map(|(position, _)| *position)
        .into_iter()
        .chain(right.peek().map(|(position, _)| *position))
        .min()
    {
        if cancel() {
            return None;
        }
        let at_position = |entry: &&(CellPosition, Cell)| entry.0 == position;
        let old = left.next_if(at_position).map(|(_, cell)| cell);
        let new = right.next_if(at_position).map(|(_, cell)| cell);
        let values = match (old, new) {
            (Some(old), Some(new)) if old == new => CellValues::Same(new.display().into_owned()),
            (Some(old), Some(new)) => {
                CellValues::Changed(old.display().into_owned(), new.display().into_owned())
            }
            (None, Some(new)) => CellValues::Added(new.display().into_owned()),
            (Some(old), None) => CellValues::Removed(old.display().into_owned()),
            (None, None) => unreachable!("position came from an input"),
        };
        diff.rows = diff.rows.max(position.0.saturating_add(1));
        diff.columns = diff.columns.max(position.1.saturating_add(1));
        if values.is_changed() {
            diff.changes.push(diff.cells.len());
        }
        diff.cells.push((position, values));
    }
    Some(diff)
}

pub fn column_name(col: u32) -> String {
    let mut col = u64::from(col) + 1;
    let mut letters = Vec::new();
    while col > 0 {
        col -= 1;
        letters.push((b'A' + (col % 26) as u8) as char);
        col /= 26;
    }
    letters.into_iter().rev().collect()
}

pub fn address(row: u32, col: u32) -> String {
    format!("{}{row}", column_name(col), row = u64::from(row) + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_changes_keep_unicode_graphemes_and_handle_one_sided_edits() {
        let deadline = Instant::now() + Duration::from_secs(5);
        let old = "    战斗 👩‍💻 = 30";
        let new = "    战斗 👩‍🎨 = 30!";
        let [removed, added] = inline_line(old, new, deadline).unwrap();
        let slices = |text: &str, ranges: Vec<Range<usize>>| {
            ranges
                .into_iter()
                .map(|range| text[range].to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(slices(old, removed), ["👩‍💻"]);
        assert_eq!(slices(new, added), ["👩‍🎨", "!"]);

        let [removed, added] = inline_line("timeout = 30", "timeout = 60", deadline).unwrap();
        assert_eq!(slices("timeout = 30", removed), ["3"]);
        assert_eq!(slices("timeout = 60", added), ["6"]);
        assert!(inline_line("old", "NEW", deadline).is_none());
        assert!(inline_line("same", "same", deadline).is_none());
        assert!(inline_line(&"a".repeat(MAX_INLINE_LINE_BYTES + 1), "ab", deadline).is_none());
    }

    #[test]
    fn inline_changes_obey_the_comparison_budget_and_cancellation() {
        let lines = [Line {
            old: Some((1, "value 1".into())),
            new: Some((1, "value 2".into())),
            kind: Kind::Replace,
        }];
        let (sender, cancel) = Cancellation::new();
        assert!(inline_changes(&lines, Instant::now(), &cancel).is_empty());
        let deadline = Instant::now() + Duration::from_secs(5);
        let ranges = inline_changes(&lines, deadline, &cancel);
        assert_eq!(
            &lines[0].old.as_ref().unwrap().1[ranges[&0][0][0].clone()],
            "1"
        );
        drop(sender);
        assert!(inline_changes(&lines, deadline, &cancel).is_empty());
    }

    #[test]
    fn raw_lines_share_diff_line_boundaries_for_cr_and_crlf() {
        let document = TextDocument::parse(b"first\r\n\tsecond\rthird\n").unwrap();
        let diff = TextDiff::from_lines("", &document.text);
        let visible: Vec<_> = diff
            .new_slices()
            .iter()
            .map(|line| display_line(line))
            .collect();
        assert_eq!(document.lines, ["first", "    second", "third"]);
        assert_eq!(visible, document.lines);
    }
    #[test]
    fn absolute_excel_addresses_are_one_based() {
        assert_eq!(address(0, 0), "A1");
        assert_eq!(address(8, 26), "AA9");
        assert_eq!(address(2, 16_383), "XFD3");
    }

    #[test]
    fn utf16_text_and_binary_are_distinguished() {
        assert_eq!(decode(&[0xff, 0xfe, b'A', 0]).as_deref(), Some("A"));
        assert_eq!(decode(&[1, 0, 3]), None);
    }

    #[test]
    fn worksheet_keeps_context_and_absolute_change_positions() {
        let value = |text: &str| Cell {
            value: text.into(),
            ..Cell::default()
        };
        let before = Worksheet::new(
            "function".into(),
            WorksheetCells::from([
                ((0, 0), value("name")),
                ((356, 12), value("context")),
                ((357, 13), value("old")),
                ((359, 13), value("removed")),
            ]),
        );
        let after = Worksheet::new(
            "function".into(),
            WorksheetCells::from([
                ((0, 0), value("name")),
                ((356, 12), value("context")),
                ((357, 13), value("new")),
                (
                    (1500, 27),
                    Cell {
                        formula: "N358".into(),
                        ..Cell::default()
                    },
                ),
            ]),
        );
        let sheet = compare_worksheet("function", Some(&before), Some(&after), || false).unwrap();
        assert_eq!((sheet.rows, sheet.columns), (1501, 28));
        assert_eq!(
            sheet.cell((356, 12)).unwrap().sides(),
            (Some("context"), Some("context"))
        );
        assert!(sheet.cell((358, 13)).is_none());
        assert_eq!(sheet.change_count(), 3);
        assert_eq!(sheet.change(0), Some((357, 13)));
        assert_eq!(
            sheet.cell((357, 13)).unwrap().sides(),
            (Some("old"), Some("new"))
        );
        assert_eq!(sheet.change(1), Some((359, 13)));
        assert_eq!(
            sheet.cell((359, 13)).unwrap().sides(),
            (Some("removed"), None)
        );
        assert_eq!(sheet.change(2), Some((1500, 27)));
        assert_eq!(
            sheet.cell((1500, 27)).unwrap().sides(),
            (None, Some("=N358"))
        );
    }

    #[test]
    fn shared_value_and_formula_use_one_cell_of_the_workbook_budget() {
        let mut sheet = SheetCells {
            used: MAX_USED_CELLS - 1,
            ..SheetCells::default()
        };
        let position = (1023, 27);
        sheet.value(position, &Data::Int(2)).unwrap();
        sheet.at(position).unwrap().formula = "1+1".into();
        assert_eq!(
            sheet
                .cells
                .remove(&position)
                .unwrap()
                .display()
                .into_owned(),
            "=1+1  →  2"
        );
        assert!(sheet.value((0, 0), &Data::Int(3)).is_err());
    }

    #[test]
    fn parsed_snapshots_gain_workbook_changes_without_reparsing() {
        let old = parse_workbook(include_bytes!("../tests/fixtures/workbook/before.xlsx")).unwrap();
        let new = parse_workbook(include_bytes!("../tests/fixtures/workbook/after.xlsx")).unwrap();
        assert_eq!(old[0].cell((357, 13)).as_deref(), Some("12"));
        assert_eq!(new[0].cell((357, 13)).as_deref(), Some("18"));
        assert_eq!(new[0].cell((356, 12)).as_deref(), Some("Context"));
        let compared = compare_workbooks([Some(&old), Some(&new)], || false).unwrap();
        let [balance, notes] = compared.sheets.as_slice() else {
            panic!("expected Balance and Notes");
        };
        assert_eq!(balance.change(0), Some((357, 13)));
        assert_eq!(balance.change(1), Some((400, 17)));
        assert_eq!(
            balance.cell((400, 17)).unwrap().sides(),
            (Some("=N358*2  →  24"), Some("=N358*3  →  54"))
        );
        assert_eq!(notes.kind, Kind::Add);
        assert_eq!(notes.change(0), Some((3, 1)));
    }
}
