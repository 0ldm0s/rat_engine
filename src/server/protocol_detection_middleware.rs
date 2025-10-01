//! 协议检测中间件
//! 
//! 基于 psi_detector 库实现的协议检测中间件，集成到 RAT Engine 框架中。
//! 提供自动协议检测、恶意协议拦截和统计功能，对最终使用者保持透明。

use hyper::{Request, Response, StatusCode};
use hyper::body::Incoming;
use http_body_util::{combinators::BoxBody, BodyExt, Full};
use hyper::body::Bytes;
use psi_detector::{
    builder::DetectorBuilder,
    core::{
        detector::{DefaultProtocolDetector, DetectionConfig, DetectionResult, ProtocolDetector},
        protocol::ProtocolType,
        probe::ProbeStrategy,
    },
    error::{DetectorError, Result as DetectorResult},
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::time::timeout;
use crate::utils::logger::{info, warn, error, debug};

/// 协议检测统计信息
#[derive(Debug, Default, Clone)]
pub struct ProtocolDetectionStats {
    /// 总检测次数
    pub total_detections: u64,
    /// 总检测时间
    pub total_detection_time: Duration,
    /// 各协议类型的检测次数
    pub protocol_counts: HashMap<ProtocolType, u64>,
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
    pub allowed_protocols: Vec<ProtocolType>,
    /// 是否启用详细日志
    pub verbose_logging: bool,
}

impl Default for ProtocolDetectionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_confidence: 0.7,
            timeout_ms: 100,
            block_unknown_protocols: true,
            block_low_confidence: false,
            // 限制为 HTTP 服务器库支持的协议，移除 WebSocket
            allowed_protocols: vec![
                ProtocolType::HTTP1_1,
                ProtocolType::HTTP2,
                ProtocolType::GRPC,  // gRPC 基于 HTTP/2
                ProtocolType::TLS,   // HTTPS 支持
            ],
            verbose_logging: false,
        }
    }
}

/// 协议检测中间件
/// 
/// 提供自动协议检测、恶意协议拦截和统计功能。
/// 集成到 RAT Engine 的请求处理流程中，对最终使用者透明。
#[derive(Debug, Clone)]
pub struct ProtocolDetectionMiddleware {
    /// 协议检测器
    detector: Arc<DefaultProtocolDetector>,
    /// 统计信息
    stats: Arc<Mutex<ProtocolDetectionStats>>,
    /// 配置
    config: ProtocolDetectionConfig,
}

impl ProtocolDetectionMiddleware {
    /// 创建新的协议检测中间件
    /// 
    /// # 参数
    /// * `config` - 协议检测配置
    /// 
    /// # 返回值
    /// 返回配置好的协议检测中间件实例
    /// 
    /// # 错误
    /// 如果检测器初始化失败，返回 DetectorError
    pub fn new(config: ProtocolDetectionConfig) -> DetectorResult<Self> {
        // 创建高性能的协议探测器
        let detector = DetectorBuilder::new()
            .enable_http()
            .enable_http2()
            .enable_tls()
            .with_strategy(ProbeStrategy::Passive)
            .with_min_confidence(config.min_confidence)
            .with_timeout(Duration::from_millis(config.timeout_ms))
            .build()?;

        info!("🔍 协议检测中间件已初始化 - 最小置信度: {:.1}%, 超时: {}ms", 
              config.min_confidence * 100.0, config.timeout_ms);

        Ok(Self {
            detector: Arc::new(detector),
            stats: Arc::new(Mutex::new(ProtocolDetectionStats::default())),
            config,
        })
    }

    /// 使用默认配置创建协议检测中间件
    pub fn with_default_config() -> DetectorResult<Self> {
        Self::new(ProtocolDetectionConfig::default())
    }

    /// 处理请求前的协议检测
    /// 
    /// # 参数
    /// * `req` - HTTP 请求
    /// 
    /// # 返回值
    /// * `Ok(None)` - 协议检测通过，继续处理请求
    /// * `Ok(Some(response))` - 协议被拦截，返回拦截响应
    /// * `Err(error)` - 检测过程中发生错误
    pub async fn process_request(
        &self,
        req: &Request<Incoming>,
    ) -> Result<Option<Response<BoxBody<Bytes, Box<dyn std::error::Error + Send + Sync>>>>, DetectorError> {
        if !self.config.enabled {
            return Ok(None);
        }

        let start_time = Instant::now();
        
        // 提取请求数据进行协议检测
        let detection_data = self.extract_detection_data(req).await?;
        
        // 执行协议检测
        let detection_result = timeout(
            Duration::from_millis(self.config.timeout_ms),
            async {
                self.detector.detect(&detection_data)
            }
        ).await;

        let detection_duration = start_time.elapsed();

        match detection_result {
            Ok(Ok(result)) => {
                if self.config.verbose_logging {
                    debug!("🔍 协议检测: {} (置信度: {:.1}%, 耗时: {}ms)", 
                           result.protocol_type(), 
                           result.confidence() * 100.0, 
                           detection_duration.as_millis());
                }

                // 注意：当前在 TCP 层已经进行了协议拦截，这里只做日志记录
                Ok(None)
            }
            Ok(Err(e)) => {
                warn!("⚠️ 协议检测失败，但允许通过: {}", e);
                Ok(None)
            }
            Err(_) => {
                warn!("⚠️ 协议检测超时，但允许通过");
                Ok(None)
            }
        }
    }

    /// 提取用于协议检测的数据
    async fn extract_detection_data(&self, req: &Request<Incoming>) -> DetectorResult<Vec<u8>> {
        let mut data = Vec::new();
        
        // 添加HTTP方法和版本信息
        data.extend_from_slice(req.method().as_str().as_bytes());
        data.push(b' ');
        data.extend_from_slice(req.uri().path().as_bytes());
        data.push(b' ');
        
        // 添加HTTP版本
        match req.version() {
            hyper::Version::HTTP_09 => data.extend_from_slice(b"HTTP/0.9"),
            hyper::Version::HTTP_10 => data.extend_from_slice(b"HTTP/1.0"),
            hyper::Version::HTTP_11 => data.extend_from_slice(b"HTTP/1.1"),
            hyper::Version::HTTP_2 => data.extend_from_slice(b"HTTP/2.0"),
            hyper::Version::HTTP_3 => data.extend_from_slice(b"HTTP/3.0"),
            _ => data.extend_from_slice(b"HTTP/1.1"),
        }
        data.extend_from_slice(b"\r\n");
        
        // 添加关键头部信息
        for (name, value) in req.headers() {
            data.extend_from_slice(name.as_str().as_bytes());
            data.extend_from_slice(b": ");
            if let Ok(value_str) = value.to_str() {
                data.extend_from_slice(value_str.as_bytes());
            }
            data.extend_from_slice(b"\r\n");
        }
        data.extend_from_slice(b"\r\n");
        
        // 如果是 gRPC 请求，添加特殊标识
        if self.is_grpc_request(req) {
            data.extend_from_slice(b"grpc-encoding: identity\r\n");
            data.extend_from_slice(b"grpc-accept-encoding: identity,deflate,gzip\r\n");
        }
        
        Ok(data)
    }

    /// 检查是否为 gRPC 请求
    fn is_grpc_request(&self, req: &Request<Incoming>) -> bool {
        req.headers().get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(|v| v.starts_with("application/grpc"))
            .unwrap_or(false)
            || req.headers().get("user-agent")
                .and_then(|v| v.to_str().ok())
                .map(|v| v.contains("grpc"))
                .unwrap_or(false)
    }



    // 注意：协议检测中间件当前未被实际使用，因为我们在 TCP 层直接进行协议检测和拦截
    // 如果需要在 HTTP 层进行协议检测，可以重新启用这些方法

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
            "protocol_counts": stats.protocol_counts.iter().map(|(k, v)| (k.to_string(), *v)).collect::<HashMap<String, u64>>(),
            "config": {
                "enabled": self.config.enabled,
                "min_confidence": self.config.min_confidence,
                "timeout_ms": self.config.timeout_ms,
                "block_unknown_protocols": self.config.block_unknown_protocols,
                "block_low_confidence": self.config.block_low_confidence,
                "allowed_protocols": self.config.allowed_protocols.iter().map(|p| p.to_string()).collect::<Vec<String>>()
            }
        })
    }
}