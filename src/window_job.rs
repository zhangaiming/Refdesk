use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::process::Child;
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject,
};

pub(crate) fn assign(
    child: &Child,
    mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
) -> io::Result<OwnedHandle> {
    limits.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    unsafe {
        let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        let job = OwnedHandle::from_raw_handle(handle);
        if SetInformationJobObject(
            job.as_raw_handle(),
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            std::mem::size_of_val(&limits) as u32,
        ) == 0
            || AssignProcessToJobObject(job.as_raw_handle(), child.as_raw_handle()) == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(job)
    }
}

pub(crate) fn cancel_with<T>(
    job: OwnedHandle,
    canceled: &crossbeam_channel::Receiver<()>,
    work: impl FnOnce() -> T,
) -> T {
    let (finished, done) = crossbeam_channel::bounded::<()>(0);
    std::thread::scope(|scope| {
        scope.spawn(move || {
            crossbeam_channel::select! {
                recv(canceled) -> _ => (),
                recv(done) -> _ => (),
            }
            drop(job);
        });
        let result = work();
        drop(finished);
        result
    })
}
