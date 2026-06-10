// Soul Agent Launcher - 后端
// Release 构建隐藏控制台窗口（dev 模式仍显示，方便看日志）
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{Emitter, Manager, State};
use tauri::tray::{TrayIconBuilder, MouseButton, MouseButtonState, TrayIconEvent};
use tauri::menu::{MenuBuilder, MenuItemBuilder};

/// 服务状态：单服务器进程 + 代理 + 待机
struct ServerState {
    server: Mutex<Option<std::process::Child>>,
    proxy_running: Mutex<bool>,
    server_starting: Mutex<bool>,
    standby: Mutex<bool>,
    spawn_params: Mutex<Option<SpawnParams>>,
    model_name: Mutex<String>,
}

/// 多模型管理：跟踪所有已启动的模型实例
#[derive(Serialize, Clone)]
struct RunningModel {
    name: String,
    model_path: String,
    proxy_port: u16,
    llama_port: u16,
    pid: String,
    started_at: String,
    ctx: u32,
}

struct RunningModelsState {
    models: Mutex<Vec<RunningModel>>,
}

#[derive(Clone)]
struct SpawnParams {
    llama_path: String,
    model_path: String,
    port: u16,
    ctx: u32,
}

/// 唤醒/休眠监控日志
#[derive(Serialize, Clone)]
struct SleepLogEntry {
    action: String,
    timestamp: String,
    duration_ms: Option<u64>,
    detail: Option<String>,
}

struct SleepMonitor {
    entries: Mutex<Vec<SleepLogEntry>>,
}

fn now_iso() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    // 简单格式化：秒级时间戳转 YYYY-MM-DD HH:MM:SS
    let s = secs as i64 + 8 * 3600; // UTC+8
    let h = (s / 3600) % 24;
    let m = (s / 60) % 60;
    let sec = s % 60;
    let d = s / 86400;
    // 粗略计算日期
    let year = 1970 + (d as f64 / 365.25) as i64;
    let month = 1 + ((d % 365) / 30) as i64;
    let day = 1 + ((d % 365) % 30) as i64;
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", year, month.min(12).max(1), day.min(31).max(1), h, m, sec)
}

fn log_sleep_event(monitor: &State<SleepMonitor>, action: &str, duration_ms: Option<u64>, detail: Option<String>) {
    if let Ok(mut entries) = monitor.entries.lock() {
        entries.push(SleepLogEntry {
            action: action.to_string(),
            timestamp: now_iso(),
            duration_ms,
            detail,
        });
        if entries.len() > 100 {
            entries.remove(0);
        }
    }
}

#[derive(Serialize, Deserialize)]
struct ModelInfo {
    name: String,
    size: String,
    path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LauncherConfig {
    llama_path: String,
    models_dir: String,
    port: u16,
    ctx: u32,
    #[serde(default = "default_true")]
    auto_unload: bool,
}

fn default_true() -> bool { true }

// ============================================================
// Tauri 命令
// ============================================================

/// 检查服务器是否在运行（直接检查 llama-server 端口）
#[tauri::command]
fn check_server(state: State<ServerState>, port: u16) -> Result<bool, String> {
    let mut server_guard = state.server.lock().map_err(|e| e.to_string())?;
    if let Some(ref mut child) = *server_guard {
        match child.try_wait() {
            Ok(Some(_)) => Ok(false),
            Ok(None) => {
                let url = format!("http://127.0.0.1:{}/health", port);
                match reqwest::blocking::get(&url) {
                    Ok(resp) => Ok(resp.status().is_success() || resp.status().as_u16() == 503),
                    Err(_) => Ok(false),
                }
            }
            Err(_) => Ok(false),
        }
    } else {
        Ok(false)
    }
}

/// 检查用户端口是否可达
#[tauri::command]
fn check_server_port(port: u16) -> Result<bool, String> {
    let url = format!("http://127.0.0.1:{}/health", port);
    match reqwest::blocking::get(&url) {
        Ok(resp) => Ok(resp.status().is_success()),
        Err(_) => Ok(false),
    }
}

/// 检查当前是否为待机模式
#[tauri::command]
fn check_standby(state: State<ServerState>) -> Result<bool, String> {
    let s = state.standby.lock().map_err(|e| e.to_string())?;
    Ok(*s)
}

// ============================================================
// 多模型管理：启动/卸载
// ============================================================

/// 启动一个模型实例
#[tauri::command]
fn start_model(
    state: State<RunningModelsState>,
    llama_path: String,
    model_path: String,
    model_name: String,
    port: u16,
    ctx: u32,
) -> Result<String, String> {
    let llama_port = port + 1;

    let child = std::process::Command::new(&llama_path)
        .args(["-m", &model_path, "--host", "127.0.0.1",
               "--port", &llama_port.to_string(), "-c", &ctx.to_string(),
               "-ngl", "99"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("启动模型失败: {}", e))?;

    let pid = child.id();
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap_or_default()
        .as_secs();
    let time_str = format_ts(ts);

    let model = RunningModel {
        name: if model_name.is_empty() {
            std::path::Path::new(&model_path)
                .file_stem().and_then(|s| s.to_str())
                .unwrap_or("unknown").to_string()
        } else { model_name },
        model_path,
        proxy_port: port,
        llama_port,
        pid: pid.to_string(),
        started_at: time_str,
        ctx,
    };

    // 检查该端口的代理是否已存在（避免端口冲突）
    let need_proxy = state.models.lock()
        .map(|m| !m.iter().any(|x| x.proxy_port == port))
        .unwrap_or(true);

    if let Ok(mut models) = state.models.lock() {
        models.push(model.clone());
    }

    // 启动代理线程（如果端口尚未被占）
    if need_proxy {
        let base_name = model.name.clone();
        let public_p = port;
        let llama_p = llama_port;
        std::thread::spawn(move || proxy_loop(public_p, llama_p, &base_name));
    } else {
        eprintln!("[start_model] 端口 {} 已有代理，跳过", port);
    }

    Ok(serde_json::json!(model).to_string())
}

/// 卸载指定模型
#[tauri::command]
fn unload_model(state: State<RunningModelsState>, model_name: String) -> Result<String, String> {
    let mut models = state.models.lock().map_err(|e| e.to_string())?;
    let idx = models.iter().position(|m| m.name == model_name)
        .ok_or_else(|| format!("未找到模型: {}", model_name))?;
    let model = models.remove(idx);

    // 通过 PID kill 进程
    if let Ok(pid) = model.pid.parse::<u32>() {
        #[cfg(windows)]
        {
            let _ = std::process::Command::new("taskkill")
                .args(["/F", "/PID", &pid.to_string()])
                .output();
        }
        #[cfg(not(windows))]
        {
            let _ = std::process::Command::new("kill")
                .args(["-9", &pid.to_string()])
                .output();
        }
    }

    Ok(format!("已卸载模型: {}", model.name))
}

/// 列出所有运行中的模型
#[tauri::command]
fn list_running_models(state: State<RunningModelsState>) -> Result<Vec<RunningModel>, String> {
    let models = state.models.lock().map_err(|e| e.to_string())?;
    Ok(models.clone())
}

/// 停止所有运行中的模型
#[tauri::command]
fn stop_all_models(state: State<RunningModelsState>) -> Result<String, String> {
    let mut models = state.models.lock().map_err(|e| e.to_string())?;
    let count = models.len();
    for model in models.iter() {
        if let Ok(pid) = model.pid.parse::<u32>() {
            let _ = std::process::Command::new("taskkill")
                .args(["/F", "/PID", &pid.to_string()])
                .output();
        }
    }
    models.clear();
    Ok(format!("已停止 {} 个模型", count))
}

/// 启动服务器（启动 llama-server + 代理，支持待机模式）
#[tauri::command]
fn start_server(
    app_handle: tauri::AppHandle,
    state: State<ServerState>,
    llama_path: String,
    model_path: String,
    port: u16,
    ctx: u32,
    standby: bool,
) -> Result<String, String> {
    let mut starting = state.server_starting.lock().map_err(|e| e.to_string())?;
    if *starting {
        return Err("服务器正在启动中".to_string());
    }
    *starting = true;
    drop(starting);

    let mut server_guard = state.server.lock().map_err(|e| e.to_string())?;

    // 检查是否已在运行
    if let Some(ref mut child) = *server_guard {
        match child.try_wait() {
            Ok(Some(_)) => {}
            Ok(None) => {
                if let Ok(mut s) = state.server_starting.lock() {
                    *s = false;
                }
                return Ok("服务器已在运行中".to_string());
            }
            Err(e) => {
                if let Ok(mut s) = state.server_starting.lock() {
                    *s = false;
                }
                return Err(format!("检查服务器状态失败: {}", e));
            }
        }
    }

    // 保存启动参数 + 模型名（供 wake/sleep 和模型列表使用）
    let mname = std::path::Path::new(&model_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("model")
        .to_string();
    if let Ok(mut mn) = state.model_name.lock() {
        *mn = mname.clone();
    }
    if let Ok(mut p) = state.spawn_params.lock() {
        *p = Some(SpawnParams {
            llama_path: llama_path.clone(),
            model_path: model_path.clone(),
            port,
            ctx,
        });
    }
    if let Ok(mut s) = state.standby.lock() {
        *s = standby;
    }

    // 计算 llama-server 内部端口：用户端口 + 1，溢出时用 20001
    let llama_port = if port >= 65535 { 20001u16 } else { port + 1 };

    // 启动 llama-server：待机时不加载模型
    let mut cmd = std::process::Command::new(&llama_path);
    if !standby {
        cmd.arg("-m").arg(&model_path);
    }
    cmd.args(["--host", "127.0.0.1", "--port", &llama_port.to_string(), "-c", &ctx.to_string()]);

    let mut child = cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| {
            let _ = state.server_starting.lock().map(|mut s| *s = false);
            format!("启动服务器失败: {}", e)
        })?;

    let pid = child.id();
    // 先取出 stdout/stderr 再存进程（避免死锁）
    let child_stderr = child.stderr.take();
    let child_stdout = child.stdout.take();
    *server_guard = Some(child);

    // 后台线程读取 stderr（防管道阻塞 + 错误输出显示到控制台）
    if let Some(stderr) = child_stderr {
        let app_clone = app_handle.clone();
        std::thread::spawn(move || {
            let reader = std::io::BufReader::new(stderr);
            for line in reader.lines() {
                if let Ok(text) = line {
                    let trimmed = text.trim().to_string();
                    if !trimmed.is_empty() {
                        let _ = app_clone.emit("server-stderr", serde_json::json!({
                            "message": trimmed,
                        }));
                    }
                }
            }
        });
    }

    // 后台线程读取 stdout（同样防阻塞）
    if let Some(stdout) = child_stdout {
        let app_clone = app_handle.clone();
        std::thread::spawn(move || {
            let reader = std::io::BufReader::new(stdout);
            for line in reader.lines() {
                if let Ok(text) = line {
                    let trimmed = text.trim().to_string();
                    if !trimmed.is_empty() {
                        let _ = app_clone.emit("server-stdout", serde_json::json!({
                            "message": trimmed,
                        }));
                    }
                }
            }
        });
    }

    // 启动代理线程
    let mut proxy_guard = state.proxy_running.lock().map_err(|e| e.to_string())?;
    if !*proxy_guard {
        let proxy_port = port;
        let upstream_port = llama_port;
        let model = mname.clone();
        std::thread::spawn(move || {
            proxy_loop(proxy_port, upstream_port, &model);
        });
        *proxy_guard = true;
    }
    drop(proxy_guard);

    // 释放锁
    drop(server_guard);

    if let Ok(mut s) = state.server_starting.lock() {
        *s = false;
    }

    let mode = if standby { "待机" } else { "运行" };
    Ok(format!("服务器已启动 (PID: {}, 端口: {}, 模式: {})", pid, port, mode))
}

/// 停止服务器
#[tauri::command]
fn stop_server(state: State<ServerState>) -> Result<String, String> {
    let mut server_guard = state.server.lock().map_err(|e| e.to_string())?;
    if let Some(mut child) = server_guard.take() {
        child.kill().map_err(|e| format!("停止服务器失败: {}", e))?;
        child.wait().ok();
        // 清除待机标记
        if let Ok(mut s) = state.standby.lock() { *s = false; }
        Ok("服务器已停止".to_string())
    } else {
        Err("服务器未在运行".to_string())
    }
}

/// 唤醒服务器（从待机切换到加载模型）
#[tauri::command]
fn wake_server(
    _app_handle: tauri::AppHandle,
    state: State<ServerState>,
    monitor: State<SleepMonitor>,
) -> Result<String, String> {
    let params = state.spawn_params.lock().map_err(|e| e.to_string())?
        .clone().ok_or("未找到启动参数，请先启动服务器")?;

    if let Ok(s) = state.standby.lock() {
        if !*s {
            return Ok("服务器已处于唤醒状态".to_string());
        }
    }

    let start = std::time::Instant::now();
    kill_server(&state);
    if let Ok(mut s) = state.standby.lock() { *s = false; }

    let llama_port = if params.port >= 65535 { 20001u16 } else { params.port + 1 };
    let child = std::process::Command::new(&params.llama_path)
        .args(["-m", &params.model_path, "--host", "127.0.0.1",
               "--port", &llama_port.to_string(), "-c", &params.ctx.to_string()])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("唤醒失败: {}", e))?;

    let pid = child.id();
    if let Ok(mut g) = state.server.lock() { *g = Some(child); }

    let elapsed = start.elapsed().as_millis() as u64;
    log_sleep_event(&monitor, "wake", Some(elapsed), Some(format!("PID {}", pid)));

    Ok(format!("模型已加载 (PID: {}, 耗时: {}ms)", pid, elapsed))
}

/// 休眠服务器（卸载模型回到待机，会先检查活跃会话）
#[tauri::command]
fn sleep_server(
    state: State<ServerState>,
    monitor: State<SleepMonitor>,
) -> Result<String, String> {
    let params = state.spawn_params.lock().map_err(|e| e.to_string())?
        .clone().ok_or("未找到启动参数，请先启动服务器")?;

    if let Ok(s) = state.standby.lock() {
        if *s {
            return Ok("服务器已处于待机模式".to_string());
        }
    }

    // 检查活跃会话
    let llama_port = if params.port >= 65535 { 20001u16 } else { params.port + 1 };
    let active_sessions = get_active_session_count(llama_port);
    if active_sessions > 0 {
        let detail = format!("有 {} 个活跃会话，拒绝休眠", active_sessions);
        log_sleep_event(&monitor, "reject_sleep", None, Some(detail.clone()));
        return Err(detail);
    }

    let start = std::time::Instant::now();
    kill_server(&state);
    if let Ok(mut s) = state.standby.lock() { *s = true; }

    let child = std::process::Command::new(&params.llama_path)
        .args(["--host", "127.0.0.1",
               "--port", &llama_port.to_string(), "-c", &params.ctx.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("休眠失败: {}", e))?;

    if let Ok(mut g) = state.server.lock() { *g = Some(child); }

    let elapsed = start.elapsed().as_millis() as u64;
    log_sleep_event(&monitor, "sleep", Some(elapsed), None);

    Ok(format!("已回到待机模式 (耗时: {}ms)", elapsed))
}

/// 检查活跃会话数（通过 llama-server /slots 端点）
#[tauri::command]
fn check_idle(state: State<ServerState>) -> Result<serde_json::Value, String> {
    let params = state.spawn_params.lock().map_err(|e| e.to_string())?
        .clone().ok_or("未找到启动参数")?;
    let llama_port = if params.port >= 65535 { 20001u16 } else { params.port + 1 };
    let active = get_active_session_count(llama_port);
    Ok(serde_json::json!({
        "active_sessions": active,
        "idle": active == 0,
    }))
}

/// 查询 llama-server 活跃会话数
fn get_active_session_count(llama_port: u16) -> usize {
    let url = format!("http://127.0.0.1:{}/slots", llama_port);
    match reqwest::blocking::get(&url) {
        Ok(resp) if resp.status().is_success() => {
            if let Ok(slots) = resp.json::<Vec<serde_json::Value>>() {
                slots.iter()
                    .filter(|s| s["state"].as_str() != Some("idle"))
                    .count()
            } else { 0 }
        }
        _ => 0,
    }
}

/// 获取唤醒/休眠日志
#[tauri::command]
fn get_sleep_logs(monitor: State<SleepMonitor>) -> Result<Vec<SleepLogEntry>, String> {
    let entries = monitor.entries.lock().map_err(|e| e.to_string())?;
    Ok(entries.clone())
}

/// 内部：终止当前服务器进程
fn kill_server(state: &State<ServerState>) {
    if let Ok(mut g) = state.server.lock() {
        if let Some(mut child) = g.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// 极致精简编译 llama-server（运行 build-minimal.ps1）
#[tauri::command]
async fn build_llama_minimal(
    app_handle: tauri::AppHandle,
    script_path: String,
    output_dir: String,
) -> Result<String, String> {
    let _ = app_handle.emit("build-progress", serde_json::json!({
        "progress": 0, "message": "开始构建 llama-server 极致精简版..."
    }));

    let script_path = std::path::Path::new(&script_path);
    if !script_path.exists() {
        return Err(format!("编译脚本不存在: {}", script_path.display()));
    }

    // 创建输出目录
    std::fs::create_dir_all(&output_dir)
        .map_err(|e| format!("创建输出目录失败: {}", e))?;

    let _ = app_handle.emit("build-progress", serde_json::json!({
        "progress": 5, "message": "正在检测构建工具..."
    }));

    // 用 PowerShell 执行编译脚本
    let mut child = std::process::Command::new("powershell")
        .args([
            "-NoProfile", "-ExecutionPolicy", "Bypass",
            "-File", script_path.to_string_lossy().as_ref(),
            "-OutputDir", &output_dir,
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("启动编译失败: {}", e))?;

    // 后台线程读取输出
    let app_clone = app_handle.clone();
    let stdout = child.stdout.take();

    std::thread::spawn(move || {
        if let Some(out) = stdout {
            let reader = BufReader::new(out);
            for line in reader.lines() {
                if let Ok(text) = line {
                    let t = text.trim().to_string();
                    if !t.is_empty() {
                        let _ = app_clone.emit("build-progress", serde_json::json!({
                            "progress": 0, "message": t,
                        }));
                    }
                }
            }
        }
    });

    // 等待编译完成
    let exit_status = child.wait()
        .map_err(|e| format!("等待编译完成失败: {}", e))?;

    if !exit_status.success() {
        return Err("编译失败，请检查控制台输出".to_string());
    }

    // 验证输出文件
    let server_path = std::path::Path::new(&output_dir).join("llama-server.exe");
    if !server_path.exists() {
        return Err("编译完成但未找到 llama-server.exe".to_string());
    }

    let size = std::fs::metadata(&server_path)
        .map(|m| format_size(m.len()))
        .unwrap_or_default();

    let _ = app_handle.emit("build-progress", serde_json::json!({
        "progress": 100,
        "message": format!("构建完成！llama-server 大小: {}", size),
    }));

    Ok(format!("编译成功 ({})", size))
}

/// 列出 models 目录下的 GGUF 模型
#[tauri::command]
fn list_models(models_dir: String) -> Result<Vec<ModelInfo>, String> {
    let dir = std::path::Path::new(&models_dir);
    if !dir.exists() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("创建模型目录失败: {}", e))?;
        return Ok(Vec::new());
    }

    let mut models = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(|e| format!("读取目录失败: {}", e))? {
        let entry = entry.map_err(|e| format!("读取条目失败: {}", e))?;
        let path = entry.path();
        if path.is_file() {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext == "gguf" || ext == "GGUF" {
                let meta = std::fs::metadata(&path).map_err(|e| format!("读取元数据失败: {}", e))?;
                let size = format_size(meta.len());
                models.push(ModelInfo {
                    name: path.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown").to_string(),
                    size,
                    path: path.to_string_lossy().to_string(),
                });
            }
        }
    }

    models.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(models)
}

/// 删除本地模型文件
#[tauri::command]
fn delete_model_file(path: String) -> Result<String, String> {
    let p = std::path::Path::new(&path);

    // 安全检查：只允许删除 .gguf 文件
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
    if ext.to_lowercase() != "gguf" {
        return Err("只能删除 .gguf 文件".to_string());
    }

    if !p.exists() {
        return Err("文件不存在".to_string());
    }

    std::fs::remove_file(p)
        .map_err(|e| format!("删除失败: {}", e))?;

    Ok(format!("已删除: {}", path))
}

/// 自动查找模型目录中的第一个 GGUF 模型
#[tauri::command]
fn auto_find_model(models_dir: String) -> Result<Option<ModelInfo>, String> {
    let dir = std::path::Path::new(&models_dir);
    if !dir.exists() {
        return Ok(None);
    }

    for entry in std::fs::read_dir(dir).map_err(|e| format!("读取目录失败: {}", e))? {
        let entry = entry.map_err(|e| format!("读取条目失败: {}", e))?;
        let path = entry.path();
        if path.is_file() {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext == "gguf" || ext == "GGUF" {
                let meta = std::fs::metadata(&path).map_err(|e| format!("读取元数据失败: {}", e))?;
                let size = format_size(meta.len());
                return Ok(Some(ModelInfo {
                    name: path.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown").to_string(),
                    size,
                    path: path.to_string_lossy().to_string(),
                }));
            }
        }
    }

    Ok(None)
}

// ============================================================
// 首次启动设置
// ============================================================

#[derive(Serialize, Clone)]
struct SetupProgress {
    progress: u32,
    message: String,
}

/// 获取安装目录路径
fn get_install_dir() -> PathBuf {
    let app_data = std::env::var("APPDATA").unwrap_or_else(|_| {
        "C:\\Users\\Default\\AppData\\Roaming".to_string()
    });
    PathBuf::from(app_data).join("Soul-Agent-Launcher").join("llama")
}

/// 获取启动器数据根目录 (%APPDATA%/Soul-Agent-Launcher)
fn get_launcher_dir() -> PathBuf {
    let app_data = std::env::var("APPDATA").unwrap_or_else(|_| {
        "C:\\Users\\Default\\AppData\\Roaming".to_string()
    });
    PathBuf::from(app_data).join("Soul-Agent-Launcher")
}

/// 检查是否已安装
#[tauri::command]
fn check_setup_needed() -> Result<bool, String> {
    let install_dir = get_install_dir();
    let server_exe = install_dir.join("llama-server.exe");
    Ok(!server_exe.exists())
}

/// 运行首次安装（解压 llama.cpp）
#[tauri::command]
async fn run_setup(app_handle: tauri::AppHandle) -> Result<String, String> {
    let install_dir = get_install_dir();

    // 步骤1: 创建安装目录
    emit_progress(&app_handle, 5, "正在创建安装目录...")?;
    std::fs::create_dir_all(&install_dir)
        .map_err(|e| format!("创建目录失败: {}", e))?;

    // 步骤2: 检测 GPU 并选择最佳预编译包
    emit_progress(&app_handle, 10, "正在检测硬件...")?;
    let gpu = detect_gpu_internal();
    eprintln!("[setup] 检测到: {:?}", gpu);

    let (llama_zip_path, cudart_zip_opt) = get_llama_package_path(&gpu.package_key)
        .or_else(|| get_llama_package_path("cpu-x64"))
        .or_else(|| get_llama_package_path("cpu-arm64"))
        .ok_or_else(|| format!(
            "未找到任何预编译包，请先将 llama.cpp 预编译包下载到 Downloads/llama.cpp/ 目录"
        ))?;

    emit_progress(&app_handle, 15, &format!(
        "检测到: {} ({}), 正在安装 {}...",
        gpu.gpu_name,
        if let Some(ref cv) = gpu.cuda_version { format!("CUDA {}", cv) } else { gpu.backend.to_string() },
        gpu.package_key
    ))?;

    // 步骤3: 解压 llama ZIP
    emit_progress(&app_handle, 25, "正在解压 llama.cpp...")?;
    extract_zip(&llama_zip_path, &install_dir, &app_handle)?;

    // 步骤3b: 如果是 CUDA 版本，额外解压 CUDA 运行时
    if let Some(ref cudart_path) = cudart_zip_opt {
        if cudart_path.exists() {
            emit_progress(&app_handle, 55, "正在安装 CUDA 运行时...")?;
            extract_zip(cudart_path, &install_dir, &app_handle)?;
        } else {
            eprintln!("[setup] CUDA 运行时包不存在: {:?}", cudart_path);
        }
    }

    // 步骤4: 验证安装
    emit_progress(&app_handle, 80, "正在验证安装...")?;
    let server_exe = install_dir.join("llama-server.exe");
    if !server_exe.exists() {
        return Err("安装失败: 未找到 llama-server.exe".to_string());
    }

    // 保存设备信息
    let launcher_dir = install_dir.parent().unwrap_or(&install_dir);
    let device_path = launcher_dir.join("device.json");
    let device_json = serde_json::to_string_pretty(&serde_json::json!({
        "backend": gpu.backend,
        "package_key": gpu.package_key,
        "gpu_name": gpu.gpu_name,
        "cuda_version": gpu.cuda_version,
        "installed_at": "now",
    })).map_err(|e| format!("序列化设备信息失败: {}", e))?;
    std::fs::write(&device_path, device_json)
        .map_err(|e| format!("保存设备信息失败: {}", e))?;

    // 步骤5: 创建模型目录和令牌目录
    emit_progress(&app_handle, 88, "正在创建模型目录...")?;
    let models_dir = launcher_dir.join("models");
    std::fs::create_dir_all(&models_dir)
        .map_err(|e| format!("创建模型目录失败: {}", e))?;
    let tokens_dir = launcher_dir.join("tokens");
    std::fs::create_dir_all(&tokens_dir)
        .map_err(|e| format!("创建令牌目录失败: {}", e))?;

    emit_progress(&app_handle, 93, "正在保存配置...")?;
    let config_path = launcher_dir.join("config.json");
    let config = serde_json::json!({
        "llama_path": server_exe.to_string_lossy().to_string(),
        "models_dir": models_dir.to_string_lossy().to_string(),
        "port": 20000,
        "ctx": 4096,
        "backend": gpu.backend,
    });
    std::fs::write(&config_path, serde_json::to_string_pretty(&config).unwrap())
        .map_err(|e| format!("保存配置失败: {}", e))?;

    emit_progress(&app_handle, 100, &format!("安装完成！使用 {} 后端", gpu.package_key))?;
    Ok(server_exe.to_string_lossy().to_string())
}

/// 解压 ZIP 到目标目录
fn extract_zip(zip_path: &std::path::Path, target_dir: &std::path::Path, app_handle: &tauri::AppHandle) -> Result<(), String> {
    let zip_file = std::fs::File::open(zip_path)
        .map_err(|e| format!("打开压缩包 {:?} 失败: {}", zip_path, e))?;
    let mut archive = zip::ZipArchive::new(zip_file)
        .map_err(|e| format!("读取压缩包 {:?} 失败: {}", zip_path, e))?;

    let total = archive.len();
    for i in 0..total {
        let mut entry = archive.by_index(i)
            .map_err(|e| format!("读取压缩条目失败: {}", e))?;
        let out_path = target_dir.join(entry.name());
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        if !entry.is_dir() {
            let mut outfile = std::fs::File::create(&out_path)
                .map_err(|e| format!("创建文件失败 [{}]: {}", out_path.display(), e))?;
            std::io::copy(&mut entry, &mut outfile)
                .map_err(|e| format!("写入文件失败 [{}]: {}", out_path.display(), e))?;
        }
        if i % 10 == 0 || i == total - 1 {
            let p = 25 + ((i as f64 / total as f64) * 50.0) as u32;
            emit_progress(app_handle, p, &format!("正在解压... ({}/{})", i + 1, total))?;
        }
    }
    Ok(())
}

fn emit_progress(app_handle: &tauri::AppHandle, progress: u32, message: &str) -> Result<(), String> {
    let payload = SetupProgress {
        progress,
        message: message.to_string(),
    };
    app_handle.emit("setup-progress", payload)
        .map_err(|e| format!("发送进度事件失败: {}", e))
}

// ============================================================
// GPU / 硬件检测
// ============================================================

/// GPU 信息
#[derive(Serialize, Clone, Debug)]
struct GpuInfo {
    backend: String,
    gpu_name: String,
    cuda_version: Option<String>,
    package_key: String,
}

/// 检测 GPU 和 CUDA 版本
#[tauri::command]
fn detect_gpu() -> GpuInfo {
    detect_gpu_internal()
}

fn detect_gpu_internal() -> GpuInfo {
    // 只调用 nvidia-smi 一次，避免重复调用
    if let Ok(output) = std::process::Command::new("nvidia-smi")
        .args(["--query-gpu=name,driver_version", "--format=csv,noheader"])
        .output()
    {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            let gpu_name = text.trim().to_string();
            let driver_ver = gpu_name.split(',').nth(1).unwrap_or("").trim();
            // 从驱动版本推断 CUDA 版本（驱动 560+ → CUDA 12.x, 530+ → CUDA 12.x）
            let major = driver_ver.split('.').next().unwrap_or("0").parse::<u32>().unwrap_or(0);
            let (cuda_ver, package_key) = if major >= 570 {
                ("13.0".to_string(), "cuda-13.3".to_string())
            } else if major >= 525 {
                ("12.5".to_string(), "cuda-12.4".to_string())
            } else {
                ("12.0".to_string(), "cuda-12.4".to_string())
            };
            return GpuInfo {
                backend: "cuda".to_string(),
                gpu_name,
                cuda_version: Some(cuda_ver),
                package_key: package_key.to_string(),
            };
        }
    }

    // Vulkan 检测较重，只在没有 NVIDIA 时才尝试
    if check_vulkan_available() {
        return GpuInfo {
            backend: "vulkan".to_string(),
            gpu_name: "Vulkan GPU".to_string(),
            cuda_version: None,
            package_key: "vulkan-x64".to_string(),
        };
    }

    let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "x64" };
    GpuInfo {
        backend: "cpu".to_string(),
        gpu_name: format!("CPU ({})", arch),
        cuda_version: None,
        package_key: format!("cpu-{}", arch),
    }
}

fn check_vulkan_available() -> bool {
    // vulkaninfo 可能在某些驱动环境下永久卡住，用子进程 + 超时保护
    let mut child = match std::process::Command::new("vulkaninfo")
        .arg("--summary")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return false,
    };

    // 最多等 5 秒，超时直接杀掉
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) => {
                if start.elapsed().as_secs() > 5 {
                    let _ = child.kill();
                    let _ = child.wait();
                    return false;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            Err(_) => {
                let _ = child.kill();
                return false;
            }
        }
    }
}

fn get_llama_package_path(package_key: &str) -> Option<(PathBuf, Option<PathBuf>)> {
    let downloads_base: PathBuf = [
        &std::env::var("USERPROFILE").unwrap_or_else(|_| "C:\\Users\\Default".to_string()),
        "Downloads", "llama.cpp",
    ].iter().collect();

    let (llama_zip, cudart_zip) = match package_key {
        "cuda-12.4" => (
            downloads_base.join("llama-b9568-bin-win-cuda-12.4-x64.zip"),
            Some(downloads_base.join("cudart-llama-bin-win-cuda-12.4-x64.zip")),
        ),
        "cuda-13.3" => (
            downloads_base.join("llama-b9568-bin-win-cuda-13.3-x64.zip"),
            Some(downloads_base.join("cudart-llama-bin-win-cuda-13.3-x64.zip")),
        ),
        "vulkan-x64" => (
            downloads_base.join("llama-b9568-bin-win-vulkan-x64.zip"),
            None,
        ),
        "hip-radeon-x64" => (
            downloads_base.join("llama-b9568-bin-win-hip-radeon-x64.zip"),
            None,
        ),
        "cpu-arm64" => (
            downloads_base.join("llama-b9568-bin-win-cpu-arm64.zip"),
            None,
        ),
        _ => (
            downloads_base.join("llama-b9568-bin-win-cpu-x64.zip"),
            None,
        ),
    };

    if llama_zip.exists() {
        Some((llama_zip, cudart_zip))
    } else {
        None
    }
}

/// 检查已安装的版本与当前设备是否匹配
#[tauri::command]
fn check_llama_installed() -> Result<serde_json::Value, String> {
    let install_dir = get_install_dir();
    let server_exe = install_dir.join("llama-server.exe");
    let installed = server_exe.exists();
    let gpu = detect_gpu_internal();
    let device_path = get_launcher_dir().join("device.json");

    let mut update_needed = false;
    if let Ok(content) = std::fs::read_to_string(&device_path) {
        if let Ok(saved) = serde_json::from_str::<serde_json::Value>(&content) {
            let old_key = saved["package_key"].as_str().unwrap_or("").to_string();
            if installed && old_key != gpu.package_key {
                update_needed = true;
            }
        }
    } else if installed {
        if let Ok(device_json) = serde_json::to_string_pretty(&serde_json::json!({
            "backend": gpu.backend,
            "package_key": gpu.package_key,
            "gpu_name": gpu.gpu_name,
            "cuda_version": gpu.cuda_version,
        })) {
            std::fs::write(&device_path, device_json).ok();
        }
    }

    let packages = ["cpu-x64", "cpu-arm64", "cuda-12.4", "cuda-13.3", "vulkan-x64", "hip-radeon-x64"];
    let available: Vec<&str> = packages.iter()
        .filter(|p| get_llama_package_path(p).is_some())
        .copied()
        .collect();

    Ok(serde_json::json!({
        "backend": gpu.backend,
        "package_key": gpu.package_key,
        "gpu_name": gpu.gpu_name,
        "cuda_version": gpu.cuda_version,
        "installed": installed,
        "update_needed": update_needed,
        "available_packages": available,
    }))
}

/// 读取已保存的配置（安装后自动加载）
#[tauri::command]
fn load_config() -> Result<Option<LauncherConfig>, String> {
    let launcher_dir = get_install_dir().parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(get_install_dir);
    let config_path = launcher_dir.join("config.json");
    if !config_path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&config_path)
        .map_err(|e| format!("读取配置失败: {}", e))?;
    let config: LauncherConfig = serde_json::from_str(&content)
        .map_err(|e| format!("解析配置失败: {}", e))?;
    Ok(Some(config))
}

/// 保存配置
#[tauri::command]
fn save_config(config: LauncherConfig) -> Result<(), String> {
    let launcher_dir = get_install_dir().parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(get_install_dir);
    let config_path = launcher_dir.join("config.json");
    let content = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("序列化配置失败: {}", e))?;
    std::fs::write(&config_path, content)
        .map_err(|e| format!("写入配置失败: {}", e))?;
    Ok(())
}

// ============================================================
// 魔塔社区 (ModelScope) 集成
// ============================================================

/// 下载进程跟踪（仅存 PID，进程对象留在后台线程中）
struct DownloadProcess {
    pid: Mutex<Option<u32>>,
}

#[derive(Serialize)]
struct ModelScopeModelBrief {
    id: String,
    name: String,
    owner: String,
    description: String,
    task: String,
    downloads: i64,
    updated: String,
    path: String,
}

/// 搜索魔塔社区模型 — 只解析我们需要的字段，忽略其余巨大字段
#[allow(non_snake_case)]
#[derive(Deserialize, Default)]
#[serde(default)]
struct ModelRaw {
    Name: Option<String>,
    Path: Option<String>,
    Downloads: Option<i64>,
    UpdatedAt: Option<String>,
    ChineseName: Option<String>,
    Description: Option<String>,
    CreatedBy: Option<String>,
    /// Organization 是嵌套对象，我们只取 FullName
    Organization: Option<OrgRaw>,
    /// Tasks 是数组，我们只取第一个的 ChineseName
    Tasks: Option<Vec<TaskRaw>>,
}
#[allow(non_snake_case)]
#[derive(Deserialize, Default)]
#[serde(default)]
struct OrgRaw { FullName: Option<String> }
#[allow(non_snake_case)]
#[derive(Deserialize, Default)]
#[serde(default)]
struct TaskRaw { ChineseName: Option<String> }

#[allow(non_snake_case)]
#[derive(Deserialize)]
struct SearchResponse {
    #[serde(rename = "Data")]
    data: Option<SearchData>,
}
#[allow(non_snake_case)]
#[derive(Deserialize)]
struct SearchData {
    #[serde(rename = "Models")]
    models: Option<Vec<ModelRaw>>,
}

#[tauri::command]
async fn search_models(query: String) -> Result<Vec<ModelScopeModelBrief>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("Mozilla/5.0 SoulAgentLauncher/0.1.0")
        .build()
        .map_err(|e| format!("创建客户端失败: {}", e))?;

    // 读取 modelscope token
    let token_path = get_launcher_dir().join("tokens").join("modelscope.token");

    let body = serde_json::json!({
        "page_number": 1, "page_size": 10, "name": query,
    });
    let mut req = client
        .put("https://www.modelscope.cn/api/v1/models")
        .header("Content-Type", "application/json")
        .json(&body);

    if let Ok(content) = std::fs::read_to_string(&token_path) {
        let token = content.trim().to_string();
        if !token.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", token));
        }
    }

    let resp = req.send().await
        .map_err(|e| format!("网络请求失败: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body_preview = resp.text().await.unwrap_or_default();
        let preview: String = body_preview.chars().take(200).collect();
        return Err(format!("API 返回错误 ({}): {}", status, preview));
    }

    // 直接反序列化为 SearchResponse（只解析我们声明的字段）
    let parsed: SearchResponse = resp.json().await
        .map_err(|e| format!("解析响应失败: {}", e))?;

    let models = parsed.data
        .and_then(|d| d.models)
        .unwrap_or_default();

    if models.is_empty() {
        return Ok(Vec::new());
    }

    let query_lower = query.to_lowercase();

    let mut results: Vec<ModelScopeModelBrief> = models.into_iter()
        .filter(|m| {
            m.Name.as_deref()
                .map(|n| n.to_lowercase().contains(&query_lower))
                .unwrap_or(false)
        })
        .map(|m| {
            let name = m.Name.unwrap_or_default();
            let path = m.Path.unwrap_or_default();
            let owner = m.Organization
                .and_then(|o| o.FullName)
                .or(m.CreatedBy)
                .unwrap_or_default();
            let task = m.Tasks
                .and_then(|t| t.into_iter().next())
                .and_then(|t| t.ChineseName)
                .unwrap_or_default();
            ModelScopeModelBrief {
                id: path.clone(),
                name,
                owner,
                description: m.ChineseName.or(m.Description).unwrap_or_default(),
                task,
                downloads: m.Downloads.unwrap_or(0),
                updated: m.UpdatedAt.unwrap_or_default(),
                path,
            }
        })
        .collect();

    // 按下载量排序（最热门的在前面）
    results.sort_by(|a, b| b.downloads.cmp(&a.downloads));

    Ok(results)
}

/// 官方模型描述（从 JSON 文件解析）
#[derive(Serialize, Deserialize)]
struct OfficialModel {
    model_id: String,
    name: String,
    /// 第一级分组（大分类，如 "Qwen系列"）；为空时仅用 family
    #[serde(default)]
    group: Option<String>,
    /// 第二级分组（小分类，如 "Qwen3"、"Qwen2.5"）
    family: String,
    #[serde(default)]
    prefix: Option<String>,
    #[serde(default)]
    include: Option<String>,
    #[serde(default)]
    quant: Option<Vec<String>>,
    #[serde(default)]
    default_quant: Option<String>,
    #[serde(default)]
    size: Option<String>,
    #[serde(default)]
    desc: Option<String>,
}

/// 列出官方推荐模型（扫描 official-models/ 目录下的所有 .json 文件 + 内建默认列表）
#[tauri::command]
fn list_official_models() -> Result<Vec<OfficialModel>, String> {
    let mut models = get_default_official_models();

    // 额外补充：读取磁盘上的用户自定义官方模型 JSON 文件
    let base_dir = get_launcher_dir().join("official-models");
    if base_dir.exists() {
        fn visit_dir(dir: &std::path::Path, models: &mut Vec<OfficialModel>) -> Result<(), String> {
            for entry in std::fs::read_dir(dir).map_err(|e| format!("读取目录失败: {}", e))? {
                let entry = entry.map_err(|e| format!("读取条目失败: {}", e))?;
                let path = entry.path();
                if path.is_dir() {
                    visit_dir(&path, models)?;
                } else if path.extension().and_then(|e| e.to_str()) == Some("json") {
                    let content = std::fs::read_to_string(&path)
                        .map_err(|e| format!("读取 {} 失败: {}", path.display(), e))?;
                    match serde_json::from_str::<OfficialModel>(&content) {
                        Ok(m) => models.push(m),
                        Err(e) => eprintln!("解析 {} 失败: {}", path.display(), e),
                    }
                }
            }
            Ok(())
        }
        visit_dir(&base_dir, &mut models)?;
    }

    // 按 family 优先级排序（新系列优先），同 family 内：GGUF 量化版在前，再按参数大小升序
    let family_priority: &[&str] = &["DeepSeek R1 Distill", "DeepSeek Coder", "Gemma 3", "Llama", "Qwen3", "Qwen2.5"];
    models.sort_by(|a, b| {
        let pa = family_priority.iter().position(|&f| f == a.family).unwrap_or(usize::MAX);
        let pb = family_priority.iter().position(|&f| f == b.family).unwrap_or(usize::MAX);
        // GGUF（有 quant 选项）排在 FP16（无 quant）之前
        let fmt_a = if a.quant.is_some() { 0 } else { 1 };
        let fmt_b = if b.quant.is_some() { 0 } else { 1 };
        pa.cmp(&pb).then(fmt_a.cmp(&fmt_b)).then(
            parse_size_to_float(a.size.as_deref().unwrap_or(""))
                .partial_cmp(&parse_size_to_float(b.size.as_deref().unwrap_or("")))
                .unwrap_or(std::cmp::Ordering::Equal)
        )
    });
    Ok(models)
}

/// 从参数字符串（如 "7B"、"30B-A3B"、"0.6B"）中提取数字部分转为 f64，用于排序
fn parse_size_to_float(size: &str) -> f64 {
    let s = size.trim();
    // 取第一个 'B' 之前的部分作为数值
    if let Some(idx) = s.find('B') {
        let num_str = &s[..idx];
        num_str.parse::<f64>().unwrap_or(f64::MAX)
    } else {
        f64::MAX // 无法解析的放最后
    }
}

/// 内建默认官方模型列表（新版在上）
fn get_default_official_models() -> Vec<OfficialModel> {
    let quants = vec![
        "q2_K".into(), "q3_K_M".into(), "q4_K_M".into(),
        "q5_K_M".into(), "q6_K".into(), "q8_0".into(),
    ];

    let mut all = Vec::new();

    // ── DeepSeek 系列 ──
    // GGUF 版本由 unsloth 社区提供（原始 deepseek-ai 仓库无 GGUF）
    for (model_id, name, size, desc) in &[
        ("unsloth/DeepSeek-R1-Distill-Qwen-1.5B-GGUF",   "DeepSeek-R1-Distill-Qwen-1.5B",  "1.5B", "轻量代码补全与基础问答"),
        ("unsloth/DeepSeek-R1-Distill-Qwen-7B-GGUF",     "DeepSeek-R1-Distill-Qwen-7B",    "7B",   "日常对话与工具调用助手"),
        ("unsloth/DeepSeek-R1-Distill-Llama-8B-GGUF",    "DeepSeek-R1-Distill-Llama-8B",   "8B",   "复杂逻辑推理与代码分析"),
        ("unsloth/DeepSeek-R1-Distill-Qwen-14B-GGUF",    "DeepSeek-R1-Distill-Qwen-14B",   "14B",  "长文本理解与多步规划"),
        ("unsloth/DeepSeek-R1-Distill-Qwen-32B-GGUF",    "DeepSeek-R1-Distill-Qwen-32B",   "32B",  "专业级数学证明与深度思考"),
    ] {
        all.push(OfficialModel {
            model_id: format!("{}:*", model_id),
            name: name.to_string(),
            group: Some("DeepSeek".into()),
            family: "DeepSeek R1 Distill".into(),
            prefix: Some(format!("{}:", model_id)),
            include: Some("*q4_k_m*.gguf".into()),
            quant: Some(quants.clone()),
            default_quant: Some("q4_K_M".into()),
            size: Some(size.to_string()),
            desc: Some(desc.to_string()),
        });
    }

    // ── DeepSeek Coder 系列（官方 FP16 + 社区 GGUF）──
    // FP16 原始版（由 deepseek-ai 发布）
    for (model_id, name, size, family, desc) in &[
        ("deepseek-ai/deepseek-coder-1.3b-instruct",   "DeepSeek-Coder-1.3B-Instruct",  "1.3B", "DeepSeek Coder", "代码 Instruct 版，FP16 原始格式"),
        ("deepseek-ai/deepseek-coder-1.3b-base",       "DeepSeek-Coder-1.3B-Base",      "1.3B", "DeepSeek Coder", "代码 Base 版，FP16 原始格式"),
        ("deepseek-ai/deepseek-coder-5.7bmqa-instruct","DeepSeek-Coder-5.7BMQA-Instruct","5.7B", "DeepSeek Coder", "代码 Instruct 版，FP16 原始格式"),
        ("deepseek-ai/deepseek-coder-5.7bmqa-base",    "DeepSeek-Coder-5.7BMQA-Base",    "5.7B", "DeepSeek Coder", "代码 Base 版，FP16 原始格式"),
        ("deepseek-ai/deepseek-coder-6.7b-instruct",   "DeepSeek-Coder-6.7B-Instruct",  "6.7B", "DeepSeek Coder", "代码 Instruct 版，FP16 原始格式"),
        ("deepseek-ai/deepseek-coder-6.7b-base",       "DeepSeek-Coder-6.7B-Base",      "6.7B", "DeepSeek Coder", "代码 Base 版，FP16 原始格式"),
        ("deepseek-ai/deepseek-coder-33b-instruct",    "DeepSeek-Coder-33B-Instruct",   "33B",  "DeepSeek Coder", "代码 Instruct 版，FP16 原始格式"),
        ("deepseek-ai/deepseek-coder-33b-base",        "DeepSeek-Coder-33B-Base",       "33B",  "DeepSeek Coder", "代码 Base 版，FP16 原始格式"),
    ] {
        all.push(OfficialModel {
            model_id: model_id.to_string(),
            name: name.to_string(),
            group: Some("DeepSeek".into()),
            family: family.to_string(),
            prefix: None,
            include: None,
            quant: None,
            default_quant: None,
            size: Some(size.to_string()),
            desc: Some(desc.to_string()),
        });
    }

    // GGUF 社区量化版（由 TheBloke 提供）
    for (model_id, name, size, family, mid_prefix, desc) in &[
        ("TheBloke/deepseek-coder-1.3b-instruct-GGUF", "DeepSeek-Coder-1.3B-Instruct (GGUF)", "1.3B", "DeepSeek Coder", "deepseek-coder-1.3b-instruct.", "GGUF 量化版，推荐 Q4_K_M"),
        ("TheBloke/deepseek-coder-1.3b-base-GGUF",     "DeepSeek-Coder-1.3B-Base (GGUF)",     "1.3B", "DeepSeek Coder", "deepseek-coder-1.3b-base.",     "GGUF 量化版，推荐 Q4_K_M"),
        ("TheBloke/deepseek-coder-6.7b-instruct-GGUF", "DeepSeek-Coder-6.7B-Instruct (GGUF)", "6.7B", "DeepSeek Coder", "deepseek-coder-6.7b-instruct.", "GGUF 量化版，推荐 Q4_K_M"),
        ("TheBloke/deepseek-coder-6.7b-base-GGUF",     "DeepSeek-Coder-6.7B-Base (GGUF)",     "6.7B", "DeepSeek Coder", "deepseek-coder-6.7b-base.",     "GGUF 量化版，推荐 Q4_K_M"),
    ] {
        all.push(OfficialModel {
            model_id: format!("{}:*", model_id),
            name: name.to_string(),
            group: Some("DeepSeek".into()),
            family: family.to_string(),
            prefix: Some(mid_prefix.to_string()),
            include: Some("*Q4_K_M*.gguf".into()),
            quant: Some(quants.clone()),
            default_quant: Some("q4_K_M".into()),
            size: Some(size.to_string()),
            desc: Some(desc.to_string()),
        });
    }

    // ── Gemma 3 系列（GGUF 由 unsloth 社区提供）──
    for (model_id, name, size, desc) in &[
        ("unsloth/gemma-3-270m-it-GGUF", "Gemma 3 270M", "0.27B", "超轻量纯文本，543MB"),
        ("unsloth/gemma-3-1b-it-GGUF",   "Gemma 3 1B",   "1B",    "轻量纯文本，2.04GB"),
        ("unsloth/gemma-3-4b-it-GGUF",   "Gemma 3 4B",   "4B",    "轻量纯文本，8.64GB"),
        ("unsloth/gemma-3-12b-it-GGUF",  "Gemma 3 12B",  "12B",   "多模态（文本+视觉），24.41GB"),
        ("unsloth/gemma-3-27b-it-GGUF",  "Gemma 3 27B",  "27B",   "多模态（文本+视觉），54.90GB"),
    ] {
        all.push(OfficialModel {
            model_id: format!("{}:*", model_id),
            name: name.to_string(),
            group: Some("Google".into()),
            family: "Gemma 3".into(),
            prefix: Some(format!("{}:", model_id)),
            include: Some("*q4_k_m*.gguf".into()),
            quant: Some(quants.clone()),
            default_quant: Some("q4_K_M".into()),
            size: Some(size.to_string()),
            desc: Some(desc.to_string()),
        });
    }

    // ── Llama 系列 ──
    for (model_id, name, size, family, desc) in &[
        ("unsloth/Llama-3.2-1B-Instruct-GGUF",  "Llama-3.2-1B-Instruct",  "1B", "Llama 3.2", "轻量，适合端侧部署"),
        ("unsloth/Llama-3.2-3B-Instruct-GGUF",  "Llama-3.2-3B-Instruct",  "3B", "Llama 3.2", "轻量高性价比"),
        ("LLM-Research/Meta-Llama-3-8B-Instruct-GGUF","Meta-Llama-3-8B-Instruct","8B", "Llama 3",   "通用推理，性能强劲"),
    ] {
        all.push(OfficialModel {
            model_id: format!("{}:*", model_id),
            name: name.to_string(),
            group: Some("Llama".into()),
            family: family.to_string(),
            prefix: Some(format!("{}:", model_id)),
            include: Some("*q4_k_m*.gguf".into()),
            quant: Some(quants.clone()),
            default_quant: Some("q4_K_M".into()),
            size: Some(size.to_string()),
            desc: Some(desc.to_string()),
        });
    }

    // ── Llama 2 系列（FP16 原始格式 + GGUF 量化版）──
    for (model_id, name, size, family, has_quant, desc) in &[
        // Llama 2 原始 FP16
        ("modelscope/Llama-2-7b-chat-ms",         "Llama-2-7b-chat-ms (FP16)",    "7B",  "Llama 2", "false", "原始 FP16 格式，需自行转 GGUF"),
        ("modelscope/Llama-2-13b-ms",             "Llama-2-13b-ms (FP16)",       "13B", "Llama 2", "false", "原始 FP16 格式，需自行转 GGUF"),
        ("AI-ModelScope/chinese-alpaca-2-7b",     "Chinese-Alpaca-2-7B (FP16)",  "7B",  "Llama 2", "false", "中文优化版，原始 FP16 格式"),
        ("AI-ModelScope/chinese-alpaca-2-13b",    "Chinese-Alpaca-2-13B (FP16)", "13B", "Llama 2", "false", "中文优化版，原始 FP16 格式"),
        // Llama 2 GGUF 量化版
        ("Xorbits/Llama-2-7b-Chat-GGUF",          "Llama-2-7b-chat (GGUF)",      "7B",  "Llama 2", "true",  "GGUF 量化版，支持多量化选择"),
        ("Xorbits/Llama-2-13b-Chat-GGUF",         "Llama-2-13b-chat (GGUF)",     "13B", "Llama 2", "true",  "GGUF 量化版，支持多量化选择"),
        ("shaowenchen/chinese-alpaca-2-7b-gguf",  "Chinese-Alpaca-2-7B (GGUF)",  "7B",  "Llama 2", "true",  "中文优化 GGUF 版，支持多量化选择"),
    ] {
        let is_quant = *has_quant == "true";
        // GGUF 版使用 `:*` 版本通配 + 量化 include；FP16 版不处理
        let mid = if is_quant { format!("{}:*", model_id) } else { model_id.to_string() };
        all.push(OfficialModel {
            model_id: mid,
            name: name.to_string(),
            group: Some("Llama".into()),
            family: family.to_string(),
            prefix: if is_quant { Some(format!("{}:", model_id)) } else { None },
            include: if is_quant { Some("*q4_k_m*.gguf".into()) } else { None },
            quant: if is_quant { Some(quants.clone()) } else { None },
            default_quant: if is_quant { Some("q4_K_M".into()) } else { None },
            size: Some(size.to_string()),
            desc: Some(desc.to_string()),
        });
    }

    // ── Qwen3 系列 ──
    for (model_id, name, size, desc) in &[
        ("qwen/Qwen3-0.6B-GGUF",    "Qwen3-0.6B-Instruct",     "0.6B",   "超轻量，简单推理无压力"),
        ("qwen/Qwen3-1.7B-GGUF",    "Qwen3-1.7B-Instruct",     "1.7B",   "低配设备友好，够用省资源"),
        ("qwen/Qwen3-4B-GGUF",      "Qwen3-4B-Instruct",       "4B",     "轻量高性价比，平衡之选"),
        ("qwen/Qwen3-8B-GGUF",      "Qwen3-8B-Instruct",       "8B",     "通用推理首选，推荐 q4_K_M"),
        ("qwen/Qwen3-14B-GGUF",     "Qwen3-14B-Instruct",      "14B",    "中大规模，复杂推理需合并分卷"),
        ("qwen/Qwen3-32B-GGUF",     "Qwen3-32B-Instruct",      "32B",    "大规模深度推理，约需 20GB 显存"),
        ("qwen/Qwen3-30B-A3B-GGUF", "Qwen3-30B-A3B-Instruct",  "30B-A3B","MoE 架构，激活 3B，边缘设备神器"),
    ] {
        all.push(OfficialModel {
            model_id: format!("{}:*", model_id),
            name: name.to_string(),
            group: Some("Qwen系列".into()),
            family: "Qwen3".into(),
            prefix: Some(format!("{}:", model_id)),
            include: Some("*q4_k_m*.gguf".into()),
            quant: Some(quants.clone()),
            default_quant: Some("q4_K_M".into()),
            size: Some(size.to_string()),
            desc: Some(desc.to_string()),
        });
    }

    // ── Qwen2.5 系列 ──
    for (model_id, name, size, desc) in &[
        ("Qwen/Qwen2.5-0.5B-Instruct-GGUF",  "Qwen2.5-0.5B-Instruct",  "0.5B",  "超轻量入门，极致小巧"),
        ("Qwen/Qwen2.5-1.5B-Instruct-GGUF",  "Qwen2.5-1.5B-Instruct",  "1.5B",  "轻量简单推理，起步友好"),
        ("Qwen/Qwen2.5-3B-Instruct-GGUF",    "Qwen2.5-3B-Instruct",    "3B",    "轻量高性价比，够用不贵"),
        ("Qwen/Qwen2.5-7B-Instruct-GGUF",    "Qwen2.5-7B-Instruct",    "7B",    "通用推理主流，稳定可靠"),
        ("Qwen/Qwen2.5-14B-Instruct-GGUF",   "Qwen2.5-14B-Instruct",   "14B",   "中大规模，复杂任务合适"),
        ("Qwen/Qwen2.5-32B-Instruct-GGUF",   "Qwen2.5-32B-Instruct",   "32B",   "大规模深度推理，性能强劲"),
        ("Qwen/Qwen2.5-72B-Instruct-GGUF",   "Qwen2.5-72B-Instruct",   "72B",   "超大规模，追求最高质量"),
    ] {
        all.push(OfficialModel {
            model_id: format!("{}:*", model_id),
            name: name.to_string(),
            group: Some("Qwen系列".into()),
            family: "Qwen2.5".into(),
            prefix: Some(format!("{}:", model_id)),
            include: Some("*.gguf".into()),
            quant: Some(quants.clone()),
            default_quant: Some("q4_K_M".into()),
            size: Some(size.to_string()),
            desc: Some(desc.to_string()),
        });
    }

    all
}

/// 检查 modelscope CLI 是否可用
#[tauri::command]
fn check_modelscope_available() -> bool {
    std::process::Command::new("modelscope")
        .arg("--help")
        .output()
        .is_ok()
}

/// 自动安装 modelscope SDK（静默安装，发射进度事件）
#[tauri::command]
async fn install_modelscope(app_handle: tauri::AppHandle) -> Result<String, String> {
    // 先检查是否已安装
    if check_modelscope_available() {
        let _ = app_handle.emit("ms-install-progress", serde_json::json!({
            "progress": 100, "message": "已安装"
        }));
        return Ok("已安装".to_string());
    }

    let _ = app_handle.emit("ms-install-progress", serde_json::json!({
        "progress": 10, "message": "正在安装 modelscope SDK..."
    }));

    // 使用阿里云镜像安装 modelscope
    let child = std::process::Command::new("pip")
        .args(["install", "modelscope", "-i", "https://mirrors.aliyun.com/pypi/simple/"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("启动 pip 安装失败: {}", e))?;

    // 等待安装完成
    let output = child.wait_with_output()
        .map_err(|e| format!("等待安装完成失败: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let preview: String = stderr.chars().take(300).collect();
        return Err(format!("安装 modelscope 失败: {}", preview));
    }

    // 验证安装
    if check_modelscope_available() {
        let _ = app_handle.emit("ms-install-progress", serde_json::json!({
            "progress": 100, "message": "modelscope 安装完成！"
        }));
        Ok("安装成功".to_string())
    } else {
        Err("安装后验证失败，请尝试手动安装: pip install modelscope".to_string())
    }
}

/// 获取模型文件列表
#[tauri::command]
fn list_model_files(owner: String, name: String) -> Result<Vec<String>, String> {
    let url = format!("https://www.modelscope.cn/api/v1/models/{}/{}/repo/files", owner, name);
    let resp = reqwest::blocking::get(&url)
        .map_err(|e| format!("请求文件列表失败: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("获取文件列表失败: {}", resp.status()));
    }

    let json: serde_json::Value = resp.json().map_err(|e| format!("解析失败: {}", e))?;
    let files = json["Data"].as_array()
        .ok_or_else(|| "文件列表格式异常".to_string())?;

    let paths: Vec<String> = files.iter()
        .filter_map(|f| f["Path"].as_str().or_else(|| f["name"].as_str()))
        .map(|s| s.to_string())
        .collect();

    Ok(paths)
}

/// 读取令牌文件（tokens/{name}.token）
#[tauri::command]
fn read_token(name: String) -> Result<Option<String>, String> {
    let token_path = get_launcher_dir().join("tokens").join(format!("{}.token", name));
    if !token_path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&token_path)
        .map_err(|e| format!("读取令牌文件失败: {}", e))?;
    Ok(Some(content.trim().to_string()))
}

/// 开始下载模型
#[tauri::command]
async fn start_model_download(
    app_handle: tauri::AppHandle,
    state: State<'_, DownloadProcess>,
    model_id: String,
    local_dir: String,
    include_pattern: Option<String>,
) -> Result<String, String> {
    let mut pid_guard = state.pid.lock().map_err(|e| e.to_string())?;

    if let Some(pid) = *pid_guard {
        // 检查进程是否还在运行
        #[cfg(windows)]
        {
            let check = std::process::Command::new("tasklist")
                .args(["/FI", &format!("PID eq {}", pid), "/NH"])
                .output();
            if let Ok(out) = check {
                if String::from_utf8_lossy(&out.stdout).contains(&pid.to_string()) {
                    return Err("已有下载任务进行中".to_string());
                }
            }
        }
    }

    // 统一使用 modelscope CLI 下载（所有模型均来自魔塔社区）
    // 去掉 :* 通配符后缀，modelscope CLI 不支持
    let clean_id = model_id.trim_end_matches(":*");
    let mut cmd = std::process::Command::new("modelscope");
    cmd.args(["download", "--model", &clean_id, "--local_dir", &local_dir]);

    // 自动读取 modelscope.token 并注入 --token
    let token_path = get_launcher_dir().join("tokens").join("modelscope.token");
    if token_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&token_path) {
            let token = content.trim().to_string();
            if !token.is_empty() {
                cmd.arg("--token");
                cmd.arg(&token);
            }
        }
    }

    if let Some(pattern) = &include_pattern {
        cmd.arg("--include");
        cmd.arg(pattern);
    }

    // 捕获 stdout 和 stderr（modelscope 可能输出到任意一个）
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| format!("启动下载失败: {}\n\n请确保已安装 modelscope CLI:\n  pip install modelscope", e))?;
    let pid = child.id();
    *pid_guard = Some(pid);

    // 在后台线程中等待下载完成
    let app_clone = app_handle.clone();
    let model_clone = model_id.trim_end_matches(":*").to_string();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    std::thread::spawn(move || {
        let _ = app_clone.emit("download-progress", serde_json::json!({
            "progress": 0,
            "message": format!("正在下载 {} ...", model_clone),
            "status": "downloading",
        }));

        // 从 stdout 读取行
        let app_clone2 = app_clone.clone();
        let stdout_handle = if let Some(out) = stdout {
            Some(std::thread::spawn(move || {
                let reader = BufReader::new(out);
                for line in reader.lines() {
                    if let Ok(text) = line {
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            let _ = app_clone2.emit("download-progress", serde_json::json!({
                                "progress": 0,
                                "message": trimmed,
                                "status": "downloading",
                            }));
                        }
                    }
                }
            }))
        } else { None };

        // 从 stderr 读取行（tqdm 用 \r 更新同一行，不能用 lines()）
        let app_clone3 = app_clone.clone();
        let stderr_handle = if let Some(err) = stderr {
            Some(std::thread::spawn(move || {
                let mut reader = BufReader::new(err);
                let mut buf = Vec::new();
                // 用 \r 作为分隔符，捕获 tqdm 的每一帧更新
                while let Ok(n) = reader.read_until(b'\r', &mut buf) {
                    if n == 0 { break; }
                    let text = String::from_utf8_lossy(&buf).trim().to_string();
                    buf.clear();
                    if text.is_empty() { continue; }

                    // 提取百分比 " 45%|" 或 "100%|"
                    let pct = text.split('%').next()
                        .and_then(|s| s.split_whitespace().last())
                        .and_then(|s| s.parse::<u32>().ok());

                    let _ = app_clone3.emit("download-progress", serde_json::json!({
                        "progress": pct.unwrap_or(0),
                        "message": text,
                        "status": "downloading",
                    }));
                }
            }))
        } else { None };

        // 等待读取线程完成（最多额外等5秒）
        if let Some(h) = stdout_handle { let _ = h.join(); }
        if let Some(h) = stderr_handle { let _ = h.join(); }

        // 等待进程完全退出并检查退出码
        let status = child.wait();
        let success = status.as_ref().map(|s| s.success()).unwrap_or(false);

        if success {
            let _ = app_clone.emit("download-progress", serde_json::json!({
                "progress": 100,
                "message": format!("{} 已下载完成！请刷新本地模型列表", model_clone),
                "status": "completed",
            }));
        } else {
            let code = status.as_ref().map(|s| s.code().unwrap_or(-1)).unwrap_or(-1);
            let _ = app_clone.emit("download-progress", serde_json::json!({
                "progress": 0,
                "message": format!("{} 下载失败（退出码: {}），请检查模型 ID 或网络连接", model_clone, code),
                "status": "error",
            }));
        }
    });

    Ok(format!("下载已开始 (PID: {})", pid))
}

/// 取消下载
#[tauri::command]
fn cancel_download(state: State<'_, DownloadProcess>) -> Result<String, String> {
    let mut pid_guard = state.pid.lock().map_err(|e| e.to_string())?;
    if let Some(pid) = pid_guard.take() {
        #[cfg(windows)]
        {
            let _ = std::process::Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/F"])
                .output();
        }
        #[cfg(not(windows))]
        {
            let _ = std::process::Command::new("kill")
                .arg(pid.to_string())
                .output();
        }
        Ok(format!("下载已取消 (PID: {})", pid))
    } else {
        Err("没有正在进行的下载任务".to_string())
    }
}

// ============================================================
// 窗口控制
// ============================================================

#[tauri::command]
async fn minimize_window(handle: tauri::AppHandle) {
    if let Some(window) = handle.get_webview_window("main") {
        let _ = window.minimize();
    }
}

#[tauri::command]
async fn maximize_window(handle: tauri::AppHandle) {
    if let Some(window) = handle.get_webview_window("main") {
        if window.is_maximized().unwrap_or(false) {
            let _ = window.unmaximize();
        } else {
            let _ = window.maximize();
        }
    }
}

#[tauri::command]
async fn close_window(handle: tauri::AppHandle) {
    // 读取配置，判断是否退出时卸载
    let should_unload = load_auto_unload_setting();

    if should_unload {
        // 卸载所有模型
        if let Some(models_state) = handle.try_state::<RunningModelsState>() {
            if let Ok(mut models) = models_state.models.lock() {
                for model in models.iter() {
                    if let Ok(pid) = model.pid.parse::<u32>() {
                        let _ = std::process::Command::new("taskkill")
                            .args(["/F", "/PID", &pid.to_string()])
                            .output();
                    }
                }
                models.clear();
            }
        }
        // 完全退出
        handle.exit(0);
    } else {
        // 不卸载，隐藏到托盘
        if let Some(window) = handle.get_webview_window("main") {
            let _ = window.hide();
        }
    }
}

fn load_auto_unload_setting() -> bool {
    let launcher_dir = std::env::var("APPDATA")
        .map(|d| PathBuf::from(d).join("Soul-Agent-Launcher"))
        .unwrap_or_else(|_| PathBuf::from("."));
    let config_path = launcher_dir.join("config.json");
    if let Ok(content) = std::fs::read_to_string(&config_path) {
        if let Ok(config) = serde_json::from_str::<LauncherConfig>(&content) {
            return config.auto_unload;
        }
    }
    true // 默认卸载
}

// ============================================================
// 辅助函数
// ============================================================

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    }
}

/// 格式化时间戳为 HH:MM:SS
fn format_ts(secs: u64) -> String {
    let h = (secs / 3600) % 24;
    let m = (secs / 60) % 60;
    let s = secs % 60;
    format!("{:02}:{:02}:{:02}", h, m, s)
}

// ============================================================
// 双端点 HTTP 代理
// ============================================================

/// 轻量 TCP 代理：监听 public_port，转发到 llama_port
/// - `/chat` → 重写为 `/completion`（自研轻量端点）
/// - `/v1/chat/completions` → 直通（OpenAI 兼容）
/// - 其余路径 → 直通
fn proxy_loop(public_port: u16, llama_port: u16, model_name: &str) {
    let addr = format!("127.0.0.1:{}", public_port);
    let listener = match TcpListener::bind(&addr) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("代理启动失败 {}: {}", addr, e);
            return;
        }
    };
    listener.set_nonblocking(true).ok();
    let model = model_name.to_string();

    for stream in listener.incoming() {
        match stream {
            Ok(mut client) => {
                let upstream_addr = format!("127.0.0.1:{}", llama_port);
                let m = model.clone();
                std::thread::spawn(move || {
                    handle_proxy_connection(&mut client, &upstream_addr, &m);
                });
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(_) => break,
        }
    }
}

/// OpenAI → 原生格式转换：/v1/chat/completions → /completion
/// 提取 messages 并格式化为 ChatML prompt 字符串
fn convert_openai_to_native(
    body: &[u8], request_line: &str, path: &str, rest_headers: &str,
    force_no_stream: bool,
) -> (Vec<u8>, String, String) {
    if body.is_empty() {
        return (body.to_vec(), rest_headers.to_string(), request_line.to_string());
    }

    if let Ok(json) = serde_json::from_slice::<serde_json::Value>(body) {
        if let Some(obj) = json.as_object() {
            // 提取原有字段（stream 默认 true）
            let stream = if force_no_stream {
                false
            } else {
                obj.get("stream").and_then(|v| v.as_bool()).unwrap_or(true)
            };

            // 提取 messages 并格式化为 prompt
            let prompt = if let Some(msgs) = obj.get("messages").and_then(|v| v.as_array()) {
                let mut parts = Vec::new();
                for msg in msgs {
                    let role = msg["role"].as_str().unwrap_or("user");
                    let content = msg["content"].as_str().unwrap_or("");
                    match role {
                        "system" => parts.push(format!("<|im_start|>system\n{}<|im_end|>", content)),
                        "user" => parts.push(format!("<|im_start|>user\n{}<|im_end|>", content)),
                        "assistant" => parts.push(format!("<|im_start|>assistant\n{}<|im_end|>", content)),
                        _ => parts.push(format!("<|im_start|>{}\n{}<|im_end|>", role, content)),
                    }
                }
                parts.push("<|im_start|>assistant\n".to_string());
                parts.join("\n")
            } else {
                return (body.to_vec(), rest_headers.to_string(), request_line.to_string());
            };

            // 构建原生格式 body
            let body_str = serde_json::to_string(&serde_json::json!({
                "prompt": prompt,
                "stream": stream,
            })).unwrap_or_else(|_| String::new());

            let new_body = body_str.into_bytes();
            let new_cl = new_body.len();

            // 更新 Content-Length 头（排除 \r\n\r\n 产生的空行）
            let updated_headers: String = rest_headers.lines()
                .filter(|l| !l.is_empty())  // 去掉尾部的空行
                .map(|l| {
                    if l.to_lowercase().starts_with("content-length:") {
                        format!("Content-Length: {}", new_cl)
                    } else { l.to_string() }
                })
                .collect::<Vec<_>>()
                .join("\r\n");

            let rewritten = request_line.replace(path, "/completion");
            return (new_body, format!("{}\r\n\r\n", updated_headers), rewritten);
        }
    }
    (body.to_vec(), rest_headers.to_string(), request_line.to_string())
}

/// 处理单个代理连接：读请求 → 重写路径 → 转发 → 管道回响应
fn handle_proxy_connection(client: &mut std::net::TcpStream, upstream_addr: &str, model_name: &str) {
    // 1. 读请求头（到 \r\n\r\n）
    let mut buf = Vec::new();
    let mut header_end = None;
    loop {
        let mut byte = [0u8; 1];
        match client.read(&mut byte) {
            Ok(0) => break,
            Ok(_) => {
                buf.push(byte[0]);
                let len = buf.len();
                if len >= 4 && buf[len-4..] == [b'\r', b'\n', b'\r', b'\n'] {
                    header_end = Some(len);
                    break;
                }
            }
            Err(_) => break,
        }
    }
    if buf.is_empty() { return; }

    let header_bytes = &buf[..header_end.unwrap_or(buf.len())];
    let header_str = String::from_utf8_lossy(header_bytes);

    let header_lines: Vec<&str> = header_str.splitn(2, '\n').collect();
    let request_line = header_lines.first().unwrap_or(&"").trim_end().to_string();
    let rest_headers = header_lines.get(1).unwrap_or(&"").to_string();

    // 提取请求方法
    let method = request_line.split(' ').next().unwrap_or("GET").to_string();
    // 提取请求路径
    let path = request_line.split(' ').nth(1).unwrap_or("/").to_string();

    // 0. OPTIONS 预检请求：直接返回 204 + CORS 头
    if method == "OPTIONS" {
        let _ = client.write_all(b"HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: *\r\nConnection: close\r\n\r\n");
        let _ = client.flush();
        return;
    }

    // 1. 模型列表请求：直接返回（极致版 llama-server 可能不支持）
    // 匹配 /v1/models、/models、/api/tags 以及任意前缀组合
    let is_model_list = path.contains("/v1/models") || path.contains("/api/tags")
        || (path == "/models" || path.ends_with("/models"));
    eprintln!("[proxy] path={}, is_model_list={}", path, is_model_list);
    if is_model_list {
        let models_json = format!(r#"{{"object":"list","data":[{{"id":"{}","object":"model"}}]}}"#, model_name);
        let resp = format!(
            "HTTP/1.1 200 OK\r\nAccess-Control-Allow-Origin: *\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            models_json.len(), models_json
        );
        let _ = client.write_all(resp.as_bytes());
        let _ = client.flush();
        return;
    }

    // 解析 Content-Length 读 body
    let content_length: usize = rest_headers.lines()
        .find(|l| l.to_lowercase().starts_with("content-length:"))
        .and_then(|l| l.split(':').nth(1))
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);

    let mut body = Vec::new();
    if content_length > 0 {
        let after_headers = if let Some(pos) = header_end { &buf[pos..] } else { &buf[..0] };
        body.extend_from_slice(after_headers);
        let remaining = content_length.saturating_sub(body.len());
        if remaining > 0 {
            let mut body_buf = vec![0u8; remaining];
            let mut total = 0;
            while total < remaining {
                match client.read(&mut body_buf[total..]) {
                    Ok(0) => break,
                    Ok(n) => total += n,
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(2));
                        continue;
                    }
                    Err(_) => break,
                }
            }
            body.extend_from_slice(&body_buf[..total]);
        }
    }

    // 2. `/chat` 端点：注入默认字段（流式默认 true，多模态默认 false）
    let is_chat_endpoint = path.contains("/chat") && !path.contains("/v1/chat/completions");
    let is_openai_endpoint = path.contains("/v1/chat/completions");

    // 检测客户端请求的 stream 字段
    let client_wants_stream = if !body.is_empty() {
        if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&body) {
            json.get("stream").and_then(|v| v.as_bool()).unwrap_or(true)
        } else { true }
    } else { true };

    // 两个端点都需转换： OpenAI 格式 → 原生 /completion 格式
    // （极致版 llama-server 无 /v1/chat/completions 路由)
    let (forward_body, forward_headers, rewritten_line) = if is_openai_endpoint {
        // OpenAI 端点：传递客户端的 stream 偏好（代理现在支持流式格式转换)
        convert_openai_to_native(&body, &request_line, &path, &rest_headers, !client_wants_stream)
    } else if is_chat_endpoint {
        convert_openai_to_native(&body, &request_line, &path, &rest_headers, false)
    } else {
        (body.clone(), rest_headers.clone(), request_line.clone())
    };
    let mut upstream = None;
    for attempt in 0..10 {
        match std::net::TcpStream::connect(upstream_addr) {
            Ok(u) => { upstream = Some(u); break; }
            Err(_) => {
                if attempt < 9 {
                    std::thread::sleep(std::time::Duration::from_millis(300));
                }
            }
        }
    }
    let mut upstream = match upstream {
        Some(u) => u,
        None => {
            // 模型未就绪，返回 503 友好提示（含 Retry-After）
            let body_text = "{\"error\":\"Model waking up\",\"message\":\"模型正在唤醒，请稍候...\",\"retry_after\":5}";
            let resp = format!(
                "HTTP/1.1 503 Service Unavailable\r\nAccess-Control-Allow-Origin: *\r\nContent-Type: application/json\r\nContent-Length: {}\r\nRetry-After: 5\r\nConnection: close\r\n\r\n{}",
                body_text.len(), body_text
            );
            let _ = client.write_all(resp.as_bytes());
            let _ = client.flush();
            return;
        }
    };
    // 设置上游读取超时（120秒，模型推理可能较慢，流式需要更长）
    let _ = upstream.set_read_timeout(Some(std::time::Duration::from_secs(120)));

    // 5. 转发请求
    let _ = upstream.write_all(rewritten_line.as_bytes());
    let _ = upstream.write_all(b"\r\n");
    let _ = upstream.write_all(forward_headers.as_bytes());
    if !forward_body.is_empty() {
        let _ = upstream.write_all(&forward_body);
    }
    let _ = upstream.flush();

    eprintln!("[proxy] routing path={} to {} (stream={})", path, if is_openai_endpoint { "forward_openai_response" } else if is_chat_endpoint { "pipe_raw_response (/chat)" } else { "pipe_raw_response (other)" }, client_wants_stream);

    // 6. OpenAI 端点：格式转换响应；其他端点：直接管道
    if is_openai_endpoint {
        forward_openai_response(&mut upstream, client, client_wants_stream);
    } else {
        pipe_raw_response(&mut upstream, client);
    }
}

/// 读取上游响应、转换为 OpenAI 格式、返回给客户端
/// requested_stream: 客户端是否期望 SSE 流式响应
fn forward_openai_response(upstream: &mut std::net::TcpStream, client: &mut std::net::TcpStream, requested_stream: bool) {
    upstream.set_read_timeout(Some(std::time::Duration::from_secs(120))).ok();
    client.set_write_timeout(Some(std::time::Duration::from_secs(30))).ok();

    // 1. 读取整个上游响应（包括HTTP头）
    // 流式模式下检测 [DONE] 后提前退出，避免等满 120 秒超时
    let mut all = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        match upstream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                all.extend_from_slice(&chunk[..n]);
                // 流式模式：检测到 [DONE] 标记即退出
                if requested_stream && all.windows(6).any(|w| w == b"[DONE]") {
                    break;
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => { std::thread::sleep(std::time::Duration::from_millis(2)); continue; }
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => break,
            Err(ref e) if e.raw_os_error() == Some(10054) || e.kind() == std::io::ErrorKind::ConnectionReset => break,
            Err(_) => break,
        }
    }

    if all.is_empty() {
        eprintln!("[proxy] openai response empty");
        let body_text = "{\"error\":\"empty upstream response\",\"message\":\"上游返回空响应\"}";
        let resp = format!(
            "HTTP/1.1 502 Bad Gateway\r\nAccess-Control-Allow-Origin: *\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body_text.len(), body_text
        );
        let _ = client.write_all(resp.as_bytes());
        let _ = client.flush();
        return;
    }

    // 2. 解析HTTP响应：找 \r\n\r\n 或 \n\n 作为头部分界
    // 流式模式下 SSE 数据含 \n\n，必须用 position（第一个）而非 rposition
    fn find_header_end(data: &[u8], from_end: bool) -> usize {
        let needle = b"\r\n\r\n";
        if from_end {
            data.windows(4).rposition(|w| w == needle).map(|p| p + 4)
        } else {
            data.windows(4).position(|w| w == needle).map(|p| p + 4)
        }.unwrap_or_else(|| {
            // 回退：查找 \n\n（部分极致版 llama-server 使用 POSIX 换行）
            data.windows(2).position(|w| w == b"\n\n").map(|p| p + 2).unwrap_or(0)
        })
    }
    let hdr_end = if requested_stream {
        // 流式：position 找第一个
        let first = find_header_end(&all, false);
        if first > 0 {
            let status_text = String::from_utf8_lossy(&all[..first.min(100)]);
            if status_text.contains("100") {
                // 跳过 100 Continue：在 first 之后找下一个
                all[first..].windows(4).position(|w| w == b"\r\n\r\n")
                    .or_else(|| all[first..].windows(2).position(|w| w == b"\n\n"))
                    .map(|p| first + p + 2)
                    .unwrap_or(first)
            } else { first }
        } else { 0 }
    } else {
        // 非流式：rposition 跳过 100 Continue
        find_header_end(&all, true)
    };
    let _header_str = String::from_utf8_lossy(&all[..hdr_end]);
    let body = &all[hdr_end..];
    eprintln!("[proxy] openai response ({} bytes header, {} bytes body)", hdr_end, body.len());
    // 调试：上游响应为空时打印原始数据前 300 字节
    if body.is_empty() {
        eprintln!("[proxy] EMPTY BODY! raw first 300: {:?}", String::from_utf8_lossy(&all[..all.len().min(300)]));
    }

    // 检查上游 HTTP 状态码 — 非 200 直接透传
    let header_str = &String::from_utf8_lossy(&all[..hdr_end]);
    let first_line = header_str.lines().next().unwrap_or("");
    let status_code: u16 = first_line.split(' ').nth(1).unwrap_or("0").parse().unwrap_or(0);
    if status_code != 200 {
        eprintln!("[proxy] openai upstream status: {}, piping through", status_code);
        let _ = client.write_all(&all);
        let _ = client.flush();
        return;
    }

    // === 流式模式 --- 累积所有 token 后一次性发送 ===
    if requested_stream {
        // 检查上游是否返回了空响应（模型未加载/待机）
        if body.len() <= 2 {
            eprintln!("[proxy] stream: upstream returned empty body (model not loaded?)");
            let empty_body = "{\"error\":\"empty upstream response\",\"message\":\"模型未加载或已休眠，请检查服务状态\"}";
            let resp = format!("HTTP/1.1 502 Bad Gateway\r\nAccess-Control-Allow-Origin: *\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", empty_body.len(), empty_body);
            let _ = client.write_all(resp.as_bytes());
            let _ = client.flush();
            return;
        }
        // 检查 Content-Type 是否非 JSON/SSE（例如 text/html 说明请求有误）
        let ct = header_str.lines().find(|l| l.to_lowercase().starts_with("content-type:")).unwrap_or("").to_lowercase();
        if ct.contains("text/html") {
            eprintln!("[proxy] stream: upstream returned text/html (wrong endpoint?)");
            let err_body = format!("{{\"error\":\"wrong content type\",\"message\":\"上游返回了 text/html，请求可能未正确路由到 /completion\"}}");
            let resp = format!("HTTP/1.1 502 Bad Gateway\r\nAccess-Control-Allow-Origin: *\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", err_body.len(), err_body);
            let _ = client.write_all(resp.as_bytes());
            let _ = client.flush();
            return;
        }
        let body_str = String::from_utf8_lossy(body);
        let chatcmpl_id = format!("chatcmpl-soul-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis());
        let created = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();

        // 解析上游 SSE，累积全部内容
        let mut all_content = String::new();
        let mut finished = false;
        let mut prompt_tokens: u64 = 0;
        let mut completion_tokens: u64 = 0;
        for line in body_str.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() { continue; }
            let json_str = if trimmed.starts_with("data: ") { trimmed[6..].trim() }
                else if trimmed.starts_with("data:") { trimmed[5..].trim() }
                else { trimmed };
            if json_str == "[DONE]" { continue; }
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(json_str) {
                let c = json["content"].as_str().unwrap_or("");
                let s = json["stop"].as_bool().unwrap_or(false);
                if !c.is_empty() { all_content.push_str(c); }
                if s {
                    finished = true;
                    prompt_tokens = json["tokens_evaluated"].as_u64().unwrap_or(0);
                    completion_tokens = json["tokens_predicted"].as_u64().unwrap_or(0);
                }
            }
        }

        // 发给客户端：角色 + 内容 + finish + [DONE]
        let total_tokens = prompt_tokens + completion_tokens;
        let mut sse_body = String::new();
        sse_body.push_str(&format!("data: {}\n\n", serde_json::to_string(&serde_json::json!({
            "id": &chatcmpl_id, "object": "chat.completion.chunk", "created": created,
            "choices": [{"index": 0, "delta": {"role": "assistant"}, "finish_reason": null}]
        })).unwrap_or_default()));
        sse_body.push_str(&format!("data: {}\n\n", serde_json::to_string(&serde_json::json!({
            "id": &chatcmpl_id, "object": "chat.completion.chunk", "created": created,
            "choices": [{"index": 0, "delta": {"content": &all_content}, "finish_reason": null}]
        })).unwrap_or_default()));
        sse_body.push_str(&format!("data: {}\n\n", serde_json::to_string(&serde_json::json!({
            "id": &chatcmpl_id, "object": "chat.completion.chunk", "created": created,
            "choices": [{"index": 0, "delta": {}, "finish_reason": if finished { "stop" } else { "length" }}],
            "usage": {"prompt_tokens": prompt_tokens, "completion_tokens": completion_tokens, "total_tokens": total_tokens}
        })).unwrap_or_default()));
        sse_body.push_str("data: [DONE]\n\n");
        let resp = format!(
            "HTTP/1.1 200 OK\r\nAccess-Control-Allow-Origin: *\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
            sse_body.len(), sse_body
        );
        if let Err(e) = client.write_all(resp.as_bytes()) {
            eprintln!("[proxy] stream: write error: {}", e);
            return;
        }
        if let Err(e) = client.flush() {
            eprintln!("[proxy] stream: flush error: {}", e);
            return;
        }
        eprintln!("[proxy] openai stream done ({} chars)", all_content.len());
        return;
    }

    // === 非流式模式：上游返回单个 JSON ===
    if body.len() <= 2 {
        eprintln!("[proxy] body too small");
        let resp = "HTTP/1.1 502 Bad Gateway\r\nAccess-Control-Allow-Origin: *\r\nContent-Type: application/json\r\nContent-Length: 0\r\n\r\n";
        let _ = client.write_all(resp.as_bytes());
        let _ = client.flush();
        return;
    }

    let body_str = String::from_utf8_lossy(body);
    if let Ok(json) = serde_json::from_slice::<serde_json::Value>(body) {
        let content = json["content"].as_str().unwrap_or("");
        let finish = if json["stop"].as_bool().unwrap_or(false) { "stop" } else { "length" };
        let model = json["model"].as_str().unwrap_or("model");
        let prompt_tokens = json["tokens_evaluated"].as_u64().unwrap_or(0);
        let completion_tokens = json["tokens_predicted"].as_u64().unwrap_or(0);
        let id = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis();
        let openai = serde_json::json!({
            "id": format!("chatcmpl-soul-{}", id),
            "object": "chat.completion",
            "created": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs(),
            "model": model,
            "choices": [{"index": 0, "message": {"role": "assistant", "content": content}, "finish_reason": finish}],
            "usage": {"prompt_tokens": prompt_tokens, "completion_tokens": completion_tokens, "total_tokens": prompt_tokens + completion_tokens}
        });
        let final_body = serde_json::to_vec(&openai).unwrap_or_default();
        let resp = format!(
            "HTTP/1.1 200 OK\r\nAccess-Control-Allow-Origin: *\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            final_body.len()
        );
        let _ = client.write_all(resp.as_bytes());
        let _ = client.write_all(&final_body);
        let _ = client.flush();
        return;
    }

    // JSON 解析失败
    eprintln!("[proxy] openai JSON parse failed, raw body (first 500 chars): {:?}", &body_str[..body_str.len().min(500)]);
    let resp = format!(
        "HTTP/1.1 502 Bad Gateway\r\nAccess-Control-Allow-Origin: *\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(), body_str
    );
    let _ = client.write_all(resp.as_bytes());
}

/// 纯管道传输（/chat、/health 等无需格式转换的端点）
fn pipe_raw_response(upstream: &mut std::net::TcpStream, client: &mut std::net::TcpStream) {
    upstream.set_read_timeout(Some(std::time::Duration::from_secs(60))).ok();
    client.set_write_timeout(Some(std::time::Duration::from_secs(30))).ok();
    let mut pipe_buf = [0u8; 65536];
    loop {
        match upstream.read(&mut pipe_buf) {
            Ok(0) => break,
            Ok(n) => {
                if client.write_all(&pipe_buf[..n]).is_err() { break; }
                let _ = client.flush();
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(2));
                continue;
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => break,
            Err(ref e) if e.raw_os_error() == Some(10054) || e.kind() == std::io::ErrorKind::ConnectionReset => break,
            Err(_) => break,
        }
    }
}

// ============================================================
// 入口
// ============================================================

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            // 创建托盘图标与菜单
            let show_item = MenuItemBuilder::with_id("show", "显示窗口").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "退出").build(app)?;
            let menu = MenuBuilder::new(app)
                .item(&show_item)
                .separator()
                .item(&quit_item)
                .build()?;

            let _tray = TrayIconBuilder::new()
                .icon(tauri::image::Image::from_bytes(include_bytes!("../icons/icon.png"))
                    .expect("加载托盘图标失败"))
                .menu(&menu)
                .tooltip("Soul Agent Launcher")
                .on_menu_event(|app, event| {
                    match event.id().as_ref() {
                        "show" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                        "quit" => {
                            // 退出前清理所有模型
                            if let Some(models_state) = app.try_state::<RunningModelsState>() {
                                if let Ok(mut models) = models_state.models.lock() {
                                    for model in models.iter() {
                                        if let Ok(pid) = model.pid.parse::<u32>() {
                                            let _ = std::process::Command::new("taskkill")
                                                .args(["/F", "/PID", &pid.to_string()])
                                                .output();
                                        }
                                    }
                                    models.clear();
                                }
                            }
                            app.exit(0);
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // 阻止关闭，改为隐藏到托盘
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .manage(ServerState {
            server: Mutex::new(None),
            proxy_running: Mutex::new(false),
            server_starting: Mutex::new(false),
            standby: Mutex::new(false),
            spawn_params: Mutex::new(None),
            model_name: Mutex::new(String::new()),
        })
        .manage(DownloadProcess {
            pid: Mutex::new(None),
        })
        .manage(RunningModelsState {
            models: Mutex::new(Vec::new()),
        })
        .manage(SleepMonitor {
            entries: Mutex::new(Vec::new()),
        })
        .invoke_handler(tauri::generate_handler![
            check_server,
            check_server_port,
            check_standby,
            check_idle,
            get_sleep_logs,
            start_server,
            stop_server,
            wake_server,
            sleep_server,
            auto_find_model,
            build_llama_minimal,
            list_models,
            delete_model_file,
            check_setup_needed,
            run_setup,
            detect_gpu,
            check_llama_installed,
            load_config,
            save_config,
            search_models,
            list_official_models,
            check_modelscope_available,
            install_modelscope,
            list_model_files,
            read_token,
            start_model_download,
            cancel_download,
            minimize_window,
            maximize_window,
            close_window,
            start_model,
            unload_model,
            list_running_models,
            stop_all_models,
        ])
        .run(tauri::generate_context!())
        .expect("启动失败");
}
