mod fd;
mod rootfs;
mod signal;

use nix::fcntl::OFlag;
use nix::sched::{CloneFlags, unshare};
use nix::unistd::Pid;
use nix::unistd::{pipe2, sethostname};
use std::format;
use std::io::Write;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::{fs, io, path::Path};
use uuid::Uuid;

pub(crate) fn run(
    command: Vec<String>,
    hostname: Option<String>,
    rootfs: &Path,
) -> io::Result<i32> {
    let hostname = hostname.unwrap_or_else(|| Uuid::new_v4().to_string());
    let executable = std::env::current_exe()?;

    let (startup_read, startup_write) = pipe2(OFlag::O_CLOEXEC).map_err(io::Error::other)?;
    fd::remove_fdcloexec(&startup_write)?;

    let mut cmd = std::process::Command::new(executable);
    cmd.arg("__bootstrap");
    cmd.arg("--hostname");
    cmd.arg(hostname);
    cmd.arg("--rootfs");
    cmd.arg(rootfs);
    cmd.arg("--startup-fd");
    cmd.arg(startup_write.as_raw_fd().to_string());
    cmd.arg("--");
    cmd.args(command);

    let signal_fd = signal::setup_signal_fd(&mut cmd)?;

    let child = cmd
        .spawn()
        .map_err(|e| io::Error::new(e.kind(), format!("failed to spawn child process: {e}")))?;

    let workload_pid = Pid::from_raw(child.id() as i32);

    drop(startup_write);

    signal::listen_signals(signal_fd, workload_pid)
}

pub(crate) fn bootstrap(
    command: Vec<String>,
    hostname: String,
    rootfs: &Path,
    startup_fd: i32,
) -> io::Result<i32> {
    if startup_fd < 3 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "startup_fd must be >= 3",
        ));
    }

    if unsafe { nix::libc::fcntl(startup_fd, nix::libc::F_GETFD) == -1 } {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "startup_fd is closed",
        ));
    }

    let startup_fd = unsafe { OwnedFd::from_raw_fd(startup_fd) };

    let rootfs = fs::canonicalize(rootfs)
        .map_err(|e| io::Error::new(e.kind(), format!("failed to canonicalize: {e}")))?;

    let executable = std::env::current_exe()?;

    unshare(CloneFlags::CLONE_NEWUTS | CloneFlags::CLONE_NEWPID | CloneFlags::CLONE_NEWNS)
        .map_err(|errno| io::Error::other(format!("create UTS namespace: {}", errno as i32)))?;

    rootfs::make_mounts_private()?;

    sethostname(hostname)
        .map_err(|errno| io::Error::other(format!("set hostname: {}", errno as i32)))?;

    let mut cmd = std::process::Command::new(executable);
    cmd.arg("__container");
    cmd.arg("--rootfs");
    cmd.arg(rootfs);
    cmd.arg("--startup-fd");
    cmd.arg(startup_fd.as_raw_fd().to_string());
    cmd.arg("--");
    cmd.args(command);

    let signal_fd = signal::setup_signal_fd(&mut cmd)?;

    let child = cmd
        .spawn()
        .map_err(|e| io::Error::new(e.kind(), format!("failed to spawn child process: {e}")))?;

    drop(startup_fd);

    let workload_pid = Pid::from_raw(child.id() as i32);

    signal::listen_signals(signal_fd, workload_pid)
}

pub(crate) fn container(command: Vec<String>, rootfs: &Path, startup_fd: i32) -> io::Result<i32> {
    if startup_fd < 3 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "startup_fd must be >= 3",
        ));
    }

    if unsafe { nix::libc::fcntl(startup_fd, nix::libc::F_GETFD) == -1 } {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "startup_fd is closed",
        ));
    }

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

    if startup_fd > 3 {
        let result = unsafe { nix::libc::close_range(3, (startup_fd - 1) as u32, 0) };
        if result == -1 {
            let error = io::Error::last_os_error();
            return Err(io::Error::new(
                error.kind(),
                format!("failed to close fd: {error}"),
            ));
        };
    }

    let result = unsafe { nix::libc::close_range((startup_fd + 1) as u32, u32::MAX, 0) };
    if result == -1 {
        let error = io::Error::last_os_error();
        return Err(io::Error::new(
            error.kind(),
            format!("failed to close fd: {error}"),
        ));
    };

    let startup_fd = unsafe { OwnedFd::from_raw_fd(startup_fd) };
    fd::set_fdcloexec(&startup_fd)?;

    let signal_fd = signal::setup_signal_fd(&mut cmd)?;
    let child = cmd
        .spawn()
        .map_err(|e| io::Error::new(e.kind(), format!("failed to spawn child process: {e}")))?;

    let mut startup_writer = std::fs::File::from(startup_fd);
    if let Err(err) = startup_writer.write_all(b"S") {
        eprintln!("failed to report container startup: {err}");
    }
    drop(startup_writer);

    let workload_pid = Pid::from_raw(child.id() as i32);

    signal::listen_signals(signal_fd, workload_pid)
}
