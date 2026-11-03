//! Host glibc registers restartable sequences in supervisor-key TLS.
//!
//! The kernel updates that TLS on scheduling and signal delivery. It cannot
//! write the registration while guest PKRU denies the supervisor key, so each
//! host thread must unregister it before entering a cage.

use std::io;
use std::mem::{align_of, size_of};
use std::sync::mpsc::{self, SyncSender};

const RSEQ_FLAG_UNREGISTER: libc::c_uint = 1;
const RSEQ_SIGNATURE: libc::c_uint = 0x5305_3053;
const RSEQ_CPU_ID_REGISTRATION_FAILED: i32 = -2;

#[repr(C)]
struct RseqCpuIds {
    cpu_id_start: u32,
    cpu_id: i32,
}

struct GlibcRseq {
    area: *mut RseqCpuIds,
    size: libc::c_uint,
}

impl GlibcRseq {
    fn cpu_id(&self) -> i32 {
        unsafe { std::ptr::addr_of!((*self.area).cpu_id).read_volatile() }
    }
}

fn current_glibc_rseq() -> io::Result<Option<GlibcRseq>> {
    let size_symbol = unsafe { libc::dlsym(libc::RTLD_DEFAULT, c"__rseq_size".as_ptr()) };
    if size_symbol.is_null() {
        // Older glibc versions do not register rseq or export this ABI.
        return Ok(None);
    }
    let size = unsafe { size_symbol.cast::<libc::c_uint>().read() };
    if size == 0 {
        return Ok(None);
    }

    let offset_symbol = unsafe { libc::dlsym(libc::RTLD_DEFAULT, c"__rseq_offset".as_ptr()) };
    let flags_symbol = unsafe { libc::dlsym(libc::RTLD_DEFAULT, c"__rseq_flags".as_ptr()) };
    if offset_symbol.is_null() || flags_symbol.is_null() {
        return Err(io::Error::other(
            "glibc rseq ABI is missing its offset or flags",
        ));
    }
    let offset = unsafe { offset_symbol.cast::<isize>().read() };
    let flags = unsafe { flags_symbol.cast::<libc::c_uint>().read() };
    if flags != 0 || (size as usize) < size_of::<RseqCpuIds>() {
        return Err(io::Error::other(format!(
            "unsupported glibc rseq registration: size={size}, flags={flags:#x}"
        )));
    }

    let thread_pointer: usize;
    unsafe {
        std::arch::asm!(
            "rdfsbase {}",
            out(reg) thread_pointer,
            options(nostack, preserves_flags, nomem)
        );
    }
    let address = thread_pointer
        .checked_add_signed(offset)
        .filter(|address| address % align_of::<RseqCpuIds>() == 0)
        .ok_or_else(|| io::Error::other("invalid glibc rseq TLS offset"))?;
    Ok(Some(GlibcRseq {
        area: address as *mut RseqCpuIds,
        size,
    }))
}

/// Disables glibc's rseq registration on this thread, without changing PKRU.
pub(super) fn disable_for_current_thread() -> io::Result<()> {
    let Some(rseq) = current_glibc_rseq()? else {
        return Ok(());
    };
    if rseq.cpu_id() < 0 {
        return Ok(());
    }

    let result = unsafe {
        libc::syscall(
            libc::SYS_rseq,
            rseq.area,
            rseq.size,
            RSEQ_FLAG_UNREGISTER,
            RSEQ_SIGNATURE,
        )
    };
    if result != 0 {
        return Err(io::Error::last_os_error());
    }

    // Match glibc's failed-registration state so it and newly created threads
    // use their non-rseq paths.
    unsafe {
        std::ptr::addr_of_mut!((*rseq.area).cpu_id).write_volatile(RSEQ_CPU_ID_REGISTRATION_FAILED);
    }
    Ok(())
}

/// Starts a host thread only after its rseq registration has been disabled.
/// The returned sender releases the thread to run once its caller has prepared
/// the guest resources. Dropping the sender cancels that launch.
pub(super) fn prepare_thread<T: Send + 'static>(
    run: impl FnOnce(T) + Send + 'static,
) -> io::Result<SyncSender<T>> {
    prepare_thread_with(disable_for_current_thread, run)
}

fn prepare_thread_with<T: Send + 'static>(
    setup: impl FnOnce() -> io::Result<()> + Send + 'static,
    run: impl FnOnce(T) + Send + 'static,
) -> io::Result<SyncSender<T>> {
    let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
    let (launch_sender, launch_receiver) = mpsc::sync_channel(1);
    let thread = std::thread::Builder::new().spawn(move || {
        let ready = setup();
        let success = ready.is_ok();
        if ready_sender.send(ready).is_err() || !success {
            return;
        }
        if let Ok(launch) = launch_receiver.recv() {
            run(launch);
        }
    })?;

    match ready_receiver.recv() {
        Ok(Ok(())) => Ok(launch_sender),
        result => {
            if thread.join().is_err() {
                return Err(io::Error::other("host thread panicked during rseq setup"));
            }
            match result {
                Ok(Err(error)) => Err(error),
                Err(error) => Err(io::Error::other(format!(
                    "host thread exited before rseq setup: {error}"
                ))),
                Ok(Ok(())) => unreachable!(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn unregisters_only_the_current_thread_and_is_inherited() {
        std::thread::spawn(|| {
            let Some(registration) = current_glibc_rseq().unwrap() else {
                disable_for_current_thread().unwrap();
                return;
            };
            if registration.cpu_id() < 0 {
                disable_for_current_thread().unwrap();
                return;
            }

            let (ready_tx, ready_rx) = mpsc::channel();
            let (resume_tx, resume_rx) = mpsc::channel();
            let existing = std::thread::spawn(move || {
                let rseq = current_glibc_rseq().unwrap().unwrap();
                ready_tx.send(rseq.cpu_id()).unwrap();
                resume_rx.recv().unwrap();
                rseq.cpu_id()
            });
            assert!(ready_rx.recv().unwrap() >= 0);

            disable_for_current_thread().unwrap();
            assert_eq!(registration.cpu_id(), RSEQ_CPU_ID_REGISTRATION_FAILED);
            disable_for_current_thread().unwrap();
            assert!(unsafe { libc::sched_getcpu() } >= 0);

            let child = std::thread::spawn(|| {
                let rseq = current_glibc_rseq().unwrap().unwrap();
                disable_for_current_thread().unwrap();
                rseq.cpu_id()
            });
            assert!(child.join().unwrap() < 0);
            resume_tx.send(()).unwrap();
            assert!(existing.join().unwrap() >= 0);
        })
        .join()
        .unwrap();
    }

    #[test]
    fn prepared_thread_waits_for_launch_and_reports_setup_errors() {
        let (result_sender, result_receiver) = mpsc::sync_channel(1);
        let launch =
            prepare_thread_with(|| Ok(()), move |value| result_sender.send(value).unwrap())
                .unwrap();
        assert!(matches!(
            result_receiver.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
        launch.send(42).unwrap();
        assert_eq!(result_receiver.recv().unwrap(), 42);

        let (result_sender, result_receiver) = mpsc::sync_channel(1);
        let launch =
            prepare_thread_with(|| Ok(()), move |()| result_sender.send(()).unwrap()).unwrap();
        drop(launch);
        assert_eq!(
            result_receiver.recv_timeout(std::time::Duration::from_secs(2)),
            Err(mpsc::RecvTimeoutError::Disconnected)
        );

        let (result_sender, result_receiver) = mpsc::sync_channel(1);
        let failure = prepare_thread_with::<()>(
            || Err(io::Error::other("rseq setup failed")),
            move |_| result_sender.send(()).unwrap(),
        );
        assert_eq!(failure.unwrap_err().to_string(), "rseq setup failed");
        assert!(matches!(
            result_receiver.try_recv(),
            Err(mpsc::TryRecvError::Disconnected)
        ));
    }
}
