//! 协议检测中间件
//!
//! 注意：由于协议检测已经移至 TCP 层（简化方案），此模块保留用于兼容性。
//! 此中间件不再进行实际的协议检测。

use hyper::{Request, Response};
use hyper::body::Incoming;
use http_body_util::{combinators::BoxBody, BodyExt, Full};
use hyper::body::Bytes;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use crate::utils::logger::{info, warn};

// 重导出 mod.rs 中的 ProtocolType
pub use crate::server::ProtocolType;

/// 协议检测统计信息
#[derive(Debug, Default, Clone)]
pub struct ProtocolDetectionStats {
    /// 总检测次数
    pub total_detections: u64,
    /// 总检测时间
    pub total_detection_time: Duration,
    /// 各协议类型的检测次数
    pub protocol_counts: HashMap<String, u64>,
    /// 高置信度检测次数
    pub high_confidence_detections: u64,
    /// 被拦截的恶意协议次数
    pub blocked_protocols: u64,
    /// 检测错误次数
    pub detection_errors: u64,
}

/// 协议检测配置
#[derive(Debug, Clone)]
pub struct ProtocolDetectionConfig {
    /// 是否启用协议检测
    pub enabled: bool,
    /// 最小置信度阈值
    pub min_confidence: f32,
    /// 检测超时时间（毫秒）
    pub timeout_ms: u64,
    /// 是否拦截未知协议
    pub block_unknown_protocols: bool,
    /// 是否拦截低置信度检测
    pub block_low_confidence: bool,
    /// 允许的协议类型白名单（空表示允许所有已知协议）
    pub allowed_protocols: Vec<String>,
    /// 是否启用详细日志
    pub verbose_logging: bool,
}

impl Default for ProtocolDetectionConfig {
    fn default() -> Self {
        Self {
            enabled: false,  // TCP 层已处理协议检测，此处默认禁用
            min_confidence: 0.7,
            timeout_ms: 100,
            block_unknown_protocols: true,
            block_low_confidence: false,
            allowed_protocols: vec![
                "HTTP1_1".to_string(),
                "HTTP2".to_string(),
                "GRPC".to_string(),
                "TLS".to_string(),
            ],
            verbose_logging: false,
        }
    }
}

/// 协议检测中间件
///
/// 注意：此中间件已废弃，协议检测已移至 TCP 层。
/// 保留此结构用于兼容性。
#[derive(Debug, Clone)]
pub struct ProtocolDetectionMiddleware {
    /// 统计信息
    stats: Arc<Mutex<ProtocolDetectionStats>>,
    /// 配置
    config: ProtocolDetectionConfig,
}

impl ProtocolDetectionMiddleware {
    /// 创建新的协议检测中间件（兼容性方法）
    ///
    /// 注意：此中间件不再进行实际的协议检测。
    pub fn new(config: ProtocolDetectionConfig) -> Result<Self, Box<dyn std::error::Error>> {
        info!("ℹ️  协议检测中间件已初始化（兼容模式，TCP层已处理协议检测）");

        Ok(Self {
            stats: Arc::new(Mutex::new(ProtocolDetectionStats::default())),
            config,
        })
    }

    /// 使用默认配置创建协议检测中间件
    pub fn with_default_config() -> Result<Self, Box<dyn std::error::Error>> {
        Self::new(ProtocolDetectionConfig::default())
    }

    /// 处理请求前的协议检测（空实现，TCP层已处理）
    pub async fn process_request(
        &self,
        _req: &Request<Incoming>,
    ) -> Result<Option<Response<BoxBody<Bytes, Box<dyn std::error::Error + Send + Sync>>>>, Box<dyn std::error::Error>> {
        // TCP 层已处理协议检测，此处直接返回 None（允许通过）
        Ok(None)
    }

    /// 获取统计信息
    pub fn get_stats(&self) -> ProtocolDetectionStats {
        self.stats.lock().unwrap().clone()
    }

    /// 获取配置信息
    pub fn get_config(&self) -> &ProtocolDetectionConfig {
        &self.config
    }

    /// 重置统计信息
    pub fn reset_stats(&self) {
        if let Ok(mut stats) = self.stats.lock() {
            *stats = ProtocolDetectionStats::default();
        }
        info!("🔄 协议检测统计信息已重置");
    }

    /// 获取统计信息的 JSON 表示
    pub fn get_stats_json(&self) -> serde_json::Value {
        let stats = self.get_stats();
        let avg_detection_time = if stats.total_detections > 0 {
            stats.total_detection_time.as_millis() as f64 / stats.total_detections as f64
        } else {
            0.0
        };

        let success_rate = if stats.total_detections > 0 {
            ((stats.total_detections - stats.detection_errors) as f64 / stats.total_detections as f64) * 100.0
        } else {
            0.0
        };

        serde_json::json!({
            "total_detections": stats.total_detections,
            "detection_errors": stats.detection_errors,
            "success_rate_percent": success_rate,
            "blocked_protocols": stats.blocked_protocols,
            "high_confidence_detections": stats.high_confidence_detections,
            "avg_detection_time_ms": avg_detection_time,
            "total_detection_time_ms": stats.total_detection_time.as_millis(),
            "protocol_counts": stats.protocol_counts,
            "config": {
                "enabled": self.config.enabled,
                "min_confidence": self.config.min_confidence,
                "timeout_ms": self.config.timeout_ms,
                "block_unknown_protocols": self.config.block_unknown_protocols,
                "block_low_confidence": self.config.block_low_confidence,
                "allowed_protocols": self.config.allowed_protocols
            }
        })
    }
}
