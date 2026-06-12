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
        'chat.api_error': '⚠️ API请求失败 (HTTP {status}){detail}',
        'chat.network_error': '⚠️ 网络请求失败: {msg}',
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
        'chat.api_error': '⚠️ API request failed (HTTP {status}){detail}',
        'chat.network_error': '⚠️ Network request failed: {msg}',
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
