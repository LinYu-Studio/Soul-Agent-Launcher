// Soul Agent Launcher - 前端逻辑

// ============================================================
// 更新检查
// ============================================================

const UPDATE_API_URL = 'https://sal.bszx.site/api/check-update';
const SKIP_UPDATE_KEY = 'sal-skip-update';

// 版本号唯一出处（需与 src-tauri/tauri.conf.json / Cargo.toml 保持一致）
const APP_VERSION = '0.4.1';
const APP_VERSION_LABEL = 'v' + APP_VERSION + '-Beta';

async function checkUpdate(btn) {
    try {
        var currentVersion = APP_VERSION;
        var resp = await fetch(UPDATE_API_URL + '?current_version=' + currentVersion, {
            signal: AbortSignal.timeout(5000)
        });
        var data = await resp.json();
        if (!data.has_update) {
            if (btn) {
                btn.innerHTML = '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="#4CAF50" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" style="vertical-align:-2px;margin-right:4px;"><polyline points="20 6 9 17 4 12"/></svg> 已是最新版本';
                btn.style.color = '#4CAF50';
                setTimeout(function() {
                    btn.textContent = '检查更新';
                    btn.style.color = '';
                }, 3000);
            }
            return;
        }

        // 检查是否已跳过此版本
        var skipped = localStorage.getItem(SKIP_UPDATE_KEY);
        if (skipped === data.latest_version) {
            if (btn) {
                btn.innerHTML = '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="#4CAF50" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" style="vertical-align:-2px;margin-right:4px;"><polyline points="20 6 9 17 4 12"/></svg> 已是最新版本';
                btn.style.color = '#4CAF50';
                setTimeout(function() {
                    btn.textContent = '检查更新';
                    btn.style.color = '';
                }, 3000);
            }
            return;
        }

        if (btn) { btn.textContent = '检查更新'; btn.style.color = ''; }
        showUpdateCard(data);
    } catch (e) {
        console.warn('检查更新失败:', e);
        if (btn) { btn.textContent = '检查失败'; setTimeout(function() { btn.textContent = '检查更新'; }, 2000); }
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
            '<div class="update-card-icon"><svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"/><polyline points="3.27 6.96 12 12.01 20.73 6.96"/><line x1="12" y1="22.08" x2="12" y2="12"/></svg></div>' +
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
    var icons = { pending: '○', running: '◌', success: '<svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="#4CAF50" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>', skipped: '—', error: '<svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="#E57373" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>' };
    icon.innerHTML = icons[status] || '○';
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
// 模式选择
// ============================================================

var _modeSelected = null;

function selectMode(mode) {
    _modeSelected = mode;
    document.querySelectorAll('.mode-card').forEach(function(el) {
        el.classList.remove('selected');
    });
    var card = document.getElementById(mode === 'dev' ? 'modeDev' : 'modeNewbie');
    if (card) card.classList.add('selected');
}

async function showModeSelection() {
    // 检查是否勾选了"不再提示"
    if (localStorage.getItem('sal-mode-dontask') === 'true') {
        return false;
    }

    var overlay = document.getElementById('modeOverlay');
    if (!overlay) return false;

    _modeSelected = null;
    document.querySelectorAll('.mode-card').forEach(function(el) {
        el.classList.remove('selected');
    });
    document.getElementById('modeDontAsk').checked = false;

    overlay.classList.remove('hidden');

    // 等待用户点击确认
    await new Promise(function(resolve) {
        window._modeResolve = resolve;
    });

    overlay.classList.add('hidden');

    // 保存偏好
    if (document.getElementById('modeDontAsk').checked) {
        localStorage.setItem('sal-mode-dontask', 'true');
    }

    // 保存选择的模式（可后续用于引导逻辑）
    if (_modeSelected) {
        localStorage.setItem('sal-mode', _modeSelected);
    }

    return true;
}

function confirmMode() {
    if (!_modeSelected) return;
    if (window._modeResolve) {
        window._modeResolve();
        window._modeResolve = null;
    }
}

// ============================================================
// 初始化
// ============================================================

/// 启动时恢复上次运行的模型（设置开关：launcher-restore-models，默认开）
async function restoreLastModels() {
    if (localStorage.getItem('launcher-restore-models') === 'false') return;
    var saved = null;
    try { saved = JSON.parse(localStorage.getItem('last-running-models') || '[]'); } catch (e) { return; }
    if (!Array.isArray(saved) || saved.length === 0) return;
    var llamaPath = localStorage.getItem('launcher-llama-path') || '';
    if (!llamaPath) { debugLog('恢复模型跳过：未配置 llama.cpp 路径'); return; }
    var basePort = parseInt(localStorage.getItem('launcher-port') || '20000', 10) || 20000;
    debugLog('开始恢复上次运行的模型，共 ' + saved.length + ' 个');
    var restored = 0;
    for (var i = 0; i < saved.length; i++) {
        var m = saved[i];
        if (!m || !m.model_path || (m.name || '').indexOf('bench-') === 0) continue;
        try {
            await window.__TAURI__.core.invoke('start_model', {
                llamaPath: llamaPath,
                modelPath: m.model_path,
                modelName: m.name || '',
                port: basePort + restored * 2,
                ctx: m.ctx || 4096,
            });
            restored++;
        } catch (e) {
            debugLog('恢复模型失败 ' + (m.name || '') + ': ' + e);
        }
    }
    if (restored > 0) {
        debugLog('已恢复 ' + restored + ' 个模型');
        showKbToast('已自动恢复上次运行的 ' + restored + ' 个模型');
    }
}

document.addEventListener('DOMContentLoaded', async () => {
    debugLog('DOMContentLoaded 开始');

    // 侧边栏版本号（唯一来源 APP_VERSION；只替换首个文本节点，保留跑分徽章）
    var verEl = document.getElementById('sidebarVersion');
    if (verEl && verEl.firstChild) verEl.firstChild.nodeValue = APP_VERSION_LABEL + ' ';

    // 隐私政策检查
    if (!localStorage.getItem('sal-privacy-accepted')) {
        document.getElementById('privacyOverlay')?.classList.remove('hidden');
    }

    // 模式选择弹窗：检查是否需要显示
    var modeShown = await showModeSelection();
    if (modeShown) {
        // 用户已选择模式，继续初始化
    }

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
                if (config.ctx) localStorage.setItem('launcher-ctx', String(config.ctx));
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
    // 在轮询持久化之前读取上次状态并恢复（读操作是同步的，无竞态）
    restoreLastModels();
    startStatusPolling();

    // 延迟 5 秒检查更新
    setTimeout(checkUpdate, 5000);

    listenServerOutput();

    // 新手模式：高亮提示一键部署（闪烁 3 次）
    if (localStorage.getItem('sal-mode') === 'newbie') {
        var deployCard = document.querySelector('.card[onclick*="startAutoDeploy"]');
        if (deployCard) {
            var origBg = deployCard.style.boxShadow;
            for (var f = 0; f < 3; f++) {
                setTimeout(function() {
                    deployCard.style.boxShadow = 'inset 0 0 30px rgba(79,195,247,0.25), 0 0 25px rgba(79,195,247,0.35)';
                    deployCard.style.borderColor = 'var(--accent)';
                    deployCard.style.transition = 'box-shadow 0.1s, border-color 0.1s';
                }, f * 600);
                setTimeout(function() {
                    deployCard.style.boxShadow = '';
                    deployCard.style.borderColor = '';
                }, f * 600 + 250);
            }
        }
    }

    // 加载跑分称号
    loadBenchBadge();

    // 初始化知识库状态
    setTimeout(function() {
        var kbAuto = document.getElementById('kbAutoBtn');
        if (kbAuto) { updateKbAutoUI(); }
        var kbTool = document.getElementById('kbToolBtn');
        if (kbTool) { detectAndShowKbTool(); }
    }, 100);

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

    // 崩溃自动恢复事件提示
    try {
        await window.__TAURI__.event.listen('model-restarted', (event) => {
            const p = event.payload || {};
            addConsoleLine('warn', `⚠️ 模型 ${p.name} 意外退出，已自动重启（第 ${p.crash_count} 次尝试，PID ${p.pid}）`);
            showKbToast(`模型 ${p.name} 意外退出，已自动重启`);
        });
        await window.__TAURI__.event.listen('model-crash-giveup', (event) => {
            const p = event.payload || {};
            addConsoleLine('warn', `❌ 模型 ${p.name} 连续崩溃 ${p.crash_count} 次，已停止自动重启`);
            showKbToast(`模型 ${p.name} 连续崩溃，已停止自动重启`);
        });
        await window.__TAURI__.event.listen('server-restarted', (event) => {
            const p = event.payload || {};
            addConsoleLine('warn', `⚠️ llama-server 意外退出，已自动恢复（PID ${p.pid}）`);
            showKbToast('llama-server 意外退出，已自动恢复');
        });
        await window.__TAURI__.event.listen('server-crash-giveup', (event) => {
            addConsoleLine('warn', '❌ llama-server 连续崩溃，已停止自动恢复');
            showKbToast('llama-server 连续崩溃，已停止自动恢复');
        });
    } catch (e) {
        console.warn('崩溃恢复事件监听失败:', e);
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
            if (config.ctx) localStorage.setItem('launcher-ctx', String(config.ctx));
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
                    if (config2.ctx) localStorage.setItem('launcher-ctx', String(config2.ctx));
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
            // 仍然检查 modelscope（可能在升级后首次运行）
            checkAndInstallModelscope().catch(e => console.warn('modelscope 安装失败:', e));
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
        if (pageId === 'chat') { updateLiteInputState(); detectAndShowKbTool(); }
        if (pageId === 'sessions') { loadSessionList(); }
        if (pageId === 'knowledge') { initKnowledgePage(); }
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

        // 持久化"上次运行的模型"列表（供下次启动恢复；跳过跑分专用实例）
        try {
            var snapshot = (models || [])
                .filter(function(m) { return m.name && m.name.indexOf('bench-') !== 0; })
                .map(function(m) { return { name: m.name, model_path: m.model_path, ctx: m.ctx }; });
            localStorage.setItem('last-running-models', JSON.stringify(snapshot));
        } catch (e) {}

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

// ============================================================
// 隐私政策
// ============================================================

/// 确认隐私政策
function confirmPrivacy() {
    localStorage.setItem('sal-privacy-accepted', '1');
    document.getElementById('privacyOverlay')?.classList.add('hidden');
}

/// 显示使用教程弹窗
function showTutorial() {
    document.getElementById('tutorialOverlay')?.classList.remove('hidden');
}

/// 关闭使用教程弹窗
function hideTutorial() {
    document.getElementById('tutorialOverlay')?.classList.add('hidden');
}

// ============================================================
// AI 性能跑分
// ============================================================

let _benchMode = null;
let _benchSelectedModel = null;
let _benchAbortController = null;

/// 显示跑分模式选择
function showBenchMode() {
    _benchMode = null;
    _benchSelectedModel = null;
    document.querySelectorAll('#benchModeOverlay .mode-card').forEach(function(el) { el.classList.remove('selected'); });
    document.getElementById('benchModeConfirmBtn').disabled = true;
    document.getElementById('benchModeOverlay').classList.remove('hidden');
}

/// 选择跑分模式
function selectBenchMode(mode) {
    _benchMode = mode;
    document.querySelectorAll('#benchModeOverlay .mode-card').forEach(function(el) { el.classList.remove('selected'); });
    document.getElementById(mode === 'quick' ? 'benchQuick' : 'benchCustom').classList.add('selected');
    document.getElementById('benchModeConfirmBtn').disabled = false;
}

function closeBenchMode() {
    document.getElementById('benchModeOverlay').classList.add('hidden');
}

async function confirmBenchMode() {
    if (!_benchMode) return;
    document.getElementById('benchModeOverlay').classList.add('hidden');
    var ok = await benchPreCheck(_benchMode === 'quick' ? 'quick' : 'custom');
    if (!ok) return;
    if (_benchMode === 'quick') {
        await startQuickBenchmark();
    } else {
        showBenchModelList();
    }
}

async function showBenchModelList() {
    var list = document.getElementById('benchModelList');
    var btn = document.getElementById('benchModelConfirmBtn');
    if (!list) return;
    list.innerHTML = '<div style="padding:16px;text-align:center;color:var(--text-tertiary);font-size:13px;">加载中...</div>';
    btn.disabled = true;
    document.getElementById('benchModelOverlay').classList.remove('hidden');
    var modelsDir = localStorage.getItem('launcher-models-dir') || '';
    var localModels = [];
    if (modelsDir) {
        try { localModels = await window.__TAURI__.core.invoke('list_models', { modelsDir: modelsDir }); } catch(e) {}
    }
    var officialModels = _officialModelsCache || [];
    if (officialModels.length === 0) {
        try {
            officialModels = await window.__TAURI__.core.invoke('list_official_models');
            officialModels.forEach(function(m, i) { m._idx = i; });
            _officialModelsCache = officialModels;
        } catch(e) {}
    }
    var ggufOfficial = officialModels.filter(function(m) { return m.quant && m.quant.length > 0; });
    var html = '';
    if (localModels.length > 0) {
        html += '<div style="font-size:12px;font-weight:600;color:var(--text-secondary);margin-bottom:6px;">本地模型</div>';
        html += '<div class="model-check-list" style="max-height:180px;margin-bottom:12px;">';
        localModels.forEach(function(m, i) {
            var safeName = m.name.replace(/'/g,"\\'");
            html += '<label class="win11-check-item" onclick="pickBenchModel(this,\'local\',' + i + ')">' +
                '<input type="radio" name="benchModel">' +
                '<span class="win11-check-fake"></span>' +
                '<span class="win11-check-label">' + m.name + '</span>' +
                '<span class="win11-check-size">' + (m.size || '') + '</span></label>';
        });
        html += '</div>';
    }
    if (ggufOfficial.length > 0) {
        html += '<div style="font-size:12px;font-weight:600;color:var(--text-secondary);margin-bottom:6px;">SAL 官方模型</div>';
        html += '<div class="model-check-list" style="max-height:240px;">';
        ggufOfficial.forEach(function(m, i) {
            var label = m.name || m.model_id;
            var safeLabel = label.replace(/'/g,"\\'");
            html += '<label class="win11-check-item" onclick="pickBenchModel(this,\'official\',' + i + ')">' +
                '<input type="radio" name="benchModel">' +
                '<span class="win11-check-fake"></span>' +
                '<span class="win11-check-label">' + label + '</span>' +
                '<span class="win11-check-size">' + (m.size || '') + '</span></label>';
        });
        html += '</div>';
    }
    if (!html) html = '<div style="padding:16px;text-align:center;color:var(--text-tertiary);">暂无可用模型</div>';
    list.innerHTML = html;
    _benchSelectedModel = null;
}

function pickBenchModel(el, type, idx) {
    document.querySelectorAll('#benchModelList .win11-check-item').forEach(function(c) { c.classList.remove('selected'); });
    el.classList.add('selected');
    el.querySelector('input[type="radio"]').checked = true;
    _benchSelectedModel = { type: type, idx: idx };
    document.getElementById('benchModelConfirmBtn').disabled = false;
}

function closeBenchModel() {
    document.getElementById('benchModelOverlay').classList.add('hidden');
}

/// 跑分前环境检测
async function benchPreCheck(mode) {
    // 1) 检查是否有模型在运行→自动记忆并停止
    try {
        var running = await window.__TAURI__.core.invoke('list_running_models');
        if (running && running.length > 0) {
            _benchSavedRunningModels = running.map(function(m) { return Object.assign({}, m); });
            await window.__TAURI__.core.invoke('stop_all_models');
        } else {
            _benchSavedRunningModels = null;
        }
    } catch(e) {
        _benchSavedRunningModels = null;
    }

    // 2) 检查磁盘空间
    if (mode === 'quick') {
        var ok = await checkDiskSpaceGB(2.5);
        if (!ok) {
            showBenchCheck(
                '磁盘空间不足',
                '快速跑分需要下载两个模型（约 2.2GB），当前磁盘剩余空间可能不够。<br>建议清理磁盘后重试，或选择"自定义跑分"使用已下载的模型。',
                [{ label: '知道了', accent: true, action: function() { hideBenchCheck(); } }]
            );
            return false;
        }
    }

    // 3) 检查是否需要下载
    var needsDownload = (mode === 'quick');
    if (mode === 'custom' && _benchSelectedModel && _benchSelectedModel.type === 'official') {
        var officialModels = _officialModelsCache || [];
        var ggufOfficial = officialModels.filter(function(om) { return om.quant && om.quant.length > 0; });
        var om = ggufOfficial[_benchSelectedModel.idx];
        if (om) {
            var modelsDir = localStorage.getItem('launcher-models-dir') || '';
            if (modelsDir) {
                var localModels = await window.__TAURI__.core.invoke('list_models', { modelsDir: modelsDir });
                needsDownload = !localModels.some(function(lm) { return om.name && lm.name && lm.name.indexOf(om.name) >= 0; });
            }
        }
    }

    return true;
}

var _benchSavedRunningModels = null;

async function restoreBenchModels() {
    var saved = _benchSavedRunningModels;
    _benchSavedRunningModels = null;
    if (!saved || saved.length === 0) return;
    var llamaPath = localStorage.getItem('launcher-llama-path') || '';
    if (!llamaPath) {
        addBenchStatus('[!]未配置 llama.cpp 路径，无法自动恢复模型');
        return;
    }
    addBenchStatus('正在恢复已暂停的模型...');
    for (var i = 0; i < saved.length; i++) {
        var m = saved[i];
        addBenchStatus('恢复 ' + m.name + '...');
        try {
            await window.__TAURI__.core.invoke('start_model', {
                llamaPath: llamaPath,
                modelPath: m.model_path,
                modelName: m.name,
                port: m.proxy_port,
                ctx: m.ctx || 4096,
            });
            addBenchStatus('[OK]' + m.name + ' 已恢复');
        } catch (e) {
            addBenchStatus('[!]' + m.name + ' 恢复失败: ' + e);
        }
    }
    addBenchStatus('[OK]模型自动恢复完成');
}

/// 磁盘空间检查（GB）
async function checkDiskSpaceGB(needGb) {
    try {
        var modelsDir = localStorage.getItem('launcher-models-dir') || '';
        if (!modelsDir) return true;
        var freeBytes = await window.__TAURI__.core.invoke('check_disk_space', { path: modelsDir });
        var freeGb = freeBytes / (1024 * 1024 * 1024);
        return freeGb >= needGb;
    } catch(e) {
        return true; // 无法检测时默认通过
    }
}

/// 显示环境检测弹窗
function showBenchCheck(title, body, actions) {
    document.getElementById('benchCheckTitle').textContent = title;
    document.getElementById('benchCheckBody').innerHTML = body;
    var actionsEl = document.getElementById('benchCheckActions');
    actionsEl.innerHTML = actions.map(function(a) {
        return '<button class="btn ' + (a.accent ? 'btn-accent' : 'btn-danger') + '" onclick="benchCheckAction(' + actions.indexOf(a) + ')">' + a.label + '</button>';
    }).join('');
    window._benchCheckActions = actions;
    document.getElementById('benchCheckOverlay').classList.remove('hidden');
}

function hideBenchCheck() {
    document.getElementById('benchCheckOverlay').classList.add('hidden');
    window._benchCheckActions = null;
}

function benchCheckAction(idx) {
    var actions = window._benchCheckActions;
    if (actions && actions[idx] && actions[idx].action) actions[idx].action();
}

async function confirmBenchModel() {
    if (!_benchSelectedModel) return;
    document.getElementById('benchModelOverlay').classList.add('hidden');
    var ok = await benchPreCheck('custom');
    if (!ok) return;
    var modelsDir = localStorage.getItem('launcher-models-dir') || '';
    if (!modelsDir) { alert('请先在设置中配置模型目录路径'); return; }
    var modelName = '', modelId = '', isThinking = false;
    var promptLen = parseInt(document.getElementById('benchPromptLen')?.value || '200');
    var runCount = parseInt(document.getElementById('benchRunCount')?.value || '3');
    if (_benchSelectedModel.type === 'local') {
        var localModels = await window.__TAURI__.core.invoke('list_models', { modelsDir: modelsDir });
        var m = localModels[_benchSelectedModel.idx];
        if (!m) { alert('模型不存在'); return; }
        modelName = m.name;
        await runSingleBenchmark(modelName, null, false, promptLen, runCount, m.path);
    } else {
        var officialModels = _officialModelsCache || [];
        var ggufOfficial = officialModels.filter(function(om) { return om.quant && om.quant.length > 0; });
        var om = ggufOfficial[_benchSelectedModel.idx];
        if (!om) { alert('模型不存在'); return; }
        modelName = om.name || om.model_id;
        modelId = om.model_id;
        var include = om.include || null;
        var localModels2 = await window.__TAURI__.core.invoke('list_models', { modelsDir: modelsDir });
        var exists = localModels2.some(function(lm) { return modelName && lm.name && lm.name.indexOf(modelName) >= 0; });
        if (!exists) {
            showDownloadOverlay(modelId);
            try {
                await window.__TAURI__.core.invoke('start_model_download', { modelId: modelId, localDir: modelsDir, includePattern: include || '*.gguf' });
                await waitForDownloadComplete();
            } catch(e) { alert('下载失败: ' + e); hideDownloadOverlay(); return; }
        }
        var modelPath = await findModelPath(modelName);
        await runSingleBenchmark(modelName, modelId, false, promptLen, runCount, modelPath);
    }
}

async function startQuickBenchmark() {
    var modelsDir = localStorage.getItem('launcher-models-dir') || '';
    if (!modelsDir) { alert('请先在设置中配置模型目录路径'); return; }
    var quickModels = [
        { modelId: 'Qwen/Qwen2.5-1.5B-Instruct-GGUF:*', name: 'Qwen2.5-1.5B', include: '*.gguf', thinking: false },
        { modelId: 'qwen/Qwen3-1.7B-GGUF:*', name: 'Qwen3-1.7B', include: '*.gguf', thinking: true },
    ];
    openBenchOverlay();
    _benchAbortController = new AbortController();
    if (_benchSavedRunningModels && _benchSavedRunningModels.length > 0) {
        addBenchStatus('自动停止 ' + _benchSavedRunningModels.length + ' 个正在运行的模型');
        addBenchStatus('跑分完成后自动恢复');
    }
    for (var mi = 0; mi < quickModels.length; mi++) {
        if (_benchAbortController.signal.aborted) break;
        var qm = quickModels[mi];

        addBenchStatus('正在下载 ' + qm.name + '...');
        updateBenchPct(((mi + 0.15) / quickModels.length) * 100, '正在跑分中 (' + (mi+1) + '/' + quickModels.length + ') – 下载测试模型...');
        showDownloadOverlay(qm.modelId);
        debugLog('[DL:QUICK] invoke download ' + qm.name).catch(function(){});
        try {
            await window.__TAURI__.core.invoke('start_model_download', { modelId: qm.modelId, localDir: modelsDir, includePattern: qm.include });
            debugLog('[DL:QUICK] invoke done, waitForDownloadComplete').catch(function(){});
            await waitForDownloadComplete();
        } catch(e) { debugLog('[DL:QUICK] FAIL ' + e).catch(function(){}); hideDownloadOverlay(); addBenchStatus('??? ' + qm.name + ' ???'); continue; }

        var modelPath = await findModelPath(qm.name);
        if (!modelPath) { addBenchStatus('[!]未找到 ' + qm.name + ' 文件'); continue; }

        var benchPort = null;
        try {
            benchPort = await startBenchModel(qm.name, modelPath);
        } catch(e) {
            addBenchStatus('[!]启动 ' + qm.name + ' 失败: ' + e);
            continue;
        }

        var label = qm.name + (qm.thinking ? ' (深度思考)' : '');
        var stepLabel = qm.thinking ? '测试复杂推理深度' : '测试通用对话速度';
        updateBenchPct(((mi + 0.5) / quickModels.length) * 100, '正在跑分中 (' + (mi+1) + '/' + quickModels.length + ') – ' + stepLabel + '...');
        addBenchStatus('测试 ' + label + '...');
        var prompt = qm.thinking ? '请详细分析：一个游泳池长50米宽25米深2米，每分钟注水5立方米，多久注满？给出计算过程。' : '请用Python写一个冒泡排序算法并给出使用示例。';
        var result = await runModelBenchmark(qm.name, prompt, qm.thinking, 600, benchPort);
        qm._result = result;
        if (result) {
            updateBenchPct(((mi + 1) / quickModels.length) * 100, '正在跑分中 (' + (mi+1) + '/' + quickModels.length + ') – 完成');
            addBenchStatus(label + ': ' + result.tokS + ' tok/s' + (result.firstTokenLatency ? ', 首Token ' + result.firstTokenLatency + 'ms' : '') + ' (' + result.tokens + ' tokens, ' + result.duration + 's)');
        }
        await stopBenchModel(qm.name);
    }
    updateBenchPct(100);
    addBenchStatus('[OK]快速跑分完成');
    // 保存历史
    var gpuInfo = null;
    try { gpuInfo = await window.__TAURI__.core.invoke('detect_gpu'); } catch(e) {}
    var results = [];
    for (var ri = 0; ri < quickModels.length; ri++) {
        var qr = quickModels[ri];
        if (qr._result) results.push({ name: qr.name + (qr.thinking ? ' (深思考)' : ''), tokS: qr._result.tokS, firstTokenLatency: qr._result.firstTokenLatency, tokens: qr._result.tokens, duration: qr._result.duration });
    }
    var normalSpeed = quickModels[0] && quickModels[0]._result ? parseFloat(quickModels[0]._result.tokS) : 0;
    var thinkSpeed = quickModels[1] && quickModels[1]._result ? parseFloat(quickModels[1]._result.tokS) : 0;
    var score = calcBenchScore(normalSpeed, thinkSpeed);
    if (score.salScore > 0) {
        addBenchStatus('SAL 算力分: ' + score.salScore + ' · 超过 ' + score.percentile + '% 用户');
        saveBenchResult({
            date: new Date().toLocaleString('zh-CN'),
            mode: 'quick', gpu: gpuInfo?.gpu_name || '', cuda_version: gpuInfo?.cuda_version || '',
            vram_mb: gpuInfo?.vram_total_mb || 0, sal_version: APP_VERSION,
            results: results, score: score
        });
        // 称号和首次庆祝
        var bestSpeed = Math.max(normalSpeed, thinkSpeed);
        updateBenchBadge(bestSpeed);
        setTimeout(function() { showFirstBenchCelebration(bestSpeed); }, 1000);
    }
    await restoreBenchModels();
    finishBenchOverlay();
}

/// 等待下载完成（轮询 _downloadDone 标志）
function waitForDownloadComplete() {
    debugLog('[DL:WAIT] start').catch(function(){});
    return new Promise(function(resolve, reject) {
        var maxWait = 600000;
        var start = Date.now();
        var checkNum = 0;
        function check() {
            checkNum++;
            if (_downloadDone) { debugLog('[DL:WAIT] done after ' + checkNum).catch(function(){}); _downloadDone = false; resolve(); return; }
            if (_downloadError) { debugLog('[DL:WAIT] error ' + _downloadError).catch(function(){}); var e = new Error(_downloadError); _downloadError = null; reject(e); return; }
            if (Date.now() - start > maxWait) { debugLog('[DL:WAIT] timeout').catch(function(){}); reject(new Error('下载超时')); return; }
            if (checkNum % 10 === 1) debugLog('[DL:WAIT] check#' + checkNum).catch(function(){});
            setTimeout(check, 300);
        }
        check();
    });
}

async function runSingleBenchmark(modelName, modelId, isThinking, promptLen, runCount, modelPath) {
    openBenchOverlay();
    _benchAbortController = new AbortController();
    if (_benchSavedRunningModels && _benchSavedRunningModels.length > 0) {
        addBenchStatus('自动停止 ' + _benchSavedRunningModels.length + ' 个正在运行的模型');
        addBenchStatus('跑分完成后自动恢复');
    }
    var benchPort = null;
    if (modelPath) {
        try {
            benchPort = await startBenchModel(modelName, modelPath);
        } catch(e) {
            addBenchStatus('[!]自动启动 ' + modelName + ' 失败: ' + e);
            finishBenchOverlay();
            return;
        }
    }
    var stepLabel = isThinking ? '测试复杂推理深度' : '测试通用对话速度';
    var prompt = buildBenchPrompt(isThinking, promptLen || 200);
    var maxTok = (promptLen || 200) * 3;
    runCount = runCount || 3;

    var results = [];
    for (var ri = 0; ri < runCount; ri++) {
        if (_benchAbortController.signal.aborted) break;
        updateBenchPct((ri / runCount) * 100, '正在跑分中 (' + (ri+1) + '/' + runCount + ') – ' + stepLabel + '...');
        addBenchStatus('第 ' + (ri+1) + ' 次测试...');
        var result = await runModelBenchmark(modelName, prompt, isThinking, maxTok, benchPort);
        if (result) results.push(result);
    }

    if (results.length > 0 && !_benchAbortController.signal.aborted) {
        var avgTokS, avgFirstLat, avgTokens, avgDuration;
        if (results.length >= 3) {
            var sorted = results.slice().sort(function(a, b) { return parseFloat(a.tokS) - parseFloat(b.tokS); });
            var middle = sorted.slice(1, -1);
            avgTokS = middle.reduce(function(s, r) { return s + parseFloat(r.tokS); }, 0) / middle.length;
            avgFirstLat = Math.round(middle.reduce(function(s, r) { return s + (r.firstTokenLatency || 0); }, 0) / middle.length);
            avgTokens = Math.round(middle.reduce(function(s, r) { return s + r.tokens; }, 0) / middle.length);
            avgDuration = middle.reduce(function(s, r) { return s + parseFloat(r.duration); }, 0) / middle.length;
        } else {
            avgTokS = results.reduce(function(s, r) { return s + parseFloat(r.tokS); }, 0) / results.length;
            avgFirstLat = Math.round(results.reduce(function(s, r) { return s + (r.firstTokenLatency || 0); }, 0) / results.length);
            avgTokens = Math.round(results.reduce(function(s, r) { return s + r.tokens; }, 0) / results.length);
            avgDuration = results.reduce(function(s, r) { return s + parseFloat(r.duration); }, 0) / results.length;
        }

        var avgResult = { tokS: avgTokS.toFixed(1), firstTokenLatency: avgFirstLat, tokens: avgTokens, duration: avgDuration.toFixed(2) };
        updateBenchPct(100, '跑分完成');
        addBenchStatus('[OK]' + modelName + ' 平均: ' + avgResult.tokS + ' tok/s' + (avgResult.firstTokenLatency ? ', 首Token ' + avgResult.firstTokenLatency + 'ms' : '') + ' (' + runCount + ' 次测试)');

        // 与快速跑分对比
        var quickHistory = loadBenchHistory().filter(function(h) { return h.mode === 'quick' && h.results && h.results.length > 0; });
        if (quickHistory.length > 0) {
            var quick = quickHistory[0];
            addBenchStatus('── 与快速跑分对比 ──');
            addBenchStatus('SAL 算力分: ' + (quick.score ? quick.score.salScore : 'N/A') + ' · 超过 ' + (quick.score ? quick.score.percentile : 'N/A') + '% 用户');
            if (quick.results) {
                quick.results.forEach(function(qr) {
                    addBenchStatus('  ' + qr.name + ': ' + qr.tokS + ' tok/s');
                });
            }
        }

        // 保存历史
        var gpuInfo = null;
        try { gpuInfo = await window.__TAURI__.core.invoke('detect_gpu'); } catch(e) {}
        var score = calcBenchScore(isThinking ? 0 : avgTokS, isThinking ? avgTokS : 0);
        if (score.salScore > 0) {
            addBenchStatus('SAL 算力分: ' + score.salScore + ' · 超过 ' + score.percentile + '% 用户');
            saveBenchResult({
                date: new Date().toLocaleString('zh-CN'), mode: 'custom',
                gpu: gpuInfo?.gpu_name || '', cuda_version: gpuInfo?.cuda_version || '',
                vram_mb: gpuInfo?.vram_total_mb || 0, sal_version: APP_VERSION,
                results: [{ name: modelName, tokS: avgResult.tokS, firstTokenLatency: avgResult.firstTokenLatency, tokens: avgResult.tokens, duration: avgResult.duration, runs: runCount }],
                score: score,
                prompt_len: promptLen, run_count: runCount
            });
            var bestSpeed = Math.max(isThinking ? avgTokS : 0, isThinking ? 0 : avgTokS);
            updateBenchBadge(bestSpeed || avgTokS);
            setTimeout(function() { showFirstBenchCelebration(bestSpeed || avgTokS); }, 1000);
        }
    } else if (results.length === 0) {
        addBenchStatus('[!]未获取到有效测试数据');
    }
    if (benchPort) await stopBenchModel(modelName);
    await restoreBenchModels();
    finishBenchOverlay();
}

/// 跑分专用：启动模型并返回端口
async function startBenchModel(modelName, modelPath) {
    var llamaPath = localStorage.getItem('launcher-llama-path') || '';
    if (!llamaPath) throw new Error('未配置 llama.cpp 路径');
    var port = 20201;
    var ctx = parseInt(localStorage.getItem('launcher-ctx') || '4096');
    addBenchStatus('启动 ' + modelName + '...');
    await window.__TAURI__.core.invoke('start_model', {
        llamaPath: llamaPath,
        modelPath: modelPath,
        modelName: 'bench-' + modelName,
        port: port,
        ctx: ctx,
    });
    await new Promise(function(r) { setTimeout(r, 3000); });
    return port;
}

/// 跑分专用：停止跑分模型
async function stopBenchModel(modelName) {
    try {
        await window.__TAURI__.core.invoke('unload_model', { modelName: 'bench-' + modelName });
        addBenchStatus('已停止 ' + modelName);
    } catch(e) {
        addBenchStatus('[!]停止 ' + modelName + ' 失败: ' + e);
    }
}

/// 查找已下载的 GGUF 文件路径
async function findModelPath(modelName) {
    var modelsDir = localStorage.getItem('launcher-models-dir') || '';
    if (!modelsDir) return null;
    try {
        var localModels = await window.__TAURI__.core.invoke('list_models', { modelsDir: modelsDir });
        for (var i = 0; i < localModels.length; i++) {
            if (localModels[i].name && localModels[i].name.indexOf(modelName) >= 0) {
                return localModels[i].path;
            }
        }
    } catch(e) {}
    return null;
}

function buildBenchPrompt(isThinking, tokenCount) {
    if (isThinking) {
        var m = {
            50: '计算：2+2×3-1=?',
            200: '一个游泳池长50米宽25米深2米，每分钟注水5立方米，多久注满？请给出计算过程。',
            500: '请详细分析，分步骤推理。一个游泳池长50米宽25米深2米。以每分钟5立方米注水。\n(1) 多久注满？\n(2) 同时排水 2m³/min 需要多久？\n(3) 注到 80% 需要多久？\n列出所有计算过程。'
        };
        return m[tokenCount] || m[200];
    }
    var m = {
        50: 'Python 冒泡排序代码？',
        200: '请用Python写一个冒泡排序算法并给出使用示例。',
        500: '用Python实现冒泡排序：\n(1) 标准版\n(2) 提前退出优化版\n(3) 双向冒泡排序\n每个需注释和示例。'
    };
    return m[tokenCount] || m[200];
}

/// 跑分称号
function calcBenchTitle(tokS) {
    var speed = parseFloat(tokS) || 0;
    if (speed > 120) return '光速仲裁者';
    if (speed >= 80) return '零延迟幽灵';
    if (speed >= 50) return '过载核心';
    if (speed >= 30) return '张量洪流';
    if (speed >= 15) return '浮点先锋';
    return '算力残影';
}

/// 检查并更新称号显示
function updateBenchBadge(tokS) {
    var title = calcBenchTitle(tokS);
    var old = localStorage.getItem('sal-bench-title');
    var badgeEl = document.getElementById('sidebarBadge');
    if (badgeEl) badgeEl.textContent = title;
    // 如果新称号比旧的好就更新
    var rank = ['算力残影','浮点先锋','张量洪流','过载核心','零延迟幽灵','光速仲裁者'];
    if (!old || rank.indexOf(title) > rank.indexOf(old)) {
        localStorage.setItem('sal-bench-title', title);
        if (badgeEl) badgeEl.style.display = '';
    }
}

/// 加载称号显示
function loadBenchBadge() {
    var title = localStorage.getItem('sal-bench-title');
    var badgeEl = document.getElementById('sidebarBadge');
    if (badgeEl && title) {
        badgeEl.textContent = title;
        badgeEl.style.display = '';
    }
}

/// 首次跑分完成庆祝
async function showFirstBenchCelebration(tokS) {
    var title = calcBenchTitle(tokS);
    var isNew = !localStorage.getItem('sal-bench-title');
    if (!isNew) return;

    // 推荐最适合的模型
    var gpuInfo = null;
    try { gpuInfo = await window.__TAURI__.core.invoke('detect_gpu'); } catch(e) {}
    var vramMB = gpuInfo?.vram_total_mb || 0;
    var models = _officialModelsCache || [];
    if (models.length === 0) {
        try {
            models = await window.__TAURI__.core.invoke('list_official_models');
            models.forEach(function(m, i) { m._idx = i; });
            _officialModelsCache = models;
        } catch(e) {}
    }
    var recs = recommendModels(gpuInfo, vramMB, models);
    var recName = recs.length > 0 ? (recs[0].name || recs[0].model_id) : '';

    var overlay = document.createElement('div');
    overlay.className = 'download-overlay';
    overlay.id = 'celebrationOverlay';
    overlay.style.cssText = 'z-index:99999;';
    overlay.innerHTML =
        '<div class="download-box" style="text-align:center;max-width:480px;animation:celebratePop 0.4s ease;">' +
            '<div style="margin-bottom:12px;animation:celebrateBounce 1s ease infinite;">' +
            '<svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="#FFD700" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"/></svg>' +
            '</div>' +
            '<h3 style="margin-bottom:8px;">完成 AI 性能跑分！</h3>' +
            '<div style="font-size:13px;color:var(--text-secondary);line-height:1.8;margin-bottom:12px;">' +
                '您的设备获得称号：<strong style="color:var(--accent);font-size:18px;">' + title + '</strong><br>' +
                (recName ? '推荐您下载 <strong>' + recName + '</strong> 以获得最佳体验。' : '') +
            '</div>' +
            '<div class="card-actions" style="justify-content:center;">' +
                (recs.length > 0 ? '<button class="btn btn-accent" onclick="confirmCelebrationDeploy()" style="min-width:100px;">确认下载</button>' : '') +
                '<button class="btn btn-danger" onclick="closeCelebration()">取消</button>' +
            '</div>' +
        '</div>';
    document.body.appendChild(overlay);

    // 缓存推荐用于确认
    window._celebRecs = recs;
    window._celebOverlay = overlay;
}

function confirmCelebrationDeploy() {
    closeCelebration();
    if (window._celebRecs && window._celebRecs.length > 0) {
        setTimeout(function() { startAutoDeploy(); }, 300);
    }
}

function closeCelebration() {
    var overlay = document.getElementById('celebrationOverlay');
    if (overlay) overlay.remove();
    window._celebRecs = null;
    window._celebOverlay = null;
}

/// 预览测试 Prompt
function previewBenchPrompt() {
    var len = parseInt(document.getElementById('benchPromptLen')?.value || '200');
    var isThinking = false; // 默认非思考模式预览
    var preview = document.getElementById('benchPromptPreview');
    if (preview) preview.textContent = buildBenchPrompt(isThinking, len);
}

function toggleBenchPromptPreview() {
    var preview = document.getElementById('benchPromptPreview');
    var toggle = document.getElementById('benchPromptToggle');
    if (!preview) return;
    var isHidden = preview.style.display === 'none';
    preview.style.display = isHidden ? 'block' : 'none';
    if (toggle) toggle.textContent = isHidden ? '收起' : '展开';
    if (isHidden) previewBenchPrompt();
}

/// 一键反馈（打包跑分数据）
async function feedbackBenchmark() {
    if (!confirm('将打包以下数据用于反馈：\n• 硬件信息（GPU、显存）\n• 最近跑分结果\n• SAL 版本\n\n点击确定后数据将被复制到剪贴板，可粘贴发送给开发者。')) return;
    var lines = ['SAL 跑分反馈', '============', ''];
    lines.push('SAL 版本: ' + APP_VERSION);
    try {
        var gpuInfo = await window.__TAURI__.core.invoke('detect_gpu');
        lines.push('GPU: ' + (gpuInfo.gpu_name || 'N/A'));
        lines.push('CUDA: ' + (gpuInfo.cuda_version || 'N/A'));
        lines.push('显存: ' + (gpuInfo.vram_total_mb ? Math.round(gpuInfo.vram_total_mb/1024) + ' GB' : 'N/A'));
    } catch(e) { lines.push('GPU: 无法检测'); }
    lines.push('');
    var history = loadBenchHistory();
    if (history.length > 0) {
        lines.push('最近跑分记录:');
        history.slice(0, 3).forEach(function(h, i) {
            lines.push('  [' + (i+1) + '] ' + (h.date || '') + ' - ' + (h.mode || '') + ' - GPU: ' + (h.gpu || '') + ' - SAL算力分: ' + (h.score ? h.score.salScore : 'N/A'));
            if (h.results) h.results.forEach(function(r) { lines.push('      ' + r.name + ': ' + r.tokS + ' tok/s'); });
        });
    } else {
        lines.push('暂无跑分记录');
    }
    lines.push('');
    lines.push('反馈说明:');
    lines.push('请在此描述你遇到的问题：');
    lines.push('');
    lines.push('--- 以上数据复制到剪贴板 ---');

    try {
        await navigator.clipboard.writeText(lines.join('\n'));
        showBenchCheck('反馈数据已复制', '已打包硬件信息和跑分数据到剪贴板。<br><br>请粘贴发送给开发者，并附上问题描述。',
            [{ label: '确定', accent: true, action: function() { hideBenchCheck(); } }]);
    } catch(e) {
        showBenchCheck('复制失败', '无法访问剪贴板: ' + e,
            [{ label: '确定', accent: true, action: function() { hideBenchCheck(); } }]);
    }
}

async function runModelBenchmark(modelName, prompt, thinking, maxTokens, benchPort) {
    try {
        var port = benchPort || parseInt(document.getElementById('launchPort')?.value || localStorage.getItem('launcher-port') || '20000');
        var resp = await fetch('http://127.0.0.1:' + port + '/chat', {
            method: 'POST', headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ model: modelName, messages: [{ role: 'user', content: prompt }], stream: true, thinking: !!thinking, max_tokens: maxTokens || 600 }),
            signal: _benchAbortController ? _benchAbortController.signal : null,
        });
        if (!resp.ok) {
            var errText = '';
            try { errText = await resp.text(); } catch(e) {}
            var suggestion = '';
            if (resp.status === 500 && (errText.indexOf('out of memory') >= 0 || errText.indexOf('OOM') >= 0 || errText.indexOf('CUDA') >= 0)) {
                suggestion = '显存不足，建议使用更小的量化版本（如 Q4→Q2）或降低上下文长度';
            } else if (resp.status === 500 && errText.indexOf('Killed') >= 0) {
                suggestion = '进程被系统终止，可能是内存不足，建议关闭其他程序或使用更小的模型';
            } else if (resp.status === 404) {
                suggestion = '模型未加载或已卸载，请先在"启动"页启动该模型';
            } else if (resp.status === 503 || resp.status === 502) {
                suggestion = '服务暂不可用，可能是模型正在加载或初始化，请稍后重试';
            } else {
                suggestion = '请检查模型是否已启动，或尝试在"启动"页重新启动';
            }
            addBenchStatus('[!]' + modelName + ' 请求失败 (HTTP ' + resp.status + ')');
            addBenchStatus('<svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="#FFB74D" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" style="vertical-align:-2px;margin-right:2px;"><path d="M9 18h6"/><path d="M10 22h4"/><path d="M15.09 14c.18-.98.65-1.74 1.41-2.5A4.65 4.65 0 0 0 18 8 6 6 0 0 0 6 8c0 1 .23 2.23 1.5 3.5A4.61 4.61 0 0 1 8.91 14"/></svg>' + suggestion);
            return null;
        }
        var reader = resp.body.getReader();
        var decoder = new TextDecoder();
        var fullContent = '';
        var firstToken = true, firstTokenTime = 0, startTime = Date.now();
        while (true) {
            var r = await reader.read();
            if (r.done) break;
            var chunk = decoder.decode(r.value, { stream: true });
            var lines = chunk.split('\n');
            for (var j = 0; j < lines.length; j++) {
                var line = lines[j].trim();
                if (!line || !line.startsWith('data: ')) continue;
                var data = line.slice(6).trim();
                if (data === '[DONE]') continue;
                try { var json = JSON.parse(data); var delta = json?.choices?.[0]?.delta?.content || json?.content || ''; if (delta) { if (firstToken) { firstToken = false; firstTokenTime = Date.now(); } fullContent += delta; } } catch(e) {}
            }
        }
        var endTime = Date.now();
        var duration = (endTime - startTime) / 1000;
        var tokens = Math.ceil(fullContent.length / 2);

        // 速度异常提醒
        var tokS = duration > 0 ? (tokens / duration).toFixed(1) : '0';
        if (parseFloat(tokS) < 2 && tokens > 10) {
            addBenchStatus('[!]速度偏低 (' + tokS + ' tok/s)，是否在 CPU 上运行？建议启用 GPU 加速或使用更小的模型');
        }

        return { tokens: tokens, duration: duration.toFixed(1), tokS: tokS, firstTokenLatency: firstTokenTime > 0 ? (firstTokenTime - startTime) : null };
    } catch (e) {
        if (e.name === 'AbortError') {
            addBenchStatus('[STOP] 测试已取消');
        } else if (e.message && e.message.indexOf('Failed to fetch') >= 0) {
            addBenchStatus('[!]无法连接到服务 (端口 ' + port + ')');
            addBenchStatus('<svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="#FFB74D" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" style="vertical-align:-2px;margin-right:2px;"><path d="M9 18h6"/><path d="M10 22h4"/><path d="M15.09 14c.18-.98.65-1.74 1.41-2.5A4.65 4.65 0 0 0 18 8 6 6 0 0 0 6 8c0 1 .23 2.23 1.5 3.5A4.61 4.61 0 0 1 8.91 14"/></svg>请先启动模型服务，然后在"启动"页确认服务状态为"已连接"');
        } else if (e.message && e.message.indexOf('NetworkError') >= 0) {
            addBenchStatus('[!]网络连接失败');
            addBenchStatus('<svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="#FFB74D" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" style="vertical-align:-2px;margin-right:2px;"><path d="M9 18h6"/><path d="M10 22h4"/><path d="M15.09 14c.18-.98.65-1.74 1.41-2.5A4.65 4.65 0 0 0 18 8 6 6 0 0 0 6 8c0 1 .23 2.23 1.5 3.5A4.61 4.61 0 0 1 8.91 14"/></svg>请确认服务已启动且端口 ' + port + ' 未被占用');
        } else if (e.message && (e.message.indexOf('timeout') >= 0 || e.message.indexOf('abort') >= 0)) {
            addBenchStatus('[!]请求超时，模型响应过慢');
            addBenchStatus('<svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="#FFB74D" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" style="vertical-align:-2px;margin-right:2px;"><path d="M9 18h6"/><path d="M10 22h4"/><path d="M15.09 14c.18-.98.65-1.74 1.41-2.5A4.65 4.65 0 0 0 18 8 6 6 0 0 0 6 8c0 1 .23 2.23 1.5 3.5A4.61 4.61 0 0 1 8.91 14"/></svg>可尝试降低 prompt 长度或使用更小/更快的模型');
        } else {
            addBenchStatus('[!]测试失败: ' + (e.message || e));
        }
        return null;
    }
}

function openBenchOverlay() {
    document.getElementById('benchResults').innerHTML = '';
    document.getElementById('benchProgressFill').style.width = '0%';
    document.getElementById('benchRunningOverlay').classList.remove('hidden');
    document.getElementById('benchCancelBtn').textContent = '取消测试';
    document.getElementById('benchCancelBtn').onclick = function() { cancelBenchmark(); };
}

function finishBenchOverlay() {
    document.getElementById('benchCancelBtn').textContent = '完成';
    document.getElementById('benchCancelBtn').onclick = function() { closeBenchmark(); };
    var actions = document.getElementById('benchRunningOverlay').querySelector('.card-actions');
    if (actions) {
        if (!document.getElementById('benchHistoryBtn')) {
            var historyBtn = document.createElement('button');
            historyBtn.id = 'benchHistoryBtn';
            historyBtn.className = 'btn btn-sm';
            historyBtn.textContent = '历史记录';
            historyBtn.onclick = function() { showBenchHistory(); };
            historyBtn.style.cssText = 'font-size:11px;padding:4px 12px;';
            actions.parentNode.insertBefore(historyBtn, actions);
        }
        if (!document.getElementById('benchFeedbackBtn')) {
            var fbBtn = document.createElement('button');
            fbBtn.id = 'benchFeedbackBtn';
            fbBtn.className = 'btn btn-sm';
            fbBtn.textContent = '反馈';
            fbBtn.onclick = function() { feedbackBenchmark(); };
            fbBtn.style.cssText = 'font-size:11px;padding:4px 12px;';
            actions.parentNode.insertBefore(fbBtn, actions);
        }
    }
}

function addBenchStatus(line) {
    var el = document.getElementById('benchResults');
    if (!el) return;
    var div = document.createElement('div');
    div.style.cssText = 'padding:2px 0;font-size:12px;color:var(--text-secondary);';
    div.textContent = line;
    el.appendChild(div);
}

function updateBenchPct(pct, status) {
    var fill = document.getElementById('benchProgressFill');
    if (fill) fill.style.width = Math.min(pct, 100) + '%';
    var text = document.getElementById('benchStatusText');
    if (text) text.textContent = status || ('进度 ' + Math.round(pct) + '%');
}

function cancelBenchmark() {
    if (_benchAbortController) _benchAbortController.abort();
    addBenchStatus('[STOP] 跑分已取消（已下载的模型可复用）');
    finishBenchOverlay();
}

function closeBenchmark() {
    _benchAbortController = null;
    document.getElementById('benchRunningOverlay').classList.add('hidden');
    var dot = document.querySelector('.card:nth-child(3) .status-dot');
    var label = document.querySelector('.card:nth-child(3) .status-label');
    if (dot) dot.style.color = 'var(--accent)';
    if (label) { label.style.color = 'var(--accent)'; label.textContent = '已测试'; }
}

// ============================================================
// 跑分历史与 TOPS 排行
// ============================================================

var BENCH_TOPS_TABLE = [
    { tops: 3352.0, pct: 99.9, gpu: 'RTX 5090' },
    { tops: 1801.0, pct: 99.5, gpu: 'RTX 5080' },
    { tops: 1321.0, pct: 98.5, gpu: 'RTX 4090' },
    { tops: 836.0,  pct: 94.0, gpu: 'RTX 4080 SUPER' },
    { tops: 780.0,  pct: 92.5, gpu: 'RTX 4080' },
    { tops: 766.0,  pct: 91.5, gpu: 'RTX 5070 Ti' },
    { tops: 706.0,  pct: 89.0, gpu: 'RTX 4070 Ti SUPER' },
    { tops: 641.0,  pct: 86.0, gpu: 'RTX 4070 Ti' },
    { tops: 568.0,  pct: 82.0, gpu: 'RTX 4070 SUPER' },
    { tops: 466.0,  pct: 73.0, gpu: 'RTX 4070' },
    { tops: 370.0,  pct: 62.5, gpu: 'RTX 4060 Ti 16G' },
    { tops: 357.0,  pct: 60.5, gpu: 'RTX 4060 Ti 8G' },
    { tops: 242.0,  pct: 44.0, gpu: 'RTX 4060' },
    { tops: 150.0,  pct: 24.0, gpu: 'RTX 3060' },
    { tops: 100.0,  pct: 14.0, gpu: 'RTX 3050 / GTX 1660 Ti' },
    { tops: 80.0,   pct: 9.0,  gpu: 'GTX 1660 / 入门级' },
    { tops: 50.0,   pct: 4.0,  gpu: 'GTX 1050 Ti 等老旧显卡' },
    { tops: 20.0,   pct: 1.5,  gpu: '核显/极低端' },
];

function calcBenchScore(speedNormal, speedThink) {
    var composite = (speedNormal || 0) * 0.4 + (speedThink || 0) * 0.6;
    var salScore = composite * 36.8;
    var pct = 0;
    for (var i = 0; i < BENCH_TOPS_TABLE.length; i++) {
        if (salScore >= BENCH_TOPS_TABLE[i].tops) { pct = BENCH_TOPS_TABLE[i].pct; break; }
    }
    return { composite: Math.round(composite * 10) / 10, salScore: Math.round(salScore * 10) / 10, percentile: pct };
}

function saveBenchResult(data) {
    var history = JSON.parse(localStorage.getItem('sal-bench-history') || '[]');
    history.unshift(data);
    if (history.length > 50) history.length = 50;
    localStorage.setItem('sal-bench-history', JSON.stringify(history));
}

function loadBenchHistory() { return JSON.parse(localStorage.getItem('sal-bench-history') || '[]'); }

function showBenchHistory() {
    var history = loadBenchHistory();
    var html = '';
    if (history.length === 0) {
        html = '<div style="padding:20px;text-align:center;color:var(--text-tertiary);">暂无跑分记录</div>';
    } else {
        html += '<div style="font-size:11px;color:var(--text-tertiary);margin-bottom:8px;">共 ' + history.length + ' 条记录</div>';
        history.forEach(function(h, i) {
            var s = h.score || {};
            html += '<div class="session-item" style="cursor:default;padding:8px 10px;margin-bottom:6px;border:1px solid var(--border-subtle);border-radius:6px;">' +
                '<div style="display:flex;justify-content:space-between;align-items:center;">' +
                    '<div><strong style="font-size:13px;">' + escHtml(h.date || '') + '</strong></div>' +
                    '<div style="font-size:11px;color:var(--text-tertiary);">' + escHtml(h.gpu || '') + '</div>' +
                '</div>';
            if (s.percentile > 0) {
                html += '<div style="font-size:12px;color:var(--accent);margin-top:4px;">SAL 算力分 ' + s.salScore + ' · 超过 ' + s.percentile + '% 用户 <span style="font-size:10px;color:var(--text-tertiary);">（基于公开 TOPS 数据估算）</span></div>';
            }
            if (h.results) {
                h.results.forEach(function(r) {
                    html += '<div style="font-size:11px;color:var(--text-secondary);margin-top:2px;">' + escHtml(r.name) + ': ' + r.tokS + ' tok/s' + (r.firstTokenLatency ? ', 首Token ' + r.firstTokenLatency + 'ms' : '') + '</div>';
                });
            }
            html += '<div style="margin-top:6px;display:flex;gap:6px;">' +
                '<button class="btn btn-sm" onclick="exportBenchResult(' + i + ')" style="font-size:10px;padding:2px 8px;">导出</button>' +
                '<button class="btn btn-sm btn-danger" onclick="deleteBenchResult(' + i + ')" style="font-size:10px;padding:2px 8px;">删除</button></div>';
            html += '</div>';
        });
    }
    showBenchCheck('跑分历史', html, [{ label: '关闭', accent: true, action: function() { hideBenchCheck(); } }]);
}

function deleteBenchResult(idx) {
    var history = loadBenchHistory();
    history.splice(idx, 1);
    localStorage.setItem('sal-bench-history', JSON.stringify(history));
    showBenchHistory();
}

function exportBenchResult(idx) {
    var history = loadBenchHistory();
    var h = history[idx];
    if (!h) return;
    var text = 'SAL 性能跑分报告\n================\n';
    text += '日期: ' + (h.date || '') + '\n' + 'SAL 版本: ' + (h.sal_version || '') + '\n';
    text += 'GPU: ' + (h.gpu || '') + '\n' + 'CUDA: ' + (h.cuda_version || '') + '\n';
    text += '显存: ' + (h.vram_mb ? Math.round(h.vram_mb/1024) + ' GB' : '') + '\n\n';
    if (h.score) {
        text += '综合速度: ' + h.score.composite + ' tok/s\nSAL 算力分: ' + h.score.salScore + '\n';
        text += '超过 ' + h.score.percentile + '% 用户（基于公开 TOPS 数据估算）\n\n';
    }
    text += '测试结果:\n';
    if (h.results) {
        h.results.forEach(function(r) {
            text += '  ' + r.name + ': ' + r.tokS + ' tok/s';
            if (r.firstTokenLatency) text += ', 首Token ' + r.firstTokenLatency + 'ms';
            text += ' (' + r.tokens + ' tokens, ' + r.duration + 's)\n';
        });
    }
    try {
        navigator.clipboard.writeText(text);
        showBenchCheck('导出完成', '报告已复制到剪贴板，可在论坛或反馈中粘贴分享。',
            [{ label: '确定', accent: true, action: function() { hideBenchCheck(); } }]);
    } catch(e) {
        showBenchCheck('导出失败', '无法访问剪贴板: ' + e,
            [{ label: '确定', accent: true, action: function() { hideBenchCheck(); } }]);
    }
}

// ============================================================
// 快速启动弹窗
// ============================================================

/// 显示快速启动弹窗
async function showQuickStart() {
    var overlay = document.getElementById('quickStartOverlay');
    var list = document.getElementById('quickModelCheckList');
    if (!overlay || !list) return;

    overlay.classList.remove('hidden');
    list.innerHTML = '<div class="model-check-empty">正在加载模型列表...</div>';

    var modelsDir = localStorage.getItem('launcher-models-dir') || '';
    if (!modelsDir) {
        list.innerHTML = '<div class="model-check-empty">请先在设置中配置模型目录路径</div>';
        return;
    }

    try {
        var models = await window.__TAURI__.core.invoke('list_models', { modelsDir: modelsDir });
        if (models.length === 0) {
            list.innerHTML = '<div class="model-check-empty">未找到 GGUF 模型，请先下载</div>';
        } else {
            list.innerHTML = models.map(function(m, i) {
                return '<label class="win11-check-item">' +
                    '<input type="checkbox" value="' + m.path.replace(/"/g,'&quot;') + '" data-name="' + m.name.replace(/"/g,'&quot;') + '" data-idx="' + i + '">' +
                    '<span class="win11-check-fake"></span>' +
                    '<span class="win11-check-label">' + m.name + '</span>' +
                    '<span class="win11-check-size">' + (m.size || '') + '</span>' +
                    '</label>';
            }).join('');
        }
    } catch (e) {
        list.innerHTML = '<div class="model-check-empty">加载失败: ' + e + '</div>';
    }
}

/// 关闭快速启动弹窗
function closeQuickStart() {
    document.getElementById('quickStartOverlay').classList.add('hidden');
}

/// 快速启动选中的模型
async function quickStartModels() {
    var overlay = document.getElementById('quickStartOverlay');
    var checked = document.querySelectorAll('#quickModelCheckList input[type="checkbox"]:checked');
    if (checked.length === 0) {
        alert('请先勾选要启动的模型');
        return;
    }

    var llamaPath = localStorage.getItem('launcher-llama-path') || '';
    if (!llamaPath) {
        alert('请先在设置中配置 llama.cpp 路径');
        return;
    }

    overlay.classList.add('hidden');

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

// 自动部署状态标志
var _autoDeployMode = false;
var _autoDeployModelId = null;
var _autoDeployInclude = null;
var _lastRecModelIds = null;
var _modelHwDb = null;
var _modelHwDbPromise = null;

async function loadModelHwDb() {
    if (_modelHwDb) return _modelHwDb;
    if (_modelHwDbPromise) return _modelHwDbPromise;
    _modelHwDbPromise = (async function() {
        try {
            var resp = await fetch('data/model_hardware_db.json');
            var db = await resp.json();
            _modelHwDb = db;
            return db;
        } catch (e) {
            _modelHwDb = null;
            return null;
        }
    })();
    return _modelHwDbPromise;
}

/// 一键自动部署：扫描硬件 → 推荐模型 → 下载 → 自动启动
var _deployResolve = null;
var _deployCancelled = false;

async function startAutoDeploy() {
    var overlay = document.getElementById('deployOverlay');
    if (!overlay) return;

    // 重置状态
    document.getElementById('deployScanPhase').classList.remove('hidden');
    document.getElementById('deployRecPhase').classList.add('hidden');
    overlay.classList.remove('hidden');

    var startTime = Date.now();
    var statusEl = document.getElementById('deployScanStatus');

    // 并行加载硬件数据库
    loadModelHwDb();

    // 获取硬件信息
    var gpuInfo = null;
    var vramMB = 0;
    try {
        statusEl.textContent = '正在检测 GPU...';
        gpuInfo = await window.__TAURI__.core.invoke('detect_gpu');
        vramMB = gpuInfo.vram_total_mb || 0;
        statusEl.textContent = '检测到 ' + (gpuInfo.gpu_name || 'GPU') + (vramMB > 0 ? ' (' + Math.round(vramMB/1024) + 'GB 显存)' : '') + '，正在分析性能...';
    } catch (e) {
        statusEl.textContent = '未检测到独立 GPU，使用 CPU 模式';
    }

    // 确保至少 5 秒
    var elapsed = Date.now() - startTime;
    if (elapsed < 5000) {
        statusEl.textContent = '正在分析硬件性能...';
        await new Promise(function(r) { setTimeout(r, 5000 - elapsed); });
    }

    // 获取官方模型列表
    var models = _officialModelsCache;
    if (!models || models.length === 0) {
        try {
            models = await window.__TAURI__.core.invoke('list_official_models');
            // 为每个模型分配唯一索引
            models.forEach(function(m, i) { m._idx = i; });
            _officialModelsCache = models;
        } catch (e) {
            overlay.classList.add('hidden');
            alert('获取模型列表失败: ' + e);
            return;
        }
    } else {
        // 确保缓存中也有 _idx
        models.forEach(function(m, i) { if (m._idx === undefined) m._idx = i; });
    }

    // 推荐三个模型
    var recs = recommendModels(gpuInfo, vramMB, models);
    if (!recs || recs.length === 0) {
        overlay.classList.add('hidden');
        alert('暂无可推荐的模型');
        return;
    }

    // 渲染推荐卡片
    var container = document.getElementById('deployRecCards');
    var cardColors = { 'Qwen系列': '#4FC3F7', 'DeepSeek': '#81C784', 'GLM': '#FFB74D', 'Yi系列': '#E57373', 'Google': '#BA68C8', 'Llama': '#64B5F6' };
    container.innerHTML = recs.map(function(m, i) {
        var selected = i === 0 ? ' selected' : '';
        var color = cardColors[m._recGroup] || '#4FC3F7';
        var tokStr = m._recTokS ? m._recTokS + ' tok/s' : '';
        var quantStr = m._recQuant ? m._recQuant : '';
        var groupStr = m._recGroup || m.group || '';
        var descParts = [];
        if (groupStr) descParts.push(groupStr);
        if (quantStr) descParts.push(quantStr);
        if (tokStr) descParts.push('~' + tokStr);
        var descLine = descParts.join(' · ');
        return '<div class="rec-card' + selected + '" data-idx="' + i + '" onclick="selectRecCard(this)">' +
            '<div class="rec-card-title" style="color:' + color + '">' + (m.name || m.model_id) + '</div>' +
            '<div class="rec-card-desc">' + descLine + '</div>' +
            '<div class="rec-card-desc" style="margin-top:4px;font-size:11px;color:var(--text-tertiary)">' + (m.desc || m.size || '') + '</div>' +
        '</div>';
    }).join('');

    // 动态更新推荐描述
    var groups = {};
    recs.forEach(function(m) {
        var g = m._recGroup || m.group || '其他';
        groups[g] = (groups[g] || 0) + 1;
    });
    var groupStr = Object.keys(groups).join('、');
    var recDescEl = document.getElementById('deployRecDesc');
    if (recDescEl) {
        recDescEl.textContent = '共推荐 ' + recs.length + ' 个模型（' + groupStr + '），目标 ' +
            (recs[0] && recs[0]._recTokS ? recs[0]._recTokS + '~' + recs[recs.length-1]._recTokS + ' tok/s' : '稳定运行');
    }

    // 扫描完成动画：圆环变打勾，保留在顶部
    var ring = document.getElementById('deployScanRing');
    var check = document.getElementById('deployScanCheck');
    if (ring && check) {
        ring.classList.add('hidden');
        check.classList.remove('hidden');
        var polyline = check.querySelector('polyline');
        if (polyline) { polyline.classList.add('check-draw'); }
    }
    var statusEl2 = document.getElementById('deployScanStatus');
    if (statusEl2) statusEl2.textContent = '扫描完成';

    // 推荐卡片直接显示在打勾下方（不隐藏扫描阶段）
    await new Promise(function(r) { setTimeout(r, 500); });
    document.getElementById('deployRecPhase').classList.remove('hidden');

    // 等待用户确认
    _deployResolve = null;
    _deployCancelled = false;
    await new Promise(function(r) { _deployResolve = r; });

    overlay.classList.add('hidden');
    if (_deployCancelled) { _deployResolve = null; _deployCancelled = false; return; }
    _deployCancelled = false;

    // 开始下载选中的模型
    var idx = getSelectedRecIdx();
    if (idx < 0 || idx >= recs.length) return;
    var selectedModel = recs[idx];

    // 设置自动部署标志
    _autoDeployMode = true;
    _autoDeployModelId = selectedModel.model_id;
    _autoDeployInclude = selectedModel.include || null;

    // 下载模型（复用官方模型下载逻辑）
    var modelsDir = localStorage.getItem('launcher-models-dir') || '';
    if (!modelsDir) {
        alert('请先在设置中配置模型目录路径');
        switchPage('settings');
        return;
    }

    // 检查 modelscope
    try {
        var available = await window.__TAURI__.core.invoke('check_modelscope_available');
        if (!available) {
            alert('需要安装 modelscope 命令行工具才能下载模型。\n\n请运行: pip install modelscope');
            return;
        }
    } catch (e) {
        alert('检查 modelscope 失败: ' + e);
        return;
    }

    // 构建 include 模式
    var inc = selectedModel.include || '';
    var includePattern = inc || '*.gguf';

    showDownloadOverlay(selectedModel.model_id);

    try {
        await window.__TAURI__.core.invoke('start_model_download', {
            modelId: selectedModel.model_id,
            localDir: modelsDir,
            includePattern: includePattern || null,
        });
    } catch (e) {
        updateDownloadProgress(0, '启动失败: ' + e);
        _autoDeployMode = false;
        setTimeout(function() { hideDownloadOverlay(); }, 3000);
    }
}

function selectRecCard(el) {
    document.querySelectorAll('.rec-card').forEach(function(c) { c.classList.remove('selected'); });
    el.classList.add('selected');
}

function getSelectedRecIdx() {
    var el = document.querySelector('.rec-card.selected');
    return el ? parseInt(el.dataset.idx) : -1;
}

function confirmAutoDeploy() {
    if (_deployResolve) {
        _deployResolve();
        _deployResolve = null;
    }
}

function cancelAutoDeploy() {
    document.getElementById('deployOverlay').classList.add('hidden');
    if (_deployResolve) {
        _deployResolve();
        _deployResolve = null;
    }
    _deployCancelled = true;
}

// ============================================================
// 模型硬件性能数据库（内建回退数据，JSON 加载失败时使用）
var MODEL_HW_DB = {
    q4_vram_gb: {
        "0.27": 0.3, "0.5": 0.4, "0.6": 0.5, "1": 0.7, "1.3": 0.9, "1.5": 1.0,
        "1.7": 1.2, "3": 2.0, "4": 2.6, "5.7": 3.5, "6": 3.8, "6.7": 4.2,
        "7": 4.5, "8": 5.0, "9": 5.8, "12": 7.5, "13": 8.2,
        "14": 8.8, "27": 16.5, "30": 6.5, "32": 19.0, "33": 20.5, "34": 21.0, "72": 42.0
    },
    q_mult: { q2_K: 0.55, q3_K_M: 0.70, q4_K_M: 1.0, Q4_K_M: 1.0, q5_K_M: 1.15,
              Q5_0: 1.25, Q5_K_M: 1.2, q6_K: 1.35, q8_0: 1.75, Q6_K: 1.4, Q8_0: 1.8 },
    arch_mult: { MoE: 1.4, Dense: 1.0 },
    gpu_tiers: [
        { vram_min: 24000, bw: 900, tpg: 0.70 },
        { vram_min: 16000, bw: 700, tpg: 0.65 },
        { vram_min: 12000, bw: 520, tpg: 0.60 },
        { vram_min: 8000,  bw: 380, tpg: 0.55 },
        { vram_min: 6000,  bw: 300, tpg: 0.50 },
        { vram_min: 4000,  bw: 200, tpg: 0.45 },
        { vram_min: 0,     bw: 30,  tpg: 0.30 }
    ],
    speed_targets: {
        "旗舰":   { min: 25, max: 40 },
        "高端":   { min: 20, max: 35 },
        "中高端": { min: 15, max: 30 },
        "中端":   { min: 12, max: 25 },
        "入门":   { min: 8,  max: 18 },
        "核显/无独显": { min: 5, max: 10 }
    }
};

/// 从 GPU 名称中匹配型号，返回 { bw, tpg, tier } 或 null
function matchGpuByName(gpuName, db) {
    if (!gpuName || !db || !db.gpu_perf) return null;
    var list = db.gpu_perf;
    for (var i = 0; i < list.length; i++) {
        var entry = list[i];
        var pat = entry.pattern;
        try {
            var regex = new RegExp(pat, 'i');
            if (regex.test(gpuName)) {
                return { bw: entry.bw, tpg: entry.tpg, tier: entry.tier };
            }
        } catch(e) {}
    }
    return null;
}

/// 根据硬件智能推荐三个模型（跨家族、稳定 Token/s、量化适配）
function recommendModels(gpuInfo, vramMB, models) {
    var ggufModels = models.filter(function(m) {
        return m.quant && m.quant.length > 0 && m.size;
    });
    if (ggufModels.length === 0) return [];

    // 优先使用加载的 JSON 数据，否则回退到内建
    var db = _modelHwDb || MODEL_HW_DB;
    var q4base = (db.q4_vram_gb || db.vram_usage_gb?.q4_base || MODEL_HW_DB.q4_vram_gb);
    var qmult = (db.q_mult || db.quant_mult || MODEL_HW_DB.q_mult);
    var tiers = (db.gpu_tiers || db.vram_tiers || MODEL_HW_DB.gpu_tiers);
    var archMult = (db.arch_mult || MODEL_HW_DB.arch_mult);
    var speedTbl = (db.speed_targets || MODEL_HW_DB.speed_targets);

    var isCuda = gpuInfo && gpuInfo.backend === 'cuda';
    var availMB = vramMB > 0 ? vramMB : 8192;
    var gpuName = gpuInfo && gpuInfo.gpu_name ? gpuInfo.gpu_name : '';

    // 1) 按 GPU 具体型号匹配带宽（精确）
    var matched = matchGpuByName(gpuName, db);
    var bandwidth, tokPerGb, gpuTier;
    if (matched) {
        bandwidth = matched.bw;
        tokPerGb = matched.tpg;
        gpuTier = matched.tier;
    } else {
        // 2) 回退：按显存分档
        var fallbackTier = tiers.find(function(t) { return availMB >= (t.vram_min || t.vram_min_mb || 0); }) || tiers[tiers.length - 1];
        bandwidth = fallbackTier.bw || fallbackTier.bw_fallback || 30;
        tokPerGb = fallbackTier.tpg || fallbackTier.tpg_fallback || 0.30;
        gpuTier = fallbackTier.label || '核显/无独显';
    }

    // CPU 模式强制低带宽
    if (!isCuda) { bandwidth = 30; tokPerGb = 0.30; gpuTier = '核显/无独显'; }

    // 查速度目标
    var target = speedTbl[gpuTier] || speedTbl['核显/无独显'] || { min: 5, max: 10 };

    // 为每个模型打分
    var scored = [];
    for (var i = 0; i < ggufModels.length; i++) {
        var m = ggufModels[i];
        var sizeMatch = m.size.match(/([\d.]+)/);
        if (!sizeMatch) continue;
        var sizeKey = sizeMatch[1];
        var q4Vram = q4base[sizeKey];
        if (!q4Vram) continue;

        var quants = m.quant || [];
        if (quants.length === 0) continue;

        // 按质量降序排序
        var sortedQuants = quants.slice().sort(function(a, b) {
            var mulA = qmult[a] || qmult[a.toUpperCase()] || qmult[a.toLowerCase()] || 1.0;
            var mulB = qmult[b] || qmult[b.toUpperCase()] || qmult[b.toLowerCase()] || 1.0;
            return mulB - mulA;
        });

        // 判断架构类型：MoE 模型用修正系数
        var isMoE = m.family && (m.family.indexOf('MoE') >= 0 || m.name.indexOf('A3B') >= 0);
        var archFactor = isMoE ? (archMult.MoE || 1.4) : (archMult.Dense || 1.0);
        var headroom = isMoE ? 0.90 : 0.85;

        var selectedQuant = null;
        var selectedVram = 0;

        for (var q = 0; q < sortedQuants.length; q++) {
            var qName = sortedQuants[q];
            var mul = qmult[qName] || qmult[qName.toUpperCase()] || qmult[qName.toLowerCase()] || 1.0;
            var vram = q4Vram * mul;
            if (vram < availMB / 1024 * headroom) {
                selectedQuant = qName;
                selectedVram = vram;
                break;
            }
        }

        if (!selectedQuant) {
            var smallest = sortedQuants[sortedQuants.length - 1];
            var smul = qmult[smallest] || qmult[smallest.toUpperCase()] || qmult[smallest.toLowerCase()] || 1.0;
            selectedVram = q4Vram * smul;
            if (selectedVram < availMB / 1024 * 0.95) {
                selectedQuant = smallest;
            } else {
                continue;
            }
        }

        // 估算 Token/s（MoE × 架构修正系数）
        var estTokS = (bandwidth * tokPerGb) / selectedVram * archFactor;

        // 速度分数
        var speedScore;
        if (estTokS < target.min) {
            speedScore = estTokS / target.min * 0.8;
        } else if (estTokS <= target.max) {
            speedScore = 1.0 + (target.max - estTokS) / (target.max - target.min + 0.1) * 0.2;
        } else if (estTokS <= target.max * 1.5) {
            speedScore = 1.0 - (estTokS - target.max) / target.max * 0.2;
        } else {
            speedScore = target.max / estTokS * 0.6;
        }

        var isQ4 = selectedQuant.toLowerCase().indexOf('q4') >= 0 || selectedQuant.indexOf('Q4') >= 0;
        var quantBonus = isQ4 ? 1.15 : (selectedQuant.toLowerCase().indexOf('q2') >= 0 ? 0.75 : 1.0);

        scored.push({
            model: m, idx: m._idx, group: m.group || '', family: m.family || '',
            size: parseFloat(sizeKey), quant: selectedQuant,
            vramGb: Math.round(selectedVram * 10) / 10,
            tokS: Math.round(estTokS),
            totalScore: speedScore * quantBonus
        });
    }

    if (scored.length === 0) return [];

    scored.sort(function(a, b) { return b.totalScore - a.totalScore; });

    // 多样性选择
    var groups = ['DeepSeek', 'Qwen系列', 'GLM', 'Yi系列', 'Google', 'Llama'];
    var result = [];
    var usedGroup = {};
    var usedIdx = {};

    for (var g = 0; g < groups.length && result.length < 3; g++) {
        var bestInGroup = null;
        for (var s = 0; s < scored.length; s++) {
            var item = scored[s];
            if (usedIdx[item.idx]) continue;
            if (item.group === groups[g] || item.family.indexOf(groups[g].replace('系列','')) >= 0) {
                if (!bestInGroup || item.totalScore > bestInGroup.totalScore) {
                    bestInGroup = item;
                }
            }
        }
        if (bestInGroup) {
            result.push(bestInGroup);
            usedGroup[bestInGroup.group] = true;
            usedIdx[bestInGroup.idx] = true;
        }
    }

    if (result.length < 3) {
        for (var s2 = 0; s2 < scored.length && result.length < 3; s2++) {
            var item2 = scored[s2];
            if (usedIdx[item2.idx]) continue;
            if (!usedGroup[item2.group]) {
                result.push(item2);
                usedGroup[item2.group] = true;
                usedIdx[item2.idx] = true;
            }
        }
    }

    if (result.length < 3) {
        for (var s3 = 0; s3 < scored.length && result.length < 3; s3++) {
            var item3 = scored[s3];
            if (usedIdx[item3.idx]) continue;
            result.push(item3);
            usedIdx[item3.idx] = true;
        }
    }

    // 避免重复
    var recSig = result.map(function(r) { return r.idx; }).sort().join(',');
    if (_lastRecModelIds && _lastRecModelIds === recSig && result.length === 3 && scored.length > 3) {
        var usedG = {};
        result.forEach(function(r) { usedG[r.group] = true; });
        var replaced = false;
        for (var sAlt = 0; sAlt < scored.length; sAlt++) {
            var alt = scored[sAlt];
            if (usedIdx[alt.idx]) continue;
            if (!usedG[alt.group]) {
                result[2] = alt;
                usedIdx[alt.idx] = true;
                replaced = true;
                break;
            }
        }
        if (!replaced) {
            for (var sAlt2 = 0; sAlt2 < scored.length; sAlt2++) {
                var alt2 = scored[sAlt2];
                if (usedIdx[alt2.idx]) continue;
                result[2] = alt2;
                usedIdx[alt2.idx] = true;
                break;
            }
        }
        recSig = result.map(function(r) { return r.idx; }).sort().join(',');
    }
    _lastRecModelIds = recSig;

    return result.map(function(r) {
        r.model._recTokS = r.tokS;
        r.model._recQuant = r.quant;
        r.model._recGroup = r.group;
        r.model._recFamily = r.family;
        r.model._recVramGb = r.vramGb;
        return r.model;
    });
}

/// 自动部署模式：下载完成后自动启动模型并跳转到会话
async function autoStartModelAfterDownload() {
    try {
        var modelsDir = localStorage.getItem('launcher-models-dir') || '';
        if (!modelsDir) return;

        // 查找刚下载的模型
        var modelInfo = await window.__TAURI__.core.invoke('auto_find_model', {
            modelsDir: modelsDir
        });
        if (!modelInfo) return;

        var llamaPath = localStorage.getItem('launcher-llama-path') || '';
        if (!llamaPath) return;

        var port = parseInt(localStorage.getItem('launcher-port') || '20000');
        var ctx = parseInt(localStorage.getItem('launcher-ctx') || '4096');

        await window.__TAURI__.core.invoke('start_model', {
            llamaPath: llamaPath,
            modelPath: modelInfo.path,
            modelName: modelInfo.name,
            port: port,
            ctx: ctx,
        });

        // 重置自动部署标志
        _autoDeployMode = false;
        _autoDeployModelId = null;
        _autoDeployInclude = null;

        // 跳转到极简对话
        pollServerStatus();
        loadRunningModels();
        switchPage('chat');

    } catch (e) {
        console.warn('自动启动失败:', e);
    }
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

// ---- 下载进度净化（纯前端修复：tqdm 乱码 / ANSI 光标码 / 进度条鬼畜）----
// 后端有两条事件流：原始 stderr 行流（progress 恒为 0、message 为 tqdm 原文，
// 含 GBK→UTF-8 乱码与 \x1b[A 光标码）和解析流（带真实 pct）。
// 直接渲染会导致进度条反复归零（鬼畜）和乱码文案。
var _dlShownPct = null;      // 当前展示的百分比（只许前进）
var _dlShownFile = null;     // 当前文件名（多文件下载，换文件允许回退重计）
var _dlPending = null;       // 节流窗口内挂起的最新渲染
var _dlThrottleTimer = null;

/// 解析 tqdm 进度行。对乱码免疫：文件名/数字/单位都是 ASCII 字段，不受 GBK 转换影响。
function parseTqdmInfo(raw) {
    if (!raw) return null;
    var text = String(raw)
        .replace(/\x1b\[[0-9;]*[A-Za-z]/g, '')   // ANSI 转义序列（光标控制等）
        .replace(/\[A/g, '')                     // 残留的光标上移码
        .replace(/\uFFFD/g, '');                 // 乱码占位符
    var segs = text.split(/[\r\n]+/);
    for (var i = segs.length - 1; i >= 0; i--) { // 取最新一段（多文件时最后一段是当前进度）
        var seg = segs[i];
        if (seg.indexOf('%') < 0 && seg.indexOf('/') < 0) continue;
        var mfile = seg.match(/\[([^\[\]]+)\]:\s*\d{1,3}\s*%/);
        var mpct = seg.match(/(\d{1,3})\s*%/);
        var msize = seg.match(/(\d+(?:\.\d+)?[KMG]?B?)\s*\/\s*(\d+(?:\.\d+)?[KMG]?B?)/);
        var mmeta = seg.match(/\[(\d+:\d+)<(\d+:\d+),\s*([\d.]+\s*[KMG]?B?\/s)/);
        if (mpct || msize) {
            return {
                file: mfile ? mfile[1] : null,
                pct: mpct ? Math.min(parseInt(mpct[1], 10), 100) : null,
                size: msize ? msize[1] + '/' + msize[2] : null,
                speed: mmeta ? mmeta[3] : null,
                eta: mmeta ? mmeta[2] : null
            };
        }
    }
    return null;
}

function buildCleanDownloadMsg(info) {
    var parts = [];
    if (info.file) parts.push(info.file);
    if (info.size) parts.push(info.size);
    if (info.speed) parts.push(info.speed);
    if (info.eta) parts.push('剩余 ' + info.eta);
    return parts.length ? parts.join(' · ') : '正在下载...';
}

/// 节流渲染（tqdm 每秒吐多行，限频 ~5次/秒，窗口内始终渲染最新值）
function renderDownloadProgress(pct, msg) {
    _dlPending = [pct, msg];
    if (_dlThrottleTimer) return;
    _dlThrottleTimer = setTimeout(function() {
        _dlThrottleTimer = null;
        var p = _dlPending;
        _dlPending = null;
        if (p) updateDownloadProgress(p[0], p[1]);
    }, 200);
}

/// 显示下载弹层
let _downloadUnlisten = null;
let _downloadPulseTimer = null;
let _lastModelId = null;
let _downloadDone = false;
let _downloadError = null;

function showDownloadOverlay(modelId) {
    _lastModelId = modelId;
    document.getElementById('downloadTitle').textContent = '正在下载模型';
    document.getElementById('downloadModelInfo').textContent = modelId;
    document.getElementById('downloadOverlay').classList.remove('hidden');

    document.getElementById('cancelDownloadBtn').classList.remove('hidden');
    document.getElementById('retryDownloadBtn').classList.add('hidden');

    if (_downloadUnlisten) { _downloadUnlisten(); _downloadUnlisten = null; }
    if (_downloadPulseTimer) { clearInterval(_downloadPulseTimer); _downloadPulseTimer = null; }

    _downloadDone = false;
    _downloadError = null;
    _dlShownPct = null;
    _dlShownFile = null;
    _dlPending = null;
    if (_dlThrottleTimer) { clearTimeout(_dlThrottleTimer); _dlThrottleTimer = null; }

    updateDownloadProgress(0, '正在连接下载源...');
    // 等待首条真实进度期间只做文字动画，不伪造进度条（避免与真实进度打架）
    _downloadPulseTimer = setInterval(function() {
        var msgEl = document.getElementById('downloadProgressMsg');
        if (msgEl) msgEl.textContent = '正在连接下载源' + '...'.slice(0, 1 + (Math.floor(Date.now() / 400) % 3));
    }, 400);

    if (window.__TAURI__?.event?.listen) {
        window.__TAURI__.event.listen('download-progress', function(event) {
            var data = event.payload;

            if (data.status === 'downloading') {
                // 原始行流的 progress 恒为 0（会把进度条打回起点的元凶），
                // 一律以消息里解析出的 pct 为准；进度只许前进，换文件才允许回退重计。
                var info = parseTqdmInfo(data.message);
                if (info && info.pct !== null) {
                    if (info.file && _dlShownFile && info.file !== _dlShownFile) _dlShownPct = null;
                    if (info.file) _dlShownFile = info.file;
                    if (_dlShownPct === null || info.pct >= _dlShownPct) _dlShownPct = info.pct;
                    renderDownloadProgress(_dlShownPct, buildCleanDownloadMsg(info));
                } else if (typeof data.progress === 'number' && data.progress > 0) {
                    if (_dlShownPct === null || data.progress >= _dlShownPct) _dlShownPct = data.progress;
                    renderDownloadProgress(_dlShownPct, '正在下载...');
                } else if (data.message && String(data.message).indexOf('\uFFFD') < 0) {
                    // UTF-8 环境修复后的非进度信息行（如"正在下载 xxx"），净化 ANSI 码后照常显示
                    var clean = String(data.message)
                        .replace(/\x1b\[[0-9;]*[A-Za-z]/g, '')
                        .replace(/\[A/g, '')
                        .replace(/[\r\n]+/g, ' ')
                        .trim();
                    if (clean) renderDownloadProgress(_dlShownPct === null ? 0 : _dlShownPct, clean);
                }
                return;
            }

            if (data.status === 'completed') {
                _downloadDone = true; _downloadError = null;
                updateDownloadProgress(100, data.message || '下载完成！');
                document.getElementById('cancelDownloadBtn').classList.add('hidden');
                document.getElementById('retryDownloadBtn').classList.add('hidden');
                refreshModels();
                if (_autoDeployMode) {
                    setTimeout(function() { hideDownloadOverlay(); autoStartModelAfterDownload(); }, 1500);
                } else {
                    setTimeout(function() { hideDownloadOverlay(); }, 3000);
                }
            } else if (data.status === 'error') {
                _downloadError = data.message || '下载失败';
                _dlShownPct = null;
                updateDownloadProgress(0, data.message || '下载失败');
                document.getElementById('cancelDownloadBtn').classList.add('hidden');
                document.getElementById('retryDownloadBtn').classList.remove('hidden');
            } else if (data.status === 'verifying') {
                updateDownloadProgress(99, data.message || '校验中...');
            } else if (data.status === 'retrying') {
                _dlShownPct = null;
                updateDownloadProgress(0, data.message || '重试中...');
                document.getElementById('cancelDownloadBtn').classList.remove('hidden');
                document.getElementById('retryDownloadBtn').classList.add('hidden');
            } else if (data.status === 'downloading') {
                if (_downloadPulseTimer) {
                    clearInterval(_downloadPulseTimer);
                    _downloadPulseTimer = null;
                }
            }
        }).then(function(u) { _downloadUnlisten = u; });
    }
}

function hideDownloadOverlay() {
    document.getElementById('downloadOverlay').classList.add('hidden');
    if (_downloadUnlisten) { _downloadUnlisten(); _downloadUnlisten = null; }
    if (_downloadPulseTimer) { clearInterval(_downloadPulseTimer); _downloadPulseTimer = null; }
    if (_dlThrottleTimer) { clearTimeout(_dlThrottleTimer); _dlThrottleTimer = null; }
    _dlPending = null;
}

/// 重试下载
async function retryDownload() {
    document.getElementById('cancelDownloadBtn').classList.remove('hidden');
    document.getElementById('retryDownloadBtn').classList.add('hidden');
    updateDownloadProgress(0, '正在重试...');

    try {
        await window.__TAURI__.core.invoke('retry_download');
    } catch (e) {
        updateDownloadProgress(0, '[!]重试启动失败: ' + e);
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
                                    // 按参数大小排序（小到大）
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
    debugLog('[DL:OFFICIAL] downloadOfficialModel started, model=' + m.model_id).catch(function(){});

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
        updateDownloadProgress(0, '启动失败: ' + e);
        setTimeout(function() { hideDownloadOverlay(); }, 3000);
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
    if (!llamaInstallDir) {
        alert('请先完成首次启动配置（llama.cpp 路径），再进行编译');
        return;
    }
    // 输出到 llama 安装目录的父级
    let outputDir = llamaInstallDir.substring(0, llamaInstallDir.lastIndexOf('\\llama\\'));
    if (!outputDir) outputDir = llamaInstallDir;

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

    // 开机自启（读注册表实时状态；变更立即生效，不等"保存设置"）
    const autoLaunchEl = document.getElementById('autoLaunchOnBoot');
    if (autoLaunchEl) {
        if (window.__TAURI__?.core?.invoke) {
            window.__TAURI__.core.invoke('get_auto_launch')
                .then(function(v) { autoLaunchEl.checked = !!v; })
                .catch(function() {});
        }
        autoLaunchEl.onchange = function() {
            var v = autoLaunchEl.checked;
            autoLaunchEl.disabled = true;
            window.__TAURI__.core.invoke('set_auto_launch', { enable: v })
                .then(function(msg) { showKbToast(msg); })
                .catch(function(e) { alert('设置开机自启失败: ' + e); autoLaunchEl.checked = !v; })
                .finally(function() { autoLaunchEl.disabled = false; });
        };
    }
    // 启动时恢复上次运行的模型
    const restoreEl = document.getElementById('restoreModelsOnBoot');
    if (restoreEl) restoreEl.checked = localStorage.getItem('launcher-restore-models') !== 'false';
}

function onLangChange(value) {
    setLanguage(value);
    // 刷新 UI 中的动态文本（状态轮询是异步的，完成后会重绘所有动态文案）
    pollServerStatus();
    loadSessionList();
    updateLiteCtx();
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

    // 保存"启动时恢复模型"开关
    const restoreEl = document.getElementById('restoreModelsOnBoot');
    if (restoreEl) {
        localStorage.setItem('launcher-restore-models', restoreEl.checked ? 'true' : 'false');
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
    var html = text
        .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

    // 保护代码块内容（先用占位符替代，避免后续处理破坏）
    var codeBlocks = [];
    var idx = 0;
    html = html.replace(/```(\w*)\n?([\s\S]*?)```/g, function(m, lang, code) {
        var safeLang = lang.replace(/[^a-zA-Z0-9_\-]/g, '');
        var langClass = safeLang ? ' class="lang-' + safeLang + '"' : '';
        var placeholder = '%%CODE_BLOCK_' + (idx++) + '%%';
        codeBlocks.push('<pre><code' + langClass + '>' + code.trim() + '</code></pre>');
        return placeholder;
    });

    // 行内代码
    html = html.replace(/`([^`]+)`/g, '<code>$1</code>');

    // 水平线
    html = html.replace(/^---+$/gm, '<hr>');

    // 标题
    html = html.replace(/^######\s+(.+)$/gm, '<h6>$1</h6>');
    html = html.replace(/^#####\s+(.+)$/gm, '<h5>$1</h5>');
    html = html.replace(/^####\s+(.+)$/gm, '<h4>$1</h4>');
    html = html.replace(/^###\s+(.+)$/gm, '<h3>$1</h3>');
    html = html.replace(/^##\s+(.+)$/gm, '<h2>$1</h2>');
    html = html.replace(/^#\s+(.+)$/gm, '<h1>$1</h1>');

    // 引用块（支持多行连续 >）
    html = html.replace(/(?:^>[\s\S]*?(?:\n|$))+/gm, function(m) {
        var content = m.replace(/^>\s?/gm, '').trim();
        return '<blockquote>' + content.replace(/\n/g, '<br>') + '</blockquote>';
    });

    // 粗斜体
    html = html.replace(/\*\*\*(.+?)\*\*\*/g, '<strong><em>$1</em></strong>');
    // 加粗
    html = html.replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>');
    html = html.replace(/__(.+?)__/g, '<strong>$1</strong>');
    // 斜体
    html = html.replace(/\*(.+?)\*/g, '<em>$1</em>');
    html = html.replace(/_(.+?)_/g, '<em>$1</em>');
    // 删除线
    html = html.replace(/~~(.+?)~~/g, '<del>$1</del>');

    // 链接
    html = html.replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2" target="_blank" rel="noopener">$1</a>');
    // 图片
    html = html.replace(/!\[([^\]]*)\]\(([^)]+)\)/g, '<img src="$2" alt="$1" style="max-width:100%;border-radius:6px;">');

    // 无序/有序列表
    var lines = html.split('\n');
    var result = [];
    var inUl = false;
    var inOl = false;
    for (var i = 0; i < lines.length; i++) {
        var line = lines[i];
        var ulMatch = line.match(/^(\s*)[-*+]\s+(.+)$/);
        var olMatch = line.match(/^(\s*)\d+\.\s+(.+)$/);
        if (ulMatch) {
            if (inOl) { result.push('</ol>'); inOl = false; }
            if (!inUl) { result.push('<ul>'); inUl = true; }
            result.push('<li>' + ulMatch[2] + '</li>');
        } else if (olMatch) {
            if (inUl) { result.push('</ul>'); inUl = false; }
            if (!inOl) { result.push('<ol>'); inOl = true; }
            result.push('<li>' + olMatch[2] + '</li>');
        } else {
            if (inUl) { result.push('</ul>'); inUl = false; }
            if (inOl) { result.push('</ol>'); inOl = false; }
            result.push(line);
        }
    }
    if (inUl) result.push('</ul>');
    if (inOl) result.push('</ol>');
    html = result.join('\n');

    // 段落包裹（跳过已经是块级标签的行）
    var blockTags = /^<(h[1-6]|ul|ol|li|pre|blockquote|hr|p|div|table|img)/i;
    var paraLines = html.split('\n');
    var paraResult = [];
    var paraBuffer = [];
    for (var p = 0; p < paraLines.length; p++) {
        var pl = paraLines[p].trim();
        if (pl === '' || blockTags.test(pl) || pl.indexOf('</') === 0 || pl.indexOf('%%CODE_BLOCK_') >= 0) {
            if (paraBuffer.length > 0) {
                paraResult.push('<p>' + paraBuffer.join('<br>') + '</p>');
                paraBuffer = [];
            }
            if (pl !== '') paraResult.push(pl);
        } else {
            paraBuffer.push(pl);
        }
    }
    if (paraBuffer.length > 0) {
        paraResult.push('<p>' + paraBuffer.join('<br>') + '</p>');
    }
    html = paraResult.join('\n');

    // 恢复代码块
    for (var ci = 0; ci < codeBlocks.length; ci++) {
        html = html.replace('%%CODE_BLOCK_' + ci + '%%', codeBlocks[ci]);
    }

    return html;
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
    // 清理模板残留（小模型偶尔会输出 %%CODEBLOCK\d+%% 之类的占位符）
    let cleaned = text.replace(/%%\w+%%/g, '');
    // 只从字符串末尾反复剥离这些 token
    const endTokens = [
        '<|im_end|>', '<|im_start|>', '<|im_sep|>',
        '<|assistant|>', '<|user|>', '<|system|>',
        '</s>', '<s>',
        '<end_of_turn>', '<eos>', '<bos>',
    ];
    // 从末尾循环剥离（允许堆叠，如 <|im_end|><|im_end|>）
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
        addLiteMessage('system', '[!]服务未运行，请先在「启动」页面启动服务');
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
    // 1.5 知识库上下文注入（自动检索，作为 system 消息放在总结之后、用户消息之前）
    if (_kbAutoRetrieve && window.__TAURI__?.core?.invoke) {
        try {
            var kbCtx = await window.__TAURI__.core.invoke('kb_get_context', { query: text, top_k: 3 });
            if (kbCtx) {
                bodyMessages.push({ role: 'system', content: kbCtx });
            }
        } catch(e) { /* KB 不可用时静默跳过 */ }
    }
    // 2. 加入最近消息 + 当前消息，裁剪总长度
    var recentCount = 6;
    var msgs = _liteMessages.slice(-recentCount);
    msgs.forEach(function(m) {
        bodyMessages.push({ role: m.role === 'think' ? 'assistant' : m.role, content: m.content });
    });
    var totalChars = bodyMessages.reduce(function(sum, m) { return sum + (m.content || '').length; }, 0);
    if (totalChars > MAX_CTX_CHARS) {
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

    // 工具调用时：获取 llama 直连端口，添加 tools 定义
    var useToolCall = _kbToolCall && window.__TAURI__?.core?.invoke;
    var llamaDirectPort = null;
    if (useToolCall) {
        try {
            llamaDirectPort = await window.__TAURI__.core.invoke('get_model_llama_port', {
                modelName: _liteSelectedModel
            });
            body.tools = getToolDefinitions();
        } catch(e) {
            // 获取端口失败则回退到普通模式
            useToolCall = false;
        }
    }

    if (multimodal) {
        body.multimodal = true;
        var msgs2 = body.messages;
        for (var i = msgs2.length - 1; i >= 0; i--) {
            if (msgs2[i].role === 'user') {
                msgs2[i].content = [{ type: 'text', text: msgs2[i].content }];
                break;
            }
        }
    }

    // 选择端点：有工具调用 → 直接发 llama /v1/chat/completions；否则走代理 /chat
    var endpoint = useToolCall && llamaDirectPort
        ? ('http://127.0.0.1:' + llamaDirectPort + '/v1/chat/completions')
        : ('http://127.0.0.1:' + port + '/chat');

    try {
        var resp = await fetch(endpoint, {
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
    var _liteToolCalls = null;  // 工具调用累积
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
                debugLog(`chat 流超时(${timeoutMs}ms)：${_hasOutput ? '已输出无新数据' : '等待首次响应超时'}`);
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

                    // ── 工具调用：累积 tool_calls delta ──
                    var toolCallsDelta = json?.choices?.[0]?.delta?.tool_calls;
                    if (toolCallsDelta && Array.isArray(toolCallsDelta)) {
                        _hasOutput = true;
                        onFirstToken();
                        for (var tci = 0; tci < toolCallsDelta.length; tci++) {
                            var tc = toolCallsDelta[tci];
                            if (!_liteToolCalls) _liteToolCalls = {};
                            if (!_liteToolCalls[tc.index]) {
                                _liteToolCalls[tc.index] = {
                                    index: tc.index,
                                    id: tc.id || ('call_' + Date.now() + '_' + tc.index),
                                    type: tc.type || 'function',
                                    function: { name: '', arguments: '' }
                                };
                            }
                            if (tc.function) {
                                if (tc.function.name) {
                                    _liteToolCalls[tc.index].function.name += tc.function.name;
                                }
                                if (tc.function.arguments) {
                                    _liteToolCalls[tc.index].function.arguments += tc.function.arguments;
                                }
                            }
                        }
                    }

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
        } else if (fullContent) {
            // API 没返回 usage 时，按字符数估算 token 数
            var estTokens = Math.ceil(fullContent.length / 2.5) + 50;
            _liteAccTokens = (_liteAccTokens || 0) + estTokens;
        }
        updateLiteCtx();

        // ── 工具调用执行 ──
        if (_liteToolCalls && Object.keys(_liteToolCalls).length > 0) {
            var toolCallIds = Object.keys(_liteToolCalls).sort();
            // 把模型的 tool_call 加入消息历史
            var toolCallMsg = {
                role: 'assistant',
                content: null,
                tool_calls: toolCallIds.map(function(k) { return _liteToolCalls[k]; })
            };
            _liteMessages.push(toolCallMsg);

            // 更新气泡显示工具调用状态
            if (bubble) {
                bubble.innerHTML = '<div style="padding:6px;color:var(--text-secondary);font-size:12px;display:flex;align-items:center;gap:6px;">' +
                    '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/></svg>' +
                    '正在调用工具...</div>';
                msgDiv.classList.remove('streaming');
            }

            // 串行执行所有工具调用
            var toolResults = [];
            for (var ti = 0; ti < toolCallIds.length; ti++) {
                var tc = _liteToolCalls[toolCallIds[ti]];
                var result = await executeToolCall(tc);
                toolResults.push({
                    role: 'tool',
                    tool_call_id: tc.id,
                    content: result
                });
            }

            // 发送 follow-up 请求（非流式，更可靠）
            var followBody = {
                model: _liteSelectedModel || 'default',
                messages: bodyMessages.concat([toolCallMsg]).concat(toolResults),
                stream: false
            };
            var followEndpoint = llamaDirectPort
                ? ('http://127.0.0.1:' + llamaDirectPort + '/v1/chat/completions')
                : ('http://127.0.0.1:' + port + '/chat');
            try {
                var followResp = await fetch(followEndpoint, {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify(followBody)
                });
                if (followResp.ok) {
                    var followJson = await followResp.json();
                    var answer = followJson?.choices?.[0]?.message?.content || followJson?.content || '';
                    if (answer && bubble) {
                        bubble.innerHTML = marked(answer);
                        _liteMessages.push({ role: 'assistant', content: answer });
                    } else if (bubble) {
                        bubble.innerHTML = '<span style="color:var(--text-secondary);font-style:italic;">（工具调用完成，模型未返回额外内容）</span>';
                    }
                } else {
                    if (bubble) {
                        bubble.innerHTML = '<span style="color:var(--danger);">工具调用结果获取失败 (HTTP ' + followResp.status + ')</span>';
                    }
                }
            } catch(e) {
                debugLog('chat 工具调用 follow-up 失败: ' + e);
                if (bubble) {
                    bubble.innerHTML = '<span style="color:var(--danger);">工具调用出错: ' + e + '</span>';
                }
            }
            _liteToolCalls = null;
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
    // 初始化知识库工具调用开关
    setupLiteKbToolToggle();
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

function setupLiteKbToolToggle() {
    const bar = document.getElementById('kbToolStatusToggle');
    const icon = bar ? bar.querySelector('.tgl-icon') : null;
    if (!bar) return;
    bar.onclick = function() {
        if (icon) {
            icon.style.transition = 'transform 0.15s ease';
            icon.style.transform = 'scale(0.7)';
        }
        setTimeout(() => {
            if (icon) {
                icon.style.transition = 'none';
                icon.style.transform = 'scale(1)';
            }
            toggleKbTool();
            requestAnimationFrame(() => {
                if (icon) icon.style.transition = 'transform 0.15s ease';
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
                    detectAndShowKbTool();
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
                    '<button class="btn btn-sm session-rename-btn" onclick="event.stopPropagation();renameSession(\'' + s.id + '\',\'' + escHtml(s.title).replace(/'/g,"\\'") + '\')"><svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/><path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/></svg></button>' +
                    '<button class="btn btn-sm session-del-btn" onclick="event.stopPropagation();deleteSession(\'' + s.id + '\')"><svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/></svg></button>' +
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
        refreshLiteSessionName();
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
    refreshLiteSessionName();
    switchPage('chat');
}

/// 重命名会话
async function renameSession(sessionId, currentTitle) {
    var newTitle = prompt('输入新名称:', currentTitle);
    if (!newTitle || newTitle === currentTitle) return;
    try {
        await window.__TAURI__.core.invoke('rename_session', { sessionId: sessionId, newTitle: newTitle });
        await loadSessionList();
        refreshLiteSessionName();
    } catch (e) { alert('重命名失败: ' + e); }
}

// ============================================================
// 会话栏（SA Lite 顶部）
// ============================================================

/// 刷新会话栏显示的会话名
function refreshLiteSessionName() {
    var el = document.getElementById('liteSessionName');
    if (!el) return;
    var sessions = document.querySelectorAll('#sessionList .session-item');
    if (_currentSessionId) {
        var active = document.querySelector('#sessionList .session-item-active');
        var title = active ? active.querySelector('.session-item-title')?.textContent : '';
        el.textContent = title || '会话';
    } else {
        el.textContent = '新会话';
    }
}

/// 刷新会话下拉框
async function refreshLiteSessionDropdown() {
    var dropdown = document.getElementById('liteSessionDropdown');
    if (!dropdown) return;
    try {
        var sessions = await window.__TAURI__.core.invoke('list_sessions');
        if (!sessions || sessions.length === 0) {
            dropdown.innerHTML = '<div class="mqs-option" style="color:var(--text-tertiary);cursor:default;">暂无会话</div>';
            return;
        }
        dropdown.innerHTML = sessions.map(function(s) {
            var active = s.id === _currentSessionId ? ' active' : '';
            var title = escHtml(s.title || '会话');
            return '<div class="mqs-option' + active + '" onclick="switchLiteSession(\'' + s.id + '\')">' + title + '</div>';
        }).join('');
    } catch (e) {
        dropdown.innerHTML = '<div class="mqs-option" style="color:var(--text-tertiary);cursor:default;">加载失败</div>';
    }
}

/// 切换会话（会话栏下拉用）
async function switchLiteSession(sessionId) {
    document.getElementById('liteSessionDropdown').classList.remove('open');
    await switchSession(sessionId);
    refreshLiteSessionName();
}

/// 切换会话栏下拉框
function toggleLiteSessionDropdown() {
    var dropdown = document.getElementById('liteSessionDropdown');
    if (!dropdown) return;
    var isOpen = dropdown.classList.contains('open');
    // 关闭其他 dropdown
    document.querySelectorAll('.mqs-dropdown.open').forEach(function(el) { el.classList.remove('open'); });
    if (!isOpen) {
        refreshLiteSessionDropdown();
        dropdown.classList.add('open');
    }
}

// 点击外部关闭会话栏下拉
document.addEventListener('click', function(e) {
    var sw = document.getElementById('liteSessionSwitch');
    if (sw && !sw.contains(e.target)) {
        document.getElementById('liteSessionDropdown')?.classList.remove('open');
    }
});

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
        refreshLiteSessionName();
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

// ============================================================
// 知识库 (Knowledge Base)
// ============================================================

var _kbAutoRetrieve = true;
var _kbToolCall = false;

/// 初始化知识库页面
function initKnowledgePage() {
    loadKbFolders();
    initKbDropZone();
    var kbToggle = document.getElementById('kbToggleBtn');
    if (kbToggle) {
        updateKbToggleUI();
    }
}

/// 加载文件夹列表
async function loadKbFolders() {
    try {
        var folders = await window.__TAURI__.core.invoke('kb_list_folders');
        var listEl = document.getElementById('kbFolderList');
        if (!listEl) return;
        if (!folders || folders.length === 0) {
            listEl.innerHTML = '<div style="padding:12px;text-align:center;color:var(--text-tertiary);font-size:13px;">暂无文件夹，点击下方添加</div>';
            return;
        }
        var html = '';
        folders.forEach(function(f) {
            var safeName = f.replace(/'/g, "\\'");
            html += '<div class="session-item" style="display:flex;align-items:center;justify-content:space-between;padding:8px 10px;font-size:13px;">';
            html += '<span style="overflow:hidden;text-overflow:ellipsis;white-space:nowrap;max-width:70%;">' + f + '</span>';
            html += '<button class="btn btn-sm btn-danger" onclick="removeKbFolder(\'' + safeName + '\')" style="font-size:11px;padding:2px 8px;">移除</button>';
            html += '</div>';
        });
        listEl.innerHTML = html;
    } catch(e) {
        console.warn('加载知识库文件夹失败:', e);
    }
}

/// 添加文件夹
async function addKbFolder() {
    var input = document.getElementById('kbFolderInput');
    var folder = input?.value.trim();
    if (!folder) { alert('请输入文件夹路径'); return; }
    try {
        var result = await window.__TAURI__.core.invoke('kb_add_folder', { folder: folder });
        alert(result);
        input.value = '';
        loadKbFolders();
        updateKbStatus();
    } catch(e) {
        alert('添加失败: ' + e);
    }
}

/// 浏览文件夹
async function browseKbFolder() {
    // Tauri 2 无原生文件夹选择器，用 prompt 让用户输入
    var folder = prompt('请输入文件夹路径：\n例如: E:\\Documents\\my_knowledge');
    if (folder) {
        document.getElementById('kbFolderInput').value = folder;
    }
}

/// 移除文件夹
async function removeKbFolder(folder) {
    try {
        var result = await window.__TAURI__.core.invoke('kb_remove_folder', { folder: folder });
        alert(result);
        loadKbFolders();
        updateKbStatus();
    } catch(e) {
        alert('移除失败: ' + e);
    }
}

/// 处理拖拽/选择的文件
function initKbDropZone() {
    var zone = document.getElementById('kbDropZone');
    if (!zone) return;
    zone.addEventListener('dragover', function(e) {
        e.preventDefault();
        zone.style.borderColor = 'var(--accent)';
        zone.style.background = 'rgba(79,195,247,0.08)';
    });
    zone.addEventListener('dragleave', function(e) {
        e.preventDefault();
        zone.style.borderColor = 'var(--border-color)';
        zone.style.background = 'transparent';
    });
    zone.addEventListener('drop', function(e) {
        e.preventDefault();
        zone.style.borderColor = 'var(--border-color)';
        zone.style.background = 'transparent';
        var files = e.dataTransfer.files;
        if (files && files.length > 0) {
            handleKbFiles(files);
        }
    });
}

async function handleKbFiles(files) {
    if (!files || files.length === 0) return;
    var resultsEl = document.getElementById('kbSingleFiles');
    if (!resultsEl) return;
    resultsEl.innerHTML = '';

    for (var fi = 0; fi < files.length; fi++) {
        var file = files[fi];
        var filePath = file.path || file.name;
        resultsEl.innerHTML += '<div style="padding:4px 0;font-size:12px;color:var(--text-secondary);">正在索引 ' + file.name + '...</div>';
        try {
            var result = await window.__TAURI__.core.invoke('kb_index_file', { filePath: filePath });
            resultsEl.innerHTML += '<div style="padding:4px 0;font-size:12px;color:var(--text-accent);">[OK]' + result + '</div>';
        } catch(e) {
            resultsEl.innerHTML += '<div style="padding:4px 0;font-size:12px;color:#e57373;">[!]' + e + '</div>';
        }
    }
    updateKbStatus();
}

/// 搜索知识库
async function searchKb() {
    var input = document.getElementById('kbSearchInput');
    var query = input?.value.trim();
    if (!query) return;
    var resultsEl = document.getElementById('kbSearchResults');
    if (!resultsEl) return;
    resultsEl.innerHTML = '<div style="padding:8px;text-align:center;color:var(--text-tertiary);font-size:12px;">搜索中...</div>';
    try {
        var results = await window.__TAURI__.core.invoke('kb_search', { query: query, top_k: 10 });
        if (!results || results.length === 0) {
            resultsEl.innerHTML = '<div style="padding:8px;text-align:center;color:var(--text-tertiary);font-size:12px;">未找到相关结果</div>';
            return;
        }
        var html = '';
        results.forEach(function(r, i) {
            var text = (r.text || '').substring(0, 200);
            var name = r.doc_name || 'unknown';
            var score = r.score || 0;
            html += '<div class="session-item" style="padding:8px 10px;margin-bottom:4px;cursor:default;">';
            html += '<div style="font-size:12px;font-weight:600;color:var(--text-primary);">' + name + ' <span style="font-weight:400;color:var(--text-tertiary);font-size:11px;">(相关度 ' + score + ')</span></div>';
            html += '<div style="font-size:11px;color:var(--text-secondary);margin-top:2px;">' + escapeHtml(text) + '</div>';
            html += '</div>';
        });
        resultsEl.innerHTML = html;
    } catch(e) {
        resultsEl.innerHTML = '<div style="padding:8px;text-align:center;color:#e57373;font-size:12px;">搜索失败: ' + e + '</div>';
    }
}

/// 更新知识库状态
async function updateKbStatus() {
    try {
        var status = await window.__TAURI__.core.invoke('kb_status');
        if (status) {
            var dot = document.getElementById('kbEmbedDot');
            var label = document.getElementById('kbEmbedLabel');
            var embedStats = document.getElementById('kbEmbedStatus');
            if (status.embedding_available) {
                if (dot) dot.style.background = '#4CAF50';
                if (label) label.textContent = '已就绪 (' + (status.embedded_chunks || 0) + ' 分块已嵌入)';
                if (embedStats) embedStats.style.display = '';
            }
            if (status.embedded_chunks > 0 && embedStats) {
                embedStats.innerHTML = '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="#4CAF50" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" style="vertical-align:-2px;margin-right:4px;"><polyline points="20 6 9 17 4 12"/></svg> 嵌入模型已就绪 — ' + status.embedded_chunks + ' 个分块已嵌入，使用语义搜索。';
            }
        }
    } catch(e) {}
}

/// 初始化知识库页面
function initKnowledgePage() {
    loadKbFolders();
    initKbDropZone();
    var kbAuto = document.getElementById('kbAutoBtn');
    if (kbAuto) { updateKbAutoUI(); }
    checkEmbeddingStatus();
}

/// 嵌入功能已禁用（0.5.x 版本启用）——以下为占位实现，防止 onclick 调用未定义函数崩溃
function startEmbeddingService() {
    showKbToast('嵌入功能将在 0.5.x 版本启用，当前使用关键词检索');
}

function stopEmbeddingService() {
    showKbToast('嵌入功能将在 0.5.x 版本启用，当前使用关键词检索');
}

/// 检查嵌入状态（嵌入禁用期间仅同步 kb_status 显示）
async function checkEmbeddingStatus() {
    try {
        var status = await window.__TAURI__.core.invoke('kb_status');
        if (status && status.embedding_available) {
            await updateKbStatus();
        } else {
            var dot = document.getElementById('kbEmbedDot');
            var label = document.getElementById('kbEmbedLabel');
            if (dot) dot.style.background = '#888';
            if (label) label.textContent = '已禁用（0.5.x 启用）';
        }
    } catch(e) {}
}







/// 下载嵌入模型（嵌入已禁用，0.5.x 启用；避免用户白下 23MB 模型）
async function downloadEmbeddingModel() {
    showKbToast('嵌入功能将在 0.5.x 版本启用，当前使用关键词检索');
}

/// SA Lite 自动检索开关
function toggleKbAuto() {
    var turningOn = !_kbAutoRetrieve;
    _kbAutoRetrieve = turningOn;
    updateKbAutoUI();
    try {
        window.__TAURI__.core.invoke('kb_toggle', { enabled: _kbAutoRetrieve });
    } catch(e) {}
    // 开启时提示知识库已就绪（纯关键词检索，无嵌入模型）
    if (turningOn) {
        showKbToast('知识库已开启（关键词检索模式）');
    }
}

function updateKbAutoUI() {
    var label = document.getElementById('kbAutoLabel');
    var btn = document.getElementById('kbAutoBtn');
    if (!label || !btn) return;
    if (_kbAutoRetrieve) {
        label.innerHTML = '<svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor" style="vertical-align:-2px;margin-right:2px;"><path d="M4 6H2v14c0 1.1.9 2 2 2h14v-2H4V6zm16-4H8c-1.1 0-2 .9-2 2v12c0 1.1.9 2 2 2h12c1.1 0 2-.9 2-2V4c0-1.1-.9-2-2-2zm0 14H8V4h12v12z"/></svg> 知识库';
        btn.style.borderColor = 'var(--accent)';
        btn.style.color = 'var(--accent)';
    } else {
        label.innerHTML = '<svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor" style="vertical-align:-2px;margin-right:2px;"><path d="M4 6H2v14c0 1.1.9 2 2 2h14v-2H4V6zm16-4H8c-1.1 0-2 .9-2 2v12c0 1.1.9 2 2 2h12c1.1 0 2-.9 2-2V4c0-1.1-.9-2-2-2zm0 14H8V4h12v12z"/></svg> 关闭';
        btn.style.borderColor = 'var(--border-color)';
        btn.style.color = 'var(--text-tertiary)';
    }
}

/// 自动配置嵌入服务（点开知识库时触发）
async function autoSetupEmbedding() {
    var llamaPath = localStorage.getItem('launcher-llama-path') || '';
    var modelsDir = localStorage.getItem('launcher-models-dir') || '';
    if (!llamaPath || !modelsDir) return;

    // 显示加载状态
    var btn = document.getElementById('kbAutoBtn');
    if (btn) { btn.style.opacity = '0.6'; btn.style.pointerEvents = 'none'; }

    try {
        var result = await window.__TAURI__.core.invoke('auto_setup_embedding', {
            llamaPath: llamaPath,
            modelsDir: modelsDir
        });
        if (result === 'EMBEDDING_ALREADY_RUNNING') {
            showKbToast('知识库系统已启动');
            procKbEmbedAll();
        } else if (result === 'EMBEDDING_SETUP_OK') {
            showKbToast('知识库系统已启动');
            procKbEmbedAll();
        } else if (result === 'EMBEDDING_DOWNLOAD_FAILED') {
            showKbToast('知识库已开启（关键词检索模式，嵌入模型下载失败）');
        }
        checkEmbeddingStatus();
        updateKbStatus();
    } catch(e) {
        showKbToast('知识库启动失败: ' + e);
    }
    if (btn) { btn.style.opacity = '1'; btn.style.pointerEvents = ''; }
}

/// 后台执行嵌入（不阻塞用户）
async function procKbEmbedAll() {
    setTimeout(async function() {
        // 等 2 秒让嵌入服务就绪
        try {
            await window.__TAURI__.core.invoke('kb_embed_all');
            updateKbStatus();
        } catch(e) { /* 嵌入失败不影响聊天 */ }
    }, 2000);
}

/// 显示小弹窗（右下角，自动消失）
function showKbToast(msg) {
    var existing = document.getElementById('kbToast');
    if (existing) existing.remove();
    var toast = document.createElement('div');
    toast.id = 'kbToast';
    toast.style.cssText = 'position:fixed;bottom:80px;right:20px;padding:10px 16px;background:var(--bg-card);border:1px solid var(--accent);border-radius:8px;font-size:13px;color:var(--text-primary);box-shadow:0 4px 16px rgba(0,0,0,0.25);z-index:9999;animation:kbToastIn 0.3s ease;max-width:360px;';
    toast.textContent = msg;
    document.body.appendChild(toast);
    setTimeout(function() {
        toast.style.transition = 'opacity 0.3s ease';
        toast.style.opacity = '0';
        setTimeout(function() { toast.remove(); }, 300);
    }, 4000);
}

/// 检测并显示工具调用按钮（模型支持时显示）
function detectAndShowKbTool() {
    var toggle = document.getElementById('kbToolStatusToggle');
    if (!toggle) return;
    var supportsTool = isToolCallSupported(_liteSelectedModel);
    if (supportsTool) {
        toggle.classList.remove('hidden');
    } else {
        toggle.classList.add('hidden');
    }
}

/// 判断模型是否支持原生工具调用
function isToolCallSupported(modelName) {
    if (!modelName || modelName === 'default') return false;
    var name = modelName.toLowerCase();
    var patterns = ['qwen3', 'qwen2.5', 'deepseek', 'functionary', 'glm-4', 'glm4'];
    for (var i = 0; i < patterns.length; i++) {
        if (name.indexOf(patterns[i]) >= 0) return true;
    }
    return false;
}

/// 高级工具调用开关
function toggleKbTool() {
    _kbToolCall = !_kbToolCall;
    updateKbToolUI();
}

function updateKbToolUI() {
    var toggle = document.getElementById('kbToolStatusToggle');
    if (!toggle) return;
    if (_kbToolCall) {
        toggle.classList.add('active');
    } else {
        toggle.classList.remove('active');
    }
}

/// 构建工具定义（当工具调用开启时）
function getToolDefinitions() {
    return [
        {
            type: 'function',
            function: {
                name: 'search_knowledge_base',
                description: '在用户的知识库中搜索相关内容。当用户的问题涉及文档、笔记、代码或项目文件时，调用此工具。',
                parameters: {
                    type: 'object',
                    properties: {
                        query: {
                            type: 'string',
                            description: '搜索关键词，用空格分隔多个关键词'
                        }
                    },
                    required: ['query']
                }
            }
        },
        {
            type: 'function',
            function: {
                name: 'read_file',
                description: '读取指定文件的完整内容。当用户想查看某个文件的具体内容时调用此工具。',
                parameters: {
                    type: 'object',
                    properties: {
                        path: {
                            type: 'string',
                            description: '文件的绝对路径'
                        }
                    },
                    required: ['path']
                }
            }
        }
    ];
}

/// 根据函数名执行工具
async function executeToolCall(toolCall) {
    var name = toolCall.function.name;
    var args = {};
    try {
        args = JSON.parse(toolCall.function.arguments || '{}');
    } catch(e) { args = {}; }

    if (name === 'search_knowledge_base') {
        try {
            var results = await window.__TAURI__.core.invoke('kb_search', {
                query: args.query || '',
                top_k: 5
            });
            return JSON.stringify(results);
        } catch(e) {
            return JSON.stringify({ error: '知识库检索失败: ' + e });
        }
    } else if (name === 'read_file') {
        try {
            var content = await window.__TAURI__.core.invoke('kb_read_file', {
                path: args.path || ''
            });
            return content;
        } catch(e) {
            return JSON.stringify({ error: '读取文件失败: ' + e });
        }
    }
    return JSON.stringify({ error: '未知工具: ' + name });
}

/// HTML 转义
function escapeHtml(text) {
    var d = document.createElement('div');
    d.textContent = text;
    return d.innerHTML;
}

/// 重新索引所有知识库文件夹
async function reindexKbAll() {
    try {
        var folders = await window.__TAURI__.core.invoke('kb_list_folders');
        if (!folders || folders.length === 0) { alert('没有已添加的文件夹'); return; }
        // 逐个重加实现重新索引
        for (var fi = 0; fi < folders.length; fi++) {
            await window.__TAURI__.core.invoke('kb_remove_folder', { folder: folders[fi] });
        }
        for (var fi = 0; fi < folders.length; fi++) {
            await window.__TAURI__.core.invoke('kb_add_folder', { folder: folders[fi] });
        }
        loadKbFolders();
        alert('[OK]重新索引完成 (' + folders.length + ' 个文件夹)');
    } catch(e) {
        alert('重新索引失败: ' + e);
    }
}

/// 清除临时索引文件（拖拽/选择的单次文件）
async function clearKbTemp() {
    var el = document.getElementById('kbSingleFiles');
    if (el) el.innerHTML = '';
    try {
        var result = await window.__TAURI__.core.invoke('kb_clear_temp');
        loadKbFolders();
        alert(result);
    } catch(e) {
        alert('清除失败: ' + e);
    }
}
