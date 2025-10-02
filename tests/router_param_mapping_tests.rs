//! 路由映射表测试
//!
//! 测试新增的快速路由参数映射功能，包括：
//! - 普通参数的快速提取
//! - path类型参数的特殊处理
//! - path类型参数位置的编译时检查

use rat_engine::server::router::RouteKey;
use hyper::Method;
use std::collections::HashMap;

#[test]
fn test_static_route_mapping() {
    // 测试静态路由（无参数）
    let route = RouteKey::new(Method::GET, "/api/status".to_string());

    // 静态路由应该没有参数映射
    assert!(route.get_param_mapping().is_none());

    // 静态路由应该只匹配完全相同的路径
    assert!(route.extract_params_fast(&Method::GET, "/api/status").is_some());
    assert!(route.extract_params_fast(&Method::GET, "/api/status/other").is_none());
    assert!(route.extract_params_fast(&Method::POST, "/api/status").is_none());
}

#[test]
fn test_single_param_mapping() {
    // 测试单个参数的路由
    let route = RouteKey::new(Method::GET, "/users/<id>".to_string());

    assert!(route.get_param_mapping().is_some());

    let param_mapping = route.get_param_mapping();
    let mapping = param_mapping.as_ref().unwrap();
    assert_eq!(mapping.get_param_positions().get("id"), Some(&1)); // 在第2个位置(0-based)
    assert_eq!(mapping.get_param_types().get("id"), Some(&"int".to_string()));

    // 测试参数提取
    let params = route.extract_params_fast(&Method::GET, "/users/123").unwrap();
    assert_eq!(params.get("id"), Some(&"123".to_string()));

    let params = route.extract_params_fast(&Method::GET, "/users/-456").unwrap();
    assert_eq!(params.get("id"), Some(&"-456".to_string()));

    let params = route.extract_params_fast(&Method::GET, "/users/user123").unwrap();
    assert_eq!(params.get("id"), Some(&"user123".to_string()));
}

#[test]
fn test_typed_param_mapping() {
    // 测试带类型的参数
    let route = RouteKey::new(Method::GET, "/users/<uuid:id>".to_string());

    assert!(route.get_param_mapping().is_some());

    let param_mapping = route.get_param_mapping();
    let mapping = param_mapping.as_ref().unwrap();
    assert_eq!(mapping.get_param_positions().get("id"), Some(&1));
    assert_eq!(mapping.get_param_types().get("id"), Some(&"uuid".to_string()));

    // 测试参数提取
    let params = route.extract_params_fast(&Method::GET, "/users/550e8400-e29b-41d4-a716-446655440000").unwrap();
    assert_eq!(params.get("id"), Some(&"550e8400-e29b-41d4-a716-446655440000".to_string()));
}

#[test]
fn test_float_param_mapping() {
    // 测试浮点数参数
    let route = RouteKey::new(Method::GET, "/products/price/<float:price>".to_string());

    assert!(route.get_param_mapping().is_some());

    let param_mapping = route.get_param_mapping();
    let mapping = param_mapping.as_ref().unwrap();
    assert_eq!(mapping.get_param_positions().get("price"), Some(&2));
    assert_eq!(mapping.get_param_types().get("price"), Some(&"float".to_string()));

    // 测试参数提取
    let params = route.extract_params_fast(&Method::GET, "/products/price/99.99").unwrap();
    assert_eq!(params.get("price"), Some(&"99.99".to_string()));

    let params = route.extract_params_fast(&Method::GET, "/products/price/-12.34").unwrap();
    assert_eq!(params.get("price"), Some(&"-12.34".to_string()));

    let params = route.extract_params_fast(&Method::GET, "/products/price/100").unwrap();
    assert_eq!(params.get("price"), Some(&"100".to_string()));
}

#[test]
fn test_path_param_mapping() {
    // 测试path类型参数
    let route = RouteKey::new(Method::GET, "/files/<path:file_path>".to_string());

    assert!(route.get_param_mapping().is_some());

    let param_mapping = route.get_param_mapping();
    let mapping = param_mapping.as_ref().unwrap();
    assert_eq!(mapping.get_param_positions().get("file_path"), Some(&1));
    assert_eq!(mapping.get_param_types().get("file_path"), Some(&"path".to_string()));

    // 测试单级路径
    let params = route.extract_params_fast(&Method::GET, "/files/readme.md").unwrap();
    assert_eq!(params.get("file_path"), Some(&"readme.md".to_string()));

    // 测试多级路径
    let params = route.extract_params_fast(&Method::GET, "/files/docs/readme.md").unwrap();
    assert_eq!(params.get("file_path"), Some(&"docs/readme.md".to_string()));

    // 测试复杂路径
    let params = route.extract_params_fast(&Method::GET, "/files/user/documents/2024/report.pdf").unwrap();
    assert_eq!(params.get("file_path"), Some(&"user/documents/2024/report.pdf".to_string()));

    // 测试包含特殊字符的路径
    let params = route.extract_params_fast(&Method::GET, "/files/我的文档.docx").unwrap();
    assert_eq!(params.get("file_path"), Some(&"我的文档.docx".to_string()));
}

#[test]
fn test_mixed_params_mapping() {
    // 测试混合参数
    let route = RouteKey::new(Method::GET, "/users/<int:user_id>/files/<path:file_path>".to_string());

    assert!(route.get_param_mapping().is_some());

    let param_mapping = route.get_param_mapping();
    let mapping = param_mapping.as_ref().unwrap();
    assert_eq!(mapping.get_param_positions().get("user_id"), Some(&1));
    assert_eq!(mapping.get_param_positions().get("file_path"), Some(&3));
    assert_eq!(mapping.get_param_types().get("user_id"), Some(&"int".to_string()));
    assert_eq!(mapping.get_param_types().get("file_path"), Some(&"path".to_string()));

    // 测试参数提取
    let params = route.extract_params_fast(&Method::GET, "/users/123/files/docs/readme.md").unwrap();
    assert_eq!(params.get("user_id"), Some(&"123".to_string()));
    assert_eq!(params.get("file_path"), Some(&"docs/readme.md".to_string()));

    let params = route.extract_params_fast(&Method::GET, "/users/-456/files/user/documents/report.pdf").unwrap();
    assert_eq!(params.get("user_id"), Some(&"-456".to_string()));
    assert_eq!(params.get("file_path"), Some(&"user/documents/report.pdf".to_string()));
}

#[test]
fn test_three_params_mapping() {
    // 测试三个参数的混合路由
    let route = RouteKey::new(Method::GET, "/api/<int:user_id>/posts/<int:post_id>/comments/<int:comment_id>".to_string());

    assert!(route.get_param_mapping().is_some());

    let param_mapping = route.get_param_mapping();
    let mapping = param_mapping.as_ref().unwrap();
    assert_eq!(mapping.get_param_positions().get("user_id"), Some(&1));
    assert_eq!(mapping.get_param_positions().get("post_id"), Some(&3));
    assert_eq!(mapping.get_param_positions().get("comment_id"), Some(&5));

    // 测试参数提取
    let params = route.extract_params_fast(&Method::GET, "/api/123/posts/456/comments/789").unwrap();
    assert_eq!(params.get("user_id"), Some(&"123".to_string()));
    assert_eq!(params.get("post_id"), Some(&"456".to_string()));
    assert_eq!(params.get("comment_id"), Some(&"789".to_string()));
}

#[test]
fn test_fast_match_vs_regex_consistency() {
    // 测试快速匹配与正则匹配的一致性

    let test_cases = vec![
        ("/users/123", "GET", "/users/<id>"),
        ("/products/99.99", "GET", "/products/price/<float:price>"),
        ("/files/docs/readme.md", "GET", "/files/<path:file_path>"),
        ("/users/123/files/docs/report.pdf", "GET", "/users/<int:user_id>/files/<path:file_path>"),
        ("/api/123/posts/456", "GET", "/api/<int:user_id>/posts/<int:post_id>"),
    ];

    for (path, method_str, pattern) in test_cases {
        let method = match method_str {
            "GET" => Method::GET,
            "POST" => Method::POST,
            _ => Method::GET,
        };

        let route = RouteKey::new(method.clone(), pattern.to_string());

        let fast_params = route.extract_params_fast(&method, path);
        let regex_params = route.matches(&method, path);

        assert_eq!(
            fast_params, regex_params,
            "快速匹配与正则匹配结果不一致 - 路径: {}, 模式: {}",
            path, pattern
        );
    }
}

#[test]
#[should_panic(expected = "path 参数")]
fn test_path_param_position_validation() {
    // 测试path参数位置的编译时检查
    // 这应该会panic，因为path参数不是最后一个参数

    RouteKey::new(Method::GET, "/files/<path:file_path>/download".to_string());
}

#[test]
#[should_panic(expected = "path 参数")]
fn test_path_param_with_another_param() {
    // 测试path参数后面还有其他参数的情况
    // 这应该会panic，因为path参数后面不能有其他参数

    RouteKey::new(Method::GET, "/files/<path:file_path>/<ext>".to_string());
}

#[test]
fn test_edge_cases() {
    // 测试边界情况

    // 空路径
    let route = RouteKey::new(Method::GET, "/".to_string());
    assert!(route.get_param_mapping().is_none());
    assert!(route.extract_params_fast(&Method::GET, "/").is_some());

    // 只有参数的路由
    let route = RouteKey::new(Method::GET, "/<id>".to_string());
    assert!(route.get_param_mapping().is_some());
    assert!(route.extract_params_fast(&Method::GET, "/123").is_some());

    // 根路径的path参数
    let route = RouteKey::new(Method::GET, "/<path:full_path>".to_string());
    assert!(route.get_param_mapping().is_some());

    let params = route.extract_params_fast(&Method::GET, "/api/v1/users/123/posts/456").unwrap();
    assert_eq!(params.get("full_path"), Some(&"api/v1/users/123/posts/456".to_string()));
}

#[test]
fn test_negative_numbers_and_special_chars() {
    // 测试负数和特殊字符

    // 负数
    let route = RouteKey::new(Method::GET, "/values/<int:value>".to_string());
    let params = route.extract_params_fast(&Method::GET, "/values/-123").unwrap();
    assert_eq!(params.get("value"), Some(&"-123".to_string()));

    // 负浮点数
    let route = RouteKey::new(Method::GET, "/prices/<float:price>".to_string());
    let params = route.extract_params_fast(&Method::GET, "/prices/-99.99").unwrap();
    assert_eq!(params.get("price"), Some(&"-99.99".to_string()));

    // 包含特殊字符的字符串
    let route = RouteKey::new(Method::GET, "/users/<str:username>".to_string());
    let params = route.extract_params_fast(&Method::GET, "/users/user-123_456").unwrap();
    assert_eq!(params.get("username"), Some(&"user-123_456".to_string()));

    // 包含点的用户名
    let params = route.extract_params_fast(&Method::GET, "/users/user.name").unwrap();
    assert_eq!(params.get("username"), Some(&"user.name".to_string()));
}