#!/usr/bin/env bash
# ANFAGENT-30 M3 端到端冒烟：邀请码注册 → 待审（不入网）→ 放行 → 入网
# 前提：easytier-web（embed）已运行在 API 11211 / config server 22020；et.db 已初始化
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
. "$SCRIPT_DIR/_anf_env.sh"

WEB="${WEB:-${ANF_WEB_BASE:-http://127.0.0.1:11211}}"
CONFIG_SERVER="${CONFIG_SERVER:-${ANF_CONFIG_SERVER:-udp://127.0.0.1:22020/admin}}"
DB="${DB:-/tmp/anf-m3.db}"
WEB_BIN="${WEB_BIN:-/home/anf-et/anf-easytier/target/debug/easytier-web}"
CORE_BIN="${CORE_BIN:-/opt/easytier/easytier-core}"
NETWORK_NAME="${ANF_NETWORK_NAME:-anf-m3}"
ADMIN_USER="${ANF_ADMIN_USER:-admin}"
ADMIN_PASSWORD="${ANF_ADMIN_PASSWORD:-}"
if [ -z "$ADMIN_PASSWORD" ]; then
  echo "缺少配置：请在仓库根 .env 设置 ANF_ADMIN_PASSWORD（参考 .env.example）" >&2
  exit 1
fi
ADMIN_MD5="$(printf %s "$ADMIN_PASSWORD" | md5sum | awk '{print $1}')"
TMP="$(mktemp -d)"
CJ="$TMP/cookies.txt"

ADMIN_MACHINE="$(cat /proc/sys/kernel/random/uuid)"
DEVICE_MACHINE="$(cat /proc/sys/kernel/random/uuid)"

echo "== 1. 初始化 DB 并绑定管理员设备 =="
rm -f "$DB"
"$WEB_BIN" --db "$DB" admin-bind --machine-id "$ADMIN_MACHINE" --username "$ADMIN_USER" --create-user-password "$ADMIN_PASSWORD"

echo "== 2. 启动 easytier-web（embed） =="
ANF_PEER_ARG=()
if [ -n "${ANF_CENTER_PEER_URL:-}" ]; then
  ANF_PEER_ARG=(--anf-center-peer-url "$ANF_CENTER_PEER_URL")
fi
"$WEB_BIN" --db "$DB" --config-server-port 22020 --api-server-port 11211 \
  --anf-network-name "$NETWORK_NAME" \
  --anf-network-secret "${ANF_NETWORK_SECRET:-}" \
  "${ANF_PEER_ARG[@]}" \
  >"$TMP/web.log" 2>&1 &
WEB_PID=$!
trap 'kill $WEB_PID 2>/dev/null || true; rm -rf "$TMP"' EXIT
sleep 2

echo "== 3. 管理员登录 =="
LOGIN_RESP="$(curl -s -c "$CJ" -b "$CJ" -X POST "$WEB/api/v1/auth/login" \
  -H 'Content-Type: application/json' \
  -d "{\"username\":\"$ADMIN_USER\",\"password\":\"$ADMIN_MD5\"}")"
echo "login: $LOGIN_RESP"

echo "== 4. 创建邀请码 =="
CODE="$(curl -s -c "$CJ" -b "$CJ" -X POST "$WEB/api/v1/invites" \
  -H 'Content-Type: application/json' -d '{"max_uses":1}' | python3 -c 'import sys,json;print(json.load(sys.stdin)["code"])')"
echo "邀请码: $CODE"

echo "== 5. 设备注册（待审） =="
curl -s -X POST "$WEB/api/v1/devices/register" \
  -H 'Content-Type: application/json' \
  -d "{\"invite_code\":\"$CODE\",\"machine_id\":\"$DEVICE_MACHINE\"}" | tee "$TMP/register.json"
echo

echo "== 6. 待审设备运行客户端（应不入网，无 tun 配置） =="
sudo -n true 2>/dev/null || { echo "需要 sudo 权限跑客户端"; exit 1; }
sudo -n env PATH="$PATH" "$CORE_BIN" -w "$CONFIG_SERVER" \
  --machine-id "$DEVICE_MACHINE" --network-name anf-m3 \
  --no-listener \
  >"$TMP/client-pending.log" 2>&1 &
CLIENT_PID=$!
sleep 6
echo "-- 客户端日志（待审阶段）--"
tail -5 "$TMP/client-pending.log" || true

echo "== 7. 管理员放行 =="
DEV_ID="$(python3 -c 'import sys,json;print(json.load(open(sys.argv[1]))["id"])' "$TMP/register.json")"
curl -s -c "$CJ" -b "$CJ" -X POST "$WEB/api/v1/devices/$DEV_ID/approve" >/dev/null
echo "设备 $DEV_ID 已放行"

echo "== 8. 创建网络并分配（触发配置生成/下发） =="
NET="$(curl -s -c "$CJ" -b "$CJ" -X POST "$WEB/api/v1/networks" \
  -H 'Content-Type: application/json' -d '{"name":"办公网","cidr":"10.99.0.0/24"}')"
echo "net: $NET"
NET_ID="$(python3 -c 'import sys,json;print(json.loads(sys.argv[1])["id"])' "$NET")"
curl -s -c "$CJ" -b "$CJ" -X PATCH "$WEB/api/v1/devices/$DEV_ID" \
  -H 'Content-Type: application/json' \
  -d "{\"tags\":[\"办公\"],\"networks\":[\"$NET_ID\"]}" >/dev/null
echo "设备 $DEV_ID 已分配网络 $NET_ID（tag: 办公）"

echo "== 9. 客户端应获得配置并启动实例 =="
sleep 8
tail -8 "$TMP/client-pending.log" || true
echo
if grep -q "instance .* started" "$TMP/client-pending.log"; then
  echo "== 完成：客户端已收到托管配置并启动实例 =="
else
  echo "== 完成（客户端日志未见实例启动，请检查 web 日志） =="
  tail -20 "$TMP/web.log" || true
fi

echo "== 完成 =="
