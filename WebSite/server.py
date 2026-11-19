#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Soul Agent Launcher — 更新服务器"""
import sys
import io
sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8')
sys.stderr = io.TextIOWrapper(sys.stderr.buffer, encoding='utf-8')

import os, json, hashlib, shutil, datetime
from pathlib import Path
from flask import Flask, request, jsonify, render_template, send_file, session, redirect, url_for
from werkzeug.utils import secure_filename
from functools import wraps

BASE_DIR = Path(__file__).parent
UPDATES_DIR = BASE_DIR / 'updates'
UPLOAD_DIR = BASE_DIR / 'downloads'
CONFIG_PATH = BASE_DIR / 'config.json'
UPLOAD_DIR.mkdir(exist_ok=True)
UPDATES_DIR.mkdir(exist_ok=True)

# 默认配置
DEFAULT_CONFIG = {
    "admin_user": "Saki",
    "admin_password_hash": hashlib.sha256("Saki5201314".encode()).hexdigest(),
    "session_secret": os.urandom(24).hex()
}

def load_config():
    if CONFIG_PATH.exists():
        with open(CONFIG_PATH) as f:
            return json.load(f)
    with open(CONFIG_PATH, 'w') as f:
        json.dump(DEFAULT_CONFIG, f, indent=2)
    return DEFAULT_CONFIG

config = load_config()
app = Flask(__name__)
app.secret_key = config.get("session_secret", "dev-secret-key-12345")
app.jinja_env.add_extension('jinja2.ext.loopcontrols')

# 确保 Flask 以 UTF-8 读取模板
import jinja2
app.jinja_loader = jinja2.FileSystemLoader(str(BASE_DIR / 'templates'), encoding='utf-8')

# ============================================================
# 工具函数
# ============================================================

def load_current_version():
    path = UPDATES_DIR / 'current.json'
    if path.exists():
        with open(path) as f:
            return json.load(f)
    return {"latest_version": "0.0.0", "download_url": "", "release_notes": "", "force_update": False}

def save_current_version(data):
    path = UPDATES_DIR / 'current.json'
    with open(path, 'w') as f:
        json.dump(data, f, indent=2, ensure_ascii=False)

def login_required(f):
    @wraps(f)
    def decorated(*args, **kwargs):
        if not session.get('admin_logged_in'):
            return redirect(url_for('admin_login'))
        return f(*args, **kwargs)
    return decorated

# ============================================================
# API — 客户端更新检查
# ============================================================

@app.route('/api/check-update', methods=['GET'])
def api_check_update():
    current_version = request.args.get('current_version', '0.0.0')
    version_info = load_current_version()
    
    # 比较版本号
    def parse_version(v):
        parts = v.lstrip('v').split('.')
        return tuple(int(x) for x in parts[:3])
    
    current = parse_version(current_version)
    latest = parse_version(version_info.get('latest_version', '0.0.0'))
    
    has_update = latest > current
    
    return jsonify({
        "has_update": has_update,
        "latest_version": version_info.get('latest_version', '0.0.0'),
        "download_url": version_info.get('download_url', ''),
        "sha256": version_info.get('sha256', ''),
        "release_notes": version_info.get('release_notes', ''),
        "force_update": version_info.get('force_update', False),
        "release_date": version_info.get('release_date', ''),
    })

@app.route('/downloads/<path:filename>', methods=['GET'])
def api_download(filename):
    # 先按原始文件名找（支持文件名含空格）
    file_path = UPLOAD_DIR / filename
    if not file_path.exists():
        # 回退：secure_filename 会把空格转下划线
        safe = secure_filename(filename)
        file_path2 = UPLOAD_DIR / safe
        if file_path2.exists():
            return send_file(file_path2, as_attachment=True, download_name=safe)
        return jsonify({"error": "File not found"}), 404
    return send_file(file_path, as_attachment=True, download_name=filename)

# ============================================================
# 管理后台
# ============================================================

@app.route('/admin', methods=['GET', 'POST'])
def admin_login():
    error = None
    if request.method == 'POST':
        username = request.form.get('username', '')
        password = request.form.get('password', '')
        pw_hash = hashlib.sha256(password.encode()).hexdigest()
        if username == config['admin_user'] and pw_hash == config['admin_password_hash']:
            session['admin_logged_in'] = True
            return redirect(url_for('admin_panel'))
        else:
            error = '用户名或密码错误'
    
    return render_template('admin.html', error=error, login=True)

@app.route('/admin/panel')
@login_required
def admin_panel():
    version_info = load_current_version()
    versions = []
    for d in sorted(UPLOAD_DIR.iterdir(), reverse=True):
        if d.is_file() and d.suffix.lower() in ('.msi', '.exe', '.zip'):
            stat = d.stat()
            versions.append({
                'name': d.name,
                'size': f"{stat.st_size / 1024 / 1024:.1f}MB",
                'mtime': datetime.datetime.fromtimestamp(stat.st_mtime).strftime('%Y-%m-%d %H:%M'),
            })
    
    return render_template('admin.html', login=False, version_info=version_info, versions=versions)

@app.route('/admin/update-version', methods=['POST'])
@login_required
def admin_update_version():
    latest_version = request.form.get('latest_version', '').strip()
    release_notes = request.form.get('release_notes', '').strip()
    force_update = request.form.get('force_update', 'off') == 'on'
    release_date = request.form.get('release_date', datetime.date.today().isoformat())

    # 找到最新上传的文件作为下载 URL
    download_url = ''
    sha256 = ''
    uploaded_file = request.files.get('file')
    if uploaded_file and uploaded_file.filename:
        safe_name = secure_filename(uploaded_file.filename)
        file_path = UPLOAD_DIR / safe_name
        uploaded_file.save(file_path)
        download_url = f'/downloads/{safe_name}'
        # 上传即计算 SHA256，随 current.json 下发，客户端安装前校验
        h = hashlib.sha256()
        with open(file_path, 'rb') as f:
            for chunk in iter(lambda: f.read(1024 * 1024), b''):
                h.update(chunk)
        sha256 = h.hexdigest()

    if latest_version:
        data = load_current_version()
        data['latest_version'] = latest_version
        if release_notes:
            data['release_notes'] = release_notes
        if download_url:
            data['download_url'] = download_url
            data['sha256'] = sha256
        data['force_update'] = force_update
        data['release_date'] = release_date
        save_current_version(data)

    return redirect(url_for('admin_panel'))

@app.route('/admin/logout')
def admin_logout():
    session.pop('admin_logged_in', None)
    return redirect(url_for('admin_login'))

# ============================================================
# 启动
# ============================================================

if __name__ == '__main__':
    port = int(os.environ.get('PORT', 7890))
    print(f"\n  Soul Agent Launcher 更新服务器")
    print(f"  ─────────────────────────────")
    print(f"  API:       http://0.0.0.0:{port}/api/check-update")
    print(f"  管理后台:  http://0.0.0.0:{port}/admin")
    print(f"  启动完成\n")
    app.run(host='0.0.0.0', port=port, debug=False)
