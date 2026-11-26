#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Soul Agent Launcher — 更新服务器

部署形态（生产）：
  - 本服务只监听 127.0.0.1:7890，由 Caddy 反向代理 sal.bszx.site
  - /api/check-update   客户端检查更新
  - /downloads/<file>   安装包下载（支持 Range 断点续传）
  - /admin              管理后台（上传 MSI 自动算 SHA256 并发布）
  - /health             健康检查

安全说明：源码不再内置默认密码。密码来源优先级：
  1) 同目录 config.json 的 admin_password_hash
  2) 环境变量 SAL_ADMIN_PASSWORD_HASH（sha256 hex）或 SAL_ADMIN_PASSWORD（明文，兼容用）
  3) 都没有 → 自动生成随机密码，仅打印到 stdout/日志一次
"""
import sys
import io
sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8', line_buffering=True)
sys.stderr = io.TextIOWrapper(sys.stderr.buffer, encoding='utf-8', line_buffering=True)

import os, json, re, hashlib, hmac, secrets, time, datetime
from pathlib import Path
from flask import Flask, request, jsonify, render_template, send_file, session, redirect, url_for, abort
from functools import wraps

BASE_DIR = Path(__file__).parent
UPDATES_DIR = BASE_DIR / 'updates'
UPLOAD_DIR = BASE_DIR / 'downloads'
CONFIG_PATH = BASE_DIR / 'config.json'
UPLOAD_DIR.mkdir(exist_ok=True)
UPDATES_DIR.mkdir(exist_ok=True)


def sha256_hex(text: str) -> str:
    return hashlib.sha256(text.encode('utf-8')).hexdigest()


def build_default_config():
    """生成默认配置：密码优先取环境变量，否则随机生成并打印一次"""
    pw_hash = os.environ.get('SAL_ADMIN_PASSWORD_HASH', '').strip()
    generated = None
    if not pw_hash:
        raw_pw = os.environ.get('SAL_ADMIN_PASSWORD', '').strip()
        if raw_pw:
            pw_hash = sha256_hex(raw_pw)
        else:
            generated = secrets.token_urlsafe(12)
            pw_hash = sha256_hex(generated)
    cfg = {
        "admin_user": os.environ.get('SAL_ADMIN_USER', 'Saki'),
        "admin_password_hash": pw_hash,
        "session_secret": secrets.token_hex(24),
    }
    if generated:
        print("=" * 60, flush=True)
        print(f"[SAL] 首次启动，已生成管理后台随机密码: {generated}", flush=True)
        print("[SAL] 请立即登录 /admin 并记录；此密码只显示这一次。", flush=True)
        print("=" * 60, flush=True)
    return cfg


def load_config():
    if CONFIG_PATH.exists():
        with open(CONFIG_PATH, encoding='utf-8') as f:
            return json.load(f)
    cfg = build_default_config()
    atomic_write_text(CONFIG_PATH, json.dumps(cfg, indent=2))
    return cfg


def atomic_write_text(path: Path, content: str):
    """原子写：临时文件 + rename，避免进程中断留下半截 JSON"""
    tmp = path.with_suffix(path.suffix + '.tmp')
    with open(tmp, 'w', encoding='utf-8') as f:
        f.write(content)
        f.flush()
        os.fsync(f.fileno())
    os.replace(tmp, path)


config = load_config()
app = Flask(__name__)
app.secret_key = config.get("session_secret", secrets.token_hex(24))
app.jinja_env.add_extension('jinja2.ext.loopcontrols')
app.json.ensure_ascii = False

# 确保 Flask 以 UTF-8 读取模板
import jinja2
app.jinja_loader = jinja2.FileSystemLoader(str(BASE_DIR / 'templates'), encoding='utf-8')


def login_required(f):
    @wraps(f)
    def decorated(*args, **kwargs):
        if not session.get('admin_logged_in'):
            return redirect(url_for('admin_login'))
        return f(*args, **kwargs)
    return decorated


# ---------- 登录限流（暴力破解防护） ----------
LOGIN_MAX_ATTEMPTS = 5
LOGIN_WINDOW_SECS = 300
_login_attempts = {}  # ip -> [时间戳]


def _client_ip() -> str:
    xff = request.headers.get('X-Forwarded-For', '')
    if xff:
        return xff.split(',')[0].strip()
    return request.remote_addr or 'unknown'


def _recent_failures(ip: str):
    now = time.time()
    arr = [t for t in _login_attempts.get(ip, []) if now - t < LOGIN_WINDOW_SECS]
    _login_attempts[ip] = arr
    return arr


def _login_blocked(ip: str) -> bool:
    return len(_recent_failures(ip)) >= LOGIN_MAX_ATTEMPTS


def _record_login_failure(ip: str):
    arr = _recent_failures(ip)
    arr.append(time.time())
    _login_attempts[ip] = arr


# ---------- CSRF ----------
def _csrf_token() -> str:
    tok = session.get('csrf_token')
    if not tok:
        tok = secrets.token_urlsafe(24)
        session['csrf_token'] = tok
    return tok


def _csrf_ok() -> bool:
    sent = request.form.get('csrf_token', '')
    expected = session.get('csrf_token', '')
    return bool(expected) and hmac.compare_digest(sent, expected)


# ============================================================
# 工具函数
# ============================================================

def load_current_version():
    path = UPDATES_DIR / 'current.json'
    if path.exists():
        try:
            with open(path, encoding='utf-8') as f:
                return json.load(f)
        except (json.JSONDecodeError, OSError):
            pass
    return {"latest_version": "0.0.0", "download_url": "", "release_notes": "", "force_update": False}


def save_current_version(data):
    # 覆盖前留一份上一版，发布失误时可直接回滚
    path = UPDATES_DIR / 'current.json'
    if path.exists():
        try:
            with open(path, encoding='utf-8') as f:
                prev = f.read()
            atomic_write_text(UPDATES_DIR / 'current.json.bak', prev)
        except OSError:
            pass
    atomic_write_text(path, json.dumps(data, indent=2, ensure_ascii=False))


def parse_version(v) -> tuple:
    """把任意版本串解析成 (major, minor, patch)。
    兼容 'v0.4.2'、'0.4.2-beta'、'0.4.2+Beta' 等——只取前三段数字，不因后缀崩溃。"""
    nums = [int(x) for x in re.findall(r'\d+', str(v or '0'))[:3]]
    while len(nums) < 3:
        nums.append(0)
    return tuple(nums)


def safe_filename(name: str) -> str:
    """上传文件名归一化：保留 ASCII 字母数字与 . _ -，空格统一成下划线。
    返回的名字同时用作磁盘文件名和 download_url，保证两者一致（避免历史 404）。"""
    base = os.path.basename(str(name or '')).strip()
    base = re.sub(r'\s+', '_', base)
    base = re.sub(r'[^A-Za-z0-9._\-]', '', base)
    return base.lstrip('.') or 'package.bin'


# ============================================================
# API — 客户端更新检查
# ============================================================

@app.route('/health')
def health():
    return jsonify({"ok": True, "service": "sal-update", "version": load_current_version().get('latest_version', '')})


@app.route('/')
def index():
    """本服务是更新服务器，根路径跳转到管理后台"""
    return redirect(url_for('admin_login'))


@app.route('/api/check-update', methods=['GET'])
def api_check_update():
    current_version = request.args.get('current_version', '0.0.0')
    version_info = load_current_version()

    current = parse_version(current_version)
    latest = parse_version(version_info.get('latest_version', '0.0.0'))
    has_update = latest > current

    resp = jsonify({
        "has_update": has_update,
        "latest_version": version_info.get('latest_version', '0.0.0'),
        "display_version": version_info.get('display_version', ''),
        "download_url": version_info.get('download_url', ''),
        "sha256": version_info.get('sha256', ''),
        "release_notes": version_info.get('release_notes', ''),
        "force_update": version_info.get('force_update', False),
        "release_date": version_info.get('release_date', ''),
    })
    # 客户端每次启动都查询，禁止任何缓存层返回旧版本号
    resp.headers['Cache-Control'] = 'no-store, must-revalidate'
    resp.headers['Access-Control-Allow-Origin'] = '*'
    return resp


@app.route('/downloads/<path:filename>', methods=['GET', 'HEAD'])
def api_download(filename):
    # 只允许取 basename，阻断 ../ 路径穿越
    target = UPLOAD_DIR / os.path.basename(filename)
    if not target.exists() or not target.is_file():
        # 兼容历史带空格文件名（旧版本按原样存过）
        alt = UPLOAD_DIR / safe_filename(filename)
        if alt.exists() and alt.is_file():
            target = alt
        else:
            abort(404, description="File not found")
    # conditional=True：自动支持 Range/If-Range/ETag，断点续传依赖它
    return send_file(target, as_attachment=True, download_name=target.name, conditional=True)


# ============================================================
# 管理后台
# ============================================================

@app.route('/admin', methods=['GET', 'POST'])
def admin_login():
    error = None
    ip = _client_ip()
    if request.method == 'POST':
        if not _csrf_ok():
            error = '会话已过期，请刷新页面后重试'
        elif _login_blocked(ip):
            error = '尝试次数过多，请 5 分钟后再试'
        else:
            username = request.form.get('username', '')
            password = request.form.get('password', '')
            if username == config.get('admin_user') and sha256_hex(password) == config.get('admin_password_hash'):
                session['admin_logged_in'] = True
                _login_attempts.pop(ip, None)
                return redirect(url_for('admin_panel'))
            _record_login_failure(ip)
            error = '用户名或密码错误'

    return render_template('admin.html', error=error, login=True, csrf_token=_csrf_token())


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

    return render_template('admin.html', login=False, version_info=version_info,
                           versions=versions, csrf_token=_csrf_token())


@app.route('/admin/update-version', methods=['POST'])
@login_required
def admin_update_version():
    if not _csrf_ok():
        abort(400, description="CSRF token 无效或已过期，请刷新页面后重试")
    latest_version = request.form.get('latest_version', '').strip()
    release_notes = request.form.get('release_notes', '').strip()
    force_update = request.form.get('force_update', 'off') == 'on'
    release_date = request.form.get('release_date', '').strip() or datetime.date.today().isoformat()

    download_url = ''
    sha256 = ''
    uploaded_file = request.files.get('file')
    if uploaded_file and uploaded_file.filename:
        name = safe_filename(uploaded_file.filename)
        file_path = UPLOAD_DIR / name
        # 先写临时文件再 rename，避免上传中断留下损坏的包被直接发布
        tmp_path = UPLOAD_DIR / (name + '.uploading')
        uploaded_file.save(tmp_path)
        os.replace(tmp_path, file_path)
        download_url = f'/downloads/{name}'
        h = hashlib.sha256()
        with open(file_path, 'rb') as f:
            for chunk in iter(lambda: f.read(1024 * 1024), b''):
                h.update(chunk)
        sha256 = h.hexdigest()

    if latest_version:
        data = load_current_version()
        data['latest_version'] = latest_version
        data['display_version'] = latest_version  # 网站展示用（可含 - Beta 等后缀）
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
    host = os.environ.get('HOST', '127.0.0.1')
    print(f"\n  Soul Agent Launcher 更新服务器")
    print(f"  ─────────────────────────────")
    print(f"  监听:      {host}:{port}")
    print(f"  API:       /api/check-update")
    print(f"  管理后台:  /admin")
    print(f"  下载目录:  {UPLOAD_DIR}  ({len(list(UPLOAD_DIR.glob('*.msi')))} 个 MSI)")
    print(f"  启动完成\n", flush=True)
    app.run(host=host, port=port, debug=False)
