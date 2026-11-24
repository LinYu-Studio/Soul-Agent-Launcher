# SAL 更新服务部署说明

> 目标：`https://sal.bszx.site` 同时承载官网与自动更新服务。
> 最后更新：2026-11-25

## 架构

```
                         Caddy (443, 自动 Let's Encrypt 证书)
                                     │
              ┌──────────────────────┴───────────────────────┐
       /api/*  /downloads/*                         其余路径 /
       /admin* /health                                        │
              │                                                │
     sal-update.service                              sal-web.service
     127.0.0.1:7890                                  127.0.0.1:5001
     /srv/sal/server.py                              /srv/sal/app.py
     （更新 API / 下载 / 管理后台）                  （官网 / 教程 / 下载引导）
```

| 组件 | 位置 |
|------|------|
| 站点根目录 | `/srv/sal/`（属主 `yuki:yuki`） |
| Python 环境 | `/srv/sal/venv`（flask 3.1.x） |
| 安装包目录 | `/srv/sal/downloads/` |
| 版本元数据 | `/srv/sal/updates/current.json` |
| 管理密码 | `/srv/sal/config.json`（权限 600，**不入库**） |
| systemd | `sal-update.service` / `sal-web.service` |
| Caddy | `/etc/caddy/Caddyfile` 末段 `sal.bszx.site { ... }` |

## 首次部署 / 重新部署

```bash
# 1) 目录与依赖
ssh hk "mkdir -p /srv/sal/{downloads,updates,templates,static} && python3 -m venv /srv/sal/venv && /srv/sal/venv/bin/pip install -q flask"

# 2) 代码与资源（在本地 WebSite/ 目录执行）
scp server.py app.py config.json hk:/srv/sal/
scp templates/*.html hk:/srv/sal/templates/
scp static/* hk:/srv/sal/static/
scp updates/current.json hk:/srv/sal/updates/
ssh hk "chown -R yuki:yuki /srv/sal && chmod 600 /srv/sal/config.json"

# 3) 重启服务
ssh hk "systemctl restart sal-update sal-web && systemctl status --no-pager -l sal-update sal-web | head -20"
```

> 首次部署还需写 systemd 单元与 Caddy 配置，见仓库历史提交（`Caddyfile` 中 `sal.bszx.site` 段）。

## 发布新版本（0.4.2 示例）

1. 本地出包：`cargo tauri build`（产物在 `F:/sal_target/release/bundle/msi/*.msi`）
2. 打开 https://sal.bszx.site/admin 登录
3. 填版本号 `0.4.2`、更新说明，选择 MSI 上传 → 提交
   - 服务端自动计算 SHA256 并写入 `current.json`（客户端安装前会校验）
   - 文件名自动归一化（空格→下划线，仅保留安全字符）
4. 客户端下次启动 5 秒后检查更新 → 弹出更新卡片 → 一键下载安装

## 客户端联动

- 客户端常量：`src/js/app.js` 的 `UPDATE_API_URL = 'https://sal.bszx.site/api/check-update'`
- 客户端发送自身版本（如 `0.4.2`），服务端用「取前三段数字」的方式比较，`-beta` 等后缀安全
- 服务端返回的 `download_url` 是相对路径（`/downloads/xxx.msi`），客户端自动补全域名

## 常用排查

```bash
ssh hk "journalctl -u sal-update -n 50 --no-pager"   # 更新服务日志
ssh hk "journalctl -u sal-web -n 50 --no-pager"      # 官网日志
ssh hk "tail -50 /var/log/caddy/sal.access.log"      # 访问日志
ssh hk "curl -s https://sal.bszx.site/health"        # 健康检查
ssh hk "cat /srv/sal/updates/current.json"           # 当前发布的版本
```

## 安全注意

- `config.json` 含管理密码 hash，**切勿提交到 Git**（已加入 `.gitignore`）
- 源码不再内置任何默认密码；无配置时启动会生成随机密码并打印到日志（`journalctl -u sal-update` 可见）
- 修改密码：编辑 `/srv/sal/config.json` 的 `admin_password_hash`（sha256(明文)），然后
  `ssh hk "systemctl restart sal-update"`

## 回滚

```bash
# Caddy 配置已自动备份为 /etc/caddy/Caddyfile.bak-pre-sal-*
ssh hk "cp /etc/caddy/Caddyfile.bak-pre-sal-<时间戳> /etc/caddy/Caddyfile && systemctl reload caddy"
ssh hk "systemctl disable --now sal-update sal-web"
```
