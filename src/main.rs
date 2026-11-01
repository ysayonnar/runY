use clap::{Parser, Subcommand};
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
    },
}

fn main() {
    let cli = Args::parse();

    match cli.command {
        Commands::Run { command } => match handle_run(command) {
            Ok(exit_code) => std::process::exit(exit_code),
            Err(why) => {
                eprintln!("error occured: {why}");
                std::process::exit(1)
            }
        },
    }
}

fn handle_run(command: Vec<String>) -> io::Result<i32> {
    let exec = command.first().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "expected executive, found nothing",
        )
    })?;

    let mut cmd = std::process::Command::new(exec.as_str());

    if let Some(args) = command.get(1..) {
        for arg in args.iter() {
            cmd.arg(arg.as_str());
        }
    }

    let status = cmd
        .status()
        .map_err(|e| io::Error::new(e.kind(), format!("failed to run {}: {}", exec, e)))?;

    let exit_code = match status.code() {
        Some(code) => code,
        None => 128 + status.signal().unwrap(),
    };

    Ok(exit_code)
}
