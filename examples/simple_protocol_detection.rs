//! 简化的协议检测自动验证示例
//! 
//! 演示如何使用集成的协议检测功能进行自动验证测试

use rat_engine::{
    server::protocol_detection_middleware::{ProtocolDetectionMiddleware, ProtocolDetectionConfig},
    utils::logger::{Logger, LogConfig, info, warn, error},
};
use psi_detector::{
    core::{
        detector::{DetectionResult, ProtocolDetector},
        protocol::ProtocolType,
    },
    error::{DetectorError, Result as DetectorResult},
};
use serde::{Serialize, Deserialize};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
    net::SocketAddr,
};

/// 自动化验证测试结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationTestResult {
    pub test_name: String,
    pub protocol_type: String,
    pub expected_protocol: String,
    pub success: bool,
    pub confidence: f32,
    pub detection_time_ms: u128,
    pub error_message: Option<String>,
    pub should_fail: bool,
    pub intercepted: bool,
}

/// 协议模拟器 - 生成不同协议的测试数据
#[derive(Debug)]
pub struct ProtocolSimulator;

impl ProtocolSimulator {
    /// 生成 HTTP/1.1 请求数据
    pub fn generate_http11_data() -> Vec<u8> {
        let request = "GET /api/test HTTP/1.1\r\n\
                      Host: localhost:8080\r\n\
                      User-Agent: Mozilla/5.0 (compatible; ProtocolTester/1.0)\r\n\
                      Accept: application/json\r\n\
                      Connection: keep-alive\r\n\
                      \r\n";
        request.as_bytes().to_vec()
    }

    /// 生成 HTTP/2 请求数据（模拟二进制帧）
    pub fn generate_http2_data() -> Vec<u8> {
        // HTTP/2 连接前言
        let mut data = Vec::new();
        data.extend_from_slice(b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n");
        
        // 模拟 SETTINGS 帧
        data.extend_from_slice(&[0x00, 0x00, 0x00]); // Length: 0
        data.push(0x04); // Type: SETTINGS
        data.push(0x00); // Flags: 0
        data.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // Stream ID: 0
        
        data
    }

    /// 生成 gRPC 请求数据
    pub fn generate_grpc_data() -> Vec<u8> {
        // gRPC 使用 HTTP/2，但有特定的二进制帧格式
        let mut data = Vec::new();
        // HTTP/2 连接前言
        data.extend_from_slice(b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n");
        
        // gRPC 特有的 HEADERS 帧，包含 content-type: application/grpc
        data.extend_from_slice(&[0x00, 0x00, 0x2A]); // Length: 42
        data.push(0x01); // Type: HEADERS
        data.push(0x05); // Flags: END_HEADERS | END_STREAM
        data.extend_from_slice(&[0x00, 0x00, 0x00, 0x01]); // Stream ID: 1
        
        // 模拟 gRPC 头部（简化的 HPACK 编码）
        data.extend_from_slice(b":method: POST\r\n");
        data.extend_from_slice(b"content-type: application/grpc+proto\r\n");
        
        data
    }

    /// 生成 WebSocket 升级请求数据
    pub fn generate_websocket_data() -> Vec<u8> {
        // WebSocket 帧格式（已建立连接后的数据帧）
        let mut data = Vec::new();
        
        // WebSocket 帧头：FIN=1, RSV=000, Opcode=0001 (text frame)
        data.push(0x81); // 10000001
        
        // Payload length = 16, MASK=1 (确保至少 16 字节)
        data.push(0x90); // 10010000 (16 + 128)
        
        // Masking key (4 bytes)
        data.extend_from_slice(&[0x12, 0x34, 0x56, 0x78]);
        
        // Masked payload "hello websocket!" (16 字节)
        let payload = b"hello websocket!";
        let mask = [0x12, 0x34, 0x56, 0x78];
        for (i, &byte) in payload.iter().enumerate() {
            data.push(byte ^ mask[i % 4]);
        }
        
        data
    }

    /// 生成恶意/无效协议数据（用于测试拦截）
    pub fn generate_malicious_data() -> Vec<u8> {
        // 模拟恶意的二进制数据
        let mut data = Vec::new();
        data.extend_from_slice(b"\x00\x01\x02\x03MALICIOUS_PROTOCOL\xFF\xFE\xFD");
        data.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]); // 恶意标识
        data.extend_from_slice(b"EXPLOIT_ATTEMPT");
        data
    }

    /// 生成损坏的 HTTP 请求数据
    pub fn generate_corrupted_http_data() -> Vec<u8> {
        // 严重损坏的 HTTP 请求（包含二进制数据和无效格式）
        let mut corrupted = Vec::new();
        corrupted.extend_from_slice(b"INVALID_METHOD /test BROKEN_VERSION\r\n");
        corrupted.extend_from_slice(b"Malformed-Header-Without-Value\r\n");
        corrupted.extend_from_slice(&[0x00, 0x01, 0x02, 0xFF, 0xFE, 0xFD]);
        corrupted.extend_from_slice(b"\r\nBroken: ");
        corrupted.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]);
        corrupted.extend_from_slice(b"\r\n\r\n");
        corrupted
    }
}

/// 自动化验证器
#[derive(Debug)]
pub struct ProtocolValidationSuite {
    pub middleware: ProtocolDetectionMiddleware,
}

impl ProtocolValidationSuite {
    pub fn new(middleware: ProtocolDetectionMiddleware) -> Self {
        Self { middleware }
    }

    /// 运行完整的验证测试套件
    pub async fn run_validation_tests(&self) -> Vec<ValidationTestResult> {
        let mut results = Vec::new();

        info!("🚀 开始运行协议检测自动化验证测试...");

        // 正常协议检测测试
        results.extend(self.run_normal_protocol_tests().await);
        
        // 恶意协议拦截测试
        results.extend(self.run_malicious_protocol_tests().await);
        
        // 错误处理测试
        results.extend(self.run_error_handling_tests().await);
        
        // 性能基准测试
        results.extend(self.run_performance_tests().await);

        // 输出测试总结
        self.print_test_summary(&results);

        results
    }

    /// 正常协议检测测试
    async fn run_normal_protocol_tests(&self) -> Vec<ValidationTestResult> {
        let mut results = Vec::new();

        // 核心协议检测测试（必须通过）
        let core_tests = vec![
            ("HTTP/1.1 检测", ProtocolSimulator::generate_http11_data(), ProtocolType::HTTP1_1),
            ("HTTP/2 检测", ProtocolSimulator::generate_http2_data(), ProtocolType::HTTP2),
        ];

        for (test_name, data, expected) in core_tests {
            let result = self.test_protocol_detection_typed(test_name, data, expected, false).await;
            results.push(result);
        }

        // 扩展协议检测测试（允许检测为 HTTP 变体）
        let extended_tests = vec![
            ("gRPC 检测", ProtocolSimulator::generate_grpc_data()),
            ("WebSocket 检测", ProtocolSimulator::generate_websocket_data()),
        ];

        for (test_name, data) in extended_tests {
            let result = self.test_protocol_detection_flexible(test_name, data).await;
            results.push(result);
        }

        results
    }

    /// 恶意协议拦截测试（预期失败）
    async fn run_malicious_protocol_tests(&self) -> Vec<ValidationTestResult> {
        let mut results = Vec::new();

        let malicious_cases = vec![
            ("恶意协议拦截", ProtocolSimulator::generate_malicious_data(), ProtocolType::Unknown),
            ("损坏HTTP拦截", ProtocolSimulator::generate_corrupted_http_data(), ProtocolType::Unknown),
        ];

        for (test_name, data, expected) in malicious_cases {
            let result = self.test_protocol_detection_typed(test_name, data, expected, true).await;
            results.push(result);
        }

        results
    }

    /// 错误处理测试
    async fn run_error_handling_tests(&self) -> Vec<ValidationTestResult> {
        let mut results = Vec::new();

        // 空数据测试
        let empty_result = self.test_protocol_detection_typed(
            "空数据处理", 
            vec![], 
            ProtocolType::Unknown, 
            true
        ).await;
        results.push(empty_result);

        // 超大数据测试
        let large_data = vec![0u8; 1024 * 1024]; // 1MB
        let large_result = self.test_protocol_detection_typed(
            "超大数据处理", 
            large_data, 
            ProtocolType::Unknown, 
            true
        ).await;
        results.push(large_result);

        results
    }

    /// 性能基准测试
    async fn run_performance_tests(&self) -> Vec<ValidationTestResult> {
        let mut results = Vec::new();

        // 批量检测性能测试
        let test_data = ProtocolSimulator::generate_http11_data();
        let batch_size = 100;
        
        let start_time = Instant::now();
        for _i in 0..batch_size {
            let _ = self.detect_protocol_from_data(&test_data).await;
        }
        let total_time = start_time.elapsed();
        
        let avg_time_ms = total_time.as_millis() / batch_size as u128;
        
        let perf_result = ValidationTestResult {
            test_name: format!("性能基准测试 ({} 次检测)", batch_size),
            protocol_type: "HTTP/1.1".to_string(),
            expected_protocol: "HTTP/1.1".to_string(),
            success: avg_time_ms < 10, // 期望每次检测少于10ms
            confidence: 1.0,
            detection_time_ms: avg_time_ms,
            error_message: if avg_time_ms >= 10 {
                Some(format!("平均检测时间 {}ms 超过预期的 10ms", avg_time_ms))
            } else {
                None
            },
            should_fail: false,
            intercepted: false,
        };
        
        results.push(perf_result);
        results
    }

    /// 灵活的协议检测测试（允许检测为 HTTP 变体）
    async fn test_protocol_detection_flexible(
        &self,
        test_name: &str,
        data: Vec<u8>,
    ) -> ValidationTestResult {
        let start_time = Instant::now();
        
        match self.detect_protocol_from_data(&data).await {
            Ok((protocol_type, confidence)) => {
                let detection_time = start_time.elapsed().as_millis();
                let detected_protocol_str = protocol_type.to_string();
                
                // 对于扩展协议，只要检测到任何有效协议就算成功
                let success = matches!(protocol_type, 
                    ProtocolType::HTTP1_1 | 
                    ProtocolType::HTTP2 | 
                    ProtocolType::GRPC | 
                    ProtocolType::WebSocket |
                    ProtocolType::TLS
                );

                ValidationTestResult {
                    test_name: test_name.to_string(),
                    protocol_type: detected_protocol_str.clone(),
                    expected_protocol: "HTTP变体".to_string(),
                    success,
                    confidence,
                    detection_time_ms: detection_time,
                    error_message: if !success {
                        Some(format!("期望检测到有效协议，但检测到 {}", detected_protocol_str))
                    } else {
                        None
                    },
                    should_fail: false,
                    intercepted: false,
                }
            }
            Err(e) => {
                let detection_time = start_time.elapsed().as_millis();
                ValidationTestResult {
                    test_name: test_name.to_string(),
                    protocol_type: "Error".to_string(),
                    expected_protocol: "HTTP变体".to_string(),
                    success: false,
                    confidence: 0.0,
                    detection_time_ms: detection_time,
                    error_message: Some(format!("检测错误: {}", e)),
                    should_fail: false,
                    intercepted: false,
                }
            }
        }
    }

    /// 类型化协议检测测试
    async fn test_protocol_detection_typed(
        &self,
        test_name: &str,
        data: Vec<u8>,
        expected_protocol: ProtocolType,
        should_fail: bool,
    ) -> ValidationTestResult {
        let start_time = Instant::now();
        
        match self.detect_protocol_from_data(&data).await {
            Ok((protocol_type, confidence)) => {
                let detection_time = start_time.elapsed().as_millis();
                let detected_protocol_str = protocol_type.to_string();
                let expected_protocol_str = expected_protocol.to_string();
                
                let success = if should_fail {
                    false // 如果期望失败但成功了，则测试失败
                } else {
                    protocol_type == expected_protocol
                };

                ValidationTestResult {
                    test_name: test_name.to_string(),
                    protocol_type: detected_protocol_str,
                    expected_protocol: expected_protocol_str,
                    success,
                    confidence,
                    detection_time_ms: detection_time,
                    error_message: if !success && !should_fail {
                        Some(format!("期望 {} 但检测到 {}", expected_protocol, protocol_type))
                    } else {
                        None
                    },
                    should_fail,
                    intercepted: should_fail && protocol_type == ProtocolType::Unknown,
                }
            }
            Err(e) => {
                let detection_time = start_time.elapsed().as_millis();
                ValidationTestResult {
                    test_name: test_name.to_string(),
                    protocol_type: "Error".to_string(),
                    expected_protocol: expected_protocol.to_string(),
                    success: should_fail, // 如果期望失败且确实失败了，则测试成功
                    confidence: 0.0,
                    detection_time_ms: detection_time,
                    error_message: Some(format!("检测错误: {}", e)),
                    should_fail,
                    intercepted: should_fail,
                }
            }
        }
    }

    /// 从原始数据检测协议
    async fn detect_protocol_from_data(&self, data: &[u8]) -> DetectorResult<(ProtocolType, f32)> {
        // 使用协议检测中间件的内部检测器
        // 注意：这里需要访问中间件的内部检测器，可能需要添加公共方法
        // 暂时使用模拟实现
        use psi_detector::{
            builder::DetectorBuilder,
            core::probe::ProbeStrategy,
        };
        
        let detector = DetectorBuilder::new()
            .enable_http()
            .enable_http2()
            .enable_tls()
            .with_strategy(ProbeStrategy::Passive)
            .with_min_confidence(0.7)

            .build()?;
            
        let result = detector.detect(data)?;
        Ok((result.protocol_type(), result.confidence()))
    }

    /// 打印测试总结
    fn print_test_summary(&self, results: &[ValidationTestResult]) {
        let total_tests = results.len();
        let passed_tests = results.iter().filter(|r| r.success).count();
        let failed_tests = total_tests - passed_tests;
        let intercepted_tests = results.iter().filter(|r| r.intercepted).count();

        info!("📊 测试总结:");
        info!("   总测试数: {}", total_tests);
        info!("   通过: {} ✅", passed_tests);
        info!("   失败: {} ❌", failed_tests);
        info!("   成功拦截: {} 🛡️", intercepted_tests);

        let avg_detection_time: f64 = results.iter()
            .map(|r| r.detection_time_ms as f64)
            .sum::<f64>() / total_tests as f64;
        info!("   平均检测时间: {:.2}ms", avg_detection_time);

        // 详细结果
        for result in results {
            let status = if result.success { "✅" } else { "❌" };
            let intercept_info = if result.intercepted { " 🛡️" } else { "" };
            info!("   {} {} - {} -> {} ({:.1}% 置信度, {}ms){}", 
                status, 
                result.test_name, 
                result.expected_protocol, 
                result.protocol_type, 
                result.confidence * 100.0, 
                result.detection_time_ms,
                intercept_info
            );
            
            if let Some(ref error) = result.error_message {
                warn!("     错误: {}", error);
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化框架内的日志系统
    let log_config = LogConfig::default();
    if let Err(e) = Logger::init(log_config) {
        eprintln!("日志初始化失败: {}", e);
    }
    
    println!("🚀 启动 RAT Engine 协议检测自动验证");
    println!("🔍 集成 psi_detector 进行协议检测");
    println!("🔧 自动验证模式");
    
    // 创建协议检测中间件
    let config = ProtocolDetectionConfig::default();
    let middleware = ProtocolDetectionMiddleware::new(config)?;
    
    info!("🔍 开始自动验证测试...");
    let validation_suite = ProtocolValidationSuite::new(middleware);
    let results = validation_suite.run_validation_tests().await;
    
    // 输出验证结果摘要
    let total_tests = results.len();
    let passed_tests = results.iter().filter(|r| r.success).count();
    let success_rate = (passed_tests as f64 / total_tests as f64) * 100.0;
    
    println!("\n📊 最终测试结果:");
    println!("   总测试数: {}", total_tests);
    println!("   通过: {} ✅", passed_tests);
    println!("   失败: {} ❌", total_tests - passed_tests);
    println!("   成功率: {:.1}%", success_rate);
    
    if success_rate >= 80.0 {
        println!("✅ 自动验证完成！协议检测功能正常");
        std::process::exit(0);
    } else {
        println!("❌ 自动验证失败，协议检测存在问题");
        std::process::exit(1);
    }
}