//! 拥塞控制集成模块
//! 
//! 集成 rat_congestion 库，提供智能拥塞控制功能：
//! - 基于现有指标的网络状态监控
//! - BBR 和 CUBIC 算法自动切换
//! - 平台优化和性能调优

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use std::collections::VecDeque;

use rat_congestion::{
    CongestionController, ControllerConfig, NetworkMetrics, MetricsWindow,
    CongestionEvent, WindowSize, PacingRate
};
/// 拥塞控制配置
#[derive(Debug, Clone)]
pub struct CongestionControlConfig {
    pub enabled: bool,
    pub algorithm: String,
    pub auto_switching: bool,
    pub platform_optimized: bool,
    pub metrics_window_size: usize,
    pub switch_cooldown_ms: u64,
}
use crate::engine::metrics::AtomicMetrics;
use crate::utils::logger::{info, warn, debug, error};

/// RAT Engine 网络指标适配器
/// 
/// 将 RAT Engine 的原子指标适配为 rat_congestion 库所需的 NetworkMetrics 接口
pub struct RatEngineNetworkMetrics {
    /// 引用 RAT Engine 的原子指标
    engine_metrics: Arc<AtomicMetrics>,
    
    /// 数据包发送计数
    packets_sent: AtomicU64,
    /// 数据包确认计数
    packets_acked: AtomicU64,
    /// 数据包丢失计数
    packets_lost: AtomicU64,
    /// 发送字节数
    bytes_sent: AtomicU64,
    /// 确认字节数
    bytes_acked: AtomicU64,
    /// 丢失字节数
    bytes_lost: AtomicU64,
    
    /// 当前 RTT 估计（微秒）
    current_rtt_us: AtomicU64,
    /// 最小 RTT（微秒）
    min_rtt_us: AtomicU64,
    /// 带宽估计（bps）
    bandwidth_estimate: AtomicU64,
    /// 丢包率
    loss_rate: AtomicU64, // 存储为千分比（0-1000）
    /// 带宽比率
    bandwidth_ratio: AtomicU64, // 存储为千分比（0-1000）
    /// RTT 变异系数
    rtt_cv: AtomicU64, // 存储为千分比（0-1000）
    
    /// 创建时间
    created_at: Instant,
}

impl RatEngineNetworkMetrics {
    /// 创建新的网络指标适配器
    pub fn new(engine_metrics: Arc<AtomicMetrics>) -> Self {
        Self {
            engine_metrics,
            packets_sent: AtomicU64::new(0),
            packets_acked: AtomicU64::new(0),
            packets_lost: AtomicU64::new(0),
            bytes_sent: AtomicU64::new(0),
            bytes_acked: AtomicU64::new(0),
            bytes_lost: AtomicU64::new(0),
            current_rtt_us: AtomicU64::new(50_000), // 默认 50ms
            min_rtt_us: AtomicU64::new(50_000),
            bandwidth_estimate: AtomicU64::new(1_000_000), // 默认 1Mbps
            loss_rate: AtomicU64::new(0),
            bandwidth_ratio: AtomicU64::new(1000), // 默认 1.0
            rtt_cv: AtomicU64::new(100), // 默认 0.1
            created_at: Instant::now(),
        }
    }
    
    /// 从引擎指标更新网络指标
    pub fn update_from_engine_metrics(&self) {
        let metrics = self.engine_metrics.get_all();
        
        // 更新 RTT（从引擎的延迟统计中获取）
        if let Some(avg_latency) = metrics.get("avg_latency_us") {
            let latency_us = (*avg_latency as f64) as u64;
            self.current_rtt_us.store(latency_us, Ordering::Relaxed);
            
            // 更新最小 RTT
            let current_min = self.min_rtt_us.load(Ordering::Relaxed);
            if latency_us < current_min || current_min == 0 {
                self.min_rtt_us.store(latency_us, Ordering::Relaxed);
            }
        }
        
        // 更新带宽估计（基于吞吐量）
        if let Some(rps) = metrics.get("requests_per_second") {
            // 假设平均请求大小为 1KB，计算带宽
            let bandwidth = (*rps as f64 * 1024.0 * 8.0) as u64; // 转换为 bps
            self.bandwidth_estimate.store(bandwidth, Ordering::Relaxed);
        }
        
        // 更新丢包率（基于错误率）
        if let (Some(total), Some(errors)) = (metrics.get("request_count"), metrics.get("error_count")) {
            if *total > 0 {
                let loss_rate = (*errors as f64 / *total as f64 * 1000.0) as u64; // 千分比
                self.loss_rate.store(loss_rate.min(1000), Ordering::Relaxed);
            }
        }
        
        debug!("拥塞控制指标已更新: RTT={}μs, 带宽={}bps, 丢包率={}‰", 
               self.current_rtt_us.load(Ordering::Relaxed),
               self.bandwidth_estimate.load(Ordering::Relaxed),
               self.loss_rate.load(Ordering::Relaxed));
    }
}

impl NetworkMetrics for RatEngineNetworkMetrics {
    fn on_packet_sent(&mut self, bytes: u32) {
        self.packets_sent.fetch_add(1, Ordering::Relaxed);
        self.bytes_sent.fetch_add(bytes as u64, Ordering::Relaxed);
    }
    
    fn on_packet_acked(&mut self, bytes: u32, rtt: Duration) {
        self.packets_acked.fetch_add(1, Ordering::Relaxed);
        self.bytes_acked.fetch_add(bytes as u64, Ordering::Relaxed);
        
        // 更新 RTT
        let rtt_us = rtt.as_micros() as u64;
        self.current_rtt_us.store(rtt_us, Ordering::Relaxed);
        
        // 更新最小 RTT
        let current_min = self.min_rtt_us.load(Ordering::Relaxed);
        if rtt_us < current_min || current_min == 0 {
            self.min_rtt_us.store(rtt_us, Ordering::Relaxed);
        }
    }
    
    fn on_packet_lost(&mut self, bytes: u32) {
        self.packets_lost.fetch_add(1, Ordering::Relaxed);
        self.bytes_lost.fetch_add(bytes as u64, Ordering::Relaxed);
        
        // 重新计算丢包率
        let total_packets = self.packets_sent.load(Ordering::Relaxed);
        let lost_packets = self.packets_lost.load(Ordering::Relaxed);
        if total_packets > 0 {
            let loss_rate = (lost_packets as f64 / total_packets as f64 * 1000.0) as u64;
            self.loss_rate.store(loss_rate.min(1000), Ordering::Relaxed);
        }
    }
    
    fn rtt(&self) -> Duration {
        Duration::from_micros(self.current_rtt_us.load(Ordering::Relaxed))
    }
    
    fn min_rtt(&self) -> Duration {
        Duration::from_micros(self.min_rtt_us.load(Ordering::Relaxed))
    }
    
    fn bandwidth(&self) -> u64 {
        self.bandwidth_estimate.load(Ordering::Relaxed)
    }
    
    fn loss_rate(&self) -> f64 {
        self.loss_rate.load(Ordering::Relaxed) as f64 / 1000.0
    }
    
    fn bw_ratio(&self) -> f64 {
        self.bandwidth_ratio.load(Ordering::Relaxed) as f64 / 1000.0
    }
    
    fn rtt_cv(&self) -> f64 {
        self.rtt_cv.load(Ordering::Relaxed) as f64 / 1000.0
    }
    
    fn set_bw_ratio(&mut self, ratio: f64) {
        let ratio_scaled = (ratio * 1000.0) as u64;
        self.bandwidth_ratio.store(ratio_scaled, Ordering::Relaxed);
    }
    
    fn set_rtt_cv(&mut self, cv: f64) {
        let cv_scaled = (cv * 1000.0) as u64;
        self.rtt_cv.store(cv_scaled, Ordering::Relaxed);
    }
    
    fn has_variable_rtt(&self) -> bool {
        self.rtt_cv() > 0.2 // RTT 变异系数大于 0.2 认为是可变的
    }
    
    fn has_low_bandwidth_utilization(&self) -> bool {
        self.bw_ratio() < 0.8 // 带宽利用率低于 80%
    }
    
    fn has_high_loss(&self) -> bool {
        self.loss_rate() > 0.01 // 丢包率大于 1%
    }
    
    fn update(&mut self, rtt: Duration, loss_rate: f64, throughput: u64, _cwnd: u32) {
        let rtt_us = rtt.as_micros() as u64;
        self.current_rtt_us.store(rtt_us, Ordering::Relaxed);
        
        // 更新最小 RTT
        let current_min = self.min_rtt_us.load(Ordering::Relaxed);
        if rtt_us < current_min || current_min == 0 {
            self.min_rtt_us.store(rtt_us, Ordering::Relaxed);
        }
        
        // 更新丢包率
        let loss_rate_scaled = (loss_rate * 1000.0) as u64;
        self.loss_rate.store(loss_rate_scaled.min(1000), Ordering::Relaxed);
        
        // 更新带宽估计
        self.bandwidth_estimate.store(throughput, Ordering::Relaxed);
    }
    
    fn reset(&mut self) {
        self.packets_sent.store(0, Ordering::Relaxed);
        self.packets_acked.store(0, Ordering::Relaxed);
        self.packets_lost.store(0, Ordering::Relaxed);
        self.bytes_sent.store(0, Ordering::Relaxed);
        self.bytes_acked.store(0, Ordering::Relaxed);
        self.bytes_lost.store(0, Ordering::Relaxed);
        self.current_rtt_us.store(50000, Ordering::Relaxed); // 默认 50ms
        self.min_rtt_us.store(0, Ordering::Relaxed);
        self.bandwidth_estimate.store(1_000_000, Ordering::Relaxed); // 默认 1Mbps
        self.loss_rate.store(0, Ordering::Relaxed);
        self.bandwidth_ratio.store(1000, Ordering::Relaxed); // 默认 100%
        self.rtt_cv.store(0, Ordering::Relaxed);
    }
}

/// RAT Engine 指标窗口
/// 
/// 实现滑动窗口来跟踪网络指标的历史数据
pub struct RatEngineMetricsWindow {
    /// 指标样本：(RTT, 丢包率, 带宽)
    samples: RwLock<VecDeque<(Duration, f64, u64)>>,
    /// 窗口大小
    window_size: usize,
    /// 创建时间
    created_at: Instant,
}

impl RatEngineMetricsWindow {
    /// 创建新的指标窗口
    pub fn new(window_size: usize) -> Self {
        Self {
            samples: RwLock::new(VecDeque::with_capacity(window_size)),
            window_size,
            created_at: Instant::now(),
        }
    }
}

impl MetricsWindow for RatEngineMetricsWindow {
    fn add_sample(&mut self, rtt: Duration, loss_rate: f64, bandwidth: u64) {
        let mut samples = self.samples.write().unwrap();
        
        // 添加新样本
        samples.push_back((rtt, loss_rate, bandwidth));
        
        // 保持窗口大小
        while samples.len() > self.window_size {
            samples.pop_front();
        }
        
        debug!("添加指标样本: RTT={:?}, 丢包率={:.4}, 带宽={}bps, 窗口大小={}", 
               rtt, loss_rate, bandwidth, samples.len());
    }
    
    fn avg_rtt(&self) -> Duration {
        let samples = self.samples.read().unwrap();
        if samples.is_empty() {
            return Duration::from_millis(50);
        }
        
        let sum: u128 = samples.iter().map(|(rtt, _, _)| rtt.as_micros()).sum();
        Duration::from_micros((sum / samples.len() as u128) as u64)
    }
    
    fn avg_loss_rate(&self) -> f64 {
        let samples = self.samples.read().unwrap();
        if samples.is_empty() {
            return 0.0;
        }
        
        samples.iter().map(|(_, loss, _)| *loss).sum::<f64>() / samples.len() as f64
    }
    
    fn avg_bandwidth(&self) -> u64 {
        let samples = self.samples.read().unwrap();
        if samples.is_empty() {
            return 1_000_000; // 默认 1Mbps
        }
        
        samples.iter().map(|(_, _, bw)| *bw).sum::<u64>() / samples.len() as u64
    }
    
    fn rtt_coefficient_of_variation(&self) -> Option<f64> {
        let samples = self.samples.read().unwrap();
        if samples.len() < 2 {
            return None;
        }
        
        let rtts: Vec<f64> = samples.iter()
            .map(|(rtt, _, _)| rtt.as_micros() as f64)
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
            cv < 0.2 // RTT 变异系数小于 0.2 认为是稳定的
        } else {
            false
        }
    }
    
    fn clear(&mut self) {
        let mut samples = self.samples.write().unwrap();
        samples.clear();
        debug!("指标窗口已清空");
    }
}

/// 拥塞控制管理器
/// 
/// 集成 rat_congestion 库，提供拥塞控制功能
pub struct CongestionControlManager {
    /// 拥塞控制器
    controller: Option<CongestionController<RatEngineNetworkMetrics, RatEngineMetricsWindow>>,
    /// 配置
    config: CongestionControlConfig,
    /// 网络指标
    network_metrics: Arc<RatEngineNetworkMetrics>,
    /// 指标窗口
    metrics_window: RatEngineMetricsWindow,
    /// 上次更新时间
    last_update: Instant,
}

impl CongestionControlManager {
    /// 创建新的拥塞控制管理器
    pub fn new(config: CongestionControlConfig, engine_metrics: Arc<AtomicMetrics>) -> Self {
        let network_metrics = Arc::new(RatEngineNetworkMetrics::new(engine_metrics));
        let metrics_window = RatEngineMetricsWindow::new(config.metrics_window_size);
        
        let controller = if config.enabled {
            let controller_config = ControllerConfig::builder()
                .initial_algorithm(&config.algorithm)
                .auto_switching(config.auto_switching)
                .platform_optimized(config.platform_optimized)
                .metrics_window_size(config.metrics_window_size)
                .switch_cooldown(Duration::from_millis(config.switch_cooldown_ms))
                .build();
            
            // 注意：这里需要克隆，因为 CongestionController 需要拥有所有权
            let metrics_clone = RatEngineNetworkMetrics::new(network_metrics.engine_metrics.clone());
            let window_clone = RatEngineMetricsWindow::new(config.metrics_window_size);
            
            match std::panic::catch_unwind(|| {
                CongestionController::with_config(controller_config, metrics_clone, window_clone)
            }) {
                Ok(controller) => {
                    info!("拥塞控制已启用: 算法={}, 自动切换={}, 平台优化={}", 
                          config.algorithm, config.auto_switching, config.platform_optimized);
                    Some(controller)
                },
                Err(_) => {
                    error!("拥塞控制初始化失败，将禁用拥塞控制功能");
                    None
                }
            }
        } else {
            info!("拥塞控制已禁用");
            None
        };
        
        Self {
            controller,
            config,
            network_metrics,
            metrics_window,
            last_update: Instant::now(),
        }
    }
    
    /// 处理数据包发送事件
    pub fn on_packet_sent(&mut self, bytes: u32) {
        // CongestionController 没有 on_packet_sent 方法
        // 我们只在内部的 NetworkMetrics 中记录
        // 注意：这里需要获取可变引用，但 controller 拥有 metrics
        // 所以我们通过 update_metrics 来间接更新
        debug!("数据包发送: {} 字节", bytes);
    }
    
    /// 处理数据包确认事件
    pub fn on_packet_acked(&mut self, bytes: u32, rtt: Duration) {
        if let Some(ref mut controller) = self.controller {
            controller.on_packet_acked(bytes.into(), rtt);
        }
    }
    
    /// 处理数据包丢失事件
    pub fn on_packet_lost(&mut self, bytes: u32) {
        if let Some(ref mut controller) = self.controller {
            controller.on_packet_lost(bytes.into());
        }
    }
    
    /// 获取当前窗口大小
    pub fn window_size(&self) -> WindowSize {
        if let Some(ref controller) = self.controller {
            controller.window_size()
        } else {
            65536 // 默认窗口大小
        }
    }
    
    /// 获取当前发送速率
    pub fn pacing_rate(&self) -> PacingRate {
        if let Some(ref controller) = self.controller {
            controller.pacing_rate()
        } else {
            1_000_000 // 默认 1Mbps
        }
    }
    
    /// 获取当前算法名称
    pub fn current_algorithm(&self) -> String {
        if let Some(ref controller) = self.controller {
            controller.current_algorithm().to_string()
        } else {
            "disabled".to_string()
        }
    }
    
    /// 手动切换算法
    pub fn switch_algorithm(&mut self, algorithm: &str) -> Result<(), String> {
        if let Some(ref mut controller) = self.controller {
            controller.switch_algorithm(algorithm);
            info!("手动切换拥塞控制算法到: {}", algorithm);
            Ok(())
        } else {
            Err("拥塞控制未启用".to_string())
        }
    }
    
    /// 更新网络指标
    pub fn update_metrics(&mut self) {
        let now = Instant::now();
        
        // 每 100ms 更新一次指标
        if now.duration_since(self.last_update) >= Duration::from_millis(100) {
            self.network_metrics.update_from_engine_metrics();
            self.last_update = now;
        }
    }
    
    /// 获取拥塞控制统计信息
    pub fn get_stats(&self) -> std::collections::HashMap<String, f64> {
        let mut stats = std::collections::HashMap::new();
        
        stats.insert("enabled".to_string(), if self.controller.is_some() { 1.0 } else { 0.0 });
        stats.insert("window_size".to_string(), self.window_size() as f64);
        stats.insert("pacing_rate".to_string(), self.pacing_rate() as f64);
        stats.insert("rtt_us".to_string(), self.network_metrics.current_rtt_us.load(Ordering::Relaxed) as f64);
        stats.insert("bandwidth_bps".to_string(), self.network_metrics.bandwidth_estimate.load(Ordering::Relaxed) as f64);
        stats.insert("loss_rate".to_string(), self.network_metrics.loss_rate() * 100.0);
        
        stats
    }
    
    /// 是否启用拥塞控制
    pub fn is_enabled(&self) -> bool {
        self.controller.is_some()
    }
}