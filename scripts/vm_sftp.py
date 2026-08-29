#!/usr/bin/env python3
"""ANFAGENT-30: put/get a file to/from the VM via SFTP (password auth, source-bound).

Usages:
    python vm_sftp.py put <local_file> /home/anf-et/<remote_path>
    python vm_sftp.py get /home/anf-et/<remote_path> <local_file>

Reads VM creds from the repo-root `.env`.
"""

import socket
import sys

import paramiko

from _anf_env import load_env, require_env


def load_pkey(path):
    """按常见算法尝试解析私钥文件（Transport.connect 只收 pkey 对象）。"""
    last_error = None
    for cls in (paramiko.Ed25519Key, paramiko.RSAKey, paramiko.ECDSAKey):
        try:
            return cls.from_private_key_file(path)
        except paramiko.SSHException as exc:
            last_error = exc
    print(f"无法解析私钥 {path}: {last_error}", file=sys.stderr)
    raise SystemExit(2)

def main():
    env = load_env()
    host = require_env(env, "ANF_VM_HOST")
    port = int(env.get("ANF_VM_PORT", "22"))
    user = env.get("ANF_VM_USER", "anf-et")
    password = env.get("ANF_VM_PASSWORD") or ""
    key_path = (env.get("ANF_VM_SSH_KEY") or "").strip()
    if not password and not key_path:
        print("缺少认证配置：请在 .env 设置 ANF_VM_SSH_KEY 或 ANF_VM_PASSWORD",
              file=sys.stderr)
        return 2
    bind = env.get("ANF_VM_SSH_BIND") or ""

    if len(sys.argv) < 4:
        print("usage: vm_sftp.py put <local> <remote> | get <remote> <local>",
              file=sys.stderr)
        return 2
    action, a, b = sys.argv[1], sys.argv[2], sys.argv[3]

    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.settimeout(60)
    if bind:
        sock.bind((bind, 0))
    sock.connect((host, port))
    t = paramiko.Transport(sock)
    if key_path:
        t.connect(username=user, password=password or None, pkey=load_pkey(key_path))
    else:
        t.connect(username=user, password=password)
    client = paramiko.SSHClient()
    client._transport = t
    sftp = client.open_sftp()
    try:
        if action == "put":
            sftp.put(a, b)
            print(f"put {a} -> {b} ({os.path.getsize(a)} bytes)")
        elif action == "get":
            sftp.get(a, b)
            print(f"get {a} -> {b}")
        else:
            print("unknown action", action, file=sys.stderr)
            return 3
    finally:
        sftp.close()
        t.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
