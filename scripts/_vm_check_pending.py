import urllib.request, hashlib, http.cookiejar, json

BASE = 'http://127.0.0.1:11211'
cj = http.cookiejar.CookieJar()
opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(cj))
md5 = hashlib.md5(b'admin').hexdigest()

data = ('{"username":"admin","password":"' + md5 + '"}').encode()
req = urllib.request.Request(BASE + '/api/v1/auth/login', data=data,
                             headers={'Content-Type': 'application/json'})
opener.open(req)

devices = json.loads(opener.open(urllib.request.Request(BASE + '/api/v1/devices')).read().decode())
print('device count:', len(devices))
for d in devices:
    print({ 'id': d['id'], 'machine_id': d['machine_id'][:8], 'display_name': d['display_name'],
            'status': d['status'], 'networks': d.get('networks'), 'tags': d.get('tags') })
