mod rootfs;

use nix::sched::{CloneFlags, unshare};
use nix::unistd::sethostname;
use std::{fs, io, os::unix::process::CommandExt, os::unix::process::ExitStatusExt, path::Path};
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

    let error = cmd.exec();

    Err(io::Error::new(
        error.kind(),
        format!("failed to exec command: {error}"),
    ))
}
