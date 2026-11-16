mod rootfs;

use nix::errno::Errno;
use nix::sched::{CloneFlags, unshare};
use nix::sys::signal::{SigSet, Signal, killpg};
use nix::sys::signalfd::{SfdFlags, SignalFd};
use nix::sys::wait::{WaitPidFlag, WaitStatus, waitpid};
use nix::unistd::{Pid, sethostname};
use std::format;
use std::os::unix::process::CommandExt;
use std::{fs, io, os::unix::process::ExitStatusExt, path::Path};
use uuid::Uuid;

pub(crate) fn run(
    command: Vec<String>,
    hostname: Option<String>,
    rootfs: &Path,
) -> io::Result<i32> {
    let hostname = hostname.unwrap_or_else(|| Uuid::new_v4().to_string());
    let executable = std::env::current_exe()?;

    let status = std::process::Command::new(executable)
        .arg("__bootstrap")
        .arg("--hostname")
        .arg(hostname)
        .arg("--rootfs")
        .arg(rootfs)
        .arg("--")
        .args(command)
        .status()
        .map_err(|e| io::Error::new(e.kind(), format!("failed to run self process: {}", e)))?;

    let exit_code = status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(0));

    Ok(exit_code)
}

pub(crate) fn bootstrap(command: Vec<String>, hostname: String, rootfs: &Path) -> io::Result<i32> {
    let rootfs = fs::canonicalize(rootfs)
        .map_err(|e| io::Error::new(e.kind(), format!("failed to canonicalize: {e}")))?;

    let executable = std::env::current_exe()?;

    unshare(CloneFlags::CLONE_NEWUTS | CloneFlags::CLONE_NEWPID | CloneFlags::CLONE_NEWNS)
        .map_err(|errno| io::Error::other(format!("create UTS namespace: {}", errno as i32)))?;

    rootfs::make_mounts_private()?;

    sethostname(hostname)
        .map_err(|errno| io::Error::other(format!("set hostname: {}", errno as i32)))?;

    let status = std::process::Command::new(executable)
        .arg("__container")
        .arg("--rootfs")
        .arg(rootfs)
        .arg("--")
        .args(command)
        .status()
        .map_err(|e| io::Error::new(e.kind(), format!("failed to run self process: {}", e)))?;

    let exit_code = status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(0));

    Ok(exit_code)
}

pub(crate) fn container(command: Vec<String>, rootfs: &Path) -> io::Result<i32> {
    let entrypoint = command.first().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "expected executive, found nothing",
        )
    })?;

    let mut cmd = std::process::Command::new(entrypoint);
    cmd.args(&command[1..]);

    rootfs::switch_root(rootfs)?;
    rootfs::mount_proc()?;

    let result = unsafe { nix::libc::close_range(3, u32::MAX, 0) };
    if result == -1 {
        let error = io::Error::last_os_error();
        return Err(io::Error::new(
            error.kind(),
            format!("failed to close fd: {error}"),
        ));
    }

    let original_mask = SigSet::thread_get_mask().map_err(io::Error::other)?;
    let mut mask = SigSet::empty();
    mask.add(Signal::SIGINT);
    mask.add(Signal::SIGTERM);
    mask.add(Signal::SIGCHLD);
    mask.thread_block().map_err(io::Error::other)?;

    let signal_fd = SignalFd::with_flags(&mask, SfdFlags::SFD_CLOEXEC).map_err(io::Error::other)?;

    cmd.process_group(0);

    unsafe {
        cmd.pre_exec(move || original_mask.thread_set_mask().map_err(io::Error::other));
    }

    let child = cmd
        .spawn()
        .map_err(|e| io::Error::new(e.kind(), format!("failed to spawn child process: {e}")))?;

    let workload_pid = Pid::from_raw(child.id() as i32);

    loop {
        let siginfo = match signal_fd.read_signal() {
            Ok(Some(siginfo)) => siginfo,
            Ok(None) => continue,
            Err(Errno::EINTR) => continue,
            Err(error) => return Err(io::Error::other(error)),
        };

        let signal = Signal::try_from(siginfo.ssi_signo as i32).map_err(io::Error::other)?;

        match signal {
            Signal::SIGTERM | Signal::SIGINT => match killpg(workload_pid, signal) {
                Ok(()) => {}
                Err(Errno::ESRCH) => {}
                Err(error) => return Err(io::Error::other(error)),
            },
            Signal::SIGCHLD => {}
            _ => continue,
        }

        loop {
            match waitpid(None, Some(WaitPidFlag::WNOHANG)) {
                Ok(WaitStatus::Exited(pid, code)) => {
                    if pid == workload_pid {
                        return Ok(code);
                    }
                }
                Ok(WaitStatus::Signaled(pid, signal, _)) => {
                    if pid == workload_pid {
                        return Ok(128 + signal as i32);
                    }
                }
                Ok(WaitStatus::StillAlive) => break,
                Err(Errno::ECHILD) => break,
                Err(Errno::EINTR) => continue,
                Err(error) => return Err(io::Error::other(error)),
                Ok(_) => {}
            }
        }
    }
}
