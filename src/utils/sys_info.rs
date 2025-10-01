use std::sync::OnceLock;
use sys_info as sysinfo;

static SYS_INFO: OnceLock<SystemInfo> = OnceLock::new();

#[derive(Debug)]
pub struct SystemInfo {
    pub cpu_cores: usize,
    pub total_memory: u64,
    pub os_name: String,
    pub os_version: String,
}

impl SystemInfo {
    pub fn global() -> &'static Self {
        SYS_INFO.get_or_init(|| {
            SystemInfo {
                cpu_cores: num_cpus::get(),
                total_memory: sysinfo::mem_info()
                    .map(|m| m.total)
                    .unwrap_or(0) * 1024,
                os_name: sysinfo::os_type().unwrap_or_else(|_| "Unknown".to_string()),
                os_version: sysinfo::os_release().unwrap_or_else(|_| "Unknown".to_string()),
            }
        })
    }
}