//! 错误信息多语言支持模块
//!
//! 为RatEngine的错误信息提供多语言支持

use std::collections::HashMap;
use rat_embed_lang::i18n::register_translations;

/// 初始化错误信息的翻译
pub fn init_error_translations() {
    let mut translations = HashMap::new();

    // ConfigError - 配置错误
    let mut config_error = HashMap::new();
    config_error.insert("zh-CN".to_string(), "配置错误: {msg}".to_string());
    config_error.insert("en-US".to_string(), "Configuration error: {msg}".to_string());
    config_error.insert("ja-JP".to_string(), "設定エラー: {msg}".to_string());
    translations.insert("config_error".to_string(), config_error);

    // NetworkError - 网络错误
    let mut network_error = HashMap::new();
    network_error.insert("zh-CN".to_string(), "网络错误: {msg}".to_string());
    network_error.insert("en-US".to_string(), "Network error: {msg}".to_string());
    network_error.insert("ja-JP".to_string(), "ネットワークエラー: {msg}".to_string());
    translations.insert("network_error".to_string(), network_error);

    // CacheError - 缓存错误
    let mut cache_error = HashMap::new();
    cache_error.insert("zh-CN".to_string(), "缓存错误: {msg}".to_string());
    cache_error.insert("en-US".to_string(), "Cache error: {msg}".to_string());
    cache_error.insert("ja-JP".to_string(), "キャッシュエラー: {msg}".to_string());
    translations.insert("cache_error".to_string(), cache_error);

    // RequestError - 请求错误
    let mut request_error = HashMap::new();
    request_error.insert("zh-CN".to_string(), "请求错误: {msg}".to_string());
    request_error.insert("en-US".to_string(), "Request error: {msg}".to_string());
    request_error.insert("ja-JP".to_string(), "リクエストエラー: {msg}".to_string());
    translations.insert("request_error".to_string(), request_error);

    // TimeoutError - 超时错误
    let mut timeout_error = HashMap::new();
    timeout_error.insert("zh-CN".to_string(), "超时错误: {msg}".to_string());
    timeout_error.insert("en-US".to_string(), "Timeout error: {msg}".to_string());
    timeout_error.insert("ja-JP".to_string(), "タイムアウトエラー: {msg}".to_string());
    translations.insert("timeout_error".to_string(), timeout_error);

    // DecodingError - 解码错误
    let mut decoding_error = HashMap::new();
    decoding_error.insert("zh-CN".to_string(), "解码错误: {msg}".to_string());
    decoding_error.insert("en-US".to_string(), "Decoding error: {msg}".to_string());
    decoding_error.insert("ja-JP".to_string(), "デコードエラー: {msg}".to_string());
    translations.insert("decoding_error".to_string(), decoding_error);

    // RouteError - 路由错误
    let mut route_error = HashMap::new();
    route_error.insert("zh-CN".to_string(), "路由错误: {msg}".to_string());
    route_error.insert("en-US".to_string(), "Route error: {msg}".to_string());
    route_error.insert("ja-JP".to_string(), "ルートエラー: {msg}".to_string());
    translations.insert("route_error".to_string(), route_error);

    // WorkerPoolError - 工作池错误
    let mut worker_pool_error = HashMap::new();
    worker_pool_error.insert("zh-CN".to_string(), "工作池错误: {msg}".to_string());
    worker_pool_error.insert("en-US".to_string(), "Worker pool error: {msg}".to_string());
    worker_pool_error.insert("ja-JP".to_string(), "ワーカープールエラー: {msg}".to_string());
    translations.insert("worker_pool_error".to_string(), worker_pool_error);

    // SystemInfoError - 系统信息错误
    let mut system_info_error = HashMap::new();
    system_info_error.insert("zh-CN".to_string(), "系统信息错误: {msg}".to_string());
    system_info_error.insert("en-US".to_string(), "System info error: {msg}".to_string());
    system_info_error.insert("ja-JP".to_string(), "システム情報エラー: {msg}".to_string());
    translations.insert("system_info_error".to_string(), system_info_error);

    // IoError - IO错误
    let mut io_error = HashMap::new();
    io_error.insert("zh-CN".to_string(), "IO错误: {msg}".to_string());
    io_error.insert("en-US".to_string(), "IO error: {msg}".to_string());
    io_error.insert("ja-JP".to_string(), "IOエラー: {msg}".to_string());
    translations.insert("io_error".to_string(), io_error);

    // HyperError - HTTP错误
    let mut hyper_error = HashMap::new();
    hyper_error.insert("zh-CN".to_string(), "HTTP错误: {msg}".to_string());
    hyper_error.insert("en-US".to_string(), "HTTP error: {msg}".to_string());
    hyper_error.insert("ja-JP".to_string(), "HTTPエラー: {msg}".to_string());
    translations.insert("hyper_error".to_string(), hyper_error);

    // ReqwestError - HTTP客户端错误
    let mut reqwest_error = HashMap::new();
    reqwest_error.insert("zh-CN".to_string(), "HTTP客户端错误: {msg}".to_string());
    reqwest_error.insert("en-US".to_string(), "HTTP client error: {msg}".to_string());
    reqwest_error.insert("ja-JP".to_string(), "HTTPクライアントエラー: {msg}".to_string());
    translations.insert("reqwest_error".to_string(), reqwest_error);

    // ParseError - 解析错误
    let mut parse_error = HashMap::new();
    parse_error.insert("zh-CN".to_string(), "解析错误: {msg}".to_string());
    parse_error.insert("en-US".to_string(), "Parse error: {msg}".to_string());
    parse_error.insert("ja-JP".to_string(), "解析エラー: {msg}".to_string());
    translations.insert("parse_error".to_string(), parse_error);

    // ValidationError - 验证错误
    let mut validation_error = HashMap::new();
    validation_error.insert("zh-CN".to_string(), "验证错误: {msg}".to_string());
    validation_error.insert("en-US".to_string(), "Validation error: {msg}".to_string());
    validation_error.insert("ja-JP".to_string(), "検証エラー: {msg}".to_string());
    translations.insert("validation_error".to_string(), validation_error);

    // SecurityError - 安全错误
    let mut security_error = HashMap::new();
    security_error.insert("zh-CN".to_string(), "安全错误: {msg}".to_string());
    security_error.insert("en-US".to_string(), "Security error: {msg}".to_string());
    security_error.insert("ja-JP".to_string(), "セキュリティエラー: {msg}".to_string());
    translations.insert("security_error".to_string(), security_error);

    // TlsError - TLS错误
    let mut tls_error = HashMap::new();
    tls_error.insert("zh-CN".to_string(), "TLS错误: {msg}".to_string());
    tls_error.insert("en-US".to_string(), "TLS error: {msg}".to_string());
    tls_error.insert("ja-JP".to_string(), "TLSエラー: {msg}".to_string());
    translations.insert("tls_error".to_string(), tls_error);

    // SerializationError - 序列化错误
    let mut serialization_error = HashMap::new();
    serialization_error.insert("zh-CN".to_string(), "序列化错误: {msg}".to_string());
    serialization_error.insert("en-US".to_string(), "Serialization error: {msg}".to_string());
    serialization_error.insert("ja-JP".to_string(), "シリアル化エラー: {msg}".to_string());
    translations.insert("serialization_error".to_string(), serialization_error);

    // DeserializationError - 反序列化错误
    let mut deserialization_error = HashMap::new();
    deserialization_error.insert("zh-CN".to_string(), "反序列化错误: {msg}".to_string());
    deserialization_error.insert("en-US".to_string(), "Deserialization error: {msg}".to_string());
    deserialization_error.insert("ja-JP".to_string(), "デシリアル化エラー: {msg}".to_string());
    translations.insert("deserialization_error".to_string(), deserialization_error);

    // PythonError - Python错误
    let mut python_error = HashMap::new();
    python_error.insert("zh-CN".to_string(), "Python错误: {msg}".to_string());
    python_error.insert("en-US".to_string(), "Python error: {msg}".to_string());
    python_error.insert("ja-JP".to_string(), "Pythonエラー: {msg}".to_string());
    translations.insert("python_error".to_string(), python_error);

    // TransferError - 传输错误
    let mut transfer_error = HashMap::new();
    transfer_error.insert("zh-CN".to_string(), "传输错误: {msg}".to_string());
    transfer_error.insert("en-US".to_string(), "Transfer error: {msg}".to_string());
    transfer_error.insert("ja-JP".to_string(), "転送エラー: {msg}".to_string());
    translations.insert("transfer_error".to_string(), transfer_error);

    // InvalidArgument - 无效参数错误
    let mut invalid_argument = HashMap::new();
    invalid_argument.insert("zh-CN".to_string(), "无效参数: {msg}".to_string());
    invalid_argument.insert("en-US".to_string(), "Invalid argument: {msg}".to_string());
    invalid_argument.insert("ja-JP".to_string(), "無効な引数: {msg}".to_string());
    translations.insert("invalid_argument".to_string(), invalid_argument);

    // Other - 其他错误
    let mut other_error = HashMap::new();
    other_error.insert("zh-CN".to_string(), "错误: {msg}".to_string());
    other_error.insert("en-US".to_string(), "Error: {msg}".to_string());
    other_error.insert("ja-JP".to_string(), "エラー: {msg}".to_string());
    translations.insert("other_error".to_string(), other_error);

    // 硬编码错误信息
    let mut send_failed = HashMap::new();
    send_failed.insert("zh-CN".to_string(), "发送失败: {msg}".to_string());
    send_failed.insert("en-US".to_string(), "Send failed: {msg}".to_string());
    send_failed.insert("ja-JP".to_string(), "送信失敗: {msg}".to_string());
    translations.insert("send_failed".to_string(), send_failed);

    let mut encode_data_failed = HashMap::new();
    encode_data_failed.insert("zh-CN".to_string(), "编码数据失败: {msg}".to_string());
    encode_data_failed.insert("en-US".to_string(), "Failed to encode data: {msg}".to_string());
    encode_data_failed.insert("ja-JP".to_string(), "データエンコード失敗: {msg}".to_string());
    translations.insert("encode_data_failed".to_string(), encode_data_failed);

    let mut encode_close_failed = HashMap::new();
    encode_close_failed.insert("zh-CN".to_string(), "编码关闭指令失败: {msg}".to_string());
    encode_close_failed.insert("en-US".to_string(), "Failed to encode close command: {msg}".to_string());
    encode_close_failed.insert("ja-JP".to_string(), "クローズコマンドエンコード失敗: {msg}".to_string());
    translations.insert("encode_close_failed".to_string(), encode_close_failed);

    let mut stream_not_found = HashMap::new();
    stream_not_found.insert("zh-CN".to_string(), "未找到流上下文，流ID: {msg}".to_string());
    stream_not_found.insert("en-US".to_string(), "Stream context not found, stream ID: {msg}".to_string());
    stream_not_found.insert("ja-JP".to_string(), "ストリームコンテキストが見つかりません、ストリームID: {msg}".to_string());
    translations.insert("stream_not_found".to_string(), stream_not_found);

    let mut close_stream_failed = HashMap::new();
    close_stream_failed.insert("zh-CN".to_string(), "关闭委托模式双向流失败: {msg}".to_string());
    close_stream_failed.insert("en-US".to_string(), "Failed to close delegated bidirectional stream: {msg}".to_string());
    close_stream_failed.insert("ja-JP".to_string(), "委任モード双方向ストリームのクローズ失敗: {msg}".to_string());
    translations.insert("close_stream_failed".to_string(), close_stream_failed);

    let mut encode_grpc_request_failed = HashMap::new();
    encode_grpc_request_failed.insert("zh-CN".to_string(), "编码 gRPC 请求失败: {msg}".to_string());
    encode_grpc_request_failed.insert("en-US".to_string(), "Failed to encode gRPC request: {msg}".to_string());
    encode_grpc_request_failed.insert("ja-JP".to_string(), "gRPCリクエストエンコード失敗: {msg}".to_string());
    translations.insert("encode_grpc_request_failed".to_string(), encode_grpc_request_failed);

    let mut invalid_uri = HashMap::new();
    invalid_uri.insert("zh-CN".to_string(), "无效的 URI: {msg}".to_string());
    invalid_uri.insert("en-US".to_string(), "Invalid URI: {msg}".to_string());
    invalid_uri.insert("ja-JP".to_string(), "無効なURI: {msg}".to_string());
    translations.insert("invalid_uri".to_string(), invalid_uri);

    let mut build_request_failed = HashMap::new();
    build_request_failed.insert("zh-CN".to_string(), "构建请求失败: {msg}".to_string());
    build_request_failed.insert("en-US".to_string(), "Failed to build request: {msg}".to_string());
    build_request_failed.insert("ja-JP".to_string(), "リクエスト構築失敗: {msg}".to_string());
    translations.insert("build_request_failed".to_string(), build_request_failed);

    let mut serialize_request_failed = HashMap::new();
    serialize_request_failed.insert("zh-CN".to_string(), "序列化请求数据失败: {msg}".to_string());
    serialize_request_failed.insert("en-US".to_string(), "Failed to serialize request data: {msg}".to_string());
    serialize_request_failed.insert("ja-JP".to_string(), "リクエストデータシリアル化失敗: {msg}".to_string());
    translations.insert("serialize_request_failed".to_string(), serialize_request_failed);

    let mut get_connection_failed = HashMap::new();
    get_connection_failed.insert("zh-CN".to_string(), "获取连接失败: {msg}".to_string());
    get_connection_failed.insert("en-US".to_string(), "Failed to get connection: {msg}".to_string());
    get_connection_failed.insert("ja-JP".to_string(), "接続取得失敗: {msg}".to_string());
    translations.insert("get_connection_failed".to_string(), get_connection_failed);

    let mut tcp_connection_failed = HashMap::new();
    tcp_connection_failed.insert("zh-CN".to_string(), "TCP 连接失败: {msg}".to_string());
    tcp_connection_failed.insert("en-US".to_string(), "TCP connection failed: {msg}".to_string());
    tcp_connection_failed.insert("ja-JP".to_string(), "TCP接続失敗: {msg}".to_string());
    translations.insert("tcp_connection_failed".to_string(), tcp_connection_failed);

    let mut tcp_timeout = HashMap::new();
    tcp_timeout.insert("zh-CN".to_string(), "连接超时".to_string());
    tcp_timeout.insert("en-US".to_string(), "Connection timeout".to_string());
    tcp_timeout.insert("ja-JP".to_string(), "接続タイムアウト".to_string());
    translations.insert("tcp_timeout".to_string(), tcp_timeout);

    // 连接和TLS相关错误
    let mut tcp_connection_failed = HashMap::new();
    tcp_connection_failed.insert("zh-CN".to_string(), "TCP 连接失败: {msg}".to_string());
    tcp_connection_failed.insert("en-US".to_string(), "TCP connection failed: {msg}".to_string());
    tcp_connection_failed.insert("ja-JP".to_string(), "TCP接続失敗: {msg}".to_string());
    translations.insert("tcp_connection_failed".to_string(), tcp_connection_failed);

    let mut set_tcp_nodelay_failed = HashMap::new();
    set_tcp_nodelay_failed.insert("zh-CN".to_string(), "设置 TCP_NODELAY 失败: {msg}".to_string());
    set_tcp_nodelay_failed.insert("en-US".to_string(), "Failed to set TCP_NODELAY: {msg}".to_string());
    set_tcp_nodelay_failed.insert("ja-JP".to_string(), "TCP_NODELAY設定失敗: {msg}".to_string());
    translations.insert("set_tcp_nodelay_failed".to_string(), set_tcp_nodelay_failed);

    let mut create_ssl_connector_failed = HashMap::new();
    create_ssl_connector_failed.insert("zh-CN".to_string(), "创建 SSL 连接器失败: {msg}".to_string());
    create_ssl_connector_failed.insert("en-US".to_string(), "Failed to create SSL connector: {msg}".to_string());
    create_ssl_connector_failed.insert("ja-JP".to_string(), "SSLコネクタ作成失敗: {msg}".to_string());
    translations.insert("create_ssl_connector_failed".to_string(), create_ssl_connector_failed);

    let mut parse_ca_cert_failed = HashMap::new();
    parse_ca_cert_failed.insert("zh-CN".to_string(), "解析 CA 证书失败: {msg}".to_string());
    parse_ca_cert_failed.insert("en-US".to_string(), "Failed to parse CA certificate: {msg}".to_string());
    parse_ca_cert_failed.insert("ja-JP".to_string(), "CA証明書解析失敗: {msg}".to_string());
    translations.insert("parse_ca_cert_failed".to_string(), parse_ca_cert_failed);

    let mut add_ca_cert_failed = HashMap::new();
    add_ca_cert_failed.insert("zh-CN".to_string(), "添加 CA 证书失败: {msg}".to_string());
    add_ca_cert_failed.insert("en-US".to_string(), "Failed to add CA certificate: {msg}".to_string());
    add_ca_cert_failed.insert("ja-JP".to_string(), "CA証明書追加失敗: {msg}".to_string());
    translations.insert("add_ca_cert_failed".to_string(), add_ca_cert_failed);

    let mut parse_client_cert_failed = HashMap::new();
    parse_client_cert_failed.insert("zh-CN".to_string(), "解析客户端证书失败: {msg}".to_string());
    parse_client_cert_failed.insert("en-US".to_string(), "Failed to parse client certificate: {msg}".to_string());
    parse_client_cert_failed.insert("ja-JP".to_string(), "クライアント証明書解析失敗: {msg}".to_string());
    translations.insert("parse_client_cert_failed".to_string(), parse_client_cert_failed);

    let mut set_client_cert_failed = HashMap::new();
    set_client_cert_failed.insert("zh-CN".to_string(), "设置客户端证书失败: {msg}".to_string());
    set_client_cert_failed.insert("en-US".to_string(), "Failed to set client certificate: {msg}".to_string());
    set_client_cert_failed.insert("ja-JP".to_string(), "クライアント証明書設定失敗: {msg}".to_string());
    translations.insert("set_client_cert_failed".to_string(), set_client_cert_failed);

    let mut parse_client_key_failed = HashMap::new();
    parse_client_key_failed.insert("zh-CN".to_string(), "解析客户端私钥失败: {msg}".to_string());
    parse_client_key_failed.insert("en-US".to_string(), "Failed to parse client private key: {msg}".to_string());
    parse_client_key_failed.insert("ja-JP".to_string(), "クライアント秘密鍵解析失敗: {msg}".to_string());
    translations.insert("parse_client_key_failed".to_string(), parse_client_key_failed);

    let mut set_client_key_failed = HashMap::new();
    set_client_key_failed.insert("zh-CN".to_string(), "设置客户端私钥失败: {msg}".to_string());
    set_client_key_failed.insert("en-US".to_string(), "Failed to set client private key: {msg}".to_string());
    set_client_key_failed.insert("ja-JP".to_string(), "クライアント秘密鍵設定失敗: {msg}".to_string());
    translations.insert("set_client_key_failed".to_string(), set_client_key_failed);

    let mut set_alpn_failed = HashMap::new();
    set_alpn_failed.insert("zh-CN".to_string(), "设置 ALPN 协议失败: {msg}".to_string());
    set_alpn_failed.insert("en-US".to_string(), "Failed to set ALPN protocol: {msg}".to_string());
    set_alpn_failed.insert("ja-JP".to_string(), "ALPNプロトコル設定失敗: {msg}".to_string());
    translations.insert("set_alpn_failed".to_string(), set_alpn_failed);

    let mut create_ssl_failed = HashMap::new();
    create_ssl_failed.insert("zh-CN".to_string(), "创建 SSL 失败: {msg}".to_string());
    create_ssl_failed.insert("en-US".to_string(), "Failed to create SSL: {msg}".to_string());
    create_ssl_failed.insert("ja-JP".to_string(), "SSL作成失敗: {msg}".to_string());
    translations.insert("create_ssl_failed".to_string(), create_ssl_failed);

    let mut set_sni_failed = HashMap::new();
    set_sni_failed.insert("zh-CN".to_string(), "设置 SNI 主机名失败: {msg}".to_string());
    set_sni_failed.insert("en-US".to_string(), "Failed to set SNI hostname: {msg}".to_string());
    set_sni_failed.insert("ja-JP".to_string(), "SNIホスト名設定失敗: {msg}".to_string());
    translations.insert("set_sni_failed".to_string(), set_sni_failed);

    let mut create_tls_stream_failed = HashMap::new();
    create_tls_stream_failed.insert("zh-CN".to_string(), "创建 TLS 流失败: {msg}".to_string());
    create_tls_stream_failed.insert("en-US".to_string(), "Failed to create TLS stream: {msg}".to_string());
    create_tls_stream_failed.insert("ja-JP".to_string(), "TLSストリーム作成失敗: {msg}".to_string());
    translations.insert("create_tls_stream_failed".to_string(), create_tls_stream_failed);

    let mut tls_handshake_failed = HashMap::new();
    tls_handshake_failed.insert("zh-CN".to_string(), "TLS 握手失败: {msg}".to_string());
    tls_handshake_failed.insert("en-US".to_string(), "TLS handshake failed: {msg}".to_string());
    tls_handshake_failed.insert("ja-JP".to_string(), "TLSハンドシェイク失敗: {msg}".to_string());
    translations.insert("tls_handshake_failed".to_string(), tls_handshake_failed);

    let mut h2_tls_handshake_failed = HashMap::new();
    h2_tls_handshake_failed.insert("zh-CN".to_string(), "HTTP/2 over TLS 握手失败: {msg}".to_string());
    h2_tls_handshake_failed.insert("en-US".to_string(), "HTTP/2 over TLS handshake failed: {msg}".to_string());
    h2_tls_handshake_failed.insert("ja-JP".to_string(), "HTTP/2 over TLSハンドシェイク失敗: {msg}".to_string());
    translations.insert("h2_tls_handshake_failed".to_string(), h2_tls_handshake_failed);

    let mut h2_handshake_failed = HashMap::new();
    h2_handshake_failed.insert("zh-CN".to_string(), "H2 握手失败: {msg}".to_string());
    h2_handshake_failed.insert("en-US".to_string(), "H2 handshake failed: {msg}".to_string());
    h2_handshake_failed.insert("ja-JP".to_string(), "H2ハンドシェイク失敗: {msg}".to_string());
    translations.insert("h2_handshake_failed".to_string(), h2_handshake_failed);

    // GrpcCodec相关错误
    let mut grpc_serialize_failed = HashMap::new();
    grpc_serialize_failed.insert("zh-CN".to_string(), "GrpcCodec 序列化失败: {msg}".to_string());
    grpc_serialize_failed.insert("en-US".to_string(), "GrpcCodec serialization failed: {msg}".to_string());
    grpc_serialize_failed.insert("ja-JP".to_string(), "GrpcCodecシリアル化失敗: {msg}".to_string());
    translations.insert("grpc_serialize_failed".to_string(), grpc_serialize_failed);

    let mut grpc_serialize_close_failed = HashMap::new();
    grpc_serialize_close_failed.insert("zh-CN".to_string(), "GrpcCodec 序列化关闭指令失败: {msg}".to_string());
    grpc_serialize_close_failed.insert("en-US".to_string(), "GrpcCodec failed to serialize close command: {msg}".to_string());
    grpc_serialize_close_failed.insert("ja-JP".to_string(), "GrpcCodecクローズコマンドシリアル化失敗: {msg}".to_string());
    translations.insert("grpc_serialize_close_failed".to_string(), grpc_serialize_close_failed);

    let mut send_close_failed = HashMap::new();
    send_close_failed.insert("zh-CN".to_string(), "发送关闭指令失败: {msg}".to_string());
    send_close_failed.insert("en-US".to_string(), "Failed to send close command: {msg}".to_string());
    send_close_failed.insert("ja-JP".to_string(), "クローズコマンド送信失敗: {msg}".to_string());
    translations.insert("send_close_failed".to_string(), send_close_failed);

    let mut grpc_deserialize_failed = HashMap::new();
    grpc_deserialize_failed.insert("zh-CN".to_string(), "GrpcCodec 反序列化失败: {msg}".to_string());
    grpc_deserialize_failed.insert("en-US".to_string(), "GrpcCodec deserialization failed: {msg}".to_string());
    grpc_deserialize_failed.insert("ja-JP".to_string(), "GrpcCodecデシリアル化失敗: {msg}".to_string());
    translations.insert("grpc_deserialize_failed".to_string(), grpc_deserialize_failed);

    // 其他错误
    let mut lz4_decompress_failed = HashMap::new();
    lz4_decompress_failed.insert("zh-CN".to_string(), "LZ4 解压缩失败: {msg}".to_string());
    lz4_decompress_failed.insert("en-US".to_string(), "LZ4 decompression failed: {msg}".to_string());
    lz4_decompress_failed.insert("ja-JP".to_string(), "LZ4解凍失敗: {msg}".to_string());
    translations.insert("lz4_decompress_failed".to_string(), lz4_decompress_failed);

    let mut read_response_failed = HashMap::new();
    read_response_failed.insert("zh-CN".to_string(), "读取响应体失败: {msg}".to_string());
    read_response_failed.insert("en-US".to_string(), "Failed to read response body: {msg}".to_string());
    read_response_failed.insert("ja-JP".to_string(), "レスポンディ読み取り失敗: {msg}".to_string());
    translations.insert("read_response_failed".to_string(), read_response_failed);

    let mut grpc_http_error = HashMap::new();
    grpc_http_error.insert("zh-CN".to_string(), "gRPC HTTP 错误: {msg}".to_string());
    grpc_http_error.insert("en-US".to_string(), "gRPC HTTP error: {msg}".to_string());
    grpc_http_error.insert("ja-JP".to_string(), "gRPC HTTPエラー: {msg}".to_string());
    translations.insert("grpc_http_error".to_string(), grpc_http_error);

    let mut parse_grpc_frame_failed = HashMap::new();
    parse_grpc_frame_failed.insert("zh-CN".to_string(), "解析 gRPC 帧失败: {msg}".to_string());
    parse_grpc_frame_failed.insert("en-US".to_string(), "Failed to parse gRPC frame: {msg}".to_string());
    parse_grpc_frame_failed.insert("ja-JP".to_string(), "gRPCフレーム解析失敗: {msg}".to_string());
    translations.insert("parse_grpc_frame_failed".to_string(), parse_grpc_frame_failed);

    let mut deserialize_data_type_failed = HashMap::new();
    deserialize_data_type_failed.insert("zh-CN".to_string(), "反序列化最终数据类型失败: {msg}".to_string());
    deserialize_data_type_failed.insert("en-US".to_string(), "Failed to deserialize final data type: {msg}".to_string());
    deserialize_data_type_failed.insert("ja-JP".to_string(), "最終データ型デシリアル化失敗: {msg}".to_string());
    translations.insert("deserialize_data_type_failed".to_string(), deserialize_data_type_failed);

    let mut add_ca_cert_to_store_failed = HashMap::new();
    add_ca_cert_to_store_failed.insert("zh-CN".to_string(), "添加 CA 证书到存储失败: {msg}".to_string());
    add_ca_cert_to_store_failed.insert("en-US".to_string(), "Failed to add CA certificate to store: {msg}".to_string());
    add_ca_cert_to_store_failed.insert("ja-JP".to_string(), "CA証明書をストアに追加失敗: {msg}".to_string());
    translations.insert("add_ca_cert_to_store_failed".to_string(), add_ca_cert_to_store_failed);

    let mut set_default_cert_path_failed = HashMap::new();
    set_default_cert_path_failed.insert("zh-CN".to_string(), "设置默认证书路径失败: {msg}".to_string());
    set_default_cert_path_failed.insert("en-US".to_string(), "Failed to set default certificate path: {msg}".to_string());
    set_default_cert_path_failed.insert("ja-JP".to_string(), "デフォルト証明書パス設定失敗: {msg}".to_string());
    translations.insert("set_default_cert_path_failed".to_string(), set_default_cert_path_failed);

    let mut parse_private_key_failed = HashMap::new();
    parse_private_key_failed.insert("zh-CN".to_string(), "解析私钥失败: {msg}".to_string());
    parse_private_key_failed.insert("en-US".to_string(), "Failed to parse private key: {msg}".to_string());
    parse_private_key_failed.insert("ja-JP".to_string(), "秘密鍵解析失敗: {msg}".to_string());
    translations.insert("parse_private_key_failed".to_string(), parse_private_key_failed);

    let mut set_private_key_failed = HashMap::new();
    set_private_key_failed.insert("zh-CN".to_string(), "设置私钥失败: {msg}".to_string());
    set_private_key_failed.insert("en-US".to_string(), "Failed to set private key: {msg}".to_string());
    set_private_key_failed.insert("ja-JP".to_string(), "秘密鍵設定失敗: {msg}".to_string());
    translations.insert("set_private_key_failed".to_string(), set_private_key_failed);

    let mut key_cert_match_check_failed = HashMap::new();
    key_cert_match_check_failed.insert("zh-CN".to_string(), "私钥证书匹配检查失败: {msg}".to_string());
    key_cert_match_check_failed.insert("en-US".to_string(), "Private key certificate match check failed: {msg}".to_string());
    key_cert_match_check_failed.insert("ja-JP".to_string(), "秘密鍵証明書一致チェック失敗: {msg}".to_string());
    translations.insert("key_cert_match_check_failed".to_string(), key_cert_match_check_failed);

    let mut invalid_user_agent = HashMap::new();
    invalid_user_agent.insert("zh-CN".to_string(), "无效的用户代理: {msg}".to_string());
    invalid_user_agent.insert("en-US".to_string(), "Invalid user agent: {msg}".to_string());
    invalid_user_agent.insert("ja-JP".to_string(), "無効なユーザーエージェント: {msg}".to_string());
    translations.insert("invalid_user_agent".to_string(), invalid_user_agent);

    let mut request_failed = HashMap::new();
    request_failed.insert("zh-CN".to_string(), "请求失败: {msg}".to_string());
    request_failed.insert("en-US".to_string(), "Request failed: {msg}".to_string());
    request_failed.insert("ja-JP".to_string(), "リクエスト失敗: {msg}".to_string());
    translations.insert("request_failed".to_string(), request_failed);

    // build_client_stream_request_failed - 构建客户端流请求失败
    let mut build_client_stream_request_failed = HashMap::new();
    build_client_stream_request_failed.insert("zh-CN".to_string(), "构建客户端流请求失败: {msg}".to_string());
    build_client_stream_request_failed.insert("en-US".to_string(), "Build client stream request failed: {msg}".to_string());
    build_client_stream_request_failed.insert("ja-JP".to_string(), "クライアントストリームリクエストの構築に失敗: {msg}".to_string());
    translations.insert("build_client_stream_request_failed".to_string(), build_client_stream_request_failed);

    // send_client_stream_request_failed - 发送客户端流请求失败
    let mut send_client_stream_request_failed = HashMap::new();
    send_client_stream_request_failed.insert("zh-CN".to_string(), "发送客户端流请求失败: {msg}".to_string());
    send_client_stream_request_failed.insert("en-US".to_string(), "Send client stream request failed: {msg}".to_string());
    send_client_stream_request_failed.insert("ja-JP".to_string(), "クライアントストリームリクエストの送信に失敗: {msg}".to_string());
    translations.insert("send_client_stream_request_failed".to_string(), send_client_stream_request_failed);

    // receive_client_stream_response_failed - 接收客户端流响应失败
    let mut receive_client_stream_response_failed = HashMap::new();
    receive_client_stream_response_failed.insert("zh-CN".to_string(), "接收客户端流响应失败: {msg}".to_string());
    receive_client_stream_response_failed.insert("en-US".to_string(), "Receive client stream response failed: {msg}".to_string());
    receive_client_stream_response_failed.insert("ja-JP".to_string(), "クライアントストリームレスポンスの受信に失敗: {msg}".to_string());
    translations.insert("receive_client_stream_response_failed".to_string(), receive_client_stream_response_failed);

    // receive_response_data_failed - 接收响应数据失败
    let mut receive_response_data_failed = HashMap::new();
    receive_response_data_failed.insert("zh-CN".to_string(), "接收响应数据失败: {msg}".to_string());
    receive_response_data_failed.insert("en-US".to_string(), "Receive response data failed: {msg}".to_string());
    receive_response_data_failed.insert("ja-JP".to_string(), "レスポンスデータの受信に失敗: {msg}".to_string());
    translations.insert("receive_response_data_failed".to_string(), receive_response_data_failed);

    // decode_response_data_failed - 解码响应数据失败
    let mut decode_response_data_failed = HashMap::new();
    decode_response_data_failed.insert("zh-CN".to_string(), "解码响应数据失败: {msg}".to_string());
    decode_response_data_failed.insert("en-US".to_string(), "Decode response data failed: {msg}".to_string());
    decode_response_data_failed.insert("ja-JP".to_string(), "レスポンスデータのデコードに失敗: {msg}".to_string());
    translations.insert("decode_response_data_failed".to_string(), decode_response_data_failed);

    // decode_grpc_response_frame_failed - 解码 gRPC 响应帧失败
    let mut decode_grpc_response_frame_failed = HashMap::new();
    decode_grpc_response_frame_failed.insert("zh-CN".to_string(), "解码 gRPC 响应帧失败: {msg}".to_string());
    decode_grpc_response_frame_failed.insert("en-US".to_string(), "Decode gRPC response frame failed: {msg}".to_string());
    decode_grpc_response_frame_failed.insert("ja-JP".to_string(), "gRPCレスポンスフレームのデコードに失敗: {msg}".to_string());
    translations.insert("decode_grpc_response_frame_failed".to_string(), decode_grpc_response_frame_failed);

    // invalid_request_uri - 无效的请求 URI
    let mut invalid_request_uri = HashMap::new();
    invalid_request_uri.insert("zh-CN".to_string(), "无效的请求 URI: {msg}".to_string());
    invalid_request_uri.insert("en-US".to_string(), "Invalid request URI: {msg}".to_string());
    invalid_request_uri.insert("ja-JP".to_string(), "無効なリクエストURI: {msg}".to_string());
    translations.insert("invalid_request_uri".to_string(), invalid_request_uri);

    // http_error - HTTP 错误
    let mut http_error = HashMap::new();
    http_error.insert("zh-CN".to_string(), "HTTP 错误: {msg}".to_string());
    http_error.insert("en-US".to_string(), "HTTP error: {msg}".to_string());
    http_error.insert("ja-JP".to_string(), "HTTPエラー: {msg}".to_string());
    translations.insert("http_error".to_string(), http_error);

    // build_bidirectional_stream_request_failed - 构建双向流请求失败
    let mut build_bidirectional_stream_request_failed = HashMap::new();
    build_bidirectional_stream_request_failed.insert("zh-CN".to_string(), "构建双向流请求失败: {msg}".to_string());
    build_bidirectional_stream_request_failed.insert("en-US".to_string(), "Build bidirectional stream request failed: {msg}".to_string());
    build_bidirectional_stream_request_failed.insert("ja-JP".to_string(), "双方向ストリームリクエストの構築に失敗: {msg}".to_string());
    translations.insert("build_bidirectional_stream_request_failed".to_string(), build_bidirectional_stream_request_failed);

    // send_bidirectional_stream_request_failed - 发送双向流请求失败
    let mut send_bidirectional_stream_request_failed = HashMap::new();
    send_bidirectional_stream_request_failed.insert("zh-CN".to_string(), "发送双向流请求失败: {msg}".to_string());
    send_bidirectional_stream_request_failed.insert("en-US".to_string(), "Send bidirectional stream request failed: {msg}".to_string());
    send_bidirectional_stream_request_failed.insert("ja-JP".to_string(), "双方向ストリームリクエストの送信に失敗: {msg}".to_string());
    translations.insert("send_bidirectional_stream_request_failed".to_string(), send_bidirectional_stream_request_failed);

    // receive_bidirectional_stream_response_failed - 接收双向流响应失败
    let mut receive_bidirectional_stream_response_failed = HashMap::new();
    receive_bidirectional_stream_response_failed.insert("zh-CN".to_string(), "接收双向流响应失败: {msg}".to_string());
    receive_bidirectional_stream_response_failed.insert("en-US".to_string(), "Receive bidirectional stream response failed: {msg}".to_string());
    receive_bidirectional_stream_response_failed.insert("ja-JP".to_string(), "双方向ストリームレスポンスの受信に失敗: {msg}".to_string());
    translations.insert("receive_bidirectional_stream_response_failed".to_string(), receive_bidirectional_stream_response_failed);

    // delegate_deserialize_actual_message_failed - 委托模式反序列化实际消息失败
    let mut delegate_deserialize_actual_message_failed = HashMap::new();
    delegate_deserialize_actual_message_failed.insert("zh-CN".to_string(), "反序列化实际消息失败: {msg}".to_string());
    delegate_deserialize_actual_message_failed.insert("en-US".to_string(), "Deserialize actual message failed: {msg}".to_string());
    delegate_deserialize_actual_message_failed.insert("ja-JP".to_string(), "実際のメッセージのデシリアライズに失敗: {msg}".to_string());
    translations.insert("delegate_deserialize_actual_message_failed".to_string(), delegate_deserialize_actual_message_failed);

    // delegate_grpc_stream_message_deserialize_failed - 委托模式 GrpcStreamMessage 反序列化失败
    let mut delegate_grpc_stream_message_deserialize_failed = HashMap::new();
    delegate_grpc_stream_message_deserialize_failed.insert("zh-CN".to_string(), "GrpcStreamMessage 反序列化失败: {msg}".to_string());
    delegate_grpc_stream_message_deserialize_failed.insert("en-US".to_string(), "GrpcStreamMessage deserialize failed: {msg}".to_string());
    delegate_grpc_stream_message_deserialize_failed.insert("ja-JP".to_string(), "GrpcStreamMessage デシリアライズ失敗: {msg}".to_string());
    translations.insert("delegate_grpc_stream_message_deserialize_failed".to_string(), delegate_grpc_stream_message_deserialize_failed);

    // receive_data_failed - 接收数据失败
    let mut receive_data_failed = HashMap::new();
    receive_data_failed.insert("zh-CN".to_string(), "接收数据失败: {msg}".to_string());
    receive_data_failed.insert("en-US".to_string(), "Receive data failed: {msg}".to_string());
    receive_data_failed.insert("ja-JP".to_string(), "データ受信失敗: {msg}".to_string());
    translations.insert("receive_data_failed".to_string(), receive_data_failed);

    // invalid_user_agent - 无效的用户代理
    let mut invalid_user_agent_msg = HashMap::new();
    invalid_user_agent_msg.insert("zh-CN".to_string(), "无效的用户代理: {msg}".to_string());
    invalid_user_agent_msg.insert("en-US".to_string(), "Invalid user agent: {msg}".to_string());
    invalid_user_agent_msg.insert("ja-JP".to_string(), "無効なユーザーエージェント: {msg}".to_string());
    translations.insert("invalid_user_agent_msg".to_string(), invalid_user_agent_msg);

    // deserialize_grpc_stream_message_failed - 反序列化 gRPC 流消息失败
    let mut deserialize_grpc_stream_message_failed = HashMap::new();
    deserialize_grpc_stream_message_failed.insert("zh-CN".to_string(), "反序列化 gRPC 流消息失败: {msg}".to_string());
    deserialize_grpc_stream_message_failed.insert("en-US".to_string(), "Deserialize gRPC stream message failed: {msg}".to_string());
    deserialize_grpc_stream_message_failed.insert("ja-JP".to_string(), "gRPCストリームメッセージのデシリアライズ失敗: {msg}".to_string());
    translations.insert("deserialize_grpc_stream_message_failed".to_string(), deserialize_grpc_stream_message_failed);

    // deserialize_data_field_failed - 反序列化数据字段失败
    let mut deserialize_data_field_failed = HashMap::new();
    deserialize_data_field_failed.insert("zh-CN".to_string(), "反序列化数据字段失败: {msg}".to_string());
    deserialize_data_field_failed.insert("en-US".to_string(), "Deserialize data field failed: {msg}".to_string());
    deserialize_data_field_failed.insert("ja-JP".to_string(), "データフィールドのデシリアライズ失敗: {msg}".to_string());
    translations.insert("deserialize_data_field_failed".to_string(), deserialize_data_field_failed);

    // receive_stream_data_error - 接收流数据错误
    let mut receive_stream_data_error = HashMap::new();
    receive_stream_data_error.insert("zh-CN".to_string(), "接收流数据错误: {msg}".to_string());
    receive_stream_data_error.insert("en-US".to_string(), "Receive stream data error: {msg}".to_string());
    receive_stream_data_error.insert("ja-JP".to_string(), "ストリームデータ受信エラー: {msg}".to_string());
    translations.insert("receive_stream_data_error".to_string(), receive_stream_data_error);

    // grpc_error_with_status - gRPC 错误 (状态码: xxx): xxx
    let mut grpc_error_with_status = HashMap::new();
    grpc_error_with_status.insert("zh-CN".to_string(), "gRPC 错误 (状态码: {status}): {message}".to_string());
    grpc_error_with_status.insert("en-US".to_string(), "gRPC error (status: {status}): {message}".to_string());
    grpc_error_with_status.insert("ja-JP".to_string(), "gRPCエラー (ステータス: {status}): {message}".to_string());
    translations.insert("grpc_error_with_status".to_string(), grpc_error_with_status);

    // grpc_message_serialize_failed - gRPC 消息序列化失败
    let mut grpc_message_serialize_failed = HashMap::new();
    grpc_message_serialize_failed.insert("zh-CN".to_string(), "gRPC 消息序列化失败: {msg}".to_string());
    grpc_message_serialize_failed.insert("en-US".to_string(), "gRPC message serialize failed: {msg}".to_string());
    grpc_message_serialize_failed.insert("ja-JP".to_string(), "gRPCメッセージシリアライズ失敗: {msg}".to_string());
    translations.insert("grpc_message_serialize_failed".to_string(), grpc_message_serialize_failed);

    // grpc_message_deserialize_failed - gRPC 消息反序列化失败
    let mut grpc_message_deserialize_failed = HashMap::new();
    grpc_message_deserialize_failed.insert("zh-CN".to_string(), "gRPC 消息反序列化失败: {msg}".to_string());
    grpc_message_deserialize_failed.insert("en-US".to_string(), "gRPC message deserialize failed: {msg}".to_string());
    grpc_message_deserialize_failed.insert("ja-JP".to_string(), "gRPCメッセージデシリアライズ失敗: {msg}".to_string());
    translations.insert("grpc_message_deserialize_failed".to_string(), grpc_message_deserialize_failed);

    // encode_grpc_response_failed - 编码 gRPC 响应失败
    let mut encode_grpc_response_failed = HashMap::new();
    encode_grpc_response_failed.insert("zh-CN".to_string(), "编码 gRPC 响应失败: {msg}".to_string());
    encode_grpc_response_failed.insert("en-US".to_string(), "Encode gRPC response failed: {msg}".to_string());
    encode_grpc_response_failed.insert("ja-JP".to_string(), "gRPCレスポンスエンコード失敗: {msg}".to_string());
    translations.insert("encode_grpc_response_failed".to_string(), encode_grpc_response_failed);

    // method_not_implemented - 方法未实现
    let mut method_not_implemented = HashMap::new();
    method_not_implemented.insert("zh-CN".to_string(), "方法未实现: {msg}".to_string());
    method_not_implemented.insert("en-US".to_string(), "Method not implemented: {msg}".to_string());
    method_not_implemented.insert("ja-JP".to_string(), "メソッド未実装: {msg}".to_string());
    translations.insert("method_not_implemented".to_string(), method_not_implemented);

    // release_flow_control_capacity_failed - 释放流控制容量失败
    let mut release_flow_control_capacity_failed = HashMap::new();
    release_flow_control_capacity_failed.insert("zh-CN".to_string(), "释放流控制容量失败: {msg}".to_string());
    release_flow_control_capacity_failed.insert("en-US".to_string(), "Release flow control capacity failed: {msg}".to_string());
    release_flow_control_capacity_failed.insert("ja-JP".to_string(), "フロー制御容量解放失敗: {msg}".to_string());
    translations.insert("release_flow_control_capacity_failed".to_string(), release_flow_control_capacity_failed);

    // read_request_body_failed - 读取请求体失败
    let mut read_request_body_failed = HashMap::new();
    read_request_body_failed.insert("zh-CN".to_string(), "读取请求体失败: {msg}".to_string());
    read_request_body_failed.insert("en-US".to_string(), "Read request body failed: {msg}".to_string());
    read_request_body_failed.insert("ja-JP".to_string(), "リクエストボディ読み取り失敗: {msg}".to_string());
    translations.insert("read_request_body_failed".to_string(), read_request_body_failed);

    // parse_grpc_frame_failed_server - 解析 gRPC 帧失败
    let mut parse_grpc_frame_failed_server = HashMap::new();
    parse_grpc_frame_failed_server.insert("zh-CN".to_string(), "解析 gRPC 帧失败: {msg}".to_string());
    parse_grpc_frame_failed_server.insert("en-US".to_string(), "Parse gRPC frame failed: {msg}".to_string());
    parse_grpc_frame_failed_server.insert("ja-JP".to_string(), "gRPCフレーム解析失敗: {msg}".to_string());
    translations.insert("parse_grpc_frame_failed_server".to_string(), parse_grpc_frame_failed_server);

    // encode_grpc_stream_message_failed - 编码 gRPC 流消息失败
    let mut encode_grpc_stream_message_failed = HashMap::new();
    encode_grpc_stream_message_failed.insert("zh-CN".to_string(), "编码 gRPC 流消息失败: {msg}".to_string());
    encode_grpc_stream_message_failed.insert("en-US".to_string(), "Encode gRPC stream message failed: {msg}".to_string());
    encode_grpc_stream_message_failed.insert("ja-JP".to_string(), "gRPCストリームメッセージエンコード失敗: {msg}".to_string());
    translations.insert("encode_grpc_stream_message_failed".to_string(), encode_grpc_stream_message_failed);

    // serialize_data_failed - 序列化数据失败
    let mut serialize_data_failed = HashMap::new();
    serialize_data_failed.insert("zh-CN".to_string(), "序列化数据失败: {msg}".to_string());
    serialize_data_failed.insert("en-US".to_string(), "Serialize data failed: {msg}".to_string());
    serialize_data_failed.insert("ja-JP".to_string(), "データシリアライズ失敗: {msg}".to_string());
    translations.insert("serialize_data_failed".to_string(), serialize_data_failed);

    // read_stream_data_failed - 读取流数据失败
    let mut read_stream_data_failed = HashMap::new();
    read_stream_data_failed.insert("zh-CN".to_string(), "读取流数据失败: {msg}".to_string());
    read_stream_data_failed.insert("en-US".to_string(), "Read stream data failed: {msg}".to_string());
    read_stream_data_failed.insert("ja-JP".to_string(), "ストリームデータ読み取り失敗: {msg}".to_string());
    translations.insert("read_stream_data_failed".to_string(), read_stream_data_failed);

    // encode_grpc_stream_message_wrapper_failed - 编码 GrpcStreamMessage 失败
    let mut encode_grpc_stream_message_wrapper_failed = HashMap::new();
    encode_grpc_stream_message_wrapper_failed.insert("zh-CN".to_string(), "编码 GrpcStreamMessage 失败: {msg}".to_string());
    encode_grpc_stream_message_wrapper_failed.insert("en-US".to_string(), "Encode GrpcStreamMessage failed: {msg}".to_string());
    encode_grpc_stream_message_wrapper_failed.insert("ja-JP".to_string(), "GrpcStreamMessageエンコード失敗: {msg}".to_string());
    translations.insert("encode_grpc_stream_message_wrapper_failed".to_string(), encode_grpc_stream_message_wrapper_failed);

    // alpn_config_failed - ALPN 配置失败
    let mut alpn_config_failed = HashMap::new();
    alpn_config_failed.insert("zh-CN".to_string(), "ALPN 配置失败: {msg}".to_string());
    alpn_config_failed.insert("en-US".to_string(), "ALPN config failed: {msg}".to_string());
    alpn_config_failed.insert("ja-JP".to_string(), "ALPN設定失敗: {msg}".to_string());
    translations.insert("alpn_config_failed".to_string(), alpn_config_failed);

    // h2_tcp_connection_timeout - H2 TCP 连接超时
    let mut h2_tcp_connection_timeout = HashMap::new();
    h2_tcp_connection_timeout.insert("zh-CN".to_string(), "H2 TCP 连接超时: {msg}".to_string());
    h2_tcp_connection_timeout.insert("en-US".to_string(), "H2 TCP connection timeout: {msg}".to_string());
    h2_tcp_connection_timeout.insert("ja-JP".to_string(), "H2 TCP接続タイムアウト: {msg}".to_string());
    translations.insert("h2_tcp_connection_timeout".to_string(), h2_tcp_connection_timeout);

    // h2_tcp_connection_failed - H2 TCP 连接失败
    let mut h2_tcp_connection_failed = HashMap::new();
    h2_tcp_connection_failed.insert("zh-CN".to_string(), "H2 TCP 连接失败: {msg}".to_string());
    h2_tcp_connection_failed.insert("en-US".to_string(), "H2 TCP connection failed: {msg}".to_string());
    h2_tcp_connection_failed.insert("ja-JP".to_string(), "H2 TCP接続失敗: {msg}".to_string());
    translations.insert("h2_tcp_connection_failed".to_string(), h2_tcp_connection_failed);

    // create_ssl_failed - 创建 SSL 失败
    let mut create_ssl_failed_http = HashMap::new();
    create_ssl_failed_http.insert("zh-CN".to_string(), "创建 SSL 失败: {msg}".to_string());
    create_ssl_failed_http.insert("en-US".to_string(), "Create SSL failed: {msg}".to_string());
    create_ssl_failed_http.insert("ja-JP".to_string(), "SSL作成失敗: {msg}".to_string());
    translations.insert("create_ssl_failed_http".to_string(), create_ssl_failed_http);

    // set_sni_hostname_failed - 设置 SNI 主机名失败
    let mut set_sni_hostname_failed = HashMap::new();
    set_sni_hostname_failed.insert("zh-CN".to_string(), "设置 SNI 主机名失败: {msg}".to_string());
    set_sni_hostname_failed.insert("en-US".to_string(), "Set SNI hostname failed: {msg}".to_string());
    set_sni_hostname_failed.insert("ja-JP".to_string(), "SNIホスト名設定失敗: {msg}".to_string());
    translations.insert("set_sni_hostname_failed".to_string(), set_sni_hostname_failed);

    // set_default_hostname_failed - 设置默认主机名失败
    let mut set_default_hostname_failed = HashMap::new();
    set_default_hostname_failed.insert("zh-CN".to_string(), "设置默认主机名失败: {msg}".to_string());
    set_default_hostname_failed.insert("en-US".to_string(), "Set default hostname failed: {msg}".to_string());
    set_default_hostname_failed.insert("ja-JP".to_string(), "デフォルトホスト名設定失敗: {msg}".to_string());
    translations.insert("set_default_hostname_failed".to_string(), set_default_hostname_failed);

    // set_hostname_failed - 设置主机名失败
    let mut set_hostname_failed = HashMap::new();
    set_hostname_failed.insert("zh-CN".to_string(), "设置主机名失败: {msg}".to_string());
    set_hostname_failed.insert("en-US".to_string(), "Set hostname failed: {msg}".to_string());
    set_hostname_failed.insert("ja-JP".to_string(), "ホスト名設定失敗: {msg}".to_string());
    translations.insert("set_hostname_failed".to_string(), set_hostname_failed);

    // create_tls_stream_failed - 创建 TLS 流失败
    let mut create_tls_stream_failed = HashMap::new();
    create_tls_stream_failed.insert("zh-CN".to_string(), "创建 TLS 流失败: {msg}".to_string());
    create_tls_stream_failed.insert("en-US".to_string(), "Create TLS stream failed: {msg}".to_string());
    create_tls_stream_failed.insert("ja-JP".to_string(), "TLSストリーム作成失敗: {msg}".to_string());
    translations.insert("create_tls_stream_failed".to_string(), create_tls_stream_failed);

    // tls_handshake_failed - TLS 握手失败
    let mut tls_handshake_failed_http = HashMap::new();
    tls_handshake_failed_http.insert("zh-CN".to_string(), "TLS 握手失败: {msg}".to_string());
    tls_handshake_failed_http.insert("en-US".to_string(), "TLS handshake failed: {msg}".to_string());
    tls_handshake_failed_http.insert("ja-JP".to_string(), "TLSハンドシェイク失敗: {msg}".to_string());
    translations.insert("tls_handshake_failed_http".to_string(), tls_handshake_failed_http);

    // http2_over_tls_handshake_failed - HTTP/2 over TLS 握手失败
    let mut http2_over_tls_handshake_failed = HashMap::new();
    http2_over_tls_handshake_failed.insert("zh-CN".to_string(), "HTTP/2 over TLS 握手失败: {msg}".to_string());
    http2_over_tls_handshake_failed.insert("en-US".to_string(), "HTTP/2 over TLS handshake failed: {msg}".to_string());
    http2_over_tls_handshake_failed.insert("ja-JP".to_string(), "HTTP/2 over TLSハンドシェイク失敗: {msg}".to_string());
    translations.insert("http2_over_tls_handshake_failed".to_string(), http2_over_tls_handshake_failed);

    // h2c_handshake_failed - H2C 握手失败
    let mut h2c_handshake_failed = HashMap::new();
    h2c_handshake_failed.insert("zh-CN".to_string(), "H2C 握手失败: {msg}".to_string());
    h2c_handshake_failed.insert("en-US".to_string(), "H2C handshake failed: {msg}".to_string());
    h2c_handshake_failed.insert("ja-JP".to_string(), "H2Cハンドシェイク失敗: {msg}".to_string());
    translations.insert("h2c_handshake_failed".to_string(), h2c_handshake_failed);

    // h2_read_response_body_failed - H2 读取响应体失败
    let mut h2_read_response_body_failed = HashMap::new();
    h2_read_response_body_failed.insert("zh-CN".to_string(), "H2 读取响应体失败: {msg}".to_string());
    h2_read_response_body_failed.insert("en-US".to_string(), "H2 read response body failed: {msg}".to_string());
    h2_read_response_body_failed.insert("ja-JP".to_string(), "H2レスポンスボディ読み取り失敗: {msg}".to_string());
    translations.insert("h2_read_response_body_failed".to_string(), h2_read_response_body_failed);

    // build_response_failed - 构建响应失败
    let mut build_response_failed = HashMap::new();
    build_response_failed.insert("zh-CN".to_string(), "构建响应失败: {msg}".to_string());
    build_response_failed.insert("en-US".to_string(), "Build response failed: {msg}".to_string());
    build_response_failed.insert("ja-JP".to_string(), "レスポンス構築失敗: {msg}".to_string());
    translations.insert("build_response_failed".to_string(), build_response_failed);

    // build_h2_request_failed - 构建 H2 请求失败
    let mut build_h2_request_failed = HashMap::new();
    build_h2_request_failed.insert("zh-CN".to_string(), "构建 H2 请求失败: {msg}".to_string());
    build_h2_request_failed.insert("en-US".to_string(), "Build H2 request failed: {msg}".to_string());
    build_h2_request_failed.insert("ja-JP".to_string(), "H2リクエスト構築失敗: {msg}".to_string());
    translations.insert("build_h2_request_failed".to_string(), build_h2_request_failed);

    // h2_send_request_failed - H2 发送请求失败
    let mut h2_send_request_failed = HashMap::new();
    h2_send_request_failed.insert("zh-CN".to_string(), "H2 发送请求失败: {msg}".to_string());
    h2_send_request_failed.insert("en-US".to_string(), "H2 send request failed: {msg}".to_string());
    h2_send_request_failed.insert("ja-JP".to_string(), "H2リクエスト送信失敗: {msg}".to_string());
    translations.insert("h2_send_request_failed".to_string(), h2_send_request_failed);

    // read_request_body_failed_http - 读取请求体失败
    let mut read_request_body_failed_http = HashMap::new();
    read_request_body_failed_http.insert("zh-CN".to_string(), "读取请求体失败: {msg}".to_string());
    read_request_body_failed_http.insert("en-US".to_string(), "Read request body failed: {msg}".to_string());
    read_request_body_failed_http.insert("ja-JP".to_string(), "リクエストボディ読み取り失敗: {msg}".to_string());
    translations.insert("read_request_body_failed_http".to_string(), read_request_body_failed_http);

    // h2_send_data_failed - H2 发送数据失败
    let mut h2_send_data_failed = HashMap::new();
    h2_send_data_failed.insert("zh-CN".to_string(), "H2 发送数据失败: {msg}".to_string());
    h2_send_data_failed.insert("en-US".to_string(), "H2 send data failed: {msg}".to_string());
    h2_send_data_failed.insert("ja-JP".to_string(), "H2データ送信失敗: {msg}".to_string());
    translations.insert("h2_send_data_failed".to_string(), h2_send_data_failed);

    // h2_send_empty_data_failed - H2 发送空数据失败
    let mut h2_send_empty_data_failed = HashMap::new();
    h2_send_empty_data_failed.insert("zh-CN".to_string(), "H2 发送空数据失败: {msg}".to_string());
    h2_send_empty_data_failed.insert("en-US".to_string(), "H2 send empty data failed: {msg}".to_string());
    h2_send_empty_data_failed.insert("ja-JP".to_string(), "H2空データ送信失敗: {msg}".to_string());
    translations.insert("h2_send_empty_data_failed".to_string(), h2_send_empty_data_failed);

    // h2_response_timeout - H2 响应超时
    let mut h2_response_timeout = HashMap::new();
    h2_response_timeout.insert("zh-CN".to_string(), "H2 响应超时: {msg}".to_string());
    h2_response_timeout.insert("en-US".to_string(), "H2 response timeout: {msg}".to_string());
    h2_response_timeout.insert("ja-JP".to_string(), "H2レスポンスタイムアウト: {msg}".to_string());
    translations.insert("h2_response_timeout".to_string(), h2_response_timeout);

    // h2_receive_response_failed - H2 接收响应失败
    let mut h2_receive_response_failed = HashMap::new();
    h2_receive_response_failed.insert("zh-CN".to_string(), "H2 接收响应失败: {msg}".to_string());
    h2_receive_response_failed.insert("en-US".to_string(), "H2 receive response failed: {msg}".to_string());
    h2_receive_response_failed.insert("ja-JP".to_string(), "H2レスポンス受信失敗: {msg}".to_string());
    translations.insert("h2_receive_response_failed".to_string(), h2_receive_response_failed);

    // unable_to_read_client_cert_file - 无法读取客户端证书文件
    let mut unable_to_read_client_cert_file = HashMap::new();
    unable_to_read_client_cert_file.insert("zh-CN".to_string(), "无法读取客户端证书文件 {path}: {msg}".to_string());
    unable_to_read_client_cert_file.insert("en-US".to_string(), "Unable to read client certificate file {path}: {msg}".to_string());
    unable_to_read_client_cert_file.insert("ja-JP".to_string(), "クライアント証明書ファイル {path} を読み取れません: {msg}".to_string());
    translations.insert("unable_to_read_client_cert_file".to_string(), unable_to_read_client_cert_file);

    // unable_to_read_client_key_file - 无法读取客户端私钥文件
    let mut unable_to_read_client_key_file = HashMap::new();
    unable_to_read_client_key_file.insert("zh-CN".to_string(), "无法读取客户端私钥文件 {path}: {msg}".to_string());
    unable_to_read_client_key_file.insert("en-US".to_string(), "Unable to read client private key file {path}: {msg}".to_string());
    unable_to_read_client_key_file.insert("ja-JP".to_string(), "クライアント秘密鍵ファイル {path} を読み取れません: {msg}".to_string());
    translations.insert("unable_to_read_client_key_file".to_string(), unable_to_read_client_key_file);

    // parse_client_cert_failed_config - 解析客户端证书失败
    let mut parse_client_cert_failed_config = HashMap::new();
    parse_client_cert_failed_config.insert("zh-CN".to_string(), "解析客户端证书失败: {msg}".to_string());
    parse_client_cert_failed_config.insert("en-US".to_string(), "Parse client certificate failed: {msg}".to_string());
    parse_client_cert_failed_config.insert("ja-JP".to_string(), "クライアント証明書の解析失敗: {msg}".to_string());
    translations.insert("parse_client_cert_failed_config".to_string(), parse_client_cert_failed_config);

    // parse_client_key_failed_config - 解析客户端私钥失败
    let mut parse_client_key_failed_config = HashMap::new();
    parse_client_key_failed_config.insert("zh-CN".to_string(), "解析客户端私钥失败: {msg}".to_string());
    parse_client_key_failed_config.insert("en-US".to_string(), "Parse client private key failed: {msg}".to_string());
    parse_client_key_failed_config.insert("ja-JP".to_string(), "クライアント秘密鍵の解析失敗: {msg}".to_string());
    translations.insert("parse_client_key_failed_config".to_string(), parse_client_key_failed_config);

    // unable_to_read_ca_cert_file - 无法读取 CA 证书文件
    let mut unable_to_read_ca_cert_file = HashMap::new();
    unable_to_read_ca_cert_file.insert("zh-CN".to_string(), "无法读取 CA 证书文件 {path}: {msg}".to_string());
    unable_to_read_ca_cert_file.insert("en-US".to_string(), "Unable to read CA certificate file {path}: {msg}".to_string());
    unable_to_read_ca_cert_file.insert("ja-JP".to_string(), "CA証明書ファイル {path} を読み取れません: {msg}".to_string());
    translations.insert("unable_to_read_ca_cert_file".to_string(), unable_to_read_ca_cert_file);

    // sse_connection_failed - SSE连接失败
    let mut sse_connection_failed = HashMap::new();
    sse_connection_failed.insert("zh-CN".to_string(), "SSE连接失败: {msg}".to_string());
    sse_connection_failed.insert("en-US".to_string(), "SSE connection failed: {msg}".to_string());
    sse_connection_failed.insert("ja-JP".to_string(), "SSE接続失敗: {msg}".to_string());
    translations.insert("sse_connection_failed".to_string(), sse_connection_failed);

    // read_sse_stream_failed - 读取SSE流失败
    let mut read_sse_stream_failed = HashMap::new();
    read_sse_stream_failed.insert("zh-CN".to_string(), "读取SSE流失败: {msg}".to_string());
    read_sse_stream_failed.insert("en-US".to_string(), "Read SSE stream failed: {msg}".to_string());
    read_sse_stream_failed.insert("ja-JP".to_string(), "SSEストリーム読み取り失敗: {msg}".to_string());
    translations.insert("read_sse_stream_failed".to_string(), read_sse_stream_failed);

    // invalid_request_header_name - 无效的请求头名
    let mut invalid_request_header_name = HashMap::new();
    invalid_request_header_name.insert("zh-CN".to_string(), "无效的请求头名: {msg}".to_string());
    invalid_request_header_name.insert("en-US".to_string(), "Invalid request header name: {msg}".to_string());
    invalid_request_header_name.insert("ja-JP".to_string(), "無効なリクエストヘッダー名: {msg}".to_string());
    translations.insert("invalid_request_header_name".to_string(), invalid_request_header_name);

    // invalid_request_header_value - 无效的请求头值
    let mut invalid_request_header_value = HashMap::new();
    invalid_request_header_value.insert("zh-CN".to_string(), "无效的请求头值: {msg}".to_string());
    invalid_request_header_value.insert("en-US".to_string(), "Invalid request header value: {msg}".to_string());
    invalid_request_header_value.insert("ja-JP".to_string(), "無効なリクエストヘッダー値: {msg}".to_string());
    translations.insert("invalid_request_header_value".to_string(), invalid_request_header_value);

    // build_http_client_failed - 构建HTTP客户端失败
    let mut build_http_client_failed = HashMap::new();
    build_http_client_failed.insert("zh-CN".to_string(), "构建HTTP客户端失败: {msg}".to_string());
    build_http_client_failed.insert("en-US".to_string(), "Build HTTP client failed: {msg}".to_string());
    build_http_client_failed.insert("ja-JP".to_string(), "HTTPクライアント構築失敗: {msg}".to_string());
    translations.insert("build_http_client_failed".to_string(), build_http_client_failed);

    // response_body_not_utf8 - 响应体不是有效的UTF-8
    let mut response_body_not_utf8 = HashMap::new();
    response_body_not_utf8.insert("zh-CN".to_string(), "响应体不是有效的UTF-8: {msg}".to_string());
    response_body_not_utf8.insert("en-US".to_string(), "Response body is not valid UTF-8: {msg}".to_string());
    response_body_not_utf8.insert("ja-JP".to_string(), "レスポンスボディは有効なUTF-8ではありません: {msg}".to_string());
    translations.insert("response_body_not_utf8".to_string(), response_body_not_utf8);

    // json_parse_failed - JSON解析失败
    let mut json_parse_failed = HashMap::new();
    json_parse_failed.insert("zh-CN".to_string(), "JSON解析失败: {msg}".to_string());
    json_parse_failed.insert("en-US".to_string(), "JSON parse failed: {msg}".to_string());
    json_parse_failed.insert("ja-JP".to_string(), "JSON解析失敗: {msg}".to_string());
    translations.insert("json_parse_failed".to_string(), json_parse_failed);

    register_translations(translations);

    // 设置语言 - 优先使用系统语言，fallback到中文
    use rat_embed_lang::{get_language_from_env, set_language};
    let system_lang = get_language_from_env();
    // 如果系统语言是我们支持的语言之一，就使用它，否则使用中文
    match system_lang.as_str() {
        "zh-CN" | "en-US" | "ja-JP" => set_language(&system_lang),
        _ => set_language("zh-CN"),
    }
}