// Soul Agent Launcher - 前端逻辑

// ============================================================
// 更新检查
// ============================================================

const UPDATE_API_URL = 'https://sal.bszx.site/api/check-update';
const SKIP_UPDATE_KEY = 'sal-skip-update';

async function checkUpdate() {
    try {
        var currentVersion = '0.3.3';
        var resp = await fetch(UPDATE_API_URL + '?current_version=' + currentVersion, {
            signal: AbortSignal.timeout(5000)
        });
        var data = await resp.json();
        if (!data.has_update) return;

        // 检查是否已跳过此版本
        var skipped = localStorage.getItem(SKIP_UPDATE_KEY);
        if (skipped === data.latest_version) return;

        showUpdateCard(data);
    } catch (e) {
        console.warn('检查更新失败:', e);
    }
}

function showUpdateCard(data) {
    var existing = document.getElementById('updateCard');
    if (existing) existing.remove();

    var card = document.createElement('div');
    card.id = 'updateCard';
    card.className = 'update-card';
    card.innerHTML =
        '<div class="update-card-content">' +
            '<div class="update-card-icon">📦</div>' +
            '<div class="update-card-body">' +
                '<div class="update-card-title">SAL ' + data.latest_version + ' 发布！</div>' +
                '<div class="update-card-desc">' + (data.release_notes || '建议更新至最新版！') + '</div>' +
            '</div>' +
        '</div>' +
        '<div class="update-card-actions">' +
            '<button class="btn btn-accent" onclick="doUpdate(\'' + data.download_url + '\')">更新</button>' +
            '<button class="btn btn-outline" onclick="skipUpdate(\'' + data.latest_version + '\')">跳过</button>' +
        '</div>';

    document.body.appendChild(card);
    // 渐显动画
    requestAnimationFrame(function() {
        card.classList.add('visible');
    });
}

window.doUpdate = function(url) {
    if (url) window.open(url, '_blank');
    var card = document.getElementById('updateCard');
    if (card) card.remove();
};

window.skipUpdate = function(version) {
    localStorage.setItem(SKIP_UPDATE_KEY, version);
    var card = document.getElementById('updateCard');
    if (card) card.remove();
};

// ============================================================
// 启动步骤管理器
// ============================================================

const SETUP_STEPS = {
    LANG:      { id: 'lang',      label: '语言检测',   desc: '检测系统语言环境', critical: false, skippable: false },
    THEME:     { id: 'theme',     label: '主题加载',   desc: '加载用户主题偏好', critical: false, skippable: false },
    CONFIG:    { id: 'config',    label: '配置恢复',   desc: '从配置文件恢复设置', critical: true,  skippable: false },
    BACKEND:   { id: 'backend',   label: '硬件检测 + 后端同步', desc: '检测 GPU/CPU 并自动匹配加速端', critical: true, skippable: false },
    LLAMA_VER: { id: 'llama',     label: '版本校验',   desc: '校验 llama.cpp 版本一致性', critical: false, skippable: true },
    PYTHON:    { id: 'python',    label: 'Python 检测', desc: '检测 Python 是否已安装', critical: false, skippable: true },
    PIP:       { id: 'pip',       label: 'pip 检测',   desc: '检测 pip 包管理器', critical: false, skippable: true },
    MODEL_SCOPE:{id: 'mscope',    label: 'ModelScope', desc: '安装模型下载工具', critical: false, skippable: true },
    SETTINGS:  { id: 'settings',  label: '加载设置',   desc: '加载应用设置', critical: false, skippable: false },
    UPDATE:    { id: 'update',    label: '版本检查',   desc: '检查新版本更新', critical: false, skippable: true },
};

var _setupState = {};       // stepId -> { status, error }
var _currentStep = null;    // 当前正在执行的 stepId
var _stepSkipRequested = false;
var _setupErrorLog = [];    // 收集所有错误用于报告

function initSetupState() {
    Object.values(SETUP_STEPS).forEach(s => {
        _setupState[s.id] = { status: 'pending', error: null };
    });
    _stepSkipRequested = false;
    _setupErrorLog = [];
}

function renderSetupSteps() {
    var container = document.getElementById('setupSteps');
    if (!container) return;
    container.innerHTML = Object.values(SETUP_STEPS).map(s => `
        <div class="setup-step" id="step-${s.id}">
            <div class="setup-step-icon pending" id="step-icon-${s.id}">○</div>
            <div>
                <div class="setup-step-label">${s.label}</div>
                <div class="setup-step-desc">${s.desc}</div>
            </div>
            <div class="setup-step-status" id="step-status-${s.id}"></div>
        </div>
    `).join('');
}

function updateStepUI(stepId, status, extra) {
    var icon = document.getElementById('step-icon-' + stepId);
    var statusEl = document.getElementById('step-status-' + stepId);
    if (!icon || !statusEl) return;
    icon.className = 'setup-step-icon ' + status;
    var icons = { pending: '○', running: '◌', success: '✓', skipped: '—', error: '✗' };
    icon.textContent = icons[status] || '○';
    var labels = { pending: '', running: '进行中…', success: '完成', skipped: '已跳过', error: '失败' };
    statusEl.textContent = extra || labels[status] || '';
    statusEl.className = 'setup-step-status ' + status;
    _setupState[stepId].status = status;
    if (status === 'error') {
        _setupErrorLog.push({ step: stepId, error: extra || '未知错误' });
    }
}

function updateSetupMessage(msg) {
    var el = document.getElementById('setupMessage');
    if (el) el.textContent = msg;
}

async function runStep(stepId, fn) {
    if (_stepSkipRequested) return 'skipped';
    _currentStep = stepId;
    updateStepUI(stepId, 'running');
    showSetupStepError(false);
    try {
        await fn();
        updateStepUI(stepId, 'success');
        return 'success';
    } catch (e) {
        var msg = e.message || String(e);
        debugLog(`步骤 ${stepId} 失败: ${msg}`);
        var stepDef = SETUP_STEPS[stepId.toUpperCase().replace(/-/g,'_')];
        var isCritical = stepDef && stepDef.critical;
        var isSkippable = stepDef && stepDef.skippable;

        updateStepUI(stepId, 'error', msg.substring(0, 60));
        _setupErrorLog.push({ step: stepId, error: msg, critical: isCritical });

        if (isCritical) {
            // 关键错误 → 显示错误报告
            showCriticalError(stepId, msg);
            return 'failed';  // 不再继续
        }

        // 非关键错误 → 可跳过
        var shouldSkip = await showStepError(stepId, msg, isSkippable);
        if (shouldSkip) {
            updateStepUI(stepId, 'skipped');
            return 'skipped';
        }
        // 选择了重试
        return await runStep(stepId, fn);
    }
}

function showSetupStepError(hide) {
    var box = document.getElementById('setupErrorBox');
    if (box) box.classList.toggle('hidden', hide !== false);
}

function showStepError(stepId, msg, skippable) {
    return new Promise((resolve) => {
        var box = document.getElementById('setupErrorBox');
        var title = document.getElementById('setupErrorTitle');
        var msgEl = document.getElementById('setupErrorMsg');
        var skipBtn = document.getElementById('setupErrorSkipBtn');
        var retryBtn = document.getElementById('setupErrorRetryBtn');
        if (!box || !title || !msgEl) { resolve(true); return; }

        title.textContent = '提示';
        msgEl.textContent = `${msg}\n\n${skippable ? '点击「跳过」可继续，之后可手动配置。' : '点击「重试」再试一次。'}`;
        skipBtn.style.display = skippable ? 'inline-block' : 'none';
        box.classList.remove('hidden');

        window._resolveStepError = function(choice) {
            box.classList.add('hidden');
            resolve(choice === 'skip');
        };
        skipBtn.onclick = function() { window._resolveStepError('skip'); };
        retryBtn.onclick = function() { window._resolveStepError('retry'); };
    });
}

function showCriticalError(stepId, msg) {
    var box = document.getElementById('setupReportBox');
    var msgEl = document.getElementById('setupReportMsg');
    var detail = document.getElementById('setupReportDetail');
    if (!box) return;
    box.classList.remove('hidden');
    msgEl.textContent = `${stepId} 步骤失败，无法继续运行。`;
    var report = '=== Soul Agent Launcher 错误报告 ===\n';
    report += '时间: ' + new Date().toLocaleString() + '\n';
    report += '版本: ' + (window.__TAURI__?.os?.version ? 'Tauri' : 'Browser') + '\n\n';
    report += '错误详情:\n';
    _setupErrorLog.forEach(e => {
        report += `  [${e.step}] ${e.error}\n`;
    });
    report += '\n请将此报告提交至 https://sal.bszx.site/ 反馈区\n';
    detail.value = report;
}

window.copyErrorReport = function() {
    var detail = document.getElementById('setupReportDetail');
    if (detail) {
        navigator.clipboard.writeText(detail.value);
        alert('错误报告已复制到剪贴板');
    }
};

window.openFeedback = function() {
    window.open('https://sal.bszx.site/', '_blank');
};

window.closeApp = function() {
    if (window.__TAURI__?.core?.invoke) {
        window.__TAURI__.core.invoke('close_app').catch(function(){});
    }
};

window.skipCurrentStep = function() {
    if (window._resolveStepError) window._resolveStepError('skip');
};

window.retryCurrentStep = function() {
    if (window._resolveStepError) window._resolveStepError('retry');
};

// ============================================================
// 初始化
// ============================================================
document.addEventListener('DOMContentLoaded', async () => {
    debugLog('DOMContentLoaded 开始');
    initSetupState();
    renderSetupSteps();

    // === 步骤 1: 语言检测 ===
    await runStep('lang', async () => {
        initLanguage();
        debugLog('语言检测完成');
    });

    // === 步骤 2: 主题 ===
    await runStep('theme', async () => {
        initTheme();
        debugLog('主题加载完成');
    });

    // === 步骤 3: 配置恢复 ===
    await runStep('config', async () => {
        try {
            var config = await window.__TAURI__.core.invoke('load_config');
            if (config && config.llama_path) {
                localStorage.setItem('launcher-llama-path', config.llama_path);
                if (config.port) localStorage.setItem('launcher-port', config.port);
                if (config.ctx_size) localStorage.setItem('launcher-ctx-size', String(config.ctx_size));
            }
        } catch (e) {
            // 首次运行，无配置文件，正常
        }
    });

    // === 步骤 4: 硬件检测 + 后端同步 ===
    await runStep('backend', async () => {
        await checkAndRunSetup();
    });

    // === 步骤 5: llama.cpp 版本校验 ===
    await runStep('llama', async () => {
        await checkLlamaVersion();
    });

    // === 步骤 6: Python 检测 ===
    await runStep('python', async () => {
        var installed = await window.__TAURI__.core.invoke('check_python_installed');
        if (!installed) {
            updateStepUI('python', 'running', '未安装，正在安装…');
            await window.__TAURI__.core.invoke('install_python');
        }
    });

    // === 步骤 7: pip 检测 ===
    await runStep('pip', async () => {
        var installed = await window.__TAURI__.core.invoke('check_pip_installed');
        if (!installed) {
            updateStepUI('pip', 'running', '未安装，正在安装…');
            await window.__TAURI__.core.invoke('install_pip');
        }
    });

    // === 步骤 8: ModelScope 检测 ===
    await runStep('mscope', async () => {
        await checkAndInstallModelscope();
    });

    hideSetupOverlay();
    debugLog('所有环境检测完成');

    loadSettings();
    initNavigation();
    initLaunchPage();
    startStatusPolling();

    // 延迟 5 秒检查更新
    setTimeout(checkUpdate, 5000);

    listenServerOutput();
    debugLog('DOMContentLoaded 全部完成');
});

/// 诊断日志：写入 Rust 端日志文件
async function debugLog(msg) {
    if (window.__TAURI__?.core?.invoke) {
        try { await window.__TAURI__.core.invoke('frontend_log', { message: msg }); } catch(e) {}
    }
    console.log('[SAL]', msg);
}

async function listenServerOutput() {
    if (!window.__TAURI__?.event?.listen) return;

    try {
        await window.__TAURI__.event.listen('server-stderr', (event) => {
            const msg = event.payload?.message || '';
            // 过滤掉无关的日志行
            if (msg.includes('llama_model_loader') ||
                msg.includes('loading') ||
                msg.includes('load_data') ||
                msg.includes('n_tensors') ||
                msg.includes('mem required') ||
                msg.includes('offloading') ||
                msg.includes('server is starting') ||
                msg.startsWith('.')) return;
            addConsoleLine('warn', msg);
        });
    } catch (e) {
        console.warn('stderr 监听失败:', e);
    }

    try {
        await window.__TAURI__.event.listen('server-stdout', (event) => {
            const msg = event.payload?.message || '';
            if (!msg.trim()) return;
            addConsoleLine('info', msg);
        });
    } catch (e) {
        console.warn('stdout 监听失败:', e);
    }
}

// ============================================================
// 首次启动设置
// ============================================================

async function checkAndRunSetup() {
    debugLog('checkAndRunSetup 开始');

    // 先尝试从 Rust 端 config.json 恢复配置
    debugLog('尝试从 Rust 端恢复配置...');
    try {
        var config = await window.__TAURI__.core.invoke('load_config');
        debugLog('load_config 返回: ' + JSON.stringify(config));
        if (config && config.llama_path) {
            debugLog('从 config.json 恢复了 llama_path: ' + config.llama_path);
            localStorage.setItem('launcher-llama-path', config.llama_path);
            if (config.port) localStorage.setItem('launcher-port', config.port);
            if (config.ctx_size) localStorage.setItem('launcher-ctx-size', String(config.ctx_size));
        }
    } catch (e) {
        debugLog('load_config 失败 (可能首次安装): ' + e);
    }

    // 调用后端自动同步：检测硬件 → 自动解压匹配的后端
    // 如果后端已安装且匹配，直接返回 "synced"
    debugLog('调用 check_and_sync_backend...');
    try {
        var syncResult = await window.__TAURI__.core.invoke('check_and_sync_backend');
        debugLog('check_and_sync_backend 返回: ' + syncResult);

        if (syncResult === 'synced' || syncResult === 'ok') {
            // 后端已就绪，无需操作
            hideSetupOverlay();
            // 重新加载配置（同步后 config.json 已更新）
            try {
                var config2 = await window.__TAURI__.core.invoke('load_config');
                if (config2 && config2.llama_path && !localStorage.getItem('launcher-llama-path')) {
                    localStorage.setItem('launcher-llama-path', config2.llama_path);
                    if (config2.port) localStorage.setItem('launcher-port', config2.port);
                    if (config2.ctx_size) localStorage.setItem('launcher-ctx-size', String(config2.ctx_size));
                }
            } catch(e) {}
            checkLlamaVersion();
            return;
        }
    } catch (e) {
        debugLog('check_and_sync_backend 失败: ' + e);
        // 回退到旧流程
    }

    // === 旧版回退流程：检查 setup_needed ===
    try {
        // 先检查是否已安装（仅文件存在性检查，不触发 GPU 检测）
        debugLog('调用 check_setup_needed...');
        var needsSetup = await window.__TAURI__.core.invoke('check_setup_needed');
        debugLog('needsSetup: ' + needsSetup);
        if (!needsSetup) {
            console.log('[setup] 已安装，跳过检测');
            hideSetupOverlay();
            checkLlamaVersion();
            return;
        }

        // 确实需要安装，显示 overlay 并检测 GPU
        console.log('[setup] 需要首次安装，显示 overlay');
        showSetupOverlay();
        updateSetupProgress(0, '正在检测系统环境...');

        let gpuInfo = null;
        try {
            gpuInfo = await window.__TAURI__.core.invoke('detect_gpu');
            const gpuLabel = gpuInfo.cuda_version
                ? `${gpuInfo.gpu_name} (CUDA ${gpuInfo.cuda_version})`
                : gpuInfo.gpu_name;
            addConsoleLine('info', `检测到硬件: ${gpuLabel}`);
        } catch (e) {
            console.warn('GPU 检测失败:', e);
        }

        // 监听进度事件
        if (window.__TAURI__?.event?.listen) {
            await window.__TAURI__.event.listen('setup-progress', (event) => {
                const { progress, message } = event.payload;
                updateSetupProgress(progress, message);
            });
        }

        // 显示检测到的 GPU 信息
        if (gpuInfo) {
            const backendLabel = gpuInfo.cuda_version
                ? `CUDA ${gpuInfo.cuda_version}`
                : gpuInfo.backend.toUpperCase();
            updateSetupProgress(2, `检测到 ${gpuInfo.gpu_name} (${backendLabel})，自动选择最优包...`);
            await new Promise(r => setTimeout(r, 800));
        }

        // 运行安装
        await window.__TAURI__.core.invoke('run_setup');

        // 安装成功，自动加载配置
        await autoLoadConfig();

        // 检查并安装 modelscope SDK
        await checkAndInstallModelscope();

        // 延迟后隐藏 overlay
        setTimeout(async () => {
            hideSetupOverlay();
            loadSettings();
            refreshLaunchModelList();
            pollServerStatus();
        }, 1000);

    } catch (e) {
        updateSetupProgress(0, `安装失败: ${e}`);
        document.getElementById('setupStatus').innerHTML = `
            <span style="color:var(--danger)">${e}</span>
        `;
    }
}

function updateSetupProgress(progress, message) {
    const fill = document.getElementById('setupProgressFill');
    const text = document.getElementById('setupProgressText');
    const msg = document.getElementById('setupMessage');

    fill.style.width = progress + '%';
    text.textContent = `${progress}%`;
    if (msg) msg.textContent = message;
}

function hideSetupOverlay() {
    const overlay = document.getElementById('setupOverlay');
    if (overlay) overlay.classList.add('hidden');
}

function showSetupOverlay() {
    const overlay = document.getElementById('setupOverlay');
    if (overlay) overlay.classList.remove('hidden');
}

/// 每次启动检查 llama.cpp 版本是否与当前设备匹配，设备变更时自动重新安装
async function checkLlamaVersion() {
    console.log('[setup] checkLlamaVersion 开始...');
    try {
        const info = await window.__TAURI__.core.invoke('check_llama_installed');
        console.log('[setup] check_llama_installed 返回:', JSON.stringify(info));
        if (info.update_needed) {
            console.log('[setup] 需要更新！旧 key 与当前不匹配');
            addConsoleLine('warn', `检测到设备变更 (${info.package_key})，正在自动切换安装...`);

            showSetupOverlay();
            updateSetupProgress(0, `检测到设备变更，正在重新安装 ${info.package_key}...`);

            if (window.__TAURI__?.event?.listen) {
                await window.__TAURI__.event.listen('setup-progress', (event) => {
                    const { progress, message } = event.payload;
                    updateSetupProgress(progress, message);
                });
            }

            await window.__TAURI__.core.invoke('run_setup');
            await autoLoadConfig();
            hideSetupOverlay();
            loadSettings();
            addConsoleLine('success', `已自动切换到 ${info.package_key} 后端 (${info.gpu_name})`);
        } else if (info.installed) {
            console.log('[setup] 版本匹配，跳过更新');
            addConsoleLine('info', `llama.cpp 已就绪 (${info.backend} / ${info.package_key})`);
        }
    } catch (e) {
        console.warn('[setup] llama 版本检查失败:', e);
    }
}

// ============================================================
// 环境检测与自动安装
// ============================================================

/// 检查并安装 modelscope SDK
async function checkAndInstallModelscope() {
    try {
        const available = await window.__TAURI__.core.invoke('check_modelscope_available');
        if (available) {
            console.log('modelscope SDK 已就绪');
            return;
        }

        // 需要安装，复用 setup overlay
        showSetupOverlay();
        updateSetupProgress(0, '正在安装 modelscope SDK...');

        // 监听安装进度
        if (window.__TAURI__?.event?.listen) {
            await window.__TAURI__.event.listen('ms-install-progress', (event) => {
                const { progress, message } = event.payload;
                updateSetupProgress(progress, message);
            });
        }

        await window.__TAURI__.core.invoke('install_modelscope');
        addConsoleLine('success', 'modelscope SDK 安装完成');
        hideSetupOverlay();

    } catch (e) {
        console.warn('modelscope 安装失败（可手动安装）:', e);
        addConsoleLine('warn', `modelscope 安装失败: ${e}`);
        hideSetupOverlay();
    }
}

// ============================================================
// Tauri 窗口控制
// ============================================================

async function minimizeWindow() {
    try {
        await window.__TAURI__.core.invoke('minimize_window');
    } catch (e) {
        console.error('最小化失败:', e);
    }
}

async function maximizeWindow() {
    try {
        await window.__TAURI__.core.invoke('maximize_window');
    } catch (e) {
        console.error('最大化失败:', e);
    }
}

async function closeWindow() {
    try {
        await window.__TAURI__.core.invoke('close_window');
    } catch (e) {
        console.error('关闭失败:', e);
    }
}

// ============================================================
// 主题
// ============================================================
function initTheme() {
    const saved = localStorage.getItem('launcher-theme');
    if (saved === 'dark') {
        document.documentElement.setAttribute('data-theme', 'dark');
        document.getElementById('themeToggle').checked = true;
        document.getElementById('themeLabel').textContent = '深色模式';
    } else {
        document.documentElement.removeAttribute('data-theme');
        document.getElementById('themeToggle').checked = false;
        document.getElementById('themeLabel').textContent = '亮色模式';
    }

    document.getElementById('themeToggle').addEventListener('change', (e) => {
        if (e.target.checked) {
            document.documentElement.setAttribute('data-theme', 'dark');
            document.getElementById('themeLabel').textContent = '深色模式';
            localStorage.setItem('launcher-theme', 'dark');
        } else {
            document.documentElement.removeAttribute('data-theme');
            document.getElementById('themeLabel').textContent = '亮色模式';
            localStorage.setItem('launcher-theme', 'light');
        }
    });
}

// ============================================================
// 导航切换（PCL 2 风格）
// ============================================================
function initNavigation() {
    document.querySelectorAll('.nav-item').forEach(item => {
        item.addEventListener('click', (e) => {
            e.preventDefault();
            const page = item.dataset.page;
            switchPage(page);
        });
    });
}

function switchPage(pageId) {
    // 更新导航高亮
    document.querySelectorAll('.nav-item').forEach(n => n.classList.remove('active'));
    document.querySelector(`.nav-item[data-page="${pageId}"]`)?.classList.add('active');

    // 切换页面
    document.querySelectorAll('.page').forEach(p => p.classList.remove('active'));
    const page = document.getElementById(`page-${pageId}`);
    if (page) {
        page.classList.add('active');
        // 页面级初始化
        if (pageId === 'models') refreshModels();
        if (pageId === 'launch') { refreshLaunchModelList(); loadRunningModels(); }
        if (pageId === 'chat') { updateLiteInputState(); }
        if (pageId === 'sessions') { loadSessionList(); }
    } else {
        // 默认回到首页
        document.querySelector('.nav-item[data-page="home"]')?.classList.add('active');
        document.getElementById('page-home')?.classList.add('active');
    }
}

// ============================================================
// 服务管理 — 手动启动模型
// ============================================================

let statusPollInterval = null;

/// 初始化启动页面
function initLaunchPage() {
    refreshLaunchModelList();
}

/// 定期轮询服务状态
function startStatusPolling() {
    pollServerStatus();
    if (statusPollInterval) clearInterval(statusPollInterval);
    statusPollInterval = setInterval(pollServerStatus, 5000);
}

/// 检查服务状态并更新所有 UI
async function pollServerStatus() {
    const port = parseInt(
        document.getElementById('launchPort')?.value
        || localStorage.getItem('launcher-port')
        || '20000'
    );

    try {
        // 检查多模型中是否有任一在运行
        var models = await window.__TAURI__.core.invoke('list_running_models');
        var running = models && models.length > 0;
        var modelNames = running ? models.map(function(m) { return m.name; }).join(', ') : '';
        var totalVram = running ? models.reduce(function(sum, m) { return sum + (m.vram_mb || 0); }, 0) : 0;

        // 如果无模型运行，检查服务器是否正在加载（HTTP 503 = Loading model）
        var loading = false;
        if (!running) {
            try {
                var healthResp = await fetch('http://127.0.0.1:' + port + '/health', { method: 'GET', signal: AbortSignal.timeout(3000) });
                loading = healthResp.status === 503;
            } catch(e) {
                loading = false;
            }
        }

        var dot = document.getElementById('serverDot');
        var label = document.getElementById('serverLabel');
        var detail = document.getElementById('serverDetail');
        var startBtn = document.getElementById('startServerBtn');
        var stopBtn = document.getElementById('stopServerBtn');
        var apiEndpoint = document.getElementById('apiEndpoint');
        var apiUrl = document.getElementById('apiUrl');
        var badge = document.getElementById('serverStatusBadge');

        var statusClass = running ? 'online' : (loading ? 'loading' : 'offline');
        var statusText = running ? _t('server.running') : (loading ? _t('server.loading') : _t('server.stopped'));
        var detailText = running
            ? _tp('server.models', { count: models.length, names: modelNames })
            : (loading ? _t('server.loading_detail') : _tp('server.port_stopped', { port: port }));

        if (dot) dot.className = 'engine-dot ' + statusClass;
        if (label) label.textContent = statusText;
        if (detail) detail.textContent = detailText;
        if (startBtn) startBtn.style.display = (running || loading) ? 'none' : '';
        if (stopBtn) stopBtn.style.display = running ? '' : 'none';
        if (badge) badge.textContent = running ? _t('server.connected') : (loading ? _t('server.loading_badge') : _t('server.disconnected'));

        // 显示 API 端点（多模型时显示多个）
        if (running && apiEndpoint && apiUrl) {
            apiEndpoint.style.display = 'block';
            if (models.length === 1) {
                var p = models[0].proxy_port;
                apiUrl.innerHTML = '原生 /chat → <code>http://localhost:' + p + '/chat</code><br>OpenAI → <code>http://localhost:' + p + '/v1/chat/completions</code>';
            } else {
                var html = '<div style="margin-bottom:4px;">多模型端点：</div>';
                models.forEach(function(m) {
                    html += '<div style="font-size:10px;margin-bottom:2px;">[' + m.name + '] 原生 → <code>http://localhost:' + m.proxy_port + '/chat</code> · OpenAI → <code>http://localhost:' + m.proxy_port + '/v1/chat/completions</code></div>';
                });
                apiUrl.innerHTML = html;
            }
        } else if (apiEndpoint) {
            apiEndpoint.style.display = 'none';
        }

        // 更新首页 UI
        const homeDot = document.getElementById('homeStatusDot');
        const homeLabel = document.getElementById('homeStatusLabel');
        const homeDetail = document.getElementById('homeStatusDetail');
        const homeStartBtn = document.getElementById('homeStartBtn');
        const homeStopBtn = document.getElementById('homeStopBtn');
        const homeApiEndpoint = document.getElementById('homeApiEndpoint');
        const homeApiUrl = document.getElementById('homeApiUrl');

        if (homeDot) homeDot.className = 'status-dot ' + statusClass;
        if (homeLabel) homeLabel.textContent = running ? _t('server.home_running') : (loading ? _t('server.home_loading') : _t('server.home_stopped'));
        if (homeDetail) homeDetail.innerHTML = running
            ? models.length + ' 个模型 · ' + (totalVram > 0 ? '显存 ' + formatVram(totalVram) : '')
            : _tp('home.port', { port: port });
        if (homeStartBtn) homeStartBtn.textContent = running ? _t('server.home_manage') : (loading ? _t('server.home_loading_btn') : _t('server.home_start'));
        if (homeStopBtn) homeStopBtn.style.display = running ? '' : 'none';
        if (badge) {
            badge.textContent = running ? _t('server.connected') : (loading ? _t('server.loading_badge') : _t('server.disconnected'));
            badge.className = 'title-subtitle ' + (running ? 'online' : (loading ? 'loading' : 'offline'));
        }
        if (running && homeApiEndpoint && homeApiUrl) {
            homeApiEndpoint.style.display = 'block';
            homeApiUrl.innerHTML = `原生 /chat → http://localhost:${port}/chat<br>OpenAI → http://localhost:${port}/v1/chat/completions`;
        } else if (homeApiEndpoint) {
            homeApiEndpoint.style.display = 'none';
        }
    } catch (e) {
        console.warn('状态检查失败:', e);
    }
}

/// 启动服务（带模型）
async function startServer() {
    var llamaPath = localStorage.getItem('launcher-llama-path') || '';
    if (!llamaPath) {
        alert('请先在设置中配置 llama.cpp 路径');
        switchPage('settings');
        return;
    }

    var checked = document.querySelectorAll('#modelCheckList input[type="checkbox"]:checked');
    if (checked.length === 0) {
        alert('请先勾选要启动的模型');
        return;
    }

    var basePort = parseInt(document.getElementById('launchPort')?.value || localStorage.getItem('launcher-port') || '20000');
    var ctx = parseInt(document.getElementById('launchCtx')?.value || localStorage.getItem('launcher-ctx') || '4096');
    var started = 0;

    for (var i = 0; i < checked.length; i++) {
        var cb = checked[i];
        var modelPath = cb.value;
        var modelName = cb.getAttribute('data-name') || 'model-' + i;
        var port = basePort + i * 2;

        addConsoleLine('info', '正在启动模型: ' + modelName + ' (端口: ' + port + ', 上下文: ' + ctx + ')...');
        try {
            var result = await window.__TAURI__.core.invoke('start_model', {
                llamaPath: llamaPath,
                modelPath: modelPath,
                modelName: modelName,
                port: port,
                ctx: ctx,
            });
            addConsoleLine('success', modelName + ' 已启动: ' + result);
            started++;
        } catch (e) {
            addConsoleLine('error', modelName + ' 启动失败: ' + e);
        }
    }

    if (started > 0) {
        addConsoleLine('success', '成功启动 ' + started + '/' + checked.length + ' 个模型');
        pollServerStatus();
        loadRunningModels();
    }
}

/// 停止服务
async function stopServer() {
    addConsoleLine('warn', '正在停止所有模型...');
    try {
        var result = await window.__TAURI__.core.invoke('stop_all_models');
        addConsoleLine('success', result);
        pollServerStatus();
        loadRunningModels();
    } catch (e) {
        addConsoleLine('error', '停止失败: ' + e);
    }
}

// ============================================================
// 控制台
// ============================================================
function addConsoleLine(type, text) {
    var consoleEl = document.getElementById('consoleOutput');
    var line = document.createElement('div');
    line.className = 'console-line ' + type;
    var time = new Date().toLocaleTimeString();
    line.textContent = '[' + time + '] ' + text;
    consoleEl.appendChild(line);
    consoleEl.scrollTop = consoleEl.scrollHeight;
}

// ============================================================
// 模型管理
// ============================================================

function refreshLaunchModelList() {
    var modelsDir = localStorage.getItem('launcher-models-dir') || '';
    var container = document.getElementById('modelCheckList');
    if (!container) return;

    if (!modelsDir) {
        container.innerHTML = '<div class="model-check-empty">请先在设置中配置模型目录路径</div>';
        return;
    }

    window.__TAURI__.core.invoke('list_models', { modelsDir }).then(function(models) {
        if (models.length === 0) {
            container.innerHTML = '<div class="model-check-empty">未找到 GGUF 模型，请先下载</div>';
        } else {
            container.innerHTML = models.map(function(m) {
                return '<label class="win11-check-item">' +
                    '<input type="checkbox" value="' + m.path.replace(/"/g,'&quot;') + '" data-name="' + m.name.replace(/"/g,'&quot;') + '">' +
                    '<span class="win11-check-fake"></span>' +
                    '<span class="win11-check-label">' + m.name + '</span>' +
                    '<span class="win11-check-size">' + m.size + '</span>' +
                    '</label>';
            }).join('');
        }
        window._modelCache = models;
    }).catch(function() {
        container.innerHTML = '<div class="model-check-empty">加载失败</div>';
    });
}

var _importFilePath = null;
var _importFileDefaultName = null;

/// 打开文件选择器导入 GGUF 模型
function importModel() {
    var input = document.getElementById('importFileInput');
    input.value = '';
    input.onchange = function(e) {
        var file = e.target.files?.[0];
        if (!file) return;

        var srcPath = file.path || file.name;
        if (!srcPath) { alert('无法获取文件路径'); return; }

        var modelsDir = localStorage.getItem('launcher-models-dir') || '';
        if (!modelsDir) {
            alert('请先在设置中配置模型目录路径');
            switchPage('settings');
            return;
        }

        // 提取文件名作为默认名称
        var fname = srcPath.split(/[\\/]/).pop() || 'model.gguf';
        var defaultName = fname.replace(/\.gguf$/i, '');

        _importFilePath = srcPath;
        _importFileDefaultName = defaultName;

        document.getElementById('importFileInfo').textContent = '选中文件: ' + fname;
        document.getElementById('importNameInput').value = defaultName;
        document.getElementById('importOverlay').classList.remove('hidden');
    };
    input.click();
}

/// 取消导入
function cancelImport() {
    document.getElementById('importOverlay').classList.add('hidden');
    _importFilePath = null;
    _importFileDefaultName = null;
}

/// 确认导入
async function confirmImport() {
    var name = document.getElementById('importNameInput').value.trim();
    if (!name) { alert('请输入模型名称'); return; }
    if (name.match(/[<>:"/\\|?*]/)) { alert('名称包含非法字符'); return; }

    document.getElementById('importOverlay').classList.add('hidden');

    var modelsDir = localStorage.getItem('launcher-models-dir') || '';
    try {
        var result = await window.__TAURI__.core.invoke('import_model_file', {
            srcPath: _importFilePath,
            modelsDir: modelsDir,
            modelName: name,
        });
        _importFilePath = null;
        _importFileDefaultName = null;
        refreshModels();
    } catch (e) {
        alert('导入失败: ' + e);
        _importFilePath = null;
    }
}

async function refreshModels() {
    const list = document.getElementById('modelList');
    const modelsDir = localStorage.getItem('launcher-models-dir') || '';
    if (!modelsDir) {
        list.innerHTML = '<div class="model-empty"><p>请先在设置中配置模型目录路径</p></div>';
        return;
    }

    list.innerHTML = '<div class="model-empty"><p>正在扫描...</p></div>';

    try {
        const models = await window.__TAURI__.core.invoke('list_models', { modelsDir });

        if (models.length === 0) {
            list.innerHTML = '<div class="model-empty"><p>未找到 GGUF 模型文件<br><small>请将 .gguf 文件放入模型目录</small></p></div>';
            return;
        }

        list.innerHTML = models.map((m, i) => `
            <div class="model-item">
                <div>
                    <div class="model-item-name">${m.name}</div>
                    <div class="model-item-size">${m.size}</div>
                    <div class="model-item-path" title="${m.path}">${m.path}</div>
                </div>
                <div style="display:flex;gap:4px;">
                    <button class="btn btn-accent" onclick="selectModel(${i})">选择</button>
                    <button class="btn btn-danger" onclick="deleteModel(${i})" style="padding:6px 10px;font-size:12px;">删除</button>
                </div>
            </div>
        `).join('');
        window._modelCache = models;

    } catch (e) {
        list.innerHTML = `<div class="model-empty"><p>扫描失败: ${e}</p></div>`;
    }
}

function selectModel(index) {
    const models = window._modelCache || [];
    if (models[index]) {
        // 在启动页选择该模型
        const select = document.getElementById('launchModel');
        if (select) {
            for (let i = 0; i < select.options.length; i++) {
                if (select.options[i].value === models[index].path) {
                    select.selectedIndex = i;
                    break;
                }
            }
        }
        // 跳转到启动页
        switchPage('launch');
    }
}

async function deleteModel(index) {
    const models = window._modelCache || [];
    if (!models[index]) return;
    const m = models[index];

    if (!confirm(`确定要删除 ${m.name} 吗？\n\n路径: ${m.path}\n\n此操作不可撤销！`)) return;

    try {
        await window.__TAURI__.core.invoke('delete_model_file', { path: m.path });
        addConsoleLine('success', `已删除模型: ${m.name}`);
        refreshModels();
        refreshLaunchModelList();
    } catch (e) {
        alert(`删除失败: ${e}`);
    }
}

// ============================================================
// 魔塔社区 (Model Hub)
// ============================================================

let _hubQuery = 'GGUF';

function switchModelTab(tab) {
    document.querySelectorAll('.tab-btn').forEach(t => t.classList.remove('active'));
    document.querySelectorAll('.tab-content').forEach(t => t.classList.remove('active'));
    document.querySelector(`.tab-btn[onclick*="${tab}"]`)?.classList.add('active');
    document.getElementById(`tab-${tab}`)?.classList.add('active');
    if (tab === 'local') refreshModels();
    if (tab === 'hub') searchHubModels();
    if (tab === 'official') loadOfficialModels();
}

async function searchHubModels() {
    const input = document.getElementById('hubSearchInput');
    const query = (input?.value || _hubQuery).trim();
    if (!query) {
        // 显示令牌状态提示
        await showTokenStatus();
        return;
    }
    _hubQuery = query;
    _hubPage = 1;

    await doHubSearch();
}

async function showTokenStatus() {
    const status = document.getElementById('hubStatus');
    let hasToken = false;
    try {
        const token = await window.__TAURI__.core.invoke('read_token', { name: 'modelscope' });
        hasToken = !!token;
    } catch {}
    if (hasToken) {
        status.innerHTML = '<span class="hub-status-text"><svg class="svg-icon" style="width:16px;height:16px;vertical-align:text-bottom;margin-right:4px;stroke:var(--accent)"><use href="#icon-check"/></svg> 已检测到 modelScope 令牌，可下载私有模型</span>';
    } else {
        status.innerHTML = '<span class="hub-status-text"><svg class="svg-icon" style="width:16px;height:16px;vertical-align:text-bottom;margin-right:4px;stroke:var(--text-tertiary)"><use href="#icon-key"/></svg> 未检测到 modelScope 令牌，仅可下载公开模型<br><small>如需下载私有模型，请将令牌文件放入 tokens/ 目录</small></span>';
    }
}

function quickSearch(keyword, btn) {
    document.querySelectorAll('.chip').forEach(c => c.classList.remove('active'));
    if (btn) btn.classList.add('active');
    document.getElementById('hubSearchInput').value = keyword;
    _hubQuery = keyword;
    doHubSearch();
}

async function doHubSearch() {
    const status = document.getElementById('hubStatus');
    const grid = document.getElementById('hubGrid');

    status.innerHTML = '<span class="hub-status-text">正在搜索...</span>';
    grid.innerHTML = '';

    try {
        const models = await window.__TAURI__.core.invoke('search_models', {
            query: _hubQuery,
        });

        if (!models || models.length === 0) {
            status.innerHTML = '<span class="hub-status-text">未找到相关模型，请尝试其他关键词</span>';
            return;
        }

        status.innerHTML = `<span class="hub-status-text">找到 ${models.length} 个相关模型，按下载量排序</span>`;
        renderHubCards(models);
    } catch (e) {
        status.innerHTML = `<span class="hub-status-text" style="color:var(--danger)">搜索失败: ${e}</span>`;
    }
}

function renderHubCards(models) {
    const grid = document.getElementById('hubGrid');
    grid.innerHTML = models.map(m => {
        const modelId = m.id || `${m.owner}/${m.name}`;
        const shortId = modelId.length > 45 ? modelId.substring(0, 42) + '...' : modelId;
        const desc = m.description || '(暂无描述)';
        return `
            <div class="hub-card">
                <div class="hub-card-name" title="${modelId}">${m.name || shortId}</div>
                <div class="hub-card-owner">${m.owner} · ${m.task || '通用'}</div>
                <div class="hub-card-desc">${desc}</div>
                <div class="hub-card-meta">
                    <span><svg class="svg-icon" style="width:14px;height:14px;vertical-align:text-bottom;margin-right:2px;display:inline"><use href="#icon-download"/></svg> ${fmtCount(m.downloads)}</span>
                    <span><svg class="svg-icon" style="width:14px;height:14px;vertical-align:text-bottom;margin-right:2px;display:inline"><use href="#icon-calendar"/></svg> ${m.updated ? m.updated.substring(0, 10) : '-'}</span>
                </div>
                <div class="hub-card-actions">
                    <button class="btn btn-accent" onclick="downloadFromHub('${modelId}')">下载</button>
                    <button class="btn" onclick="openModelLink('${modelId}')">详情</button>
                </div>
            </div>
        `;
    }).join('');
}

function fmtCount(n) {
    if (n >= 10000) return (n / 10000).toFixed(1) + 'w';
    if (n >= 1000) return (n / 1000).toFixed(1) + 'k';
    return String(n);
}

function openModelLink(modelId) {
    const url = `https://www.modelscope.cn/models/${modelId}`;
    window.open(url, '_blank');
}

/// 从魔塔社区下载模型
async function downloadFromHub(modelId) {
    // 检查 modelscope CLI 是否可用
    let available = false;
    try {
        available = await window.__TAURI__.core.invoke('check_modelscope_available');
    } catch {}
    if (!available) {
        alert('需要安装 modelscope 命令行工具才能下载模型。\n\n请运行: pip install modelscope\n\n或者使用 Git 手动克隆:\n  git clone https://www.modelscope.cn/' + modelId + '.git');
        return;
    }

    const modelsDir = localStorage.getItem('launcher-models-dir') || '';
    if (!modelsDir) {
        alert('请先在设置中配置模型目录路径');
        switchPage('settings');
        return;
    }

    // 询问是否只下载 GGUF 文件
    const downloadOnlyGGUF = confirm('是否只下载 GGUF 文件？（点击"确定"只下载 GGUF，点击"取消"下载全部）');

    // 显示下载进度弹层
    showDownloadOverlay(modelId);

    try {
        const result = await window.__TAURI__.core.invoke('start_model_download', {
            modelId: modelId,
            localDir: modelsDir,
            includePattern: downloadOnlyGGUF ? '*.gguf' : null,
        });
        console.log('下载已启动:', result);
    } catch (e) {
        updateDownloadProgress(0, `启动失败: ${e}`);
        setTimeout(() => hideDownloadOverlay(), 3000);
    }
}

/// 取消下载
async function cancelDownload() {
    try {
        await window.__TAURI__.core.invoke('cancel_download');
        updateDownloadProgress(0, '已取消');
        setTimeout(() => hideDownloadOverlay(), 1500);
    } catch (e) {
        alert(e);
    }
}

// ============================================================
// 自定义确认对话框
// ============================================================

var _confirmResolve = null;

function showConfirmDialog(message) {
    return new Promise(function(resolve) {
        _confirmResolve = resolve;
        document.getElementById('confirmMessage').textContent = message;
        document.getElementById('confirmOverlay').classList.remove('hidden');
    });
}

function closeConfirmDialog(confirmed) {
    document.getElementById('confirmOverlay').classList.add('hidden');
    if (_confirmResolve) {
        _confirmResolve(confirmed);
        _confirmResolve = null;
    }
}

/// 显示下载弹层
let _downloadUnlisten = null;
let _downloadPulseTimer = null;
let _lastModelId = null;

function showDownloadOverlay(modelId) {
    _lastModelId = modelId;
    document.getElementById('downloadTitle').textContent = '正在下载模型';
    document.getElementById('downloadModelInfo').textContent = modelId;
    document.getElementById('downloadOverlay').classList.remove('hidden');

    // 重置按钮状态
    document.getElementById('cancelDownloadBtn').classList.remove('hidden');
    document.getElementById('retryDownloadBtn').classList.add('hidden');

    // 取消之前的监听和脉冲定时器
    if (_downloadUnlisten) { _downloadUnlisten(); _downloadUnlisten = null; }
    if (_downloadPulseTimer) { clearInterval(_downloadPulseTimer); _downloadPulseTimer = null; }

    // 启动脉冲定时器：每3秒推进一点进度，让用户感知"正在下载"
    let pulseProgress = 0;
    _downloadPulseTimer = setInterval(() => {
        if (pulseProgress < 95) {
            pulseProgress += Math.random() * 3 + 0.5;
            if (pulseProgress > 95) pulseProgress = 95;
            updateDownloadProgress(Math.floor(pulseProgress), '正在下载中，请稍候...');
        }
    }, 3000);

    // 监听下载进度事件
    if (window.__TAURI__?.event?.listen) {
        window.__TAURI__.event.listen('download-progress', (event) => {
            var data = event.payload;
            updateDownloadProgress(data.progress, data.message);

            if (data.status === 'completed') {
                updateDownloadProgress(100, data.message || '下载完成！');
                document.getElementById('cancelDownloadBtn').classList.add('hidden');
                document.getElementById('retryDownloadBtn').classList.add('hidden');
                refreshModels();
                setTimeout(function() { hideDownloadOverlay(); }, 3000);
            } else if (data.status === 'error') {
                updateDownloadProgress(0, '⚠️ ' + (data.message || '下载失败'));
                document.getElementById('cancelDownloadBtn').classList.add('hidden');
                document.getElementById('retryDownloadBtn').classList.remove('hidden');
            } else if (data.status === 'verifying') {
                updateDownloadProgress(99, data.message || '校验中...');
            } else if (data.status === 'retrying') {
                updateDownloadProgress(0, data.message || '重试中...');
                document.getElementById('cancelDownloadBtn').classList.remove('hidden');
                document.getElementById('retryDownloadBtn').classList.add('hidden');
            } else if (data.status === 'downloading') {
                // 取消脉冲定时器，用真实进度
                if (_downloadPulseTimer) {
                    clearInterval(_downloadPulseTimer);
                    _downloadPulseTimer = null;
                }
            }
        }).then(function(u) { _downloadUnlisten = u; });
    }
}

/// 重试下载
async function retryDownload() {
    document.getElementById('cancelDownloadBtn').classList.remove('hidden');
    document.getElementById('retryDownloadBtn').classList.add('hidden');
    updateDownloadProgress(0, '正在重试...');

    try {
        await window.__TAURI__.core.invoke('retry_download');
    } catch (e) {
        updateDownloadProgress(0, '⚠️ 重试启动失败: ' + e);
        document.getElementById('cancelDownloadBtn').classList.add('hidden');
        document.getElementById('retryDownloadBtn').classList.remove('hidden');
    }
}

function updateDownloadProgress(progress, message) {
    var fill = document.getElementById('downloadProgressFill');
    var pct = document.getElementById('downloadProgressPct');
    var msg = document.getElementById('downloadProgressMsg');
    if (fill) {
        var p = Math.min(Math.max(progress, 0), 100);
        fill.style.width = p + '%';
        if (progress === 0 && p === 0) fill.classList.add('indeterminate');
        else fill.classList.remove('indeterminate');
    }
    if (pct) {
        pct.textContent = progress >= 100 ? '100%' : (progress > 0 ? progress + '%' : '...');
    }
    if (msg) msg.textContent = message || '';
}

function hideDownloadOverlay() {
    document.getElementById('downloadOverlay').classList.add('hidden');
    // 清理下载事件监听和脉冲定时器
    if (_downloadUnlisten) { _downloadUnlisten(); _downloadUnlisten = null; }
    if (_downloadPulseTimer) { clearInterval(_downloadPulseTimer); _downloadPulseTimer = null; }
}

// ============================================================
// 官方模型
// ============================================================

let _officialModelsCache = null;

async function loadOfficialModels() {
    const list = document.getElementById('officialModelList');
    list.innerHTML = '<div class="official-loading">正在加载官方模型列表...</div>';

    try {
        // 每次都重新请求，确保拿到最新数据
        _officialModelsCache = await window.__TAURI__.core.invoke('list_official_models');
        const models = _officialModelsCache;

        if (!models || models.length === 0) {
            list.innerHTML = '<div class="official-loading">暂无官方模型</div>';
            return;
        }

        // 两级分组：先按 group（大分类），再按 family（小分类）
        const categories = {};  // { groupName: { familyName: [models] } }
        models.forEach((m, idx) => {
            const cat = m.group || m.family || '其他';
            const fam = m.family || '其他';
            if (!categories[cat]) categories[cat] = {};
            if (!categories[cat][fam]) categories[cat][fam] = [];
            categories[cat][fam].push({ ...m, _idx: idx });
        });

        list.innerHTML = Object.entries(categories).map(([catName, families]) => {
            const catId = 'oc-' + catName.replace(/[^a-zA-Z0-9]/g, '-');
            const totalModels = Object.values(families).reduce((sum, arr) => sum + arr.length, 0);
            return `
            <div class="official-category">
                <div class="official-category-title" onclick="toggleCategory('${catId}')">
                    <span class="oc-arrow" id="${catId}-arrow">▶</span>
                    ${catName}
                    <span class="oc-count">${totalModels} 个模型</span>
                </div>
                <div class="oc-body" id="${catId}">
                    ${Object.entries(families).map(([family, items]) => {
                        const groupId = 'og-' + catName.replace(/[^a-zA-Z0-9]/g, '-') + '-' + family.replace(/[^a-zA-Z0-9]/g, '-');
                        return `
                        <div class="official-group">
                            <div class="official-group-title" onclick="toggleOfficialGroup('${groupId}')">
                                <span class="og-arrow" id="${groupId}-arrow">▶</span>
                                ${family} 系列
                                <span class="og-count">${items.length} 个模型</span>
                            </div>
                            <div class="og-body" id="${groupId}">
                                ${items.sort((a, b) => {
                                    // 按参数大小排序（小 ➝ 大）
                                    var pa = extractParamSize(a.name);
                                    var pb = extractParamSize(b.name);
                                    return pa - pb;
                                }).map(m => {
                                    const defaultQuant = m.default_quant || 'q4_K_M';
                                    const quants = (m.quant || []).map(q => {
                                        const isDefault = q === defaultQuant;
                                        const label = isDefault ? `${q}（推荐）` : q;
                                        const active = isDefault ? ' active' : '';
                                        return `<span class="quant-chip${active}" data-model="${m._idx}" data-quant="${q}" onclick="selectOfficialQuant(this)">${label}</span>`;
                                    }).join('');
                                    const meta = [m.size, m.model_id].filter(Boolean).join(' · ');
                                    return `
                                    <div class="official-model-card">
                                        <div class="official-model-info">
                                            <div class="official-model-name">${m.name}</div>
                                            <div class="official-model-meta">${meta}</div>
                                            ${quants ? `<div class="official-model-quant">${quants}</div>` : ''}
                                            ${m.desc ? `<div class="official-model-desc">${m.desc}</div>` : ''}
                                        </div>
                                        <button class="btn btn-accent" onclick="downloadOfficialModel(${m._idx})">下载</button>
                                    </div>`;
                                }).join('')}
                            </div>
                        </div>`;
                    }).join('')}
                </div>
            </div>`;
        }).join('');

    } catch (e) {
        list.innerHTML = `<div class="official-loading" style="color:var(--danger)">加载失败: ${e}</div>`;
    }
}

/// 从模型名称中提取参数大小（7B → 7, 1.5B → 1.5, 0.5B → 0.5）
function extractParamSize(name) {
    if (!name) return 999;
    var m = name.match(/(\d+\.?\d*)\s*[Bb]/);
    if (m) return parseFloat(m[1]);
    return 999;
}

/// 切换官方模型分组的展开/收起（带平滑高度动画）
function toggleOfficialGroup(groupId) {
    const body = document.getElementById(groupId);
    const arrow = document.getElementById(groupId + '-arrow');
    if (!body || !arrow) return;

    const isOpen = body.classList.contains('og-open');
    if (isOpen) {
        // 收起：固定当前高度 → 设 0
        const h = body.scrollHeight;
        body.style.height = h + 'px';
        requestAnimationFrame(() => {
            body.style.height = '0px';
            body.classList.remove('og-open');
        });
        arrow.textContent = '▶';
    } else {
        // 展开：显示并测量目标高度 → 从 0 动画到目标高度
        body.classList.add('og-open');
        body.style.height = 'auto';
        const target = body.scrollHeight;
        body.style.height = '0px';
        requestAnimationFrame(() => {
            body.style.height = target + 'px';
        });
        // 动画完成后清除固定高度，让 flex 布局自由伸展
        const onEnd = () => {
            body.style.height = 'auto';
            body.removeEventListener('transitionend', onEnd);
        };
        body.addEventListener('transitionend', onEnd, { once: true });
        arrow.textContent = '▼';
    }
}

/// 切换大分类的展开/收起
function toggleCategory(catId) {
    const body = document.getElementById(catId);
    const arrow = document.getElementById(catId + '-arrow');
    if (!body || !arrow) return;
    const isOpen = body.classList.contains('oc-open');
    if (isOpen) {
        const h = body.scrollHeight;
        body.style.height = h + 'px';
        requestAnimationFrame(() => {
            body.style.height = '0px';
            body.classList.remove('oc-open');
        });
        arrow.textContent = '▶';
    } else {
        body.classList.add('oc-open');
        body.style.height = 'auto';
        const target = body.scrollHeight;
        body.style.height = '0px';
        requestAnimationFrame(() => {
            body.style.height = target + 'px';
        });
        const onEnd = () => {
            body.style.height = 'auto';
            body.removeEventListener('transitionend', onEnd);
        };
        body.addEventListener('transitionend', onEnd, { once: true });
        arrow.textContent = '▼';
    }
}

/// 点击量化标签切换选中状态
function selectOfficialQuant(el) {
    // 同一卡片内取消其他选中
    const parent = el.closest('.official-model-quant');
    parent.querySelectorAll('.quant-chip').forEach(c => c.classList.remove('active'));
    el.classList.add('active');
}

/// 获取当前选中的量化版本
function getSelectedQuant(modelIdx) {
    const chips = document.querySelectorAll(`.quant-chip[data-model="${modelIdx}"]`);
    if (chips.length === 0) return ''; // 没有量化选项时不匹配任何 quant
    const active = Array.from(chips).find(c => c.classList.contains('active'));
    return active ? active.dataset.quant : 'q4_K_M';
}

async function downloadOfficialModel(modelIdx) {
    const models = _officialModelsCache;
    if (!models || !models[modelIdx]) return;
    const m = models[modelIdx];

    const modelsDir = localStorage.getItem('launcher-models-dir') || '';
    if (!modelsDir) {
        alert('请先在设置中配置模型目录路径');
        switchPage('settings');
        return;
    }

    // 检查 modelscope CLI
    let available = false;
    try {
        available = await window.__TAURI__.core.invoke('check_modelscope_available');
    } catch {}
    if (!available) {
        alert('需要安装 modelscope 命令行工具才能下载模型。\n\n请运行: pip install modelscope');
        return;
    }

    // 构建 include 模式（仅文件名通配，不带仓库路径前缀）
    const selectedQuant = getSelectedQuant(modelIdx);
    const hasQuantOption = m.quant && m.quant.length > 0;
    const hasInclude = m.include ? true : false;
    const inc = m.include || '';
    let includePattern = null;
    if (selectedQuant) {
        includePattern = `*${selectedQuant}*.gguf`;
    } else if (hasQuantOption) {
        includePattern = inc || '*.gguf';
    } else if (hasInclude) {
        // GGUF 仓库但无量化选项（如 Qwen3-0.6B 只有 Q8_0）
        includePattern = inc;
    } else {
        // 无量化也无可选 include（FP16 原始模型如 DeepSeek Coder）→ 下载全部
        includePattern = null;
    }

    // 非 GGUF 模型 → 弹窗确认
    if (includePattern === null) {
        var confirmed = await showConfirmDialog(
            '您下载的模型无 GGUF 格式，可能需要进行自行格式转换，确认下载？'
        );
        if (!confirmed) return;
    }

    showDownloadOverlay(m.model_id);

    try {
        await window.__TAURI__.core.invoke('start_model_download', {
            modelId: m.model_id,
            localDir: modelsDir,
            includePattern: includePattern || null,
        });
    } catch (e) {
        updateDownloadProgress(0, `启动失败: ${e}`);
        setTimeout(() => hideDownloadOverlay(), 3000);
    }
}

// ============================================================
// 极致精简编译
// ============================================================

let _buildUnlisten = null;

async function buildMinimal() {
    const scriptPath = document.getElementById('buildScriptPath').value.trim();
    if (!scriptPath) {
        alert('请先选择编译脚本路径');
        return;
    }

    const llamaInstallDir = localStorage.getItem('launcher-llama-path') || '';
    let outputDir = '';
    if (llamaInstallDir) {
        // 输出到 llama 安装目录的父级
        outputDir = llamaInstallDir.substring(0, llamaInstallDir.lastIndexOf('\\llama\\'));
        if (!outputDir) outputDir = llamaInstallDir;
    } else {
        outputDir = 'C:\\Users\\Default\\AppData\\Roaming\\Soul-Agent-Launcher\\llama';
    }

    const status = document.getElementById('buildStatus');
    status.innerHTML = '准备中...';

    // 监听编译进度
    if (_buildUnlisten) { _buildUnlisten(); _buildUnlisten = null; }
    if (window.__TAURI__?.event?.listen) {
        window.__TAURI__.event.listen('build-progress', (event) => {
            const { progress, message } = event.payload;
            status.innerHTML = message;
        }).then(unlisten => { _buildUnlisten = unlisten; });
    }

    try {
        const result = await window.__TAURI__.core.invoke('build_llama_minimal', {
            scriptPath,
            outputDir,
        });
        status.innerHTML = `<span style="color:var(--success)">${result}</span>`;
        addConsoleLine('success', `极致精简编译: ${result}`);
    } catch (e) {
        status.innerHTML = `<span style="color:var(--danger)">编译失败: ${e}</span>`;
        addConsoleLine('error', `极致精简编译失败: ${e}`);
    }
}

// ============================================================
// 设置
// ============================================================

/// 安装后自动加载配置（开箱即用）
async function autoLoadConfig() {
    try {
        const config = await window.__TAURI__.core.invoke('load_config');
        if (config) {
            if (config.llama_path) localStorage.setItem('launcher-llama-path', config.llama_path);
            if (config.models_dir) localStorage.setItem('launcher-models-dir', config.models_dir);
            if (config.port) localStorage.setItem('launcher-port', String(config.port));
            if (config.ctx) localStorage.setItem('launcher-ctx', String(config.ctx));
        }
    } catch (e) {
        console.warn('自动加载配置失败:', e);
    }
}

// 首次启动时如果 localStorage 为空，从 Rust config.json 加载
let _configLoaded = false;
async function tryLoadConfigFile() {
    if (_configLoaded) return;
    _configLoaded = true;

    // 如果已有 localStorage 数据，说明不是第一次运行
    if (localStorage.getItem('launcher-llama-path')) return;

    try {
        const config = await window.__TAURI__.core.invoke('load_config');
        if (config) {
            if (config.llama_path) localStorage.setItem('launcher-llama-path', config.llama_path);
            if (config.models_dir) localStorage.setItem('launcher-models-dir', config.models_dir);
            if (config.port) localStorage.setItem('launcher-port', String(config.port));
            if (config.ctx) localStorage.setItem('launcher-ctx', String(config.ctx));
            if (config.auto_unload !== undefined) localStorage.setItem('launcher-auto-unload', String(config.auto_unload));
        }
    } catch (e) {
        console.warn('读取配置文件失败:', e);
    }
}

function loadSettings() {
    // 优先尝试从 Rust config.json 加载（仅首次）
    tryLoadConfigFile();
    
    const fields = {
        'llamaPath': localStorage.getItem('launcher-llama-path') || '',
        'modelsDir': localStorage.getItem('launcher-models-dir') || '',
        'defaultPort': localStorage.getItem('launcher-port') || '20000',
        'defaultCtx': localStorage.getItem('launcher-ctx') || '4096',
    };

    Object.entries(fields).forEach(([id, val]) => {
        const el = document.getElementById(id);
        if (el) el.value = val;
    });

    // 加载语言设置
    const langSelect = document.getElementById('langSelect');
    if (langSelect) langSelect.value = getCurrentLang();

    // 加载退出时卸载选项
    const autoUnload = localStorage.getItem('launcher-auto-unload');
    const autoUnloadEl = document.getElementById('autoUnloadModels');
    if (autoUnloadEl) autoUnloadEl.checked = autoUnload !== 'false';  // 默认 true
}

function onLangChange(value) {
    setLanguage(value);
    // 刷新 UI 中的动态文本
    updateServerStatusUI('stopped');
    loadSessionList();
    updateCtxDisplay();
}

// 暴露到全局供 HTML onclick 调用
window.onLangChange = onLangChange;

function saveSettings() {
    const data = {
        'launcher-llama-path': document.getElementById('llamaPath').value,
        'launcher-models-dir': document.getElementById('modelsDir').value,
        'launcher-port': document.getElementById('defaultPort').value,
        'launcher-ctx': document.getElementById('defaultCtx').value,
    };

    Object.entries(data).forEach(([key, val]) => {
        localStorage.setItem(key, val);
    });

    // 保存退出时卸载选项
    const autoUnloadEl = document.getElementById('autoUnloadModels');
    if (autoUnloadEl) {
        localStorage.setItem('launcher-auto-unload', autoUnloadEl.checked);
    }

    // 同步启动页面的端口和上下文
    const launchPort = document.getElementById('launchPort');
    const launchCtx = document.getElementById('launchCtx');
    if (launchPort) launchPort.value = data['launcher-port'];
    if (launchCtx) launchCtx.value = data['launcher-ctx'];

    alert('设置已保存');
}

async function selectFilePath(inputId) {
    try {
        // Tauri 2 dialog API (需安装 tauri-plugin-dialog)
        if (window.__TAURI__?.dialog?.open) {
            const result = await window.__TAURI__.dialog.open({
                title: '选择文件',
                filters: [{ name: '可执行文件', extensions: ['exe', 'bat'] }],
            });
            if (result) document.getElementById(inputId).value = result;
        } else {
            // fallback: 手动输入
            const path = prompt('请输入文件完整路径:');
            if (path) document.getElementById(inputId).value = path;
        }
    } catch (e) {
        console.warn('选择文件失败:', e);
        const path = prompt('请输入文件完整路径:');
        if (path) document.getElementById(inputId).value = path;
    }
}

async function selectFolderPath(inputId) {
    try {
        if (window.__TAURI__?.dialog?.open) {
            const result = await window.__TAURI__.dialog.open({
                title: '选择文件夹',
                directory: true,
            });
            if (result) document.getElementById(inputId).value = result;
        } else {
            const path = prompt('请输入文件夹路径:');
            if (path) document.getElementById(inputId).value = path;
        }
    } catch (e) {
        console.warn('选择文件夹失败:', e);
        const path = prompt('请输入文件夹路径:');
        if (path) document.getElementById(inputId).value = path;
    }
}

/// 简易行内 Markdown 渲染（无需第三方库）
function marked(text) {
    if (!text) return '';
    return text
        .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
        .replace(/```([\s\S]*?)```/g, '<pre><code>$1</code></pre>')
        .replace(/`([^`]+)`/g, '<code>$1</code>')
        .replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>')
        .replace(/\*(.+?)\*/g, '<em>$1</em>')
        .replace(/\n/g, '<br>');
}

// ============================================================
// Soul Agent Lite - 极简对话（UI 复刻 Soul Agent）
// ============================================================

let _liteMessages = [];
let _liteAbortController = null;
let _liteAccTokens = 0;
let _liteSessionSummary = '';   // 当前会话的自动总结
let _ctx80Triggered = false;    // 80% 阈值是否已触发过
let _pendingSummary = false;    // 消息处理中延迟触发的总结
let isLiteProcessing = false;   // 是否正在处理消息
const MAX_CTX_CHARS = 4096;     // 发送给 API 的总字符数硬上限

function getLiteTotalCtx() {
    return parseInt(document.getElementById('launchCtx')?.value
        || localStorage.getItem('launcher-ctx')
        || '4096');
}

function updateLiteCtx() {
    var total = getLiteTotalCtx();
    var used = _liteAccTokens || 0;
    var pct = total > 0 ? Math.min(used / total, 1) : 0;
    var circumference = 31.416;
    var offset = circumference * (1 - pct);
    var ring = document.getElementById('liteCtxRingFill');
    if (ring) ring.setAttribute('stroke-dashoffset', String(offset));
    var text = document.getElementById('liteCtxText');
    if (text) {
        var pctDisplay = Math.round(pct * 100);
        var usedK = (used / 1000).toFixed(2);
        var totalK = (total / 1000).toFixed(2);
        text.textContent = pctDisplay + '% - ' + usedK + 'K/' + totalK + 'K 已使用';
    }
}

function getLitePort() {
    return parseInt(document.getElementById('launchPort')?.value
        || localStorage.getItem('launcher-port')
        || '20000');
}

/// 清理模型输出中的 END token（仅从末尾剥离，不影响中间内容）
/// EOS tokens by model family:
///   ChatML (Qwen/DeepSeek/GLM/Yi): <|im_end|> <|im_start|>
///   Llama: </s>
///   Gemma: <end_of_turn> <eos>
///   DeepSeek native:  ＜end＞
function cleanResponseText(text) {
    if (!text) return '';
    // 只从字符串末尾反复剥离这些 token
    const endTokens = [
        '<|im_end|>', '<|im_start|>', '<|im_sep|>',
        '<|assistant|>', '<|user|>', '<|system|>',
        '</s>', '<s>',
        '<end_of_turn>', '<eos>', '<bos>',
    ];
    // 从末尾循环剥离（允许堆叠，如 <|im_end|><|im_end|>）
    let cleaned = text;
    while (true) {
        let found = false;
        for (const t of endTokens) {
            if (cleaned.endsWith(t)) {
                cleaned = cleaned.slice(0, -t.length).trimEnd();
                found = true;
                break;
            }
        }
        if (!found) break;
    }
    return cleaned.trim();
}

function isLiteServerRunning() {
    const badge = document.getElementById('serverStatusBadge');
    return badge && badge.textContent.includes('已连接');
}

function updateLiteInputState() {
    var textarea = document.getElementById('userInput');
    var btn = document.getElementById('sendBtn');
    var ok = isLiteServerRunning();
    if (textarea) textarea.disabled = !ok;
    if (btn) btn.disabled = !ok;
}

function addLiteMessage(role, content) {
    const container = document.getElementById('liteMessages');
    if (!container) return;
    // 移除欢迎消息
    var welcome = container.querySelector('.welcome-message');
    if (welcome) welcome.remove();

    var msgDiv = document.createElement('div');
    msgDiv.className = 'chat-msg ' + role;

    var bubble = document.createElement('div');
    bubble.className = 'chat-bubble';
    bubble.innerHTML = marked(content);

    var time = document.createElement('div');
    time.className = 'chat-time';
    time.textContent = new Date().toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' });

    msgDiv.appendChild(bubble);
    msgDiv.appendChild(time);
    container.appendChild(msgDiv);
    container.scrollTop = container.scrollHeight;
    return msgDiv;
}

/* ===== 深度思考文本框（CSS grid-template-rows 过渡驱动，同 Soul Agent） ===== */
function animateThinkBlock(block, expand) {
    if (expand) { block.classList.remove('collapsed'); }
    else { block.classList.add('collapsed'); }
}
function collapseThinkBlock(block, toggle) {
    animateThinkBlock(block, false);
    if (toggle) toggle.classList.add('collapsed');
}

function showLiteTyping() {
    var container = document.getElementById('liteMessages');
    if (!container) return;
    var welcome = container.querySelector('.welcome-message');
    if (welcome) welcome.remove();
    var indicator = document.createElement('div');
    indicator.id = 'liteTyping';
    indicator.className = 'chat-msg assistant';
    indicator.innerHTML = '<div class="typing-indicator"><span></span><span></span><span></span></div>';
    container.appendChild(indicator);
    container.scrollTop = container.scrollHeight;
}

function removeLiteTyping() {
    var el = document.getElementById('liteTyping');
    if (el) el.remove();
}

// ============================================================
// 自动总结功能（上下文监控 + 总结生成 + 总结注入）
// ============================================================

/// 估算当前上下文使用率（%）
function calcContextPct() {
    let totalChars = 0;
    _liteMessages.forEach(function(m) { totalChars += (m.content || '').length; });
    if (totalChars === 0) return 0;
    var estimatedTokens = Math.round(totalChars / 2);
    return Math.min(100, Math.round((estimatedTokens / getLiteTotalCtx()) * 100));
}

/// 检查上下文阈值并触发总结
async function checkContextThreshold(pct) {
    if (!_currentSessionId || _liteMessages.length === 0) return;

    // 95%：强制触发总结
    if (pct >= 95) {
        await triggerSummary();
        _liteAccTokens = 0;
        return;
    }

    // 80%：自动总结（无提示）
    if (pct >= 80 && !_ctx80Triggered) {
        _ctx80Triggered = true;
        if (isLiteProcessing) {
            _pendingSummary = true;
        } else {
            await triggerSummary();
            _liteAccTokens = 0;
            _ctx80Triggered = false;
        }
    }
}

/// 生成对话总结
async function triggerSummary() {
    if (!_currentSessionId || _liteMessages.length === 0) return;
    console.log('[summary] 正在生成对话总结...', _liteMessages.length, '条消息');

    var port = parseInt(
        document.getElementById('launchPort')?.value
        || localStorage.getItem('launcher-port')
        || '20000'
    );

    try {
        // 构建总结请求：所有消息 + 总结提示词
        var summaryMessages = _liteMessages.map(function(m) {
            return { role: m.role === 'think' ? 'assistant' : m.role, content: m.content };
        });
        summaryMessages.push({
            role: 'user',
            content: _t('summary.prompt')
        });

        var summary = await window.__TAURI__.core.invoke('chat_non_streaming', {
            port: port,
            model: 'default',
            messages: summaryMessages
        });

        if (!summary) {
            console.warn('[summary] 返回为空');
            return;
        }

        _liteSessionSummary = summary;
        console.log('[summary] 总结完成:', summary.substring(0, 80));

        // 持久化到磁盘
        await window.__TAURI__.core.invoke('write_session_summary', {
            sessionId: _currentSessionId,
            summary: summary
        });
    } catch (e) {
        console.warn('[summary] 生成失败:', e);
    }
}

/// 加载会话总结
async function loadSessionSummary(sessionId) {
    try {
        var summary = await window.__TAURI__.core.invoke('read_session_summary', {
            sessionId: sessionId
        });
        _liteSessionSummary = summary || '';
        if (summary) console.log('[summary] 已加载总结:', summary.substring(0, 60));
    } catch (e) {
        _liteSessionSummary = '';
    }
}

async function sendLiteMessage() {
    var input = document.getElementById('userInput');
    var text = input?.value.trim();
    if (!text) return;
    if (!isLiteServerRunning()) {
        addLiteMessage('system', '⚠️ 服务未运行，请先在「启动」页面启动服务');
        return;
    }

    input.value = '';
    input.style.height = 'auto';
    _liteMessages.push({ role: 'user', content: text });
    addLiteMessage('user', text);
    showLiteTyping();

    var port = _liteModelPort || getLitePort();
    var multimodal = _liteMultimodal;
    _liteAbortController = new AbortController();
    isLiteProcessing = true;

    // 构建消息数组：注入总结 + 裁剪上下文
    var bodyMessages = [];
    // 1. 如果有历史总结，作为 system 消息注入
    if (_liteSessionSummary) {
        bodyMessages.push({
            role: 'system',
            content: _tp('summary.inject', { summary: _liteSessionSummary })
        });
    }
    // 2. 加入最近消息 + 当前消息，裁剪总长度
    var recentCount = 6;  // 最近 6 条 + 当前用户消息
    var msgs = _liteMessages.slice(-recentCount);
    msgs.forEach(function(m) {
        bodyMessages.push({ role: m.role === 'think' ? 'assistant' : m.role, content: m.content });
    });
    // 3. 裁剪 if 超 MAX_CTX_CHARS
    var totalChars = bodyMessages.reduce(function(sum, m) { return sum + (m.content || '').length; }, 0);
    if (totalChars > MAX_CTX_CHARS) {
        // 从最旧的历史消息开始移除（跳过总结 system）
        while (bodyMessages.length > 2 && totalChars > MAX_CTX_CHARS) {
            var removeIdx = bodyMessages[0].role === 'system' ? 1 : 0;
            var removed = bodyMessages.splice(removeIdx, 1)[0];
            totalChars -= (removed.content || '').length;
        }
    }

    var body = {
        model: _liteSelectedModel || 'default',
        messages: bodyMessages,
        stream: true,
        thinking: _liteThinking,
    };
    if (multimodal) {
        body.multimodal = true;
        var msgs = body.messages;
        for (var i = msgs.length - 1; i >= 0; i--) {
            if (msgs[i].role === 'user') {
                msgs[i].content = [{ type: 'text', text: msgs[i].content }];
                break;
            }
        }
    }

    try {
        var resp = await fetch('http://127.0.0.1:' + port + '/chat', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(body),
            signal: _liteAbortController.signal,
        });

        if (!resp.ok) {
            removeLiteTyping();
            var errText = await resp.text().catch(function() { return ''; });
            addLiteMessage('system', _tp('chat.api_error', { status: resp.status, detail: errText ? ': ' + errText : '' }));
            _liteMessages.pop();
            return;
        }

        // 不在前面创建气泡，等第一个 Token 到达时才创建
        var msgDiv = null;
        var bubble = null;
        var time = null;

        var reader = resp.body.getReader();
        var decoder = new TextDecoder();
        var fullContent = '';
        var _liteThinkingContent = '';
        var _liteUsage = null;
        var thinkUIReady = false;
        var _liteThinkToggle, _liteThinkBlock, _liteThinkInner;
        var _msgInitialized = false;  // 气泡是否已创建
        var _lastTokenTime = Date.now();  // 上次收到 token 的时间
        var _noOutputTimeout = 60000;     // 无任何输出时超时 60 秒
        var _hasOutput = false;           // 是否已收到至少一个 token

        /// 接收第一个 Token 时：移除打字动画 + 创建气泡
        function initMsgBubble() {
            if (_msgInitialized) return;
            _msgInitialized = true;
            removeLiteTyping();

            msgDiv = document.createElement('div');
            msgDiv.className = 'chat-msg assistant streaming';
            bubble = document.createElement('div');
            bubble.className = 'chat-bubble';
            bubble.textContent = '';
            time = document.createElement('div');
            time.className = 'chat-time';
            msgDiv.appendChild(bubble);
            msgDiv.appendChild(time);
            document.getElementById('liteMessages').appendChild(msgDiv);
        }

        // 辅助：实时创建/展开思考框
        function ensureThinkUI() {
            if (thinkUIReady) return;
            if (!msgDiv) return;
            thinkUIReady = true;
            _liteThinkToggle = document.createElement('div');
            _liteThinkToggle.className = 'think-toggle';
            _liteThinkToggle.innerHTML = '<img class="think-icon" src="assets/sal-icon.png" width="14" height="14"> 思考过程';
            _liteThinkToggle.onclick = function() {
                var isColl = _liteThinkBlock.classList.contains('collapsed');
                animateThinkBlock(_liteThinkBlock, isColl);
                _liteThinkToggle.classList.toggle('collapsed', !isColl);
            };

            _liteThinkBlock = document.createElement('div');
            _liteThinkBlock.className = 'think-block';
            _liteThinkInner = document.createElement('div');
            _liteThinkInner.className = 'think-inner';
            _liteThinkBlock.appendChild(_liteThinkInner);

            msgDiv.prepend(_liteThinkToggle, _liteThinkBlock);
            document.getElementById('liteMessages').scrollTop = 1e9;
        }

        // 接收到第一个 Token 时
        function onFirstToken() {
            initMsgBubble();
        }

        while (true) {
            // 超时检测：Reader 阻塞时通过 race 实现超时
            var timeoutMs = _hasOutput ? 15000 : _noOutputTimeout;
            var readResult = await Promise.race([
                reader.read().then(function(r) { return { type: 'data', result: r }; }),
                new Promise(function(resolve) {
                    setTimeout(function() { resolve({ type: 'timeout' }); }, timeoutMs);
                })
            ]);

            if (readResult.type === 'timeout') {
                soulLog('chat', `流超时(${timeoutMs}ms)：${_hasOutput ? '已输出无新数据' : '等待首次响应超时'}`);
                break;  // 直接结束，已收到的内容保留显示
            }

            var result = readResult.result;
            if (result.done) break;
            var chunk = decoder.decode(result.value, { stream: true });
            var lines = chunk.split('\n');
            for (var j = 0; j < lines.length; j++) {
                var line = lines[j].trim();
                if (!line || !line.startsWith('data: ')) continue;
                var data = line.slice(6).trim();
                if (data === '[DONE]') continue;
                try {
                    var json = JSON.parse(data);
                    var delta = json?.choices?.[0]?.delta?.content || json?.content;
                    var thinkField = json?.thinking;
                    if (thinkField) {
                        _lastTokenTime = Date.now();
                        _hasOutput = true;
                        onFirstToken();
                        _liteThinkingContent += thinkField;
                        ensureThinkUI();
                        _liteThinkInner.textContent = _liteThinkingContent;
                        document.getElementById('liteMessages').scrollTop = 1e9;
                    }
                    if (delta) {
                        _lastTokenTime = Date.now();
                        _hasOutput = true;
                        onFirstToken();
                        // 清理模型特殊 token（仅末尾）
                        fullContent += cleanResponseText(delta);

                        // 每次迭代重新解析 fullContent 中的 <think> 标签
                        // 天然防重复：每次都是基于最新 fullContent 重新提取
                        var ts = '<think>', te = '</think>';
                        var si = fullContent.indexOf(ts);
                        var ei = fullContent.indexOf(te);

                        if (si >= 0) {
                            // 有 <think> 标签
                            var thinkEnd = ei >= 0 ? ei : fullContent.length;
                            var extractedThink = fullContent.slice(si + ts.length, thinkEnd).trim();
                            _liteThinkingContent = extractedThink;

                            ensureThinkUI();
                            _liteThinkInner.textContent = _liteThinkingContent;

                            // bubble 只显示 <think> 之前 + </think> 之后
                            var before = fullContent.slice(0, si);
                            var after = ei >= 0 ? fullContent.slice(ei + te.length).replace(/^\n+/, '') : '';
                            var bubbleShow = cleanResponseText(before + after);
                            bubble.textContent = bubbleShow;
                            // 思考期间如果 bubble 为空，隐藏以避免空气泡
                            bubble.style.display = (ei >= 0 && bubbleShow) ? '' : 'none';
                        } else {
                            // 没有 <think> 标签，全部是正文
                            bubble.style.display = '';
                            bubble.textContent = cleanResponseText(fullContent);
                        }

                        document.getElementById('liteMessages').scrollTop = 1e9;
                    }
                    if (json.usage) { _liteUsage = json.usage; }
                } catch (e) {}
            }
        }

        // 流结束
        // 安全兜底：如果全程无 Token，移除打字动画
        if (!_msgInitialized) {
            removeLiteTyping();
        }
        if (!bubble) {
            // 完全无输出，不做任何处理
        } else {
            bubble.style.display = ''; // 确保气泡可见
            msgDiv.classList.remove('streaming');

            // 提取纯正文（去掉 <think>...</think>）
            var finalContent = cleanResponseText(fullContent.replace(/<think>[\s\S]*?<\/think>/g, '').trim());
            var finalThinking = _liteThinkingContent;
            if (!finalThinking) {
                var split = splitLiteThinkContent(fullContent);
                finalThinking = split.thinking;
                finalContent = split.content || fullContent;
            }

            if (thinkUIReady) {
                setTimeout(function() {
                    _liteThinkBlock.classList.add('collapsed');
                    _liteThinkToggle.classList.add('collapsed');
                }, 600);
            }

            if (finalContent) {
                bubble.innerHTML = marked(finalContent);
            } else if (fullContent) {
                bubble.innerHTML = marked(cleanResponseText(fullContent));
            } else {
                bubble.remove();
            }
            time.textContent = new Date().toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' });
        }
        if (fullContent) {
            _liteMessages.push({ role: 'assistant', content: finalContent || fullContent });
            if (finalThinking) {
                _liteMessages.push({ role: 'think', content: finalThinking });
            }
            // 保存到当前会话
            if (_currentSessionId) {
                window.__TAURI__.core.invoke('save_message', { sessionId: _currentSessionId, role: 'user', content: text }).catch(function(){});
                window.__TAURI__.core.invoke('save_message', { sessionId: _currentSessionId, role: 'assistant', content: finalContent || fullContent }).catch(function(){});
                if (finalThinking) {
                    window.__TAURI__.core.invoke('save_message', { sessionId: _currentSessionId, role: 'think', content: finalThinking }).catch(function(){});
                }
            }
        }
        if (_liteUsage && _liteUsage.total_tokens) {
            _liteAccTokens = (_liteAccTokens || 0) + _liteUsage.total_tokens;
            updateLiteCtx();
        }

        // 检查上下文阈值，触发自动总结
        var ctxPct = calcContextPct();
        await checkContextThreshold(ctxPct);

    } catch (e) {
        removeLiteTyping();
        if (e.name === 'AbortError') return;
        addLiteMessage('system', _tp('chat.network_error', { msg: e.message }));
        _liteMessages.pop();
    } finally {
        _liteAbortController = null;
        isLiteProcessing = false;
        updateLiteInputState();
        // 处理延迟总结
        if (_pendingSummary) {
            _pendingSummary = false;
            _ctx80Triggered = false;
            await triggerSummary();
        }
    }
}

/** 深度思考分离：从原生 /completion 内容中尝试分离思考部分（Qwen3 等模型） */
function splitLiteThinkContent(raw) {
    var text = raw;
    // 去掉 "reasoning\n" 前缀（Qwen3 格式）
    if (text.startsWith('reasoning\n')) {
        text = text.slice('reasoning\n'.length);
    }
    // 找第一个 \n\n 作为思考/回答分界
    var idx = text.indexOf('\n\n');
    if (idx > 0 && idx < text.length - 2) {
        var thinking = text.slice(0, idx).trim();
        var content = text.slice(idx + 2).trim();
        if (thinking && content) {
            return { thinking: thinking, content: content };
        }
    }
    // 如果内容较短，不分离
    if (text.length < 20) return { thinking: '', content: raw };
    // 尝试用中文/英文句号分界
    var dots = ['. ', '。', '！', '？', '!\n', '？\n', '。\n'];
    for (var i = 0; i < dots.length; i++) {
        var di = text.indexOf(dots[i]);
        if (di > 10 && di < text.length / 2) {
            var thinking = text.slice(0, di + 1).trim();
            var content = text.slice(di + 1).trim();
            if (thinking && content) {
                return { thinking: thinking, content: content };
            }
        }
    }
    return { thinking: '', content: raw };
}

function clearChat() {
    _liteMessages = [];
    _liteAccTokens = 0;
    updateLiteCtx();
    var container = document.getElementById('liteMessages');
    var msgs = container.querySelectorAll('.chat-msg');
    for (var i = 0; i < msgs.length; i++) msgs[i].remove();
    var welcome = container.querySelector('.welcome-message');
    if (!welcome) {
        var w = document.createElement('div');
        w.className = 'welcome-message';
        w.style.textAlign = 'center';
        w.style.padding = '40px 20px';
        w.innerHTML = '<h2 style="font-size:24px;font-weight:600;margin-bottom:12px;">Soul Agent Lite</h2><p style="color:var(--text-secondary);font-size:14px;">启动服务后即可开始对话</p>';
        container.appendChild(w);
    }
}

document.addEventListener('DOMContentLoaded', function() {
    var input = document.getElementById('userInput');
    if (input) {
        input.addEventListener('keydown', function(e) {
            if (e.key === 'Enter' && !e.shiftKey) {
                e.preventDefault();
                sendLiteMessage();
            }
        });
        input.addEventListener('input', function() {
            input.style.height = 'auto';
            input.style.height = Math.min(input.scrollHeight, 200) + 'px';
        });
    }
    // 初始化深度思考开关（照搬 Soul Agent 的 360° 旋转动画）
    setupLiteThinkingToggle();
    // 初始化工具调用开关
    setupLiteMultimodalToggle();
    // 初始化模型快速切换
    initLiteModelQuickSwitch();
    setInterval(updateLiteInputState, 2000);
    updateLiteCtx();
    setInterval(updateLiteCtx, 10000);
    // 定期同步模型列表
    setInterval(syncLiteModelQuickSwitch, 10000);
});

// ===== 深度思考 & 多模态开关 =====
var _liteThinking = true;   // 深度思考默认开启
var _liteMultimodal = false; // 多模态默认关闭

function setupLiteThinkingToggle() {
    const bar = document.getElementById('liteThinkToggle');
    const icon = bar ? bar.querySelector('.tgl-icon') : null;
    if (!bar || !icon) return;

    bar.classList.toggle('active', _liteThinking);

    bar.onclick = () => {
        const newActive = !bar.classList.contains('active');
        // 每次点击从 0° 转到 360°（通过瞬间复位 + RAF 触发新过渡）
        icon.style.transition = 'none';
        icon.style.transform = 'rotate(0deg)';
        void icon.offsetHeight; // 强制回流确保复位生效
        requestAnimationFrame(() => {
            icon.style.transition = 'transform 0.3s ease';
            icon.style.transform = 'rotate(360deg)';
        });
        bar.classList.toggle('active', newActive);
        _liteThinking = newActive;
    };
}

function setupLiteMultimodalToggle() {
    const bar = document.getElementById('liteMultimodalToggle');
    const icon = bar ? bar.querySelector('.tgl-icon') : null;
    if (!bar || !icon) return;

    bar.classList.toggle('active', _liteMultimodal);

    bar.onclick = () => {
        const newActive = !bar.classList.contains('active');
        icon.style.transition = 'transform 0.15s ease';
        icon.style.transform = 'scale(0.7)';

        setTimeout(() => {
            icon.style.transition = 'none';
            icon.style.transform = 'scale(1)';
            bar.classList.toggle('active', newActive);
            _liteMultimodal = newActive;

            requestAnimationFrame(() => {
                icon.style.transition = 'transform 0.15s ease';
            });
        }, 150);
    };
}

// ===== SA Lite 模型快速切换（与运行模型同步） =====
var _liteSelectedModel = 'default';
var _liteModelPort = null; // 当前选中模型的 proxy 端口

function initLiteModelQuickSwitch() {
    const mqs = document.getElementById('liteModelQuickSwitch');
    const dropdown = document.getElementById('liteMqsDropdown');
    if (!mqs || !dropdown) return;

    mqs.onclick = (e) => {
        e.stopPropagation();
        const isOpen = dropdown.classList.contains('open');
        document.querySelectorAll('.mqs-dropdown.open').forEach(d => d.classList.remove('open'));
        if (!isOpen) {
            syncLiteModelQuickSwitch();
            dropdown.classList.add('open');
        }
    };

    document.addEventListener('click', () => { dropdown.classList.remove('open'); });
}

async function syncLiteModelQuickSwitch() {
    const label = document.getElementById('liteMqsLabel');
    const dropdown = document.getElementById('liteMqsDropdown');
    if (!label || !dropdown) return;

    try {
        var models = await window.__TAURI__.core.invoke('list_running_models');
        if (models && models.length > 0) {
            // 默认选择第一个运行模型
            if (_liteSelectedModel === 'default' || !models.some(function(m) { return m.name === _liteSelectedModel; })) {
                _liteSelectedModel = models[0].name;
            }
            label.textContent = _liteSelectedModel;

            // 更新端口映射
            var selected = models.find(function(m) { return m.name === _liteSelectedModel; });
            _liteModelPort = selected ? selected.proxy_port : null;

            dropdown.innerHTML = models.map(function(m) {
                var active = m.name === _liteSelectedModel ? ' active' : '';
                return '<div class="mqs-option' + active + '" data-value="' + m.name + '" data-port="' + m.proxy_port + '" data-label="' + m.name + '">' + m.name + '</div>';
            }).join('');

            // 绑定选项点击：更新端口
            dropdown.querySelectorAll('.mqs-option').forEach(function(opt) {
                opt.onclick = function(ev) {
                    ev.stopPropagation();
                    _liteSelectedModel = this.dataset.value;
                    _liteModelPort = parseInt(this.dataset.port);
                    document.getElementById('liteMqsLabel').textContent = this.dataset.label || this.dataset.value;
                    dropdown.classList.remove('open');
                };
            });
        } else {
            label.textContent = '未启动';
            dropdown.innerHTML = '<div class="mqs-option" style="color:var(--text-tertiary);cursor:default;">无运行模型</div>';
            _liteModelPort = null;
        }
    } catch (e) {
        console.warn('同步模型列表失败:', e);
    }
}

var _selectedUnloadModels = {};

async function loadRunningModels() {
    try {
        var models = await window.__TAURI__.core.invoke('list_running_models');
        var container = document.getElementById('runningModelsList');
        var empty = document.getElementById('noRunningModels');
        var unloadBtn = document.getElementById('unloadSelectedBtn');
        var card = document.getElementById('runningModelsCard');
        if (!models || models.length === 0) {
            if (empty) empty.style.display = 'block';
            if (container) container.style.display = 'none';
            if (unloadBtn) unloadBtn.style.display = 'none';
            if (card) card.style.display = 'none';
            return;
        }
        if (card) card.style.display = 'block';
        if (empty) empty.style.display = 'none';
        if (container) container.style.display = 'block';
        if (unloadBtn) unloadBtn.style.display = Object.keys(_selectedUnloadModels).length > 0 ? 'inline-flex' : 'none';

        // 计算总显存
        var totalVram = 0;
        models.forEach(function(m) { totalVram += m.vram_mb || 0; });

        container.innerHTML = '<div class="rm-summary">' +
            '<span><strong>' + models.length + '</strong> 个模型运行中</span>' +
            (totalVram > 0 ? '<span class="rm-vram-total">显存: <strong>' + formatVram(totalVram) + '</strong></span>' : '') +
            '</div>';

        models.forEach(function(m) {
            var row = document.createElement('div');
            row.className = 'running-model-row';
            var vramText = m.vram_mb > 0 ? formatVram(m.vram_mb) : '—';
            var statusClass = m.status === 'running' ? 'rm-dot-online' : 'rm-dot-offline';
            var statusText = m.status === 'running' ? '运行中' : '已停止';
            row.innerHTML =
                '<label class="rm-checkbox"><input type="checkbox" data-name="' + m.name.replace(/"/g,'&quot;') + '" onchange="toggleUnloadModel(this)"><span class="rm-checkmark"></span></label>' +
                '<div class="rm-info">' +
                    '<div class="rm-name"><span class="rm-status-dot ' + statusClass + '"></span>' + m.name +
                    ' <span class="rm-mode-badge ' + (m.persistent ? 'badge-persistent' : 'badge-standby') + '">' + (m.persistent ? '常驻' : '待机') + '</span>' +
                    '</div>' +
                    '<div class="rm-meta">端口 <strong>' + m.proxy_port + '</strong> · 上下文 ' + m.ctx + 'K · PID ' + m.pid + ' · ' + m.started_at + '</div>' +
                '</div>' +
                '<div class="rm-vram">' + vramText + '</div>' +
                '<button class="btn btn-sm rm-unload" onclick="unloadSingleModel(\'' + m.name.replace(/'/g,"\\'") + '\')">停止</button>';
            container.appendChild(row);
        });
    } catch (e) { console.warn('加载运行模型失败:', e); }
}

function formatVram(mb) {
    if (mb >= 1024) return (mb / 1024).toFixed(1) + ' GB';
    return mb + ' MB';
}

function toggleUnloadModel(el) {
    if (el.checked) { _selectedUnloadModels[el.dataset.name] = true; }
    else { delete _selectedUnloadModels[el.dataset.name]; }
    var btn = document.getElementById('unloadSelectedBtn');
    var count = Object.keys(_selectedUnloadModels).length;
    if (btn) btn.style.display = count > 0 ? 'inline-flex' : 'none';
}

async function unloadSingleModel(name) {
    if (!confirm('确定要卸载模型「' + name + '」吗？')) return;
    try {
        await window.__TAURI__.core.invoke('unload_model', { modelName: name });
        delete _selectedUnloadModels[name]; loadRunningModels(); pollServerStatus();
    } catch (e) { alert('卸载失败: ' + e); }
}

async function unloadSelectedModels() {
    var names = Object.keys(_selectedUnloadModels);
    if (names.length === 0) return;
    if (!confirm('确定要卸载选中的 ' + names.length + ' 个模型吗？')) return;
    for (var i = 0; i < names.length; i++) {
        try { await window.__TAURI__.core.invoke('unload_model', { modelName: names[i] }); }
        catch (e) { console.warn('卸载 ' + names[i] + ' 失败:', e); }
    }
    _selectedUnloadModels = {}; loadRunningModels(); pollServerStatus();
}

async function unloadAllModels() {
    try {
        var models = await window.__TAURI__.core.invoke('list_running_models');
        if (!models || models.length === 0) return;
        if (!confirm('确定要卸载全部 ' + models.length + ' 个模型吗？')) return;
        for (var i = 0; i < models.length; i++) {
            try { await window.__TAURI__.core.invoke('unload_model', { modelName: models[i].name }); }
            catch (e) { console.warn('卸载 ' + models[i].name + ' 失败:', e); }
        }
        _selectedUnloadModels = {}; loadRunningModels(); pollServerStatus();
    } catch (e) { alert('操作失败: ' + e); }
}

setInterval(loadRunningModels, 5000);

// ============================================================
// 会话管理 (Session Management)
// ============================================================

let _currentSessionId = null;

/// 加载会话列表
async function loadSessionList() {
    try {
        var sessions = await window.__TAURI__.core.invoke('list_sessions');
        var container = document.getElementById('sessionList');
        if (!container) return;

        if (!sessions || sessions.length === 0) {
            container.innerHTML = '<div class="session-empty">' + _t('session.empty') + '</div>';
            return;
        }

        container.innerHTML = sessions.map(function(s) {
            var active = s.id === _currentSessionId ? ' session-item-active' : '';
            return '<div class="session-item' + active + '" onclick="switchSession(\'' + s.id + '\')">' +
                '<div class="session-item-info">' +
                    '<div class="session-item-title">' + escHtml(s.title) + '</div>' +
                    '<div class="session-item-meta">' + _tp('session.messages', { count: s.message_count, time: s.updated_at }) + '</div>' +
                '</div>' +
                '<div class="session-item-actions">' +
                    '<button class="btn btn-sm session-rename-btn" onclick="event.stopPropagation();renameSession(\'' + s.id + '\',\'' + escHtml(s.title).replace(/'/g,"\\'") + '\')">✏️</button>' +
                    '<button class="btn btn-sm session-del-btn" onclick="event.stopPropagation();deleteSession(\'' + s.id + '\')">🗑️</button>' +
                '</div>' +
            '</div>';
        }).join('');
    } catch (e) { console.warn('加载会话失败:', e); }
}

/// 简单的 HTML 转义
function escHtml(s) { return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;'); }

/// 创建新会话
/// 显示聊天欢迎页（新建会话或清空时）
function showChatWelcome() {
    var container = document.getElementById('liteMessages');
    if (!container) return;
    container.innerHTML =
        '<div class="welcome-message" style="text-align:center;padding:60px 20px 40px;">' +
            '<img src="assets/sal-icon.png" width="64" height="64" style="margin-bottom:16px;border-radius:14px;">' +
            '<h2 style="font-size:28px;font-weight:700;margin-bottom:8px;letter-spacing:-0.5px;">' + _t('chat.welcome_title') + '</h2>' +
            '<p style="color:var(--text-secondary);font-size:15px;line-height:1.6;">' + _t('chat.welcome_desc') + '</p>' +
        '</div>';
}

async function createNewSession() {
    try {
        var session = await window.__TAURI__.core.invoke('create_session');
        _currentSessionId = session.id;
        _liteSessionSummary = '';
        _ctx80Triggered = false;
        clearChatMessages();
        showChatWelcome();
        await loadSessionList();
        switchPage('chat');
    } catch (e) { alert('创建会话失败: ' + e); }
}

/// 切换到指定会话
async function switchSession(sessionId) {
    _currentSessionId = sessionId;
    _liteSessionSummary = '';
    _ctx80Triggered = false;
    clearChatMessages();
    try {
        var messages = await window.__TAURI__.core.invoke('load_messages', { sessionId: sessionId });
        messages.forEach(function(m) { addChatMessage(m.role, m.content); });
        // 加载总结
        await loadSessionSummary(sessionId);
    } catch (e) { console.warn('加载消息失败:', e); }
    await loadSessionList();
    switchPage('chat');
}

/// 重命名会话
async function renameSession(sessionId, currentTitle) {
    var newTitle = prompt('输入新名称:', currentTitle);
    if (!newTitle || newTitle === currentTitle) return;
    try {
        await window.__TAURI__.core.invoke('rename_session', { sessionId: sessionId, newTitle: newTitle });
        await loadSessionList();
    } catch (e) { alert('重命名失败: ' + e); }
}

/// 删除会话
async function deleteSession(sessionId) {
    if (!confirm('确定要删除此会话及其所有消息吗？')) return;
    try {
        await window.__TAURI__.core.invoke('delete_session', { sessionId: sessionId });
        await window.__TAURI__.core.invoke('delete_session_summary', { sessionId: sessionId });
        if (_currentSessionId === sessionId) {
            _currentSessionId = null;
            _liteSessionSummary = '';
            clearChatMessages();
        }
        await loadSessionList();
    } catch (e) { alert('删除失败: ' + e); }
}

/// 清空聊天消息
function clearChatMessages() {
    _liteMessages = [];
    var container = document.getElementById('liteMessages');
    if (container) container.innerHTML = '';
}

/// 在聊天中添加消息（供加载历史时复用）
/// 思考块缓存：遇到 think 先存，遇到 assistant 一起渲染
var _pendingThinkContent = null;

function addChatMessage(role, content) {
    // 同步写入 _liteMessages（供 API 请求用）
    _liteMessages.push({ role: role, content: content });

    var container = document.getElementById('liteMessages');
    if (!container) return;

    // 移除欢迎消息
    var welcome = container.querySelector('.welcome-message');
    if (welcome) welcome.remove();

    if (role === 'think') {
        // 缓存思考内容，等 assistant 消息一起渲染
        _pendingThinkContent = content;
        return;
    }

    if (role === 'assistant') {
        // 助手消息：可能携带思考块
        var msgDiv = document.createElement('div');
        msgDiv.className = 'chat-msg assistant';
        msgDiv.style.marginBottom = '8px';

        // 如果有缓存的思考内容，先渲染思考块（在正文上方）
        if (_pendingThinkContent) {
            var toggle = document.createElement('div');
            toggle.className = 'think-toggle';
            toggle.innerHTML = '<img class="think-icon" src="assets/sal-icon.png" width="14" height="14"> 思考过程';

            var block = document.createElement('div');
            block.className = 'think-block';
            var inner = document.createElement('div');
            inner.className = 'think-inner';
            inner.textContent = _pendingThinkContent;
            block.appendChild(inner);

            toggle.onclick = function() {
                var isColl = block.classList.contains('collapsed');
                block.classList.toggle('collapsed', !isColl);
                toggle.classList.toggle('collapsed', !isColl);
            };

            msgDiv.appendChild(toggle);
            msgDiv.appendChild(block);

            // 60ms 后自动折叠
            setTimeout(function() {
                block.classList.add('collapsed');
                toggle.classList.add('collapsed');
            }, 60);

            _pendingThinkContent = null; // 清空缓存
        }

        var bubble = document.createElement('div');
        bubble.className = 'chat-bubble';
        bubble.innerHTML = marked(content);

        var time = document.createElement('div');
        time.className = 'chat-time';
        time.textContent = new Date().toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' });

        msgDiv.appendChild(bubble);
        msgDiv.appendChild(time);
        container.appendChild(msgDiv);
        container.scrollTop = container.scrollHeight;
        return;
    }

    // 用户消息
    var msgDiv = document.createElement('div');
    msgDiv.className = 'chat-msg user';
    msgDiv.style.marginBottom = '8px';

    var bubble = document.createElement('div');
    bubble.className = 'chat-bubble';
    bubble.innerHTML = marked(content);

    var time = document.createElement('div');
    time.className = 'chat-time';
    time.textContent = new Date().toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' });

    msgDiv.appendChild(bubble);
    msgDiv.appendChild(time);
    container.appendChild(msgDiv);
    container.scrollTop = container.scrollHeight;
}
