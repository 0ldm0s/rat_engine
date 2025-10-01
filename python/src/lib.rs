//! RAT Engine Python 绑定
//! 
//! 高性能 HTTP 服务器引擎的 Python 接口
//! 使用 rat_quick_threshold 进行数据编码解码和传递

use pyo3::prelude::*;

// 包含构建时生成的版本信息
include!(concat!(env!("OUT_DIR"), "/version.rs"));

/// RAT Engine Python 模块
#[pymodule]
fn _rat_engine(py: Python, m: &PyModule) -> PyResult<()> {
    // 极端修改：暂时注释掉日志系统初始化，让用户手动控制
    // rat_engine::utils::logger::init_python_logger();
    
    // 设置模块信息
    m.add("__version__", VERSION)?;
    m.add("__author__", "RAT Team")?;
    m.add("__description__", "高性能 Web 框架 Python 绑定")?;
    
    // 添加构建信息
    m.add("__build_timestamp__", BUILD_TIMESTAMP)?;
    m.add("__git_hash__", GIT_HASH)?;
    m.add("__build_profile__", BUILD_PROFILE)?;
    
    // 注册 Python API 模块
    rat_engine::python_api::register_python_api_module(py, m)?;
    
    Ok(())
}