use log::{error, info};
use roadwork_core::opendata::json::model::service_descriptor::ServiceDescriptor;
use std::collections::HashMap;
use std::path::PathBuf;

pub struct DescriptorManager {
    data_dir: PathBuf,
    descriptors: HashMap<String, ServiceDescriptor>,
}

impl DescriptorManager {
    pub fn new(data_dir: PathBuf) -> Self {
        let mut descriptor_manager = Self {
            data_dir,
            descriptors: HashMap::new(),
        };
        descriptor_manager.descriptors = descriptor_manager.load_descriptors();
        descriptor_manager
    }

    fn descriptors_path(&self) -> PathBuf {
        self.data_dir.join("descriptors.json")
    }

    pub fn descriptor(&self, service_name: &str) -> Option<&ServiceDescriptor> {
        self.descriptors.get(service_name)
    }

    pub fn descriptor_names(&self) -> Vec<String> {
        self.descriptors.keys().cloned().collect()
    }

    fn load_descriptors(&self) -> HashMap<String, ServiceDescriptor> {
        let path = self.descriptors_path();
        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(content) => {
                    match serde_json::from_str::<HashMap<String, ServiceDescriptor>>(&content) {
                        Ok(descriptors) => {
                            info!(
                                "Loaded {} descriptors from {}",
                                descriptors.len(),
                                path.display()
                            );
                            return descriptors;
                        }
                        Err(e) => error!("Failed to parse descriptors: {e}"),
                    }
                }
                Err(e) => error!("Failed to read descriptors: {e}"),
            }
        }
        let descriptors = roadwork_service::load_descriptors();
        info!("Loaded {} built-in descriptors", descriptors.len());
        descriptors
    }

    pub fn save_descriptors(&self, descriptors: &HashMap<String, ServiceDescriptor>) {
        let path = self.descriptors_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        match serde_json::to_string_pretty(descriptors) {
            Ok(content) => {
                if let Err(e) = std::fs::write(&path, content) {
                    error!("Failed to write descriptors: {e}");
                } else {
                    info!("Saved {} descriptors", descriptors.len());
                }
            }
            Err(e) => error!("Failed to serialize descriptors: {e}"),
        }
    }
}
