# CertManager 拆分计划

## 文件概况
- **文件路径**: `src/server/cert_manager.rs`
- **总行数**: 1451 行

## 精准定位

### 1. CertManagerConfig (25-76行)
**包含注释**: 行 25-76
```bash
sed -n '25,76p' src/server/cert_manager.rs > src/server/cert_manager/config.rs
```

### 2. CertManagerConfig Default 实现 (78-107行)
```bash
sed -n '78,107p' src/server/cert_manager.rs >> src/server/cert_manager/config.rs
```

### 3. CertificateInfo (108-127行)
**包含注释**: 行 108-127
```bash
sed -n '108,127p' src/server/cert_manager.rs > src/server/cert_manager/certificate_info.rs
```

### 4. CertificateManager 结构体 (130-148行)
```bash
sed -n '130,148p' src/server/cert_manager.rs > src/server/cert_manager/manager.rs
```

### 5. CertificateManager 核心实现 (150-1256行)
```bash
sed -n '150,1256p' src/server/cert_manager.rs > src/server/cert_manager/manager_core.rs
```

### 6. CertificateManager Drop 实现 (1260-1275行)
```bash
sed -n '1260,1275p' src/server/cert_manager.rs >> src/server/cert_manager/manager.rs
```

### 7. CertManagerBuilder (1279-1282行)
```bash
sed -n '1279,1282p' src/server/cert_manager.rs > src/server/cert_manager/builder.rs
```

### 8. CertManagerBuilder 实现 (1283-1451行)
```bash
sed -n '1283,1451p' src/server/cert_manager.rs >> src/server/cert_manager/builder.rs
```

## 模块文件结构

### 目标文件
```
src/server/cert_manager/
├── mod.rs
├── config.rs              # 27-106行 (80行)
├── certificate_info.rs    # 110-127行 (18行)
├── manager.rs             # 130-148行 + 1260-1275行 (34行)
├── manager_core.rs         # 150-1256行 (1107行)
└── builder.rs             # 1279-1451行 (173行)
```

### mod.rs 内容
```rust
pub mod config;
pub mod certificate_info;
pub mod manager;
pub mod manager_core;
pub mod builder;

pub use config::CertManagerConfig;
pub use certificate_info::CertificateInfo;
pub use manager::CertificateManager;
pub use builder::CertManagerBuilder;
```

## 行数分布
- **config.rs**: 80行
- **certificate_info.rs**: 18行
- **manager.rs**: 34行
- **manager_core.rs**: 1107行
- **builder.rs**: 173行
- **最大文件**: manager_core.rs (1107行)

## 进一步细分方案 (manager_core.rs 拆分为5个模块)

### 1. 基础模块 (core.rs) - 150-213行 (64行)
**包含内容**: new(), get_config(), 基础初始化逻辑
```bash
sed -n '150,213p' src/server/cert_manager.rs > src/server/cert_manager/core.rs
```

### 2. 开发模式证书生成 (dev_cert.rs) - 214-512行 (299行)
**包含内容**:
- initialize_mtls_certificates (214-244)
- generate_development_certificate (245-291)
- should_regenerate_certificate (292-309)
- load_existing_development_certificate (310-363)
- create_new_development_certificate (364-459)
- load_production_certificate (460-512)
```bash
sed -n '214,512p' src/server/cert_manager.rs > src/server/cert_manager/dev_cert.rs
```

### 3. 证书验证与信息 (cert_validation.rs) - 513-707行 (195行)
**包含内容**:
- validate_certificate_algorithm (513-553)
- parse_certificate_info (554-620)
- get_* 系列方法 (621-683)
- is_certificate_expiring (684-693)
- set_force_rotation/get_force_rotation (694-707)
```bash
sed -n '513,707p' src/server/cert_manager.rs > src/server/cert_manager/cert_validation.rs
```

### 4. ACME集成 (acme_integration.rs) - 708-859行 (152行)
**包含内容**:
- handle_acme_certificate (708-758)
- load_acme_certificate (759-795)
- issue_new_acme_certificate (796-858)
```bash
sed -n '708,859p' src/server/cert_manager.rs > src/server/cert_manager/acme_integration.rs
```

### 5. 客户端证书与SSL配置 (client_cert.rs) - 859-1110行 (252行)
**包含内容**:
- generate_client_certificate (859-947)
- load_client_certificate (948-984)
- configure_alpn_protocols (985-995)
- recreate_server_config_with_alpn (996-1074)
- recreate_server_config_with_mtls (1075-1110)
```bash
sed -n '859,1110p' src/server/cert_manager.rs > src/server/cert_manager/client_cert.rs
```

### 6. 证书自动刷新 (auto_refresh.rs) - 1111-1256行 (146行)
**包含内容**:
- start_certificate_refresh_task (1111-1143)
- check_and_refresh_certificates_static (1144-1201)
- is_certificate_expiring_at_path_static (1202-1212)
- generate_development_certificate_at_path_static (1213-1256)
```bash
sed -n '1111,1256p' src/server/cert_manager.rs > src/server/cert_manager/auto_refresh.rs
```

## 优化后文件结构
```
src/server/cert_manager/
├── mod.rs                  # 模块导出
├── config.rs               # 25-76 + 78-107行 (83行)
├── certificate_info.rs     # 108-127行 (20行)
├── manager.rs              # 130-148行 + 1260-1275行 (34行)
├── core.rs                 # 150-213行 (64行)
├── dev_cert.rs             # 214-512行 (299行)
├── cert_validation.rs      # 513-707行 (195行)
├── acme_integration.rs     # 708-859行 (152行)
├── client_cert.rs          # 859-1110行 (252行)
├── auto_refresh.rs         # 1111-1256行 (146行)
└── builder.rs              # 1279-1451行 (173行)
```

## 优化后行数分布
- **config.rs**: 83行 (25-76 + 78-107)
- **certificate_info.rs**: 20行 (108-127)
- **manager.rs**: 34行 (130-148 + 1260-1275)
- **core.rs**: 64行 (150-213)
- **dev_cert.rs**: 299行 (214-512)
- **cert_validation.rs**: 195行 (513-707)
- **acme_integration.rs**: 152行 (708-859)
- **client_cert.rs**: 252行 (859-1110)
- **auto_refresh.rs**: 146行 (1111-1256)
- **builder.rs**: 173行 (1279-1451)
- **最大文件**: dev_cert.rs (299行)

## 最终mod.rs内容
```rust
pub mod config;
pub mod certificate_info;
pub mod manager;
pub mod core;
pub mod dev_cert;
pub mod cert_validation;
#[cfg(feature = "acme")]
pub mod acme_integration;
pub mod client_cert;
pub mod auto_refresh;
pub mod builder;

pub use config::CertManagerConfig;
pub use certificate_info::CertificateInfo;
pub use manager::CertificateManager;
pub use builder::CertManagerBuilder;
```