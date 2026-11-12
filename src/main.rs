mod cli;
mod runtime;

use clap::Parser;
use std::eprintln;

fn main() {
    let cli = cli::Args::parse();

    let result = match cli.command {
        cli::Commands::Run {
            command,
            hostname,
            rootfs,
        } => runtime::run(command, hostname, &rootfs),
        cli::Commands::Bootstrap {
            command,
            hostname,
            rootfs,
        } => runtime::bootstrap(command, hostname, &rootfs),
        cli::Commands::Container { command, rootfs } => runtime::container(command, &rootfs),
    };

    match result {
        Ok(exit_code) => std::process::exit(exit_code),
        Err(why) => {
            eprintln!("error occured: {why}");
            std::process::exit(1)
        }
    }
}
