use google_cloud_compute_v1::client::Instances;
use google_cloud_gax::paginator::ItemPaginator;
use std::process::Command;
use std::time::{Duration, Instant};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let program = args.first().cloned().unwrap_or_else(|| "gvm".to_string());

    if args.len() < 2 {
        eprintln!("Usage: {program} <start|status|ssh> [args...]");
        return Err("missing subcommand".into());
    }

    let subcommand = args[1].clone();
    let rest: Vec<String> = args[2..].to_vec();

    // GVM_INSTANCE is required for every subcommand.
    let instance_name = std::env::var("GVM_INSTANCE").map_err(|_| {
        eprintln!("\x1b[31mError:\x1b[0m GVM_INSTANCE environment variable is not set.");
        "missing GVM_INSTANCE"
    })?;

    let project_id = std::env::var("GOOGLE_CLOUD_PROJECT").map_err(|_| {
        eprintln!("\x1b[31mError:\x1b[0m GOOGLE_CLOUD_PROJECT environment variable is not set.");
        "missing GOOGLE_CLOUD_PROJECT"
    })?;

    if !credentials_available() {
        eprintln!(
            "\x1b[31mError:\x1b[0m No Google Cloud credentials found.\n\
             Set the GOOGLE_APPLICATION_CREDENTIALS environment variable to point to a \
             service account key file, or run `gcloud auth application-default login` \
             to create application default credentials."
        );
        return Err("missing credentials".into());
    }

    let client = Instances::builder().build().await?;

    // Look the instance up in the aggregated list to confirm it exists and
    // to discover its zone and current state.
    let (zone, status, ip) = find_instance(&client, &project_id, &instance_name)
        .await?
        .ok_or_else(|| format!("instance `{instance_name}` not found in project `{project_id}`"))?;

    match subcommand.as_str() {
        "status" => {
            let simple = if status == "RUNNING" {
                "RUNNING"
            } else {
                "STOPPED"
            };
            println!("{simple}");
            Ok(())
        }

        "ssh" => {
            // GVM_USER is required for ssh.
            let user = require_gvm_user()?;

            let ip = ip
                .ok_or_else(|| format!("instance `{instance_name}` has no reachable IP address"))?;

            eprintln!(
                "\x1b[32mConnecting\x1b[0m to {user}@{ip} \
                 (instance `{instance_name}` in zone {zone})"
            );

            let status = Command::new("ssh")
                .arg(format!("{user}@{ip}"))
                .args(&rest)
                .status()?;

            std::process::exit(status.code().unwrap_or(1));
        }

        "start" => {
            // Require GVM_USER up front so we don't wait for a start just to
            // discover we can't complete the ssh step.
            let user = require_gvm_user()?;

            if status == "RUNNING" {
                eprintln!("Instance `{instance_name}` is already RUNNING.");
            } else {
                eprintln!(
                    "\x1b[32mStarting\x1b[0m instance `{instance_name}` in zone {zone} \
                     (current status: {status})..."
                );
                client
                    .start()
                    .set_project(project_id.clone())
                    .set_zone(zone.clone())
                    .set_instance(instance_name.clone())
                    .send()
                    .await?;
            }

            let timeout_secs: u64 = std::env::var("GVM_START_TIMEOUT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(180);

            let ip = wait_for_ssh(
                &client,
                &project_id,
                &zone,
                &instance_name,
                Duration::from_secs(timeout_secs),
            )
            .await?;

            eprintln!(
                "\x1b[32mConnecting\x1b[0m to {user}@{ip} \
                 (instance `{instance_name}` in zone {zone})"
            );

            let status = Command::new("ssh")
                .arg(format!("{user}@{ip}"))
                .args(&rest)
                .status()?;

            std::process::exit(status.code().unwrap_or(1));
        }

        other => {
            eprintln!(
                "Unknown subcommand `{other}`. \
                 Usage: {program} <start|status|ssh> [args...]"
            );
            Err(format!("unknown subcommand: {other}").into())
        }
    }
}

fn require_gvm_user() -> Result<String, Box<dyn std::error::Error>> {
    std::env::var("GVM_USER").map_err(|_| {
        eprintln!("\x1b[31mError:\x1b[0m GVM_USER environment variable is not set.");
        "missing GVM_USER".into()
    })
}

/// Scan the aggregated instance list for `instance_name`.
///
/// Returns `(zone, status, optional_ip)`. `zone` is stripped of its
/// `zones/` prefix.
async fn find_instance(
    client: &Instances,
    project_id: &str,
    instance_name: &str,
) -> Result<Option<(String, String, Option<String>)>, Box<dyn std::error::Error>> {
    let mut items = client
        .aggregated_list()
        .set_project(project_id.to_string())
        .by_item();

    while let Some((zone_key, scoped_list)) = items.next().await.transpose()? {
        for instance in scoped_list.instances {
            if instance.name.as_deref() != Some(instance_name) {
                continue;
            }

            let zone = zone_key
                .strip_prefix("zones/")
                .unwrap_or(&zone_key)
                .to_string();
            let status = instance
                .status
                .map(|s| format!("{s:?}").to_uppercase())
                .unwrap_or_default();

            // Prefer external (NAT) IP, fall back to internal IP.
            let mut ip: Option<String> = None;
            for ni in &instance.network_interfaces {
                for ac in &ni.access_configs {
                    if let Some(candidate) = ac.nat_ip.as_deref() {
                        if !candidate.is_empty() {
                            ip = Some(candidate.to_string());
                            break;
                        }
                    }
                }
                if ip.is_some() {
                    break;
                }
            }
            if ip.is_none() {
                if let Some(ni) = instance.network_interfaces.first() {
                    if let Some(candidate) = ni.network_ip.as_deref() {
                        if !candidate.is_empty() {
                            ip = Some(candidate.to_string());
                        }
                    }
                }
            }

            return Ok(Some((zone, status, ip)));
        }
    }

    Ok(None)
}

/// Poll the instance until it is RUNNING and we can open a TCP
/// connection to its SSH port, or until `timeout` elapses.
async fn wait_for_ssh(
    client: &Instances,
    project_id: &str,
    zone: &str,
    instance_name: &str,
    timeout: Duration,
) -> Result<String, Box<dyn std::error::Error>> {
    let start = Instant::now();
    let mut last_status = String::new();
    let mut last_ip: Option<String> = None;

    loop {
        if start.elapsed() >= timeout {
            return Err(format!(
                "timed out after {}s waiting for `{instance_name}` to accept SSH",
                timeout.as_secs()
            )
            .into());
        }

        let instance = client
            .get()
            .set_project(project_id.to_string())
            .set_zone(zone.to_string())
            .set_instance(instance_name.to_string())
            .send()
            .await?;

        let status = instance
            .status
            .map(|s| format!("{s:?}").to_uppercase())
            .unwrap_or_default();
        if status != last_status {
            eprintln!("  instance status: {status}");
            last_status = status.clone();
        }

        // Extract IP (prefer NAT / external).
        let mut ip: Option<String> = None;
        for ni in &instance.network_interfaces {
            for ac in &ni.access_configs {
                if let Some(candidate) = ac.nat_ip.as_deref() {
                    if !candidate.is_empty() {
                        ip = Some(candidate.to_string());
                        break;
                    }
                }
            }
            if ip.is_some() {
                break;
            }
        }
        if ip.is_none() {
            if let Some(ni) = instance.network_interfaces.first() {
                if let Some(candidate) = ni.network_ip.as_deref() {
                    if !candidate.is_empty() {
                        ip = Some(candidate.to_string());
                    }
                }
            }
        }

        if status == "RUNNING" {
            if let Some(ip) = ip.clone() {
                if last_ip.as_deref() != Some(ip.as_str()) {
                    eprintln!("  probing SSH on {ip}:22 ...");
                    last_ip = Some(ip.clone());
                }

                let probe = tokio::time::timeout(
                    Duration::from_secs(3),
                    tokio::net::TcpStream::connect(format!("{ip}:22")),
                )
                .await;

                if let Ok(Ok(_)) = probe {
                    eprintln!("\x1b[32mSSH is ready\x1b[0m on {ip}");
                    return Ok(ip);
                }
            }
        }

        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}

fn credentials_available() -> bool {
    #[cfg(not(windows))]
    if let Ok(path) = std::env::var("GOOGLE_APPLICATION_CREDENTIALS") {
        let path = std::path::Path::new(&path);
        if path.is_file() {
            return true;
        }
        eprintln!(
            "\x1b[33mWarning:\x1b[0m GOOGLE_APPLICATION_CREDENTIALS is set to `{}`, \
             but that file does not exist.",
            path.display()
        );
    }

    if let Some(path) = application_default_credentials_path() {
        if path.is_file() {
            return true;
        }
    }

    false
}

fn application_default_credentials_path() -> Option<std::path::PathBuf> {
    #[cfg(windows)]
    {
        let appdata = std::env::var_os("LOCALAPPDATA")?;
        Some(
            std::path::PathBuf::from(appdata)
                .join("gcloud")
                .join("application_default_credentials.json"),
        )
    }
    #[cfg(not(windows))]
    {
        let home = std::env::var_os("HOME")?;
        Some(
            std::path::PathBuf::from(home)
                .join(".config")
                .join("gcloud")
                .join("application_default_credentials.json"),
        )
    }
}
