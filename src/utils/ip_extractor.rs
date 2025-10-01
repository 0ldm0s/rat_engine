//! IP 地址提取工具
//! 
//! 提供统一的真实 IP 地址提取功能，支持多种代理协议和头部
//! 包括 Cloudflare、HAProxy Proxy Protocol v2、AWS ALB 等

use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// IP 信息结构体
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpInfo {
    /// IP 地址字符串
    pub address: String,
    /// 是否为公网 IP
    pub is_public: bool,
    /// 是否为有效 IP（排除广播、组播等）
    pub is_valid: bool,
}

impl IpInfo {
    /// 创建新的 IP 信息
    pub fn new(address: String, is_public: bool, is_valid: bool) -> Self {
        Self {
            address,
            is_public,
            is_valid,
        }
    }
    
    /// 判断是否为内网 IP
    pub fn is_private(&self) -> bool {
        !self.is_public
    }
}

/// IP 提取器
/// 
/// 负责从各种代理头部和协议中提取真实的客户端 IP 地址
pub struct IpExtractor;

impl IpExtractor {
    /// 从 HTTP 头部提取真实 IP 信息（推荐使用）
    /// 
    /// # 参数
    /// * `headers` - HTTP 请求头部映射
    /// * `remote_addr` - 直连的远程地址
    /// 
    /// # 返回
    /// 包含 IP 地址和类型信息的 IpInfo 结构体
    pub fn extract_real_ip_info(headers: &HashMap<String, String>, remote_addr: &str) -> IpInfo {
        Self::extract_real_ip_info_internal(headers, remote_addr)
    }
    
    /// 从 HTTP 头部提取真实 IP 地址 (仅公网IP) - 向后兼容
    /// 
    /// # 参数
    /// * `headers` - HTTP 请求头部映射
    /// * `remote_addr` - 直连的远程地址
    /// 
    /// # 返回
    /// 提取到的真实 IP 地址字符串
    pub fn extract_real_ip(headers: &HashMap<String, String>, remote_addr: &str) -> String {
        Self::extract_real_ip_with_validation(headers, remote_addr, true)
    }
    
    /// 从 HTTP 头部提取真实 IP 地址 (支持内网IP) - 向后兼容
    /// 
    /// # 参数
    /// * `headers` - HTTP 请求头部映射
    /// * `remote_addr` - 直连的远程地址
    /// 
    /// # 返回
    /// 提取到的真实 IP 地址字符串
    pub fn extract_real_ip_allow_private(headers: &HashMap<String, String>, remote_addr: &str) -> String {
        Self::extract_real_ip_with_validation(headers, remote_addr, false)
    }
    
    /// 从 HTTP 头部提取真实 IP 信息 (内部实现)
    /// 
    /// # 参数
    /// * `headers` - HTTP 请求头部映射
    /// * `remote_addr` - 直连的远程地址
    /// 
    /// # 返回
    /// 包含 IP 地址和类型信息的 IpInfo 结构体
    fn extract_real_ip_info_internal(headers: &HashMap<String, String>, remote_addr: &str) -> IpInfo {
        // 按优先级检查各种代理头部
        let proxy_headers = [
            // Cloudflare 专用头部 (最高优先级)
            "cf-connecting-ip",
            "cf-ipcountry", // 备用 Cloudflare 头部
            
            // 标准代理头部
            "x-forwarded-for",
            "x-real-ip",
            "x-client-ip",
            
            // AWS ALB/ELB 头部
            "x-forwarded-for",
            "x-amzn-trace-id",
            
            // 其他常见代理头部
            "true-client-ip",
            "x-originating-ip",
            "x-cluster-client-ip",
            "forwarded-for",
            "forwarded",
            "via",
        ];
        
        for header_name in &proxy_headers {
            if let Some(value) = Self::get_header_case_insensitive(headers, header_name) {
                if let Some(ip) = Self::parse_ip_from_header(&value, header_name) {
                    if Self::is_valid_ip(&ip) {
                        let is_public = Self::is_valid_public_ip(&ip);
                        return IpInfo::new(ip, is_public, true);
                    }
                }
            }
        }
        
        // 如果没有找到有效的代理头部，使用直连地址
        let fallback_ip = Self::extract_ip_from_addr(remote_addr);
        let is_public = Self::is_valid_public_ip(&fallback_ip);
        let is_valid = Self::is_valid_ip(&fallback_ip);
        
        IpInfo::new(fallback_ip, is_public, is_valid)
    }
    
    /// 从 HTTP 头部提取真实 IP 地址 (内部实现) - 向后兼容
    /// 
    /// # 参数
    /// * `headers` - HTTP 请求头部映射
    /// * `remote_addr` - 直连的远程地址
    /// * `public_only` - 是否只接受公网IP
    /// 
    /// # 返回
    /// 提取到的真实 IP 地址字符串
    fn extract_real_ip_with_validation(headers: &HashMap<String, String>, remote_addr: &str, public_only: bool) -> String {
        // 按优先级检查各种代理头部
        let proxy_headers = [
            // Cloudflare 专用头部 (最高优先级)
            "cf-connecting-ip",
            "cf-ipcountry", // 备用 Cloudflare 头部
            
            // 标准代理头部
            "x-forwarded-for",
            "x-real-ip",
            "x-client-ip",
            
            // AWS ALB/ELB 头部
            "x-forwarded-for",
            "x-amzn-trace-id",
            
            // 其他常见代理头部
            "true-client-ip",
            "x-originating-ip",
            "x-cluster-client-ip",
            "forwarded-for",
            "forwarded",
            "via",
        ];
        
        for header_name in &proxy_headers {
            if let Some(value) = Self::get_header_case_insensitive(headers, header_name) {
                if let Some(ip) = Self::parse_ip_from_header(&value, header_name) {
                    if public_only {
                        if Self::is_valid_public_ip(&ip) {
                            return ip;
                        }
                    } else {
                        if Self::is_valid_ip(&ip) {
                            return ip;
                        }
                    }
                }
            }
        }
        
        // 如果没有找到有效的代理头部，从 remote_addr 提取 IP
        Self::extract_ip_from_addr(remote_addr)
    }
    
    /// 从 Hyper 请求头部提取真实 IP 信息（推荐使用）
    /// 
    /// # 参数
    /// * `headers` - Hyper HeaderMap
    /// * `remote_addr` - 直连的远程地址
    /// 
    /// # 返回
    /// 包含 IP 地址和类型信息的 IpInfo 结构体
    pub fn extract_real_ip_info_from_hyper(
        headers: &hyper::HeaderMap,
        remote_addr: Option<SocketAddr>
    ) -> IpInfo {
        Self::extract_real_ip_info_from_hyper_internal(headers, remote_addr)
    }
    
    /// 从 Hyper 请求头部提取真实 IP 地址 (仅公网IP) - 向后兼容
    /// 
    /// # 参数
    /// * `headers` - Hyper HeaderMap
    /// * `remote_addr` - 直连的远程地址
    /// 
    /// # 返回
    /// 提取到的真实 IP 地址字符串
    pub fn extract_real_ip_from_hyper(
        headers: &hyper::HeaderMap,
        remote_addr: Option<SocketAddr>
    ) -> String {
        Self::extract_real_ip_from_hyper_with_validation(headers, remote_addr, true)
    }
    
    /// 从 Hyper 请求头部提取真实 IP 地址 (支持内网IP) - 向后兼容
    /// 
    /// # 参数
    /// * `headers` - Hyper HeaderMap
    /// * `remote_addr` - 直连的远程地址
    /// 
    /// # 返回
    /// 提取到的真实 IP 地址字符串
    pub fn extract_real_ip_from_hyper_allow_private(
        headers: &hyper::HeaderMap,
        remote_addr: Option<SocketAddr>
    ) -> String {
        Self::extract_real_ip_from_hyper_with_validation(headers, remote_addr, false)
    }
    
    /// 从 Hyper 请求头部提取真实 IP 信息 (内部实现)
    /// 
    /// # 参数
    /// * `headers` - Hyper HeaderMap
    /// * `remote_addr` - 直连的远程地址
    /// 
    /// # 返回
    /// 包含 IP 地址和类型信息的 IpInfo 结构体
    fn extract_real_ip_info_from_hyper_internal(
        headers: &hyper::HeaderMap,
        remote_addr: Option<SocketAddr>
    ) -> IpInfo {
        // 按优先级检查各种代理头部
        let proxy_headers = [
            "cf-connecting-ip",
            "x-forwarded-for",
            "x-real-ip",
            "x-client-ip",
            "true-client-ip",
            "x-originating-ip",
            "x-cluster-client-ip",
        ];
        
        for header_name in &proxy_headers {
            if let Some(header_value) = headers.get(*header_name) {
                if let Ok(value_str) = header_value.to_str() {
                    if let Some(ip) = Self::parse_ip_from_header(value_str, header_name) {
                        if Self::is_valid_ip(&ip) {
                            let is_public = Self::is_valid_public_ip(&ip);
                            return IpInfo::new(ip, is_public, true);
                        }
                    }
                }
            }
        }
        
        // 如果没有找到有效的代理头部，从 remote_addr 提取 IP
        let fallback_ip = Self::extract_ip_from_addr_option(remote_addr);
        let is_public = Self::is_valid_public_ip(&fallback_ip);
        let is_valid = Self::is_valid_ip(&fallback_ip);
        
        IpInfo::new(fallback_ip, is_public, is_valid)
    }
    
    /// 从 Hyper 请求头部提取真实 IP 地址 (内部实现) - 向后兼容
    /// 
    /// # 参数
    /// * `headers` - Hyper HeaderMap
    /// * `remote_addr` - 直连的远程地址
    /// * `public_only` - 是否只接受公网IP
    /// 
    /// # 返回
    /// 提取到的真实 IP 地址字符串
    fn extract_real_ip_from_hyper_with_validation(
        headers: &hyper::HeaderMap,
        remote_addr: Option<SocketAddr>,
        public_only: bool
    ) -> String {
        // 按优先级检查各种代理头部
        let proxy_headers = [
            "cf-connecting-ip",
            "x-forwarded-for",
            "x-real-ip",
            "x-client-ip",
            "true-client-ip",
            "x-originating-ip",
            "x-cluster-client-ip",
        ];
        
        for header_name in &proxy_headers {
            if let Some(header_value) = headers.get(*header_name) {
                if let Ok(value_str) = header_value.to_str() {
                    if let Some(ip) = Self::parse_ip_from_header(value_str, header_name) {
                        if public_only {
                            if Self::is_valid_public_ip(&ip) {
                                return ip;
                            }
                        } else {
                            if Self::is_valid_ip(&ip) {
                                return ip;
                            }
                        }
                    }
                }
            }
        }
        
        // 如果没有找到有效的代理头部，返回直连 IP
        if let Some(addr) = remote_addr {
            addr.ip().to_string()
        } else {
            "unknown".to_string()
        }
    }
    
    /// 解析 HAProxy Proxy Protocol v2
    /// 
    /// # 参数
    /// * `data` - 原始的 Proxy Protocol 数据
    /// 
    /// # 返回
    /// 解析出的源 IP 地址，如果解析失败返回 None
    pub fn parse_proxy_protocol_v2(data: &[u8]) -> Option<String> {
        // Proxy Protocol v2 签名: \x0D\x0A\x0D\x0A\x00\x0D\x0A\x51\x55\x49\x54\x0A
        const SIGNATURE: &[u8] = b"\r\n\r\n\x00\r\nQUIT\n";
        
        if data.len() < 16 || !data.starts_with(SIGNATURE) {
            return None;
        }
        
        // 检查版本和命令 (第13字节)
        let ver_cmd = data[12];
        let version = (ver_cmd & 0xF0) >> 4;
        let command = ver_cmd & 0x0F;
        
        if version != 2 || command != 1 {
            return None; // 只处理 v2 PROXY 命令
        }
        
        // 检查协议族和协议 (第14字节)
        let fam_proto = data[13];
        let family = (fam_proto & 0xF0) >> 4;
        let protocol = fam_proto & 0x0F;
        
        // 获取地址长度 (第15-16字节)
        let addr_len = u16::from_be_bytes([data[14], data[15]]) as usize;
        
        if data.len() < 16 + addr_len {
            return None;
        }
        
        let addr_data = &data[16..16 + addr_len];
        
        match (family, protocol) {
            (1, 1) => { // IPv4 TCP
                if addr_data.len() >= 12 {
                    let src_ip = format!("{}.{}.{}.{}", 
                        addr_data[0], addr_data[1], addr_data[2], addr_data[3]);
                    Some(src_ip)
                } else {
                    None
                }
            },
            (2, 1) => { // IPv6 TCP
                if addr_data.len() >= 36 {
                    let mut ipv6_parts = Vec::new();
                    for i in (0..16).step_by(2) {
                        let part = u16::from_be_bytes([addr_data[i], addr_data[i + 1]]);
                        ipv6_parts.push(format!("{:x}", part));
                    }
                    Some(ipv6_parts.join(":"))
                } else {
                    None
                }
            },
            _ => None, // 不支持的协议族/协议组合
        }
    }
    
    /// 大小写不敏感地获取头部值
    fn get_header_case_insensitive(headers: &HashMap<String, String>, key: &str) -> Option<String> {
        let key_lower = key.to_lowercase();
        for (header_key, header_value) in headers {
            if header_key.to_lowercase() == key_lower {
                return Some(header_value.clone());
            }
        }
        None
    }
    
    /// 从头部值中解析 IP 地址
    fn parse_ip_from_header(value: &str, header_name: &str) -> Option<String> {
        let trimmed = value.trim();
        
        if trimmed.is_empty() || trimmed == "unknown" {
            return None;
        }
        
        match header_name {
            "x-forwarded-for" | "forwarded-for" => {
                // X-Forwarded-For 可能包含多个 IP，格式: "client, proxy1, proxy2"
                // 取第一个 IP (最左边的是原始客户端)
                trimmed.split(',').next()?.trim().to_string().into()
            },
            "forwarded" => {
                // RFC 7239 Forwarded 头部格式: "for=192.0.2.60;proto=http;by=203.0.113.43"
                Self::parse_forwarded_header(trimmed)
            },
            _ => {
                // 其他头部通常直接包含 IP 地址
                Some(trimmed.to_string())
            }
        }
    }
    
    /// 解析 RFC 7239 Forwarded 头部
    fn parse_forwarded_header(value: &str) -> Option<String> {
        // 查找 for= 参数
        for part in value.split(';') {
            let part = part.trim();
            if part.starts_with("for=") {
                let ip_part = &part[4..]; // 跳过 "for="
                // 移除可能的引号和端口号
                let ip = ip_part.trim_matches('"').split(':').next()?;
                return Some(ip.to_string());
            }
        }
        None
    }
    
    /// 检查 IP 是否为有效的公网 IP
    fn is_valid_public_ip(ip_str: &str) -> bool {
        if let Ok(ip) = IpAddr::from_str(ip_str) {
            match ip {
                IpAddr::V4(ipv4) => {
                    // 排除私有地址、回环地址、链路本地地址等
                    !ipv4.is_private() && 
                    !ipv4.is_loopback() && 
                    !ipv4.is_link_local() && 
                    !ipv4.is_broadcast() && 
                    !ipv4.is_multicast() &&
                    !ipv4.is_unspecified()
                },
                IpAddr::V6(ipv6) => {
                    // 排除私有地址、回环地址、链路本地地址等
                    !ipv6.is_loopback() && 
                    !ipv6.is_multicast() && 
                    !ipv6.is_unspecified()
                }
            }
        } else {
            false
        }
    }
    
    /// 检查 IP 是否为有效的 IP 地址 (包括私有IP)
    fn is_valid_ip(ip_str: &str) -> bool {
        if let Ok(ip) = IpAddr::from_str(ip_str) {
            match ip {
                IpAddr::V4(ipv4) => {
                    // 只排除无效的特殊地址
                    !ipv4.is_broadcast() && 
                    !ipv4.is_multicast() &&
                    !ipv4.is_unspecified()
                },
                IpAddr::V6(ipv6) => {
                    // 只排除无效的特殊地址
                    !ipv6.is_multicast() && 
                    !ipv6.is_unspecified()
                }
            }
        } else {
            false
        }
    }
    
    /// 从地址字符串中提取 IP 部分
    fn extract_ip_from_addr(addr_str: &str) -> String {
        // 处理 "IP:port" 格式
        if let Some(colon_pos) = addr_str.rfind(':') {
            addr_str[..colon_pos].to_string()
        } else {
            addr_str.to_string()
        }
    }
    
    /// 从 Option<SocketAddr> 中提取 IP 部分
    fn extract_ip_from_addr_option(addr: Option<SocketAddr>) -> String {
        match addr {
            Some(socket_addr) => socket_addr.ip().to_string(),
            None => "unknown".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_extract_real_ip_cloudflare() {
        let mut headers = HashMap::new();
        headers.insert("cf-connecting-ip".to_string(), "203.0.113.1".to_string());
        headers.insert("x-forwarded-for".to_string(), "192.168.1.1".to_string());
        
        let ip = IpExtractor::extract_real_ip(&headers, "127.0.0.1:8080");
        assert_eq!(ip, "203.0.113.1"); // Cloudflare 头部优先级更高
    }
    
    #[test]
    fn test_extract_real_ip_x_forwarded_for() {
        let mut headers = HashMap::new();
        headers.insert("x-forwarded-for".to_string(), "203.0.113.1, 192.168.1.1, 10.0.0.1".to_string());
        
        let ip = IpExtractor::extract_real_ip(&headers, "127.0.0.1:8080");
        assert_eq!(ip, "203.0.113.1"); // 应该取第一个 IP
    }
    
    #[test]
    fn test_extract_real_ip_forwarded() {
        let mut headers = HashMap::new();
        headers.insert("forwarded".to_string(), "for=203.0.113.1;proto=https;by=192.168.1.1".to_string());
        
        let ip = IpExtractor::extract_real_ip(&headers, "127.0.0.1:8080");
        assert_eq!(ip, "203.0.113.1");
    }
    
    #[test]
    fn test_extract_real_ip_fallback() {
        let headers = HashMap::new();
        
        let ip = IpExtractor::extract_real_ip(&headers, "203.0.113.1:8080");
        assert_eq!(ip, "203.0.113.1"); // 应该从 remote_addr 提取
    }
    
    #[test]
    fn test_is_valid_public_ip() {
        assert!(IpExtractor::is_valid_public_ip("203.0.113.1")); // 公网 IP
        assert!(!IpExtractor::is_valid_public_ip("192.168.1.1")); // 私有 IP
        assert!(!IpExtractor::is_valid_public_ip("127.0.0.1")); // 回环 IP
        assert!(!IpExtractor::is_valid_public_ip("10.0.0.1")); // 私有 IP
    }
    
    #[test]
    fn test_is_valid_ip() {
        assert!(IpExtractor::is_valid_ip("203.0.113.1")); // 公网 IP
        assert!(IpExtractor::is_valid_ip("192.168.1.1")); // 私有 IP
        assert!(IpExtractor::is_valid_ip("127.0.0.1")); // 回环 IP
        assert!(IpExtractor::is_valid_ip("10.0.0.1")); // 私有 IP
        assert!(!IpExtractor::is_valid_ip("255.255.255.255")); // 广播地址
        assert!(!IpExtractor::is_valid_ip("0.0.0.0")); // 未指定地址
    }
    
    #[test]
    fn test_extract_real_ip_allow_private() {
        let mut headers = HashMap::new();
        
        // 测试私有IP头部
        headers.insert("x-real-ip".to_string(), "192.168.1.100".to_string());
        let ip = IpExtractor::extract_real_ip_allow_private(&headers, "127.0.0.1:8080");
        assert_eq!(ip, "192.168.1.100");
        
        // 测试回环IP头部
        headers.clear();
        headers.insert("x-real-ip".to_string(), "127.0.0.1".to_string());
        let ip = IpExtractor::extract_real_ip_allow_private(&headers, "203.0.113.1:8080");
        assert_eq!(ip, "127.0.0.1");
        
        // 测试X-Forwarded-For中的私有IP
        headers.clear();
        headers.insert("x-forwarded-for".to_string(), "192.168.1.1, 10.0.0.1".to_string());
        let ip = IpExtractor::extract_real_ip_allow_private(&headers, "127.0.0.1:8080");
        assert_eq!(ip, "192.168.1.1"); // 应该取第一个IP
        
        // 测试fallback到私有remote_addr
        headers.clear();
        let ip = IpExtractor::extract_real_ip_allow_private(&headers, "192.168.1.1:8080");
        assert_eq!(ip, "192.168.1.1");
    }
    
    #[test]
    fn test_parse_proxy_protocol_v2() {
        // 模拟 HAProxy Proxy Protocol v2 数据 (IPv4 TCP)
        let mut data = Vec::new();
        data.extend_from_slice(b"\r\n\r\n\x00\r\nQUIT\n"); // 签名
        data.push(0x21); // 版本2 + PROXY命令
        data.push(0x11); // IPv4 + TCP
        data.extend_from_slice(&[0x00, 0x0C]); // 地址长度 12 字节
        data.extend_from_slice(&[203, 0, 113, 1]); // 源 IP: 203.0.113.1
        data.extend_from_slice(&[192, 168, 1, 1]); // 目标 IP: 192.168.1.1
        data.extend_from_slice(&[0x1F, 0x90]); // 源端口: 8080
        data.extend_from_slice(&[0x00, 0x50]); // 目标端口: 80
        
        let ip = IpExtractor::parse_proxy_protocol_v2(&data);
        assert_eq!(ip, Some("203.0.113.1".to_string()));
    }
    
    #[test]
    fn test_ip_info_creation() {
        // 测试公网 IP
        let public_ip = IpInfo::new("203.0.113.1".to_string(), true, true);
        assert_eq!(public_ip.address, "203.0.113.1");
        assert!(public_ip.is_public);
        assert!(public_ip.is_valid);
        assert!(!public_ip.is_private());
        
        // 测试私网 IP
        let private_ip = IpInfo::new("192.168.1.1".to_string(), false, true);
        assert_eq!(private_ip.address, "192.168.1.1");
        assert!(!private_ip.is_public);
        assert!(private_ip.is_valid);
        assert!(private_ip.is_private());
        
        // 测试无效 IP
        let invalid_ip = IpInfo::new("unknown".to_string(), false, false);
        assert_eq!(invalid_ip.address, "unknown");
        assert!(!invalid_ip.is_public);
        assert!(!invalid_ip.is_valid);
        assert!(invalid_ip.is_private());
    }
    
    #[test]
    fn test_extract_real_ip_info() {
        let mut headers = HashMap::new();
        
        // 测试公网 IP
        headers.insert("x-forwarded-for".to_string(), "203.0.113.1".to_string());
        let ip_info = IpExtractor::extract_real_ip_info(&headers, "127.0.0.1:8080");
        assert_eq!(ip_info.address, "203.0.113.1");
        assert!(ip_info.is_public);
        assert!(ip_info.is_valid);
        
        // 测试私网 IP
        headers.clear();
        headers.insert("x-real-ip".to_string(), "192.168.1.100".to_string());
        let ip_info = IpExtractor::extract_real_ip_info(&headers, "127.0.0.1:8080");
        assert_eq!(ip_info.address, "192.168.1.100");
        assert!(!ip_info.is_public);
        assert!(ip_info.is_valid);
        assert!(ip_info.is_private());
    }
}