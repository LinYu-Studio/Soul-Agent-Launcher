// Soul Agent Launcher — 多语言支持（中文/英文）
// ============================================================

const LANG_ZH = 'zh';
const LANG_EN = 'en';

const TRANSLATIONS = {
    zh: {
        /* 通用 */
        'app.name': 'Soul Agent Lite',
        'app.tagline': '欢迎使用 Soul Agent Lite<br>输入消息开始对话',
        'app.slogan': '启动服务后即可开始对话',

        /* 导航 */
        'nav.home': '首页',
        'nav.models': '模型',
        'nav.chat': '对话',
        'nav.sessions': '会话',
        'nav.settings': '设置',
        'nav.launch': '启动页',

        /* 服务状态 */
        'server.running': '运行中',
        'server.loading': '模型正在加载中~马上就好~',
        'server.stopped': '未启动',
        'server.connected': '● 已连接',
        'server.disconnected': '● 已断开',
        'server.loading_badge': '● 加载中',
        'server.port_stopped': '端口 {port} · 已停止',
        'server.models': '{count} 个模型 · {names}',
        'server.loading_detail': '模型还在加载，请稍候...',
        'server.home_running': '服务运行中',
        'server.home_loading': '模型加载中...',
        'server.home_stopped': '服务未启动',
        'server.home_start': '启动服务',
        'server.home_loading_btn': '加载中...',
        'server.home_manage': '管理服务',

        /* 状态轮询错误 */
        'status.check_failed': '状态检查失败',

        /* 对话 */
        'chat.input_placeholder': '输入消息，Enter 发送，Shift+Enter 换行...',
        'chat.send': '发送',
        'chat.stop': '停止',
        'chat.thinking': '思考过程',
        'chat.new_session': '新会话已创建，开始对话吧！',
        'chat.api_error': '[!] API请求失败 (HTTP {status}){detail}',
        'chat.network_error': '[!] 网络请求失败: {msg}',
        'chat.welcome_title': 'Soul Agent Lite',
        'chat.welcome_desc': '欢迎使用 Soul Agent Lite<br>输入消息开始对话',

        /* 会话管理 */
        'session.empty': '暂无会话，点击「新建会话」开始对话。',
        'session.messages': '{count} 条消息 · {time}',
        'session.create': '新建会话',
        'session.rename_prompt': '输入新名称:',
        'session.delete_confirm': '确定要删除此会话及其所有消息吗？',
        'session.create_failed': '创建会话失败: {e}',
        'session.rename_failed': '重命名失败: {e}',
        'session.delete_failed': '删除失败: {e}',
        'session.load_failed': '加载消息失败',

        /* 设置 */
        'settings.title': '设置',
        'settings.language': '语言',
        'settings.language_zh': '中文',
        'settings.language_en': 'English',
        'settings.ctx_size': '上下文长度',
        'settings.auto_unload': '退出时卸载模型',
        'settings.auto_unload_desc': '关闭窗口时自动停止所有模型进程',
        'settings.version': '版本',

        /* 模型 */
        'model.no_models': '暂无可用模型',
        'model.download': '下载模型',
        'model.delete': '删除模型',
        'model.refresh': '刷新模型列表',
        'model.file_not_found': '模型文件不存在',

        /* 首页 */
        'home.title': 'Soul Agent Launcher',
        'home.subtitle': '启动与管理 llama.cpp 服务',
        'home.port': '端口 {port}',
        'home.model_dir': '模型目录',
        'home.open_dir': '打开目录',

        /* 自动总结 */
        'summary.generating': '正在生成对话总结...',
        'summary.failed': '生成失败',
        'summary.empty': '总结为空',
        'summary.loaded': '已加载总结',
        'summary.inject': '以下是之前的对话总结，请基于此继续对话：\n{summary}',
        'summary.prompt': '请对以上对话进行简要总结，提炼关键内容和结论，用中文回答。',

        /* 启动页 */
        'launch.start_server': '启动服务器',
        'launch.stop_server': '停止服务器',
        'launch.standby': '待机模式',
        'launch.wake': '唤醒',
        'launch.sleep': '休眠',
        'launch.port': '端口',
        'launch.ctx': '上下文',

        /* 首页卡片 */
        'home.status': '服务状态',
        'home.deploy': '一键自动部署',
        'home.bench': 'AI性能跑分',
        'home.guide': '使用说明',
        'home.tutorial': '使用教程',
        'home.select_model': '选择模型',
        'home.quick_guide': '快速指引',
        'mode.dev': '我是开发者',
        'mode.newbie': '我是新手',

        /* 启动加载画面 */
        'boot.starting': '正在启动…',
        'boot.init': '正在初始化…',
        'boot.lang': '正在加载语言…',
        'boot.theme': '正在应用主题…',
        'boot.config': '正在恢复配置…',
        'boot.backend': '正在检测硬件与加速后端…',
        'boot.llama': '正在校验 llama.cpp 版本…',
        'boot.python': '正在检测 Python…',
        'boot.pip': '正在检测 pip…',
        'boot.mscope': '正在检查 ModelScope…',
        'boot.ui': '正在加载界面…',
        'boot.restore': '正在恢复上次运行的模型…',
        'boot.poll': '正在启动状态轮询…',
        'boot.done': '启动完成',

        /* 环境检测步骤 */
        'setup.step.lang': '语言检测',
        'setup.step.lang_desc': '检测系统语言环境',
        'setup.step.theme': '主题加载',
        'setup.step.theme_desc': '加载用户主题偏好',
        'setup.step.config': '配置恢复',
        'setup.step.config_desc': '从配置文件恢复设置',
        'setup.step.backend': '硬件检测 + 后端同步',
        'setup.step.backend_desc': '检测 GPU/CPU 并自动匹配加速端',
        'setup.step.llama': '版本校验',
        'setup.step.llama_desc': '校验 llama.cpp 版本一致性',
        'setup.step.python': 'Python 检测',
        'setup.step.python_desc': '检测 Python 是否已安装',
        'setup.step.pip': 'pip 检测',
        'setup.step.pip_desc': '检测 pip 包管理器',
        'setup.step.mscope': 'ModelScope',
        'setup.step.mscope_desc': '安装模型下载工具',
        'setup.step.settings': '加载设置',
        'setup.step.settings_desc': '加载应用设置',
        'setup.step.update': '版本检查',
        'setup.step.update_desc': '检查新版本更新',
        'setup.status.running': '进行中…',
        'setup.status.success': '完成',
        'setup.status.skipped': '已跳过',
        'setup.status.error': '失败',
        'setup.status.installing': '未安装，正在安装…',

        /* 环境修复面板 */
        'env.title': '环境修复',
        'env.repair_hint': '组件异常时一键重装（Python/pip/ModelScope 是模型下载的依赖，加速端按硬件自动匹配）',
        'env.python': 'Python 运行时',
        'env.pip': 'pip 包管理器',
        'env.mscope': 'ModelScope 下载工具',
        'env.backend': 'llama.cpp 加速端',
        'env.check': '检测',
        'env.check_all': '全部重新检测',
        'env.not_checked': '未检测',

        /* 模型完整性校验 */
        'integrity.title': '模型完整性校验',
        'integrity.hint': '下载完成的模型会自动记录 SHA256。启动模型时自动比对体积；此处可粘贴路径做全量哈希校验。',
        'integrity.path_hint': '粘贴 GGUF 模型文件完整路径',
        'integrity.check_btn': '校验',
        'integrity.checking': '正在计算 SHA256（大文件需要一些时间）…',
        'integrity.ok': '✅ 完整性正常（{size}）',
        'integrity.no_record': 'ℹ️ 无下载记录（非本应用下载或记录缺失），当前体积 {size}',
        'integrity.recorded_at': '记录时间: {time}',
        'integrity.size_mismatch': '体积与下载记录不符',
        'integrity.hash_mismatch': '哈希与下载记录不符',
        'integrity.fail': '完整性校验未通过！',

        /* 反馈与支持（Beta⑤） */
        'feedback.title': '反馈与支持',
        'feedback.hint': '遇到问题或有建议？欢迎通过以下渠道反馈，或导出诊断报告一键提交。',
        'feedback.copy_qq': '复制 QQ 群号',
        'feedback.github': 'GitHub Issues',
        'feedback.anon': '匿名反馈',
        'feedback.diag': '复制诊断报告',
        'feedback.qq_missing': 'QQ 群号尚未配置（请在 app.js 顶部 FEEDBACK_QQ_GROUP 填入）',
        'feedback.qq_copied': 'QQ 群号已复制',
        'feedback.diag_copied': '诊断报告已复制到剪贴板',

        /* 模型恢复 */
        'restore.done': '已自动恢复上次运行的 {count} 个模型',
    },

    en: {
        /* General */
        'app.name': 'Soul Agent Lite',
        'app.tagline': 'Welcome to Soul Agent Lite<br>Start typing to begin a conversation',
        'app.slogan': 'Start the server to begin chatting',

        /* Navigation */
        'nav.home': 'Home',
        'nav.models': 'Models',
        'nav.chat': 'Chat',
        'nav.sessions': 'Sessions',
        'nav.settings': 'Settings',
        'nav.launch': 'Launch',

        /* Server Status */
        'server.running': 'Running',
        'server.loading': 'Model is loading~Almost ready~',
        'server.stopped': 'Stopped',
        'server.connected': '● Connected',
        'server.disconnected': '● Disconnected',
        'server.loading_badge': '● Loading',
        'server.port_stopped': 'Port {port} · Stopped',
        'server.models': '{count} model(s) · {names}',
        'server.loading_detail': 'Model loading, please wait...',
        'server.home_running': 'Service Running',
        'server.home_loading': 'Loading Model...',
        'server.home_stopped': 'Service Stopped',
        'server.home_start': 'Start Server',
        'server.home_loading_btn': 'Loading...',
        'server.home_manage': 'Manage',

        /* Status polling errors */
        'status.check_failed': 'Status check failed',

        /* Chat */
        'chat.input_placeholder': 'Type a message, Enter to send, Shift+Enter for newline...',
        'chat.send': 'Send',
        'chat.stop': 'Stop',
        'chat.thinking': 'Thinking',
        'chat.new_session': 'New session created, start chatting!',
        'chat.api_error': '[!] API request failed (HTTP {status}){detail}',
        'chat.network_error': '[!] Network request failed: {msg}',
        'chat.welcome_title': 'Soul Agent Lite',
        'chat.welcome_desc': 'Welcome to Soul Agent Lite<br>Start typing to begin a conversation',

        /* Sessions */
        'session.empty': 'No sessions yet. Click "New Session" to start a conversation.',
        'session.messages': '{count} messages · {time}',
        'session.create': 'New Session',
        'session.rename_prompt': 'Enter new name:',
        'session.delete_confirm': 'Delete this session and all its messages?',
        'session.create_failed': 'Failed to create session: {e}',
        'session.rename_failed': 'Failed to rename: {e}',
        'session.delete_failed': 'Failed to delete: {e}',
        'session.load_failed': 'Failed to load messages',

        /* Settings */
        'settings.title': 'Settings',
        'settings.language': 'Language',
        'settings.language_zh': '中文',
        'settings.language_en': 'English',
        'settings.ctx_size': 'Context Size',
        'settings.auto_unload': 'Unload models on exit',
        'settings.auto_unload_desc': 'Auto-stop all model processes when closing window',
        'settings.version': 'Version',

        /* Model */
        'model.no_models': 'No models available',
        'model.download': 'Download Model',
        'model.delete': 'Delete Model',
        'model.refresh': 'Refresh',
        'model.file_not_found': 'Model file not found',

        /* Home */
        'home.title': 'Soul Agent Launcher',
        'home.subtitle': 'Launch & manage llama.cpp service',
        'home.port': 'Port {port}',
        'home.model_dir': 'Model Directory',
        'home.open_dir': 'Open Folder',

        /* Summary */
        'summary.generating': 'Generating conversation summary...',
        'summary.failed': 'Summary failed',
        'summary.empty': 'Summary is empty',
        'summary.loaded': 'Summary loaded',
        'summary.inject': 'Below is a summary of our previous conversation. Please continue based on it:\n{summary}',
        'summary.prompt': 'Please provide a brief summary of the above conversation, extracting key points and conclusions.',

        /* Launch page */
        'launch.start_server': 'Start Server',
        'launch.stop_server': 'Stop Server',
        'launch.standby': 'Standby',
        'launch.wake': 'Wake Up',
        'launch.sleep': 'Sleep',
        'launch.port': 'Port',
        'launch.ctx': 'Context',

        /* Home cards */
        'home.status': 'Service Status',
        'home.deploy': 'One-Click Deploy',
        'home.bench': 'AI Benchmark',
        'home.guide': 'Guide',
        'home.tutorial': 'Tutorial',
        'home.select_model': 'Select Model',
        'home.quick_guide': 'Quick Guide',
        'mode.dev': "I'm a Developer",
        'mode.newbie': "I'm a Beginner",

        /* Boot splash */
        'boot.starting': 'Starting…',
        'boot.init': 'Initializing…',
        'boot.lang': 'Loading language…',
        'boot.theme': 'Applying theme…',
        'boot.config': 'Restoring configuration…',
        'boot.backend': 'Detecting hardware and backend…',
        'boot.llama': 'Verifying llama.cpp version…',
        'boot.python': 'Checking Python…',
        'boot.pip': 'Checking pip…',
        'boot.mscope': 'Checking ModelScope…',
        'boot.ui': 'Loading UI…',
        'boot.restore': 'Restoring last session models…',
        'boot.poll': 'Starting status polling…',
        'boot.done': 'Ready',

        /* Setup steps */
        'setup.step.lang': 'Language',
        'setup.step.lang_desc': 'Detect system language',
        'setup.step.theme': 'Theme',
        'setup.step.theme_desc': 'Load theme preference',
        'setup.step.config': 'Configuration',
        'setup.step.config_desc': 'Restore from config file',
        'setup.step.backend': 'Hardware + Backend',
        'setup.step.backend_desc': 'Detect GPU/CPU and match acceleration backend',
        'setup.step.llama': 'Version Check',
        'setup.step.llama_desc': 'Verify llama.cpp version consistency',
        'setup.step.python': 'Python',
        'setup.step.python_desc': 'Check Python installation',
        'setup.step.pip': 'pip',
        'setup.step.pip_desc': 'Check pip package manager',
        'setup.step.mscope': 'ModelScope',
        'setup.step.mscope_desc': 'Install model download tool',
        'setup.step.settings': 'Load Settings',
        'setup.step.settings_desc': 'Load application settings',
        'setup.step.update': 'Update Check',
        'setup.step.update_desc': 'Check for updates',
        'setup.status.running': 'Running…',
        'setup.status.success': 'Done',
        'setup.status.skipped': 'Skipped',
        'setup.status.error': 'Failed',
        'setup.status.installing': 'Not installed, installing…',

        /* Environment repair panel */
        'env.title': 'Environment Repair',
        'env.repair_hint': 'One-click reinstall for broken components (Python/pip/ModelScope power model downloads; backend matches your hardware)',
        'env.python': 'Python Runtime',
        'env.pip': 'pip Package Manager',
        'env.mscope': 'ModelScope Downloader',
        'env.backend': 'llama.cpp Backend',
        'env.check': 'Check',
        'env.check_all': 'Re-check All',
        'env.not_checked': 'Not checked',

        /* Model integrity check */
        'integrity.title': 'Model Integrity Check',
        'integrity.hint': 'SHA256 is recorded for models downloaded in-app. Size is verified automatically on launch; paste a path here for a full hash check.',
        'integrity.path_hint': 'Paste full path of a GGUF model file',
        'integrity.check_btn': 'Verify',
        'integrity.checking': 'Computing SHA256 (large files take a while)…',
        'integrity.ok': '✅ Integrity OK ({size})',
        'integrity.no_record': 'ℹ️ No download record (not downloaded in-app or record missing); current size {size}',
        'integrity.recorded_at': 'Recorded: {time}',
        'integrity.size_mismatch': 'size differs from download record',
        'integrity.hash_mismatch': 'hash differs from download record',
        'integrity.fail': 'Integrity check FAILED!',

        /* Feedback & support (Beta⑤) */
        'feedback.title': 'Feedback & Support',
        'feedback.hint': 'Found a bug or have ideas? Reach us via the channels below, or export a diagnostic report.',
        'feedback.copy_qq': 'Copy QQ Group No.',
        'feedback.github': 'GitHub Issues',
        'feedback.anon': 'Anonymous Feedback',
        'feedback.diag': 'Copy Diagnostic Report',
        'feedback.qq_missing': 'QQ group number is not configured yet (set FEEDBACK_QQ_GROUP in app.js)',
        'feedback.qq_copied': 'QQ group number copied',
        'feedback.diag_copied': 'Diagnostic report copied to clipboard',

        /* Model restore */
        'restore.done': 'Restored {count} model(s) from last session',
    }
};

// ============================================================
// 翻译函数
// ============================================================

let _currentLang = LANG_ZH;
const LANG_KEY = 'sal-language';

function getSystemLang() {
    try {
        if (navigator.language && navigator.language.startsWith('zh')) return LANG_ZH;
        if (navigator.languages && navigator.languages.some(function(l) { return l.startsWith('zh'); })) return LANG_ZH;
    } catch(e) {}
    return LANG_EN;
}

function getSavedLang() {
    return localStorage.getItem(LANG_KEY) || '';
}

function initLanguage() {
    var saved = getSavedLang();
    _currentLang = saved || getSystemLang();
    applyLanguage();
}

function setLanguage(lang) {
    _currentLang = lang;
    localStorage.setItem(LANG_KEY, lang);
    applyLanguage();
}

function getCurrentLang() {
    return _currentLang;
}

/// 翻译单条 key，支持 {param} 替换
function _t(key) {
    var map = TRANSLATIONS[_currentLang] || TRANSLATIONS[LANG_ZH];
    var text = map[key];
    if (text === undefined) {
        // fallback to zh
        text = TRANSLATIONS[LANG_ZH][key];
        if (text === undefined) return key;
    }
    // 处理额外参数
    if (arguments.length > 1) {
        var args = Array.prototype.slice.call(arguments, 1);
        for (var i = 0; i < args.length; i += 2) {
            if (i + 1 < args.length) {
                text = text.replace(new RegExp('\\{' + args[i] + '\\}', 'g'), args[i + 1]);
            }
        }
    }
    return text;
}

/// 翻译带参数的 key，参数以对象形式传入
/// _tp('server.port_stopped', { port: 20000 })
function _tp(key, params) {
    var map = TRANSLATIONS[_currentLang] || TRANSLATIONS[LANG_ZH];
    var text = map[key];
    if (text === undefined) {
        text = TRANSLATIONS[LANG_ZH][key];
        if (text === undefined) return key;
    }
    if (params) {
        for (var p in params) {
            text = text.replace(new RegExp('\\{' + p + '\\}', 'g'), params[p]);
        }
    }
    return text;
}

/// 更新页面上所有 data-i18n 元素的文本
function applyLanguage() {
    var els = document.querySelectorAll('[data-i18n]');
    for (var i = 0; i < els.length; i++) {
        var el = els[i];
        var key = el.getAttribute('data-i18n');
        var text = _t(key);
        if (text !== key) {
            el.innerHTML = text;
        }
    }
    // 更新 placeholder
    var placeholders = document.querySelectorAll('[data-i18n-placeholder]');
    for (var i = 0; i < placeholders.length; i++) {
        var el = placeholders[i];
        var key = el.getAttribute('data-i18n-placeholder');
        el.placeholder = _t(key);
    }
    // 更新 value
    var values = document.querySelectorAll('[data-i18n-value]');
    for (var i = 0; i < values.length; i++) {
        var el = values[i];
        var key = el.getAttribute('data-i18n-value');
        el.value = _t(key);
    }
}
