use crate::diff::{WorkbookDiff, compare_workbooks};
use crate::git::MAX_SNAPSHOT;
use crate::logging::{self, ReadLog};
use anyhow::{Context, Result, ensure};
use std::io::{BufReader, BufWriter, Read, Write, stdin, stdout};
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::os::windows::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::time::Instant;
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOB_OBJECT_LIMIT_PROCESS_MEMORY, JOB_OBJECT_LIMIT_PROCESS_TIME,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject,
};

pub(crate) const ARGUMENT: &str = "--workbook-diff-worker";
const MAX_RESULT: u64 = 128 * 1024 * 1024;
const MAX_MEMORY: usize = 512 * 1024 * 1024;
const MAX_CPU_TIME: i64 = 30 * 10_000_000; // 100 ns units
const LIMIT_MESSAGE: &str = "工作簿超出内置比较的解析预算或无法读取，请使用外部工具";
type Reply = std::result::Result<WorkbookDiff, String>;

pub(crate) fn compare(left: &[u8], right: &[u8], log: &ReadLog) -> Result<WorkbookDiff> {
    let (worktree, request) = (log.worktree, log.request);
    let started = Instant::now();
    let mut child = Command::new(std::env::current_exe()?)
        .arg(ARGUMENT)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(0x08000000)
        .spawn()
        .context("无法启动工作簿解析进程")
        .inspect_err(|_| {
            tracing::warn!(worktree, request, stage = "spawn", "workbook_worker_failed")
        })?;
    let pid = child.id();
    tracing::debug!(
        worktree,
        request,
        pid,
        left_bytes = left.len(),
        right_bytes = right.len(),
        "workbook_worker_started"
    );
    // The child waits for input until its memory and CPU limits are installed.
    let job = match limit_process(&child) {
        Ok(job) => job,
        Err(error) => {
            let _ = child.kill();
            let exit_code = child.wait().ok().and_then(|status| status.code());
            tracing::warn!(
                worktree,
                request,
                pid,
                stage = "apply_limits",
                exit_code,
                run_ms = logging::millis(started.elapsed()),
                "workbook_worker_failed"
            );
            return Err(error).context("无法限制工作簿解析进程，请使用外部工具");
        }
    };
    let mut input = child.stdin.take().expect("piped stdin");
    let written = write_book(&mut input, left).and_then(|()| write_book(&mut input, right));
    drop(input);

    let mut output =
        BufReader::new(child.stdout.take().expect("piped stdout")).take(MAX_RESULT + 1);
    let reply = serde_json::from_reader::<_, Reply>(&mut output);
    let bounded = output.limit() > 0;
    drop(output);
    if !bounded || reply.is_err() {
        let _ = child.kill();
    }
    let status = child.wait().inspect_err(|_| {
        tracing::warn!(
            worktree,
            request,
            pid,
            stage = "wait",
            run_ms = logging::millis(started.elapsed()),
            "workbook_worker_failed"
        );
    })?;
    drop(job);
    let exit_code = status.code();
    let run_ms = logging::millis(started.elapsed());
    let stage = if !bounded {
        Some("output_limit")
    } else if written.is_err() {
        Some("write_input")
    } else if !status.success() {
        Some("process_exit")
    } else if reply.is_err() {
        Some("read_result")
    } else if matches!(reply, Ok(Err(_))) {
        Some("parse_workbook")
    } else {
        None
    };
    if let Some(stage) = stage {
        tracing::warn!(
            worktree,
            request,
            pid,
            stage,
            exit_code,
            run_ms,
            "workbook_worker_failed"
        );
    } else {
        tracing::debug!(
            worktree,
            request,
            pid,
            exit_code,
            run_ms,
            "workbook_worker_finished"
        );
    }
    ensure!(status.success() && bounded, "{LIMIT_MESSAGE}");
    written.context(LIMIT_MESSAGE)?;
    reply.context(LIMIT_MESSAGE)?.map_err(anyhow::Error::msg)
}

fn write_book(input: &mut impl Write, book: &[u8]) -> Result<()> {
    let size = book.len() as u64;
    ensure!(size <= MAX_SNAPSHOT, "{LIMIT_MESSAGE}");
    input.write_all(&size.to_le_bytes())?;
    input.write_all(book)?;
    Ok(())
}

fn read_book(input: &mut impl Read) -> Result<Vec<u8>> {
    let mut size = [0; 8];
    input.read_exact(&mut size)?;
    let size = u64::from_le_bytes(size);
    ensure!(size <= MAX_SNAPSHOT, "{LIMIT_MESSAGE}");
    let mut data = vec![0; size as usize];
    input.read_exact(&mut data)?;
    Ok(data)
}

pub(crate) fn run() -> Result<()> {
    let mut input = stdin().lock();
    let left = read_book(&mut input)?;
    let right = read_book(&mut input)?;
    let reply: Reply = compare_workbooks(&left, &right).map_err(|error| format!("{error:#}"));
    let mut output = BufWriter::new(stdout().lock());
    serde_json::to_writer(&mut output, &reply)?;
    output.flush()?;
    Ok(())
}

fn limit_process(child: &Child) -> Result<OwnedHandle> {
    // Own the job handle before assigning the process so every exit closes it.
    unsafe {
        let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if handle.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }
        let job = OwnedHandle::from_raw_handle(handle);
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_PROCESS_MEMORY
            | JOB_OBJECT_LIMIT_PROCESS_TIME
            | JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        limits.BasicLimitInformation.PerProcessUserTimeLimit = MAX_CPU_TIME;
        limits.ProcessMemoryLimit = MAX_MEMORY;
        if SetInformationJobObject(
            job.as_raw_handle(),
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            std::mem::size_of_val(&limits) as u32,
        ) == 0
            || AssignProcessToJobObject(job.as_raw_handle(), child.as_raw_handle()) == 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(job)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::System::JobObjects::QueryInformationJobObject;

    #[test]
    fn windows_job_limits_the_parser_before_input() -> Result<()> {
        let mut child = Command::new("cmd.exe")
            .args(["/D", "/C", "set /p waiting="])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .creation_flags(0x08000000)
            .spawn()?;
        let job = limit_process(&child)?;
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        let queried = unsafe {
            QueryInformationJobObject(
                job.as_raw_handle(),
                JobObjectExtendedLimitInformation,
                (&mut limits as *mut JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of_val(&limits) as u32,
                std::ptr::null_mut(),
            )
        };
        drop(job);
        child.wait()?;
        assert_ne!(queried, 0);
        assert_eq!(limits.ProcessMemoryLimit, MAX_MEMORY);
        assert_eq!(
            limits.BasicLimitInformation.PerProcessUserTimeLimit,
            MAX_CPU_TIME
        );
        Ok(())
    }
}
