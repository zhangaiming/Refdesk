use std::borrow::Cow;
use std::ops::Range;
use std::path::Path;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use syntect::parsing::{ParseState, Scope, ScopeStack, ScopeStackOp, SyntaxSet};

const MAX_LINE_BYTES: usize = 4 * 1024;
const MAX_SPANS: usize = 60_000;
const PARSE_BUDGET: Duration = Duration::from_millis(250);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Role {
    Keyword,
    String,
    Comment,
    Literal,
    Type,
}

pub(crate) struct Span {
    pub range: Range<usize>,
    pub role: Role,
}

#[derive(Default)]
pub(crate) struct Highlight {
    lines: Vec<Vec<Span>>,
}

impl Highlight {
    pub fn line(&self, number: usize) -> &[Span] {
        self.lines.get(number - 1).map_or(&[], Vec::as_slice)
    }
}

pub(crate) fn highlight(path: &Path, text: &str, canceled: impl Fn() -> bool) -> Option<Highlight> {
    static SYNTAXES: OnceLock<SyntaxSet> = OnceLock::new();
    let name = path.file_name()?.to_str()?;
    let extension = path.extension().and_then(|value| value.to_str());
    let syntaxes = SYNTAXES.get_or_init(SyntaxSet::load_defaults_newlines);
    let syntax = syntaxes
        .find_syntax_by_extension(name)
        .or_else(|| syntaxes.find_syntax_by_extension(extension?))?;
    if syntax.scope == Scope::new("text.plain").expect("valid syntax scope") {
        return None;
    }

    let mut parser = ParseState::new(syntax);
    let mut scopes = ScopeStack::new();
    let deadline = Instant::now() + PARSE_BUDGET;
    let mut remaining = MAX_SPANS;
    let mut lines = Vec::new();
    for line in text.split_inclusive(['\n', '\r']) {
        if canceled() {
            return None;
        }
        if line.len() > MAX_LINE_BYTES || remaining == 0 || Instant::now() >= deadline {
            break;
        }
        let line = match line.strip_suffix('\r') {
            Some(content) => Cow::Owned(format!("{content}\n")),
            None => Cow::Borrowed(line),
        };
        let Ok(operations) = parser.parse_line(&line, syntaxes) else {
            break;
        };
        let Some(spans) = line_spans(&line, operations, &mut scopes) else {
            break;
        };
        let Some(budget) = remaining.checked_sub(spans.len()) else {
            break;
        };
        remaining = budget;
        lines.push(spans);
    }
    lines
        .iter()
        .any(|line| !line.is_empty())
        .then_some(Highlight { lines })
}

fn line_spans(
    line: &str,
    operations: Vec<(usize, ScopeStackOp)>,
    scopes: &mut ScopeStack,
) -> Option<Vec<Span>> {
    let visible = line.strip_suffix('\n').unwrap_or(line);
    let mut spans: Vec<Span> = Vec::new();
    let mut source_offset = 0;
    let mut display_offset = 0;
    for (offset, operation) in operations
        .into_iter()
        .chain([(line.len(), ScopeStackOp::Noop)])
    {
        let offset = offset.min(visible.len());
        let part = visible.get(source_offset..offset)?;
        // The source is parsed with literal tabs; display_line expands each to four spaces.
        let end =
            display_offset + part.len() + 3 * part.bytes().filter(|&byte| byte == b'\t').count();
        if display_offset < end
            && let Some(role) = role_for(scopes)
        {
            if let Some(last) = spans.last_mut()
                && last.role == role
                && last.range.end == display_offset
            {
                last.range.end = end;
            } else {
                spans.push(Span {
                    range: display_offset..end,
                    role,
                });
            }
        }
        source_offset = offset;
        display_offset = end;
        scopes.apply(&operation).ok()?;
    }
    Some(spans)
}

fn role_for(scopes: &ScopeStack) -> Option<Role> {
    static PREFIXES: OnceLock<[(Scope, Role); 14]> = OnceLock::new();
    let prefixes = PREFIXES.get_or_init(|| {
        [
            ("comment", Role::Comment),
            ("string", Role::String),
            ("keyword", Role::Keyword),
            ("storage", Role::Keyword),
            ("constant", Role::Literal),
            ("entity.name.type", Role::Type),
            ("entity.name.class", Role::Type),
            ("entity.name.struct", Role::Type),
            ("support.type", Role::Type),
            ("support.class", Role::Type),
            ("markup.heading", Role::Keyword),
            ("markup.bold", Role::Literal),
            ("markup.italic", Role::Literal),
            ("markup.raw", Role::String),
        ]
        .map(|(scope, role)| (Scope::new(scope).expect("valid syntax scope"), role))
    });
    scopes.as_slice().iter().rev().find_map(|&active| {
        prefixes
            .iter()
            .find_map(|(prefix, role)| prefix.is_prefix_of(active).then_some(*role))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn syntax_uses_display_offsets_and_keeps_multiline_state() {
        let text = "/* start\n\t中文 comment\n*/\n\tlet label = \"旧值\";\n";
        let syntax = highlight(Path::new("SOURCE.RS"), text, || false).unwrap();
        let comment = "    中文 comment";
        let comment_span = &syntax.line(2)[0];
        assert_eq!(comment_span.role, Role::Comment);
        assert_eq!(&comment[comment_span.range.clone()], comment);

        let code = "    let label = \"旧值\";";
        assert!(
            syntax
                .line(4)
                .iter()
                .any(|span| { span.role == Role::Keyword && &code[span.range.clone()] == "let" })
        );
        assert!(syntax.line(4).iter().any(|span| {
            span.role == Role::String && &code[span.range.clone()] == "\"旧值\""
        }));
    }

    #[test]
    fn syntax_keeps_only_the_safe_prefix_after_an_oversized_line() {
        let text = format!("let a = 1;\n{}\nlet b = 2;\n", "x".repeat(MAX_LINE_BYTES));
        let syntax = highlight(Path::new("source.rs"), &text, || false).unwrap();
        assert!(!syntax.line(1).is_empty());
        assert!(syntax.line(2).is_empty());
        assert!(syntax.line(3).is_empty());
        assert!(highlight(Path::new("unknown.txt"), &text, || false).is_none());
        assert!(highlight(Path::new("source.rs"), &text, || true).is_none());
    }
}
