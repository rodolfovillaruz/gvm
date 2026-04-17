use google_cloud_compute_v1::client::Instances;
use google_cloud_gax::paginator::ItemPaginator;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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
    while let Some((zone, scoped_list)) = items.next().await.transpose()? {
        for instance in scoped_list.instances {
            println!(
                "Instance {} found in zone: {zone}, status: {}",
                instance.name.expect("name should be Some()"),
                instance.status.expect("status should be Some()"),
            );
        }
    }

    Ok(())
}

/// Checks for Google Cloud credentials availability.
///
/// Returns `true` if either:
/// - `GOOGLE_APPLICATION_CREDENTIALS` env var is set and points to an existing file, or
/// - The platform-specific application default credentials file exists.
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
