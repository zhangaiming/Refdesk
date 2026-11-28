use anyhow::{Context, Result, ensure};
use crossbeam_channel::{Receiver, Sender, bounded};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::ops::ControlFlow;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::Duration;

pub(crate) const ARGUMENT: &str = "--rebase-message-editor";
const EXECUTABLE: &str = "REFDESK_REBASE_EDITOR_EXE";
const PORT: &str = "REFDESK_REBASE_EDITOR_PORT";
const TOKEN: &str = "REFDESK_REBASE_EDITOR_TOKEN";
const FILE: &str = "REFDESK_REBASE_EDITOR_FILE";
const MESSAGE_LIMIT: usize = 1024 * 1024;
const WIRE_LIMIT: usize = 6 * MESSAGE_LIMIT + 1024;
const SOCKET_TIMEOUT: Duration = Duration::from_secs(2);

pub(crate) struct RebaseMessageRequest {
    pub message: String,
    pub commit: Option<String>,
    pub reply: Sender<RebaseMessageReply>,
}

pub(crate) enum RebaseMessageReply {
    Save(String),
    Abort,
}

#[derive(Serialize, Deserialize)]
struct Greeting {
    token: String,
    message: String,
}

fn receive<T: DeserializeOwned>(stream: &mut TcpStream) -> Result<T> {
    let mut data = Vec::new();
    BufReader::new(stream)
        .take(WIRE_LIMIT as u64)
        .read_until(b'\n', &mut data)?;
    ensure!(data.last() == Some(&b'\n'), "Git 编辑器消息不完整或过大");
    Ok(serde_json::from_slice(&data)?)
}

fn send(stream: &mut TcpStream, value: &impl Serialize) -> Result<()> {
    serde_json::to_writer(&mut *stream, value)?;
    stream.write_all(b"\n")?;
    Ok(())
}

pub(crate) fn run_helper(file: &Path) -> Result<()> {
    let expected = std::env::var_os(FILE).context("缺少 Git 提交说明文件")?;
    let parent = Path::new(&expected)
        .parent()
        .context("缺少 Git 提交说明目录")?
        .canonicalize()?;
    ensure!(
        file.canonicalize()? == parent.join("COMMIT_EDITMSG"),
        "无法确认 Git 请求的变基提交说明文件"
    );
    let origin = parent.join("rebase-merge/orig-head");
    let origin_modified = origin.metadata()?.modified()?;
    let message_modified = file.metadata()?.modified()?;
    let token = std::env::var(TOKEN).context("缺少 Git 编辑请求凭据")?;
    let port = std::env::var(PORT)?.parse::<u16>()?;
    ensure!(token.len() == 32, "Git 编辑请求凭据无效");

    let mut data = Vec::new();
    std::fs::File::open(file)?
        .take(MESSAGE_LIMIT as u64 + 1)
        .read_to_end(&mut data)?;
    ensure!(data.len() <= MESSAGE_LIMIT, "提交说明过大，请在终端处理");
    let message = String::from_utf8(data).context("提交说明不是 UTF-8，请在终端编辑")?;

    let mut stream =
        TcpStream::connect_timeout(&(Ipv4Addr::LOCALHOST, port).into(), SOCKET_TIMEOUT)?;
    stream.set_write_timeout(Some(SOCKET_TIMEOUT))?;
    send(&mut stream, &Greeting { token, message })?;
    let saved: String = receive(&mut stream).context("Refdesk 未完成变基提交说明编辑")?;
    ensure!(
        saved.len() <= MESSAGE_LIMIT,
        "提交说明过大，请缩短说明后重试"
    );
    ensure!(
        origin.metadata()?.modified()? == origin_modified
            && file.metadata()?.modified()? == message_modified,
        "Git 编辑现场已变化，请刷新后继续变基"
    );
    let mut output = tempfile::NamedTempFile::new_in(file.parent().context("缺少提交说明目录")?)?;
    output.write_all(saved.as_bytes())?;
    output.persist(file)?;
    Ok(())
}

pub(super) fn run(
    command: &mut Command,
    git_dir: &Path,
    on_message: &mut (dyn FnMut(RebaseMessageRequest) + Send),
) -> Result<(Output, bool)> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    listener.set_nonblocking(true)?;
    let port = listener.local_addr()?.port();
    let secret = tempfile::Builder::new()
        .prefix("")
        .rand_bytes(32)
        .tempdir()?;
    let token = secret
        .path()
        .file_name()
        .context("无法建立编辑请求凭据")?
        .to_string_lossy();
    // The shell expands the quoted environment value without interpreting the executable path as shell code.
    command
        .env("GIT_EDITOR", format!("\"${EXECUTABLE}\" {ARGUMENT}"))
        .env(EXECUTABLE, std::env::current_exe()?)
        .env(PORT, port.to_string())
        .env(TOKEN, token.as_ref())
        .env(FILE, git_dir.join("COMMIT_EDITMSG"));

    let mut git = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = git.stdout.take().expect("piped Git output");
    let stderr = git.stderr.take().expect("piped Git errors");
    let (done, stopped) = bounded::<()>(0);
    std::thread::scope(|scope| {
        let editor = scope.spawn(move || serve(&listener, &token, git_dir, &stopped, on_message));
        let stdout = scope.spawn(move || collect(stdout));
        let stderr = scope.spawn(move || collect(stderr));
        // The editor descendant can retain Git's output pipes after Git exits.
        let status = git.wait();
        drop(done);
        let abort = editor.join().expect("Git editor bridge")?;
        Ok((
            Output {
                status: status?,
                stdout: stdout.join().expect("Git output reader")?,
                stderr: stderr.join().expect("Git error reader")?,
            },
            abort,
        ))
    })
}

fn collect(mut stream: impl Read) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn serve(
    listener: &TcpListener,
    token: &str,
    git_dir: &Path,
    stopped: &Receiver<()>,
    on_message: &mut (dyn FnMut(RebaseMessageRequest) + Send),
) -> Result<bool> {
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                if let ControlFlow::Break(abort) =
                    handle(stream, token, git_dir, stopped, on_message)
                {
                    return Ok(abort);
                }
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                if !matches!(
                    stopped.recv_timeout(Duration::from_millis(15)),
                    Err(crossbeam_channel::RecvTimeoutError::Timeout)
                ) {
                    return Ok(false);
                }
            }
            Err(error) => return Err(error.into()),
        }
    }
}

fn handle(
    mut stream: TcpStream,
    token: &str,
    git_dir: &Path,
    stopped: &Receiver<()>,
    on_message: &mut (dyn FnMut(RebaseMessageRequest) + Send),
) -> ControlFlow<bool> {
    if stream.set_nonblocking(false).is_err()
        || stream.set_read_timeout(Some(SOCKET_TIMEOUT)).is_err()
        || stream.set_write_timeout(Some(SOCKET_TIMEOUT)).is_err()
    {
        return ControlFlow::Continue(());
    }
    let Ok(greeting) = receive::<Greeting>(&mut stream) else {
        return ControlFlow::Continue(());
    };
    if greeting.token != token || greeting.message.len() > MESSAGE_LIMIT {
        return ControlFlow::Continue(());
    }
    let (reply, response) = bounded(1);
    on_message(RebaseMessageRequest {
        message: greeting.message,
        commit: super::editor_commit(git_dir),
        reply,
    });
    crossbeam_channel::select! {
        recv(stopped) -> _ => ControlFlow::Break(false),
        recv(response) -> reply => match reply {
            Ok(RebaseMessageReply::Save(message)) => {
                let _ = send(&mut stream, &message);
                ControlFlow::Continue(())
            }
            Ok(RebaseMessageReply::Abort) => ControlFlow::Break(true),
            Err(_) => ControlFlow::Continue(()),
        }
    }
}
