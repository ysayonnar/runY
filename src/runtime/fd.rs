use nix::fcntl::{FcntlArg, FdFlag, fcntl};
use std::os::fd::AsFd;
use std::{format, io};

pub(super) fn remove_fdcloexec(fd: impl AsFd) -> io::Result<()> {
    let mut flags =
        FdFlag::from_bits_retain(fcntl(&fd, FcntlArg::F_GETFD).map_err(|errno| {
            io::Error::other(format!("failed to get fg flags: {}", errno as i32))
        })?);

    flags.remove(FdFlag::FD_CLOEXEC);
    fcntl(&fd, FcntlArg::F_SETFD(flags))
        .map_err(|errno| io::Error::other(format!("failed to sef fd flags: {}", errno as i32)))?;

    Ok(())
}

pub(super) fn set_fdcloexec(fd: impl AsFd) -> io::Result<()> {
    let mut flags =
        FdFlag::from_bits_retain(fcntl(&fd, FcntlArg::F_GETFD).map_err(|errno| {
            io::Error::other(format!("failed to get fg flags: {}", errno as i32))
        })?);

    flags.insert(FdFlag::FD_CLOEXEC);
    fcntl(&fd, FcntlArg::F_SETFD(flags))
        .map_err(|errno| io::Error::other(format!("failed to sef fd flags: {}", errno as i32)))?;

    Ok(())
}
