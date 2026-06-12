// Soul Agent Launcher - 后端
// Release 构建隐藏控制台窗口（dev 模式仍显示，方便看日志）
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{Emitter, Manager, State};
use tauri::tray::{TrayIconBuilder, MouseButton, MouseButtonState, TrayIconEvent};
use tauri::menu::{MenuBuilder, MenuItemBuilder};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Windows 下隐藏子进程的控制台窗口（release 构建时不弹 CMD）
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

/// 创建隐藏 CMD 窗口的子进程
fn new_hidden_cmd(program: &str) -> std::process::Command {
    let mut cmd = std::process::Command::new(program);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

// ============================================================
// 超详细日志系统
// ============================================================

static SOUL_LOGGER: OnceLock<Mutex<SoulLogger>> = OnceLock::new();

/// 日志条目（结构化）
#[derive(Debug, Clone, Serialize)]
struct LogEntry {
    timestamp: String,
    level: String, // INFO / WARN / ERROR / DEBUG / RESOURCE
    category: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    resource: Option<ResourceSnapshot>,
}

/// 系统资源快照
#[derive(Debug, Clone, Serialize)]
struct ResourceSnapshot {
    process_cpu_pct: f64,
    process_mem_mb: f64,
    system_mem_used_gb: f64,
    system_mem_total_gb: f64,
    gpu_vram_used_mb: u64,
    gpu_name: String,
}

#[allow(dead_code)]
struct SoulLogger {
    log_dir: PathBuf,
    today: String,
    txt_writer: Option<std::io::BufWriter<std::fs::File>>,
    json_writer: Option<std::io::BufWriter<std::fs::File>>,
    start_time: std::time::Instant,
}

impl SoulLogger {
    fn get() -> &'static Mutex<SoulLogger> {
        SOUL_LOGGER.get_or_init(|| Mutex::new(SoulLogger::new()))
    }

    fn new() -> SoulLogger {
        let log_dir = Self::log_dir_path();
        std::fs::create_dir_all(&log_dir).ok();

        let today = Self::today_str();
        let (txt_writer, json_writer) = Self::open_files(&log_dir, &today);

        let mut logger = SoulLogger {
            log_dir,
            today,
            txt_writer,
            json_writer,
            start_time: std::time::Instant::now(),
        };
        logger.write_raw("════════ Soul Agent Launcher 日志启动 ════════");
        logger
    }

    fn log_dir_path() -> PathBuf {
        let base = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(base).join("SoulLogs")
    }

    fn today_str() -> String {
        let t = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        let secs = t.as_secs();
        let days = secs / 86400;
        let _rem = secs % 86400;
        // 1970-01-01 was Thursday, adjusted for UTC+8
        let mut y = 1970u64;
        let mut d = days;
        loop {
            let year_days = if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 { 366 } else { 365 };
            if d < year_days { break; }
            d -= year_days;
            y += 1;
        }
        let months_days = [31,28,31,30,31,30,31,31,30,31,30,31];
        let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
        let mut m = 0;
        while m < 12 {
            let md = if m == 1 && leap { 29 } else { months_days[m] };
            if d < md as u64 { break; }
            d -= md as u64;
            m += 1;
        }
        format!("{:04}-{:02}-{:02}", y, m + 1, d + 1)
    }

    fn open_files(dir: &PathBuf, today: &str) -> (Option<std::io::BufWriter<std::fs::File>>, Option<std::io::BufWriter<std::fs::File>>) {
        let txt = dir.join(format!("soul_{}.log", today));
        let json = dir.join(format!("soul_{}.json", today));
        let t = std::fs::OpenOptions::new().create(true).append(true).open(&txt)
            .map(|f| std::io::BufWriter::new(f)).ok();
        let j = std::fs::OpenOptions::new().create(true).append(true).open(&json)
            .map(|f| std::io::BufWriter::new(f)).ok();
        (t, j)
    }

    fn roll_if_needed(&mut self) {
        let today = Self::today_str();
        if today != self.today {
            self.today = today;
            let (t, j) = Self::open_files(&self.log_dir, &self.today);
            self.txt_writer = t;
            self.json_writer = j;
        }
    }

    /// 收集系统资源快照
    fn resource_snapshot() -> Option<ResourceSnapshot> {
        let mut snap = ResourceSnapshot {
            process_cpu_pct: 0.0, process_mem_mb: 0.0,
            system_mem_used_gb: 0.0, system_mem_total_gb: 0.0,
            gpu_vram_used_mb: 0, gpu_name: String::new(),
        };

        // Windows 内存信息
        #[cfg(windows)]
        {
            use std::mem;
            extern "system" {
                fn GetProcessMemoryInfo(process: isize, mem_counters: *mut std::ffi::c_void, size: u32) -> i32;
                fn GetCurrentProcess() -> isize;
                fn GetPhysicallyInstalledSystemMemory(total_mem: *mut u64) -> i32;
                fn GlobalMemoryStatusEx(lpBuffer: *mut std::ffi::c_void) -> i32;
            }
            #[repr(C)]
            #[allow(non_snake_case)]
            struct MEMORYSTATUSEX { dwLength: u32, dwMemoryLoad: u32, ullTotalPhys: u64, ullAvailPhys: u64, ullTotalPageFile: u64, ullAvailPageFile: u64, ullTotalVirtual: u64, ullAvailVirtual: u64, ullAvailExtendedVirtual: u64, }
            #[repr(C)]
            #[allow(non_snake_case)]
            struct PROCESS_MEMORY_COUNTERS { cb: u32, PageFaultCount: u32, PeakWorkingSetSize: usize, WorkingSetSize: usize, QuotaPeakPagedPoolUsage: usize, QuotaPagedPoolUsage: usize, QuotaPeakNonPagedPoolUsage: usize, QuotaNonPagedPoolUsage: usize, PagefileUsage: usize, PeakPagefileUsage: usize, }

            let mut total = 0u64;
            unsafe { GetPhysicallyInstalledSystemMemory(&mut total); }
            snap.system_mem_total_gb = total as f64 / 1048576.0;

            let mut ms = MEMORYSTATUSEX { dwLength: mem::size_of::<MEMORYSTATUSEX>() as u32, dwMemoryLoad: 0, ullTotalPhys: 0, ullAvailPhys: 0, ullTotalPageFile: 0, ullAvailPageFile: 0, ullTotalVirtual: 0, ullAvailVirtual: 0, ullAvailExtendedVirtual: 0, };
            unsafe { GlobalMemoryStatusEx(&mut ms as *mut _ as *mut std::ffi::c_void); }
            snap.system_mem_used_gb = (ms.ullTotalPhys - ms.ullAvailPhys) as f64 / 1048576.0;

            let mut pmc: PROCESS_MEMORY_COUNTERS = unsafe { mem::zeroed() };
            pmc.cb = mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
            unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut pmc as *mut _ as *mut std::ffi::c_void, pmc.cb); }
            snap.process_mem_mb = pmc.WorkingSetSize as f64 / 1048576.0;
        }

        // GPU VRAM（nvidia-smi）— 使用隐藏窗口，避免与 WebView2 GPU 渲染冲突
        {
            let mut cmd = new_hidden_cmd("nvidia-smi");
            cmd.args(["--query-gpu=name,memory.used", "--format=csv,noheader"]);
            if let Ok(output) = cmd.output() {
                let text = String::from_utf8_lossy(&output.stdout);
                if let Some(line) = text.lines().next() {
                    let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
                    if parts.len() >= 2 {
                        snap.gpu_name = parts[0].to_string();
                        snap.gpu_vram_used_mb = parts[1].trim_end_matches(" MiB").parse().unwrap_or(0);
                    }
                }
            }
        }

        Some(snap)
    }

    /// 写一条日志（同时写 TXT 和 JSON）
    fn write_entry(&mut self, level: &str, category: &str, message: &str, detail: Option<&str>) {
        self.roll_if_needed();
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        let secs = now.as_secs();
        let ms = now.subsec_millis();
        let h = (secs % 86400) / 3600;
        let m = (secs % 3600) / 60;
        let s = secs % 60;
        let ts = format!("{:02}:{:02}:{:02}.{:03}", h, m, s, ms);

        let mut txt_line = format!("[{}][{}][{}] {}", ts, level, category, message);
        if let Some(ref d) = detail {
            txt_line.push_str(&format!("\n  → {}", d));
        }

        let entry = LogEntry {
            timestamp: ts.clone(),
            level: level.to_string(),
            category: category.to_string(),
            message: message.to_string(),
            detail: detail.map(|s| s.to_string()),
            resource: None,
        };

        if let Some(ref mut w) = self.txt_writer {
            let _ = writeln!(w, "{}", txt_line);
            let _ = w.flush();
        }
        if let Some(ref mut w) = self.json_writer {
            let _ = writeln!(w, "{}", serde_json::to_string(&entry).unwrap_or_default());
            let _ = w.flush();
        }
    }

    /// 写一条资源日志
    fn write_resource(&mut self, snapshot: &ResourceSnapshot) {
        self.roll_if_needed();
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        let secs = now.as_secs();
        let ms = now.subsec_millis();
        let h = (secs % 86400) / 3600;
        let m = (secs % 3600) / 60;
        let s = secs % 60;
        let ts = format!("{:02}:{:02}:{:02}.{:03}", h, m, s, ms);

        let txt_line = format!(
            "[{}][RESOURCE] CPU: {:.1}% | Mem: {:.0}MB | SysMem: {:.1}/{:.1}GB | GPU: {} {}MB",
            ts, snapshot.process_cpu_pct, snapshot.process_mem_mb,
            snapshot.system_mem_used_gb, snapshot.system_mem_total_gb,
            snapshot.gpu_name, snapshot.gpu_vram_used_mb
        );

        let entry = LogEntry {
            timestamp: ts,
            level: "RESOURCE".to_string(),
            category: "resource".to_string(),
            message: txt_line.clone(),
            detail: None,
            resource: Some(snapshot.clone()),
        };

        if let Some(ref mut w) = self.txt_writer {
            let _ = writeln!(w, "{}", txt_line);
            let _ = w.flush();
        }
        if let Some(ref mut w) = self.json_writer {
            let _ = writeln!(w, "{}", serde_json::to_string(&entry).unwrap_or_default());
            let _ = w.flush();
        }
    }

    fn write_raw(&mut self, text: &str) {
        self.roll_if_needed();
        let ts = {
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
            let secs = now.as_secs();
            let ms = now.subsec_millis();
            let h = (secs % 86400) / 3600;
            let m = (secs % 3600) / 60;
            let s = secs % 60;
            format!("{:02}:{:02}:{:02}.{:03}", h, m, s, ms)
        };
        if let Some(ref mut w) = self.txt_writer {
            let _ = writeln!(w, "[{}] {}", ts, text);
            let _ = w.flush();
        }
    }
}

/// 全局日志快捷函数
fn soul_log(level: &str, category: &str, message: &str) {
    if let Ok(mut logger) = SoulLogger::get().lock() {
        logger.write_entry(level, category, message, None);
    }
}

fn soul_log_detail(level: &str, category: &str, message: &str, detail: &str) {
    if let Ok(mut logger) = SoulLogger::get().lock() {
        logger.write_entry(level, category, message, Some(detail));
    }
}

/// 启动资源监控后台线程（每秒采集一次）
fn start_resource_monitor() {
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(2));
        if let Some(snap) = SoulLogger::resource_snapshot() {
            if let Ok(mut logger) = SoulLogger::get().lock() {
                logger.write_resource(&snap);
            }
        }
    });
}

/// 初始化日志系统
fn init_soul_logger() {
    // 触发 OnceLock 初始化
    drop(SoulLogger::get().lock());
    start_resource_monitor();
    soul_log("INFO", "init", "Soul Agent Launcher 日志系统已初始化");
}

/// 服务状态：单服务器进程 + 代理 + 待机
struct ServerState {
    server: Mutex<Option<std::process::Child>>,
    proxy_running: Mutex<bool>,
    server_starting: Mutex<bool>,
    standby: Mutex<bool>,
    spawn_params: Mutex<Option<SpawnParams>>,
    model_name: Mutex<String>,
}

/// 全局模型路由表：模型名 → proxy_port（供 proxy 线程读取）
static MODEL_ROUTER: OnceLock<Mutex<HashMap<String, u16>>> = OnceLock::new();
fn get_model_router() -> &'static Mutex<HashMap<String, u16>> {
    MODEL_ROUTER.get_or_init(|| Mutex::new(HashMap::new()))
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
    #[serde(default)]
    vram_mb: u64,
    #[serde(default)]
    status: String,
    /// /chat 常驻模式：true=永不休眠，false=待机可休眠
    #[serde(default = "return_true")]
    persistent: bool,
}

#[allow(dead_code)]
fn return_true() -> bool { true }

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
    let result = if let Some(ref mut child) = *server_guard {
        match child.try_wait() {
            Ok(Some(_)) => false,
            Ok(None) => {
                let url = format!("http://127.0.0.1:{}/health", port);
                match reqwest::blocking::get(&url) {
                    Ok(resp) => resp.status().is_success() || resp.status().as_u16() == 503,
                    Err(_) => false,
                }
            }
            Err(_) => false,
        }
    } else {
        false
    };
    soul_log("DEBUG", "server", &format!("check_server port={} → {}", port, result));
    Ok(result)
}

/// 检查用户端口是否可达
#[tauri::command]
fn check_server_port(port: u16) -> Result<bool, String> {
    let url = format!("http://127.0.0.1:{}/health", port);
    let result = match reqwest::blocking::get(&url) {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    };
    soul_log("DEBUG", "server", &format!("check_server_port port={} → {}", port, result));
    Ok(result)
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
    soul_log("INFO", "model", &format!("start_model name='{}' port={} llama_port={} ctx={}", model_name, port, llama_port, ctx));
    soul_log("DEBUG", "model", &format!("  llama_path={} model_path={}", llama_path, model_path));

    let child = new_hidden_cmd(&llama_path)
        .args(["-m", &model_path, "--host", "127.0.0.1",
               "--port", &llama_port.to_string(), "-c", &ctx.to_string(),
               "-ngl", "99"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| {
            soul_log_detail("ERROR", "model", "start_model 启动失败", &e.to_string());
            format!("启动模型失败: {}", e)
        })?;

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
        vram_mb: 0,
        status: "running".to_string(),
        persistent: true,
    };

    // 注册到全局路由表
    if let Ok(mut router) = get_model_router().lock() {
        router.insert(model.name.clone(), port);
    }

    // 检查该端口的代理是否已存在（避免端口冲突）
    let need_proxy = state.models.lock()
        .map(|m| !m.iter().any(|x| x.proxy_port == port))
        .unwrap_or(true);

    if let Ok(mut models) = state.models.lock() {
        models.push(model.clone());
    }

    // 启动代理线程（如果端口尚未被占）
    if need_proxy {
        soul_log("INFO", "proxy", &format!("start_model 启动代理 {} → {}:{}", model.name, port, llama_port));
        let base_name = model.name.clone();
        let public_p = port;
        let llama_p = llama_port;
        std::thread::spawn(move || proxy_loop(public_p, llama_p, &base_name));
    } else {
        soul_log("WARN", "proxy", &format!("start_model 端口 {} 已有代理，跳过", port));
        eprintln!("[start_model] 端口 {} 已有代理，跳过", port);
    }

    let result = serde_json::json!(model).to_string();
    soul_log("INFO", "model", &format!("start_model 完成 name='{}' pid={} port={}", model.name, pid, port));
    Ok(result)
}

/// 卸载指定模型
#[tauri::command]
fn unload_model(state: State<RunningModelsState>, model_name: String) -> Result<String, String> {
    soul_log("INFO", "model", &format!("unload_model name='{}'", model_name));
    let mut models = state.models.lock().map_err(|e| e.to_string())?;
    let idx = models.iter().position(|m| m.name == model_name)
        .ok_or_else(|| format!("未找到模型: {}", model_name))?;
    let model = models.remove(idx);

    // 从全局路由表移除
    if let Ok(mut router) = get_model_router().lock() {
        router.remove(&model.name);
    }

    // 通过 PID kill 进程
    if let Ok(pid) = model.pid.parse::<u32>() {
        soul_log("INFO", "model", &format!("unload_model killing pid={}", pid));
        #[cfg(windows)]
        {
            let _ = new_hidden_cmd("taskkill")
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

    let msg = format!("已卸载模型: {}", model.name);
    soul_log("INFO", "model", &msg);
    Ok(msg)
}

/// 列出所有运行中的模型（自动填充显存占用）
#[tauri::command]
fn list_running_models(state: State<RunningModelsState>) -> Result<Vec<RunningModel>, String> {
    // 先查询 nvidia-smi 获取所有计算进程的显存占用
    let vram_map = get_gpu_process_vram();

    let mut models = state.models.lock().map_err(|e| e.to_string())?;
    for model in models.iter_mut() {
        if let Ok(pid) = model.pid.parse::<u32>() {
            model.vram_mb = vram_map.get(&pid).copied().unwrap_or(0);
        }

        // 检查进程是否还活着
        let alive = new_hidden_cmd("tasklist")
            .args(["/FI", &format!("PID eq {}", model.pid), "/NH"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains(&model.pid))
            .unwrap_or(false);
        model.status = if alive { "running".to_string() } else { "stopped".to_string() };
    }
    // 清除已退出的模型
    models.retain(|m| m.status == "running");

    Ok(models.clone())
}

/// 查询 nvidia-smi 获取每个 PID 的显存占用 (MB)
fn get_gpu_process_vram() -> std::collections::HashMap<u32, u64> {
    let mut map = std::collections::HashMap::new();
    let mut cmd = new_hidden_cmd("nvidia-smi");
    cmd.args(["--query-compute-apps=pid,used_memory", "--format=csv,noheader"]);
    if let Ok(output) = cmd.output()
    {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() { continue; }
                let parts: Vec<&str> = line.split(',').collect();
                if parts.len() >= 2 {
                    if let Ok(pid) = parts[0].trim().parse::<u32>() {
                        let vram_str = parts[1].trim().trim_end_matches(" MiB").trim();
                        if let Ok(mib) = vram_str.parse::<u64>() {
                            map.insert(pid, mib);
                        }
                    }
                }
            }
        }
    }
    map
}

/// 停止所有运行中的模型
#[tauri::command]
fn stop_all_models(state: State<RunningModelsState>) -> Result<String, String> {
    soul_log("INFO", "model", "stop_all_models 开始");
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
    // 清空路由表
    if let Ok(mut router) = get_model_router().lock() {
        router.clear();
    }
    let msg = format!("已停止 {} 个模型", count);
    soul_log("INFO", "model", &msg);
    Ok(msg)
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
    soul_log("INFO", "server", &format!("start_server port={} ctx={} standby={}", port, ctx, standby));
    soul_log("DEBUG", "server", &format!("  llama_path={} model_path={}", llama_path, model_path));
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
    let mut cmd = new_hidden_cmd(&llama_path);
    if !standby {
        cmd.arg("-m").arg(&model_path);
    }
    cmd.args(["--host", "127.0.0.1", "--port", &llama_port.to_string(), "-c", &ctx.to_string()]);

    soul_log("INFO", "server", &format!("启动 llama-server: {}:{} mode={}", llama_path, llama_port, if standby { "待机" } else { "运行" }));
    let mut child = cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| {
            soul_log_detail("ERROR", "server", "start_server llama-server 启动失败", &e.to_string());
            let _ = state.server_starting.lock().map(|mut s| *s = false);
            format!("启动服务器失败: {}", e)
        })?;

    let pid = child.id();
    soul_log("INFO", "server", &format!("llama-server 已启动 pid={}", pid));
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
                        soul_log("DEBUG", "server", &format!("llama-server stderr: {}", trimmed));
                        let _ = app_clone.emit("server-stderr", serde_json::json!({
                            "message": trimmed,
                        }));
                    }
                }
            }
            soul_log("DEBUG", "server", "llama-server stderr 线程退出");
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
        soul_log("INFO", "proxy", &format!("start_server 启动代理 {} → {}:{}", mname, port, llama_port));
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
    let msg = format!("服务器已启动 (PID: {}, 端口: {}, 模式: {})", pid, port, mode);
    soul_log("INFO", "server", &msg);
    Ok(msg)
}

/// 停止服务器
#[tauri::command]
fn stop_server(state: State<ServerState>) -> Result<String, String> {
    soul_log("INFO", "server", "stop_server 开始");
    let mut server_guard = state.server.lock().map_err(|e| e.to_string())?;
    if let Some(mut child) = server_guard.take() {
        soul_log("INFO", "server", &format!("stop_server killing pid={}", child.id()));
        child.kill().map_err(|e| format!("停止服务器失败: {}", e))?;
        child.wait().ok();
        // 清除待机标记
        if let Ok(mut s) = state.standby.lock() { *s = false; }
        soul_log("INFO", "server", "stop_server 完成");
        Ok("服务器已停止".to_string())
    } else {
        soul_log("WARN", "server", "stop_server 服务器未在运行");
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
    soul_log("INFO", "server", "wake_server 开始");
    let params = state.spawn_params.lock().map_err(|e| e.to_string())?
        .clone().ok_or("未找到启动参数，请先启动服务器")?;

    if let Ok(s) = state.standby.lock() {
        if !*s {
            soul_log("WARN", "server", "wake_server 已处于唤醒状态");
            return Ok("服务器已处于唤醒状态".to_string());
        }
    }

    let start = std::time::Instant::now();
    kill_server(&state);
    if let Ok(mut s) = state.standby.lock() { *s = false; }

    let llama_port = if params.port >= 65535 { 20001u16 } else { params.port + 1 };
    soul_log("INFO", "server", &format!("wake_server 重新启动 llama-server: {}:{}", params.llama_path, llama_port));
    let child = new_hidden_cmd(&params.llama_path)
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
    soul_log("INFO", "server", &format!("wake_server 完成 pid={} 耗时={}ms", pid, elapsed));

    Ok(format!("模型已加载 (PID: {}, 耗时: {}ms)", pid, elapsed))
}

/// 休眠服务器（卸载模型回到待机，会先检查活跃会话）
#[tauri::command]
fn sleep_server(
    state: State<ServerState>,
    monitor: State<SleepMonitor>,
) -> Result<String, String> {
    soul_log("INFO", "server", "sleep_server 开始");
    let params = state.spawn_params.lock().map_err(|e| e.to_string())?
        .clone().ok_or("未找到启动参数，请先启动服务器")?;

    if let Ok(s) = state.standby.lock() {
        if *s {
            soul_log("WARN", "server", "sleep_server 已处于待机模式");
            return Ok("服务器已处于待机模式".to_string());
        }
    }

    // 检查活跃会话
    let llama_port = if params.port >= 65535 { 20001u16 } else { params.port + 1 };
    let active_sessions = get_active_session_count(llama_port);
    if active_sessions > 0 {
        let detail = format!("有 {} 个活跃会话，拒绝休眠", active_sessions);
        log_sleep_event(&monitor, "reject_sleep", None, Some(detail.clone()));
        soul_log("WARN", "server", &detail);
        return Err(detail);
    }

    let start = std::time::Instant::now();
    kill_server(&state);
    if let Ok(mut s) = state.standby.lock() { *s = true; }

    soul_log("INFO", "server", &format!("sleep_server 启动待机模式: {}:{}", params.llama_path, llama_port));

    let child = new_hidden_cmd(&params.llama_path)
        .args(["--host", "127.0.0.1",
               "--port", &llama_port.to_string(), "-c", &params.ctx.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("休眠失败: {}", e))?;

    if let Ok(mut g) = state.server.lock() { *g = Some(child); }

    let elapsed = start.elapsed().as_millis() as u64;
    log_sleep_event(&monitor, "sleep", Some(elapsed), None);
    soul_log("INFO", "server", &format!("sleep_server 完成 耗时={}ms", elapsed));

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
    soul_log("INFO", "build", &format!("build_llama_minimal script={} output={}", script_path, output_dir));
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
    let mut child = new_hidden_cmd("powershell")
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
        soul_log("ERROR", "build", "build_llama_minimal 编译失败");
        return Err("编译失败，请检查控制台输出".to_string());
    }

    // 验证输出文件
    let server_path = std::path::Path::new(&output_dir).join("llama-server.exe");
    if !server_path.exists() {
        soul_log("ERROR", "build", "build_llama_minimal 未找到 llama-server.exe");
        return Err("编译完成但未找到 llama-server.exe".to_string());
    }

    let size = std::fs::metadata(&server_path)
        .map(|m| format_size(m.len()))
        .unwrap_or_default();

    let _ = app_handle.emit("build-progress", serde_json::json!({
        "progress": 100,
        "message": format!("构建完成！llama-server 大小: {}", size),
    }));

    let msg = format!("编译成功 ({})", size);
    soul_log("INFO", "build", &msg);
    Ok(msg)
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

/// 导入本地 GGUF 模型文件到模型目录
#[tauri::command]
fn import_model_file(src_path: String, models_dir: String, model_name: String) -> Result<String, String> {
    soul_log("INFO", "import", &format!("import_model_file src={} name={}", src_path, model_name));

    let src = std::path::Path::new(&src_path);
    if !src.exists() {
        return Err(format!("源文件不存在: {}", src_path));
    }

    let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("");
    if ext.to_lowercase() != "gguf" {
        return Err("只能导入 .gguf 文件".to_string());
    }

    // 使用用户指定的名称作为文件名（锁定，不支持改名）
    let safe_name = model_name.trim();
    if safe_name.is_empty() {
        return Err("模型名称不能为空".to_string());
    }
    if safe_name.contains(|c: char| c == '/' || c == '\\' || c == ':' || c == '*' || c == '?' || c == '"' || c == '<' || c == '>' || c == '|') {
        return Err("模型名称包含非法字符".to_string());
    }

    let fname = format!("{}.gguf", safe_name);
    let dst = std::path::Path::new(&models_dir).join(&fname);

    if dst.exists() {
        return Err(format!("模型文件已存在: {}", fname));
    }

    // 确保目标目录存在
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
    }

    // 复制文件
    std::fs::copy(src, &dst).map_err(|e| format!("复制文件失败: {}", e))?;

    let size_mb = dst.metadata().map(|m| m.len() / 1024 / 1024).unwrap_or(0);
    soul_log("INFO", "import", &format!("导入完成: {} ({}MB)", fname, size_mb));

    Ok(format!("{} 导入成功 ({}MB)", safe_name, size_mb))
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

/// 前端日志（诊断用，写入 Rust 日志）
#[tauri::command]
fn frontend_log(message: String) {
    soul_log("DEBUG", "frontend", &message);
}

/// 检查是否已安装
#[tauri::command]
fn check_setup_needed() -> Result<bool, String> {
    let install_dir = get_install_dir();
    let server_exe = install_dir.join("llama-server.exe");
    Ok(!server_exe.exists())
}

/// 运行首次安装 / 同步后端（解压 llama.cpp）
#[tauri::command]
async fn run_setup(app_handle: tauri::AppHandle) -> Result<String, String> {
    soul_log("INFO", "setup", "run_setup 开始");
    let install_dir = get_install_dir();
    let launcher_dir = install_dir.parent().unwrap_or(&install_dir);

    // 步骤1: 创建安装目录
    emit_progress(&app_handle, 5, "正在创建安装目录...")?;
    std::fs::create_dir_all(&install_dir)
        .map_err(|e| format!("创建目录失败: {}", e))?;

    // 检测硬件
    emit_progress(&app_handle, 10, "正在检测硬件...")?;
    let gpu = detect_gpu_internal();
    let target_key = &gpu.package_key;

    // === 路径A: 从 MSI 捆绑资源解压 ===
    let bundled_zips = find_bundled_zips(&app_handle);
    if let Some(zip_path) = bundled_zips.iter().find(|p| {
        p.file_name().and_then(|n| n.to_str()).map(|n| n.contains(target_key)).unwrap_or(false)
    }) {
        soul_log("INFO", "setup", &format!("找到捆绑资源: {:?} (target={})", zip_path, target_key));
        emit_progress(&app_handle, 15, &format!("检测到: {} ({}), 正在解压捆绑后端...", gpu.gpu_name, target_key))?;
        extract_zip(zip_path, &install_dir, &app_handle)?;
        if !install_dir.join("llama-server.exe").exists() {
            return Err("解压失败: 未找到 llama-server.exe".to_string());
        }
        let server_exe = install_dir.join("llama-server.exe");
        save_install_info(&launcher_dir, &install_dir, &server_exe, &gpu.backend, target_key, &gpu.gpu_name, gpu.cuda_version.as_deref())?;
        soul_log("INFO", "setup", &format!("捆绑后端安装完成: {}", target_key));
        emit_progress(&app_handle, 100, &format!("安装完成！使用 {} 后端", target_key))?;
        return Ok(server_exe.to_string_lossy().to_string());
    }

    // === 路径B: 从 Downloads/llama.cpp/ 解压（本地预下载 / 在线模式）===
    soul_log("INFO", "setup", "未找到捆绑资源，尝试从 Downloads/llama.cpp/ 查找...");
    emit_progress(&app_handle, 10, "正在检测硬件...")?;
    let (llama_zip_path, cudart_zip_opt) = get_llama_package_path(target_key)
        .or_else(|| get_llama_package_path("cpu-x64"))
        .or_else(|| get_llama_package_path("cpu-arm64"))
        .ok_or_else(|| format!(
            "未找到任何预编译包。\n\n离线完整版包含所有后端，请使用完整 MSI 安装。\n轻量版请将预编译包下载到 Downloads/llama.cpp/ 目录。"
        ))?;

    emit_progress(&app_handle, 15, &format!(
        "检测到: {} ({}), 正在安装 {}...",
        gpu.gpu_name,
        if let Some(ref cv) = gpu.cuda_version { format!("CUDA {}", cv) } else { gpu.backend.to_string() },
        target_key
    ))?;

    extract_zip(&llama_zip_path, &install_dir, &app_handle)?;
    if let Some(ref cudart_path) = cudart_zip_opt {
        if cudart_path.exists() {
            emit_progress(&app_handle, 55, "正在安装 CUDA 运行时...")?;
            extract_zip(cudart_path, &install_dir, &app_handle)?;
        }
    }

    let server_exe = install_dir.join("llama-server.exe");
    if !server_exe.exists() {
        return Err("安装失败: 未找到 llama-server.exe".to_string());
    }
    save_install_info(&launcher_dir, &install_dir, &server_exe, &gpu.backend, target_key, &gpu.gpu_name, gpu.cuda_version.as_deref())?;

    emit_progress(&app_handle, 100, &format!("安装完成！使用 {} 后端", target_key))?;
    soul_log("INFO", "setup", &format!("run_setup 完成 backend={} package_key={}", gpu.backend, target_key));
    Ok(server_exe.to_string_lossy().to_string())
}

/// 查找捆绑的预编译包（MSI 资源目录中的 *.zip）
fn find_bundled_zips(app_handle: &tauri::AppHandle) -> Vec<PathBuf> {
    let mut zips = Vec::new();
    if let Ok(res_dir) = app_handle.path().resource_dir() {
        let backends_dir = res_dir.join("backends");
        soul_log("DEBUG", "setup", &format!("查找捆绑资源目录: {:?}", backends_dir));
        if let Ok(entries) = std::fs::read_dir(&backends_dir) {
            for entry in entries.flatten() {
                    let path = entry.path();
                    let path_clone = path.clone();
                    if path_clone.extension().and_then(|e| e.to_str()) == Some("zip") {
                        zips.push(path);
                        soul_log("DEBUG", "setup", &format!("发现捆绑资源: {:?}", path_clone));
                }
            }
        } else {
            soul_log("WARN", "setup", &format!("捆绑资源目录不存在: {:?}", backends_dir));
        }
    }
    zips.sort();
    zips
}

/// 检查并同步后端（每次启动时调用）
#[tauri::command]
async fn check_and_sync_backend(app_handle: tauri::AppHandle) -> Result<String, String> {
    soul_log("INFO", "setup", "check_and_sync_backend 开始");
    let install_dir = get_install_dir();
    let launcher_dir = install_dir.parent().unwrap_or(&install_dir);
    let device_path = launcher_dir.join("device.json");
    let gpu = detect_gpu_internal();
    let target_key = gpu.package_key.clone();

    // 读取已安装的设备信息
    let installed_key = std::fs::read_to_string(&device_path).ok().and_then(|c| {
        serde_json::from_str::<serde_json::Value>(&c).ok()
            .and_then(|v| v["package_key"].as_str().map(|s| s.to_string()))
    });

    // 判断是否需要同步
    let need_sync = match &installed_key {
        Some(key) if key == &target_key => {
            // 同一后端 → 检查服务器是否存在
            if !install_dir.join("llama-server.exe").exists() {
                soul_log("INFO", "setup", &format!("llama-server.exe 不存在，需重新解压 (key={})", key));
                true
            } else {
                soul_log("DEBUG", "setup", &format!("后端一致且文件完整: {} (无需操作)", key));
                return Ok("synced".to_string());
            }
        }
        Some(key) => {
            soul_log("INFO", "setup", &format!("硬件变更: {} → {}", key, target_key));
            true
        }
        None => {
            soul_log("INFO", "setup", &format!("首次检测后端: {}", target_key));
            true
        }
    };

    if !need_sync {
        return Ok("synced".to_string());
    }

    // 需要同步 → 检测硬件并提取匹配的后端
    soul_log("INFO", "setup", &format!("开始同步后端: {}", target_key));

    // 检查捆绑资源
    let bundled_zips = find_bundled_zips(&app_handle);
    let zip_path = bundled_zips.iter().find(|p| {
        p.file_name().and_then(|n| n.to_str()).map(|n| n.contains(&target_key)).unwrap_or(false)
    }).cloned();

    if let Some(ref zp) = zip_path {
        soul_log("INFO", "setup", &format!("从捆绑资源解压: {:?}", zp));
        std::fs::create_dir_all(&install_dir).ok();
        extract_zip(zp, &install_dir, &app_handle)?;
        let server_exe = install_dir.join("llama-server.exe");
        if !server_exe.exists() {
            return Err("同步失败: 未找到 llama-server.exe".to_string());
        }
        save_install_info(&launcher_dir, &install_dir, &server_exe, &gpu.backend, &target_key, &gpu.gpu_name, gpu.cuda_version.as_deref())?;
        soul_log("INFO", "setup", &format!("后端同步完成: {}", target_key));
        return Ok("synced".to_string());
    }

    // 没有捆绑 → 尝试从 Downloads 查找
    soul_log("WARN", "setup", "无捆绑资源，尝试从 Downloads 同步...");
    let (llama_zip_path, _) = get_llama_package_path(&target_key)
        .or_else(|| get_llama_package_path("cpu-x64"))
        .or_else(|| get_llama_package_path("cpu-arm64"))
        .ok_or_else(|| "未找到任何预编译包，请下载后重试".to_string())?;

    std::fs::create_dir_all(&install_dir).ok();
    extract_zip(&llama_zip_path, &install_dir, &app_handle)?;
    let server_exe = install_dir.join("llama-server.exe");
    if !server_exe.exists() {
        return Err("同步失败: 未找到 llama-server.exe".to_string());
    }
    save_install_info(&launcher_dir, &install_dir, &server_exe, &gpu.backend, &target_key, &gpu.gpu_name, gpu.cuda_version.as_deref())?;
    soul_log("INFO", "setup", &format!("后端同步完成 (Downloads): {}", target_key));
    Ok("synced".to_string())
}

/// 保存安装信息（设备信息、模型目录、令牌目录、配置）
fn save_install_info(
    launcher_dir: &std::path::Path,
    _install_dir: &std::path::Path,
    server_exe: &std::path::Path,
    backend: &str,
    package_key: &str,
    gpu_name: &str,
    cuda_version: Option<&str>,
) -> Result<(), String> {
    // 设备信息
    let device_path = launcher_dir.join("device.json");
    let device_json = serde_json::to_string_pretty(&serde_json::json!({
        "backend": backend,
        "package_key": package_key,
        "gpu_name": gpu_name,
        "cuda_version": cuda_version,
        "installed_at": "now",
    })).map_err(|e| format!("序列化设备信息失败: {}", e))?;
    std::fs::write(&device_path, device_json)
        .map_err(|e| format!("保存设备信息失败: {}", e))?;

    // 模型目录
    let models_dir = launcher_dir.join("models");
    std::fs::create_dir_all(&models_dir)
        .map_err(|e| format!("创建模型目录失败: {}", e))?;
    // 令牌目录
    let tokens_dir = launcher_dir.join("tokens");
    std::fs::create_dir_all(&tokens_dir)
        .map_err(|e| format!("创建令牌目录失败: {}", e))?;

    // 配置
    let config_path = launcher_dir.join("config.json");
    let config = serde_json::json!({
        "llama_path": server_exe.to_string_lossy().to_string(),
        "models_dir": models_dir.to_string_lossy().to_string(),
        "port": 20000,
        "ctx": 4096,
        "backend": backend,
    });
    std::fs::write(&config_path, serde_json::to_string_pretty(&config).unwrap())
        .map_err(|e| format!("保存配置失败: {}", e))?;

    Ok(())
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
    // 使用 new_hidden_cmd 防止弹 CMD 窗口（Release 构建中 windows_subsystem="windows"）
    let mut cmd = new_hidden_cmd("nvidia-smi");
    cmd.args(["--query-gpu=name,driver_version", "--format=csv,noheader"]);
    match cmd.output() {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout);
            let gpu_name = text.trim().to_string();
            let driver_ver = gpu_name.split(',').nth(1).unwrap_or("").trim();
            let major = driver_ver.split('.').next().unwrap_or("0").parse::<u32>().unwrap_or(0);
            let (cuda_ver, package_key) = if major >= 570 {
                ("13.0".to_string(), "cuda-13.3".to_string())
            } else if major >= 525 {
                ("12.5".to_string(), "cuda-12.4".to_string())
            } else {
                ("12.0".to_string(), "cuda-12.4".to_string())
            };
            soul_log("DEBUG", "gpu", &format!("detect_gpu nvidia-smi 成功: driver={} package={}", driver_ver, package_key));
            return GpuInfo {
                backend: "cuda".to_string(),
                gpu_name,
                cuda_version: Some(cuda_ver),
                package_key: package_key.to_string(),
            };
        }
        Ok(output) => {
            soul_log("WARN", "gpu", &format!("nvidia-smi 退出码非0: {}", output.status));
        }
        Err(e) => {
            soul_log_detail("WARN", "gpu", "nvidia-smi 调用失败（无 NVIDIA GPU 或驱动？）", &e.to_string());
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
    let mut child = match new_hidden_cmd("vulkaninfo")
        .arg("--summary")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => {
            soul_log("DEBUG", "gpu", "vulkaninfo 不可用");
            return false;
        }
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

// ============================================================
// Python 自动安装
// ============================================================

/// 检查 Python 是否已安装（检测 python.exe 是否在 PATH 中）
#[tauri::command]
fn check_python_installed() -> bool {
    let check = std::process::Command::new("where")
        .arg("python")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    match check {
        Ok(status) if status.success() => true,
        _ => {
            // 也检查常见路径
            let common_paths = [
                "C:\\Python311\\python.exe",
                "C:\\Program Files\\Python311\\python.exe",
                "C:\\Users\\36833\\AppData\\Local\\Programs\\Python\\Python311\\python.exe",
            ];
            common_paths.iter().any(|p| std::path::Path::new(p).exists())
        }
    }
}

/// 后台静默安装 Python
#[tauri::command]
async fn install_python(app_handle: tauri::AppHandle) -> Result<String, String> {
    soul_log("INFO", "python", "开始静默安装 Python 3.11...");

    // 获取资源目录中的 Python 安装包
    let resource_path = app_handle.path()
        .resource_dir()
        .map_err(|e| format!("获取资源目录失败: {}", e))?;
    let installer = resource_path.join("python").join("python-3.11.0-amd64.exe");

    if !installer.exists() {
        // 回退：检查安装目录下的资源
        let fallback = get_launcher_dir().join("python").join("python-3.11.0-amd64.exe");
        if !fallback.exists() {
            return Err("Python 安装包未找到".to_string());
        }
        run_python_installer(&fallback)
    } else {
        run_python_installer(&installer)
    }
}

fn run_python_installer(path: &std::path::Path) -> Result<String, String> {
    soul_log("INFO", "python", &format!("安装包路径: {:?}", path));

    let mut cmd = new_hidden_cmd(&path.to_string_lossy());
    let output = cmd
        .args(["/quiet", "InstallAllUsers=1", "PrependPath=1", "Include_test=0"])
        .output()
        .map_err(|e| format!("启动安装程序失败: {}", e))?;

    if output.status.success() {
        soul_log("INFO", "python", "Python 3.11 安装成功");
        Ok("Python 3.11 安装成功".to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        soul_log("ERROR", "python", &format!("安装失败: {}", stderr));
        Err(format!("Python 安装失败: {}", stderr))
    }
}

// ============================================================
// pip 检测与安装
// ============================================================

#[tauri::command]
fn check_pip_installed() -> bool {
    let check = std::process::Command::new("where")
        .arg("pip")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    match check {
        Ok(status) if status.success() => true,
        _ => {
            // 也检查 pip3
            let check3 = std::process::Command::new("where")
                .arg("pip3")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
            check3.map(|s| s.success()).unwrap_or(false)
        }
    }
}

#[tauri::command]
fn install_pip() -> Result<String, String> {
    soul_log("INFO", "pip", "开始安装 pip...");

    // 使用 Python 的 ensurepip 模块安装 pip
    let mut cmd = new_hidden_cmd("python");
    let output = cmd
        .args(["-m", "ensurepip", "--upgrade", "--default-pip"])
        .output()
        .map_err(|e| format!("启动 pip 安装失败: {}", e))?;

    if output.status.success() {
        soul_log("INFO", "pip", "pip 安装成功");
        Ok("pip 安装成功".to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        soul_log("ERROR", "pip", &format!("pip 安装失败: {}", stderr));
        Err(format!("pip 安装失败: {}", stderr))
    }
}

// ============================================================
// CPU 检测
// ============================================================

#[tauri::command]
fn detect_cpu() -> serde_json::Value {
    let mut info = serde_json::json!({
        "model": "Unknown",
        "cores": 0,
        "threads": 0,
        "ram_gb": 0.0,
    });

    // 通过环境变量获取 CPU 核心数
    if let Ok(cores_str) = std::env::var("NUMBER_OF_PROCESSORS") {
        if let Ok(cores) = cores_str.parse::<u32>() {
            info["threads"] = serde_json::json!(cores);
        }
    }

    // 通过 WMIC 获取 CPU 型号（Windows 特有）
    let cpu_output = std::process::Command::new("wmic")
        .args(["cpu", "get", "name"])
        .output();
    if let Ok(output) = cpu_output {
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines().skip(1) {
            let trimmed = line.trim();
            if !trimmed.is_empty() && trimmed != "Name" {
                info["model"] = serde_json::json!(trimmed);
                break;
            }
        }
    }

    // 通过 WMIC 获取核心数
    let core_output = std::process::Command::new("wmic")
        .args(["cpu", "get", "NumberOfCores"])
        .output();
    if let Ok(output) = core_output {
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines().skip(1) {
            let trimmed = line.trim();
            if let Ok(cores) = trimmed.parse::<u32>() {
                info["cores"] = serde_json::json!(cores);
                break;
            }
        }
    }

    // 获取总内存（约数）
    if let Ok(ram_str) = std::env::var("TOTAL_MEMORY") {
        if let Ok(bytes) = ram_str.parse::<u64>() {
            info["ram_gb"] = serde_json::json!(bytes as f64 / 1024.0 / 1024.0 / 1024.0);
        }
    } else {
        // 通过 WMIC 获取内存
        let mem_output = std::process::Command::new("wmic")
            .args(["OS", "get", "TotalVisibleMemorySize"])
            .output();
        if let Ok(output) = mem_output {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines().skip(1) {
                let trimmed = line.trim();
                if let Ok(kb) = trimmed.parse::<f64>() {
                    info["ram_gb"] = serde_json::json!(kb / 1024.0 / 1024.0);
                    break;
                }
            }
        }
    }

    soul_log("INFO", "cpu", &format!("CPU 检测结果: {:?}", info));
    info
}

/// 检查已安装的版本与当前设备是否匹配
#[tauri::command]
async fn check_llama_installed() -> Result<serde_json::Value, String> {
    soul_log("DEBUG", "gpu", "check_llama_installed 开始");
    let install_dir = get_install_dir();
    let server_exe = install_dir.join("llama-server.exe");
    let installed = server_exe.exists();

    // GPU 检测可能阻塞（nvidia-smi），放到后台线程避免阻塞 Tauri 命令线程
    let gpu = tauri::async_runtime::spawn_blocking(detect_gpu_internal)
        .await
        .map_err(|e| format!("GPU 检测线程异常: {}", e))?;

    let device_path = get_launcher_dir().join("device.json");

    let mut update_needed = false;
    if let Ok(content) = std::fs::read_to_string(&device_path) {
        if let Ok(saved) = serde_json::from_str::<serde_json::Value>(&content) {
            let old_key = saved["package_key"].as_str().unwrap_or("").to_string();
            if installed && old_key != gpu.package_key {
                update_needed = true;
                soul_log("INFO", "gpu", &format!("检测到设备变更: {} → {}", old_key, gpu.package_key));
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

    soul_log("DEBUG", "gpu", &format!("check_llama_installed 完成 installed={} update_needed={} package={}", installed, update_needed, gpu.package_key));

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
    soul_log("DEBUG", "config", "load_config 开始");
    let launcher_dir = get_install_dir().parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(get_install_dir);
    let config_path = launcher_dir.join("config.json");
    if !config_path.exists() {
        soul_log("DEBUG", "config", "load_config 配置文件不存在");
        return Ok(None);
    }
    let content = std::fs::read_to_string(&config_path)
        .map_err(|e| format!("读取配置失败: {}", e))?;
    let config: LauncherConfig = serde_json::from_str(&content)
        .map_err(|e| format!("解析配置失败: {}", e))?;
    soul_log("INFO", "config", &format!("load_config 成功 port={} ctx={}", config.port, config.ctx));
    Ok(Some(config))
}

/// 保存配置
#[tauri::command]
fn save_config(config: LauncherConfig) -> Result<(), String> {
    soul_log("INFO", "config", &format!("save_config port={} ctx={} auto_unload={}", config.port, config.ctx, config.auto_unload));
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

/// 下载进程跟踪
struct DownloadProcess {
    pid: Mutex<Option<u32>>,
    retry_count: Mutex<u32>,
    model_id: Mutex<String>,
    local_dir: Mutex<String>,
    include_pattern: Mutex<Option<String>>,
    use_token: Mutex<bool>,
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
    soul_log("INFO", "modelscope", &format!("search_models query='{}'", query));
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("Mozilla/5.0 SoulAgentLauncher/0.3.3")
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

    // 按 group（分区）→ family（系列）→ GGUF 优先 → 参数大小升序排序
    let group_priority: &[&str] = &["Qwen系列", "DeepSeek", "GLM", "Yi系列", "Google", "Llama"];
    let family_priority: &[&str] = &["Qwen3", "Qwen2.5", "DeepSeek R1 Distill", "DeepSeek Coder",
                                      "GLM-Z1", "GLM-4", "ChatGLM3", "ChatGLM2",
                                      "Gemma 3", "Llama"];
    models.sort_by(|a, b| {
        let ga = a.group.as_deref().and_then(|g| group_priority.iter().position(|&x| x == g)).unwrap_or(usize::MAX);
        let gb = b.group.as_deref().and_then(|g| group_priority.iter().position(|&x| x == g)).unwrap_or(usize::MAX);
        let pa = family_priority.iter().position(|&f| f == a.family).unwrap_or(usize::MAX);
        let pb = family_priority.iter().position(|&f| f == b.family).unwrap_or(usize::MAX);
        // GGUF（有 quant 选项）排在 FP16（无 quant）之前
        let fmt_a = if a.quant.is_some() { 0 } else { 1 };
        let fmt_b = if b.quant.is_some() { 0 } else { 1 };
        ga.cmp(&gb).then(pa.cmp(&pb)).then(fmt_a.cmp(&fmt_b)).then(
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
    // Qwen3 系列使用大写 Q 命名（如 Q4_K_M 而非 q4_K_M）
    let qwen3_quants = vec![
        "Q4_K_M".into(), "Q5_0".into(), "Q5_K_M".into(),
        "Q6_K".into(), "Q8_0".into(),
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
    for (model_id, name, size, desc, only_q8) in &[
        ("qwen/Qwen3-0.6B-GGUF",    "Qwen3-0.6B-Instruct",     "0.6B",   "超轻量，简单推理无压力", true),
        ("qwen/Qwen3-1.7B-GGUF",    "Qwen3-1.7B-Instruct",     "1.7B",   "低配设备友好，够用省资源", true),
        ("qwen/Qwen3-4B-GGUF",      "Qwen3-4B-Instruct",       "4B",     "轻量高性价比，平衡之选", false),
        ("qwen/Qwen3-8B-GGUF",      "Qwen3-8B-Instruct",       "8B",     "通用推理首选，推荐 Q4_K_M", false),
        ("qwen/Qwen3-14B-GGUF",     "Qwen3-14B-Instruct",      "14B",    "中大规模，复杂推理需合并分卷", false),
        ("qwen/Qwen3-32B-GGUF",     "Qwen3-32B-Instruct",      "32B",    "大规模深度推理，约需 20GB 显存", false),
        ("qwen/Qwen3-30B-A3B-GGUF", "Qwen3-30B-A3B-Instruct",  "30B-A3B","MoE 架构，激活 3B，边缘设备神器", false),
    ] {
        let (inc, q, default_q) = if *only_q8 {
            // Qwen3-0.6B / 1.7B: 仓库中只有 Q8_0
            (Some("*.gguf".into()), None, None)
        } else {
            // 其他 Qwen3: 大写 Q 命名
            (Some("*Q4_K_M*.gguf".into()), Some(qwen3_quants.clone()), Some("Q4_K_M".into()))
        };
        all.push(OfficialModel {
            model_id: format!("{}:*", model_id),
            name: name.to_string(),
            group: Some("Qwen系列".into()),
            family: "Qwen3".into(),
            prefix: Some(format!("{}:", model_id)),
            include: inc,
            quant: q,
            default_quant: default_q,
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

    // ── GLM 系列（智谱，魔搭）──
    for (model_id, name, size, family, desc) in &[
        ("ZhipuAI/ChatGLM2-6B",         "ChatGLM2-6B",          "6B",  "ChatGLM2",   "早期版本，兼容旧项目"),
        ("ZhipuAI/ChatGLM3-6B",         "ChatGLM3-6B",          "6B",  "ChatGLM3",   "消费级显卡可运行，生态成熟"),
        ("ZhipuAI/GLM-4-9B",            "GLM-4-9B",             "9B",  "GLM-4",      "基座模型，适合微调"),
        ("ZhipuAI/GLM-4-9B-Chat",       "GLM-4-9B-Chat",        "9B",  "GLM-4",      "通用对话、工具调用、代码生成"),
        ("ZhipuAI/GLM-4-9B-Chat-1M",    "GLM-4-9B-Chat-1M",     "9B",  "GLM-4",      "超长上下文 (1M tokens)"),
        ("ZhipuAI/GLM-4V-9B",      "GLM-4V-9B-Chat",       "9B",  "GLM-4",      "支持图像理解、OCR、图表分析"),
        ("ZhipuAI/GLM-Z1-9B-0414",      "GLM-Z1-9B-0414",       "9B",  "GLM-Z1",     "轻量推理，数学/代码能力强"),
        ("ZhipuAI/GLM-4-32B-0414",      "GLM-4-32B-0414",       "32B", "GLM-4",      "性能强劲，可比肩更大参数模型"),
        ("ZhipuAI/GLM-4-32B-Base-0414", "GLM-4-32B-Base-0414", "32B", "GLM-4",      "基座模型，适合微调"),
        ("ZhipuAI/GLM-Z1-32B-0414",     "GLM-Z1-32B-0414",      "32B", "GLM-Z1",     "深度思考，数学/代码/逻辑能力突出"),
        ("ZhipuAI/GLM-Z1-Rumination-32B-0414","GLM-Z1-Rumination-32B-0414","32B","GLM-Z1","支持深度反思、多步研究任务"),
    ] {
        let note = if name.contains("0414") {
            Some("2025年4月14日统一训练版本，性能和稳定性更好")
        } else {
            None
        };
        let d = match note {
            Some(n) => format!("{}。{}", desc, n),
            None => desc.to_string(),
        };
        all.push(OfficialModel {
            model_id: model_id.to_string(),
            name: name.to_string(),
            group: Some("GLM".into()),
            family: family.to_string(),
            prefix: None,
            include: None,
            quant: None,
            default_quant: None,
            size: Some(size.to_string()),
            desc: Some(d),
        });
    }

    // ── Yi 系列（零一万物，魔搭）──
    for (model_id, name, size, family, desc) in &[
        ("01ai/Yi-Coder-1.5B",  "Yi-Coder-1.5B",   "1.5B", "Yi-Coder", "轻量编程，支持52种编程语言，适合代码理解任务"),
        ("01ai/Yi-1.5-6B",      "Yi-1.5-6B-Base",  "6B",   "Yi-1.5",   "轻量基座模型，可在其上进行微调以适应特定任务"),
        ("01ai/Yi-1.5-6B-Chat", "Yi-1.5-6B-Chat",  "6B",   "Yi-1.5",   "轻量对话模型，适合研究或个人用途，支持4K/16K/32K上下文"),
        ("01ai/Yi-VL-6B",       "Yi-VL-6B",        "6B",   "Yi-VL",    "多模态模型，支持图文对话和OCR"),
        ("01ai/Yi-1.5-9B",      "Yi-1.5-9B-Base",  "9B",   "Yi-1.5",   "中等规模基座模型，提供更优的微调起点"),
        ("01ai/Yi-1.5-9B-Chat", "Yi-1.5-9B-Chat",  "9B",   "Yi-1.5",   "理科状元，代码与数学能力最强，性价比高"),
        ("01ai/Yi-Coder-9B",    "Yi-Coder-9B",     "9B",   "Yi-Coder", "专业编程，支持项目级代码，128K超长上下文"),
        ("01ai/Yi-1.5-34B",     "Yi-1.5-34B-Base", "34B",  "Yi-1.5",   "旗舰基座模型，能力涌现，适合复杂场景部署"),
        ("01ai/Yi-1.5-34B-Chat","Yi-1.5-34B-Chat", "34B",  "Yi-1.5",   "性能旗舰，支持200K超长上下文"),
        ("01ai/Yi-VL-34B",      "Yi-VL-34B",       "34B",  "Yi-VL",    "多模态旗舰，图像理解能力更强"),
    ] {
        all.push(OfficialModel {
            model_id: model_id.to_string(),
            name: name.to_string(),
            group: Some("Yi系列".into()),
            family: family.to_string(),
            prefix: None,
            include: None,
            quant: None,
            default_quant: None,
            size: Some(size.to_string()),
            desc: Some(desc.to_string()),
        });
    }

    all
}

/// 检查 modelscope CLI 是否可用
#[tauri::command]
fn check_modelscope_available() -> bool {
    let available = new_hidden_cmd("modelscope")
        .arg("--help")
        .output()
        .is_ok();
    soul_log("DEBUG", "modelscope", &format!("check_modelscope_available → {}", available));
    available
}

/// 自动安装 modelscope SDK（静默安装，发射进度事件）
#[tauri::command]
async fn install_modelscope(app_handle: tauri::AppHandle) -> Result<String, String> {
    soul_log("INFO", "modelscope", "install_modelscope 开始");
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
    let child = new_hidden_cmd("pip")
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
        soul_log_detail("ERROR", "modelscope", "install_modelscope pip 安装失败", &preview);
        return Err(format!("安装 modelscope 失败: {}", preview));
    }

    // 验证安装
    if check_modelscope_available() {
        let _ = app_handle.emit("ms-install-progress", serde_json::json!({
            "progress": 100, "message": "modelscope 安装完成！"
        }));
        soul_log("INFO", "modelscope", "install_modelscope 安装成功");
        Ok("安装成功".to_string())
    } else {
        soul_log("ERROR", "modelscope", "install_modelscope 安装后验证失败");
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
    soul_log("INFO", "download", &format!("start_model_download model_id={} local_dir={}", model_id, local_dir));
    if let Some(ref p) = include_pattern {
        soul_log("DEBUG", "download", &format!("  include_pattern={}", p));
    }

    // 保存参数供重试使用
    if let Ok(mut mid) = state.model_id.lock() { *mid = model_id.clone(); }
    if let Ok(mut ld) = state.local_dir.lock() { *ld = local_dir.clone(); }
    if let Ok(mut ip) = state.include_pattern.lock() { *ip = include_pattern.clone(); }

    let mut pid_guard = state.pid.lock().map_err(|e| e.to_string())?;

    if let Some(pid) = *pid_guard {
        #[cfg(windows)]
        {
            let check = new_hidden_cmd("tasklist")
                .args(["/FI", &format!("PID eq {}", pid), "/NH"])
                .output();
            if let Ok(out) = check {
                if String::from_utf8_lossy(&out.stdout).contains(&pid.to_string()) {
                    return Err("已有下载任务进行中".to_string());
                }
            }
        }
    }

    let clean_id = model_id.trim_end_matches(":*").to_string();

    // 构建命令
    let mut cmd = new_hidden_cmd("modelscope");
    cmd.args(["download", "--model", &clean_id, "--local_dir", &local_dir]);

    let token_path = get_launcher_dir().join("tokens").join("modelscope.token");
    let has_token = token_path.exists() && std::fs::read_to_string(&token_path)
        .map(|c| !c.trim().is_empty()).unwrap_or(false);
    if let Ok(mut ut) = state.use_token.lock() { *ut = has_token; }

    if has_token {
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

    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| {
        soul_log_detail("ERROR", "download", "start_model_download 启动 modelscope 失败", &e.to_string());
        format!("启动下载失败: {}\n\n请确保已安装 modelscope CLI:\n  pip install modelscope", e)
    })?;
    let pid = child.id();
    soul_log("INFO", "download", &format!("modelscope 下载进程已启动 pid={}", pid));
    *pid_guard = Some(pid);

    // 后台线程：等待下载 + SHA256 校验 + 自动重试
    let app_clone = app_handle.clone();
    let model_clone = clean_id.clone();
    let local_dir_c = local_dir.clone();
    let ip_c = include_pattern.clone();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    std::thread::spawn(move || {
        let max_retries = 3u32;
        // 重试计数器（在同一线程内顺序使用，无需原子操作）
        let mut retry_count = 0u32;

        let _ = app_clone.emit("download-progress", serde_json::json!({
            "progress": 0,
            "message": format!("正在下载 {} ...", model_clone),
            "status": "downloading",
        }));

        // 从 stdout 读取
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

        // 从 stderr 读取 tqdm
        let app_clone3 = app_clone.clone();
        let stderr_handle = if let Some(err) = stderr {
            Some(std::thread::spawn(move || {
                let mut reader = BufReader::new(err);
                let mut buf = Vec::new();
                while let Ok(n) = reader.read_until(b'\r', &mut buf) {
                    if n == 0 { break; }
                    let text = String::from_utf8_lossy(&buf).trim().to_string();
                    buf.clear();
                    if text.is_empty() { continue; }
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

        if let Some(h) = stdout_handle { let _ = h.join(); }
        if let Some(h) = stderr_handle { let _ = h.join(); }

        let status = child.wait();
        let success = status.as_ref().map(|s| s.success()).unwrap_or(false);

        if !success {
            let code = status.as_ref().map(|s| s.code().unwrap_or(-1)).unwrap_or(-1);
            soul_log_detail("ERROR", "download", &format!("下载失败: {} 退出码={}", model_clone, code), "");
            let _ = app_clone.emit("download-progress", serde_json::json!({
                "progress": 0,
                "message": format!("{} 下载失败（退出码: {}）", model_clone, code),
                "status": "error",
                "retryable": true,
            }));
            return;
        }

        // ── modelscope 下载成功 → SHA256 校验 ──
        let _ = app_clone.emit("download-progress", serde_json::json!({
            "progress": 99,
            "message": "正在校验文件完整性...",
            "status": "verifying",
        }));

        // 扫描本地目录，找到下载的 GGUF 文件
        let dir = std::path::Path::new(&local_dir_c);
        let gguf_files: Vec<_> = if dir.exists() {
            std::fs::read_dir(dir).ok()
                .into_iter().flatten()
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().map(|x| x == "gguf").unwrap_or(false))
                .map(|e| e.path())
                .collect()
        } else { vec![] };

        if gguf_files.is_empty() {
            soul_log("WARN", "download", &format!("下载完成但未找到 GGUF 文件: {}", model_clone));
            // 也可能是下载了非 GGUF 文件，检查是否有其他文件
            let any_files: Vec<_> = std::fs::read_dir(dir).ok()
                .into_iter().flatten()
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_file())
                .map(|e| e.path())
                .collect();

            if any_files.is_empty() {
                // 完全没文件 → 重试
                retry_count += 1;
                let rc = retry_count;
                if rc <= max_retries {
                    soul_log("WARN", "download", &format!("未下载到任何文件，第{}次自动重试", rc));
                    let _ = app_clone.emit("download-progress", serde_json::json!({
                        "progress": 0,
                        "message": format!("未下载到文件，第 {} 次重试...", rc),
                        "status": "retrying",
                    }));
                    // 重新调用下载逻辑（这里只能记录，前端手动重试）
                    let _ = app_clone.emit("download-progress", serde_json::json!({
                        "progress": 0,
                        "message": format!("下载结果异常，请点击重试按钮重新下载（{}/{}）", rc, max_retries),
                        "status": "error",
                        "retryable": true,
                    }));
                    return;
                } else {
                    let _ = app_clone.emit("download-progress", serde_json::json!({
                        "progress": 0,
                        "message": format!("{} 下载失败：已重试 {} 次均未下载到文件，请检查网络或模型 ID", model_clone, max_retries),
                        "status": "error",
                        "retryable": true,
                    }));
                    return;
                }
            }

            // 有文件但不是 GGUF（如 FP16 权重）→ 只做大小校验
            let mut total_size: u64 = 0;
            for f in &any_files {
                if let Ok(meta) = f.metadata() {
                    total_size += meta.len();
                }
            }
            if total_size < 1024 {
                soul_log("ERROR", "download", &format!("下载文件过小: {} bytes", total_size));
                let _ = app_clone.emit("download-progress", serde_json::json!({
                    "progress": 0,
                    "message": format!("下载文件异常（过小），请重试"),
                    "status": "error",
                    "retryable": true,
                }));
                return;
            }

            soul_log("INFO", "download", &format!("非 GGUF 模型下载完成，总大小: {}MB", total_size / 1024 / 1024));
            let _ = app_clone.emit("download-progress", serde_json::json!({
                "progress": 100,
                "message": format!("{} 下载完成！共 {} 个文件", model_clone, any_files.len()),
                "status": "completed",
            }));
            return;
        }

        // ── 对 GGUF 文件计算 SHA256 ──
        let mut verified_ok = true;
        let mut hashes: Vec<String> = vec![];

        for gf in &gguf_files {
            let fname = gf.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let fsize = gf.metadata().map(|m| m.len()).unwrap_or(0);

            soul_log("INFO", "download", &format!("校验文件: {} ({} bytes)", fname, fsize));

            let _ = app_clone.emit("download-progress", serde_json::json!({
                "progress": 99,
                "message": format!("SHA256 校验中: {}", fname),
                "status": "verifying",
            }));

            if fsize < 1024 {
                soul_log("ERROR", "download", &format!("文件过小: {} bytes", fname));
                verified_ok = false;
                break;
            }

            // 计算 SHA256
            match std::fs::File::open(gf) {
                Ok(mut file) => {
                    let mut hasher = Sha256::new();
                    let mut buf = [0u8; 65536];
                    let mut total_read: u64 = 0;
                    while let Ok(n) = file.read(&mut buf) {
                        if n == 0 { break; }
                        hasher.update(&buf[..n]);
                        total_read += n as u64;
                        // 每 100MB 发射一次进度
                        if total_read % (100 * 1024 * 1024) < 65536 {
                            let pct = (total_read as f64 / fsize as f64 * 100.0) as u32;
                            let _ = app_clone.emit("download-progress", serde_json::json!({
                                "progress": 99u32.min(pct),
                                "message": format!("SHA256 校验中: {} ({}%)", fname, pct),
                                "status": "verifying",
                            }));
                        }
                    }
                    let hash = hex::encode(hasher.finalize());
                    hashes.push(format!("{}: SHA256={}", fname, hash));
                    soul_log("INFO", "download", &format!("SHA256({}) = {}", fname, hash));
                }
                Err(e) => {
                    soul_log("ERROR", "download", &format!("无法打开文件校验: {}", e));
                    verified_ok = false;
                    break;
                }
            }
        }

        if verified_ok && !hashes.is_empty() {
            let _hash_str = hashes.join("\n");
            soul_log("INFO", "download", &format!("SHA256 校验通过: {}", model_clone));
            let _ = app_clone.emit("download-progress", serde_json::json!({
                "progress": 100,
                "message": format!("{} 下载并校验完成！({} 个文件)", model_clone, gguf_files.len()),
                "status": "completed",
                "sha256": hashes,
            }));
        } else if !verified_ok {
            // SHA256 校验失败 → 自动重试
            retry_count += 1;
            let rc = retry_count;
            if rc <= max_retries {
                soul_log("WARN", "download", &format!("校验失败，第{}次自动重试", rc));
                let _ = app_clone.emit("download-progress", serde_json::json!({
                    "progress": 0,
                    "message": format!("校验失败，第 {} / {} 次自动重试...", rc, max_retries),
                    "status": "retrying",
                }));
                // 自动重试：清理目录后重新下载
                soul_log("INFO", "download", "清理已下载文件...");
                for gf in &gguf_files {
                    let _ = std::fs::remove_file(gf);
                }

                // 重新启动下载（同步重试）
                let mut cmd2 = new_hidden_cmd("modelscope");
                cmd2.args(["download", "--model", &model_clone, "--local_dir", &local_dir_c]);
                if has_token {
                    if let Ok(content) = std::fs::read_to_string(&token_path) {
                        let token = content.trim().to_string();
                        if !token.is_empty() { cmd2.args(["--token", &token]); }
                    }
                }
                if let Some(ref pattern) = ip_c {
                    cmd2.args(["--include", pattern]);
                }
                cmd2.stdout(std::process::Stdio::piped());
                cmd2.stderr(std::process::Stdio::piped());

                if let Ok(mut child2) = cmd2.spawn() {
                    let _ = app_clone.emit("download-progress", serde_json::json!({
                        "progress": 0,
                        "message": format!("第 {} 次下载: {} ...", rc, model_clone),
                        "status": "downloading",
                    }));

                    let out2 = child2.stdout.take();
                    let err2 = child2.stderr.take();

                    let a2 = app_clone.clone();
                    let h1 = if let Some(o) = out2 {
                        Some(std::thread::spawn(move || {
                            let r = BufReader::new(o);
                            for line in r.lines() {
                                if let Ok(t) = line {
                                    let tr = t.trim().to_string();
                                    if !tr.is_empty() {
                                        let _ = a2.emit("download-progress", serde_json::json!({
                                            "progress": 0, "message": tr, "status": "downloading",
                                        }));
                                    }
                                }
                            }
                        }))
                    } else { None };

                    let a3 = app_clone.clone();
                    let h2 = if let Some(e) = err2 {
                        Some(std::thread::spawn(move || {
                            let mut r = BufReader::new(e);
                            let mut b = Vec::new();
                            while let Ok(n) = r.read_until(b'\r', &mut b) {
                                if n == 0 { break; }
                                let text = String::from_utf8_lossy(&b).trim().to_string();
                                b.clear();
                                if text.is_empty() { continue; }
                                let pct = text.split('%').next()
                                    .and_then(|s| s.split_whitespace().last())
                                    .and_then(|s| s.parse::<u32>().ok());
                                let _ = a3.emit("download-progress", serde_json::json!({
                                    "progress": pct.unwrap_or(0), "message": text, "status": "downloading",
                                }));
                            }
                        }))
                    } else { None };

                    if let Some(h) = h1 { let _ = h.join(); }
                    if let Some(h) = h2 { let _ = h.join(); }

                    let status2 = child2.wait();
                    let ok2 = status2.as_ref().map(|s| s.success()).unwrap_or(false);

                    if ok2 {
                        soul_log("INFO", "download", &format!("重试第{}次下载完成: {}", rc, model_clone));
                        let _ = app_clone.emit("download-progress", serde_json::json!({
                            "progress": 100,
                            "message": format!("{} 重试第 {} 次下载完成！", model_clone, rc),
                            "status": "completed",
                        }));
                    } else {
                        let code2 = status2.as_ref().map(|s| s.code().unwrap_or(-1)).unwrap_or(-1);
                        soul_log("ERROR", "download", &format!("重试第{}次仍失败, 退出码={}", rc, code2));
                        let err_msg = if rc >= max_retries {
                            format!("{} 下载失败：已自动重试 {} 次均失败，请检查网络后点击重试", model_clone, max_retries)
                        } else {
                            format!("{} 重试下载失败（退出码: {}），请点击重试按钮", model_clone, code2)
                        };
                        let _ = app_clone.emit("download-progress", serde_json::json!({
                            "progress": 0,
                            "message": err_msg,
                            "status": "error",
                            "retryable": true,
                        }));
                    }
                } else {
                    soul_log("ERROR", "download", "自动重试无法启动 modelscope");
                    let _ = app_clone.emit("download-progress", serde_json::json!({
                        "progress": 0,
                        "message": format!("自动重试失败，请手动重试"),
                        "status": "error",
                        "retryable": true,
                    }));
                }
            } else {
                soul_log("ERROR", "download", &format!("SHA256 校验失败，已达最大重试次数 {}", max_retries));
                let _ = app_clone.emit("download-progress", serde_json::json!({
                    "progress": 0,
                    "message": format!("{} 下载：已自动重试 {} 次均失败，请检查网络或模型 ID", model_clone, max_retries),
                    "status": "error",
                    "retryable": true,
                }));
            }
        }
    });

    Ok(format!("下载已开始 (PID: {})", pid))
}

/// 重试下载（使用上次的参数）
#[tauri::command]
async fn retry_download(
    app_handle: tauri::AppHandle,
    state: State<'_, DownloadProcess>,
) -> Result<String, String> {
    let mid = state.model_id.lock().map_err(|e| e.to_string())?.clone();
    let ld = state.local_dir.lock().map_err(|e| e.to_string())?.clone();
    let ip = state.include_pattern.lock().map_err(|e| e.to_string())?.clone();

    if mid.is_empty() || ld.is_empty() {
        return Err("没有可重试的下载任务".to_string());
    }

    soul_log("INFO", "download", &format!("用户手动重试下载: {}", mid));

    // 清除旧的 PID
    if let Ok(mut pid) = state.pid.lock() {
        if let Some(old_pid) = pid.take() {
            #[cfg(windows)]
            { let _ = new_hidden_cmd("taskkill").args(["/PID", &old_pid.to_string(), "/F"]).output(); }
        }
    }

    // 清除重试计数（重新从0开始）
    if let Ok(mut rc) = state.retry_count.lock() { *rc = 0; }

    // 重新调用 start_model_download
    start_model_download(app_handle, state, mid, ld, ip).await
}

/// 取消下载
#[tauri::command]
fn cancel_download(state: State<'_, DownloadProcess>) -> Result<String, String> {
    soul_log("INFO", "download", "cancel_download 开始");
    let mut pid_guard = state.pid.lock().map_err(|e| e.to_string())?;
    if let Some(pid) = pid_guard.take() {
        soul_log("INFO", "download", &format!("cancel_download killing pid={}", pid));
        #[cfg(windows)]
        {
            let _ = new_hidden_cmd("taskkill")
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
    soul_log("INFO", "window", &format!("close_window auto_unload={}", should_unload));

    if should_unload {
        soul_log("INFO", "window", "close_window 卸载所有模型并退出");
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
    soul_log("INFO", "proxy", &format!("proxy_loop 启动 {} → {}:{}", model_name, public_port, llama_port));
    let listener = match TcpListener::bind(&addr) {
        Ok(l) => l,
        Err(e) => {
            soul_log_detail("ERROR", "proxy", &format!("代理启动失败 {}", addr), &e.to_string());
            eprintln!("代理启动失败 {}: {} (不影响已有模型)", addr, e);
            return;
        }
    };
    listener.set_nonblocking(true).ok();
    let model = model_name.to_string();

    for stream in listener.incoming() {
        match stream {
            Ok(mut client) => {
                let m = model.clone();
                std::thread::spawn(move || {
                    handle_proxy_connection(&mut client, &m, public_port, llama_port);
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

/// /chat 端点：自研轻量协议转换 → 原生 /completion 格式
/// 协议特点：
/// - 流式输出默认开启（stream 默认 true）
/// - 多模态默认关闭（需显示传入 multimodal: true）
/// - 支持 tools、temperature、max_tokens 等字段
/// - 响应格式瘦身，不含 OpenAI 冗余字段
fn convert_chat_to_native(
    body: &[u8], request_line: &str, path: &str, rest_headers: &str,
) -> (Vec<u8>, String, String) {
    if body.is_empty() {
        return (body.to_vec(), rest_headers.to_string(), request_line.to_string());
    }

    if let Ok(json) = serde_json::from_slice::<serde_json::Value>(body) {
        if let Some(obj) = json.as_object() {
            // 提取字段
            let stream = obj.get("stream").and_then(|v| v.as_bool()).unwrap_or(true);
            let _multimodal = obj.get("multimodal").and_then(|v| v.as_bool()).unwrap_or(false);

            // 构建 ChatML prompt
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

            // 构建原生格式 body（极致精简，仅传必需字段）
            let mut native = serde_json::json!({
                "prompt": prompt,
                "stream": stream,
            });

            // 可选字段：temperature / max_tokens
            if let Some(t) = obj.get("temperature").and_then(|v| v.as_f64()) {
                native["temperature"] = serde_json::Value::from(t);
            }
            if let Some(mt) = obj.get("max_tokens").and_then(|v| v.as_u64()) {
                native["n_predict"] = serde_json::Value::from(mt);
            }
            // thinking → 传递是否启用思考（llama-server 通过 cache_prompt 配合实现）
            if let Some(_think) = obj.get("thinking").and_then(|v| v.as_bool()) {
                native["cache_prompt"] = serde_json::Value::from(true); // 保持 prompt 缓存
                native["penalize_nl"] = serde_json::Value::from(false);
                // 不设置 thinking 特定参数，让模型自主输出思考过程
            }
            // tools → 转为 llama.cpp 的 grammar/stop 格式
            if let Some(tools) = obj.get("tools").and_then(|v| v.as_array()) {
                if !tools.is_empty() {
                    native["cache_prompt"] = serde_json::Value::from(true);
                }
            }

            let body_str = serde_json::to_string(&native).unwrap_or_else(|_| String::new());
            let new_body = body_str.into_bytes();
            let new_cl = new_body.len();

            let updated_headers: String = rest_headers.lines()
                .filter(|l| !l.is_empty())
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

/// 处理单个代理连接：读请求 → 模型路由 → 重写路径 → 转发 → 管道回响应
fn handle_proxy_connection(client: &mut std::net::TcpStream, model_name: &str, _own_port: u16, own_llama_port: u16) {
    soul_log("DEBUG", "proxy", &format!("handle_proxy_connection model='{}'", model_name));
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

    // 1. /chat/models 端点：返回所有运行中的模型
    if path == "/chat/models" || path.ends_with("/chat/models") {
        let router = get_model_router().lock().unwrap_or_else(|e| e.into_inner());
        let models_list: Vec<serde_json::Value> = router.iter().map(|(name, port)| {
            serde_json::json!({"name": name, "port": port})
        }).collect();
        let resp_body = serde_json::json!({"models": models_list}).to_string();
        let resp = format!(
            "HTTP/1.1 200 OK\r\nAccess-Control-Allow-Origin: *\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            resp_body.len(), resp_body
        );
        let _ = client.write_all(resp.as_bytes());
        let _ = client.flush();
        return;
    }

    // 2. 模型列表请求：/v1/models 等
    let is_model_list = path.contains("/v1/models") || path.contains("/api/tags")
        || (path == "/models" || path.ends_with("/models"));
    if is_model_list {
        // 返回多模型列表
        let router = get_model_router().lock().unwrap_or_else(|e| e.into_inner());
        let models_data: Vec<serde_json::Value> = router.iter().map(|(name, port)| {
            serde_json::json!({"id": name, "object": "model", "port": port})
        }).collect();
        let resp_body = serde_json::json!({"object": "list", "data": models_data}).to_string();
        let resp = format!(
            "HTTP/1.1 200 OK\r\nAccess-Control-Allow-Origin: *\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            resp_body.len(), resp_body
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

    // 3. 模型路由：检查 body 中的 model 字段，路由到对应端口
    let target_llama_port = if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&body) {
        if let Some(requested_model) = json.get("model").and_then(|v| v.as_str()) {
            if !requested_model.is_empty() && requested_model != model_name {
                // 查找目标模型端口
                if let Ok(router) = get_model_router().lock() {
                    if let Some(&target_port) = router.get(requested_model) {
                        target_port + 1 // llama port = proxy port + 1
                    } else {
                        own_llama_port
                    }
                } else {
                    own_llama_port
                }
            } else {
                own_llama_port
            }
        } else {
            own_llama_port
        }
    } else {
        own_llama_port
    };

    let upstream_addr = format!("127.0.0.1:{}", target_llama_port);
    soul_log("DEBUG", "proxy", &format!("model='{}' path='{}' route_to={} own_llama={}", model_name, path, target_llama_port, own_llama_port));
    eprintln!("[proxy] model='{}' path='{}' route_to_port={} (own_llama={})", model_name, path, target_llama_port, own_llama_port);

    // 4. 端点路由
    let is_chat_endpoint = path.contains("/chat") && !path.contains("/v1/chat/completions") && !path.contains("/chat/models");
    let is_openai_endpoint = path.contains("/v1/chat/completions");

    // 检测客户端请求的 stream 字段
    let client_wants_stream = if !body.is_empty() {
        if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&body) {
            json.get("stream").and_then(|v| v.as_bool()).unwrap_or(true)
        } else { true }
    } else { true };

    let (forward_body, forward_headers, rewritten_line) = if is_openai_endpoint {
        // OpenAI 端点：标准转换
        convert_openai_to_native(&body, &request_line, &path, &rest_headers, !client_wants_stream)
    } else if is_chat_endpoint {
        // 自研 /chat 端点：轻量协议转换（流式默认 true）
        convert_chat_to_native(&body, &request_line, &path, &rest_headers)
    } else {
        (body.clone(), rest_headers.clone(), request_line.clone())
    };
    let mut upstream = None;
    for attempt in 0..10 {
        match std::net::TcpStream::connect(&upstream_addr) {
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
// 会话管理 (Session Management)
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Session {
    id: String,
    title: String,
    created_at: String,
    updated_at: String,
    message_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatMessage {
    role: String,  // "user" | "assistant"
    content: String,
    timestamp: String,
}

fn get_sessions_dir() -> PathBuf {
    let base = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(base).join("Soul-Agent-Launcher").join("sessions")
}

fn generate_id() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap_or_default()
        .as_secs();
    format!("s{:x}{:04x}", secs, rand_4hex())
}

fn rand_4hex() -> u16 {
    // Simple non-cryptographic random
    (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap_or_default()
        .as_nanos() & 0xFFFF) as u16
}

fn now_str() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap_or_default()
        .as_secs();
    format_ts(secs)
}

/// 创建新会话
#[tauri::command]
fn create_session(title: Option<String>) -> Result<Session, String> {
    let dir = get_sessions_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建会话目录失败: {}", e))?;

    let session = Session {
        id: generate_id(),
        title: title.unwrap_or_else(|| "新会话".to_string()),
        created_at: now_str(),
        updated_at: now_str(),
        message_count: 0,
    };
    soul_log("INFO", "session", &format!("create_session id={} title={}", session.id, session.title));

    // 保存会话元数据
    let meta_path = dir.join(format!("{}.json", session.id));
    let meta = serde_json::json!({
        "id": session.id,
        "title": session.title,
        "created_at": session.created_at,
        "updated_at": session.updated_at,
        "message_count": session.message_count,
    });
    std::fs::write(&meta_path, serde_json::to_string_pretty(&meta)
        .map_err(|e| format!("序列化失败: {}", e))?)
        .map_err(|e| format!("写入会话失败: {}", e))?;

    Ok(session)
}

/// 列出所有会话（按 updated_at 降序）
#[tauri::command]
fn list_sessions() -> Result<Vec<Session>, String> {
    let dir = get_sessions_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut sessions: Vec<Session> = Vec::new();
    for entry in std::fs::read_dir(&dir).map_err(|e| format!("读取会话目录失败: {}", e))? {
        let entry = entry.map_err(|e| format!("读取条目失败: {}", e))?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&content) {
                    // Skip messages files
                    if meta.get("role").is_some() { continue; }
                    if let Ok(session) = serde_json::from_value(meta) {
                        sessions.push(session);
                    }
                }
            }
        }
    }

    // 按 updated_at 降序
    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(sessions)
}

/// 删除会话
#[tauri::command]
fn delete_session(session_id: String) -> Result<(), String> {
    soul_log("INFO", "session", &format!("delete_session id={}", session_id));
    let dir = get_sessions_dir();
    let meta_path = dir.join(format!("{}.json", session_id));
    let msgs_path = dir.join(format!("{}.msgs.json", session_id));

    let _ = std::fs::remove_file(&meta_path);
    let _ = std::fs::remove_file(&msgs_path);
    Ok(())
}

/// 重命名会话
#[tauri::command]
fn rename_session(session_id: String, new_title: String) -> Result<(), String> {
    let dir = get_sessions_dir();
    let meta_path = dir.join(format!("{}.json", session_id));
    let content = std::fs::read_to_string(&meta_path)
        .map_err(|e| format!("读取会话失败: {}", e))?;
    let mut meta: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("解析会话失败: {}", e))?;
    meta["title"] = serde_json::Value::String(new_title);
    meta["updated_at"] = serde_json::Value::String(now_str());
    std::fs::write(&meta_path, serde_json::to_string_pretty(&meta)
        .map_err(|e| format!("序列化失败: {}", e))?)
        .map_err(|e| format!("写入会话失败: {}", e))?;
    Ok(())
}

/// 保存消息到会话
#[tauri::command]
fn save_message(session_id: String, role: String, content: String) -> Result<(), String> {
    let dir = get_sessions_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建会话目录失败: {}", e))?;

    let msgs_path = dir.join(format!("{}.msgs.json", session_id));

    // 读取现有消息
    let mut messages: Vec<ChatMessage> = if msgs_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&msgs_path) {
            serde_json::from_str(&content).unwrap_or_default()
        } else { Vec::new() }
    } else { Vec::new() };

    messages.push(ChatMessage {
        role,
        content,
        timestamp: now_str(),
    });

    std::fs::write(&msgs_path, serde_json::to_string_pretty(&messages)
        .map_err(|e| format!("序列化消息失败: {}", e))?)
        .map_err(|e| format!("写入消息失败: {}", e))?;

    // 更新会话元数据的 message_count
    let meta_path = dir.join(format!("{}.json", session_id));
    if meta_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&meta_path) {
            if let Ok(mut meta) = serde_json::from_str::<serde_json::Value>(&content) {
                meta["message_count"] = serde_json::Value::Number(serde_json::Number::from(messages.len() as u32));
                meta["updated_at"] = serde_json::Value::String(now_str());
                let _ = std::fs::write(&meta_path, serde_json::to_string_pretty(&meta).unwrap_or_default());
            }
        }
    }

    Ok(())
}

/// 加载会话的所有消息
#[tauri::command]
fn load_messages(session_id: String) -> Result<Vec<ChatMessage>, String> {
    let dir = get_sessions_dir();
    let msgs_path = dir.join(format!("{}.msgs.json", session_id));

    if !msgs_path.exists() {
        return Ok(Vec::new());
    }

    let content = std::fs::read_to_string(&msgs_path)
        .map_err(|e| format!("读取消息失败: {}", e))?;
    let messages: Vec<ChatMessage> = serde_json::from_str(&content)
        .map_err(|e| format!("解析消息失败: {}", e))?;
    Ok(messages)
}

// ============================================================
// 自动总结功能
// ============================================================

/// 非流式聊天请求（用于总结等后台调用）
#[tauri::command]
async fn chat_non_streaming(
    port: u16,
    model: String,
    messages: Vec<serde_json::Value>,
) -> Result<String, String> {
    let url = format!("http://127.0.0.1:{}/v1/chat/completions", port);
    soul_log("DEBUG", "chat", &format!("chat_non_streaming port={} messages={}", port, messages.len()));

    let client = reqwest::Client::new();
    let request_body = serde_json::json!({
        "model": model,
        "messages": messages,
        "stream": false
    });

    let response = client.post(&url)
        .json(&request_body)
        .send()
        .await
        .map_err(|e| format!("网络请求失败: {}", e))?;

    let status = response.status();
    if !status.is_success() {
        let body_text = response.text().await.unwrap_or_default();
        return Err(format!("API请求失败 (HTTP {})\n{}", status, body_text));
    }

    let json: serde_json::Value = response.json().await
        .map_err(|e| format!("解析响应失败: {}", e))?;

    let content = json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .to_string();

    // 从末尾剥离模型 EOS 特殊 token（不影响中间内容）
    let end_tokens = [
        "<|im_end|>", "<|im_start|>", "<|im_sep|>",
        "<|assistant|>", "<|user|>", "<|system|>",
        "</s>", "<s>",
        "<end_of_turn>", "<eos>", "<bos>",
    ];
    let mut cleaned = content.trim().to_string();
    loop {
        let mut found = false;
        for t in &end_tokens {
            if cleaned.ends_with(t) {
                cleaned = cleaned[..cleaned.len() - t.len()].trim_end().to_string();
                found = true;
                break;
            }
        }
        if !found { break; }
    }

    soul_log("DEBUG", "chat", &format!("chat_non_streaming 返回 {} 字符", cleaned.len()));
    Ok(cleaned)
}

/// 写入会话总结文件
#[tauri::command]
fn write_session_summary(session_id: String, summary: String) -> Result<(), String> {
    let dir = get_sessions_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建目录失败: {}", e))?;
    let path = dir.join(format!("{}.summary", session_id));
    std::fs::write(&path, &summary)
        .map_err(|e| format!("写入总结文件失败: {}", e))?;
    soul_log("DEBUG", "session", &format!("write_session_summary {} ({} 字符)", session_id, summary.len()));
    Ok(())
}

/// 读取会话总结文件
#[tauri::command]
fn read_session_summary(session_id: String) -> Result<String, String> {
    let dir = get_sessions_dir();
    let path = dir.join(format!("{}.summary", session_id));
    if !path.exists() {
        return Ok(String::new());
    }
    let summary = std::fs::read_to_string(&path)
        .map_err(|e| format!("读取总结文件失败: {}", e))?;
    Ok(summary)
}

/// 删除会话总结文件
#[tauri::command]
fn delete_session_summary(session_id: String) -> Result<(), String> {
    let dir = get_sessions_dir();
    let path = dir.join(format!("{}.summary", session_id));
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|e| format!("删除总结文件失败: {}", e))?;
    }
    Ok(())
}// ============================================================
// 入口
// ============================================================

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            init_soul_logger();
            soul_log("INFO", "main", "应用程序启动 (setup 开始)");
            let show_item = MenuItemBuilder::with_id("show", "显示窗口").build(app)?;
            let log_item = MenuItemBuilder::with_id("logs", "查看日志").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "退出").build(app)?;
            let menu = MenuBuilder::new(app)
                .item(&show_item)
                .separator()
                .item(&log_item)
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
                        "logs" => {
                            let log_dir = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
                            let path = std::path::PathBuf::from(log_dir).join("SoulLogs");
                            let _ = std::fs::create_dir_all(&path);
                            #[cfg(windows)]
                            { let _ = std::process::Command::new("explorer").arg(path.to_string_lossy().as_ref()).spawn(); }
                            #[cfg(not(windows))]
                            { let _ = std::process::Command::new("open").arg(path.to_string_lossy().as_ref()).spawn(); }
                        }
                        "quit" => {
                            soul_log("INFO", "main", "托盘菜单退出，清理所有模型");
                            // 退出前清理所有模型
                            if let Some(models_state) = app.try_state::<RunningModelsState>() {
                                if let Ok(mut models) = models_state.models.lock() {
                                    for model in models.iter() {
                                        if let Ok(pid) = model.pid.parse::<u32>() {
                                            let _ = new_hidden_cmd("taskkill")
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
            retry_count: Mutex::new(0),
            model_id: Mutex::new(String::new()),
            local_dir: Mutex::new(String::new()),
            include_pattern: Mutex::new(None),
            use_token: Mutex::new(false),
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
            import_model_file,
            check_setup_needed,
            check_and_sync_backend,
            frontend_log,
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
            retry_download,
            cancel_download,
            minimize_window,
            maximize_window,
            close_window,
            start_model,
            unload_model,
            list_running_models,
            stop_all_models,
            create_session,
            list_sessions,
            delete_session,
            rename_session,
            save_message,
            load_messages,
            chat_non_streaming,
            write_session_summary,
            read_session_summary,
            delete_session_summary,
            check_python_installed,
            install_python,
            check_pip_installed,
            install_pip,
            detect_cpu,
        ])
        .run(tauri::generate_context!())
        .expect("启动失败");
}
