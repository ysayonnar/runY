use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
pub(crate) struct Args {
    #[command(subcommand)]
    pub(crate) command: Commands,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Commands {
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
