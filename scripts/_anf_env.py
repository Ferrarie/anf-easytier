"""Shared loader for the repo-root .env (single configuration source).

Usage::

    from _anf_env import load_env

    env = load_env()
    host = env.get("ANF_VM_HOST") or ""

Real environment variables always take precedence over .env values.
"""

from __future__ import annotations

import os
from pathlib import Path


def repo_root() -> Path:
    """Repository root = parent of scripts/."""
    return Path(__file__).resolve().parent.parent


def load_env(path: Path | None = None) -> dict[str, str]:
    """Load repo-root .env merged with the real environment (env wins)."""
    cfg: dict[str, str] = {}
    env_file = path or (repo_root() / ".env")
    if env_file.exists():
        for raw in env_file.read_text(encoding="utf-8").splitlines():
            line = raw.strip()
            if not line or line.startswith("#") or "=" not in line:
                continue
            key, _, value = line.partition("=")
            key = key.strip()
            value = value.strip()
            if key:
                cfg[key] = value
    cfg.update({k: v for k, v in os.environ.items()})
    return cfg


def require_env(env: dict[str, str], key: str) -> str:
    """Return a non-empty value for key, or exit with a clear message."""
    value = env.get(key) or ""
    if not value:
        print(
            f"缺少配置：请在仓库根目录 .env 设置 {key}（参考 .env.example）",
            file=os.sys.stderr,
        )
        raise SystemExit(2)
    return value
