use anyhow::{Context, Result, ensure};
use calamine::{Data, DataRef, Reader, Sheets, open_workbook_auto_from_rs};
use serde::{Deserialize, Serialize};
use similar::{Algorithm, DiffTag, TextDiff};
use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::Path;
use std::time::{Duration, Instant};

use crate::git::{self, Compare};
use crate::logging::{self, ReadLog};

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

#[derive(Debug)]
pub enum Content {
    Text {
        lines: Vec<Line>,
        added: usize,
        removed: usize,
    },
    Sheet(WorkbookDiff),
    Binary {
        left: usize,
        right: usize,
    },
    Notice(String),
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

pub fn compare(root: &Path, pair: &Compare, force_text: bool, log: &ReadLog) -> Result<Content> {
    let (worktree, request) = (log.worktree, log.request);
    let snapshot = |version, path, gitlink| {
        git::diff_snapshot(root, version, path, gitlink).inspect_err(|error| {
            let (error_kind, exit_code) = git::error_kind(error);
            tracing::warn!(
                worktree,
                request,
                error_kind,
                exit_code,
                "diff_snapshot_failed"
            );
        })
    };
    let old = snapshot(&pair.left, pair.left_path.as_str(), pair.left_gitlink)?;
    let new = snapshot(&pair.right, pair.right_path.as_str(), pair.right_gitlink)?;
    let (left_bytes, right_bytes) = (old.len(), new.len());
    let lfs = |bytes: &[u8]| bytes.starts_with(b"version https://git-lfs.github.com/spec/v1\n");
    if !force_text
        && !pair.left_gitlink
        && !pair.right_gitlink
        && is_workbook(&pair.right_path)
        && !lfs(&old)
        && !lfs(&new)
    {
        #[cfg(windows)]
        let workbook = crate::workbook_worker::compare(&old, &new, log)?;
        #[cfg(not(windows))]
        let workbook = compare_workbooks(&old, &new).inspect_err(|_| {
            tracing::warn!(worktree, request, "workbook_compare_failed");
        })?;
        return Ok(Content::Sheet(workbook));
    }
    let (Some(old_text), Some(new_text)) = (decode(&old), decode(&new)) else {
        tracing::debug!(
            worktree,
            request,
            kind = "binary",
            left_bytes,
            right_bytes,
            "diff_input"
        );
        return Ok(Content::Binary {
            left: old.len(),
            right: new.len(),
        });
    };
    tracing::debug!(
        worktree,
        request,
        kind = "text",
        left_bytes,
        right_bytes,
        "diff_input"
    );
    if old.len().max(new.len()) > 2 * 1024 * 1024
        || old_text.lines().count().max(new_text.lines().count()) > 30_000
    {
        tracing::warn!(
            worktree,
            request,
            kind = "text",
            left_bytes,
            right_bytes,
            "diff_input_limit"
        );
        return Ok(Content::Notice(
            "文件超过内置文本比较的上限，可以交给外部 diff 工具。".into(),
        ));
    }
    let old_text = old_text.replace("\r\n", "\n");
    let new_text = new_text.replace("\r\n", "\n");
    let budget = Duration::from_millis(750);
    let started = Instant::now();
    let result = TextDiff::configure()
        .algorithm(Algorithm::Patience)
        .timeout(budget)
        .diff_lines(&old_text, &new_text);
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
    let line = |text: &str| text.trim_end_matches('\n').replace('\t', "    ");
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
                (index + 1, line(old[index]))
            });
            let right = (offset < after.len()).then(|| {
                let index = after.start + offset;
                (index + 1, line(new[index]))
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
    Ok(Content::Text {
        lines,
        added,
        removed,
    })
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

#[derive(Debug, Default, PartialEq, Eq)]
struct Cell {
    value: String,
    typed: String,
    formula: String,
}

impl Cell {
    fn into_display(self) -> String {
        if self.formula.is_empty() {
            return self.value;
        }
        if self.value.is_empty() {
            return format!("={}", self.formula);
        }
        format!("={}  →  {}", self.formula, self.value)
    }
}

const MAX_USED_CELLS: usize = 300_000;
type Worksheet = BTreeMap<(u32, u32), Cell>;
type Workbook = Vec<(String, Worksheet)>;

#[derive(Default)]
struct SheetCells {
    cells: Worksheet,
    used: usize,
}

impl SheetCells {
    fn at(&mut self, position: (u32, u32)) -> Result<&mut Cell> {
        use std::collections::btree_map::Entry;
        match self.cells.entry(position) {
            Entry::Occupied(entry) => Ok(entry.into_mut()),
            Entry::Vacant(entry) => {
                ensure!(
                    self.used < MAX_USED_CELLS,
                    "工作簿超过 300,000 个已用单元格，请使用外部工具"
                );
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
        sheets.push((name, sheet.cells));
    }
    Ok(sheets)
}

pub(crate) fn compare_workbooks(left: &[u8], right: &[u8]) -> Result<WorkbookDiff> {
    let left = read_workbook(left)?;
    let right = read_workbook(right)?;
    let left_order: Vec<_> = left.iter().map(|(name, _)| name.clone()).collect();
    let mut left: BTreeMap<_, _> = left.into_iter().collect();
    let mut sheets: Vec<_> = right
        .into_iter()
        .map(|(name, after)| {
            let before = left.remove(&name);
            compare_worksheet(name, before, Some(after))
        })
        .collect();
    sheets.extend(left_order.into_iter().filter_map(|name| {
        let before = left.remove(&name)?;
        Some(compare_worksheet(name, Some(before), None))
    }));
    let total = sheets
        .iter()
        .map(|sheet| sheet.change_count() + usize::from(sheet.kind != Kind::Equal))
        .sum();
    Ok(WorkbookDiff { sheets, total })
}

fn compare_worksheet(
    name: String,
    before: Option<Worksheet>,
    after: Option<Worksheet>,
) -> WorksheetDiff {
    let kind = match (&before, &after) {
        (None, _) => Kind::Add,
        (_, None) => Kind::Remove,
        _ => Kind::Equal,
    };
    let mut left = before.unwrap_or_default().into_iter().peekable();
    let mut right = after.unwrap_or_default().into_iter().peekable();
    let mut diff = WorksheetDiff {
        name,
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
        let at_position = |entry: &(CellPosition, Cell)| entry.0 == position;
        let old = left.next_if(at_position).map(|(_, cell)| cell);
        let new = right.next_if(at_position).map(|(_, cell)| cell);
        let values = match (old, new) {
            (Some(old), Some(new)) if old == new => CellValues::Same(new.into_display()),
            (Some(old), Some(new)) => CellValues::Changed(old.into_display(), new.into_display()),
            (None, Some(new)) => CellValues::Added(new.into_display()),
            (Some(old), None) => CellValues::Removed(old.into_display()),
            (None, None) => unreachable!("position came from an input"),
        };
        diff.rows = diff.rows.max(position.0.saturating_add(1));
        diff.columns = diff.columns.max(position.1.saturating_add(1));
        if values.is_changed() {
            diff.changes.push(diff.cells.len());
        }
        diff.cells.push((position, values));
    }
    diff
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
        let before = Worksheet::from([
            ((0, 0), value("name")),
            ((356, 12), value("context")),
            ((357, 13), value("old")),
            ((359, 13), value("removed")),
        ]);
        let after = Worksheet::from([
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
        ]);
        let sheet = compare_worksheet("function".into(), Some(before), Some(after));
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
            sheet.cells.remove(&position).unwrap().into_display(),
            "=1+1  →  2"
        );
        assert!(sheet.value((0, 0), &Data::Int(3)).is_err());
    }
}
