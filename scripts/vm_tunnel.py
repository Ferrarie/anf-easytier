#!/usr/bin/env python3
"""Forward a local TCP port to a loopback-only service on the ANF VM."""

from __future__ import annotations

import argparse
import select
import socket
import sys
import threading

import paramiko

from _anf_env import load_env, require_env


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--remote-port", type=int, default=3456)
    parser.add_argument("--local-port", type=int, default=3456)
    parser.add_argument("--local-host", default="127.0.0.1")
    return parser.parse_args()


def connect_transport() -> paramiko.Transport:
    env = load_env()
    host = require_env(env, "ANF_VM_HOST")
    port = int(env.get("ANF_VM_PORT", "22"))
    user = env.get("ANF_VM_USER", "anf-et")
    password = env.get("ANF_VM_PASSWORD") or None
    key_path = (env.get("ANF_VM_SSH_KEY") or "").strip() or None
    if not password and not key_path:
        print("缺少认证配置：请在 .env 设置 ANF_VM_SSH_KEY 或 ANF_VM_PASSWORD",
              file=sys.stderr)
        raise SystemExit(2)
    source_bind = env.get("ANF_VM_SSH_BIND") or ""

    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.settimeout(60)
    try:
        if source_bind:
            sock.bind((source_bind, 0))
        sock.connect((host, port))
        client = paramiko.SSHClient()
        client.set_missing_host_key_policy(paramiko.AutoAddPolicy())
        client.connect(
            hostname=host,
            port=port,
            username=user,
            sock=sock,
            password=password,
            key_filename=key_path,
            look_for_keys=True,
            allow_agent=True,
            timeout=60,
        )
        transport = client.get_transport()
    except (OSError, paramiko.SSHException) as error:
        print(f"SSH tunnel connect failed: {error}", file=sys.stderr)
        raise SystemExit(3)

    return transport


def forward_connection(
    local_socket: socket.socket,
    remote_port: int,
    transport: paramiko.Transport,
) -> None:
    channel = None
    try:
        channel = transport.open_channel(
            "direct-tcpip",
            ("127.0.0.1", remote_port),
            local_socket.getpeername(),
        )
        while True:
            ready, _, _ = select.select([local_socket, channel], [], [])
            for readable in ready:
                data = readable.recv(64 * 1024)
                target = channel if readable is local_socket else local_socket
                if not data:
                    return
                target.sendall(data)
    except (OSError, paramiko.SSHException):
        pass
    finally:
        if channel is not None:
            channel.close()
        local_socket.close()


def main() -> int:
    args = parse_args()
    transport = connect_transport()
    server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    server.bind((args.local_host, args.local_port))
    server.listen(16)
    print(
        f"Tunneling {args.local_host}:{args.local_port} -> "
        f"ECS 127.0.0.1:{args.remote_port}"
    )
    print("Press Ctrl+C to stop.")
    try:
        while True:
            client, address = server.accept()
            threading.Thread(
                target=forward_connection,
                args=(client, args.remote_port, transport),
                daemon=True,
            ).start()
    except KeyboardInterrupt:
        return 0
    except OSError as error:
        print(f"Tunnel listener failed: {error}", file=sys.stderr)
        return 5
    finally:
        server.close()
        transport.close()


if __name__ == "__main__":
    raise SystemExit(main())
