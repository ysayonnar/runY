use nix::mount::{MntFlags, MsFlags, mount, umount2};
use nix::unistd::{chdir, pivot_root};
use std::{io, path::Path};

pub(super) fn make_mounts_private() -> io::Result<()> {
    mount(
        None::<&str>,
        "/",
        None::<&str>,
        MsFlags::MS_PRIVATE | MsFlags::MS_REC,
        None::<&str>,
    )
    .map_err(|errno| io::Error::other(format!("make mounts securely private: {}", errno as i32)))?;

    Ok(())
}

pub(super) fn switch_root(rootfs: &Path) -> io::Result<()> {
    mount(
        Some(rootfs),
        rootfs,
        None::<&str>,
        MsFlags::MS_BIND,
        None::<&str>,
    )
    .map_err(|errno| io::Error::other(format!("mount --bind rootfs: {}", errno as i32)))?;

    let old_root = rootfs.join(".oldroot");
    std::fs::create_dir(&old_root)
        .map_err(|e| io::Error::new(e.kind(), format!("failed to create .oldroot dir: {e}")))?;

    pivot_root(rootfs, &old_root)
        .map_err(|errno| io::Error::other(format!("failed to pivot_root: {}", errno as i32)))?;

    chdir("/").map_err(|errno| io::Error::other(format!("failed to chdir: {}", errno as i32)))?;

    umount2("/.oldroot", MntFlags::MNT_DETACH).map_err(|errno| {
        io::Error::other(format!("failed to unmount2 .oldroot: {}", errno as i32))
    })?;

    std::fs::remove_dir("/.oldroot")
        .map_err(|e| io::Error::new(e.kind(), format!("falied to rmdir .oldroot: {e}")))?;

    Ok(())
}

pub(super) fn mount_proc() -> io::Result<()> {
    mount(
        Some("proc"),
        "/proc",
        Some("proc"),
        MsFlags::empty(),
        None::<&str>,
    )
    .map_err(|errno| io::Error::other(format!("failed to mount procfs: {}", errno as i32)))?;

    Ok(())
}
