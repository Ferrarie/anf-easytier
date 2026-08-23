#!/usr/bin/env python3
"""ANFAGENT-30: run a command on the VM via SSH (password auth, source-bound).

Reads connection params from the repo-root `.env`, binds the local socket to the
etgame mesh IP so EasyTier routes the traffic, and runs `<cmd>` over SSH.
Supports sudo via `--sudo` (password from ANF_VM_SUDO_PASSWORD).

Usage:
    python scripts/vm_ssh.py [-s|--sudo] [--user USER] "<command>"

Reads only .env (already gitignored). Never prints the password.
"""

import os
import socket
import sys
import re

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
    sudo_pass = env.get("ANF_VM_SUDO_PASSWORD", password)

    args = sys.argv[1:]
    use_sudo = False
    if args and args[0] in ("-s", "--sudo"):
        use_sudo = True
        args = args[1:]
    if args and args[0] == "--user":
        user = args[1]
        args = args[2:]

    if not args:
        print("usage: vm_ssh.py [-s|--sudo] [--user USER] <command>", file=sys.stderr)
        return 2
    cmd = " ".join(args)
    if use_sudo:
        cmd = f"echo {sudo_pass} | sudo -S -p '' bash -lc {sh_quote(cmd)}"
    else:
        # Non-interactive SSH has a minimal PATH; set the toolchain + system bin.
        cmd = (
            "export PATH=/home/anf-et/.cargo/bin:/usr/local/sbin:/usr/local/bin:"
            "/usr/sbin:/usr/bin:/sbin:/bin; "
            + cmd
        )

    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.settimeout(60)
    try:
        if bind:
            sock.bind((bind, 0))
        sock.connect((host, port))
    except OSError as e:
        print(f"SSH connect failed: {e}", file=sys.stderr)
        return 3

    t = paramiko.Transport(sock)
    try:
        t.connect(username=user, password=password)
    except paramiko.AuthenticationException:
        print("SSH auth failed", file=sys.stderr)
        return 4

    client = paramiko.SSHClient()
    client._transport = t
    stdin, stdout, stderr = client.exec_command(cmd, timeout=600)
    out = stdout.read().decode("utf-8", "replace")
    err = stderr.read().decode("utf-8", "replace")
    rc = stdout.channel.recv_exit_status()
    sys.stdout.write(out)
    sys.stderr.write(err)
    t.close()
    return rc


def sh_quote(s):
    return "'" + s.replace("'", "'\\''") + "'"


if __name__ == "__main__":
    sys.exit(main())
