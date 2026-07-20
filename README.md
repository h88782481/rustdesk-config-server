# rustdesk-config-server

自建 RustDesk 的 **配置中心 + 自定义更新服务器**,配套 [rdgen](../rdgen) 的 `rdpatch.py` 补丁使用。

- **远程下发 host / key / api**:patched 客户端启动时拉取 `GET /config`,写入本地配置,改面板即生效,**无需重新编译客户端**。
- **自定义更新**:patched 客户端请求 `POST /version/latest`,按平台返回你在面板设置的版本号和**直连下载链接**。
- **带 Token,不公开**:客户端接口需 `Authorization: Bearer <CLIENT_TOKEN>`;管理面板需密码登录。
- 技术栈:Rust + Axum 0.8 + Tokio,数据存 JSON 文件(数据量极小,无需数据库),单二进制,Docker 部署。

> 说明:更新检查 URL、下载逻辑、启动拉取配置这些能力是靠 `rdgen/.github/patches/rdpatch.py` 在**编译期**打进客户端的。官方原版客户端不会请求本服务。

---

## 一、快速部署(Docker,镜像来自 GHCR)

镜像由本仓库的 GitHub Actions 自动构建并推送到 `ghcr.io/<owner>/rustdesk-config-server`。

`docker-compose.yml`:

```yaml
services:
  rustdesk-config-server:
    image: ghcr.io/OWNER/rustdesk-config-server:latest   # 改成你的 GHCR 路径
    container_name: rustdesk-config-server
    environment:
      - CLIENT_TOKEN=换成一段足够长的随机字符串   # 必填,客户端内置同一值
      - ADMIN_PASSWORD=换成管理面板密码            # 必填
      - COOKIE_SECURE=0                            # 反代启用 HTTPS 时设为 1
      - TZ=Asia/Shanghai
    ports:
      - "21120:21120"
    volumes:
      - ./data:/data      # 配置持久化在 /data/config.json
    restart: unless-stopped
```

启动:

```bash
docker compose up -d
```

浏览器打开 `http://<服务器IP>:21120/`,用 `ADMIN_PASSWORD` 登录。

生产环境建议放在反向代理后启用 HTTPS(客户端拉取配置/更新走 HTTPS 更安全),并把 `COOKIE_SECURE=1`。

---

## 二、环境变量

| 变量 | 必填 | 默认 | 说明 |
|------|------|------|------|
| `CLIENT_TOKEN` | ✅ | — | 客户端接口 Bearer token,必须与 rdgen 表单里的 **Config Server Token** 一致 |
| `ADMIN_PASSWORD` | ✅ | — | 管理面板登录密码 |
| `BIND_ADDR` | | `0.0.0.0:21120` | 监听地址 |
| `DATA_DIR` | | `/data` | 配置文件目录(`config.json`) |
| `COOKIE_SECURE` | | `0` | 为 `1` 时会话 cookie 带 `Secure`(需 HTTPS) |

---

## 三、接口一览

### 客户端接口(需 `Authorization: Bearer <CLIENT_TOKEN>`)

| 方法 | 路径 | 说明 |
|------|------|------|
| `GET` | `/config` | 返回 `{ "host": "...", "key": "...", "api": "..." }` |
| `POST` | `/version/latest` | 请求体为 rustdesk 的版本检查结构;返回 `{ "url": "<直连下载链接>", "version": "<版本号>" }` |

`/version/latest` 逻辑:按客户端上报的 `os` 归类到 windows/macos/linux/android,匹配该平台的更新记录;
若该平台 **已启用** 且版本号、下载链接均非空,则返回;否则返回空 `url`(客户端视为无更新)。
下载链接支持占位符 `{version}`、`{arch}`(如 `x86_64` / `aarch64`),运行时替换。

### 管理接口(需登录会话 cookie)

| 方法 | 路径 | 说明 |
|------|------|------|
| `POST` | `/admin/login` | body `{ "password": "..." }`,成功后下发会话 cookie |
| `GET` | `/admin/api/state` | 读取当前全部配置 |
| `POST` | `/admin/api/server` | 保存 host/key/api |
| `POST` | `/admin/api/update` | 保存某平台的更新记录 |
| `POST` | `/admin/api/logout` | 退出登录 |

`GET /healthz` 为健康检查,公开。

---

## 四、配合 rdgen 生成客户端

1. 先部署好本服务,记下 `CLIENT_TOKEN` 和外部访问地址(如 `https://cfg.example.com`)。
2. 在面板「服务端设置」里填好 host / key / api;在「更新设置」里给需要的平台填版本号 + 下载链接并开启。
3. 打开 rdgen 生成页,在 **Remote Config Center** 一栏填:
   - **Config Server URL** = `https://cfg.example.com`
   - **Config Server Token** = 与 `CLIENT_TOKEN` 相同
4. 选择平台/版本,生成。CI 会用 `rdpatch.py` 把上述地址和 token 编译进客户端。

生成的客户端行为:

- 启动时从 `/config` 拉取 host/key/api 并覆盖本地设置(拉取失败则用编译期默认值兜底,不影响启动)。
- 定期 / 手动检查更新时请求 `/version/latest`,若有新版本则按返回的直连链接下载安装。

> 注意:启用 **Config Server** 时,不要再:
> - 勾选 **removeNewVersionNotif**(它会删掉更新检查逻辑,与自定义更新冲突);
> - 填 **Custom URL for downloading new versions (downloadLink)**(它会把下载地址 sed 成静态值,与「服务器返回直连链接」冲突);
> - 在 **手动 override/default** 里写 `custom-rendezvous-server` / `api-server` / `key`(会进 OVERWRITE_SETTINGS,优先级高于运行时下发)。
>
> host / key / api 交给面板下发即可;其它品牌化字段(appname、图标、公司名、urlLink 等)不受影响,可正常使用。

---

## 五、本地构建(可选)

无需本地环境即可用 CI/Docker 构建。如需本地:

```bash
CLIENT_TOKEN=dev-token ADMIN_PASSWORD=dev-pass DATA_DIR=./data cargo run --release
```
