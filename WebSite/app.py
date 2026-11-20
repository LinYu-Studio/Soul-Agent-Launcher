from flask import Flask, render_template, redirect
import urllib.request, json, os

app = Flask(__name__)

def get_api_base():
    """获取 API 基础地址：环境变量 > 本地检测 > 同域"""
    env = os.environ.get('SAL_API_BASE', '').strip()
    if env:
        return env
    try:
        req = urllib.request.Request('http://127.0.0.1:7890/api/check-update?current_version=0.0.0')
        req.add_header('User-Agent', 'SAL-Website/1.0')
        with urllib.request.urlopen(req, timeout=2) as r:
            return 'http://127.0.0.1:7890'
    except:
        return ''  # 空 = 同域（nginx 统一代理）

API_BASE = get_api_base()

def is_same_origin():
    """若 API_BASE 为空，说明和网站在同一域下"""
    return API_BASE == ''

def get_latest_version():
    """调更新服务器 API 获取最新版本信息"""
    base = API_BASE or ''
    try:
        req = urllib.request.Request(base + '/api/check-update?current_version=0.0.0')
        req.add_header('User-Agent', 'SAL-Website/1.0')
        with urllib.request.urlopen(req, timeout=5) as r:
            return json.loads(r.read().decode())
    except:
        pass
    return {
        'latest_version': '0.3.2',
        'download_url': '/downloads/Soul Agent Launcher_0.3.2_x64_en-US.msi',
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
    """本地开发时代理下载请求到更新服务器"""
    import urllib.request, urllib.parse
    try:
        from flask import Response
        # 重新编码文件名中的特殊字符（空格等）
        safe = urllib.parse.quote(filename, safe='')
        src = 'http://127.0.0.1:7890/downloads/' + safe
        resp = urllib.request.urlopen(src, timeout=10)
        data = resp.read()
        return Response(data, mimetype='application/octet-stream',
                        headers={'Content-Disposition': 'attachment; filename="' + filename + '"',
                                 'Content-Length': str(len(data))})
    except Exception as e:
        return '文件不存在: ' + str(e), 404

@app.route('/download/latest')
def download_latest():
    """重定向到最新版 MSI 下载"""
    info = get_latest_version()
    url = info.get('download_url', '')
    if url:
        # 同域（nginx 代理）→ 相对路径
        # 非同域（本地开发）→ 用 API_BASE 补全
        if url.startswith('/') and not is_same_origin():
            return redirect(API_BASE + url)
        return redirect(url)
    return '暂无可用安装包', 404

@app.route('/tutorial')
@app.route('/tutorial.html')
def tutorial():
    return render_template('tutorial.html')

if __name__ == '__main__':
    app.run(debug=True, host='0.0.0.0', port=5000)
