#![cfg(windows)]

use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use std::io::{Seek, Write};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

#[test]
fn workbook_worker_preserves_sparse_cells_and_formulas() -> Result<()> {
    let sandbox = tempfile::tempdir()?;
    let mut input = tempfile::tempfile()?;
    for workbook in [
        include_bytes!("fixtures/workbook/before.xlsx").as_slice(),
        include_bytes!("fixtures/workbook/after.xlsx").as_slice(),
    ] {
        input.write_all(&(workbook.len() as u64).to_le_bytes())?;
        input.write_all(workbook)?;
    }
    input.rewind()?;

    let child = Command::new(env!("CARGO_BIN_EXE_refdesk"))
        .arg("--workbook-diff-worker")
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
    let reply: Result<Value, String> =
        serde_json::from_slice(&output.stdout).context("decode the workbook worker reply")?;
    let workbook = reply.map_err(anyhow::Error::msg)?;
    let sheets = workbook["sheets"]
        .as_array()
        .context("workbook worker reply has no sheet list")?;
    let [balance, notes] = sheets.as_slice() else {
        bail!("expected Balance and Notes, got {sheets:?}");
    };

    assert_eq!(balance["name"], "Balance");
    assert_eq!(balance["rows"], 401);
    assert_eq!(balance["columns"], 18);
    // M357 is unchanged. N358 and R401 keep their absolute zero-based coordinates.
    assert_eq!(
        balance["cells"],
        json!([
            [[356, 12], {"Same": "Context"}],
            [[357, 13], {"Changed": ["12", "18"]}],
            [[400, 17], {"Changed": ["=N358*2  →  24", "=N358*3  →  54"]}],
        ])
    );
    assert_eq!(balance["changes"], json!([1, 2]));
    assert_eq!(notes["name"], "Notes");
    assert_eq!(notes["kind"], "Add");
    assert_eq!(notes["cells"], json!([[[3, 1], {"Added": "Reviewed"}]]));
    Ok(())
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
