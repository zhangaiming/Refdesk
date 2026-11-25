use anyhow::{Context, Result, ensure};
use calamine::{Reader, open_workbook_auto_from_rs};
use similar::{Algorithm, DiffTag, TextDiff};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Cursor;
use std::path::Path;
use std::time::Duration;

use crate::git::{self, Compare};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

#[derive(Debug, PartialEq, Eq)]
pub struct CellChange {
    pub sheet: String,
    pub address: String,
    pub left: String,
    pub right: String,
    pub kind: Kind,
}

#[derive(Debug)]
pub enum Content {
    Text {
        lines: Vec<Line>,
        added: usize,
        removed: usize,
    },
    Sheet {
        cells: Vec<CellChange>,
        total: usize,
    },
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

pub fn compare(root: &Path, pair: &Compare, force_text: bool) -> Result<Content> {
    let old = git::snapshot(root, &pair.left, &pair.left_path, false)?;
    let new = git::snapshot(root, &pair.right, &pair.right_path, false)?;
    let lfs = |bytes: &[u8]| bytes.starts_with(b"version https://git-lfs.github.com/spec/v1\n");
    if !force_text && is_workbook(&pair.right_path) && !lfs(&old) && !lfs(&new) {
        let (cells, total) = compare_workbooks(&old, &new)?;
        return Ok(Content::Sheet { cells, total });
    }
    let (Some(old_text), Some(new_text)) = (decode(&old), decode(&new)) else {
        return Ok(Content::Binary {
            left: old.len(),
            right: new.len(),
        });
    };
    if old.len().max(new.len()) > 2 * 1024 * 1024
        || old_text.lines().count().max(new_text.lines().count()) > 30_000
    {
        return Ok(Content::Notice(
            "文件超过内置文本比较的上限，可以交给外部 diff 工具。".into(),
        ));
    }
    let old_text = old_text.replace("\r\n", "\n");
    let new_text = new_text.replace("\r\n", "\n");
    let result = TextDiff::configure()
        .algorithm(Algorithm::Patience)
        .timeout(Duration::from_millis(750))
        .diff_lines(&old_text, &new_text);
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
    fn display(&self) -> String {
        if self.formula.is_empty() {
            return self.value.clone();
        }
        if self.value.is_empty() {
            return format!("={}", self.formula);
        }
        format!("={}  →  {}", self.formula, self.value)
    }
}

type Workbook = BTreeMap<String, BTreeMap<(u32, u32), Cell>>;

fn read_workbook(bytes: &[u8]) -> Result<Workbook> {
    if bytes.is_empty() {
        return Ok(BTreeMap::new());
    }
    let mut workbook =
        open_workbook_auto_from_rs(Cursor::new(bytes)).context("无法读取电子表格")?;
    let mut sheets = BTreeMap::new();
    let mut cells_seen = 0;
    for name in workbook.sheet_names() {
        let values = workbook
            .worksheet_range(&name)
            .with_context(|| format!("无法读取工作表 {name}"))?;
        let origin = values.start().unwrap_or((0, 0));
        let mut cells = BTreeMap::new();
        for (row, col, value) in values.used_cells() {
            cells.insert(
                (origin.0 + row as u32, origin.1 + col as u32),
                Cell {
                    value: value.to_string(),
                    typed: format!("{value:?}"),
                    formula: String::new(),
                },
            );
        }
        if let Ok(formulas) = workbook.worksheet_formula(&name) {
            let origin = formulas.start().unwrap_or((0, 0));
            for (row, col, formula) in formulas
                .used_cells()
                .filter(|(_, _, formula)| !formula.is_empty())
            {
                cells
                    .entry((origin.0 + row as u32, origin.1 + col as u32))
                    .or_insert_with(Cell::default)
                    .formula
                    .clone_from(formula);
            }
        }
        cells_seen += cells.len();
        ensure!(
            cells_seen <= 300_000,
            "工作簿超过 300,000 个已用单元格，请使用外部工具"
        );
        sheets.insert(name, cells);
    }
    Ok(sheets)
}

pub fn compare_workbooks(left: &[u8], right: &[u8]) -> Result<(Vec<CellChange>, usize)> {
    let left = read_workbook(left)?;
    let right = read_workbook(right)?;
    let mut changes = Vec::new();
    let mut total = 0;
    let names = left.keys().chain(right.keys()).collect::<BTreeSet<_>>();
    let empty = BTreeMap::new();
    for name in names {
        let before = left.get(name);
        let after = right.get(name);
        if before.is_none() || after.is_none() {
            total += 1;
            if changes.len() < 5000 {
                changes.push(CellChange {
                    sheet: name.clone(),
                    address: "工作表".into(),
                    left: if before.is_some() {
                        name.clone()
                    } else {
                        String::new()
                    },
                    right: if after.is_some() {
                        name.clone()
                    } else {
                        String::new()
                    },
                    kind: if before.is_some() {
                        Kind::Remove
                    } else {
                        Kind::Add
                    },
                });
            }
        }
        let before = before.unwrap_or(&empty);
        let after = after.unwrap_or(&empty);
        let keys = before.keys().chain(after.keys()).collect::<BTreeSet<_>>();
        for &(row, col) in keys {
            let old = before.get(&(row, col));
            let new = after.get(&(row, col));
            if old == new {
                continue;
            }
            total += 1;
            if changes.len() < 5000 {
                changes.push(CellChange {
                    sheet: name.clone(),
                    address: address(row, col),
                    left: old.map(Cell::display).unwrap_or_default(),
                    right: new.map(Cell::display).unwrap_or_default(),
                    kind: if old.is_none() {
                        Kind::Add
                    } else if new.is_none() {
                        Kind::Remove
                    } else {
                        Kind::Replace
                    },
                });
            }
        }
    }
    Ok((changes, total))
}

fn address(row: u32, col: u32) -> String {
    let mut col = col + 1;
    let mut letters = Vec::new();
    while col > 0 {
        col -= 1;
        letters.push((b'A' + (col % 26) as u8) as char);
        col /= 26;
    }
    format!(
        "{}{}",
        letters.into_iter().rev().collect::<String>(),
        row + 1
    )
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
}
