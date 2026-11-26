use crate::builtin_diff::{Cancellation, Issue};
use crate::diff::{Workbook, parse_workbook};
use crate::git::MAX_SNAPSHOT;
use crate::logging::{self, ReadLog};
use anyhow::{Result, ensure};
use std::io::{BufReader, BufWriter, Read, Write, stdin, stdout};
use std::os::windows::io::OwnedHandle;
use std::os::windows::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::time::Instant;
use windows_sys::Win32::System::JobObjects::{
    JOB_OBJECT_LIMIT_PROCESS_MEMORY, JOB_OBJECT_LIMIT_PROCESS_TIME,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
};

pub(crate) const ARGUMENT: &str = "--workbook-read-worker";
const MAX_RESULT: u64 = 128 * 1024 * 1024;
const MAX_MEMORY: usize = 512 * 1024 * 1024;
const MAX_CPU_TIME: i64 = 30 * 10_000_000; // 100 ns units
type Reply = std::result::Result<Workbook, Issue>;

pub(crate) fn parse(bytes: &[u8], log: &ReadLog, cancel: &Cancellation) -> Reply {
    let (worktree, request) = (log.worktree, log.request);
    let started = Instant::now();
    let started_child = std::env::current_exe().and_then(|exe| {
        Command::new(exe)
            .arg(ARGUMENT)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .creation_flags(0x08000000)
            .spawn()
    });
    let mut child = started_child.map_err(|_| {
        tracing::warn!(worktree, request, stage = "spawn", "workbook_worker_failed");
        Issue::ParserUnavailable
    })?;
    let pid = child.id();
    tracing::debug!(
        worktree,
        request,
        pid,
        bytes = bytes.len(),
        "workbook_worker_started"
    );

    // The child waits for input until its memory and CPU limits are installed.
    let job = limit_process(&child).map_err(|_| {
        let _ = child.kill();
        let _ = child.wait();
        tracing::warn!(
            worktree,
            request,
            pid,
            stage = "apply_limits",
            "workbook_worker_failed"
        );
        Issue::ParserUnavailable
    })?;
    let (result, stage, exit_code) = crate::window_job::cancel_with(job, cancel.signal(), || {
        let mut input = child.stdin.take().expect("piped stdin");
        let written = write_book(&mut input, bytes);
        drop(input);

        let mut output =
            BufReader::new(child.stdout.take().expect("piped stdout")).take(MAX_RESULT + 1);
        let reply = serde_json::from_reader::<_, Reply>(&mut output);
        let bounded = output.limit() > 0;
        drop(output);
        let bad_output = (!bounded || reply.is_err())
            && matches!(child.try_wait(), Ok(None))
            && child.kill().is_ok();
        let Ok(status) = child.wait() else {
            return (Err(Issue::ParserUnavailable), "wait", None);
        };
        let (result, stage) = match reply {
            _ if !bounded => (Err(Issue::PreviewLimit), "output_limit"),
            _ if written.is_err() => (Err(Issue::ParserUnavailable), "write_input"),
            // A job may terminate the parser before it can return a structured reply.
            _ if !status.success() && !bad_output => (Err(Issue::PreviewLimit), "process_exit"),
            Err(_) => (Err(Issue::ParserUnavailable), "read_result"),
            Ok(Err(issue)) => (Err(issue), "parse_workbook"),
            Ok(Ok(book)) => (Ok(book), "complete"),
        };
        (result, stage, status.code())
    });
    let run_ms = logging::millis(started.elapsed());
    if !cancel.requested() {
        if result.is_ok() {
            tracing::debug!(
                worktree,
                request,
                pid,
                exit_code,
                run_ms,
                "workbook_worker_finished"
            );
        } else {
            tracing::warn!(
                worktree,
                request,
                pid,
                stage,
                exit_code,
                run_ms,
                "workbook_worker_failed"
            );
        }
    }
    result
}

fn write_book(input: &mut impl Write, book: &[u8]) -> Result<()> {
    let size = book.len() as u64;
    ensure!(size <= MAX_SNAPSHOT, "workbook snapshot limit");
    input.write_all(&size.to_le_bytes())?;
    input.write_all(book)?;
    Ok(())
}

fn read_book(input: &mut impl Read) -> Result<Vec<u8>> {
    let mut size = [0; 8];
    input.read_exact(&mut size)?;
    let size = u64::from_le_bytes(size);
    ensure!(size <= MAX_SNAPSHOT, "workbook snapshot limit");
    let mut data = vec![0; size as usize];
    input.read_exact(&mut data)?;
    Ok(data)
}

pub(crate) fn run() -> Result<()> {
    let mut input = stdin().lock();
    let reply = parse_workbook(&read_book(&mut input)?);
    let mut output = BufWriter::new(stdout().lock());
    serde_json::to_writer(&mut output, &reply)?;
    output.flush()?;
    Ok(())
}

fn limit_process(child: &Child) -> Result<OwnedHandle> {
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags =
        JOB_OBJECT_LIMIT_PROCESS_MEMORY | JOB_OBJECT_LIMIT_PROCESS_TIME;
    limits.BasicLimitInformation.PerProcessUserTimeLimit = MAX_CPU_TIME;
    limits.ProcessMemoryLimit = MAX_MEMORY;
    Ok(crate::window_job::assign(child, limits)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::JobObjects::{
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JobObjectExtendedLimitInformation,
        QueryInformationJobObject,
    };

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
        assert_ne!(
            limits.BasicLimitInformation.LimitFlags & JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            0
        );
        assert_eq!(limits.ProcessMemoryLimit, MAX_MEMORY);
        assert_eq!(
            limits.BasicLimitInformation.PerProcessUserTimeLimit,
            MAX_CPU_TIME
        );
        Ok(())
    }
}
