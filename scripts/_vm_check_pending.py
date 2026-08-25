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
print('device count:', len(devices))
for d in devices:
    print({ 'id': d['id'], 'machine_id': d['machine_id'][:8], 'display_name': d['display_name'],
            'status': d['status'], 'networks': d.get('networks'), 'tags': d.get('tags') })
