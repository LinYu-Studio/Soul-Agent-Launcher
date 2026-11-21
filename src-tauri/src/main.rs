// Soul Agent Launcher - 后端
// Release 构建隐藏控制台窗口（dev 模式仍显示，方便看日志）
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{Emitter, Manager, State};

mod knowledge;
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
    // 强制 Python 子进程用 UTF-8 写 stdio（中文 Windows 默认 GBK，
    // tqdm/日志的 Unicode 字符会在此处乱码且前端无法挽回）。
    // 非 Python 程序会忽略这两个环境变量，无副作用。
    cmd.env("PYTHONIOENCODING", "utf-8");
    cmd.env("PYTHONUTF8", "1");
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
        // 日志文件名按 UTC+8 本地日期滚动（凌晨 0 点切文件，而非 UTC 早上 8 点）
        let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let (y, m, d) = local_date_parts(secs);
        format!("{:04}-{:02}-{:02}", y, m, d)
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
        let (h, m, s) = local_hms(secs);
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
        let (h, m, s) = local_hms(secs);
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
            let (h, m, s) = local_hms(secs);
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

/// 单服务器模式的"计划内停止"标记：stop/wake/sleep 都会杀进程再重启，
/// 监控线程看到该标记时跳过（避免把计划内操作误判为崩溃）
static SERVER_EXPECTED_STOP: AtomicBool = AtomicBool::new(false);
/// 单服务器模式连续崩溃计数
static SERVER_CRASH_COUNT: AtomicU32 = AtomicU32::new(0);
/// 模型/服务自动重启的最大尝试次数，超过则放弃并通知前端
const MODEL_MAX_RESTARTS: u32 = 3;

/// 全局 AppHandle（setup 时写入）：代理线程/监控线程借此访问 Tauri 状态与发事件
static APP_HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();

/// 多模型待机注册表：待机中的模型（llama 进程已停止 → 显存/内存零占用，
/// 仅保留代理端口与路由表；命中请求时自动唤醒）
#[derive(Clone)]
struct StandbyInfo {
    model_path: String,
    llama_port: u16,
    ctx: u32,
    mmproj: Option<String>,
    /// 唤醒去重锁：并发请求命中同一待机模型时只有一个真正拉起进程
    loading: Arc<AtomicBool>,
}
static STANDBY_REGISTRY: OnceLock<Mutex<HashMap<String, StandbyInfo>>> = OnceLock::new();
fn get_standby_registry() -> &'static Mutex<HashMap<String, StandbyInfo>> {
    STANDBY_REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 空闲自动待机阈值（分钟，0=关闭；由前端设置页写入，监控线程扫描执行）
static STANDBY_IDLE_MIN: AtomicU32 = AtomicU32::new(0);

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
    /// 意外崩溃次数（自动重启计数）
    #[serde(default)]
    crash_count: u32,
    /// 多模态投影模型路径（多模态模型启动时随 --mmproj 传入；待机唤醒时原样恢复）
    #[serde(default)]
    mmproj: Option<String>,
    /// 最近一次收到请求的 Unix 秒（代理更新；空闲自动待机的依据）
    #[serde(default)]
    last_active: u64,
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

/// Unix 天数 → 公历 (年, 月, 日)：逐轮回退法，正确处理闰年与各月天数
fn days_to_ymd(days: u64) -> (u64, u32, u32) {
    let mut y = 1970u64;
    let mut d = days;
    loop {
        let year_days = if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 { 366 } else { 365 };
        if d < year_days { break; }
        d -= year_days;
        y += 1;
    }
    let months_days = [31u64, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let mut m = 0usize;
    while m < 12 {
        let md = if m == 1 && leap { 29 } else { months_days[m] };
        if d < md { break; }
        d -= md;
        m += 1;
    }
    (y, (m + 1) as u32, (d + 1) as u32)
}

/// Unix 秒 → UTC+8 本地日期 (年, 月, 日)
/// SAL 面向国内用户，统一固定 UTC+8（不引入 chrono，保持零依赖）
fn local_date_parts(secs: u64) -> (u64, u32, u32) {
    days_to_ymd((secs + 8 * 3600) / 86400)
}

/// Unix 秒 → UTC+8 本地 (时, 分, 秒)
fn local_hms(secs: u64) -> (u32, u32, u32) {
    let rem = (secs + 8 * 3600) % 86400;
    ((rem / 3600) as u32, ((rem % 3600) / 60) as u32, (rem % 60) as u32)
}

fn now_iso() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let (y, mo, d) = local_date_parts(secs);
    let (h, m, s) = local_hms(secs);
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", y, mo, d, h, m, s)
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

/// 启动 llama-server 加载指定模型（全量 GPU 卸载，多卡自动选 --main-gpu）
fn spawn_model_process(llama_path: &str, model_path: &str, llama_port: u16, ctx: u32, mmproj: Option<&str>) -> Result<std::process::Child, String> {
    let gpu_info = detect_gpu_internal();
    let mut cmd = new_hidden_cmd(llama_path);
    cmd.args(["-m", model_path, "--host", "127.0.0.1",
              "--port", &llama_port.to_string(), "-c", &ctx.to_string(),
              "-ngl", "99"]);
    if gpu_info.backend == "cuda" && gpu_info.gpu_count > 1 {
        soul_log("INFO", "model", &format!("  多 GPU 检测: {} 个 -> --main-gpu {}",
            gpu_info.gpu_count, gpu_info.best_gpu_index));
        cmd.args(["--main-gpu", &gpu_info.best_gpu_index.to_string()]);
    }
    // 多模态：mmproj 投影模型随主模型一起加载（待机唤醒时原样恢复）
    if let Some(mp) = mmproj {
        if !mp.trim().is_empty() {
            cmd.args(["--mmproj", mp.trim()]);
        }
    }
    cmd.stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("启动模型进程失败: {}", e))
}

/// 启动一个模型实例
#[tauri::command]
fn start_model(
    state: State<RunningModelsState>,
    llama_path: String,
    model_path: String,
    model_name: String,
    port: u16,
    ctx: u32,
    mmproj: Option<String>,
) -> Result<String, String> {
    let llama_port = port + 1;
    soul_log("INFO", "model", &format!("start_model name='{}' port={} llama_port={} ctx={} mmproj={:?}", model_name, port, llama_port, ctx, mmproj));
    soul_log("DEBUG", "model", &format!("  llama_path={} model_path={}", llama_path, model_path));
    let mmproj_clean = mmproj.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());

    // 并发防护：同一模型文件禁止重复加载（避免显存双倍浪费/端口混乱）
    if let Ok(models) = state.models.lock() {
        if let Some(existing) = models.iter().find(|m| m.model_path == model_path) {
            let msg = format!("模型已在运行中: {} (端口 {})，请先卸载后再加载", existing.name, existing.proxy_port);
            soul_log("WARN", "model", &msg);
            return Err(msg);
        }
    }

    // 完整性快速复核：与下载记录比体积（不一致仅告警不拦截，用户可能手动替换了文件）
    if let Some(warn) = quick_integrity_check(&model_path) {
        soul_log_detail("WARN", "integrity", "模型体积与下载记录不符", &warn);
        if let Some(handle) = APP_HANDLE.get() {
            let _ = handle.emit("model-integrity-warning", serde_json::json!({
                "path": model_path,
                "message": warn,
            }));
        }
    }

    let child = spawn_model_process(&llama_path, &model_path, llama_port, ctx, mmproj_clean.as_deref())
        .map_err(|e| {
            soul_log_detail("ERROR", "model", "start_model 启动失败", &e);
            e
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
        crash_count: 0,
        mmproj: mmproj_clean,
        last_active: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
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
    // 若在待机注册表中（卸载待机模型）同步移除
    if let Ok(mut reg) = get_standby_registry().lock() {
        reg.remove(&model.name);
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
/// 注意：崩溃模型的清理由 start_crash_monitor 负责（自动重启或放弃后移除），
/// 这里不再主动 retain 清除，避免与监控线程竞态把重启中的模型丢掉
#[tauri::command]
fn list_running_models(state: State<RunningModelsState>) -> Result<Vec<RunningModel>, String> {
    // 先查询 nvidia-smi 获取所有计算进程的显存占用
    let vram_map = get_gpu_process_vram();

    let mut models = state.models.lock().map_err(|e| e.to_string())?;
    for model in models.iter_mut() {
        if model.status == "restarting" || model.status == "standby" { continue; }
        if let Ok(pid) = model.pid.parse::<u32>() {
            model.vram_mb = vram_map.get(&pid).copied().unwrap_or(0);
        }

        // 检查进程是否还活着
        let alive = is_pid_alive(&model.pid);
        model.status = if alive { "running".to_string() } else { "stopped".to_string() };
    }

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
    // 先摘除列表与路由表（避免崩溃监控线程把计划内停止误判为意外退出而自动重启），再杀进程
    let pids: Vec<String> = models.iter().map(|m| m.pid.clone()).collect();
    models.clear();
    drop(models);
    if let Ok(mut router) = get_model_router().lock() {
        router.clear();
    }
    // 清空待机注册表
    if let Ok(mut reg) = get_standby_registry().lock() {
        reg.clear();
    }
    for pid in pids {
        let _ = new_hidden_cmd("taskkill")
            .args(["/F", "/PID", &pid])
            .output();
    }
    let msg = format!("已停止 {} 个模型", count);
    soul_log("INFO", "model", &msg);
    Ok(msg)
}

/// 检查 PID 对应进程是否存活（tasklist 探测）
fn is_pid_alive(pid_str: &str) -> bool {
    new_hidden_cmd("tasklist")
        .args(["/FI", &format!("PID eq {}", pid_str), "/NH"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains(pid_str))
        .unwrap_or(false)
}

/// 从 config.json 读取 llama_path（崩溃自动重启需要）
fn read_llama_path_from_config() -> Option<String> {
    let path = get_launcher_dir().join("config.json");
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<serde_json::Value>(&text).ok()?
        .get("llama_path")?
        .as_str()
        .map(|s| s.to_string())
}

// ============================================================
// 崩溃自动恢复监控（每 5 秒巡检 llama 进程）
// ============================================================

fn start_crash_monitor(app_handle: tauri::AppHandle) {
    std::thread::spawn(move || {
        soul_log("INFO", "monitor", "崩溃自动恢复监控线程已启动");
        loop {
            std::thread::sleep(std::time::Duration::from_secs(5));
            let _ = monitor_running_models(&app_handle);
            monitor_single_server(&app_handle);
            auto_standby_sweep(&app_handle);
        }
    });
}

/// 多模型模式巡检：意外退出 → 自动重启（累计超限则放弃并移除）
fn monitor_running_models(app_handle: &tauri::AppHandle) -> Result<(), String> {
    let state = match app_handle.try_state::<RunningModelsState>() {
        Some(s) => s,
        None => return Ok(()),
    };

    // 1) 找出意外退出的模型（标记 restarting，防止与 UI 轮询竞态）
    let dead: Vec<RunningModel> = {
        let mut models = match state.models.lock() { Ok(m) => m, Err(e) => return Err(e.to_string()) };
        let mut dead = Vec::new();
        for m in models.iter_mut() {
            if m.status == "restarting" || m.status == "standby" { continue; }
            if !is_pid_alive(&m.pid) {
                m.crash_count += 1;
                m.status = "restarting".to_string();
                let mut d = m.clone();
                d.status = "restarting".to_string();
                dead.push(d);
            }
        }
        dead
    };
    if dead.is_empty() { return Ok(()); }

    for d in dead {
        // 2) 放弃条件：崩溃次数超限 / 模型文件已不存在
        if d.crash_count > MODEL_MAX_RESTARTS || !std::path::Path::new(&d.model_path).exists() {
            if let Ok(mut models) = state.models.lock() {
                models.retain(|m| m.name != d.name);
            }
            if let Ok(mut router) = get_model_router().lock() {
                router.remove(&d.name);
            }
            soul_log("ERROR", "monitor", &format!("模型 '{}' 连续崩溃 {} 次，停止自动重启并移除", d.name, d.crash_count));
            let _ = app_handle.emit("model-crash-giveup", serde_json::json!({
                "name": d.name, "proxy_port": d.proxy_port, "crash_count": d.crash_count,
            }));
            continue;
        }

        // 3) 按原参数重启（llama_path 从 config.json 读取）
        let llama_path = read_llama_path_from_config();
        let respawn = llama_path
            .map(|lp| spawn_model_process(&lp, &d.model_path, d.llama_port, d.ctx, d.mmproj.as_deref()));
        match respawn {
            Some(Ok(mut child)) => {
                let new_pid = child.id().to_string();
                soul_log("INFO", "monitor", &format!("模型 '{}' 意外退出，已自动重启 pid={} (第 {}/{} 次)",
                    d.name, new_pid, d.crash_count, MODEL_MAX_RESTARTS));
                let _ = app_handle.emit("model-restarted", serde_json::json!({
                    "name": d.name, "proxy_port": d.proxy_port, "pid": new_pid, "crash_count": d.crash_count,
                }));
                // 回写新 PID；若期间用户已手动卸载，则立即回收刚拉起的孤儿进程
                let mut models = match state.models.lock() {
                    Ok(m) => m,
                    Err(_) => { let _ = child.kill(); continue; }
                };
                match models.iter_mut().find(|m| m.name == d.name && m.pid == d.pid) {
                    Some(m) => {
                        m.pid = new_pid;
                        m.started_at = format_ts(SystemTime::now().duration_since(UNIX_EPOCH)
                            .unwrap_or_default().as_secs());
                        m.status = "running".to_string();
                        m.crash_count = d.crash_count;
                    }
                    None => {
                        drop(models);
                        soul_log("WARN", "monitor", &format!("模型 '{}' 重启期间被手动卸载，回收孤儿进程 pid={}", d.name, new_pid));
                        let _ = child.kill();
                    }
                }
            }
            Some(Err(e)) => {
                soul_log_detail("ERROR", "monitor", &format!("模型 '{}' 自动重启失败", d.name), &e);
                // 回退为 stopped，下一轮继续尝试（crash_count 已累加，超限自动放弃）
                if let Ok(mut models) = state.models.lock() {
                    if let Some(m) = models.iter_mut().find(|m| m.name == d.name && m.pid == d.pid) {
                        m.status = "stopped".to_string();
                    }
                }
            }
            None => {
                soul_log("ERROR", "monitor", &format!("模型 '{}' 崩溃后未找到 llama_path (config.json)，跳过重启", d.name));
            }
        }
    }
    Ok(())
}

/// 单服务器模式巡检：进程意外退出 → 按 spawn_params 自动恢复（待机崩溃则恢复为待机）
fn monitor_single_server(app_handle: &tauri::AppHandle) {
    if SERVER_EXPECTED_STOP.load(Ordering::SeqCst) { return; }
    let state = match app_handle.try_state::<ServerState>() {
        Some(s) => s,
        None => return,
    };

    // 是否已退出（持有锁内判断）
    let exited = {
        let mut guard = match state.server.lock() { Ok(g) => g, Err(_) => return };
        match guard.as_mut() {
            Some(child) => matches!(child.try_wait(), Ok(Some(_))),
            None => false,
        }
    };
    if !exited { return; }

    let params = match state.spawn_params.lock() { Ok(p) => p.clone(), Err(_) => None };
    let Some(params) = params else { return };
    let standby = state.standby.lock().map(|s| *s).unwrap_or(false);

    let count = SERVER_CRASH_COUNT.fetch_add(1, Ordering::SeqCst) + 1;
    if count > MODEL_MAX_RESTARTS {
        soul_log("ERROR", "monitor", &format!("llama-server 连续崩溃 {} 次，停止自动恢复", count));
        let _ = app_handle.emit("server-crash-giveup", serde_json::json!({ "crash_count": count }));
        return;
    }

    let llama_port = if params.port >= 65535 { 20001u16 } else { params.port + 1 };
    // 待机模式崩溃 → 恢复为待机（不带 -m）；运行模式崩溃 → 重新加载模型
    let mut cmd = new_hidden_cmd(&params.llama_path);
    if !standby {
        cmd.arg("-m").arg(&params.model_path);
    }
    cmd.args(["--host", "127.0.0.1", "--port", &llama_port.to_string(), "-c", &params.ctx.to_string()]);
    match cmd.stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn() {
        Ok(child) => {
            let pid = child.id();
            if let Ok(mut g) = state.server.lock() { *g = Some(child); }
            soul_log("INFO", "monitor", &format!("llama-server 意外退出，已自动恢复 pid={} mode={} (第 {}/{} 次)",
                pid, if standby { "待机" } else { "运行" }, count, MODEL_MAX_RESTARTS));
            let _ = app_handle.emit("server-restarted", serde_json::json!({
                "pid": pid.to_string(), "port": params.port, "crash_count": count, "standby": standby,
            }));
        }
        Err(e) => {
            soul_log_detail("ERROR", "monitor", "llama-server 自动恢复失败", &e.to_string());
            let _ = app_handle.emit("server-crash-giveup", serde_json::json!({ "crash_count": count, "error": e.to_string() }));
        }
    }
}

// ============================================================
// 多模型待机引擎：平时零进程零显存，请求命中自动唤醒（/chat 与 OpenAI 双端点统一）
// ============================================================

/// 进入待机（内部实现）：停 llama 进程 → 登记注册表 → 列表标记 standby
fn sleep_model_internal(model_name: &str) -> Result<String, String> {
    let handle = APP_HANDLE.get().ok_or("应用未初始化")?;
    let st = handle.try_state::<RunningModelsState>().ok_or("状态不可用")?;
    let (model_path, llama_port, ctx, mmproj, pid) = {
        let mut models = st.models.lock().map_err(|e| e.to_string())?;
        let m = models.iter_mut().find(|m| m.name == model_name)
            .ok_or_else(|| format!("未找到运行中的模型: {}", model_name))?;
        if m.status == "standby" {
            return Err(format!("模型 {} 已处于待机", model_name));
        }
        if m.status == "restarting" {
            // 崩溃自动恢复进行中：此时待机会与恢复流程竞态（恢复完成会覆盖待机状态）
            return Err("模型正在自动恢复中，请稍后再待机".to_string());
        }
        (m.model_path.clone(), m.llama_port, m.ctx, m.mmproj.clone(), m.pid.clone())
    };

    // 有活跃推理会话时拒绝待机（不打断正在生成的请求）
    if get_active_session_count(llama_port) > 0 {
        return Err("有活跃会话，暂不能待机".to_string());
    }

    // 停止 llama 进程（显存/内存立即释放）
    if let Ok(p) = pid.parse::<u32>() {
        let _ = new_hidden_cmd("taskkill").args(["/F", "/PID", &p.to_string()]).output();
    }

    // 登记待机信息（唤醒所需全部参数）
    if let Ok(mut reg) = get_standby_registry().lock() {
        reg.insert(model_name.to_string(), StandbyInfo {
            model_path, llama_port, ctx, mmproj,
            loading: Arc::new(AtomicBool::new(false)),
        });
    }

    // 更新运行列表
    if let Some(st) = handle.try_state::<RunningModelsState>() {
        if let Ok(mut models) = st.models.lock() {
            if let Some(m) = models.iter_mut().find(|m| m.name == model_name) {
                m.status = "standby".to_string();
                m.pid = String::new();
                m.vram_mb = 0;
            }
        }
    }
    let msg = format!("模型 {} 已进入待机（显存已释放，请求将自动唤醒）", model_name);
    soul_log("INFO", "model", &msg);
    Ok(msg)
}

/// 从待机唤醒（代理自动唤醒与手动唤醒共用）。
/// 返回 Ok(true)=本次触发唤醒；Ok(false)=不在待机/已有唤醒进行中（调用方等待即可）。
fn wake_standby_model(model_name: &str, app: Option<&tauri::AppHandle>) -> Result<bool, String> {
    fn reset_loading(model_name: &str) {
        if let Ok(reg) = get_standby_registry().lock() {
            if let Some(i) = reg.get(model_name) { i.loading.store(false, Ordering::SeqCst); }
        }
    }

    // 取待机信息并抢占 loading 锁（并发请求只有一个真正拉起进程）
    let info = {
        let reg = get_standby_registry().lock().map_err(|e| e.to_string())?;
        let info = match reg.get(model_name) { Some(i) => i.clone(), None => return Ok(false) };
        if info.loading.swap(true, Ordering::SeqCst) { return Ok(false); }
        info
    };

    let llama_path = match read_llama_path_from_config() {
        Some(p) => p,
        None => { reset_loading(model_name); return Err("config.json 中未找到 llama_path，无法唤醒".to_string()); }
    };
    soul_log("INFO", "model", &format!("唤醒待机模型 '{}' (mmproj={:?})", model_name, info.mmproj));
    let child = match spawn_model_process(&llama_path, &info.model_path, info.llama_port, info.ctx, info.mmproj.as_deref()) {
        Ok(c) => c,
        Err(e) => { reset_loading(model_name); return Err(e); }
    };
    let new_pid = child.id();

    // 更新运行列表：恢复 PID 与状态（代理与后续请求立即正常路由）
    let handle = APP_HANDLE.get().or(app);
    let mut entry_found = false;
    if let Some(handle) = handle {
        if let Some(st) = handle.try_state::<RunningModelsState>() {
            if let Ok(mut models) = st.models.lock() {
                if let Some(m) = models.iter_mut().find(|m| m.name == model_name) {
                    m.pid = new_pid.to_string();
                    m.status = "running".to_string();
                    m.last_active = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
                    entry_found = true;
                }
            }
        }
        soul_log("INFO", "model", &format!("模型 '{}' 已从待机唤醒 pid={}", model_name, new_pid));
        let _ = handle.emit("model-wake", serde_json::json!({ "name": model_name, "pid": new_pid.to_string() }));
    }
    // 运行期间条目被手动卸载 → 回收刚拉起的孤儿进程，避免显存泄漏
    if !entry_found {
        drop(child);
        reset_loading(model_name);
        if let Ok(mut reg) = get_standby_registry().lock() { reg.remove(model_name); }
        soul_log("WARN", "model", &format!("模型 '{}' 唤醒期间被手动卸载，回收孤儿进程 pid={}", model_name, new_pid));
        return Err(format!("模型 {} 已被卸载，唤醒中止", model_name));
    }

    // 唤醒成功 → 移出注册表
    if let Ok(mut reg) = get_standby_registry().lock() {
        reg.remove(model_name);
    }
    Ok(true)
}

/// 空闲自动待机扫描（监控线程每 5 秒调用一次）
fn auto_standby_sweep(app_handle: &tauri::AppHandle) {
    let idle_min = STANDBY_IDLE_MIN.load(Ordering::SeqCst) as u64;
    if idle_min == 0 { return; }
    let state = match app_handle.try_state::<RunningModelsState>() { Some(s) => s, None => return };
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let targets: Vec<String> = {
        let models = match state.models.lock() { Ok(m) => m, Err(_) => return };
        models.iter()
            .filter(|m| m.status == "running" && m.last_active > 0
                && now.saturating_sub(m.last_active) >= idle_min * 60)
            .map(|m| m.name.clone())
            .collect()
    };
    for name in targets {
        match sleep_model_internal(&name) {
            Ok(_) => {
                soul_log("INFO", "monitor", &format!("空闲超过 {} 分钟，模型 '{}' 自动进入待机", idle_min, name));
                let _ = app_handle.emit("model-standby", serde_json::json!({ "name": name, "auto": true }));
            }
            Err(e) => {
                // 活跃会话等情况：刷新活跃时间，避免每 5 秒重复尝试
                if let Ok(mut models) = state.models.lock() {
                    if let Some(m) = models.iter_mut().find(|m| m.name == name) {
                        m.last_active = now;
                    }
                }
                soul_log("DEBUG", "monitor", &format!("自动待机 {} 暂缓: {}", name, e));
            }
        }
    }
}

/// 手动让指定模型进入待机
#[tauri::command]
fn sleep_model(model_name: String) -> Result<String, String> {
    let r = sleep_model_internal(&model_name);
    if r.is_ok() {
        if let Some(h) = APP_HANDLE.get() {
            let _ = h.emit("model-standby", serde_json::json!({ "name": model_name, "auto": false }));
        }
    }
    r
}

/// 手动唤醒待机模型
#[tauri::command]
fn wake_model(model_name: String) -> Result<String, String> {
    let start = std::time::Instant::now();
    match wake_standby_model(&model_name, APP_HANDLE.get())? {
        true => Ok(format!("模型 {} 唤醒中（已拉起进程，耗时 {}ms）", model_name, start.elapsed().as_millis())),
        false => Ok(format!("模型 {} 无需唤醒", model_name)),
    }
}

/// 设置空闲自动待机阈值（分钟，0=关闭）
#[tauri::command]
fn set_standby_idle_min(minutes: u32) {
    STANDBY_IDLE_MIN.store(minutes.min(240), Ordering::SeqCst);
    soul_log("INFO", "settings", &format!("空闲自动待机阈值 = {} 分钟", minutes.min(240)));
}

/// 获取模型直连 llama 端口（绕过代理，用于工具调用等场景）
#[tauri::command]
fn get_model_llama_port(model_name: String, state: State<RunningModelsState>) -> Result<u16, String> {
    let models = state.models.lock().map_err(|e| e.to_string())?;
    for m in models.iter() {
        if m.name == model_name {
            soul_log("DEBUG", "model", &format!("get_model_llama_port: {} → {}", model_name, m.llama_port));
            return Ok(m.llama_port);
        }
    }
    Err(format!("未找到运行中的模型: {}", model_name))
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
    // 手动启动视为全新开始：重置崩溃计数
    SERVER_CRASH_COUNT.store(0, Ordering::SeqCst);
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
    // 计划内停止：让崩溃监控线程跳过这段时间窗
    SERVER_EXPECTED_STOP.store(true, Ordering::SeqCst);
    SERVER_CRASH_COUNT.store(0, Ordering::SeqCst);
    let mut server_guard = state.server.lock().map_err(|e| e.to_string())?;
    if let Some(mut child) = server_guard.take() {
        soul_log("INFO", "server", &format!("stop_server killing pid={}", child.id()));
        child.kill().map_err(|e| format!("停止服务器失败: {}", e))?;
        child.wait().ok();
        // 清除待机标记
        if let Ok(mut s) = state.standby.lock() { *s = false; }
        soul_log("INFO", "server", "stop_server 完成");
        SERVER_EXPECTED_STOP.store(false, Ordering::SeqCst);
        Ok("服务器已停止".to_string())
    } else {
        soul_log("WARN", "server", "stop_server 服务器未在运行");
        SERVER_EXPECTED_STOP.store(false, Ordering::SeqCst);
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
    // 计划内杀进程重启：让崩溃监控线程跳过这段时间窗
    SERVER_EXPECTED_STOP.store(true, Ordering::SeqCst);
    SERVER_CRASH_COUNT.store(0, Ordering::SeqCst);
    kill_server(&state);
    if let Ok(mut s) = state.standby.lock() { *s = false; }

    let llama_port = if params.port >= 65535 { 20001u16 } else { params.port + 1 };
    soul_log("INFO", "server", &format!("wake_server 重新启动 llama-server: {}:{}", params.llama_path, llama_port));
    let spawn_result = new_hidden_cmd(&params.llama_path)
        .args(["-m", &params.model_path, "--host", "127.0.0.1",
               "--port", &llama_port.to_string(), "-c", &params.ctx.to_string()])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn();
    SERVER_EXPECTED_STOP.store(false, Ordering::SeqCst);
    let child = spawn_result.map_err(|e| format!("唤醒失败: {}", e))?;

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
    // 计划内杀进程重启：让崩溃监控线程跳过这段时间窗
    SERVER_EXPECTED_STOP.store(true, Ordering::SeqCst);
    SERVER_CRASH_COUNT.store(0, Ordering::SeqCst);
    kill_server(&state);
    if let Ok(mut s) = state.standby.lock() { *s = true; }

    soul_log("INFO", "server", &format!("sleep_server 启动待机模式: {}:{}", params.llama_path, llama_port));

    let spawn_result = new_hidden_cmd(&params.llama_path)
        .args(["--host", "127.0.0.1",
               "--port", &llama_port.to_string(), "-c", &params.ctx.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    SERVER_EXPECTED_STOP.store(false, Ordering::SeqCst);
    let child = spawn_result.map_err(|e| format!("休眠失败: {}", e))?;

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
    // 必须带超时：llama 进程挂死时，无超时会冻住调用线程
    // （监控线程被冻 = 崩溃恢复/自动待机全部失效）
    let client = match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
    {
        Ok(c) => c,
        Err(_) => return 0,
    };
    match client.get(&url).send() {
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
async fn check_and_sync_backend(app_handle: tauri::AppHandle, force: Option<bool>) -> Result<String, String> {
    let force = force.unwrap_or(false);
    soul_log("INFO", "setup", &format!("check_and_sync_backend 开始 (force={})", force));
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
            // 同一后端 → 检查服务器是否存在（force=true 时无视一致性强制重装，供环境修复面板使用）
            if !install_dir.join("llama-server.exe").exists() {
                soul_log("INFO", "setup", &format!("llama-server.exe 不存在，需重新解压 (key={})", key));
                true
            } else if force {
                soul_log("INFO", "setup", &format!("强制重装后端包: {}", key));
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
#[derive(Serialize, Clone, Debug, Default)]
struct GpuInfo {
    backend: String,
    gpu_name: String,
    cuda_version: Option<String>,
    package_key: String,
    #[serde(default)]
    gpu_count: u32,
    #[serde(default)]
    best_gpu_index: u32,
    #[serde(default)]
    vram_total_mb: u64,
    #[serde(default)]
    all_gpu_names: Vec<String>,
    #[serde(default)]
    all_vram_mb: Vec<u64>,
}

/// 检测 GPU 和 CUDA 版本
#[tauri::command]
fn detect_gpu() -> GpuInfo {
    detect_gpu_internal()
}

fn detect_gpu_internal() -> GpuInfo {
    // ── 多 GPU 检测：一次 nvidia-smi 读出所有 GPU ──
    let mut cmd = new_hidden_cmd("nvidia-smi");
    cmd.args(["--query-gpu=index,name,memory.total", "--format=csv,noheader,nounits"]);
    match cmd.output() {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout);
            let mut all_names: Vec<String> = Vec::new();
            let mut all_vram: Vec<u64> = Vec::new();
            let mut best_idx: u32 = 0;
            let mut best_vram: u64 = 0;
            let mut driver_ver = String::new();

            for line in text.lines() {
                let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
                if parts.len() >= 3 {
                    if let Ok(idx) = parts[0].parse::<u32>() {
                        let name = parts[1].to_string();
                        let vram = parts[2].parse::<u64>().unwrap_or(0);
                        all_names.push(name);
                        all_vram.push(vram);
                        if vram > best_vram {
                            best_vram = vram;
                            best_idx = idx;
                        }
                    }
                }
                // 取第一行的 driver_version（如果含逗号前的驱动版本）
                if driver_ver.is_empty() && parts.len() >= 2 {
                    if let Some(dv) = parts[1].split('.').next() {
                        driver_ver = dv.to_string();
                    }
                }
            }

            if all_names.is_empty() {
                // nvidia-smi 成功但没解析出 GPU（极少情况）
                return GpuInfo {
                    backend: "cuda".to_string(),
                    gpu_name: "NVIDIA GPU".to_string(),
                    cuda_version: Some("12.4".to_string()),
                    package_key: "cuda-12.4".to_string(),
                    gpu_count: 0,
                    best_gpu_index: 0,
                    vram_total_mb: 0,
                    all_gpu_names: vec![],
                    all_vram_mb: vec![],
                };
            }

            // 用第一块 GPU 的驱动版本来确定 CUDA 版本
            let major = driver_ver.parse::<u32>().unwrap_or(0);
            let (cuda_ver, package_key) = if major >= 570 {
                ("13.0".to_string(), "cuda-13.3".to_string())
            } else if major >= 525 {
                ("12.5".to_string(), "cuda-12.4".to_string())
            } else {
                ("12.0".to_string(), "cuda-12.4".to_string())
            };

            let best_name = all_names.get(best_idx as usize).cloned().unwrap_or_default();
            soul_log("INFO", "gpu", &format!("detect_gpu: {} GPUs, 最强 #[{}] {} ({} MB)",
                all_names.len(), best_idx, best_name, best_vram));

            return GpuInfo {
                backend: "cuda".to_string(),
                gpu_name: best_name,
                cuda_version: Some(cuda_ver),
                package_key,
                gpu_count: all_names.len() as u32,
                best_gpu_index: best_idx,
                vram_total_mb: best_vram,
                all_gpu_names: all_names,
                all_vram_mb: all_vram,
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
            ..Default::default()
        };
    }

    let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "x64" };
    GpuInfo {
        backend: "cpu".to_string(),
        gpu_name: format!("CPU ({})", arch),
        cuda_version: None,
        package_key: format!("cpu-{}", arch),
        ..Default::default()
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

    // llama 主包文件名带构建号（llama-bNNNN-bin-win-<key>[-x64].zip）。
    // 不再锁死 b9568：按 `llama-b*-bin-win-<suffix>` 模式扫描，取构建号最大的包。
    let zip_suffix = match package_key {
        "cuda-12.4" | "cuda-13.3" => format!("{}-x64.zip", package_key),
        other => format!("{}.zip", other),
    };
    let llama_zip = find_latest_llama_zip(&downloads_base, &zip_suffix)?;

    let cudart_zip = match package_key {
        "cuda-12.4" | "cuda-13.3" => Some(
            downloads_base.join(format!("cudart-llama-bin-win-{}-x64.zip", package_key)),
        ),
        _ => None,
    };

    Some((llama_zip, cudart_zip))
}

/// 在目录中找构建号最大的 `llama-bNNNN-bin-win-<suffix>` 包
fn find_latest_llama_zip(dir: &std::path::Path, suffix: &str) -> Option<PathBuf> {
    let mut best: Option<(u32, PathBuf)> = None;
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !(name.starts_with("llama-b") && name.ends_with(suffix)) {
            continue;
        }
        let build = name["llama-b".len()..]
            .split(|c: char| !c.is_ascii_digit())
            .next()
            .and_then(|d| d.parse::<u32>().ok())
            .unwrap_or(0);
        if best.as_ref().map_or(true, |(b, _)| build > *b) {
            best = Some((build, entry.path()));
        }
    }
    best.map(|(_, p)| p)
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
            // 也检查常见路径（用户级安装路径用 LOCALAPPDATA 动态拼接，避免硬编码用户名）
            let user_python = std::env::var("LOCALAPPDATA").map(|v| {
                std::path::PathBuf::from(v)
                    .join("Programs")
                    .join("Python")
                    .join("Python311")
                    .join("python.exe")
            });
            let common_paths = [
                Some(std::path::PathBuf::from("C:\\Python311\\python.exe")),
                Some(std::path::PathBuf::from("C:\\Program Files\\Python311\\python.exe")),
                user_python.ok(),
            ];
            common_paths.into_iter().flatten().any(|p| p.exists())
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

/// 检查指定路径所在磁盘的剩余空间（字节）
#[tauri::command]
fn check_disk_space(path: String) -> Result<u64, String> {
    use std::path::Path;
    let p = Path::new(&path);
    let root = if p.is_dir() { p } else { p.parent().unwrap_or(Path::new(".")) };
    match fs2::available_space(root) {
        Ok(bytes) => {
            soul_log("INFO", "disk", &format!("check_disk_space: path={} available={}", path, bytes));
            Ok(bytes)
        }
        Err(e) => {
            soul_log("WARN", "disk", &format!("check_disk_space 失败: {}", e));
            Err(format!("无法检测磁盘空间: {}", e))
        }
    }
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

    // 确保 pip 可用，否则自动安装
    let pip_ready = new_hidden_cmd("pip").arg("--version").output().is_ok()
        || new_hidden_cmd("python").args(["-m", "pip", "--version"]).output().is_ok();
    if !pip_ready {
        soul_log("INFO", "modelscope", "pip 未安装，开始自动安装 pip");
        let _ = app_handle.emit("ms-install-progress", serde_json::json!({
            "progress": 20, "message": "正在安装 pip..."
        }));
        let pip_out = new_hidden_cmd("python")
            .args(["-m", "ensurepip", "--upgrade", "--default-pip"])
            .output()
            .map_err(|e| format!("启动 pip 安装失败: {}", e))?;
        if !pip_out.status.success() {
            let err = String::from_utf8_lossy(&pip_out.stderr).to_string();
            soul_log("ERROR", "modelscope", &format!("pip 自动安装失败: {}", err));
            return Err(format!("pip 自动安装失败，请手动安装 pip 后再试: {}", err));
        }
        soul_log("INFO", "modelscope", "pip 安装成功");
    }

    let _ = app_handle.emit("ms-install-progress", serde_json::json!({
        "progress": 40, "message": "正在安装 modelscope..."
    }));

    // 使用阿里云镜像安装 modelscope（用 python -m pip 更可靠）
    let child = new_hidden_cmd("python")
        .args(["-m", "pip", "install", "modelscope", "-i", "https://mirrors.aliyun.com/pypi/simple/"])
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

    let pid_guard = state.pid.lock().map_err(|e| e.to_string())?;

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

    let token_path = get_launcher_dir().join("tokens").join("modelscope.token");
    let has_token = token_path.exists() && std::fs::read_to_string(&token_path)
        .map(|c| !c.trim().is_empty()).unwrap_or(false);
    if let Ok(mut ut) = state.use_token.lock() { *ut = has_token; }

    // 后台线程：下载 + 完整性校验 + 真实自动重试循环（失败自动重下）
    let app_clone = app_handle.clone();
    let model_clone = clean_id.clone();
    let local_dir_c = local_dir.clone();
    let ip_c = include_pattern.clone();
    let token_path_c = token_path.clone();
    drop(pid_guard); // 释放占用检查用的锁，线程内经 app.state 更新 PID/重试计数

    std::thread::spawn(move || {
        let max_retries = 3u32;
        let total_attempts = max_retries + 1; // 首次 + 3 次自动重试
        let mut attempts_done: u32 = 0;
        let st = app_clone.state::<DownloadProcess>();

        loop {
            attempts_done += 1;
            let attempt = attempts_done;
            let is_retry = attempt > 1;

            let _ = app_clone.emit("download-progress", serde_json::json!({
                "progress": 0,
                "message": if is_retry {
                    format!("第 {}/{} 次下载尝试: {} ...", attempt, total_attempts, model_clone)
                } else {
                    format!("正在下载 {} ...", model_clone)
                },
                "status": "downloading",
            }));

            // 构建并启动本次下载进程
            let mut cmd = new_hidden_cmd("modelscope");
            cmd.args(["download", "--model", &model_clone, "--local_dir", &local_dir_c]);
            if let Ok(content) = std::fs::read_to_string(&token_path_c) {
                let token = content.trim().to_string();
                if !token.is_empty() {
                    cmd.args(["--token", &token]);
                }
            }
            if let Some(ref pattern) = ip_c {
                cmd.args(["--include", pattern]);
            }
            cmd.stdout(std::process::Stdio::piped());
            cmd.stderr(std::process::Stdio::piped());

            let mut child = match cmd.spawn() {
                Ok(c) => c,
                Err(e) => {
                    soul_log_detail("ERROR", "download", "启动 modelscope 失败", &e.to_string());
                    let _ = app_clone.emit("download-progress", serde_json::json!({
                        "progress": 0,
                        "message": format!("启动下载失败: {}（请确认已安装 modelscope CLI）", e),
                        "status": "error",
                        "retryable": true,
                    }));
                    return;
                }
            };
            let pid = child.id();
            if let Ok(mut p) = st.pid.lock() { *p = Some(pid); }
            if let Ok(mut rc) = st.retry_count.lock() { *rc = attempt - 1; }
            soul_log("INFO", "download", &format!("modelscope 下载进程已启动 pid={} (第 {}/{} 次尝试)", pid, attempt, total_attempts));

            // 从 stdout 读取
            let app_r = app_clone.clone();
            let stdout_handle = if let Some(out) = child.stdout.take() {
                Some(std::thread::spawn(move || {
                    let reader = BufReader::new(out);
                    for line in reader.lines() {
                        if let Ok(text) = line {
                            let trimmed = text.trim();
                            if !trimmed.is_empty() {
                                let _ = app_r.emit("download-progress", serde_json::json!({
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
            let app_e = app_clone.clone();
            let stderr_handle = if let Some(err) = child.stderr.take() {
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
                        let _ = app_e.emit("download-progress", serde_json::json!({
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
                if attempt < total_attempts {
                    let _ = app_clone.emit("download-progress", serde_json::json!({
                        "progress": 0,
                        "message": format!("下载失败（退出码: {}），第 {}/{} 次重试...", code, attempt, total_attempts - 1),
                        "status": "retrying",
                    }));
                    continue;
                }
                let _ = app_clone.emit("download-progress", serde_json::json!({
                    "progress": 0,
                    "message": format!("{} 下载失败：已尝试 {} 次（退出码: {}），请检查网络后点击重试", model_clone, attempts_done, code),
                    "status": "error",
                    "retryable": true,
                }));
                return;
            }

            // ── modelscope 下载成功 → 完整性校验 ──
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
                // 完全没文件 → 若还有尝试次数则自动重下
                if attempt < total_attempts {
                    soul_log("WARN", "download", &format!("未下载到任何文件（第 {}/{} 次尝试），自动重试", attempt, total_attempts));
                    let _ = app_clone.emit("download-progress", serde_json::json!({
                        "progress": 0,
                        "message": format!("未下载到文件，第 {}/{} 次重试...", attempt, total_attempts - 1),
                        "status": "retrying",
                    }));
                    continue;
                }
                let _ = app_clone.emit("download-progress", serde_json::json!({
                    "progress": 0,
                    "message": format!("{} 下载失败：已尝试 {} 次均未下载到文件，请检查网络或模型 ID", model_clone, attempts_done),
                    "status": "error",
                    "retryable": true,
                }));
                return;
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
                    // 记录到本地 manifest（启动时体积复核、设置页可全量校验的基准）
                    record_model_hash(gf, &hash, fsize);
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
            // 完整性异常（文件过小/无法读取）→ 清理可疑文件后自动重下
            soul_log("WARN", "download", &format!("完整性校验未通过（第 {}/{} 次尝试），清理后重试", attempt, total_attempts));
            for gf in &gguf_files {
                let _ = std::fs::remove_file(gf);
            }
            if attempt < total_attempts {
                let _ = app_clone.emit("download-progress", serde_json::json!({
                    "progress": 0,
                    "message": format!("文件异常已清理，第 {}/{} 次重试...", attempt, total_attempts - 1),
                    "status": "retrying",
                }));
                continue;
            }
            let _ = app_clone.emit("download-progress", serde_json::json!({
                "progress": 0,
                "message": format!("{} 下载失败：已尝试 {} 次，文件始终异常，请检查磁盘或网络", model_clone, attempts_done),
                "status": "error",
                "retryable": true,
            }));
        }
        } // loop
    });

    Ok("下载已开始".to_string())
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

/// 格式化时间戳为 HH:MM:SS（UTC+8 本地时间）
fn format_ts(secs: u64) -> String {
    let (h, m, s) = local_hms(secs);
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
    //    30 秒收不满即放弃：客户端连上不发数据的连接不释放会导致线程永久堆积（线程泄漏根源）
    //    WouldBlock 需重试而非断开（Windows 上 accept 的连接会继承监听 socket 的非阻塞模式）
    let mut buf = Vec::new();
    let mut header_end = None;
    let header_deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
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
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if std::time::Instant::now() >= header_deadline {
                    soul_log("DEBUG", "proxy", "请求头 30 秒未收齐，断开（防泄漏）");
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(2));
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
            // 请求体同样限时收齐，防止半截 body 的僵尸连接占用线程
            let body_deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
            while total < remaining {
                if std::time::Instant::now() >= body_deadline {
                    soul_log("DEBUG", "proxy", "请求体 30 秒未收齐，放弃");
                    break;
                }
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

    // 3. 模型路由：检查 body 中的 model 字段，路由到对应端口（并记录目标模型名，供待机自动唤醒）
    let (target_llama_port, target_model_name) = if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&body) {
        if let Some(requested_model) = json.get("model").and_then(|v| v.as_str()) {
            if !requested_model.is_empty() && requested_model != model_name {
                // 查找目标模型端口
                let routed = get_model_router().lock().ok()
                    .and_then(|r| r.get(requested_model).copied());
                match routed {
                    Some(p) => (p + 1, requested_model.to_string()), // llama port = proxy port + 1
                    None => (own_llama_port, model_name.to_string()),
                }
            } else {
                (own_llama_port, model_name.to_string())
            }
        } else {
            (own_llama_port, model_name.to_string())
        }
    } else {
        (own_llama_port, model_name.to_string())
    };

    // 3.5 待机自动唤醒：目标模型在待机注册表 → 立即拉起进程并延长等待窗口
    //     覆盖所有经代理的请求（/chat、/v1/chat/completions 与透传端点统一生效）
    let mut wake_triggered = false;
    let in_standby = get_standby_registry().lock()
        .map(|r| r.contains_key(&target_model_name)).unwrap_or(false);
    if in_standby {
        soul_log("INFO", "proxy", &format!("请求命中待机模型 '{}'，自动唤醒", target_model_name));
        // 其他并发请求已触发唤醒时同样延长等待（Ok(false) 路径）
        match wake_standby_model(&target_model_name, None) {
            Ok(_) => wake_triggered = true,
            Err(e) => soul_log_detail("WARN", "proxy", &format!("待机模型 '{}' 自动唤醒失败", target_model_name), &e),
        }
    }
    // 3.6 记录最近活跃（空闲自动待机的依据；外部 API 调用同样计入）
    if let Some(handle) = APP_HANDLE.get() {
        if let Some(st) = handle.try_state::<RunningModelsState>() {
            if let Ok(mut models) = st.models.lock() {
                let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
                if let Some(m) = models.iter_mut().find(|m| m.name == target_model_name) {
                    m.last_active = now;
                }
            }
        }
    }

    let upstream_addr = format!("127.0.0.1:{}", target_llama_port);
    soul_log("DEBUG", "proxy", &format!("model='{}' path='{}' route_to={} own_llama={} wake={}", model_name, path, target_llama_port, own_llama_port, wake_triggered));
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
    // 命中待机唤醒时把等待窗口延长到 180 秒（容忍模型加载），常规请求保持 3 秒
    let deadline = std::time::Instant::now()
        + std::time::Duration::from_secs(if wake_triggered { 180 } else { 3 });
    while std::time::Instant::now() < deadline {
        match std::net::TcpStream::connect(&upstream_addr) {
            Ok(u) => { upstream = Some(u); break; }
            Err(_) => {
                if std::time::Instant::now() < deadline {
                    std::thread::sleep(std::time::Duration::from_millis(if wake_triggered { 400 } else { 300 }));
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

    // 5.5 唤醒场景：TCP 已通但模型可能仍在加载（/health 返回 503）→ 等健康检查通过再转发
    if wake_triggered {
        let http = reqwest::blocking::Client::new();
        while std::time::Instant::now() < deadline {
            let ready = http.get(format!("http://127.0.0.1:{}/health", target_llama_port))
                .timeout(std::time::Duration::from_secs(2))
                .send()
                .map(|r| r.status().as_u16() == 200)
                .unwrap_or(false);
            if ready { break; }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        if std::time::Instant::now() >= deadline {
            soul_log("ERROR", "proxy", &format!("待机模型 '{}' 唤醒后加载超时", target_model_name));
            let body_text = "{\"error\":\"Model loading timeout\",\"message\":\"模型加载超时，请稍后重试\",\"retry_after\":10}";
            let resp = format!(
                "HTTP/1.1 503 Service Unavailable\r\nAccess-Control-Allow-Origin: *\r\nContent-Type: application/json\r\nContent-Length: {}\r\nRetry-After: 10\r\nConnection: close\r\n\r\n{}",
                body_text.len(), body_text
            );
            let _ = client.write_all(resp.as_bytes());
            let _ = client.flush();
            return;
        }
        soul_log("INFO", "proxy", &format!("待机模型 '{}' 已就绪，开始转发", target_model_name));
    }
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

/// 从上游增量读取 HTTP 响应头（到 \r\n\r\n），已读字节暂存到 all
/// 返回头部长度（含终止符）；EOF/超时/异常返回 None。自动跳过 100 Continue。
fn read_upstream_headers(upstream: &mut std::net::TcpStream, all: &mut Vec<u8>) -> Option<usize> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(150);
    let mut chunk = [0u8; 8192];
    loop {
        if let Some(pos) = all.windows(4).position(|w| w == b"\r\n\r\n") {
            let end = pos + 4;
            // 100 Continue 非最终响应：丢弃后继续找真正的响应头
            let head = String::from_utf8_lossy(&all[..end.min(80)]);
            if head.starts_with("HTTP/1.") && head.contains("100 Continue") {
                all.drain(..end);
                continue;
            }
            return Some(end);
        }
        if std::time::Instant::now() >= deadline { return None; }
        match upstream.read(&mut chunk) {
            Ok(0) => return None,
            Ok(n) => all.extend_from_slice(&chunk[..n]),
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            Err(_) => return None, // 含 TimedOut / ConnectionReset
        }
    }
}

/// 从响应头文本解析 Content-Length
fn parse_content_length(header_str: &str) -> Option<usize> {
    header_str.lines()
        .find(|l| l.to_lowercase().starts_with("content-length:"))
        .and_then(|l| l.split(':').nth(1))
        .and_then(|s| s.trim().parse().ok())
}

/// 读完剩余 body 到 Content-Length 满 / EOF / 超时
/// content_length 已知时读满即收尾，避免 keep-alive 连接空等超时（防线程滞留）
fn read_upstream_body(upstream: &mut std::net::TcpStream, all: &mut Vec<u8>, body_start: usize, content_length: Option<usize>) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    let mut chunk = [0u8; 65536];
    loop {
        if let Some(cl) = content_length {
            if all.len() >= body_start + cl { return; }
        }
        if std::time::Instant::now() >= deadline { return; }
        match upstream.read(&mut chunk) {
            Ok(0) => return,
            Ok(n) => all.extend_from_slice(&chunk[..n]),
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            Err(_) => return,
        }
    }
}

/// 向客户端写一条 SSE 事件；写失败 = 客户端已断开，返回 false 让上层立即结束线程（防泄漏）
fn write_sse(client: &mut std::net::TcpStream, body: &serde_json::Value) -> bool {
    let data = format!("data: {}\n\n", serde_json::to_string(body).unwrap_or_default());
    client.write_all(data.as_bytes()).is_ok() && client.flush().is_ok()
}

/// 读取上游响应、转换为 OpenAI 格式、返回给客户端
/// requested_stream: 客户端是否期望 SSE 流式响应
/// 流式模式为真流式：上游每个 token 立即转为 OpenAI chunk 转发（不再攒完整回复后一次发 3 个 chunk）
fn forward_openai_response(upstream: &mut std::net::TcpStream, client: &mut std::net::TcpStream, requested_stream: bool) {
    upstream.set_read_timeout(Some(std::time::Duration::from_secs(120))).ok();
    client.set_write_timeout(Some(std::time::Duration::from_secs(30))).ok();

    // 1. 增量读取上游响应头
    let mut all: Vec<u8> = Vec::new();
    let hdr_end = match read_upstream_headers(upstream, &mut all) {
        Some(p) => p,
        None => {
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
    };
    let header_str = String::from_utf8_lossy(&all[..hdr_end]).to_string();

    // 2. 非 200：读完错误体直接透传（错误响应都很小）
    let first_line = header_str.lines().next().unwrap_or("");
    let status_code: u16 = first_line.split(' ').nth(1).unwrap_or("0").parse().unwrap_or(0);
    if status_code != 200 {
        eprintln!("[proxy] openai upstream status: {}, piping through", status_code);
        read_upstream_body(upstream, &mut all, hdr_end, None);
        let _ = client.write_all(&all);
        let _ = client.flush();
        return;
    }

    // 3. Content-Type 异常（text/html 说明请求未正确路由）
    let ct = header_str.lines().find(|l| l.to_lowercase().starts_with("content-type:")).unwrap_or("").to_lowercase();
    if ct.contains("text/html") {
        eprintln!("[proxy] openai: upstream returned text/html (wrong endpoint?)");
        let err_body = "{\"error\":\"wrong content type\",\"message\":\"上游返回了 text/html，请求可能未正确路由到 /completion\"}";
        let resp = format!("HTTP/1.1 502 Bad Gateway\r\nAccess-Control-Allow-Origin: *\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", err_body.len(), err_body);
        let _ = client.write_all(resp.as_bytes());
        let _ = client.flush();
        return;
    }

    let upstream_cl = parse_content_length(&header_str);

    // === 非流式模式：读完整个 JSON 后转换为 OpenAI 格式 ===
    if !requested_stream {
        read_upstream_body(upstream, &mut all, hdr_end, upstream_cl);
        let body = &all[hdr_end..];
        if body.len() <= 2 {
            eprintln!("[proxy] body too small");
            let resp = "HTTP/1.1 502 Bad Gateway\r\nAccess-Control-Allow-Origin: *\r\nContent-Type: application/json\r\nContent-Length: 0\r\n\r\n";
            let _ = client.write_all(resp.as_bytes());
            let _ = client.flush();
            return;
        }
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

        // JSON 解析失败（安全截取前 500 字符，避免多字节字符内切片 panic）
        let body_str = String::from_utf8_lossy(body).to_string();
        let preview: String = body_str.chars().take(500).collect();
        eprintln!("[proxy] openai JSON parse failed, raw body (first 500 chars): {:?}", preview);
        let resp = format!(
            "HTTP/1.1 502 Bad Gateway\r\nAccess-Control-Allow-Origin: *\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(), body_str
        );
        let _ = client.write_all(resp.as_bytes());
        return;
    }

    // === 流式模式：真流式透传 ===
    // 空响应探测：上游读完头即 EOF 且无 body → 模型未加载/已休眠（保持原 502 行为）
    if all.len() == hdr_end {
        upstream.set_read_timeout(Some(std::time::Duration::from_secs(2))).ok();
        let mut probe = [0u8; 1024];
        match upstream.read(&mut probe) {
            Ok(0) => {
                eprintln!("[proxy] stream: upstream returned empty body (model not loaded?)");
                let empty_body = "{\"error\":\"empty upstream response\",\"message\":\"模型未加载或已休眠，请检查服务状态\"}";
                let resp = format!("HTTP/1.1 502 Bad Gateway\r\nAccess-Control-Allow-Origin: *\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", empty_body.len(), empty_body);
                let _ = client.write_all(resp.as_bytes());
                let _ = client.flush();
                return;
            }
            Ok(n) => all.extend_from_slice(&probe[..n]),
            Err(_) => {} // 超时/暂时无数据 → 模型在推理（如长 prompt 预填充），按正常流继续
        }
        upstream.set_read_timeout(Some(std::time::Duration::from_secs(120))).ok();
    }

    let chatcmpl_id = format!("chatcmpl-soul-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis());
    let created = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();

    // 发送 SSE 响应头（真流式无 Content-Length，靠连接关闭收尾）
    let sse_hdr = "HTTP/1.1 200 OK\r\nAccess-Control-Allow-Origin: *\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n";
    if client.write_all(sse_hdr.as_bytes()).is_err() || client.flush().is_err() { return; }

    // 角色 chunk（OpenAI 惯例：首个 delta 携带 role）
    if !write_sse(client, &serde_json::json!({
        "id": &chatcmpl_id, "object": "chat.completion.chunk", "created": created,
        "choices": [{"index": 0, "delta": {"role": "assistant"}, "finish_reason": null}]
    })) { return; }

    // 逐行解析上游 SSE，每个 token 立即转发；客户端断开（写失败）则关闭上游提前结束
    let mut line_buf: Vec<u8> = Vec::new();
    line_buf.extend_from_slice(&all[hdr_end..]);
    let mut finished = false;
    let mut prompt_tokens: u64 = 0;
    let mut completion_tokens: u64 = 0;
    let mut chunk = [0u8; 8192];
    let mut idle_deadline = std::time::Instant::now() + std::time::Duration::from_secs(180);

    'outer: loop {
        // 先处理缓冲区里的完整行
        while let Some(nl) = line_buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = line_buf.drain(..=nl).collect();
            let mut s = String::from_utf8_lossy(&line[..line.len() - 1]).to_string();
            if s.ends_with('\r') { s.pop(); }
            let s = s.trim().to_string();
            if s.is_empty() { continue; }
            if s == "data: [DONE]" { break 'outer; }
            let json_str = if let Some(r) = s.strip_prefix("data: ") { r }
                else if let Some(r) = s.strip_prefix("data:") { r.trim() }
                else { s.as_str() };
            let json: serde_json::Value = match serde_json::from_str(json_str) { Ok(j) => j, Err(_) => continue };
            let c = json["content"].as_str().unwrap_or("");
            let stop = json["stop"].as_bool().unwrap_or(false);
            if !c.is_empty() {
                if !write_sse(client, &serde_json::json!({
                    "id": &chatcmpl_id, "object": "chat.completion.chunk", "created": created,
                    "choices": [{"index": 0, "delta": {"content": c}, "finish_reason": null}]
                })) {
                    // 客户端已断开：立即结束（关闭上游连接，llama-server 会取消生成）
                    return;
                }
            }
            if stop {
                finished = true;
                prompt_tokens = json["tokens_evaluated"].as_u64().unwrap_or(0);
                completion_tokens = json["tokens_predicted"].as_u64().unwrap_or(0);
                if !write_sse(client, &serde_json::json!({
                    "id": &chatcmpl_id, "object": "chat.completion.chunk", "created": created,
                    "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}],
                    "usage": {"prompt_tokens": prompt_tokens, "completion_tokens": completion_tokens, "total_tokens": prompt_tokens + completion_tokens}
                })) { return; }
                break; // stop 已收，补发 [DONE] 即可收尾
            }
        }
        if finished { break; }
        match upstream.read(&mut chunk) {
            Ok(0) => break, // 上游关闭
            Ok(n) => {
                line_buf.extend_from_slice(&chunk[..n]);
                idle_deadline = std::time::Instant::now() + std::time::Duration::from_secs(180);
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            Err(_) => break, // TimedOut / ConnectionReset 等
        }
        if std::time::Instant::now() >= idle_deadline {
            soul_log("WARN", "proxy", "流式响应 180 秒无数据，提前收尾");
            break;
        }
    }

    // 上游异常中断（未收到 stop）也补一个 finish chunk，客户端流才不会语义悬空
    if !finished {
        let _ = write_sse(client, &serde_json::json!({
            "id": &chatcmpl_id, "object": "chat.completion.chunk", "created": created,
            "choices": [{"index": 0, "delta": {}, "finish_reason": "length"}],
            "usage": {"prompt_tokens": prompt_tokens, "completion_tokens": completion_tokens, "total_tokens": prompt_tokens + completion_tokens}
        }));
    }
    let _ = client.write_all(b"data: [DONE]\n\n");
    let _ = client.flush();
    eprintln!("[proxy] openai stream done (finished={})", finished);
}

/// 纯管道传输（/chat、/health 等无需格式转换的端点）
/// Content-Length 感知：body 读满即收尾，避免 keep-alive 连接空等超时（防线程滞留）；
/// SSE 流式（无 CL）保持读到 EOF/超时的原始管道行为
fn pipe_raw_response(upstream: &mut std::net::TcpStream, client: &mut std::net::TcpStream) {
    upstream.set_read_timeout(Some(std::time::Duration::from_secs(120))).ok();
    client.set_write_timeout(Some(std::time::Duration::from_secs(30))).ok();

    // 先读响应头（短超时容忍），把已读部分立即转发给客户端
    let mut buf: Vec<u8> = Vec::new();
    let hdr_end = read_upstream_headers(upstream, &mut buf);
    if client.write_all(&buf).is_err() { return; }
    let _ = client.flush();

    let content_length = hdr_end
        .and_then(|h| parse_content_length(&String::from_utf8_lossy(&buf[..h])));

    let mut pipe_buf = [0u8; 65536];
    // Some(r) = 精确读完 r 字节即正常收尾；None = 读到 EOF/超时（SSE 流式或无 CL）
    let mut remaining = content_length;
    loop {
        if let Some(r) = remaining {
            if r == 0 { return; }
        }
        match upstream.read(&mut pipe_buf) {
            Ok(0) => return,
            Ok(n) => {
                if let Some(r) = remaining.as_mut() { *r = r.saturating_sub(n); }
                if client.write_all(&pipe_buf[..n]).is_err() { return; }
                let _ = client.flush();
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            Err(_) => return,
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

// ============================================================
// 开机自启 / 应用退出 / 应用内更新 / 会话全局搜索
// ============================================================

#[cfg(windows)]
const AUTORUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(windows)]
const AUTORUN_NAME: &str = "SoulAgentLauncher";

/// 查询开机自启状态（HKCU Run 注册表项）
#[tauri::command]
fn get_auto_launch() -> bool {
    #[cfg(windows)]
    {
        new_hidden_cmd("reg")
            .args(["query", AUTORUN_KEY, "/v", AUTORUN_NAME])
            .output()
            .map(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).contains(AUTORUN_NAME))
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// 设置开机自启（注册表 HKCU Run，无需管理员权限；--tray 表示启动后最小化到托盘）
#[tauri::command]
fn set_auto_launch(enable: bool) -> Result<String, String> {
    soul_log("INFO", "settings", &format!("set_auto_launch enable={}", enable));
    #[cfg(windows)]
    {
        let exe = std::env::current_exe().map_err(|e| format!("获取程序路径失败: {}", e))?;
        if enable {
            let value = format!("\"{}\" --tray", exe.to_string_lossy());
            new_hidden_cmd("reg")
                .args(["add", AUTORUN_KEY, "/v", AUTORUN_NAME, "/t", "REG_SZ", "/d", &value, "/f"])
                .output()
                .map_err(|e| format!("写入注册表失败: {}", e))?;
            Ok("已开启开机自启（将以托盘模式后台启动）".to_string())
        } else {
            new_hidden_cmd("reg")
                .args(["delete", AUTORUN_KEY, "/v", AUTORUN_NAME, "/f"])
                .output()
                .map_err(|e| format!("写入注册表失败: {}", e))?;
            Ok("已关闭开机自启".to_string())
        }
    }
    #[cfg(not(windows))]
    {
        let _ = enable;
        Err("仅支持 Windows".to_string())
    }
}

/// 退出应用（退出前清理模型进程）。此前前端在调用但后端从未注册。
#[tauri::command]
fn close_app(app_handle: tauri::AppHandle, state: State<RunningModelsState>) -> Result<(), String> {
    soul_log("INFO", "main", "close_app：退出应用");
    if let Ok(models) = state.models.lock() {
        for m in models.iter() {
            if let Ok(pid) = m.pid.parse::<u32>() {
                let _ = new_hidden_cmd("taskkill").args(["/F", "/PID", &pid.to_string()]).output();
            }
        }
    }
    app_handle.exit(0);
    Ok(())
}

/// 应用内下载更新包（MSI），进度通过 update-progress 事件推送（异步线程执行）
#[tauri::command]
fn download_update(app_handle: tauri::AppHandle, url: String) -> Result<String, String> {
    let app = app_handle.clone();
    std::thread::spawn(move || {
        soul_log("INFO", "update", &format!("开始下载更新包: {}", url));
        match download_update_blocking(&app, &url) {
            Ok(path) => {
                soul_log("INFO", "update", &format!("更新包下载完成: {:?}", path));
                let _ = app.emit("update-progress", serde_json::json!({
                    "status": "completed", "progress": 100, "path": path, "message": "下载完成",
                }));
            }
            Err(e) => {
                soul_log_detail("ERROR", "update", "更新包下载失败", &e);
                let _ = app.emit("update-progress", serde_json::json!({
                    "status": "error", "progress": 0, "message": e,
                }));
            }
        }
    });
    Ok("下载已开始".to_string())
}

fn download_update_blocking(app: &tauri::AppHandle, url: &str) -> Result<String, String> {
    // 断点续传：进度写到 .part 文件，重试时带 Range 头从已收字节继续；
    // 服务器不支持 Range（返回 200 而非 206）时自动退回整段重下
    let dest = std::env::temp_dir().join("Soul-Agent-Launcher-update.msi");
    let part = std::env::temp_dir().join("Soul-Agent-Launcher-update.msi.part");
    let max_retries = 3u32;
    let mut attempts: u32 = 0;

    loop {
        attempts += 1;
        let existing = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);

        let mut req = reqwest::blocking::Client::new()
            .get(url)
            .timeout(std::time::Duration::from_secs(1800));
        if existing > 0 {
            req = req.header("Range", format!("bytes={}-", existing));
        }
        let mut resp = match req.send() {
            Ok(r) => r,
            Err(e) => {
                soul_log_detail("WARN", "update", &format!("更新下载连接失败（第 {}/{} 次尝试）", attempts, max_retries + 1), &e.to_string());
                if attempts <= max_retries {
                    let _ = app.emit("update-progress", serde_json::json!({
                        "status": "downloading", "progress": 0,
                        "message": format!("连接失败，正在重试（{}/{}）...", attempts, max_retries),
                    }));
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    continue;
                }
                return Err(format!("连接更新服务器失败: {}", e));
            }
        };
        if !resp.status().is_success() {
            // 清掉坏的 .part（如 416 Range 越界），重试
            let _ = std::fs::remove_file(&part);
            if attempts <= max_retries {
                std::thread::sleep(std::time::Duration::from_secs(2));
                continue;
            }
            return Err(format!("更新服务器返回 {}", resp.status()));
        }

        // 206 = 续传成功；200 = 服务器不支持 Range，从头写
        let resume = resp.status() == reqwest::StatusCode::PARTIAL_CONTENT && existing > 0;
        let base: u64 = if resume { existing } else { 0 };
        let total: u64 = if resume {
            base + resp.content_length().unwrap_or(0)
        } else {
            resp.content_length().unwrap_or(0)
        };
        soul_log("INFO", "update", &format!("更新下载 status={} base={} total={}", resp.status(), base, total));

        let mut file = if resume {
            std::fs::OpenOptions::new().append(true).open(&part)
        } else {
            std::fs::File::create(&part)
        }.map_err(|e| format!("创建临时文件失败: {}", e))?;

        let mut received: u64 = base;
        let mut last_pct = 100u32; // 首个字节必触发一次进度事件
        let mut buffer = [0u8; 65536];
        loop {
            let n = resp.read(&mut buffer).map_err(|e| format!("下载中断: {}", e))?;
            if n == 0 { break; }
            file.write_all(&buffer[..n]).map_err(|e| format!("写入失败: {}", e))?;
            received += n as u64;
            if total > 0 {
                let pct = ((received as f64 / total as f64) * 100.0) as u32;
                if pct != last_pct {
                    last_pct = pct;
                    let _ = app.emit("update-progress", serde_json::json!({
                        "status": "downloading", "progress": pct,
                        "message": format!("已下载 {:.1}MB / {:.1}MB{}", received as f64 / 1048576.0, total as f64 / 1048576.0,
                            if resume { "（断点续传）" } else { "" }),
                    }));
                }
            }
        }
        file.flush().ok();
        if total > 0 && received != total {
            // 不完整：保留 .part 供下次续传
            if attempts <= max_retries {
                let _ = app.emit("update-progress", serde_json::json!({
                    "status": "downloading", "progress": ((received as f64 / total as f64) * 100.0) as u32,
                    "message": format!("下载不完整（{:.1}/{:.1}MB），自动续传重试（{}/{}）...",
                        received as f64 / 1048576.0, total as f64 / 1048576.0, attempts, max_retries),
                }));
                std::thread::sleep(std::time::Duration::from_secs(2));
                continue;
            }
            return Err(format!("下载不完整（{}/{} 字节），已保留进度，请重试", received, total));
        }

        // 完成：.part → 最终文件
        std::fs::rename(&part, &dest).map_err(|e| format!("移动下载文件失败: {}", e))?;
        return Ok(dest.to_string_lossy().to_string());
    }
}

/// 计算文件 SHA256（用于更新包完整性校验）
fn sha256_file(path: &std::path::Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|e| format!("无法打开文件: {}", e))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = file.read(&mut buf).map_err(|e| format!("读取失败: {}", e))?;
        if n == 0 { break; }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

// ============================================================
// 模型完整性 manifest：下载时记录 SHA256/体积，启动时体积复核
// （ModelScope 不下发 hash，本地首验记录是唯一的比对基准）
// ============================================================

fn model_hashes_path() -> PathBuf {
    get_launcher_dir().join("model-hashes.json")
}

/// 读取 manifest（路径 → {sha256, size, verified_at}），损坏时返回空表
fn load_model_hashes() -> serde_json::Map<String, serde_json::Value> {
    std::fs::read_to_string(model_hashes_path())
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("records").and_then(|r| r.as_object()).cloned())
        .unwrap_or_default()
}

/// 写回 manifest
fn save_model_hashes(records: serde_json::Map<String, serde_json::Value>) {
    let dir = get_launcher_dir();
    let _ = std::fs::create_dir_all(&dir);
    let doc = serde_json::json!({ "version": 1, "records": records });
    let tmp = model_hashes_path().with_extension("json.tmp");
    if std::fs::write(&tmp, serde_json::to_vec_pretty(&doc).unwrap_or_default()).is_ok() {
        let _ = std::fs::rename(&tmp, model_hashes_path());
    }
}

/// 下载校验通过后记录单文件 hash
fn record_model_hash(path: &std::path::Path, sha256: &str, size: u64) {
    let key = path.to_string_lossy().to_string();
    let mut records = load_model_hashes();
    records.insert(key, serde_json::json!({
        "sha256": sha256,
        "size": size,
        "verified_at": now_iso(),
    }));
    save_model_hashes(records);
    soul_log("INFO", "integrity", &format!("已记录模型 hash: {}", path.display()));
}

/// 启动模型前的快速完整性复核（只比体积，不重算 hash）
/// 返回 Some(警告文本) 表示与下载记录不一致
fn quick_integrity_check(model_path: &str) -> Option<String> {
    let records = load_model_hashes();
    let rec = records.get(model_path)?;
    let expected = rec.get("size").and_then(|s| s.as_u64())?;
    let actual = std::fs::metadata(model_path).map(|m| m.len()).ok()?;
    if expected != actual {
        Some(format!(
            "模型文件体积与下载记录不符（记录 {:.1}MB，当前 {:.1}MB），文件可能已损坏或被替换",
            expected as f64 / 1048576.0, actual as f64 / 1048576.0
        ))
    } else {
        None
    }
}

/// 安装更新：生成提权批处理（杀进程 → msiexec 静默安装 → 重启新版），
/// 经 PowerShell -Verb RunAs 触发一次 UAC，之后由批处理接管整个流程。
/// 服务器下发了 sha256 时，先校验安装包完整性再提权。
#[tauri::command]
fn install_update(app_handle: tauri::AppHandle, msi_path: String, expected_sha256: Option<String>) -> Result<String, String> {
    // 完整性校验（服务器提供 hash 时）
    if let Some(expected) = expected_sha256 {
        let expected = expected.trim().to_lowercase();
        if !expected.is_empty() {
            soul_log("INFO", "update", "校验更新包 SHA256...");
            let actual = sha256_file(std::path::Path::new(&msi_path))?;
            if actual != expected {
                soul_log_detail("ERROR", "update", "更新包 SHA256 不匹配", &format!("期望 {} 实得 {}", expected, actual));
                let _ = app_handle.emit("update-progress", serde_json::json!({
                    "status": "error", "progress": 0,
                    "message": "更新包校验失败（SHA256 不匹配），文件可能损坏，请重试下载",
                }));
                return Err("更新包校验失败（SHA256 不匹配），文件可能损坏，请重试下载".to_string());
            }
            soul_log("INFO", "update", "更新包 SHA256 校验通过");
        }
    }

    let exe = std::env::current_exe().map_err(|e| format!("获取程序路径失败: {}", e))?;
    let exe_name = exe.file_name().and_then(|s| s.to_str())
        .ok_or("获取程序名失败")?.to_string();
    let exe_path = exe.to_string_lossy().to_string();

    let bat = std::env::temp_dir().join("SAL_update_install.cmd");
    let content = format!(
        "@echo off\r\n\
         taskkill /f /im \"{exe_name}\" >nul 2>&1\r\n\
         timeout /t 2 /nobreak >nul\r\n\
         msiexec /i \"{msi_path}\" /qn /norestart\r\n\
         timeout /t 2 /nobreak >nul\r\n\
         start \"\" \"{exe_path}\"\r\n",
        exe_name = exe_name,
        msi_path = msi_path,
        exe_path = exe_path,
    );
    std::fs::write(&bat, content).map_err(|e| format!("写入安装脚本失败: {}", e))?;

    soul_log("INFO", "update", &format!("启动更新安装: msi={:?} exe={:?}", msi_path, exe));
    let ps = format!(
        "Start-Process -FilePath '{}' -Verb RunAs -WindowStyle Hidden",
        bat.to_string_lossy().replace('\'', "''"),
    );
    new_hidden_cmd("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &ps])
        .spawn()
        .map_err(|e| format!("启动安装程序失败: {}", e))?;

    let _ = app_handle.emit("update-progress", serde_json::json!({
        "status": "installing", "progress": 100, "message": "已提交系统安装",
    }));
    Ok("安装程序已启动".to_string())
}

/// 全局搜索结果条目
#[derive(Serialize)]
struct SessionSearchHit {
    session_id: String,
    session_title: String,
    message_index: usize,
    role: String,
    snippet: String,
}

/// 全局搜索：跨所有会话的消息内容搜索（不区分大小写，命中返回上下文片段）
#[tauri::command]
fn search_sessions(query: String, limit: Option<usize>) -> Result<Vec<SessionSearchHit>, String> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let max = limit.unwrap_or(50);
    let dir = get_sessions_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }
    soul_log("INFO", "session", &format!("search_sessions query={:?} limit={}", query, max));

    let mut hits: Vec<SessionSearchHit> = Vec::new();
    let mut entries: Vec<std::fs::DirEntry> = std::fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .collect();
    // 会话 id 形如 s{秒}{随机}，字典序≈创建时间序，倒序优先搜新会话
    entries.sort_by_key(|e| std::cmp::Reverse(e.file_name()));

    for entry in entries {
        if hits.len() >= max { break; }
        let path = entry.path();
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if !name.ends_with(".msgs.json") { continue; }
        let session_id = name.trim_end_matches(".msgs.json").to_string();
        let content = match std::fs::read_to_string(&path) { Ok(c) => c, Err(_) => continue };
        let msgs: Vec<serde_json::Value> = match serde_json::from_str(&content) { Ok(v) => v, Err(_) => continue };

        let title = std::fs::read_to_string(dir.join(format!("{}.json", session_id))).ok()
            .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok())
            .and_then(|m| m.get("title").and_then(|t| t.as_str()).map(|s| s.to_string()))
            .unwrap_or_else(|| "未命名会话".to_string());

        for (idx, msg) in msgs.iter().enumerate() {
            if hits.len() >= max { break; }
            let text = msg.get("content").and_then(|c| c.as_str()).unwrap_or("").to_string();
            let lower = text.to_lowercase();
            // to_lowercase 可能改变字节长度（极少数 Unicode 字符），此时放弃精确切片
            let pos = lower.find(&q).and_then(|p| if lower.len() == text.len() { Some(p) } else { None });
            if let Some(p) = pos {
                let mut start = p.saturating_sub(40);
                while start > 0 && !text.is_char_boundary(start) { start -= 1; }
                let mut end = (p + q.len() + 40).min(text.len());
                while end < text.len() && !text.is_char_boundary(end) { end += 1; }
                let snippet = text[start..end].replace(['\n', '\r'], " ");
                hits.push(SessionSearchHit {
                    session_id: session_id.clone(),
                    session_title: title.clone(),
                    message_index: idx,
                    role: msg.get("role").and_then(|r| r.as_str()).unwrap_or("").to_string(),
                    snippet: format!("{}{}{}", if start > 0 { "…" } else { "" }, snippet, if end < text.len() { "…" } else { "" }),
                });
            }
        }
    }
    Ok(hits)
}

/// 查询 NVIDIA 显存状态（多卡取空闲最大的一张；无 NVIDIA 返回 available=false）
#[derive(Serialize)]
struct VramStatus {
    available: bool,
    free_mb: u64,
    total_mb: u64,
}

#[tauri::command]
fn check_vram_free() -> VramStatus {
    let mut best = VramStatus { available: false, free_mb: 0, total_mb: 0 };
    let mut cmd = new_hidden_cmd("nvidia-smi");
    cmd.args(["--query-gpu=memory.free,memory.total", "--format=csv,noheader,nounits"]);
    if let Ok(out) = cmd.output() {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                let parts: Vec<&str> = line.split(',').collect();
                if parts.len() >= 2 {
                    if let (Ok(free), Ok(total)) = (
                        parts[0].trim().parse::<u64>(),
                        parts[1].trim().parse::<u64>(),
                    ) {
                        if free > best.free_mb {
                            best = VramStatus { available: true, free_mb: free, total_mb: total };
                        }
                    }
                }
            }
        }
    }
    best
}

/// 查询文件大小（MB），用于加载前的显存估算
#[tauri::command]
fn get_file_size_mb(path: String) -> u64 {
    std::fs::metadata(&path).map(|m| m.len() / (1024 * 1024)).unwrap_or(0)
}

/// 全量校验模型文件完整性：当前 SHA256/体积 vs 下载时记录（大文件计算耗时，走阻塞线程池）
#[tauri::command]
async fn verify_model_hash(path: String) -> Result<serde_json::Value, String> {
    let p = std::path::Path::new(&path);
    if !p.exists() {
        return Err(format!("文件不存在: {}", path));
    }
    let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    let record = load_model_hashes().get(&path).cloned();
    let current = tauri::async_runtime::spawn_blocking(move || sha256_file(std::path::Path::new(&path)))
        .await
        .map_err(|e| format!("校验任务失败: {}", e))?
        .map_err(|e| format!("计算 SHA256 失败: {}", e))?;

    let recorded_sha = record.as_ref()
        .and_then(|r| r.get("sha256")).and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let recorded_size = record.as_ref()
        .and_then(|r| r.get("size")).and_then(|v| v.as_u64());
    let recorded_at = record.as_ref()
        .and_then(|r| r.get("verified_at")).and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Ok(serde_json::json!({
        "size": size,
        "current_sha256": current,
        "has_record": record.is_some(),
        "recorded_sha256": recorded_sha,
        "recorded_size": recorded_size,
        "recorded_at": recorded_at,
        "hash_match": recorded_sha.as_ref().map(|h| h.eq_ignore_ascii_case(&current)),
        "size_match": recorded_size.map(|s| s == size),
    }))
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

            // 全局句柄：代理线程/待机引擎借此访问状态与发事件
            let _ = APP_HANDLE.set(app.handle().clone());

            // 崩溃自动恢复监控线程（每 5 秒巡检 llama 进程，首次延迟 5 秒避开启动期）
            let monitor_handle = app.handle().clone();
            start_crash_monitor(monitor_handle);

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

            // 以 --tray 启动（开机自启场景）：隐藏主窗口，仅在托盘驻留
            if std::env::args().any(|a| a == "--tray") {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                    soul_log("INFO", "main", "以 --tray 模式启动，窗口已隐藏到托盘");
                }
            }

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
        .manage(knowledge::KnowledgeState::new())
        .invoke_handler(tauri::generate_handler![
            check_server,
            check_server_port,
            check_standby,
            check_idle,
            get_sleep_logs,
            get_auto_launch,
            set_auto_launch,
            close_app,
            download_update,
            install_update,
            search_sessions,
            check_vram_free,
            get_file_size_mb,
            verify_model_hash,
            sleep_model,
            wake_model,
            set_standby_idle_min,
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
            check_disk_space,
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
            get_model_llama_port,
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
            knowledge::kb_add_folder,
            knowledge::kb_list_folders,
            knowledge::kb_remove_folder,
            knowledge::kb_index_file,
            knowledge::kb_search,
            knowledge::kb_get_context,
            knowledge::kb_toggle,
            knowledge::kb_clear_temp,
            knowledge::kb_status,
            knowledge::kb_embed_all,
            knowledge::kb_read_file,
        ])
        .run(tauri::generate_context!())
        .expect("启动失败");
}
