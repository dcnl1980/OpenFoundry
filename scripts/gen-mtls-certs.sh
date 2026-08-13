#!/usr/bin/env bash
# Generate a local CA, gateway client cert, and localhost server cert for mTLS.
set -euo pipefail

OUT="${1:-./certs}"
mkdir -p "$OUT"

if ! command -v openssl >/dev/null 2>&1; then
  echo "openssl is required" >&2
  exit 1
fi

openssl req -x509 -newkey rsa:2048 -nodes -days 3650 \
  -keyout "$OUT/ca.key" -out "$OUT/ca.pem" \
  -subj "/CN=OpenFoundry Dev CA"

openssl req -newkey rsa:2048 -nodes \
  -keyout "$OUT/server.key" -out "$OUT/server.csr" \
  -subj "/CN=localhost"
openssl x509 -req -in "$OUT/server.csr" -CA "$OUT/ca.pem" -CAkey "$OUT/ca.key" \
  -CAcreateserial -out "$OUT/server.pem" -days 825 \
  -extfile <(printf "subjectAltName=DNS:localhost,IP:127.0.0.1")

openssl req -newkey rsa:2048 -nodes \
  -keyout "$OUT/client.key" -out "$OUT/client.csr" \
  -subj "/CN=openfoundry-gateway"
openssl x509 -req -in "$OUT/client.csr" -CA "$OUT/ca.pem" -CAkey "$OUT/ca.key" \
  -CAcreateserial -out "$OUT/client.pem" -days 825

rm -f "$OUT/server.csr" "$OUT/client.csr" "$OUT/ca.srl"

cat <<EOF
Wrote mTLS material to $OUT

Services (server identity, require gateway client):
  TLS_CERT_PATH=$OUT/server.pem
  TLS_KEY_PATH=$OUT/server.key
  TLS_CA_PATH=$OUT/ca.pem

Gateway (client identity, trust the same CA):
  TLS_CERT_PATH=$OUT/client.pem
  TLS_KEY_PATH=$OUT/client.key
  TLS_CA_PATH=$OUT/ca.pem
EOF
