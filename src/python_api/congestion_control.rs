//! 拥塞控制 Python API
//! 
//! 基于 rat_congestion 库的简洁 Python 接口，支持：
//! - 预设配置（高性能、稳定网络、自适应）
//! - 自定义配置
//! - 算法切换和统计信息获取

use pyo3::prelude::*;
use std::collections::HashMap;
use std::time::Duration;

// 导入 rat_congestion 库
use rat_congestion::{CongestionController, ControllerConfig};

// 简单的网络指标实现，用于 Python API
#[derive(Debug)]
struct PyNetworkMetrics {
    packets_sent: u64,
    packets_acked: u64,
    packets_lost: u64,
    bytes_sent: u64,
    bytes_acked: u64,
    current_rtt: Duration,
    min_rtt: Duration,
    bandwidth_estimate: u64,
    loss_rate: f64,
    bw_ratio: f64,
    rtt_cv: f64,
}

impl PyNetworkMetrics {
    fn new() -> Self {
        Self {
            packets_sent: 0,
            packets_acked: 0,
            packets_lost: 0,
            bytes_sent: 0,
            bytes_acked: 0,
            current_rtt: Duration::from_millis(50),
            min_rtt: Duration::from_millis(50),
            bandwidth_estimate: 1_000_000,
            loss_rate: 0.0,
            bw_ratio: 1.0,
            rtt_cv: 0.1,
        }
    }
}

impl rat_congestion::NetworkMetrics for PyNetworkMetrics {
    fn on_packet_sent(&mut self, bytes: u32) {
        self.packets_sent += 1;
        self.bytes_sent += bytes as u64;
    }

    fn on_packet_acked(&mut self, bytes: u32, rtt: Duration) {
        self.packets_acked += 1;
        self.bytes_acked += bytes as u64;
        
        // 简单的 RTT 平滑
        let alpha = 0.125;
        let rtt_ms = rtt.as_millis() as f64;
        let current_ms = self.current_rtt.as_millis() as f64;
        let new_rtt_ms = (1.0 - alpha) * current_ms + alpha * rtt_ms;
        self.current_rtt = Duration::from_millis(new_rtt_ms as u64);
        
        if rtt < self.min_rtt {
            self.min_rtt = rtt;
        }
        
        // 简单的带宽估算
        let throughput = (bytes as u64 * 8 * 1000) / rtt.as_millis() as u64;
        self.bandwidth_estimate = (self.bandwidth_estimate * 7 + throughput) / 8;
    }

    fn on_packet_lost(&mut self, _bytes: u32) {
        self.packets_lost += 1;
        if self.packets_sent > 0 {
            self.loss_rate = self.packets_lost as f64 / self.packets_sent as f64;
        }
    }

    fn rtt(&self) -> Duration { self.current_rtt }
    fn min_rtt(&self) -> Duration { self.min_rtt }
    fn bandwidth(&self) -> u64 { self.bandwidth_estimate }
    fn loss_rate(&self) -> f64 { self.loss_rate }
    fn bw_ratio(&self) -> f64 { self.bw_ratio }
    fn set_bw_ratio(&mut self, ratio: f64) { self.bw_ratio = ratio; }
    fn rtt_cv(&self) -> f64 { self.rtt_cv }
    fn set_rtt_cv(&mut self, cv: f64) { self.rtt_cv = cv; }
    
    fn has_variable_rtt(&self) -> bool { self.rtt_cv > 0.2 }
    fn has_low_bandwidth_utilization(&self) -> bool { self.bw_ratio < 0.8 }
    fn has_high_loss(&self) -> bool { self.loss_rate > 0.01 }
    
    fn update(&mut self, rtt: Duration, loss_rate: f64, throughput: u64, _cwnd: u32) {
        self.current_rtt = rtt;
        self.loss_rate = loss_rate;
        self.bandwidth_estimate = throughput;
    }
    
    fn reset(&mut self) {
        *self = Self::new();
    }
}

// 简单的指标窗口实现
#[derive(Debug)]
struct PyMetricsWindow {
    samples: std::collections::VecDeque<(Duration, f64, u64)>,
    max_size: usize,
}

impl PyMetricsWindow {
    fn new(max_size: usize) -> Self {
        Self {
            samples: std::collections::VecDeque::with_capacity(max_size),
            max_size,
        }
    }
}

impl rat_congestion::MetricsWindow for PyMetricsWindow {
    fn add_sample(&mut self, rtt: Duration, loss_rate: f64, bandwidth: u64) {
        if self.samples.len() >= self.max_size {
            self.samples.pop_front();
        }
        self.samples.push_back((rtt, loss_rate, bandwidth));
    }

    fn avg_rtt(&self) -> Duration {
        if self.samples.is_empty() {
            return Duration::from_millis(50);
        }
        let sum: u128 = self.samples.iter().map(|(rtt, _, _)| rtt.as_millis()).sum();
        Duration::from_millis((sum / self.samples.len() as u128) as u64)
    }

    fn avg_bandwidth(&self) -> u64 {
        if self.samples.is_empty() {
            return 1_000_000;
        }
        self.samples.iter().map(|(_, _, bw)| *bw).sum::<u64>() / self.samples.len() as u64
    }

    fn avg_loss_rate(&self) -> f64 {
        if self.samples.is_empty() {
            return 0.0;
        }
        self.samples.iter().map(|(_, loss, _)| *loss).sum::<f64>() / self.samples.len() as f64
    }

    fn rtt_coefficient_of_variation(&self) -> Option<f64> {
        if self.samples.len() < 2 {
            return None;
        }
        
        let rtts: Vec<f64> = self.samples.iter()
            .map(|(rtt, _, _)| rtt.as_millis() as f64)
            .collect();
        
        let mean = rtts.iter().sum::<f64>() / rtts.len() as f64;
        let variance = rtts.iter()
            .map(|x| (x - mean).powi(2))
            .sum::<f64>() / rtts.len() as f64;
        
        let std_dev = variance.sqrt();
        Some(std_dev / mean)
    }

    fn is_stable(&self) -> bool {
        if let Some(cv) = self.rtt_coefficient_of_variation() {
            cv < 0.2 && self.avg_loss_rate() < 0.005
        } else {
            true
        }
    }

    fn clear(&mut self) {
        self.samples.clear();
    }
}

/// Python 拥塞控制管理器
#[pyclass(name = "CongestionController")]
pub struct PyCongestionController {
    controller: CongestionController<PyNetworkMetrics, PyMetricsWindow>,
}

#[pymethods]
impl PyCongestionController {
    /// 创建高性能配置的控制器
    #[staticmethod]
    fn high_performance() -> Self {
        let controller = CongestionController::high_performance(
            PyNetworkMetrics::new(),
            PyMetricsWindow::new(128)
        );
        Self { controller }
    }
    
    /// 创建稳定网络配置的控制器
    #[staticmethod]
    fn stable_network() -> Self {
        let controller = CongestionController::stable_network(
            PyNetworkMetrics::new(),
            PyMetricsWindow::new(32)
        );
        Self { controller }
    }
    
    /// 创建自适应配置的控制器
    #[staticmethod]
    fn adaptive() -> Self {
        let controller = CongestionController::adaptive(
            PyNetworkMetrics::new(),
            PyMetricsWindow::new(48)
        );
        Self { controller }
    }
    
    /// 创建自定义配置的控制器
    #[staticmethod]
    #[pyo3(signature = (algorithm = "auto".to_string(), auto_switching = true, window_size = 32))]
    fn custom(algorithm: String, auto_switching: bool, window_size: usize) -> Self {
        let config = ControllerConfig::builder()
            .initial_algorithm(&algorithm)
            .auto_switching(auto_switching)
            .platform_optimized(true)
            .metrics_window_size(window_size)
            .build();
        
        let controller = CongestionController::with_config(
            config,
            PyNetworkMetrics::new(),
            PyMetricsWindow::new(window_size)
        );
        Self { controller }
    }
    
    /// 获取当前算法
    fn current_algorithm(&self) -> String {
        self.controller.current_algorithm().to_string()
    }
    
    /// 切换算法
    fn switch_algorithm(&mut self, algorithm: &str) -> PyResult<()> {
        self.controller.switch_algorithm(algorithm);
        Ok(())
    }
    
    /// 获取当前窗口大小（字节）
    fn window_size(&self) -> u64 {
        self.controller.window_size()
    }
    
    /// 获取当前发送速率（bps）
    fn pacing_rate(&self) -> u64 {
        self.controller.pacing_rate()
    }
    
    /// 获取慢启动阈值
    fn slow_start_threshold(&self) -> u64 {
        self.controller.slow_start_threshold()
    }
    
    /// 检查是否可以发送指定字节数
    fn can_send(&self, bytes: u64) -> bool {
        self.controller.can_send(bytes)
    }
    
    /// 处理数据包确认事件
    fn on_packet_acked(&mut self, bytes: u64, rtt_ms: u64) {
        let rtt = Duration::from_millis(rtt_ms);
        self.controller.on_packet_acked(bytes, rtt);
    }
    
    /// 处理数据包丢失事件
    fn on_packet_lost(&mut self, bytes: u64) {
        self.controller.on_packet_lost(bytes);
    }
    
    /// 批量处理确认事件
    fn process_ack_batch(&mut self, acks: Vec<(u64, u64)>) {
        let ack_batch: Vec<(u64, Duration)> = acks.into_iter()
            .map(|(bytes, rtt_ms)| (bytes, Duration::from_millis(rtt_ms)))
            .collect();
        self.controller.process_ack_batch(&ack_batch);
    }
    
    /// 获取网络统计信息
    fn get_stats(&self) -> HashMap<String, f64> {
        let metrics = self.controller.network_metrics();
        let mut stats = HashMap::new();
        
        stats.insert("packets_sent".to_string(), metrics.packets_sent as f64);
        stats.insert("packets_acked".to_string(), metrics.packets_acked as f64);
        stats.insert("packets_lost".to_string(), metrics.packets_lost as f64);
        stats.insert("bytes_sent".to_string(), metrics.bytes_sent as f64);
        stats.insert("bytes_acked".to_string(), metrics.bytes_acked as f64);
        stats.insert("current_rtt_ms".to_string(), metrics.current_rtt.as_millis() as f64);
        stats.insert("min_rtt_ms".to_string(), metrics.min_rtt.as_millis() as f64);
        stats.insert("bandwidth_bps".to_string(), metrics.bandwidth_estimate as f64);
        stats.insert("loss_rate".to_string(), metrics.loss_rate);
        stats.insert("bw_ratio".to_string(), metrics.bw_ratio);
        stats.insert("rtt_cv".to_string(), metrics.rtt_cv);
        
        stats
    }
    
    /// 获取平台信息
    fn platform(&self) -> String {
        self.controller.platform().to_string()
    }
    
    /// 重置控制器状态
    fn reset(&mut self) {
        self.controller.reset();
    }
}

/// 获取可用的拥塞控制算法列表
#[pyfunction]
fn get_available_algorithms() -> Vec<String> {
    vec![
        "auto".to_string(),
        "bbr".to_string(),
        "cubic".to_string(),
    ]
}

/// 获取平台信息
#[pyfunction]
fn get_platform_info() -> HashMap<String, String> {
    let mut info = HashMap::new();
    info.insert("os".to_string(), std::env::consts::OS.to_string());
    info.insert("arch".to_string(), std::env::consts::ARCH.to_string());
    info.insert("family".to_string(), std::env::consts::FAMILY.to_string());
    info.insert("platform".to_string(), rat_congestion::PLATFORM.to_string());
    info
}

/// 注册拥塞控制相关函数到 Python 模块
pub fn register_congestion_control_functions(module: &PyModule) -> PyResult<()> {
    module.add_class::<PyCongestionController>()?;
    module.add_function(wrap_pyfunction!(get_available_algorithms, module)?)?;
    module.add_function(wrap_pyfunction!(get_platform_info, module)?)?;
    Ok(())
}