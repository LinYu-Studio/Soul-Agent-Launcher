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
        'nav.knowledge': '知识库',
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
        'settings.quit_on_close': '关闭窗口时退出 SAL',
        'settings.auto_launch': '开机自启动（托盘模式）',
        'settings.restore_models': '启动时自动恢复上次运行的模型',
        'settings.quit_on_close_hint': '默认开启：点击右上角 ✕ 直接退出 SAL 并释放内存/显存；关闭后 ✕ 仅隐藏到托盘继续常驻',
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

        /* 静态 UI 文案（index.html 自动标注） */
        'ui.001': "正在完成首次配置...",
        'ui.002': "出错了",
        'ui.003': "跳过",
        'ui.004': "重试",
        'ui.005': "关键错误",
        'ui.006': "硬件检测完全失败，无法继续。",
        'ui.007': "复制报告",
        'ui.008': "提交反馈",
        'ui.009': "退出应用",
        'ui.010': "正在检测系统环境...",
        'ui.011': "欢迎使用 Soul Agent Launcher",
        'ui.012': "请选择您的使用模式",
        'ui.013': "无需本地部署AI教学，自由探索SAL",
        'ui.014': "无AI部署经验，最少3步快速部署本地AI",
        'ui.015': "确认",
        'ui.016': "不再提示",
        'ui.017': "正在扫描您的硬件以匹配最佳模型",
        'ui.018': "检测中...",
        'ui.019': "为您推荐以下模型",
        'ui.020': "根据您的硬件配置，推荐最适合的模型",
        'ui.021': "取消",
        'ui.022': "确定",
        'ui.023': "● 已断开",
        'ui.024': "管理您的本地 AI 模型服务和启动器。",
        'ui.025': "双端点 API:",
        'ui.026': "启动服务",
        'ui.027': "停止服务",
        'ui.028': "就绪",
        'ui.029': "自动检测硬件，推荐最佳模型",
        'ui.030': "一键部署",
        'ui.031': "未测试",
        'ui.032': "评估本地硬件 AI 推理性能",
        'ui.033': "开始跑分",
        'ui.034': "历史",
        'ui.035': "反馈",
        'ui.036': "设置路径（在设置中配置，首次自动完成）",
        'ui.037': "下载或添加 GGUF 模型到模型目录",
        'ui.038': "在「启动服务」选择模型并一键启动",
        'ui.039': "双端点：/chat（自研轻量）· /v1/chat/completions（OpenAI 兼容）",
        'ui.040': "Soul Agent 及其他 OpenAI 客户端可直接连接",
        'ui.041': "点击查看",
        'ui.042': "了解 SAL 的完整使用方法",
        'ui.043': "查看教程",
        'ui.044': "模型管理",
        'ui.045': "刷新",
        'ui.046': "导入",
        'ui.047': "管理本地模型或从魔搭社区搜索下载。",
        'ui.048': "搜索",
        'ui.049': "输入关键词开始搜索魔搭社区模型",
        'ui.050': "Soul Agent Launcher 官方模型",
        'ui.051': "官方推荐的 GGUF 模型，由 Soul Agent 团队精选和优化，点击即可下载。",
        'ui.052': "加载中...",
        'ui.053': "选择模型并启动推理服务。",
        'ui.054': "模型文件",
        'ui.055': "暂无模型，请先在「模型」页添加 GGUF 模型",
        'ui.056': "起始端口",
        'ui.057': "上下文长度",
        'ui.058': "推理服务",
        'ui.059': "未启动",
        'ui.060': "端口 20000 · 已停止",
        'ui.061': "在设置中配置 llama.cpp 路径和模型目录（首次自动安装）",
        'ui.062': "在「模型」页面下载或添加 GGUF 模型文件",
        'ui.063': "回到本页选择模型，点击「启动服务」即可运行",
        'ui.064': "Soul Agent 及其他 OpenAI 客户端可直接连接使用",
        'ui.065': "已启动模型",
        'ui.066': "卸载选中",
        'ui.067': "全部卸载",
        'ui.068': "暂无运行中的模型",
        'ui.069': "控制台输出",
        'ui.070': "等待启动...",
        'ui.071': "&nbsp;&nbsp;默认会话",
        'ui.072': "+ 新建",
        'ui.073': "启动服务后即可开始对话",
        'ui.074': "0% - 0.00K/2.00K 已使用",
        'ui.075': "深度思考",
        'ui.076': "工具调用",
        'ui.077': "多模态",
        'ui.078': "知识库",
        'ui.079': "管理知识文档，在对话中自动检索相关内容。",
        'ui.080': "嵌入服务",
        'ui.081': "嵌入模型：all-MiniLM-L6-v2 Q4_K_M (23.5MB, 384维)",
        'ui.082': "启动",
        'ui.083': "停止",
        'ui.084': "下载模型",
        'ui.085': "已添加的文件夹",
        'ui.086': "重新索引全部",
        'ui.087': "暂无文件夹，点击下方添加",
        'ui.088': "浏览",
        'ui.089': "添加",
        'ui.090': "拖拽文件（单次查阅）",
        'ui.091': "清除临时文件",
        'ui.092': "拖拽文件到此处，或点击选择文件",
        'ui.093': "支持 .txt .md .json .py .js .rs .csv 等文本格式",
        'ui.094': "搜索知识库",
        'ui.095': "设置",
        'ui.096': "配置 Soul Agent Launcher 的各项参数。",
        'ui.097': "服务路径",
        'ui.098': "llama.cpp 可执行文件路径",
        'ui.099': "选择",
        'ui.100': "模型目录",
        'ui.101': "默认参数",
        'ui.102': "默认端口",
        'ui.103': "默认上下文长度",
        'ui.104': "空闲自动待机",
        'ui.105': "关闭",
        'ui.106': "5 分钟",
        'ui.107': "10 分钟",
        'ui.108': "30 分钟",
        'ui.109': "空闲后自动释放显存，下次请求自动唤醒（约数秒~数十秒）",
        'ui.110': "mmproj 路径（多模态模型可选）",
        'ui.111': "多模态模型启动时自动附加 --mmproj；待机唤醒后原样恢复。注意：此路径会附加到所有启动的模型，混用纯文本模型时请勿填写",
        'ui.112': "极致精简编译",
        'ui.113': "从源码编译极致精简版 llama-server",
        'ui.114': "开始编译",
        'ui.115': "外观",
        'ui.116': "主题模式",
        'ui.117': "亮色模式",
        'ui.118': "行为",
        'ui.119': "系统启动时在后台运行（最小化到托盘），立即生效",
        'ui.120': "记住关闭前正在运行的模型，下次启动自动重新加载",
        'ui.121': "保存设置",
        'ui.122': "检查更新",
        'ui.123': "会话",
        'ui.124': "管理对话历史记录。",
        'ui.125': "历史会话",
        'ui.126': "新建会话",
        'ui.127': "暂无会话，点击「新建会话」开始。",
        'ui.128': "确认下载",
        'ui.129': "正在下载模型",
        'ui.130': "请稍候...",
        'ui.131': "准备中...",
        'ui.132': "取消下载",
        'ui.133': "重试下载",
        'ui.134': "导入模型",
        'ui.135': "选中文件: -",
        'ui.136': "模型名称",
        'ui.137': "确认导入",
        'ui.138': "快速启动模型",
        'ui.139': "选择要启动的模型，支持多选",
        'ui.140': "暂无本地模型，请先下载",
        'ui.141': "快速启动",
        'ui.142': "隐私政策",
        'ui.143': "Soul Agent Launcher（SAL）是一款完全离线运行的本地工具。",
        'ui.144': "使用教程",
        'ui.145': "选择\"一键自动部署\"自动完成硬件检测、模型推荐和下载，适合新手。",
        'ui.146': "自动部署后点击左侧会话菜单新建会话即可开始对话",
        'ui.147': "点击主页\"启动服务\"→ 弹出快速启动弹窗 → 勾选要启动的模型 → 点击\"快速启动\"。",
        'ui.148': "Tips: 日常使用建议直接使用 SA Lite 在\"会话\"菜单进行对话即可（无 Agent 编码能力）",
        'ui.149': "在\"模型\"页可以下载官方推荐模型（GGUF 格式），或从本地导入已有模型文件。",
        'ui.150': "在\"设置\"页配置 llama.cpp 路径和模型目录。首次启动时会自动检测和安装依赖。",
        'ui.151': "支持模型的深度思考（Reasoning）能力，在对话界面可开启\"深度思考\"开关查看推理过程。",
        'ui.152': "知道了",
        'ui.153': "选择跑分模式",
        'ui.154': "快速跑分",
        'ui.155': "推荐",
        'ui.156': "自定义跑分",
        'ui.157': "下一步",
        'ui.158': "选择测试模型",
        'ui.159': "选择一个模型进行跑分测试",
        'ui.160': "Prompt 长度",
        'ui.161': "短 (50 tokens)",
        'ui.162': "标准 (200 tokens)",
        'ui.163': "长 (500 tokens)",
        'ui.164': "测试次数",
        'ui.165': "1 次快速",
        'ui.166': "3 次标准 (去极值平均)",
        'ui.167': "5 次精确 (去极值平均)",
        'ui.168': "展开",
        'ui.169': "返回",
        'ui.170': "环境检测",
        'ui.171': "性能跑分中",
        'ui.172': "总体进度",
        'ui.173': "等待测试开始...",
        'ui.174': "取消测试",

        'ui.175': "本地模型",
        'ui.176': "魔搭社区",
        'ui.177': "知识库",
        'ui.178': "发送",
        'ui.179': "llama-server 或 llama-cli 的完整路径",
        'ui.180': "存放 GGUF 模型的文件夹",
        'ui.181': "可多选，每个选中的模型将独立启动",
        'ui.182': "导入后文件名锁定，不支持改名",
        'ph.1': "搜索 GGUF 模型（如 Qwen、DeepSeek、Llama）",
        'ph.2': "输入消息，Enter 发送，Shift+Enter 换行...",
        'ph.3': "输入文件夹路径或点击浏览...",
        'ph.4': "输入搜索关键词...",
        'ph.5': "视觉投影模型路径，留空 = 纯文本模式",
        'ph.7': "粘贴 GGUF 模型文件完整路径",
        'ph.8': "搜索全部会话内容，Enter 搜索...",
        'ph.9': "输入模型显示名称",

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
        'env.clean_temp': '清理临时文件',
        'env.temp_cleaned': '已清理 {count} 个临时文件（{size}MB）',
        'env.temp_clean_failed': '清理失败: ',

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
        'nav.knowledge': 'Knowledge Base',
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
        'settings.quit_on_close': 'Quit SAL when closing the window',
        'settings.auto_launch': 'Launch at Windows startup (tray mode)',
        'settings.restore_models': 'Restore last run models on startup',
        'settings.quit_on_close_hint': 'On by default: clicking ✕ quits SAL and frees memory/VRAM; turn off to keep it running in the tray',
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

        /* Static UI text (index.html) */
        'ui.001': "Finishing first-time setup…",
        'ui.002': "Something went wrong",
        'ui.003': "Skip",
        'ui.004': "Retry",
        'ui.005': "Critical Error",
        'ui.006': "Hardware detection failed completely. Unable to continue.",
        'ui.007': "Copy Report",
        'ui.008': "Send Feedback",
        'ui.009': "Quit",
        'ui.010': "Detecting system environment…",
        'ui.011': "Welcome to Soul Agent Launcher",
        'ui.012': "Choose how you'll use SAL",
        'ui.013': "No local AI setup needed — explore SAL freely",
        'ui.014': "No AI experience needed — deploy local AI in 3 steps",
        'ui.015': "Confirm",
        'ui.016': "Don't show again",
        'ui.017': "Scanning your hardware to match the best model",
        'ui.018': "Detecting…",
        'ui.019': "Recommended models for you",
        'ui.020': "Best-fit models based on your hardware",
        'ui.021': "Cancel",
        'ui.022': "OK",
        'ui.023': "● Disconnected",
        'ui.024': "Manage your local AI models and launcher.",
        'ui.025': "Dual endpoints:",
        'ui.026': "Start Service",
        'ui.027': "Stop Service",
        'ui.028': "Ready",
        'ui.029': "Auto-detect hardware and recommend the best model",
        'ui.030': "One-Click Deploy",
        'ui.031': "Not tested",
        'ui.032': "Benchmark local AI inference performance",
        'ui.033': "Run Benchmark",
        'ui.034': "History",
        'ui.035': "Feedback",
        'ui.036': "Set paths (configure in Settings; done automatically on first run)",
        'ui.037': "Download or add GGUF models to your model folder",
        'ui.038': "Pick a model in \"Launch\" and start it with one click",
        'ui.039': "Endpoints: /chat (lightweight) · /v1/chat/completions (OpenAI-compatible)",
        'ui.040': "Connect Soul Agent or any OpenAI-compatible client directly",
        'ui.041': "View",
        'ui.042': "Learn everything SAL can do",
        'ui.043': "View Tutorial",
        'ui.044': "Model Management",
        'ui.045': "Refresh",
        'ui.046': "Import",
        'ui.047': "Manage local models or search and download from ModelScope.",
        'ui.048': "Search",
        'ui.049': "Type a keyword to search ModelScope models",
        'ui.050': "Official Soul Agent Launcher Models",
        'ui.051': "Hand-picked GGUF models from the Soul Agent team. Click to download.",
        'ui.052': "Loading…",
        'ui.053': "Choose a model and start the inference service.",
        'ui.054': "Model File",
        'ui.055': "No models yet — add a GGUF model on the Models page first",
        'ui.056': "Start Port",
        'ui.057': "Context Length",
        'ui.058': "Inference Service",
        'ui.059': "Not running",
        'ui.060': "Port 20000 · stopped",
        'ui.061': "Set the llama.cpp path and model folder in Settings (auto-installed on first run)",
        'ui.062': "Download or add a GGUF model on the Models page",
        'ui.063': "Come back here, pick a model and hit \"Start Service\"",
        'ui.064': "Soul Agent and other OpenAI clients can connect directly",
        'ui.065': "Running Models",
        'ui.066': "Unload Selected",
        'ui.067': "Unload All",
        'ui.068': "No models running",
        'ui.069': "Console Output",
        'ui.070': "Waiting to start…",
        'ui.071': "Default Session",
        'ui.072': "+ New",
        'ui.073': "Start the service to begin chatting",
        'ui.074': "0% - 0.00K/2.00K used",
        'ui.075': "Deep Thinking",
        'ui.076': "Tool Calls",
        'ui.077': "Multimodal",
        'ui.078': "Knowledge Base",
        'ui.079': "Manage documents and retrieve relevant content automatically in chats.",
        'ui.080': "Embedding Service",
        'ui.081': "Embedding model: all-MiniLM-L6-v2 Q4_K_M (23.5MB, 384-dim)",
        'ui.082': "Start",
        'ui.083': "Stop",
        'ui.084': "Download Model",
        'ui.085': "Added Folders",
        'ui.086': "Reindex All",
        'ui.087': "No folders yet — add one below",
        'ui.088': "Browse",
        'ui.089': "Add",
        'ui.090': "Drag files (one-off lookup)",
        'ui.091': "Clear Temp Files",
        'ui.092': "Drop files here, or click to choose",
        'ui.093': "Supports .txt .md .json .py .js .rs .csv and other text formats",
        'ui.094': "Search Knowledge Base",
        'ui.095': "Settings",
        'ui.096': "Configure Soul Agent Launcher.",
        'ui.097': "Service Paths",
        'ui.098': "llama.cpp executable path",
        'ui.099': "Browse",
        'ui.100': "Model Directory",
        'ui.101': "Default Parameters",
        'ui.102': "Default Port",
        'ui.103': "Default Context Length",
        'ui.104': "Auto-Standby When Idle",
        'ui.105': "Off",
        'ui.106': "5 minutes",
        'ui.107': "10 minutes",
        'ui.108': "30 minutes",
        'ui.109': "Free VRAM when idle and wake automatically on the next request (seconds to tens of seconds)",
        'ui.110': "mmproj path (optional, for multimodal models)",
        'ui.111': "Multimodal models get --mmproj automatically and restore it after standby. Note: this path applies to every model you launch — leave it empty if you also run text-only models.",
        'ui.112': "Minimal Build",
        'ui.113': "Build a minimal llama-server from source",
        'ui.114': "Build Now",
        'ui.115': "Appearance",
        'ui.116': "Theme",
        'ui.117': "Light",
        'ui.118': "Behavior",
        'ui.119': "Run in the background at startup (tray only); takes effect immediately",
        'ui.120': "Remember running models and reload them next launch",
        'ui.121': "Save Settings",
        'ui.122': "Check for Updates",
        'ui.123': "Sessions",
        'ui.124': "Manage your chat history.",
        'ui.125': "History",
        'ui.126': "New Session",
        'ui.127': "No sessions yet — click \"New Session\" to start.",
        'ui.128': "Confirm Download",
        'ui.129': "Downloading Model",
        'ui.130': "Please wait…",
        'ui.131': "Preparing…",
        'ui.132': "Cancel Download",
        'ui.133': "Retry Download",
        'ui.134': "Import Model",
        'ui.135': "Selected file: -",
        'ui.136': "Model Name",
        'ui.137': "Confirm Import",
        'ui.138': "Quick Launch",
        'ui.139': "Choose models to launch (multi-select)",
        'ui.140': "No local models — download one first",
        'ui.141': "Quick Launch",
        'ui.142': "Privacy Policy",
        'ui.143': "Soul Agent Launcher (SAL) is a fully offline local tool.",
        'ui.144': "Tutorial",
        'ui.145': "Use \"One-Click Deploy\" to detect hardware, recommend and download a model automatically — great for beginners.",
        'ui.146': "After deploying, open the Sessions tab and create a new session to start chatting",
        'ui.147': "Click \"Start Service\" on Home → choose models in the Quick Launch dialog → click \"Quick Launch\".",
        'ui.148': "Tip: for daily use, chat via SA Lite in the Sessions tab (no agentic coding).",
        'ui.149': "Download official GGUF models on the Models page, or import existing model files locally.",
        'ui.150': "Set the llama.cpp path and model folder in Settings. Dependencies are detected and installed on first run.",
        'ui.151': "Supports model reasoning: enable the \"Deep Thinking\" switch in chat to see the reasoning process.",
        'ui.152': "Got it",
        'ui.153': "Choose Benchmark Mode",
        'ui.154': "Quick Benchmark",
        'ui.155': "Recommended",
        'ui.156': "Custom Benchmark",
        'ui.157': "Next",
        'ui.158': "Select Test Model",
        'ui.159': "Choose a model to benchmark",
        'ui.160': "Prompt Length",
        'ui.161': "Short (50 tokens)",
        'ui.162': "Standard (200 tokens)",
        'ui.163': "Long (500 tokens)",
        'ui.164': "Runs",
        'ui.165': "1 run (fast)",
        'ui.166': "3 runs (trimmed mean)",
        'ui.167': "5 runs (trimmed mean, accurate)",
        'ui.168': "Expand",
        'ui.169': "Back",
        'ui.170': "Environment Check",
        'ui.171': "Benchmarking",
        'ui.172': "Overall Progress",
        'ui.173': "Waiting for the test to start…",
        'ui.174': "Cancel Test",

        'ui.175': "Local Models",
        'ui.176': "ModelScope",
        'ui.177': "Knowledge Base",
        'ui.178': "Send",
        'ui.179': "Full path to llama-server or llama-cli",
        'ui.180': "Folder containing GGUF models",
        'ui.181': "Multi-select supported; each model starts independently",
        'ui.182': "Filename is locked after import",
        'ph.1': "Search GGUF models (e.g. Qwen, DeepSeek, Llama)",
        'ph.2': "Type a message. Enter to send, Shift+Enter for a new line…",
        'ph.3': "Enter a folder path or click Browse…",
        'ph.4': "Enter a search keyword…",
        'ph.5': "Vision projector path; leave empty for text-only",
        'ph.7': "Paste the full path of a GGUF model file",
        'ph.8': "Search all sessions, press Enter…",
        'ph.9': "Enter a display name for the model",

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
        'env.clean_temp': 'Clean Temp Files',
        'env.temp_cleaned': 'Removed {count} temp file(s) ({size}MB)',
        'env.temp_clean_failed': 'Cleanup failed: ',

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
