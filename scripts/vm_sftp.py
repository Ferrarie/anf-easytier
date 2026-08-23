#!/usr/bin/env python3
"""ANFAGENT-30: put/get a file to/from the VM via SFTP (password auth, source-bound).

Usages:
    python vm_sftp.py put <local_file> /home/anf-et/<remote_path>
    python vm_sftp.py get /home/anf-et/<remote_path> <local_file>

Reads VM creds from the repo-root `.env`.
"""

import os
import socket
import sys

import paramiko


def load_env(path):
    cfg = {}
    if not os.path.exists(path):
        return cfg
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line or line.startswith("#") or "=" not in line:
                continue
            k, _, v = line.partition("=")
            cfg[k.strip()] = v.strip()
    return cfg


def main():
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    env = load_env(os.path.join(root, ".env"))
    host = env.get("ANF_VM_HOST", "10.0.0.6")
    port = int(env.get("ANF_VM_PORT", "22"))
    user = env.get("ANF_VM_USER", "anf-et")
    password = env.get("ANF_VM_PASSWORD")
    bind = env.get("ANF_VM_SSH_BIND", "10.0.0.3")

    if len(sys.argv) < 4:
        print("usage: vm_sftp.py put <local> <remote> | get <remote> <local>",
              file=sys.stderr)
        return 2
    action, a, b = sys.argv[1], sys.argv[2], sys.argv[3]

    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.settimeout(60)
    sock.bind((bind, 0))
    sock.connect((host, port))
    t = paramiko.Transport(sock)
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
