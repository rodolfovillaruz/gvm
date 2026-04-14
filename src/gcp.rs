use anyhow::{Context, Result};
use colored::Colorize;
use gcp_auth::AuthenticationManager;
use reqwest::Client;
use serde::Deserialize;

use crate::config::VmConfig;

// ─── GCP auth scopes ──────────────────────────────────────────────────────────

const COMPUTE_SCOPE: &[&str] = &["https://www.googleapis.com/auth/compute"];

// ─── Deserialised API response types ─────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Instance {
    pub name:              String,
    pub status:            String,
    pub network_interfaces: Vec<NetworkInterface>,
    pub machine_type:      String,
    pub zone:              String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkInterface {
    pub network_ip:    Option<String>,
    pub access_configs: Vec<AccessConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessConfig {
    pub nat_ip: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Operation {
    pub name:   String,
    pub status: String,
    #[serde(default)]
    pub error:  Option<OperationError>,
}

#[derive(Debug, Deserialize)]
pub struct OperationError {
    pub errors: Vec<ErrorItem>,
}

#[derive(Debug, Deserialize)]
pub struct ErrorItem {
    pub _code:    String,
    pub message: String,
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

async fn bearer_token() -> Result<String> {
    let manager = AuthenticationManager::new()
        .await
        .context("Failed to initialise GCP authentication. \
                  Make sure Application Default Credentials are configured \
                  (`gcloud auth application-default login`).")?;

    let token = manager
        .get_token(COMPUTE_SCOPE)
        .await
        .context("Failed to obtain an access token")?;

    Ok(token.as_str().to_owned())
}

fn http_client() -> Result<Client> {
    Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .context("Failed to build HTTP client")
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Fetch the full Instance resource from the Compute Engine API.
pub async fn get_instance(cfg: &VmConfig) -> Result<Instance> {
    let token  = bearer_token().await?;
    let client = http_client()?;

    let response = client
        .get(&cfg.instance_url())
        .bearer_auth(&token)
        .send()
        .await
        .context("HTTP request to Compute Engine failed")?;

    let status = response.status();
    let body   = response.text().await?;

    if !status.is_success() {
        anyhow::bail!(
            "Compute Engine returned HTTP {}: {}",
            status,
            body
        );
    }

    serde_json::from_str::<Instance>(&body)
        .context("Failed to parse Instance response")
}

/// Print the current status of the VM to stdout.
pub async fn get_status(cfg: &VmConfig) -> Result<()> {
    println!(
        "{}  Fetching status for instance {}…",
        "→".cyan(),
        cfg.instance.bold()
    );

    let instance = get_instance(cfg).await?;

    let status_colored = match instance.status.as_str() {
        "RUNNING"    => instance.status.green().bold(),
        "TERMINATED" => instance.status.red().bold(),
        "STAGING"    => instance.status.yellow().bold(),
        "STOPPING"   => instance.status.yellow().bold(),
        other        => other.normal().bold(),
    };

    // Extract the short machine-type name (last path segment).
    let machine_type = instance
        .machine_type
        .split('/')
        .last()
        .unwrap_or(&instance.machine_type)
        .to_owned();

    let zone = instance
        .zone
        .split('/')
        .last()
        .unwrap_or(&instance.zone)
        .to_owned();

    println!("{}", "─".repeat(40).dimmed());
    println!("  {:<16} {}", "Instance:".dimmed(),      instance.name.bold());
    println!("  {:<16} {}",  "Status:".dimmed(),       status_colored);
    println!("  {:<16} {}",  "Zone:".dimmed(),         zone);
    println!("  {:<16} {}",  "Machine type:".dimmed(), machine_type);

    for (i, nic) in instance.network_interfaces.iter().enumerate() {
        if let Some(ref ip) = nic.network_ip {
            println!("  {:<16} {}", format!("NIC[{}] internal:", i).dimmed(), ip);
        }
        for ac in &nic.access_configs {
            if let Some(ref nat) = ac.nat_ip {
                println!("  {:<16} {}", format!("NIC[{}] external:", i).dimmed(), nat.cyan());
            }
        }
    }

    println!("{}", "─".repeat(40).dimmed());
    Ok(())
}

/// Send a `start` action to the Compute Engine API.
pub async fn start_vm(cfg: &VmConfig) -> Result<()> {
    println!(
        "{}  Starting instance {}…",
        "▶".green(),
        cfg.instance.bold()
    );

    // Check current status first to give useful feedback.
    let instance = get_instance(cfg).await?;
    if instance.status == "RUNNING" {
        println!(
            "{}  Instance is already {}.",
            "✓".green(),
            "RUNNING".green().bold()
        );
        return Ok(());
    }

    let token  = bearer_token().await?;
    let client = http_client()?;

    let url = format!("{}/start", cfg.instance_url());

    let response = client
        .post(&url)
        .bearer_auth(&token)
        .header("Content-Length", "0")
        .send()
        .await
        .context("HTTP POST to start instance failed")?;

    let status = response.status();
    let body   = response.text().await?;

    if !status.is_success() {
        anyhow::bail!(
            "Start operation returned HTTP {}: {}",
            status,
            body
        );
    }

    let operation: Operation =
        serde_json::from_str(&body).context("Failed to parse Operation response")?;

    if let Some(err) = operation.error {
        let messages: Vec<_> = err.errors.iter().map(|e| e.message.as_str()).collect();
        anyhow::bail!("Start operation failed: {}", messages.join("; "));
    }

    println!(
        "{}  Start operation {} dispatched (status: {}).",
        "✓".green(),
        operation.name.bold(),
        operation.status.yellow()
    );
    println!(
        "{}  The instance may take ~30 s to reach RUNNING state.",
        "ℹ".blue()
    );

    Ok(())
}

/// Resolve the external (NAT) IP address of the instance.
pub async fn get_external_ip(cfg: &VmConfig) -> Result<String> {
    let instance = get_instance(cfg).await?;

    instance
        .network_interfaces
        .iter()
        .find_map(|nic| {
            nic.access_configs
                .iter()
                .find_map(|ac| ac.nat_ip.clone())
        })
        .context(
            "No external IP found on the instance. \
             Make sure it has an access config with a NAT IP.",
        )
}
