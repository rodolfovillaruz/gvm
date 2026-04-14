#[derive(Debug, Clone)]
pub struct VmConfig {
    pub project:  String,
    pub zone:     String,
    pub instance: String,
}

impl VmConfig {
    /// Base URL for Compute Engine instance operations.
    pub fn instance_url(&self) -> String {
        format!(
            "https://compute.googleapis.com/compute/v1/projects/{}/zones/{}/instances/{}",
            self.project, self.zone, self.instance
        )
    }
}
