#!/usr/bin/env python3
"""Check the ANF service-center REST API on the VM (localhost:11211).

Run on the VM: python3 ~/anf-easytier/scripts/anf_api_check.py
"""

import hashlib
import json
import urllib.request
import http.cookiejar

from _anf_env import load_env

ENV = load_env()
BASE = ENV.get("ANF_WEB_BASE") or "http://127.0.0.1:11211"
ADMIN_USER = ENV.get("ANF_ADMIN_USER") or "admin"
ADMIN_PASSWORD = ENV.get("ANF_ADMIN_PASSWORD") or "admin"


def req(opener, path, method="GET", data=None, headers=None):
    headers = headers or {"Content-Type": "application/json"}
    body = json.dumps(data).encode() if data is not None else None
    r = urllib.request.Request(BASE + path, data=body, method=method, headers=headers)
    return opener.open(r, timeout=10)


def main():
    cj = http.cookiejar.CookieJar()
    opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(cj))

    # root serves embedded frontend
    root = opener.open(BASE + "/", timeout=10).read().decode()
    print("root_html_has_anf:", "ANF" in root or "anf" in root, "len=", len(root))

    # login as admin (frontend MD5s the password)
    md5 = hashlib.md5(ADMIN_PASSWORD.encode()).hexdigest()
    try:
        r = opener.open(
            urllib.request.Request(
                BASE + "/api/v1/auth/login",
                data=json.dumps({"username": ADMIN_USER, "password": md5}).encode(),
                method="POST",
                headers={"Content-Type": "application/json"},
            ),
            timeout=10,
        )
        print("login status:", r.status)
    except urllib.error.HTTPError as e:
        print("login HTTPError:", e.code, e.read().decode()[:200])
        return 1

    # list devices
    try:
        devs = json.loads(opener.open(BASE + "/api/v1/devices", timeout=10).read().decode())
        print("device count:", len(devs))
        for d in devs[:5]:
            print(" ", {k: d.get(k) for k in ("id", "machine_id", "display_name", "status")})
    except Exception as e:
        print("devices ERR:", e)

    # list networks
    try:
        nets = json.loads(opener.open(BASE + "/api/v1/networks", timeout=10).read().decode())
        print("network count:", len(nets))
    except Exception as e:
        print("networks ERR:", e)

    print("API check OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
