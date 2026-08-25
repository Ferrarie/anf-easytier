import urllib.request, hashlib, http.cookiejar, json

from _anf_env import load_env

ENV = load_env()
BASE = ENV.get('ANF_WEB_BASE') or 'http://127.0.0.1:11211'
ADMIN_USER = ENV.get('ANF_ADMIN_USER') or 'admin'
ADMIN_PASSWORD = ENV.get('ANF_ADMIN_PASSWORD') or 'admin'
cj = http.cookiejar.CookieJar()
opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(cj))
md5 = hashlib.md5(ADMIN_PASSWORD.encode()).hexdigest()

data = ('{"username":"' + ADMIN_USER + '","password":"' + md5 + '"}').encode()
req = urllib.request.Request(BASE + '/api/v1/auth/login', data=data,
                             headers={'Content-Type': 'application/json'})
opener.open(req)

devices = json.loads(opener.open(urllib.request.Request(BASE + '/api/v1/devices')).read().decode())
target = next((d for d in devices if d['machine_id'].startswith('5d74b79b')), None)
if not target:
    print('no device to approve')
    raise SystemExit(0)

print('approving device id', target['id'])
try:
    r = opener.open(urllib.request.Request(BASE + f"/api/v1/devices/{target['id']}/approve", method='POST'))
    body = r.read().decode()
    print('approve status:', r.status)
    print('approve body:', body[:200])
except Exception as e:
    print('approve ERR:', e)
    # HTTPError 也读 body
    if hasattr(e, 'read'):
        print('err body:', e.read().decode()[:300])

# 再查一次状态
devices2 = json.loads(opener.open(urllib.request.Request(BASE + '/api/v1/devices')).read().decode())
target2 = next((d for d in devices2 if d['machine_id'].startswith('5d74b79b')), None)
print('after approve:', { 'status': target2['status'], 'networks': target2.get('networks') } if target2 else 'gone')
