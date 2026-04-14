mod config;
mod gcp;
mod ssh;

use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::Colorize;

use config::VmConfig;

// ─── CLI definition ───────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(
    name    = "gvm",
    about   = "Manage a Google Cloud Compute Engine VM from the command line.",
    version,
    propagate_version = true
)]
struct Cli {
    /// GCP project ID.
    #[arg(long, env = "GVM_PROJECT", global = true)]
    project: String,

    /// Compute Engine zone (e.g. `us-central1-a`).
    #[arg(
        long,
        env     = "GVM_ZONE",
        global  = true,
        default_value = "us-central1-a"
    )]
    zone: String,

    /// Instance name.
    #[arg(long, env = "GVM_INSTANCE", global = true)]
    instance: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start (power on) the VM.
    Start,

    /// Print the current status and IP addresses of the VM.
    Status,

    /// Open an interactive SSH session to the VM.
    Ssh {
        /// Remote username.
        #[arg(long, short, default_value = "ubuntu")]
        user: String,

        /// Path to the private key file (passed as `ssh -i <KEY>`).
        #[arg(long, short = 'i')]
        key: Option<String>,

        /// Extra arguments forwarded verbatim to `ssh` (place after `--`).
        ///
        /// Example: `gvm ssh -- -L 8080:localhost:8080`
        #[arg(last = true)]
        extra: Vec<String>,
    },
}

// ─── Entry point ─────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let cfg = VmConfig {
        project:  cli.project.clone(),
        zone:     cli.zone.clone(),
        instance: cli.instance.clone(),
    };

    let result = match cli.command {
        Commands::Start => gcp::start_vm(&cfg).await,
        Commands::Status => gcp::get_status(&cfg).await,
        Commands::Ssh { user, key, extra } => {
            ssh::connect(&cfg, &user, key.as_deref(), &extra).await
        }
    };

    if let Err(e) = result {
        eprintln!("{} {:#}", "error:".red().bold(), e);
        std::process::exit(1);
    }

    Ok(())
}
