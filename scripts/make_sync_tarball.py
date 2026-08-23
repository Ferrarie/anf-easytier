#!/usr/bin/env python3
"""Create a source tarball for VM deploy, excluding build artifacts/secrets.

Excludes: target/, node_modules/, .pnpm-store/, .git/ (optional), .vm_tools/,
.serena/, .codex/, .agents/, repo-root dist/ (GUI output), easytier-gui/dist,
this repo's .env files, and other caches. Keeps easytier-web/frontend/dist and
easytier-web/frontend-lib/dist (needed for the rust-embed release build).
"""

import os
import sys
import tarfile


SRC = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(SRC, ".vm_tools", "src-sync.tar.gz")

SKIP_SEGMENTS = {
    "target",
    "node_modules",
    ".pnpm-store",
    ".vm_tools",
    ".serena",
    ".codex",
    ".agents",
    "__pycache__",
    ".ruff_cache",
    ".mypy_cache",
}


def should_skip(rel):
    parts = rel.split("/")
    base = parts[-1]
    if base in ("target", "node_modules", ".pnpm-store", "__pycache__",
                ".ruff_cache", ".mypy_cache"):
        return True
    if any(p in SKIP_SEGMENTS for p in parts):
        return True
    if base == ".env" or base.endswith(".env"):
        return True
    if base.endswith(".tsbuildinfo"):
        return True
    # top-level repo dist/ (GUI release output) and easytier-gui/dist
    if rel == "dist" or rel.startswith("dist/"):
        return True
    if rel == "easytier-gui/dist" or rel.startswith("easytier-gui/dist/"):
        return True
    if rel == "src-sync.tar.gz" or ".vm_tools/" in rel:
        return True
    if base == "anf_vm_key2":
        return True
    return False


def main():
    if len(sys.argv) > 1 and sys.argv[1] == "--no-git":
        skip_git = True
    else:
        skip_git = False

    n = {"files": 0, "dirs": 0}

    def filter_(info):
        rel = info.name.replace("\\", "/")
        # strip leading ./
        if rel.startswith("./"):
            rel = rel[2:]
        if skip_git and (rel == ".git" or rel.startswith(".git/") or rel == ".git/"):
            return None
        if should_skip(rel):
            return None
        if info.isdir():
            n["dirs"] += 1
        else:
            n["files"] += 1
        return info

    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    if os.path.exists(OUT):
        os.remove(OUT)

    with tarfile.open(OUT, "w:gz") as tar:
        tar.add(SRC, arcname=".", recursive=True, filter=filter_)

    size_mb = os.path.getsize(OUT) / (1024 * 1024)
    print(f"tarball={OUT}")
    print(f"size={size_mb:.1f}MB files={n['files']} dirs={n['dirs']}")


if __name__ == "__main__":
    main()
