use super::service_registry::GrpcServiceRegistry;

impl Default for GrpcServiceRegistry {
    fn default() -> Self {
        Self::new()
    }
}