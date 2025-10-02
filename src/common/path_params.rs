//! 路径参数提取公共模块
//!
//! 提供统一的路径参数解析功能，供路由器和Python API共同使用
//! 避免代码重复，确保行为一致

use std::collections::HashMap;
use regex::Regex;
use crate::utils::logger::{debug, info, warn, error};

/// 参数类型定义
#[derive(Debug, Clone, PartialEq)]
pub enum ParamType {
    Int,        // 整数类型: <int:id> 或 <id> (默认)
    Float,      // 浮点数类型: <float:price>
    String,     // 字符串类型: <str:name> 或 <string:title>
    Uuid,       // UUID类型: <uuid:id>
    Path,       // 路径类型: <path:filepath> (可包含斜杠)
}

/// 参数类型验证和转换
pub fn validate_and_convert_param(value: &str, param_type: &ParamType) -> Result<String, String> {
    match param_type {
        ParamType::Int => {
            if value.chars().all(|c| c.is_ascii_digit()) || (value.starts_with('-') && value[1..].chars().all(|c| c.is_ascii_digit())) {
                Ok(value.to_string())
            } else {
                Err(format!("参数 '{}' 不是有效的整数", value))
            }
        }
        ParamType::Float => {
            if value.parse::<f64>().is_ok() {
                Ok(value.to_string())
            } else {
                Err(format!("参数 '{}' 不是有效的浮点数", value))
            }
        }
        ParamType::Uuid => {
            // 简单的UUID格式验证
            if uuid::Uuid::parse_str(value).is_ok() {
                Ok(value.to_string())
            } else {
                Err(format!("参数 '{}' 不是有效的UUID", value))
            }
        }
        ParamType::String | ParamType::Path => Ok(value.to_string()),
    }
}

/// 路径参数提取配置
#[derive(Debug, Clone)]
pub struct PathParamConfig {
    pub enable_logging: bool,
    pub url_decode: bool,
}

impl Default for PathParamConfig {
    fn default() -> Self {
        Self {
            enable_logging: true,
            url_decode: true,
        }
    }
}

/// 编译路径模式为正则表达式
pub fn compile_pattern(pattern: &str) -> Option<(Regex, Vec<String>)> {
    if !pattern.contains('<') {
        return None;
    }

    let mut param_names = Vec::new();
    let mut regex_pattern = pattern.to_string();

    let param_regex = Regex::new(r"<([^>]+)>").unwrap();

    regex_pattern = param_regex.replace_all(&regex_pattern, |caps: &regex::Captures| {
        let param_def = &caps[1];
        if param_def.contains(':') {
            let parts: Vec<&str> = param_def.split(':').collect();
            if parts.len() == 2 {
                let param_name = parts[1];
                param_names.push(param_name.to_string());
                r"([^/]+)"  // 统一使用字符串模式，类型约束后续处理
            } else {
                param_names.push(param_def.to_string());
                r"([^/]+)"
            }
        } else {
            param_names.push(param_def.to_string());
            r"([^/]+)"
        }
    }).to_string();

    regex_pattern = regex_pattern.replace(".", "\\.");
    regex_pattern = format!("^{}$", regex_pattern);

    match Regex::new(&regex_pattern) {
        Ok(regex) => Some((regex, param_names)),
        Err(e) => {
            error!("编译正则表达式失败 '{}': {}", regex_pattern, e);
            None
        }
    }
}

/// 从模式定义中解析参数类型
pub fn parse_param_type_from_pattern(pattern: &str, param_name: &str) -> ParamType {
    let param_regex = Regex::new(r"<([^>]+)>").unwrap();

    if let Some(caps) = param_regex.captures(pattern) {
        let param_def = &caps[1];
        if param_def.contains(':') {
            let parts: Vec<&str> = param_def.split(':').collect();
            if parts.len() == 2 && parts[1] == param_name {
                match parts[0] {
                    "int" => ParamType::Int,
                    "float" => ParamType::Float,
                    "str" | "string" => ParamType::String,
                    "uuid" => ParamType::Uuid,
                    "path" => ParamType::Path,
                    _ => ParamType::Int,  // 默认为int
                }
            } else {
                ParamType::Int  // 默认为int
            }
        } else if param_def == param_name {
            ParamType::Int  // 无类型约束时默认为int
        } else {
            ParamType::Int
        }
    } else {
        ParamType::Int
    }
}

/// 提取路径参数（包含类型验证）
/// 返回：(参数映射, 验证结果)
/// 成功时：`(params, Ok(()))`
/// 失败时：`(HashMap::new(), Err(error_message))`
pub fn extract_params(pattern: &str, path: &str, config: &PathParamConfig) -> (HashMap<String, String>, Result<(), String>) {
    let mut params = HashMap::new();

    // 如果模式不包含参数，直接返回空的 HashMap
    if !pattern.contains('<') {
        if config.enable_logging {
            debug!("路径模式 '{}' 不包含参数", pattern);
        }
        return (params, Ok(()));
    }

    if config.enable_logging {
        info!("开始提取路径参数 - 模式: '{}', 路径: '{}'", pattern, path);
    }

    // 编译路径模式
    if let Some((regex, param_names)) = compile_pattern(pattern) {
        if config.enable_logging {
            debug!("参数名列表: {:?}", param_names);
        }

        // 尝试匹配并提取参数
        if let Some(captures) = regex.captures(path) {
            if config.enable_logging {
                debug!("正则匹配成功，捕获组数量: {}", captures.len());
            }

            for (i, param_name) in param_names.iter().enumerate() {
                if let Some(capture) = captures.get(i + 1) {
                    let raw_value = capture.as_str();
                    let decoded_value = if config.url_decode {
                        // URL解码参数值
                        urlencoding::decode(raw_value)
                            .unwrap_or_else(|_| raw_value.into())
                            .to_string()
                    } else {
                        raw_value.to_string()
                    };

                    // 进行类型验证和转换
                    let param_type = parse_param_type_from_pattern(pattern, param_name);
                    match validate_and_convert_param(&decoded_value, &param_type) {
                        Ok(validated_value) => {
                            if config.enable_logging {
                                debug!("参数 '{}': 原始值='{}', 解码后='{}', 验证后='{}', 类型={:?}",
                                       param_name, raw_value, decoded_value, validated_value, param_type);
                            }
                            params.insert(param_name.clone(), validated_value);
                        }
                        Err(e) => {
                            if config.enable_logging {
                                warn!("参数验证失败 - 模式: '{}', 路径: '{}', 错误: {}", pattern, path, e);
                            }
                            // 验证失败时返回空参数和错误信息
                            return (HashMap::new(), Err(e));
                        }
                    }
                }
            }
        } else {
            let error_msg = format!("路径 '{}' 不匹配模式 '{}'", path, pattern);
            if config.enable_logging {
                warn!("{}", error_msg);
            }
            return (HashMap::new(), Err(error_msg));
        }
    } else {
        let error_msg = format!("路径模式 '{}' 编译失败", pattern);
        if config.enable_logging {
            error!("{}", error_msg);
        }
        return (HashMap::new(), Err(error_msg));
    }

    if config.enable_logging {
        info!("最终提取的参数: {:?}", params);
    }

    (params, Ok(()))
}

/// 简化版本的参数提取（使用默认配置）
/// 返回：(参数映射, 验证结果)
pub fn extract_params_simple(pattern: &str, path: &str) -> (HashMap<String, String>, Result<(), String>) {
    extract_params(pattern, path, &PathParamConfig::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_params_complete_behavior() {
        let config = PathParamConfig {
            enable_logging: false,
            url_decode: true,
        };

        // 测试默认int类型 - 成功案例
        let (params, result) = extract_params("/users/<id>", "/users/123", &config);
        assert!(result.is_ok());
        assert_eq!(params.get("id"), Some(&"123".to_string()));

        // 测试默认int类型 - 失败案例
        let (params, result) = extract_params("/users/<id>", "/users/abc", &config);
        assert!(result.is_err());
        assert!(params.is_empty());
        assert!(result.unwrap_err().contains("不是有效的整数"));

        // 测试显式int类型 - 成功案例
        let (params, result) = extract_params("/users/<int:id>", "/users/456", &config);
        assert!(result.is_ok());
        assert_eq!(params.get("id"), Some(&"456".to_string()));

        // 测试显式int类型 - 失败案例
        let (params, result) = extract_params("/users/<int:id>", "/users/xyz", &config);
        assert!(result.is_err());
        assert!(params.is_empty());

        // 测试字符串类型 - 应该都成功
        let (params, result) = extract_params("/users/<str:name>", "/users/john", &config);
        assert!(result.is_ok());
        assert_eq!(params.get("name"), Some(&"john".to_string()));

        let (params, result) = extract_params("/users/<str:name>", "/users/123", &config);
        assert!(result.is_ok());
        assert_eq!(params.get("name"), Some(&"123".to_string()));

        // 测试UUID类型 - 成功案例
        let (params, result) = extract_params("/users/<uuid:id>", "/users/550e8400-e29b-41d4-a716-446655440000", &config);
        assert!(result.is_ok());
        assert_eq!(params.get("id"), Some(&"550e8400-e29b-41d4-a716-446655440000".to_string()));

        // 测试UUID类型 - 失败案例
        let (params, result) = extract_params("/users/<uuid:id>", "/users/invalid-uuid", &config);
        assert!(result.is_err());
        assert!(params.is_empty());

        // 测试多参数
        let (params, result) = extract_params("/api/v1/users/<user_id>/posts/<post_id>", "/api/v1/users/123/posts/456", &config);
        assert!(result.is_ok());
        assert_eq!(params.get("user_id"), Some(&"123".to_string()));
        assert_eq!(params.get("post_id"), Some(&"456".to_string()));

        // 测试多参数 - 其中一个失败
        let (params, result) = extract_params("/api/v1/users/<user_id>/posts/<post_id>", "/api/v1/users/abc/posts/456", &config);
        assert!(result.is_err());
        assert!(params.is_empty());
    }
}