use google_cloud_compute_v1::client::Instances;
use google_cloud_gax::paginator::ItemPaginator;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let project_id = std::env::var("GOOGLE_CLOUD_PROJECT")?;

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
