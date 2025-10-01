use std::env;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::process::Command;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("version.rs");
    let mut f = File::create(&dest_path).unwrap();

    // 获取构建时间戳
    let build_timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string();
    
    // 获取 Git 哈希
    let git_hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    
    // 获取构建配置
    let build_profile = env::var("PROFILE").unwrap_or_else(|_| "unknown".to_string());

    // 生成版本常量
    writeln!(
        f,
        "pub const VERSION: &str = \"{}\";",
        env!("CARGO_PKG_VERSION")
    ).unwrap();

    writeln!(
        f,
        "pub const PY_VERSION: &str = \"{}\";",
        env!("CARGO_PKG_VERSION")
    ).unwrap();
    
    writeln!(
        f,
        "pub const BUILD_TIMESTAMP: &str = \"{}\";",
        build_timestamp
    ).unwrap();
    
    writeln!(
        f,
        "pub const GIT_HASH: &str = \"{}\";",
        git_hash
    ).unwrap();
    
    writeln!(
        f,
        "pub const BUILD_PROFILE: &str = \"{}\";",
        build_profile
    ).unwrap();
}