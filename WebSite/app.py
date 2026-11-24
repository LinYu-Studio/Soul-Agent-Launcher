from flask import Flask, render_template, redirect, Response, request
import urllib.request, urllib.parse, json, os, time

app = Flask(__name__)

# 服务端内部访问更新 API 的地址（仅服务端用它取数据，绝不下发给浏览器）
_INTERNAL_API = os.environ.get('SAL_API_BASE', '').strip() or 'http://127.0.0.1:7890'

# API 探测缓存：避免每次请求都探测，同时避免启动时一次探测失败就永久 fallback
_probe = {'ok': None, 'at': 0.0}
_PROBE_TTL = 30.0


def api_base() -> str:
    """返回服务端可用的 API 基础地址；探测失败返回空串（走同域相对路径兜底）"""
    now = time.time()
    if _probe['ok'] is False and now - _probe['at'] < _PROBE_TTL:
        return ''
    try:
        req = urllib.request.Request(_INTERNAL_API + '/health')
        req.add_header('User-Agent', 'SAL-Website/1.0')
        with urllib.request.urlopen(req, timeout=2) as r:
            if r.status == 200:
                _probe.update(ok=True, at=now)
                return _INTERNAL_API
    except Exception:
        pass
    _probe.update(ok=False, at=now)
    return ''


def get_latest_version():
    """取最新版本信息：同域相对路径即可，浏览器端由 Caddy 统一路由"""
    base = api_base()
    try:
        req = urllib.request.Request(base + '/api/check-update?current_version=0.0.0')
        req.add_header('User-Agent', 'SAL-Website/1.0')
        with urllib.request.urlopen(req, timeout=5) as r:
            return json.loads(r.read().decode())
    except Exception:
        pass
    return {
        'latest_version': '0.0.0',
        'download_url': '',
        'release_notes': '',
        'release_date': ''
    }


@app.route('/')
@app.route('/index.html')
def index():
    info = get_latest_version()
    return render_template('index.html', version_info=info)


@app.route('/downloads/<path:filename>')
def proxy_download(filename):
    """浏览器下载兜底：流式转发到更新服务（生产环境由 Caddy 直接路由 /downloads，不经过这里）"""
    base = api_base()
    if not base:
        return '下载服务暂时不可用，请稍后重试', 503
    safe = urllib.parse.quote(filename, safe='')
    try:
        upstream = urllib.request.urlopen(base + '/downloads/' + safe, timeout=30)
    except Exception as e:
        return '文件不存在: ' + str(e), 404

    def stream():
        try:
            while True:
                chunk = upstream.read(256 * 1024)
                if not chunk:
                    break
                yield chunk
        finally:
            upstream.close()

    headers = {
        'Content-Disposition': 'attachment; filename="%s"' % filename,
        'Content-Type': 'application/octet-stream',
    }
    length = upstream.headers.get('Content-Length')
    if length:
        headers['Content-Length'] = length
    return Response(stream(), headers=headers)


@app.route('/download/latest')
def download_latest():
    """重定向到最新版 MSI 下载（一律用同域相对路径，浏览器自己解析域名）"""
    info = get_latest_version()
    url = info.get('download_url', '')
    if url:
        # 若后端返回绝对地址且指向回环地址，强制改回相对路径
        if url.startswith('http://127.0.0.1') or url.startswith('http://localhost'):
            url = urllib.parse.urlparse(url).path or '/'
        return redirect(url)
    return '暂无可用安装包', 404


@app.route('/tutorial')
@app.route('/tutorial.html')
def tutorial():
    return render_template('tutorial.html')


if __name__ == '__main__':
    port = int(os.environ.get('PORT', 5000))
    host = os.environ.get('HOST', '0.0.0.0')
    debug = os.environ.get('SAL_DEBUG', '').lower() in ('1', 'true', 'yes')
    app.run(host=host, port=port, debug=debug)
