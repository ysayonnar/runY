use clap::{Parser, Subcommand};
use nix::sched::{CloneFlags, unshare};
use nix::unistd::sethostname;
use std::os::unix::process::CommandExt;
use std::{eprintln, format, io, os::unix::process::ExitStatusExt};

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

        #[arg(long, required = true)]
        hostname: String,
    },

    #[command(name = "__bootstrap", hide = true)]
    Bootstrap {
        #[arg(last = true, required = true)]
        command: Vec<String>,

        #[arg(long, required = true)]
        hostname: String,
    },
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

fn handle_run(command: Vec<String>, hostname: String) -> io::Result<i32> {
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

    unshare(CloneFlags::CLONE_NEWUTS)
        .map_err(|errno| io::Error::other(format!("create UTS namespace: {}", errno as i32)))?;

    sethostname(&hostname)
        .map_err(|errno| io::Error::other(format!("set hostname: {}", errno as i32)))?;

    let error = cmd.exec();

    Err(io::Error::new(
        error.kind(),
        format!("failed to execute {}: {}", exec, error),
    ))
}
