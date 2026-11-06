use clap::{Parser, Subcommand};
use nix::mount::{MsFlags, mount, umount};
use nix::sched::{CloneFlags, unshare};
use nix::unistd::sethostname;
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
    },

    #[command(name = "__bootstrap", hide = true)]
    Bootstrap {
        #[arg(last = true, required = true)]
        command: Vec<String>,

        #[arg(long, required = true)]
        hostname: String,
    },
}

fn handle_run(command: Vec<String>, hostname: Option<String>) -> io::Result<i32> {
    let hostname = hostname.unwrap_or_else(|| Uuid::new_v4().to_string());
    let executable = std::env::current_exe()?;

    let status = std::process::Command::new(executable)
        .arg("__bootstrap")
        .arg("--hostname")
        .arg(hostname)
        .arg("--")
        .args(command)
        .status()
        .map_err(|e| io::Error::new(e.kind(), format!("failed to run self process: {}", e)))?;

    let exit_code = status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(0));

    Ok(exit_code)
}

fn handle_bootstrap(command: Vec<String>, hostname: String) -> io::Result<i32> {
    let exec = command.first().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "expected executive, found nothing",
        )
    })?;

    let mut cmd = std::process::Command::new(exec);
    cmd.args(&command[1..]);

    unshare(CloneFlags::CLONE_NEWUTS | CloneFlags::CLONE_NEWPID | CloneFlags::CLONE_NEWNS)
        .map_err(|errno| io::Error::other(format!("create UTS namespace: {}", errno as i32)))?;

    sethostname(hostname)
        .map_err(|errno| io::Error::other(format!("set hostname: {}", errno as i32)))?;

    make_mounts()?;

    let command_result = cmd.status();
    let cleanup_result =
        umount("/mnt/runy-test").map_err(|e| io::Error::other(format!("unmount test tmpfs: {e}")));

    let status = match command_result {
        Ok(status) => {
            cleanup_result?;
            status
        }
        Err(e) => {
            if let Err(cleanup_error) = cleanup_result {
                eprintln!("cleanup failed: {cleanup_error}");
            }
            return Err(io::Error::new(e.kind(), format!("failed to run: {e}")));
        }
    };

    let exit_code = status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(0));

    Ok(exit_code)
}

fn make_mounts() -> io::Result<()> {
    mount(
        None::<&str>,
        "/",
        None::<&str>,
        MsFlags::MS_PRIVATE | MsFlags::MS_REC,
        None::<&str>,
    )
    .map_err(|e| io::Error::other(format!("make mounts securely private: {e}")))?;

    mount(
        Some("tmpfs"),
        "/mnt/runy-test",
        Some("tmpfs"),
        MsFlags::empty(),
        Some("size=16m"),
    )
    .map_err(|e| io::Error::other(format!("mount test tmpfs: {e}")))?;

    Ok(())
}

fn main() {
    let cli = Args::parse();

    let result = match cli.command {
        Commands::Run { command, hostname } => handle_run(command, hostname),
        Commands::Bootstrap { command, hostname } => handle_bootstrap(command, hostname),
    };

    match result {
        Ok(exit_code) => std::process::exit(exit_code),
        Err(why) => {
            eprintln!("error occured: {why}");
            std::process::exit(1)
        }
    }
}
