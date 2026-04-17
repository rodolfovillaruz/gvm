use google_cloud_compute_v1::client::Instances;
use google_cloud_gax::paginator::ItemPaginator;
use std::process::Command;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!(
            "Usage: {} [user@]<instance-name> [ssh-args...]",
            args.first().map(String::as_str).unwrap_or("gcp-ssh")
        );
        return Err("missing instance name".into());
    }

    let target = &args[1];
    let ssh_extra_args = &args[2..];

    let (user, instance_name) = match target.split_once('@') {
        Some((u, n)) => (u.to_string(), n.to_string()),
        None => ("ubuntu".to_string(), target.clone()),
    };

    let project_id = match std::env::var("GOOGLE_CLOUD_PROJECT") {
        Ok(id) => id,
        Err(_) => {
            eprintln!(
                "\x1b[33mWarning:\x1b[0m GOOGLE_CLOUD_PROJECT environment variable is not set."
            );
            return Ok(());
        }
    };

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
    let mut items = client.aggregated_list().set_project(project_id).by_item();

    let mut ip_addr: Option<String> = None;
    let mut found_zone: Option<String> = None;

    'outer: while let Some((zone, scoped_list)) = items.next().await.transpose()? {
        for instance in scoped_list.instances {
            if instance.name.as_deref() != Some(instance_name.as_str()) {
                continue;
            }
            found_zone = Some(zone.clone());

            // Prefer external (NAT) IP, fall back to internal IP.
            for ni in &instance.network_interfaces {
                for ac in &ni.access_configs {
                    if let Some(ip) = ac.nat_ip.as_deref() {
                        if !ip.is_empty() {
                            ip_addr = Some(ip.to_string());
                            break;
                        }
                    }
                }
                if ip_addr.is_some() {
                    break;
                }
            }
            if ip_addr.is_none() {
                if let Some(ni) = instance.network_interfaces.first() {
                    if let Some(ip) = ni.network_ip.as_deref() {
                        if !ip.is_empty() {
                            ip_addr = Some(ip.to_string());
                        }
                    }
                }
            }
            break 'outer;
        }
    }

    let Some(ip) = ip_addr else {
        if found_zone.is_some() {
            return Err(format!("instance `{instance_name}` has no reachable IP address").into());
        }
        return Err(format!("instance `{instance_name}` not found in project").into());
    };

    let zone = found_zone.unwrap_or_default();
    eprintln!(
        "\x1b[32mConnecting\x1b[0m to {user}@{ip} (instance `{instance_name}` in zone {zone})"
    );

    let status = Command::new("ssh")
        .arg(format!("{user}@{ip}"))
        .args(ssh_extra_args)
        .status()?;

    std::process::exit(status.code().unwrap_or(1));
}

fn credentials_available() -> bool {
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
        let appdata = std::env::var_os("APPDATA")?;
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
