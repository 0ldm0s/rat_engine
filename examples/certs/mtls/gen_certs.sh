#!/bin/bash
set -e

echo "=== 生成 mTLS 测试证书 ==="

# 1. 生成 CA 私钥
echo "1. 生成 CA 私钥..."
openssl genpkey -algorithm RSA -out ca-key.pem -pkeyopt rsa_keygen_bits:2048

# 2. 生成 CA 证书
echo "2. 生成 CA 证书..."
openssl req -new -x509 -days 365 -key ca-key.pem -out ca-cert.pem -subj "/C=CN/ST=Beijing/L=Beijing/O=Test CA/CN=Test CA"

# 3. 生成客户端私钥
echo "3. 生成客户端私钥..."
openssl genpkey -algorithm RSA -out client-key.pem -pkeyopt rsa_keygen_bits:2048

# 4. 生成客户端证书签名请求 (CSR)
echo "4. 生成客户端 CSR..."
openssl req -new -key client-key.pem -out client.csr -subj "/C=CN/ST=Beijing/L=Beijing/O=Test Client/CN=testclient"

# 5. 用 CA 签名客户端证书
echo "5. 签名客户端证书..."
openssl x509 -req -days 365 -in client.csr -CA ca-cert.pem -CAkey ca-key.pem -CAcreateserial -out client-cert.pem

# 6. 显示生成的证书
echo ""
echo "=== 生成的证书文件 ==="
ls -la

# 7. 验证客户端证书
echo ""
echo "=== 验证客户端证书 ==="
openssl verify -CAfile ca-cert.pem client-cert.pem

echo ""
echo "✅ 证书生成完成！"
