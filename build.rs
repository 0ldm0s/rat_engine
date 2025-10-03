//! RAT Engine 构建脚本
//! 
//! 在编译时进行优化配置和环境检查

use std::env;
use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
    
    // 设置编译时环境变量
    set_build_info();
    
    // 检查目标平台
    check_target_platform();
    
    // 优化配置
    set_optimization_flags();
    
    // 生成版本信息
    generate_version_info();

  }

/// 设置构建信息
fn set_build_info() {
    // 构建时间
    let build_time = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string();
    println!("cargo:rustc-env=BUILD_TIME={}", build_time);
    
    // Git信息（如果可用）
    if let Ok(output) = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
    {
        if output.status.success() {
            let git_hash = String::from_utf8_lossy(&output.stdout).trim().to_string();
            println!("cargo:rustc-env=GIT_HASH={}", git_hash);
        }
    }
    
    // 构建配置
    let profile = env::var("PROFILE").unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=BUILD_PROFILE={}", profile);
    
    // 目标架构
    let target = env::var("TARGET").unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=BUILD_TARGET={}", target);
}

/// 检查目标平台
fn check_target_platform() {
    let target = env::var("TARGET").unwrap_or_default();
    
    // 针对不同平台设置特定配置
    if target.contains("linux") {
        println!("cargo:rustc-cfg=target_os_linux");
        // Linux特定优化
        println!("cargo:rustc-link-arg=-Wl,--as-needed");
    } else if target.contains("darwin") {
        println!("cargo:rustc-cfg=target_os_macos");
        // macOS特定优化
    } else if target.contains("windows") {
        println!("cargo:rustc-cfg=target_os_windows");
        // Windows特定优化
    }
    
    // 检查是否为64位架构
    if target.contains("x86_64") || target.contains("aarch64") {
        println!("cargo:rustc-cfg=target_arch_64bit");
    }
}

/// 设置优化标志
fn set_optimization_flags() {
    let profile = env::var("PROFILE").unwrap_or_default();
    
    if profile == "release" {
        // Release模式优化
        println!("cargo:rustc-cfg=optimized_build");
        
        // 启用LTO（链接时优化）
        println!("cargo:rustc-env=CARGO_CFG_TARGET_FEATURE=+lto");
        
        // 针对本机CPU优化（如果是本地构建）
        if env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default() == env::var("HOST").unwrap_or_default().split('-').next().unwrap_or("") {
            println!("cargo:rustc-env=RUSTFLAGS=-C target-cpu=native");
        }
    } else {
        // Debug模式配置
        println!("cargo:rustc-cfg=debug_build");
    }
}

/// 生成版本信息文件
fn generate_version_info() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("version_info.rs");
    
    let version = env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "unknown".to_string());
    let name = env::var("CARGO_PKG_NAME").unwrap_or_else(|_| "rat_engine".to_string());
    let authors = env::var("CARGO_PKG_AUTHORS").unwrap_or_else(|_| "Unknown".to_string());
    let description = env::var("CARGO_PKG_DESCRIPTION").unwrap_or_else(|_| "RAT Engine".to_string());
    
    let build_time = env::var("BUILD_TIME").unwrap_or_else(|_| "unknown".to_string());
    let git_hash = env::var("GIT_HASH").unwrap_or_else(|_| "unknown".to_string());
    let profile = env::var("BUILD_PROFILE").unwrap_or_else(|_| "unknown".to_string());
    let target = env::var("BUILD_TARGET").unwrap_or_else(|_| "unknown".to_string());
    
    let version_info = format!(
        "
/// 构建时生成的版本信息
pub const VERSION_INFO: VersionInfo = VersionInfo {{
    name: \"{}\",
    version: \"{}\",
    authors: \"{}\",
    description: \"{}\",
    build_time: \"{}\",
    git_hash: \"{}\",
    profile: \"{}\",
    target: \"{}\",
}};

/// 版本信息结构
#[derive(Debug, Clone)]
pub struct VersionInfo {{
    pub name: &'static str,
    pub version: &'static str,
    pub authors: &'static str,
    pub description: &'static str,
    pub build_time: &'static str,
    pub git_hash: &'static str,
    pub profile: &'static str,
    pub target: &'static str,
}}

impl VersionInfo {{
    /// 获取完整版本字符串
    pub fn full_version(&self) -> String {{
        format!(\"{{}} {{}} ({{}})\", self.name, self.version, self.git_hash)
    }}
    
    /// 获取构建信息
    pub fn build_info(&self) -> String {{
        format!(\"Built on {{}} for {{}} ({{}})\", self.build_time, self.target, self.profile)
    }}
    
    /// 打印版本信息
    pub fn print(&self) {{
        println!(\"🚀 {{}} v{{}}\", self.name, self.version);
        println!(\"📝 {{}}\", self.description);
        println!(\"👥 Authors: {{}}\", self.authors);
        println!(\"🔨 {{}}\", self.build_info());
        println!(\"📦 Git: {{}}\", self.git_hash);
    }}
}}
",
        name, version, authors, description, build_time, git_hash, profile, target
    );
    
    fs::write(&dest_path, version_info).unwrap();
    
    println!("cargo:rerun-if-changed={}", dest_path.display());
}