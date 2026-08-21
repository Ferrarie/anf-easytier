#!/usr/bin/env bash
# ANFAGENT-30 M3 docker 栈端到端：注册→待审→放行→分配→客户端 netns 入网→默认拒绝→ACL 热更新→互通
# 前提：docker compose -f deploy/compose.anf.yaml up -d 已就绪；本脚本在 VM 上以 root 运行
set -uo pipefail

WEB="${WEB:-http://10.0.0.6:11211}"
CONFIG_SERVER="${CONFIG_SERVER:-udp://10.0.0.6:22020/admin}"
CORE_BIN="${CORE_BIN:-/home/anf-et/anf-easytier/target/release/easytier-core}"
TMP="$(mktemp -d)"
CJ="$TMP/cookies.txt"

echo "== 1. 等待 web 就绪 =="
for i in $(seq 1 30); do
  if curl -s -o /dev/null -w '%{http_code}' "$WEB/api/v1/summary" 2>/dev/null | grep -qE '200|401|302'; then
    echo "web ready (try $i)"
    break
  fi
  sleep 2
done

echo "== 2. 管理员登录（默认 admin/admin，前端 MD5） =="
LOGIN_RESP="$(curl -s -c "$CJ" -b "$CJ" -X POST "$WEB/api/v1/auth/login" \
  -H 'Content-Type: application/json' \
  -d '{"username":"admin","password":"00000000000000000000000000000000"}')"
echo "login: $LOGIN_RESP"

echo "== 3. 创建邀请码 =="
CODE="$(curl -s -c "$CJ" -b "$CJ" -X POST "$WEB/api/v1/invites" \
  -H 'Content-Type: application/json' -d '{"max_uses":2}' | python3 -c 'import sys,json;print(json.load(sys.stdin)["code"])')"
echo "邀请码: $CODE"

echo "== 4. 建网络与 tag =="
NET="$(curl -s -c "$CJ" -b "$CJ" -X POST "$WEB/api/v1/networks" \
  -H 'Content-Type: application/json' -d '{"name":"办公网","cidr":"10.99.0.0/24"}')"
NET_ID="$(echo "$NET" | python3 -c 'import sys,json;print(json.load(sys.stdin)["id"])')"
echo "网络: $NET_ID"
curl -s -c "$CJ" -b "$CJ" -X POST "$WEB/api/v1/tags" \
  -H 'Content-Type: application/json' -d '{"name":"办公"}' >/dev/null

setup_netns() {
  local ns="$1" ip="$2"
  local net="${ip%.*}"
  ip netns del "$ns" 2>/dev/null || true
  ip netns add "$ns"
  ip link add "v-$ns" type veth peer name "p-$ns"
  ip link set "p-$ns" netns "$ns"
  ip addr add "${net}.1/24" dev "v-$ns"
  ip link set "v-$ns" up
  ip netns exec "$ns" ip link set lo up
  ip netns exec "$ns" ip addr add "$ip/24" dev "p-$ns"
  ip netns exec "$ns" ip link set "p-$ns" up
  ip netns exec "$ns" ip route add default via "${net}.1"
}

sysctl -w net.ipv4.ip_forward=1 >/dev/null
iptables -t nat -C POSTROUTING -s 10.200.0.0/16 -j MASQUERADE 2>/dev/null || \
  iptables -t nat -A POSTROUTING -s 10.200.0.0/16 -j MASQUERADE

start_client() {
  local ns="$1" machine_id="$2" log="$3"
  setup_netns "$ns" "$4"
  ip netns exec "$ns" "$CORE_BIN" -w "$CONFIG_SERVER" \
    --machine-id "$machine_id" --network-name anf-m3 --no-listener \
    >"$log" 2>&1 &
  echo $!
}

M1="$(cat /proc/sys/kernel/random/uuid)"
M2="$(cat /proc/sys/kernel/random/uuid)"

echo "== 5. 注册并放行两台设备 =="
register_and_approve() {
  local machine_id="$1"
  local reg
  reg="$(curl -s -X POST "$WEB/api/v1/devices/register" \
    -H 'Content-Type: application/json' \
    -d "{\"invite_code\":\"$CODE\",\"machine_id\":\"$machine_id\"}")"
  local dev_id
  dev_id="$(echo "$reg" | python3 -c 'import sys,json;print(json.load(sys.stdin)["id"])')"
  curl -s -c "$CJ" -b "$CJ" -X POST "$WEB/api/v1/devices/$dev_id/approve" >/dev/null
  curl -s -c "$CJ" -b "$CJ" -X PATCH "$WEB/api/v1/devices/$dev_id" \
    -H 'Content-Type: application/json' \
    -d "{\"tags\":[\"办公\"],\"networks\":[\"$NET_ID\"]}" >/dev/null
  echo "$dev_id"
}

D1="$(register_and_approve "$M1")"
D2="$(register_and_approve "$M2")"
echo "设备: $D1 / $D2"

echo "== 6. 客户端入网（netns） =="
P1="$(start_client anf-cli1 "$M1" "$TMP/client1.log" 10.200.0.2)"
P2="$(start_client anf-cli2 "$M2" "$TMP/client2.log" 10.200.1.2)"
sleep 12

echo "-- client1 --"
tail -6 "$TMP/client1.log"
echo "-- client2 --"
tail -6 "$TMP/client2.log"

echo "== 7. 各 netns tun0 虚拟 IP =="
IP1="$(ip netns exec anf-cli1 ip -4 addr show tun0 2>/dev/null | awk '/inet /{print $2}' | cut -d/ -f1)"
IP2="$(ip netns exec anf-cli2 ip -4 addr show tun0 2>/dev/null | awk '/inet /{print $2}' | cut -d/ -f1)"
echo "client1 tun0=$IP1 ; client2 tun0=$IP2"

echo "== 8. 默认拒绝：client1 ping client2 应失败 =="
if [ -n "$IP2" ] && [ -n "$IP1" ]; then
  ip netns exec anf-cli1 ping -c 2 -W 2 "$IP2" >/dev/null 2>&1 && echo "意外放行！" || echo "OK：默认拒绝生效"
else
  echo "tun0 未建立（$IP1/$IP2），跳过 ping"
fi

echo "== 9. 添加 ACL 放行规则（办公→办公 any）→ 热更新 =="
RULE="$(curl -s -c "$CJ" -b "$CJ" -X POST "$WEB/api/v1/networks/$NET_ID/rules" \
  -H 'Content-Type: application/json' \
  -d '{"name":"allow-office","enabled":true,"source_tags":["办公"],"destination_tags":["办公"],"protocol":"any","ports":[],"action":"allow","priority":100}')"
echo "rule: $RULE"
sleep 10

echo "== 10. 热更新后：client1 ping client2 应成功 =="
if [ -n "$IP2" ] && [ -n "$IP1" ]; then
  ip netns exec anf-cli1 ping -c 3 -W 3 "$IP2" 2>&1 | tail -4
else
  echo "无 tun IP，跳过"
fi

echo "== 11. 收尾清理 =="
kill "$P1" "$P2" 2>/dev/null || true
ip netns del anf-cli1 2>/dev/null || true
ip netns del anf-cli2 2>/dev/null || true
rm -rf "$TMP"
echo "== done =="
