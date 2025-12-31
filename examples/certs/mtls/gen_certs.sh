#!/bin/bash
set -e

echo "=== 生成 mTLS 测试证书 ==="
echo ""
echo "说明："
echo "  - CA 证书: 用于服务端验证客户端证书"
echo "  - 客户端证书: 由 CA 签发"
echo "  - 客户端证书链: client-cert.pem (客户端证书 + CA 证书)"
echo ""
echo "文件用途："
echo "  - ca-cert.pem: 服务端使用，验证客户端证书"
echo "  - ca-key.pem: CA 私钥（服务端使用，签名客户端证书）"
echo "  - client-cert.pem: 仅客户端证书"
echo "  - client-cert-chain.pem: 客户端证书 + CA 证书（推荐）"
echo "  - client-key.pem: 客户端私钥"
echo ""

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

# 6. 生成客户端证书链（客户端证书 + CA 证书）
echo "6. 生成客户端证书链 (client-cert-chain.pem)..."
cat client-cert.pem ca-cert.pem > client-cert-chain.pem

# 7. 生成仅客户端证书（不包含 CA）
echo "7. 生成仅客户端证书 (client-cert-only.pem)..."
cp client-cert.pem client-cert-only.pem

# 8. 清理临时文件
echo "8. 清理临时文件..."
rm -f client.csr

# 9. 显示生成的证书
echo ""
echo "=== 生成的证书文件 ==="
ls -lh

# 10. 验证客户端证书
echo ""
echo "=== 验证客户端证书 ==="
openssl verify -CAfile ca-cert.pem client-cert.pem
openssl verify -CAfile ca-cert.pem client-cert-chain.pem

echo ""
echo "✅ 证书生成完成！"
echo ""
echo "使用方法："
echo "  服务端配置:"
echo "    .with_ca(\"examples/certs/mtls/ca-cert.pem\")  # 使用 CA 验证客户端"
echo ""
echo "  客户端配置:"
echo "    .with_client_certs_and_ca("
echo "      \"examples/certs/mtls/client-cert-chain.pem\",  # 证书链"
echo "      \"examples/certs/mtls/client-key.pem\","
echo "      None  # mTLS 模式下跳过服务器证书验证"
echo "    )"
