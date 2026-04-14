use anyhow::{Context, Result};
use colored::Colorize;
use std::process::Command;

use crate::{config::VmConfig, gcp};

/// Resolve the IP address of the instance, then exec SSH.
pub async fn connect(
    cfg:        &VmConfig,
    user:       &str,
    key:        Option<&str>,
    extra_args: &[String],
) -> Result<()> {
    // ── 1. Resolve the external IP ─────────────────────────────────────────
    println!(
        "{}  Resolving external IP for {}…",
        "→".cyan(),
        cfg.instance.bold()
    );

    let ip = gcp::get_external_ip(cfg)
        .await
        .context("Could not obtain instance IP address")?;

    println!("{}  Connecting to {}@{}", "→".cyan(), user.bold(), ip.cyan());

    // ── 2. Build the SSH argument list ─────────────────────────────────────
    let destination = format!("{}@{}", user, ip);

    let mut args: Vec<String> = vec![
        // Disable strict host-key checking for ephemeral cloud VMs.
        "-o".into(),
        "StrictHostKeyChecking=no".into(),
        "-o".into(),
        "UserKnownHostsFile=/dev/null".into(),
    ];

    if let Some(key_path) = key {
        args.push("-i".into());
        args.push(key_path.into());
    }

    args.push(destination);

    // Append anything the user added after `--`.
    args.extend_from_slice(extra_args);

    // ── 3. Replace the current process with SSH ────────────────────────────
    //
    // On Unix we use `exec` so SSH inherits the terminal properly (PTY,
    // signals, exit code). On Windows we fall back to spawning a child
    // process and waiting for it.

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = Command::new("ssh").args(&args).exec();
        // `exec` only returns on error.
        return Err(err).context("Failed to exec SSH");
    }

    #[cfg(not(unix))]
    {
        let status = Command::new("ssh")
            .args(&args)
            .status()
            .context("Failed to spawn SSH process")?;

        if !status.success() {
            anyhow::bail!(
                "SSH exited with status: {}",
                status.code().unwrap_or(-1)
            );
        }

        Ok(())
    }
}
