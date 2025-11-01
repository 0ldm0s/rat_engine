// 条件导入 Python API
#[cfg(feature = "python")]
use crate::python_api::client::GrpcUnaryHandler;

#[cfg(feature = "python")]
impl RatGrpcClient {
    /// 使用委托模式发送一元 gRPC 请求
    /// 
    /// 采用类似双向流的委托架构，让连接池统一管理一元请求连接
    /// 用户只需要实现处理器接口，不需要直接管理连接和响应处理
    /// 
    /// # 参数
    /// * `uri` - 服务器 URI
    /// * `service` - 服务名称
    /// * `method` - 方法名称
    /// * `request_data` - 请求数据（强类型）
    /// * `handler` - 一元请求处理器
    /// * `metadata` - 可选的元数据
    /// 
    /// # 返回
    /// 返回请求ID，用于后续管理
    /// 
    /// # 示例
    /// ```rust
    /// let request_id = client.call_unary_delegated_with_uri(
    ///     "http://127.0.0.1:50051",
    ///     "user.UserService",
    ///     "GetUser", 
    ///     user_request,
    ///     Arc::new(UserHandler::new()),
    ///     None
    /// ).await?;
    /// ```
    #[cfg(feature = "python")]
    pub async fn call_unary_delegated_with_uri<T, H>(
        &self,
        uri: &str,
        service: &str,
        method: &str,
        request_data: T,
        handler: Arc<H>,
        metadata: Option<HashMap<String, String>>,
    ) -> RatResult<u64>
    where
        T: Serialize + bincode::Encode + Send + Sync,
        H: crate::python_api::client::GrpcUnaryHandler<ResponseData = Vec<u8>> + 'static,
    {
        self.call_unary_delegated_with_uri_impl(uri, service, method, request_data, handler, metadata).await
    }

    #[cfg(not(feature = "python"))]
    pub async fn call_unary_delegated_with_uri<T, H>(
        &self,
        uri: &str,
        service: &str,
        method: &str,
        request_data: T,
        handler: Arc<H>,
        metadata: Option<HashMap<String, String>>,
    ) -> RatResult<u64>
    where
        T: Serialize + bincode::Encode + Send + Sync,
        H: Send + Sync + 'static,
    {
        self.call_unary_delegated_with_uri_impl(uri, service, method, request_data, handler, metadata).await
    }

    // Python 特性启用时的实现
    #[cfg(feature = "python")]
    async fn call_unary_delegated_with_uri_impl<T, H>(
        &self,
        uri: &str,
        service: &str,
        method: &str,
        request_data: T,
        handler: Arc<H>,
        metadata: Option<HashMap<String, String>>,
    ) -> RatResult<u64>
    where
        T: Serialize + bincode::Encode + Send + Sync,
        H: crate::python_api::client::GrpcUnaryHandler<ResponseData = Vec<u8>> + 'static,
    {
        let request_id = self.request_id_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        
        info!("🔗 创建委托模式一元请求: {}/{}, 请求ID: {}", service, method, request_id);
        
        // 解析 URI
        let parsed_uri = uri.parse::<Uri>()
            .map_err(|e| RatError::RequestError(rat_embed_lang::tf("invalid_uri", &[("msg", &e.to_string())])))?;
        
        // 1. 从连接池获取连接
        let connection = self.connection_pool.get_connection(&parsed_uri).await
            .map_err(|e| RatError::NetworkError(rat_embed_lang::tf("get_connection_failed", &[("msg", &e.to_string())])))?;

        // 2. 直接使用原始请求数据（避免双重序列化）
        let grpc_request = GrpcRequest {
            id: request_id,
            method: format!("{}/{}", service, method),
            data: request_data, // 直接使用原始数据，不进行额外序列化
            metadata: metadata.unwrap_or_default(),
        };

        // 3. 编码 gRPC 消息
        let grpc_message = GrpcCodec::encode_frame(&grpc_request)
            .map_err(|e| RatError::SerializationError(rat_embed_lang::tf("encode_grpc_request_failed", &[("msg", &e.to_string())])))?;

        // 4. 构建 HTTP 请求
        let path = format!("/{}/{}", service, method);
        let full_uri = format!("{}{}", uri.trim_end_matches('/'), path);
        
        let request_uri = full_uri
            .parse::<Uri>()
            .map_err(|e| RatError::RequestError(rat_embed_lang::tf("invalid_request_uri", &[("msg", &e.to_string())])))?;

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/grpc+bincode"));
        headers.insert(USER_AGENT, HeaderValue::from_str(&self.user_agent)
            .map_err(|e| RatError::RequestError(rat_embed_lang::tf("invalid_user_agent", &[("msg", &e.to_string())])));

        // 5. 创建请求上下文（仅在启用 python 特性时）
        #[cfg(feature = "python")]
        let context = crate::python_api::client::GrpcUnaryContext::new(
            request_id,
            service.to_string(),
            method.to_string(),
            uri.to_string(),
            grpc_request.metadata.clone(),
        );

        // 6. 启动异步请求处理任务
        let handler_clone = handler.clone();
        let client_clone = self.clone();
        let connection_id = connection.connection_id.clone();
        
        tokio::spawn(async move {
            // 通知处理器请求开始
            #[cfg(feature = "python")]
            {
                if let Err(e) = handler_clone.on_request_start(&context).await {
                    error!("❌ 一元请求处理器启动失败 (请求ID: {}): {}", request_id, e);
                    let _ = handler_clone.on_error(&context, e).await;
                    return;
                }
            }

            // 发送 HTTP 请求
            let mut request_builder = Request::builder()
                .method(Method::POST)
                .uri(request_uri);
            
            // 添加 headers
            for (key, value) in headers.iter() {
                request_builder = request_builder.header(key, value);
            }
            
            let request = request_builder
                .body(Full::new(Bytes::from(grpc_message)))
                .map_err(|e| RatError::RequestError(rat_embed_lang::tf("build_request_failed", &[("msg", &e.to_string())])));

            let request = match request {
                Ok(req) => req,
                Err(e) => {
                    error!("❌ 构建一元请求失败 (请求ID: {}): {}", request_id, e);
                    #[cfg(feature = "python")]
                    {
                        let _ = handler_clone.on_error(&context, e.to_string()).await;
                    }
                    return;
                }
            };

            // 发送请求并处理响应
            match client_clone.send_request(request).await {
                Ok((status, headers, body)) => {
                    if status.is_success() {
                        // 解析 gRPC 响应
                        match client_clone.parse_grpc_message(&body) {
                            Ok(response_data) => {
                                let grpc_response = response_data;
                                
                                // 通知处理器响应接收和完成
                                #[cfg(feature = "python")]
                                {
                                    if let Err(e) = handler_clone.on_response_received(grpc_response, &context).await {
                                        error!("❌ 一元请求响应处理失败 (请求ID: {}): {}", request_id, e);
                                        let _ = handler_clone.on_error(&context, e.to_string()).await;
                                        return;
                                    }
                                    
                                    // 通知处理器请求完成
                                    let _ = handler_clone.on_completed(&context).await;
                                }
                            }
                            Err(e) => {
                                error!("❌ 解析一元响应失败 (请求ID: {}): {}", request_id, e);
                                #[cfg(feature = "python")]
                                {
                                    let _ = handler_clone.on_error(&context, e.to_string()).await;
                                }
                            }
                        }
                    } else {
                        let error = RatError::NetworkError(rat_embed_lang::tf("http_error", &[("msg", &status.to_string())]));
                        error!("❌ 一元请求 HTTP 错误 (请求ID: {}): {}", request_id, error);
                        #[cfg(feature = "python")]
                        {
                            let _ = handler_clone.on_error(&context, error.to_string()).await;
                        }
                    }
                }
                Err(e) => {
                    error!("❌ 发送一元请求失败 (请求ID: {}): {}", request_id, e);
                    #[cfg(feature = "python")]
                    {
                        let _ = handler_clone.on_error(&context, e.to_string()).await;
                    }
                }
            }
        });

        info!("✅ 委托模式一元请求 {} 已启动", request_id);
        Ok(request_id)
    }

    // Python 特性未启用时的简化实现
    #[cfg(not(feature = "python"))]
    async fn call_unary_delegated_with_uri_impl<T, H>(
        &self,
        uri: &str,
        service: &str,
        method: &str,
        request_data: T,
        _handler: Arc<H>,
        metadata: Option<HashMap<String, String>>,
    ) -> RatResult<u64>
    where
        T: Serialize + bincode::Encode + Send + Sync,
        H: Send + Sync + 'static,
    {
        let request_id = self.request_id_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        
        info!("🔗 创建委托模式一元请求: {}/{}, 请求ID: {}", service, method, request_id);
        
        // 解析 URI
        let parsed_uri = uri.parse::<Uri>()
            .map_err(|e| RatError::RequestError(rat_embed_lang::tf("invalid_uri", &[("msg", &e.to_string())])))?;
        
        // 1. 从连接池获取连接
        let connection = self.connection_pool.get_connection(&parsed_uri).await
            .map_err(|e| RatError::NetworkError(rat_embed_lang::tf("get_connection_failed", &[("msg", &e.to_string())])))?;

        // 2. 直接使用原始请求数据（避免双重序列化）
        let grpc_request = GrpcRequest {
            id: request_id,
            method: format!("{}/{}", service, method),
            data: request_data, // 直接使用原始数据，不进行额外序列化
            metadata: metadata.unwrap_or_default(),
        };

        // 3. 编码 gRPC 消息
        let grpc_message = GrpcCodec::encode_frame(&grpc_request)
            .map_err(|e| RatError::SerializationError(rat_embed_lang::tf("encode_grpc_request_failed", &[("msg", &e.to_string())])))?;

        // 4. 构建 HTTP 请求
        let path = format!("/{}/{}", service, method);
        let full_uri = format!("{}{}", uri.trim_end_matches('/'), path);
        
        let request_uri = full_uri
            .parse::<Uri>()
            .map_err(|e| RatError::RequestError(rat_embed_lang::tf("invalid_request_uri", &[("msg", &e.to_string())])))?;

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/grpc+bincode"));
        headers.insert(USER_AGENT, HeaderValue::from_str(&self.user_agent)
            .map_err(|e| RatError::RequestError(rat_embed_lang::tf("invalid_user_agent", &[("msg", &e.to_string())])));        

        // 5. 启动异步请求处理任务（简化版本，无 handler 回调）
        let client_clone = self.clone();
        let connection_id = connection.connection_id.clone();
        
        tokio::spawn(async move {
            // 发送 HTTP 请求
            let mut request_builder = Request::builder()
                .method(Method::POST)
                .uri(request_uri);
            
            // 添加 headers
            for (key, value) in headers.iter() {
                request_builder = request_builder.header(key, value);
            }
            
            let request = request_builder
                .body(Full::new(Bytes::from(grpc_message)))
                .map_err(|e| RatError::RequestError(rat_embed_lang::tf("build_request_failed", &[("msg", &e.to_string())])));

            let request = match request {
                Ok(req) => req,
                Err(e) => {
                    error!("❌ 构建一元请求失败 (请求ID: {}): {}", request_id, e);
                    return;
                }
            };

            // 发送请求并处理响应
            match client_clone.send_request(request).await {
                Ok((status, _headers, body)) => {
                    if status.is_success() {
                        // 解析 gRPC 响应
                        match client_clone.parse_grpc_message(&body) {
                            Ok(_response_data) => {
                                info!("✅ 一元请求成功完成 (请求ID: {})", request_id);
                            }
                            Err(e) => {
                                error!("❌ 解析一元响应失败 (请求ID: {}): {}", request_id, e);
                            }
                        }
                    } else {
                        let error = RatError::NetworkError(rat_embed_lang::tf("http_error", &[("msg", &status.to_string())]));
                        error!("❌ 一元请求 HTTP 错误 (请求ID: {}): {}", request_id, error);
                    }
                }
                Err(e) => {
                    error!("❌ 发送一元请求失败 (请求ID: {}): {}", request_id, e);
                }
            }
        });

        info!("✅ 委托模式一元请求 {} 已启动", request_id);
        Ok(request_id)
    }
}
