#!/usr/bin/env bash
# ANFAGENT-30 M3 端到端冒烟：邀请码注册 → 待审（不入网）→ 放行 → 入网
# 前提：easytier-web（embed）已运行在 API 11211 / config server 22020；et.db 已初始化
set -euo pipefail

WEB="${WEB:-http://10.0.0.6:11211}"
CONFIG_SERVER="${CONFIG_SERVER:-udp://10.0.0.6:22020/admin}"
DB="${DB:-/tmp/anf-m3.db}"
BIN="${BIN:-/home/anf-et/anf-easytier/target/release}"
TMP="$(mktemp -d)"
CJ="$TMP/cookies.txt"

ADMIN_MACHINE="$(cat /proc/sys/kernel/random/uuid)"
DEVICE_MACHINE="$(cat /proc/sys/kernel/random/uuid)"

echo "== 1. 初始化 DB 并绑定管理员设备 =="
rm -f "$DB"
"$BIN/easytier-web" --db "$DB" admin-bind --machine-id "$ADMIN_MACHINE" --username admin --create-user-password admin123

echo "== 2. 启动 easytier-web（embed） =="
"$BIN/easytier-web" --db "$DB" --config-server-port 22020 --api-server-port 11211 --no-web false >"$TMP/web.log" 2>&1 &
WEB_PID=$!
trap 'kill $WEB_PID 2>/dev/null || true; rm -rf "$TMP"' EXIT
sleep 2

echo "== 3. 管理员登录 =="
curl -s -c "$CJ" -b "$CJ" -X POST "$WEB/api/v1/auth/login" \
  -H 'Content-Type: application/json' \
  -d '{"username":"admin","password":"admin123"}' >/dev/null

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
sudo -n env PATH="$PATH" "$BIN/easytier-core" -w "$CONFIG_SERVER" \
  --machine-id "$DEVICE_MACHINE" --network-name anf-m3 --dhcp false \
  >"$TMP/client-pending.log" 2>&1 &
CLIENT_PID=$!
sleep 6
echo "-- 客户端日志（待审阶段）--"
tail -5 "$TMP/client-pending.log" || true

echo "== 7. 管理员放行 =="
DEV_ID="$(python3 -c 'import sys,json;print(json.load(open(sys.argv[1]))["id"])' "$TMP/register.json")"
curl -s -c "$CJ" -b "$CJ" -X POST "$WEB/api/v1/devices/$DEV_ID/approve" >/dev/null
echo "设备 $DEV_ID 已放行"

echo "== 8. 客户端应获得配置并建立网络 =="
sleep 8
tail -8 "$TMP/client-pending.log" || true

echo "== 完成 =="
