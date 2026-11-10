use clap::{Parser, Subcommand};
use nix::mount::{MntFlags, MsFlags, mount, umount2};
use nix::sched::{CloneFlags, unshare};
use nix::unistd::{chdir, pivot_root, sethostname};
use std::fs::{self, remove_dir};
use std::path::Path;
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
        rootfs: String,
    },

    #[command(name = "__bootstrap", hide = true)]
    Bootstrap {
        #[arg(last = true, required = true)]
        command: Vec<String>,

        #[arg(long, required = true)]
        hostname: String,

        #[arg(long, short, required = true)]
        rootfs: String,
    },
}

fn handle_run(command: Vec<String>, hostname: Option<String>, rootfs: &str) -> io::Result<i32> {
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

fn handle_bootstrap(command: Vec<String>, hostname: String, rootfs: &str) -> io::Result<i32> {
    let exec = command.first().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "expected executive, found nothing",
        )
    })?;

    let rootfs = fs::canonicalize(rootfs)
        .map_err(|e| io::Error::new(e.kind(), format!("failed to canonicalize: {e}")))?;

    let mut cmd = std::process::Command::new(exec);
    cmd.args(&command[1..]);

    unshare(CloneFlags::CLONE_NEWUTS | CloneFlags::CLONE_NEWPID | CloneFlags::CLONE_NEWNS)
        .map_err(|errno| io::Error::other(format!("create UTS namespace: {}", errno as i32)))?;

    sethostname(hostname)
        .map_err(|errno| io::Error::other(format!("set hostname: {}", errno as i32)))?;

    make_mounts(&rootfs)?;

    let status = cmd
        .status()
        .map_err(|e| io::Error::new(e.kind(), format!("failed to run: {e}")))?;

    let exit_code = status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(0));

    Ok(exit_code)
}

fn make_mounts(rootfs: &Path) -> io::Result<()> {
    mount(
        None::<&str>,
        "/",
        None::<&str>,
        MsFlags::MS_PRIVATE | MsFlags::MS_REC,
        None::<&str>,
    )
    .map_err(|e| io::Error::other(format!("make mounts securely private: {e}")))?;

    mount(
        Some(rootfs.as_os_str()),
        rootfs.as_os_str(),
        None::<&str>,
        MsFlags::MS_BIND,
        None::<&str>,
    )
    .map_err(|e| io::Error::other(format!("mount --bind rootfs: {e}")))?;

    let old_root = rootfs.join(".oldroot");
    std::fs::create_dir(&old_root)
        .map_err(|e| io::Error::new(e.kind(), format!("failed to create .oldroot dir: {e}")))?;

    pivot_root(rootfs, &old_root)
        .map_err(|e| io::Error::other(format!("failed to pivot_root: {e}")))?;

    chdir("/").map_err(|e| io::Error::other(format!("failed to chdir: {e}")))?;

    umount2("/.oldroot", MntFlags::MNT_DETACH)
        .map_err(|e| io::Error::other(format!("failed to unmount2 .oldroot: {e}")))?;

    remove_dir("/.oldroot")
        .map_err(|e| io::Error::new(e.kind(), format!("falied to rmdir .oldroot: {e}")))?;

    Ok(())
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
    };

    match result {
        Ok(exit_code) => std::process::exit(exit_code),
        Err(why) => {
            eprintln!("error occured: {why}");
            std::process::exit(1)
        }
    }
}
