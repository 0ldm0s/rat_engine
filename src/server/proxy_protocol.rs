//! PROXY Protocol v2 解析器
//!
//! 支持解析 HAProxy 发送的 PROXY protocol v2 头部，
//! 获取原始客户端连接信息。

use std::net::{SocketAddr, Ipv4Addr, Ipv6Addr};
use crate::error::RatError;
use crate::utils::logger;

/// PROXY protocol v2 签名
const PROXY_V2_SIGNATURE: &[u8] = b"\x0D\x0A\x0D\x0A\x00\x0D\x0A\x51\x55\x49\x54\x0A";

/// PROXY protocol v2 命令类型
#[derive(Debug, Clone, Copy)]
pub enum ProxyCommand {
    Local = 0x00,
    Proxy = 0x01,
}

/// PROXY protocol v2 地址族
#[derive(Debug, Clone, Copy)]
pub enum ProxyAddressFamily {
    Unspec = 0x00,
    Inet = 0x01,
    Inet6 = 0x02,
    Unix = 0x03,
}

/// PROXY protocol v2 传输协议
#[derive(Debug, Clone, Copy)]
pub enum ProxyProtocol {
    Unspec = 0x00,
    Stream = 0x01,
    Datagram = 0x02,
}

/// PROXY protocol v2 地址信息
#[derive(Debug, Clone)]
pub struct ProxyAddresses {
    /// 源地址
    pub src_addr: SocketAddr,
    /// 目标地址
    pub dst_addr: SocketAddr,
}

/// PROXY protocol v2 TLV (Type-Length-Value) 信息
#[derive(Debug, Clone)]
pub struct ProxyTLV {
    /// 类型
    pub tpe: u8,
    /// 值
    pub value: Vec<u8>,
}

/// PROXY protocol v2 解析结果
#[derive(Debug, Clone)]
pub struct ProxyProtocolV2Info {
    /// 命令类型
    pub command: ProxyCommand,
    /// 地址族
    pub address_family: ProxyAddressFamily,
    /// 传输协议
    pub protocol: ProxyProtocol,
    /// 地址信息（如果有）
    pub addresses: Option<ProxyAddresses>,
    /// TLV 列表
    pub tlvs: Vec<ProxyTLV>,
    /// ALPN 协议（如果有）
    pub alpn: Option<String>,
}

impl ProxyProtocolV2Info {
    /// 创建一个新的 PROXY protocol v2 信息
    pub fn new(
        command: ProxyCommand,
        address_family: ProxyAddressFamily,
        protocol: ProxyProtocol,
    ) -> Self {
        Self {
            command,
            address_family,
            protocol,
            addresses: None,
            tlvs: Vec::new(),
            alpn: None,
        }
    }

    /// 获取客户端 IP 地址
    pub fn client_ip(&self) -> Option<String> {
        self.addresses.as_ref().map(|addrs| addrs.src_addr.ip().to_string())
    }

    /// 获取客户端端口
    pub fn client_port(&self) -> Option<u16> {
        self.addresses.as_ref().map(|addrs| addrs.src_addr.port())
    }
}

/// PROXY protocol v2 解析器
pub struct ProxyProtocolV2Parser;

impl ProxyProtocolV2Parser {
    /// 检查数据是否以 PROXY protocol v2 签名开头
    pub fn is_proxy_v2(data: &[u8]) -> bool {
        if data.len() < 16 {
            return false;
        }

        data.starts_with(PROXY_V2_SIGNATURE) && (data[12] & 0xF0) == 0x20
    }

    /// 解析 PROXY protocol v2 头部
    pub fn parse(data: &[u8]) -> Result<ProxyProtocolV2Info, RatError> {
        if data.len() < 16 {
            return Err(RatError::InvalidArgument("PROXY protocol v2 头部太短".to_string()));
        }

        // 检查签名
        if !data.starts_with(PROXY_V2_SIGNATURE) {
            return Err(RatError::InvalidArgument("无效的 PROXY protocol v2 签名".to_string()));
        }

        // 检查版本
        let version_cmd = data[12];
        let version = (version_cmd & 0xF0) >> 4;
        if version != 2 {
            return Err(RatError::InvalidArgument(format!("不支持的 PROXY protocol 版本: {}", version)));
        }

        let command = match version_cmd & 0x0F {
            0x00 => ProxyCommand::Local,
            0x01 => ProxyCommand::Proxy,
            _ => return Err(RatError::InvalidArgument("未知的 PROXY 命令".to_string())),
        };

        let fam_protocol = data[13];
        let address_family = match fam_protocol >> 4 {
            0x00 => ProxyAddressFamily::Unspec,
            0x01 => ProxyAddressFamily::Inet,
            0x02 => ProxyAddressFamily::Inet6,
            0x03 => ProxyAddressFamily::Unix,
            _ => return Err(RatError::InvalidArgument("未知的地址族".to_string())),
        };

        let protocol = match fam_protocol & 0x0F {
            0x00 => ProxyProtocol::Unspec,
            0x01 => ProxyProtocol::Stream,
            0x02 => ProxyProtocol::Datagram,
            _ => return Err(RatError::InvalidArgument("未知的传输协议".to_string())),
        };

        let len = u16::from_be_bytes([data[14], data[15]]) as usize;

        if data.len() < 16 + len {
            return Err(RatError::InvalidArgument("PROXY protocol v2 数据不完整".to_string()));
        }

        let mut info = ProxyProtocolV2Info::new(command, address_family, protocol);

        // 解析地址信息
        if len > 0 {
            Self::parse_addresses(&data[16..16+len], &mut info)?;

            // 解析 TLV
            let tlv_data = &data[16+len..];
            if !tlv_data.is_empty() {
                Self::parse_tlvs(tlv_data, &mut info)?;
            }
        }

        Ok(info)
    }

    /// 解析地址信息
    fn parse_addresses(data: &[u8], info: &mut ProxyProtocolV2Info) -> Result<(), RatError> {
        use std::net::{IpAddr, SocketAddrV4, SocketAddrV6};

        match (info.address_family, info.protocol) {
            (ProxyAddressFamily::Inet, ProxyProtocol::Stream) => {
                // TCP over IPv4 - 12 字节
                if data.len() < 12 {
                    return Err(RatError::InvalidArgument("IPv4 地址数据太短".to_string()));
                }

                let src_ip = Ipv4Addr::from([
                    data[0], data[1], data[2], data[3]
                ]);
                let dst_ip = Ipv4Addr::from([
                    data[4], data[5], data[6], data[7]
                ]);
                let src_port = u16::from_be_bytes([data[8], data[9]]);
                let dst_port = u16::from_be_bytes([data[10], data[11]]);

                info.addresses = Some(ProxyAddresses {
                    src_addr: SocketAddr::V4(SocketAddrV4::new(src_ip, src_port)),
                    dst_addr: SocketAddr::V4(SocketAddrV4::new(dst_ip, dst_port)),
                });
            }

            (ProxyAddressFamily::Inet6, ProxyProtocol::Stream) => {
                // TCP over IPv6 - 36 字节
                if data.len() < 36 {
                    return Err(RatError::InvalidArgument("IPv6 地址数据太短".to_string()));
                }

                let mut src_ip_bytes = [0u8; 16];
                let mut dst_ip_bytes = [0u8; 16];
                src_ip_bytes.copy_from_slice(&data[0..16]);
                dst_ip_bytes.copy_from_slice(&data[16..32]);

                let src_ip = Ipv6Addr::from(src_ip_bytes);
                let dst_ip = Ipv6Addr::from(dst_ip_bytes);
                let src_port = u16::from_be_bytes([data[32], data[33]]);
                let dst_port = u16::from_be_bytes([data[34], data[35]]);

                info.addresses = Some(ProxyAddresses {
                    src_addr: SocketAddr::V6(SocketAddrV6::new(src_ip, src_port, 0, 0)),
                    dst_addr: SocketAddr::V6(SocketAddrV6::new(dst_ip, dst_port, 0, 0)),
                });
            }

            (ProxyAddressFamily::Unspec, _) => {
                // 未指定协议，忽略地址信息
            }

            _ => {
                logger::debug!("不支持的地址族和协议组合: {:?} {:?}", info.address_family, info.protocol);
            }
        }

        Ok(())
    }

    /// 解析 TLV (Type-Length-Value) 字段
    fn parse_tlvs(data: &[u8], info: &mut ProxyProtocolV2Info) -> Result<(), RatError> {
        let mut pos = 0;

        while pos + 3 <= data.len() {
            let tpe = data[pos];
            let len = u16::from_be_bytes([data[pos + 1], data[pos + 2]]) as usize;

            if pos + 3 + len > data.len() {
                break; // TLV 数据不完整，停止解析
            }

            let value = if len > 0 {
                data[pos + 3..pos + 3 + len].to_vec()
            } else {
                Vec::new()
            };

            // 处理特殊类型的 TLV
            match tpe {
                0x01 => {
                    // PP2_TYPE_ALPN - Application-Layer Protocol Negotiation
                    if let Ok(alpn_str) = String::from_utf8(value.clone()) {
                        info.alpn = Some(alpn_str);
                    }
                }
                0x02 => {
                    // PP2_TYPE_AUTHORITY - SNI/Authority
                    logger::debug!("PROXY TLV - Authority: {:?}", String::from_utf8(value.clone()));
                }
                0x20 => {
                    // PP2_TYPE_SSL - SSL 信息
                    logger::debug!("PROXY TLV - SSL information present");
                }
                _ => {
                    // 其他类型的 TLV
                    logger::debug!("PROXY TLV - Type: 0x{:02x}, Length: {}", tpe, len);
                }
            }

            info.tlvs.push(ProxyTLV { tpe, value: value.clone() });

            pos += 3 + len;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proxy_v2_signature() {
        assert!(ProxyProtocolV2Parser::is_proxy_v2(b"\x0D\x0A\x0D\x0A\x00\x0D\x0A\x51\x55\x49\x54\x0A\x20\x00\x00\x00"));
        assert!(!ProxyProtocolV2Parser::is_proxy_v2(b"GET / HTTP/1.1"));
    }

    #[test]
    fn test_parse_proxy_v2_tcp4() {
        // 构造一个 PROXY v2 TCP4 头部
        let mut header = Vec::from(PROXY_V2_SIGNATURE);
        header.push(0x21); // Version 2, Command PROXY
        header.push(0x11); // AF_INET, STREAM
        header.extend_from_slice(&12u16.to_be_bytes()); // Length = 12

        // 源 IP: 192.168.1.100, 目标 IP: 10.0.0.1
        header.extend_from_slice(&[192, 168, 1, 100]);
        header.extend_from_slice(&[10, 0, 0, 1]);
        // 源端口: 12345, 目标端口: 443
        header.extend_from_slice(&12345u16.to_be_bytes());
        header.extend_from_slice(&443u16.to_be_bytes());

        let info = ProxyProtocolV2Parser::parse(&header).unwrap();
        assert!(matches!(info.command, ProxyCommand::Proxy));
        assert!(matches!(info.address_family, ProxyAddressFamily::Inet));
        assert!(matches!(info.protocol, ProxyProtocol::Stream));

        let addrs = info.addresses.unwrap();
        assert_eq!(addrs.src_addr.ip().to_string(), "192.168.1.100");
        assert_eq!(addrs.src_addr.port(), 12345);
        assert_eq!(addrs.dst_addr.ip().to_string(), "10.0.0.1");
        assert_eq!(addrs.dst_addr.port(), 443);
    }
}