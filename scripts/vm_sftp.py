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


def main():
    env = load_env()
    host = require_env(env, "ANF_VM_HOST")
    port = int(env.get("ANF_VM_PORT", "22"))
    user = env.get("ANF_VM_USER", "anf-et")
    password = require_env(env, "ANF_VM_PASSWORD")
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
