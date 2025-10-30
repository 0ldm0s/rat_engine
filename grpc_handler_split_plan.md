# grpc_handler.rs 精准拆分定位计划 (基于实际文件读取)

## 导入部分 (跳过)
- **行号**: 1-27
- **验证**: 确认27行是最后一个导入
- **处理**: 跳过不复制

## 精确结构定位

### 1. 数据类型定义 (types.rs)
- **GrpcTask**: 行 29-71 (包含enum定义、Debug实现、Send/Sync实现)
- **GrpcConnectionType**: 行 73-82 (精确: 从pub enum GrpcConnectionType {到})
- **GrpcConnection**: 行 86-109 (精确: 从pub struct GrpcConnection {到})
- **精确范围**: 29-109行

### 2. 连接管理器 (connection_manager.rs)
- **GrpcConnectionManager**: 行 113-315 (精确: 从pub struct GrpcConnectionManager {到})
- **精确范围**: 113-315行

### 3. 服务注册表 (service_registry.rs)
- **GrpcServiceRegistry**: 行 318-885 (精确: 从pub struct GrpcServiceRegistry {到})
- **精确范围**: 318-885行

### 4. 处理器接口 (handler_traits.rs)
- **UnaryHandler**: 行 888-895 (精确: 从pub trait UnaryHandler: Send + Sync {到})
- **ServerStreamHandler**: 行 898-905 (精确: 从pub trait ServerStreamHandler: Send + Sync {到})
- **TypedServerStreamHandler**: 行 908-918 (精确: 从pub trait TypedServerStreamHandler<T>: Send + Sync到})
- **TypedServerStreamAdapter**: 行 921-976 (精确: 从pub struct TypedServerStreamAdapter<T, H> {到})
- **ClientStreamHandler**: 行 979-986 (精确: 从pub trait ClientStreamHandler: Send + Sync {到})
- **BidirectionalHandler**: 行 989-996 (精确: 从pub trait BidirectionalHandler: Send + Sync {到})
- **精确范围**: 888-996行

### 5. 请求处理器模块 (拆分为6个文件)

#### 5a. 请求处理器核心 (request_handler_core.rs)
- **GrpcRequestHandler 结构体和核心方法**: 行 999-1186
- **包含**: new(), handle_request(), handle_request_lockfree(), handle_request_traditional()
- **精确范围**: 999-1186行

#### 5b. 一元请求处理器 (unary_request_handler.rs)
- **handle_unary_request**: 行 1188-1210
- **精确范围**: 1188-1210行

#### 5c. 服务端流处理器 (server_stream_handler.rs)
- **handle_server_stream_request**: 行 1212-1319
- **精确范围**: 1212-1319行

#### 5d. 客户端流处理器 (client_stream_handler.rs)
- **handle_client_stream_request**: 行 1321-1364
- **精确范围**: 1321-1364行

#### 5e. 双向流处理器 (bidirectional_request_handler.rs)
- **handle_bidirectional_request**: 行 1366-1447
- **精确范围**: 1366-1447行

#### 5f. 请求处理工具方法 (request_utils.rs)
- **工具方法**: 行 1449-1703
- **包含**: extract_grpc_method, create_grpc_context, read_grpc_request, decode_grpc_request,
         create_grpc_request_stream, encode_grpc_message, send_grpc_response, send_grpc_error,
         send_grpc_error_to_stream, send_grpc_status
- **精确范围**: 1449-1703行

#### 5g. 请求流实现 (request_stream.rs)
- **GrpcRequestStream**: 行 1705-1896
- **精确范围**: 1705-1896行

#### 5h. 默认实现 (service_registry_default.rs)
- **GrpcServiceRegistry Default**: 行 1898-1902
- **精确范围**: 1898-1902行

## 验证过的sed命令
```bash
# 数据类型 (GrpcTask: 29-71, GrpcConnectionType: 73-82, GrpcConnection: 86-109)
sed -n '29,109p' src/server/grpc_handler.rs > src/server/grpc_handler/types.rs

# 连接管理器
sed -n '113,315p' src/server/grpc_handler.rs > src/server/grpc_handler/connection_manager.rs

# 服务注册表
sed -n '318,885p' src/server/grpc_handler.rs > src/server/grpc_handler/service_registry.rs

# 处理器接口
sed -n '888,996p' src/server/grpc_handler.rs > src/server/grpc_handler/handler_traits.rs

# 请求处理器模块
sed -n '999,1186p' src/server/grpc_handler.rs > src/server/grpc_handler/request_handler_core.rs
sed -n '1188,1210p' src/server/grpc_handler.rs > src/server/grpc_handler/unary_request_handler.rs
sed -n '1212,1319p' src/server/grpc_handler.rs > src/server/grpc_handler/server_stream_handler.rs
sed -n '1321,1364p' src/server/grpc_handler.rs > src/server/grpc_handler/client_stream_handler.rs
sed -n '1366,1447p' src/server/grpc_handler.rs > src/server/grpc_handler/bidirectional_request_handler.rs
sed -n '1449,1703p' src/server/grpc_handler.rs > src/server/grpc_handler/request_utils.rs
sed -n '1705,1896p' src/server/grpc_handler.rs > src/server/grpc_handler/request_stream.rs
sed -n '1898,1902p' src/server/grpc_handler.rs > src/server/grpc_handler/service_registry_default.rs
```
