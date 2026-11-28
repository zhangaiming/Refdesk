use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn git(root: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(root);
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("GIT_") {
            cmd.env_remove(name);
        }
    }
    cmd.env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_EDITOR", "true")
        .env("GIT_TERMINAL_PROMPT", "0");
    cmd
}

fn run(root: &Path, args: &[&str], success: bool) -> Output {
    let output = git(root).args(args).output().unwrap();
    assert_eq!(
        output.status.success(),
        success,
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn commit(root: &Path, file: &str, text: &str, message: &str) {
    std::fs::write(root.join(file), text).unwrap();
    run(root, &["add", "--", file], true);
    run(root, &["commit", "-qm", message], true);
}

fn receive_editor(listener: &TcpListener) -> (TcpStream, Value) {
    let deadline = Instant::now() + Duration::from_secs(12);
    let mut stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(error) => panic!("Git editor did not connect: {error}"),
        }
    };
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut line = String::new();
    BufReader::new(&mut stream).read_line(&mut line).unwrap();
    let greeting = serde_json::from_str(&line).unwrap();
    (stream, greeting)
}

#[test]
fn git_editor_waits_and_saves_multiple_messages_during_one_rebase() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path().join("linked 工作树");
    std::fs::create_dir(&root).unwrap();
    run(&root, &["init", "-qb", "main"], true);
    run(&root, &["config", "user.name", "Refdesk Fixture"], true);
    run(
        &root,
        &["config", "user.email", "test@example.invalid"],
        true,
    );
    commit(&root, "one.txt", "base\n", "base");
    run(&root, &["switch", "-qc", "topic"], true);
    commit(&root, "one.txt", "topic\n", "first message");
    commit(&root, "two.txt", "later\n", "second message");
    run(&root, &["switch", "-q", "main"], true);
    commit(&root, "one.txt", "main\n", "main");
    run(&root, &["switch", "-q", "topic"], true);
    run(&root, &["rebase", "--merge", "main"], false);
    let todo = root.join(".git/rebase-merge/git-rebase-todo");
    let plan = std::fs::read_to_string(&todo).unwrap();
    let revised = plan.replacen("pick ", "reword ", 1);
    assert_ne!(plan, revised);
    std::fs::write(todo, revised).unwrap();
    std::fs::write(root.join("one.txt"), "resolved\n").unwrap();
    run(&root, &["add", "one.txt"], true);

    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    listener.set_nonblocking(true).unwrap();
    let token = "test-token-with-32-characters!!!";
    assert_eq!(token.len(), 32);
    let message_file = root.join(".git/COMMIT_EDITMSG");
    let executable = PathBuf::from(env!("CARGO_BIN_EXE_refdesk"));
    let child = git(&root)
        .args(["rebase", "--continue"])
        .env(
            "GIT_EDITOR",
            r#""$REFDESK_REBASE_EDITOR_EXE" --rebase-message-editor"#,
        )
        .env("REFDESK_REBASE_EDITOR_EXE", executable)
        .env(
            "REFDESK_REBASE_EDITOR_PORT",
            listener.local_addr().unwrap().port().to_string(),
        )
        .env("REFDESK_REBASE_EDITOR_TOKEN", token)
        .env("REFDESK_REBASE_EDITOR_FILE", &message_file)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    for (original, saved) in [
        ("first message", "冲突解决后的说明"),
        ("second message", "交互变基的新说明"),
    ] {
        let (mut stream, greeting) = receive_editor(&listener);
        assert_eq!(greeting["token"], token);
        assert!(greeting["message"].as_str().unwrap().starts_with(original));
        assert!(root.join(".git/rebase-merge").is_dir());
        assert!(
            std::fs::read_to_string(&message_file)
                .unwrap()
                .starts_with(original)
        );
        serde_json::to_writer(&mut stream, &format!("{saved}\n")).unwrap();
        stream.write_all(b"\n").unwrap();
    }
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(!root.join(".git/rebase-merge").exists());
    let output = run(&root, &["log", "-2", "--format=%s"], true);
    let messages = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        messages.lines().collect::<Vec<_>>(),
        ["交互变基的新说明", "冲突解决后的说明"]
    );
}
