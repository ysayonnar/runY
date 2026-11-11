use clap::{Parser, Subcommand};
use nix::mount::{MntFlags, MsFlags, mount, umount2};
use nix::sched::{CloneFlags, unshare};
use nix::unistd::{chdir, pivot_root, sethostname};
use std::fs::{self, remove_dir};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::{eprintln, format, io, os::unix::process::ExitStatusExt};
use uuid::Uuid;

#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Run {
        #[arg(last = true, required = true)]
        command: Vec<String>,

        #[arg(long)]
        hostname: Option<String>,

        #[arg(long, short, required = true)]
        rootfs: PathBuf,
    },

    #[command(name = "__bootstrap", hide = true)]
    Bootstrap {
        #[arg(last = true, required = true)]
        command: Vec<String>,

        #[arg(long, required = true)]
        hostname: String,

        #[arg(long, short, required = true)]
        rootfs: PathBuf,
    },

    #[command(name = "__container", hide = true)]
    Container {
        #[arg(last = true, required = true)]
        command: Vec<String>,

        #[arg(long, short, required = true)]
        rootfs: PathBuf,
    },
}

fn handle_run(command: Vec<String>, hostname: Option<String>, rootfs: &Path) -> io::Result<i32> {
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

fn handle_bootstrap(command: Vec<String>, hostname: String, rootfs: &Path) -> io::Result<i32> {
    let rootfs = fs::canonicalize(rootfs)
        .map_err(|e| io::Error::new(e.kind(), format!("failed to canonicalize: {e}")))?;

    let executable = std::env::current_exe()?;

    unshare(CloneFlags::CLONE_NEWUTS | CloneFlags::CLONE_NEWPID | CloneFlags::CLONE_NEWNS)
        .map_err(|errno| io::Error::other(format!("create UTS namespace: {}", errno as i32)))?;

    mount(
        None::<&str>,
        "/",
        None::<&str>,
        MsFlags::MS_PRIVATE | MsFlags::MS_REC,
        None::<&str>,
    )
    .map_err(|errno| io::Error::other(format!("make mounts securely private: {}", errno as i32)))?;

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

fn handle_container(command: Vec<String>, rootfs: &Path) -> io::Result<i32> {
    let entrypoint = command.first().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "expected executive, found nothing",
        )
    })?;

    let mut cmd = std::process::Command::new(entrypoint);
    cmd.args(&command[1..]);

    mount(
        Some(rootfs.as_os_str()),
        rootfs.as_os_str(),
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

    remove_dir("/.oldroot")
        .map_err(|e| io::Error::new(e.kind(), format!("falied to rmdir .oldroot: {e}")))?;

    mount(
        Some("proc"),
        "/proc",
        Some("proc"),
        MsFlags::empty(),
        None::<&str>,
    )
    .map_err(|errno| io::Error::other(format!("failed to mount procfs: {}", errno as i32)))?;

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

fn main() {
    let cli = Args::parse();

    let result = match cli.command {
        Commands::Run {
            command,
            hostname,
            rootfs,
        } => handle_run(command, hostname, &rootfs),
        Commands::Bootstrap {
            command,
            hostname,
            rootfs,
        } => handle_bootstrap(command, hostname, &rootfs),
        Commands::Container { command, rootfs } => handle_container(command, &rootfs),
    };

    match result {
        Ok(exit_code) => std::process::exit(exit_code),
        Err(why) => {
            eprintln!("error occured: {why}");
            std::process::exit(1)
        }
    }
}
