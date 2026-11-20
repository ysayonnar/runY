use nix::fcntl::{FcntlArg, FdFlag, fcntl};
use std::os::fd::AsFd;
use std::{format, io};

pub(super) fn close_cloexec(fd: impl AsFd) -> io::Result<()> {
    let mut flags =
        FdFlag::from_bits_retain(fcntl(&fd, FcntlArg::F_GETFD).map_err(|errno| {
            io::Error::other(format!("failed to get fg flags: {}", errno as i32))
        })?);

    flags.remove(FdFlag::FD_CLOEXEC);
    fcntl(&fd, FcntlArg::F_SETFD(flags))
        .map_err(|errno| io::Error::other(format!("failed to sef fd flags: {}", errno as i32)))?;

    Ok(())
}

pub(super) fn close_all_fd_but_std() -> io::Result<()> {
    let result = unsafe { nix::libc::close_range(3, u32::MAX, 0) };
    if result == -1 {
        let error = io::Error::last_os_error();
        return Err(io::Error::new(
            error.kind(),
            format!("failed to close fd: {error}"),
        ));
    };

    Ok(())
}
