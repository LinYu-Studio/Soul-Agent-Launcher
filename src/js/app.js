// Soul Agent Launcher - 前端逻辑

// ============================================================
// 初始化
// ============================================================
document.addEventListener('DOMContentLoaded', async () => {
    initTheme();
    await checkAndRunSetup();
    loadSettings();
    initNavigation();
    initLaunchPage();

    // 启动后定期检查服务状态
    startStatusPolling();

    // 监听 llama-server 的 stderr/stdout 事件
    listenServerOutput();
});

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
    console.log('[setup] checkAndRunSetup 开始');
    // 快速跳过：如果已经配置过 llama.cpp 路径，直接跳过首次安装
    var existingPath = localStorage.getItem('launcher-llama-path');
    console.log('[setup] localStorage launcher-llama-path:', existingPath);
    if (existingPath) {
        console.log('[setup] 已有配置，跳过安装检查，后台检查版本');
        hideSetupOverlay();
        checkLlamaVersion();
        return;
    }

    // localStorage 为空，先尝试从 Rust 端 config.json 恢复
    console.log('[setup] localStorage 无配置，尝试从 Rust 端恢复...');
    try {
        var config = await window.__TAURI__.core.invoke('load_config');
        if (config && config.llama_path) {
            console.log('[setup] 从 config.json 恢复了 llama_path:', config.llama_path);
            localStorage.setItem('launcher-llama-path', config.llama_path);
            if (config.port) localStorage.setItem('launcher-port', config.port);
            if (config.ctx_size) localStorage.setItem('launcher-ctx-size', String(config.ctx_size));
            hideSetupOverlay();
            checkLlamaVersion();
            return;
        }
    } catch (e) {
        console.warn('[setup] Rust 端配置加载失败:', e);
    }

    try {
        // 先检查是否已安装（仅文件存在性检查，不触发 GPU 检测）
        console.log('[setup] 调用 check_setup_needed...');
        var needsSetup = await window.__TAURI__.core.invoke('check_setup_needed');
        console.log('[setup] needsSetup:', needsSetup);
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

    } catch (e) {
        console.warn('modelscope 安装失败（可手动安装）:', e);
        addConsoleLine('warn', `modelscope 安装失败: ${e}`);
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

        var dot = document.getElementById('serverDot');
        var label = document.getElementById('serverLabel');
        var detail = document.getElementById('serverDetail');
        var startBtn = document.getElementById('startServerBtn');
        var stopBtn = document.getElementById('stopServerBtn');
        var apiEndpoint = document.getElementById('apiEndpoint');
        var apiUrl = document.getElementById('apiUrl');
        var badge = document.getElementById('serverStatusBadge');

        if (dot) dot.className = running ? 'engine-dot online' : 'engine-dot offline';
        if (label) label.textContent = running ? '运行中' : '未启动';
        if (detail) detail.textContent = running
            ? '端口 ' + port + ' · 运行中 · ' + modelNames
            : '端口 ' + port + ' · 已停止';
        if (startBtn) startBtn.style.display = running ? 'none' : '';
        if (stopBtn) stopBtn.style.display = running ? '' : 'none';
        if (badge) badge.textContent = running ? '● 已连接' : '● 已断开';

        // 显示 API 端点
        if (running && apiEndpoint && apiUrl) {
            apiEndpoint.style.display = 'block';
            apiUrl.innerHTML = `原生 /chat → http://localhost:${port}/chat<br>OpenAI → http://localhost:${port}/v1/chat/completions`;
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

        if (homeDot) homeDot.className = running ? 'status-dot online' : 'status-dot offline';
        if (homeLabel) homeLabel.textContent = running ? '服务运行中' : '服务未启动';
        if (homeDetail) homeDetail.textContent = `端口 ${port}`;
        if (homeStartBtn) homeStartBtn.textContent = running ? '管理服务' : '启动服务';
        if (homeStopBtn) homeStopBtn.style.display = running ? '' : 'none';
        if (badge) {
            badge.textContent = running ? '● 已连接' : '● 已断开';
            badge.className = `title-subtitle ${running ? 'online' : 'offline'}`;
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
        document.getElementById('homeModelCount').textContent = models.length;
    }).catch(function() {
        container.innerHTML = '<div class="model-check-empty">加载失败</div>';
    });
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
        document.getElementById('homeModelCount').textContent = models.length;

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

/// 显示下载弹层
let _downloadUnlisten = null;
let _downloadPulseTimer = null;

function showDownloadOverlay(modelId) {
    document.getElementById('downloadTitle').textContent = '正在下载模型';
    document.getElementById('downloadModelInfo').textContent = modelId;
    document.getElementById('downloadOverlay').classList.remove('hidden');

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
                updateDownloadProgress(100, '下载完成！');
                refreshModels();
                setTimeout(function() { hideDownloadOverlay(); }, 2000);
            } else if (data.status === 'error') {
                updateDownloadProgress(0, '⚠️ ' + (data.message || '下载失败'));
                setTimeout(function() { hideDownloadOverlay(); }, 5000);
            }
        }).then(function(u) { _downloadUnlisten = u; });
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
                                ${items.map(m => {
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
    const inc = m.include || '';
    const includePattern = selectedQuant ? `*${selectedQuant}*.gguf` : (inc || '*.gguf');

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

    // 加载退出时卸载选项
    const autoUnload = localStorage.getItem('launcher-auto-unload');
    const autoUnloadEl = document.getElementById('autoUnloadModels');
    if (autoUnloadEl) autoUnloadEl.checked = autoUnload !== 'false';  // 默认 true
}

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

    var port = getLitePort();
    var multimodal = document.getElementById('liteMultimodal')?.checked || false;
    _liteAbortController = new AbortController();

    var body = {
        model: 'default',
        messages: _liteMessages.map(function(m) { return { role: m.role, content: m.content }; }),
        stream: true,
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
        var resp = await fetch('http://127.0.0.1:' + port + '/v1/chat/completions', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(body),
            signal: _liteAbortController.signal,
        });
        removeLiteTyping();

        if (!resp.ok) {
            var errText = await resp.text().catch(function() { return ''; });
            addLiteMessage('system', '⚠️ API请求失败 (HTTP ' + resp.status + ')' + (errText ? ': ' + errText : ''));
            _liteMessages.pop();
            return;
        }

        // 创建流式消息气泡
        var msgDiv = document.createElement('div');
        msgDiv.className = 'chat-msg assistant streaming';
        var bubble = document.createElement('div');
        bubble.className = 'chat-bubble';
        bubble.textContent = '';
        var time = document.createElement('div');
        time.className = 'chat-time';
        msgDiv.appendChild(bubble);
        msgDiv.appendChild(time);
        document.getElementById('liteMessages').appendChild(msgDiv);

        var reader = resp.body.getReader();
        var decoder = new TextDecoder();
        var fullContent = '';
        var _liteUsage = null;

        while (true) {
            var result = await reader.read();
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
                    var delta = json?.choices?.[0]?.delta?.content;
                    if (delta) {
                        fullContent += delta;
                        bubble.textContent = fullContent;
                        document.getElementById('liteMessages').scrollTop = 1e9;
                    }
                    if (json.usage) { _liteUsage = json.usage; }
                } catch (e) {}
            }
        }

        msgDiv.classList.remove('streaming');
        bubble.innerHTML = marked(fullContent);
        time.textContent = new Date().toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' });
        if (fullContent) _liteMessages.push({ role: 'assistant', content: fullContent });
        if (_liteUsage && _liteUsage.total_tokens) {
            _liteAccTokens = (_liteAccTokens || 0) + _liteUsage.total_tokens;
            updateLiteCtx();
        }

    } catch (e) {
        removeLiteTyping();
        if (e.name === 'AbortError') return;
        addLiteMessage('system', '⚠️ 网络请求失败: ' + e.message);
        _liteMessages.pop();
    } finally {
        _liteAbortController = null;
        updateLiteInputState();
    }
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
    setInterval(updateLiteInputState, 2000);
    updateLiteCtx();
    setInterval(updateLiteCtx, 10000);
});

// ============================================================
// 已启动模型管理
// ============================================================

var _selectedUnloadModels = {};

async function loadRunningModels() {
    try {
        var models = await window.__TAURI__.core.invoke('list_running_models');
        var container = document.getElementById('runningModelsList');
        var empty = document.getElementById('noRunningModels');
        var unloadBtn = document.getElementById('unloadSelectedBtn');
        if (!models || models.length === 0) {
            if (empty) empty.style.display = 'block';
            if (container) container.style.display = 'none';
            if (unloadBtn) unloadBtn.style.display = 'none'; return;
        }
        if (empty) empty.style.display = 'none';
        if (container) container.style.display = 'block';
        if (unloadBtn) unloadBtn.style.display = Object.keys(_selectedUnloadModels).length > 0 ? 'inline-flex' : 'none';
        container.innerHTML = '';
        models.forEach(function(m) {
            var row = document.createElement('div');
            row.className = 'running-model-row';
            row.innerHTML = '<label class="rm-checkbox"><input type="checkbox" data-name="' + m.name + '" onchange="toggleUnloadModel(this)"><span class="rm-checkmark"></span></label><div class="rm-info"><div class="rm-name">' + m.name + '</div><div class="rm-meta">端口 ' + m.proxy_port + ' · ' + m.ctx + 'K · ' + m.started_at + ' · PID ' + m.pid + '</div></div><button class="btn btn-sm rm-unload" onclick="unloadSingleModel(\'' + m.name.replace(/'/g,"\\'") + '\')">卸载</button>';
            container.appendChild(row);
        });
    } catch (e) { console.warn('加载运行模型失败:', e); }
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
