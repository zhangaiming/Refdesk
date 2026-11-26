use anyhow::{Context, Result, bail, ensure};
use globset::GlobBuilder;
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

use crate::{diff, git};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolMode {
    BuiltIn,
    #[default]
    External,
    Viewer,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Tool {
    pub name: String,
    pub pattern: String,
    pub command: String,
    pub arguments: String,
    pub mode: ToolMode,
}

impl Default for Tool {
    fn default() -> Self {
        Self {
            name: "外部工具".into(),
            pattern: "*.xlsx".into(),
            command: String::new(),
            arguments: r#"${leftFile} ${rightFile}"#.into(),
            mode: ToolMode::External,
        }
    }
}

impl Tool {
    pub fn matches(&self, path: &str) -> bool {
        let basename = Path::new(path)
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or(path);
        self.pattern
            .split([',', ';'])
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .any(|part| {
                GlobBuilder::new(part)
                    .case_insensitive(true)
                    .build()
                    .is_ok_and(|glob| {
                        let matcher = glob.compile_matcher();
                        matcher.is_match(path) || matcher.is_match(basename)
                    })
            })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub repositories: Vec<PathBuf>,
    pub active: Option<PathBuf>,
    pub tools: Vec<Tool>,
    pub changes_only: bool,
    pub prefetch_history: bool,
    pub branch_selection: BTreeMap<PathBuf, git::HistoryScope>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            repositories: Vec::new(),
            active: None,
            tools: Vec::new(),
            changes_only: false,
            prefetch_history: true,
            branch_selection: BTreeMap::new(),
        }
    }
}

pub fn config_path() -> PathBuf {
    if let Some(path) = std::env::var_os("REFDESK_CONFIG") {
        return path.into();
    }
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("Refdesk")
        .join("settings.json")
}

impl Settings {
    pub fn load() -> Result<Self> {
        let path = config_path();
        let bytes = match fs::read(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            result => result.with_context(|| format!("无法读取 {}", path.display()))?,
        };
        serde_json::from_slice(&bytes).with_context(|| format!("配置格式有误：{}", path.display()))
    }

    pub fn save(&self) -> Result<()> {
        use std::io::Write;
        let path = config_path();
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        temp.write_all(&serde_json::to_vec_pretty(self)?)?;
        temp.as_file().sync_all()?;
        temp.persist(path)?;
        Ok(())
    }

    pub fn matching_tool(&self, path: &str) -> Option<&Tool> {
        self.tools
            .iter()
            .find(|tool| tool.matches(path))
            .filter(|tool| tool.mode != ToolMode::BuiltIn && !tool.command.trim().is_empty())
    }

    pub fn import_tools(&mut self, path: &Path) -> Result<usize> {
        let tools = import_smartgit(path)?;
        let count = tools.len();
        // Keep the order from SmartGit, including rules that select its internal viewer.
        // Keeping a second imported catch-all would shadow the newly imported rules.
        self.tools = tools;
        Ok(count)
    }
}

pub fn smartgit_preferences() -> Option<PathBuf> {
    let root = dirs::config_dir()?.join("syntevo").join("SmartGit");
    let preferred = root.join("24.1").join("preferences.yml");
    if preferred.is_file() {
        return Some(preferred);
    }
    let mut paths = fs::read_dir(root)
        .ok()?
        .flatten()
        .map(|entry| entry.path().join("preferences.yml"))
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    paths.sort_unstable();
    paths.pop()
}

pub fn import_smartgit(path: &Path) -> Result<Vec<Tool>> {
    let bytes = fs::read(path).with_context(|| format!("无法读取 {}", path.display()))?;
    let value: Value = serde_yaml::from_slice(&bytes).context("SmartGit YAML 格式有误")?;
    let entries = value["actions"]["compare"]["fileComparators"]
        .as_sequence()
        .or_else(|| value["fileComparators"].as_sequence())
        .or_else(|| value.as_sequence())
        .context("此文件没有 SmartGit Diff Tools 规则，请选择 preferences.yml")?;
    let mut tools = Vec::new();
    for item in entries {
        let pattern = item["filePattern"]
            .as_str()
            .context("Diff 规则缺少 filePattern")?;
        let settings = &item["externalFileCompare"];
        let mode = match item["compareType"].as_i64() {
            Some(0) => ToolMode::BuiltIn,
            Some(1) => ToolMode::External,
            Some(2) => ToolMode::Viewer,
            _ => bail!("不支持的 SmartGit compareType（{pattern}）"),
        };
        let command = settings["command.absolute"]
            .as_str()
            .filter(|value| !value.is_empty())
            .or_else(|| settings["command"].as_str())
            .unwrap_or_default()
            .to_owned();
        let arguments = settings["parameters"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let name = match mode {
            ToolMode::BuiltIn => "内置比较".into(),
            _ if pattern.contains(".xlsx") => "Excel Diff".into(),
            _ => format!("SmartGit · {pattern}"),
        };
        tools.push(Tool {
            name,
            pattern: pattern.into(),
            command,
            arguments,
            mode,
        });
    }
    Ok(tools)
}

fn tokenize(input: &str) -> Result<Vec<String>> {
    let mut characters = input.chars().peekable();
    let mut args = Vec::new();
    while characters.peek().is_some() {
        if characters.peek().is_some_and(|ch| ch.is_whitespace()) {
            characters.next();
            continue;
        }
        let mut word = String::new();
        let mut quoted = false;
        while let Some(&character) = characters.peek() {
            if character.is_whitespace() && !quoted {
                break;
            }
            match character {
                '\\' => {
                    let mut count = 0;
                    while characters.peek() == Some(&'\\') {
                        characters.next();
                        count += 1;
                    }
                    if characters.peek() == Some(&'"') {
                        word.extend(std::iter::repeat_n('\\', count / 2));
                        characters.next();
                        if count % 2 == 0 {
                            quoted = !quoted;
                        } else {
                            word.push('"');
                        }
                    } else {
                        word.extend(std::iter::repeat_n('\\', count));
                    }
                }
                '"' => {
                    characters.next();
                    quoted = !quoted;
                }
                _ => {
                    word.push(character);
                    characters.next();
                }
            }
        }
        ensure!(!quoted, "外部工具参数的引号不成对");
        args.push(word);
    }
    Ok(args)
}

fn expand(text: &str, values: &HashMap<&str, String>) -> Result<String> {
    let mut result = String::new();
    let mut rest = text;
    while let Some((before, variable)) = rest.split_once("${") {
        result.push_str(before);
        let (name, tail) = variable.split_once('}').context("变量缺少右括号")?;
        let value = values
            .get(name)
            .with_context(|| format!("不支持的 diff 工具变量：{name}"))?;
        result.push_str(value);
        rest = tail;
    }
    result.push_str(rest);
    Ok(result)
}

pub struct Launch {
    pub dir: TempDir,
    pub commands: Vec<Vec<String>>,
}

pub fn prepare_external(root: &Path, pair: &git::Compare, tool: &Tool) -> Result<Launch> {
    ensure!(tool.mode != ToolMode::BuiltIn, "此规则选择内置工具");
    let dir = tempfile::Builder::new().prefix("refdesk-diff-").tempdir()?;
    let old = git::diff_snapshot(root, &pair.left, &pair.left_path, pair.left_gitlink)?;
    let new = git::diff_snapshot(root, &pair.right, &pair.right_path, pair.right_gitlink)?;
    let tokens = tokenize(&tool.arguments)?;
    let needs_base = tokens.iter().any(|arg| arg.contains("${base"));
    let left_file = snapshot_path(dir.path(), "left", &pair.left_path);
    let right_file = snapshot_path(dir.path(), "right", &pair.right_path);
    fs::write(&left_file, &old)?;
    fs::write(&right_file, &new)?;
    let mut vars = HashMap::from([
        ("leftFile", left_file.to_string_lossy().into_owned()),
        ("rightFile", right_file.to_string_lossy().into_owned()),
        (
            "leftLocalFile",
            root.join(git::safe_relative(&pair.left_path)?)
                .to_string_lossy()
                .into_owned(),
        ),
        (
            "rightLocalFile",
            root.join(git::safe_relative(&pair.right_path)?)
                .to_string_lossy()
                .into_owned(),
        ),
        ("leftTitle", pair.left.title().into()),
        ("rightTitle", pair.right.title().into()),
        ("leftEncoding", diff::encoding(&old).into()),
        ("rightEncoding", diff::encoding(&new).into()),
        ("repoDir", root.to_string_lossy().into_owned()),
        ("repositoryDir", root.to_string_lossy().into_owned()),
    ]);
    if let Some(dir) = git::git_dir() {
        vars.insert("gitDir", dir.to_string_lossy().into_owned());
    }
    if let Some(dir) = std::env::var_os("REFDESK_SMARTGIT_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            Path::new("D:/SmartGit")
                .is_dir()
                .then(|| PathBuf::from("D:/SmartGit"))
        })
    {
        vars.insert("smartGitDir", dir.to_string_lossy().into_owned());
    }
    if needs_base {
        let old = git::base_snapshot(root, pair)?;
        let path = snapshot_path(dir.path(), "base", &pair.left_path);
        fs::write(&path, &old)?;
        vars.insert("baseFile", path.to_string_lossy().into_owned());
        vars.insert("baseTitle", pair.base.title().into());
        vars.insert("baseEncoding", diff::encoding(&old).into());
    }
    let command = expand(tool.command.trim().trim_matches('"'), &vars)?;
    let command = if let Some(relative) = command
        .strip_prefix("~/")
        .or_else(|| command.strip_prefix("~\\"))
    {
        dirs::home_dir()
            .context("无法解析 ~")?
            .join(relative)
            .to_string_lossy()
            .into_owned()
    } else {
        command
    };
    ensure!(!command.is_empty(), "外部工具的程序路径为空");
    let mut commands = Vec::new();
    match tool.mode {
        ToolMode::Viewer => {
            ensure!(
                tokens.iter().any(|arg| arg.contains("${file}")),
                "查看器需要 ${{file}}"
            );
            for file in [left_file, right_file] {
                vars.insert("file", file.to_string_lossy().into_owned());
                let mut invocation = vec![command.clone()];
                invocation.extend(
                    tokens
                        .iter()
                        .map(|token| expand(token, &vars))
                        .collect::<Result<Vec<_>>>()?,
                );
                commands.push(invocation);
            }
        }
        _ => {
            ensure!(
                tokens.iter().any(|arg| arg.contains("${leftFile}"))
                    && tokens.iter().any(|arg| arg.contains("${rightFile}")),
                "外部 diff 工具需要 ${{leftFile}} 与 ${{rightFile}}"
            );
            let mut invocation = vec![command];
            invocation.extend(
                tokens
                    .iter()
                    .map(|token| expand(token, &vars))
                    .collect::<Result<Vec<_>>>()?,
            );
            commands.push(invocation);
        }
    }
    Ok(Launch { dir, commands })
}

fn snapshot_path(dir: &Path, side: &str, source: &str) -> PathBuf {
    let extension = Path::new(source)
        .extension()
        .and_then(|value| value.to_str())
        .filter(|extension| {
            extension.len() <= 20 && extension.bytes().all(|ch| ch.is_ascii_alphanumeric())
        });
    dir.join(extension.map_or_else(|| side.into(), |extension| format!("{side}.{extension}")))
}

pub fn start_external(root: &Path, invocation: &[String]) -> Result<std::process::Child> {
    let (program, args) = invocation.split_first().context("无外部工具命令")?;
    #[cfg(windows)]
    if Path::new(program)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("bat") || ext.eq_ignore_ascii_case("cmd"))
    {
        use std::os::windows::process::CommandExt;
        ensure!(
            invocation
                .iter()
                .all(|value| !value.contains(['"', '%', '!', '\r', '\n', '^', '&', '|', '<', '>'])),
            "批处理工具的路径或参数包含 CMD 特殊字符，请使用可执行文件直接启动"
        );
        let quoted = invocation
            .iter()
            .map(|arg| format!("\"{arg}\""))
            .collect::<Vec<_>>()
            .join(" ");
        return Command::new("cmd.exe")
            .args(["/D", "/V:OFF", "/S", "/C"])
            .raw_arg(format!("\"{quoted}\""))
            .current_dir(root)
            .spawn()
            .context("无法启动批处理工具");
    }
    Command::new(program)
        .args(args)
        .current_dir(root)
        .spawn()
        .with_context(|| format!("无法启动外部工具：{program}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_arguments_preserve_paths_and_substitute_after_tokenization() {
        let args = tokenize(r#"-B "C:\My Tools\xlsx.py" "${leftFile}" ${rightFile}"#).unwrap();
        assert_eq!(args[1], r"C:\My Tools\xlsx.py");
        let vars = HashMap::from([("leftFile", r"C:\my temp\left.xlsx".to_owned())]);
        assert_eq!(expand(&args[2], &vars).unwrap(), r"C:\my temp\left.xlsx");
        assert!(tokenize("\"unclosed").is_err());
    }

    #[test]
    fn ordered_patterns_allow_a_built_in_override() {
        let settings = Settings {
            tools: vec![
                Tool {
                    pattern: "*.xlsx".into(),
                    mode: ToolMode::BuiltIn,
                    ..Default::default()
                },
                Tool {
                    pattern: "*".into(),
                    command: "diff.exe".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        assert!(settings.matching_tool("Assets/My Data.XLSX").is_none());
        assert!(settings.matching_tool("Assets/My Data.txt").is_some());
    }
}
