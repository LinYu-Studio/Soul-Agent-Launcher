# Soul Agent Launcher (SAL)

一款面向 Windows 的本地 AI 大模型一键部署与管理工具。基于 llama.cpp 后端，提供模型下载、服务管理、对话交互的全链路解决方案。

---

## 项目简介

Soul Agent Launcher 致力于让本地 AI 模型的部署像安装普通软件一样简单。自动检测硬件配置，匹配最优加速后端，一键下载并启动 GGUF 格式模型。无需手动配置编译环境，无需编写命令行参数。

---

## 核心特性

### 模型管理

- **官方模型仓库**：内置 Qwen / DeepSeek / GLM / Yi / Google / Llama 六大系列 60+ 个官方模型，按分组展示，支持量化版本选择
- **模型搜索**：通过 ModelScope API 搜索社区模型
- **一键下载**：集成 modelscope CLI，支持断点续传，下载完成后自动 SHA256 校验
- **下载重试**：失败时显示重试按钮（非自动无限重试），校验失败自动重试最多 3 次
- **GGUF 过滤**：自动识别 GGUF 格式文件，非 GGUF 模型下载前弹窗确认

### 服务部署

- **一键启动**：选择模型后一键启动 llama.cpp 服务
- **多模型并发**：支持同一时间启动多个模型，每个模型独占端口
- **API 兼容**：同时提供原生 `/chat` 接口和 OpenAI 兼容的 `/v1/chat/completions` 接口
- **参数配置**：自定义端口、上下文长度、GPU 层数等启动参数
- **智能空闲管理**：检测到服务空闲时自动休眠，释放系统资源

### 对话交互

- **极简对话（SA Lite）**：轻量级聊天界面，支持流式输出、深度思考、多模态输入
- **会话管理**：创建、重命名、删除会话，对话历史本地持久化
- **上下文裁剪**：自动裁剪超长上下文，支持会话摘要注入
- **模型输出净化**：自动剥离 `<|im_end|>`、`</s>` 等模型特殊 token（仅从末尾剥离，不影响内容）
- **超时保护**：流式读取 15 秒无新数据自动结束（无输出时 60 秒超时）

### 环境自动检测

首次启动时自动执行 10 步检测流程：
1. 语言检测
2. 主题加载
3. 配置恢复
4. 硬件检测（GPU / CPU） + 后端同步
5. llama.cpp 版本校验
6. Python 检测（支持离线静默安装 Python 3.11）
7. pip 检测
8. ModelScope 安装
9. 加载设置
10. 版本检查

每步失败均可跳过或重试，关键步骤失败时生成错误报告。

### 系统集成

- **系统托盘**：右键菜单支持显示窗口 / 查看日志 / 退出
- **日志记录**：结构化日志（TXT + JSON），记录 CPU / 内存 / GPU 显存资源快照
- **资源监控**：通过 nvidia-smi 自动检测 GPU 型号和显存占用

---

## 快速开始

### 下载安装

访问 [sal.bszx.site](https://sal.bszx.site) 下载 MSI 安装包，或通过 GitHub Releases 下载。

### 首次启动

1. 安装后首次启动，自动弹出环境检测窗口
2. 确认 Python 和 modelscope 环境正常（如缺失会自动安装）
3. 进入首页后，点击"模型"标签页
4. 在"官方模型"中选择需要的模型，点击下载
5. 下载完成后切换到"启动"标签页，选择模型并点击"启动服务"
6. 切换到"极简对话"开始对话

---

## 技术架构

```
┌─────────────────────────────────────────────┐
│              Tauri Desktop App               │
│  ┌──────────┐    ┌───────────────────────┐  │
│  │  Frontend │◄──►│   Rust Backend        │  │
│  │ (HTML/JS) │    │   (main.rs)           │  │
│  └──────────┘    │                       │  │
│                  │  ├─ Server Manager     │  │
│                  │  ├─ Model Downloader   │  │
│                  │  ├─ Chat Proxy         │  │
│                  │  ├─ Hardware Detector  │  │
│                  │  └─ Session Manager    │  │
│                  └───────────┬───────────┘  │
└──────────────────────────────┼──────────────┘
                               │
                    ┌──────────▼──────────┐
                    │   llama-server.exe   │
                    │  (llama.cpp backend) │
                    └──────────┬──────────┘
                               │
                    ┌──────────▼──────────┐
                    │   ModelScope CLI     │
                    │  (model download)    │
                    └─────────────────────┘
```

### 前端 (src/)

- `index.html` - 应用主界面，含首页 / 模型 / 启动 / 会话 / 极简对话 / 设置六个页面
- `js/app.js` - 前端核心逻辑，约 2500 行
- `js/lang.js` - 国际化语言文件
- `css/style.css` - Win11 云母亚克力主题样式

### 后端 (src-tauri/)

- `src/main.rs` - Rust 后端，约 4100 行，含所有 Tauri 命令
- `Cargo.toml` - Rust 依赖管理
- `tauri.conf.json` - Tauri 应用配置

### 官网 (WebSite/)

- `landing/index.html` - 营销官网（含对比表格、功能介绍）
- `landing/app.py` - Flask 服务，提供官网路由
- `landing/download.html` - 下载引导页面

---

## 官方网站

| 用途 | 地址 |
|------|------|
| 官网首页 | [https://sal.bszx.site](https://sal.bszx.site) |
| 下载页面 | [https://sal.bszx.site/download-page](https://sal.bszx.site/download-page) |
| 使用教程 | [https://sal.bszx.site/tutorial](https://sal.bszx.site/tutorial) |
| 更新服务器 | `https://sal.bszx.site/api/check-update` |

---

## 模型支持列表

### Qwen 系列（通义千问）
Qwen2.5-0.5B / 1.5B / 3B / 7B / 14B / 32B / 72B，Qwen3-0.6B / 1.7B / 4B / 8B / 14B / 32B / 235B，QwQ-32B

### DeepSeek 系列
DeepSeek-R1-Distill-Qwen-1.5B / 7B / 14B / 32B，DeepSeek-R1-Distill-Llama-8B，DeepSeek-Coder-V2-Lite-Instruct

### GLM 系列（智谱）
ChatGLM2-6B，ChatGLM3-6B，GLM-4-9B / 9B-Chat / 9B-Chat-1M / 4V-9B-Chat，GLM-Z1-9B-0414，GLM-4-32B-0414 / 32B-Base-0414，GLM-Z1-32B-0414，GLM-Z1-Rumination-32B-0414

### Yi 系列（零一万物） ————目前暂未收录GGUF格式版本
Yi-1.5-6B / 9B / 34B（Base + Chat），Yi-Coder-1.5B / 9B，Yi-VL-6B / 34B

### Google 系列
Gemma-2-2B / 9B / 27B

### Llama 系列
Meta-Llama-3-8B / 70B，Llama-3.2-1B / 3B，Llama-3.1-8B / 70B

---

## 资源占用

| 工具 | 空载内存 | 启动 7B 模型 | 安装包大小 |
|------|---------|-------------|-----------|
| SAL | 135 MB | 3-10 秒 | 5 MB / 458 MB |
| LM Studio | ~1.4 GB | 10-30 秒 | ~500 MB |
| GPT4All | ~700 MB | 1-4 分钟 | ~200 MB |
| Ollama | 200 MB | 15-30 秒 | ~1.4 GB |

> 以上数据基于公开测试。测试环境：Windows 11 24H2, i7-12700H, RTX 5060 Laptop, 模型 Qwen2.5-7B-Q4_K_M。不同环境结果可能略有差异。

---

## 开发构建

### 环境要求

- Rust 1.70+
- Node.js 18+
- Windows 10/11（目前仅支持 Windows）

### 构建步骤

```bash
# 克隆仓库
git clone https://github.com/LinYu-Studio/Soul-Agent-Launcher.git
cd Soul-Agent-Launcher

# 构建 MSI 安装包
cd src-tauri
cargo tauri build --bundles msi
```

构建产物位于 `src-tauri/target/release/bundle/msi/`。

---

## 开源协议

GPL v3
