use nix::errno::Errno;
use nix::sys::signal::{SigSet, Signal, killpg};
use nix::sys::signalfd::{SfdFlags, SignalFd};
use nix::sys::wait::{WaitPidFlag, WaitStatus, waitpid};
use nix::unistd::Pid;
use std::io;
use std::os::unix::process::CommandExt;

pub(super) fn setup_signal_fd(cmd: &mut std::process::Command) -> io::Result<SignalFd> {
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

    Ok(signal_fd)
}

pub(super) fn listen_signals(signal_fd: SignalFd, workload_pid: Pid) -> io::Result<i32> {
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
