#![cfg(windows)]

use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use std::io::{Seek, Write};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

#[test]
fn workbook_worker_delivers_each_snapshot_with_sparse_cells_and_formulas() -> Result<()> {
    let before = parse(include_bytes!("fixtures/workbook/before.xlsx"))?;
    let after = parse(include_bytes!("fixtures/workbook/after.xlsx"))?;
    let [old_balance] = before.as_array().unwrap().as_slice() else {
        bail!("expected Balance, got {before}");
    };
    let [new_balance, notes] = after.as_array().unwrap().as_slice() else {
        bail!("expected Balance and Notes, got {after}");
    };
    for balance in [old_balance, new_balance] {
        assert_eq!(balance["name"], "Balance");
        assert_eq!(balance["rows"], 401);
        assert_eq!(balance["columns"], 18);
        assert_eq!(balance["cells"][0][0], json!([356, 12]));
        assert_eq!(balance["cells"][0][1]["value"], "Context");
        assert_eq!(balance["cells"][1][0], json!([357, 13]));
        assert_eq!(balance["cells"][2][0], json!([400, 17]));
    }
    assert_eq!(old_balance["cells"][1][1]["value"], "12");
    assert_eq!(new_balance["cells"][1][1]["value"], "18");
    assert_eq!(old_balance["cells"][2][1]["formula"], "N358*2");
    assert_eq!(new_balance["cells"][2][1]["formula"], "N358*3");
    assert_eq!(notes["name"], "Notes");
    assert_eq!(notes["cells"][0][0], json!([3, 1]));
    assert_eq!(notes["cells"][0][1]["value"], "Reviewed");
    assert_eq!(parse(b"not a workbook")?.as_str(), Some("Decode"));
    Ok(())
}

fn parse(workbook: &[u8]) -> Result<Value> {
    let sandbox = tempfile::tempdir()?;
    let mut input = tempfile::tempfile()?;
    input.write_all(&(workbook.len() as u64).to_le_bytes())?;
    input.write_all(workbook)?;
    input.rewind()?;

    let child = Command::new(env!("CARGO_BIN_EXE_refdesk"))
        .arg("--workbook-read-worker")
        .current_dir(sandbox.path())
        .env("REFDESK_CONFIG", sandbox.path().join("settings.json"))
        .env("REFDESK_LOG_DIR", sandbox.path())
        .env_remove("REFDESK_CAPTURE")
        .env_remove("REFDESK_TIMINGS")
        .stdin(input)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let output = wait_for_worker(child)?;
    ensure!(
        output.status.success(),
        "workbook worker exited with {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let reply: std::result::Result<Value, Value> =
        serde_json::from_slice(&output.stdout).context("decode the workbook worker reply")?;
    Ok(match reply {
        Ok(value) | Err(value) => value,
    })
}

fn wait_for_worker(mut child: Child) -> Result<Output> {
    let deadline = Instant::now() + Duration::from_secs(15);
    while child.try_wait()?.is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            child.wait()?;
            bail!("workbook worker did not exit within 15 seconds");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Ok(child.wait_with_output()?)
}
