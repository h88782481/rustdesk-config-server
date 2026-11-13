# rustdesk-config-server

自建 RustDesk 的 **配置中心 + 自定义更新服务器**,配套 [rdgen](../rdgen) 生成的客户端使用(RustDesk 1.5.1 及以后)。

- **远程下发 host / key / api**:patched 客户端启动时拉取 `GET /config`,之后每 60 秒后台刷新,写入内存隐藏层(设置页不显示),并加密缓存 last-good。面板修改 API 后,客户端下次刷新成功后的登录、通讯录和心跳请求会使用新地址,**无需重启或重新编译客户端**。config-server 挂掉重启仍用上次成功的 host/key/api。
- **应用内更新**:实现 RustDesk 1.5.1+ 的签名更新协议(与 rdgen 上游 `allow_custom_update.py` 配套)。面板填版本号和安装包直链,点「同步」由服务器流式下载算出大小和 sha256(不落盘);清单用本服务生成的 ed25519 私钥签名,客户端只信任编译进去的公钥。Windows / macOS 在应用内下载安装,Linux / Android 点「下载」用浏览器下载面板里的直链。
- **带 Token**:客户端接口(配置、版本检查、清单、安装包)需 `Authorization: Bearer <CLIENT_TOKEN>`,管理面板需密码登录;只有浏览器打开的下载跳转是公开的。
- 技术栈:Rust + Axum 0.8 + Tokio,数据存 JSON 文件(数据量极小,无需数据库),单二进制,Docker 部署。

> 说明:客户端侧能力在**编译期**打进去:上游 `allow_custom_update.py` 负责更新检查地址、信任公钥和放开定制客户端的更新检查,`rdgen/.github/patches/rdpatch.py` 负责远程配置、请求带 token 和更新入口。官方原版客户端不会请求本服务;用旧版补丁生成的客户端请求的是已移除的 `/version/latest`,需要用新版重新生成并手动安装一次。

---

## 一、快速部署(Docker,镜像来自 GHCR)

镜像由本仓库的 GitHub Actions 构建并推送到 `ghcr.io/<owner>/rustdesk-config-server`。
**仅在推送 `v*` 版本 tag 时触发**(也可在 Actions 页手动 `workflow_dispatch`):

```bash
git tag v0.1.0
git push origin v0.1.0
```

tag `v0.1.0` 会生成镜像标签 `0.1.0`、`0.1`、`latest`。

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
      - ./data:/data      # config.json 与更新签名私钥 update_ed25519.key,请备份
    restart: unless-stopped
```

启动:

```bash
docker compose up -d
```

浏览器打开 `http://<服务器IP>:21120/`,用 `ADMIN_PASSWORD` 登录。

### 部署要求

客户端只通过 HTTPS 访问本服务,对更新地址也有严格要求:

- 放在反向代理后,用**有效证书的 HTTPS、默认 443 端口、域名根路径**对外提供,例如 `https://cfg.example.com`(小写,不带端口和路径)。客户端只接受不带端口的更新地址。
- 反向代理须透传 `Host` 头(Caddy、Traefik 默认透传;nginx 需 `proxy_set_header Host $host;`)。清单里的安装包地址按 Host 生成,必须与客户端编译进去的地址一致。
- 数据目录里的 `update_ed25519.key` 是更新签名私钥,首次启动自动生成。**务必备份**:丢失后已发出的客户端无法再验证新版本,只能重新生成客户端并手动安装。
- 把 `COOKIE_SECURE=1`,让会话 cookie 只走 HTTPS。

---

## 二、环境变量

| 变量 | 必填 | 默认 | 说明 |
|------|------|------|------|
| `CLIENT_TOKEN` | ✅ | — | 客户端接口 Bearer token,必须与 rdgen 表单里的 **Config Server Token** 一致 |
| `ADMIN_PASSWORD` | ✅ | — | 管理面板登录密码 |
| `BIND_ADDR` | | `0.0.0.0:21120` | 监听地址 |
| `DATA_DIR` | | `/data` | 数据目录(`config.json`、更新签名私钥 `update_ed25519.key`) |
| `COOKIE_SECURE` | | `0` | 为 `1` 时会话 cookie 带 `Secure`(需 HTTPS) |

---

## 三、接口一览

### 客户端接口(需 `Authorization: Bearer <CLIENT_TOKEN>`)

| 方法 | 路径 | 说明 |
|------|------|------|
| `GET` | `/config` | 返回 `{ "host": "...", "key": "...", "api": "..." }` |
| `POST` | `/api/version/latest` | 版本检查,返回 `{ "url": "https://<域名>/rustdesk/rustdesk/releases/tag/v<版本号>" }`;没有可用更新时 `url` 为空 |
| `GET` | `/rustdesk/rustdesk/releases/download/v<版本号>/rustdesk-update.json` | 更新清单,收录该版本所有已同步的安装包 |
| `GET` | `/rustdesk/rustdesk/releases/download/v<版本号>/rustdesk-update.json.sig` | 清单签名(ed25519,`key_id` 为 `custom-update`) |
| `GET` | `/rustdesk/rustdesk/releases/download/v<版本号>/<文件名>` | 跳转到清单里对应的安装包直链 |

版本检查按客户端上报的 `os` 归到 windows / macos / linux / android,该平台开启且有版本号时:Windows / macOS 还需要上报架构(`arch`)的安装包已同步,Linux / Android 还需要填了下载直链。客户端从返回地址解析版本号,比自己新才提示更新。

### 公开接口(浏览器打开,不带 token)

| 方法 | 路径 | 说明 |
|------|------|------|
| `GET` | `/download/<平台>` | 平台为 `windows` / `macos` / `linux` / `android`,跳转到该平台的下载直链(客户端的「下载」按钮) |
| `GET` | `/rustdesk/rustdesk/releases/tag/v<版本号>` | 按浏览器 User-Agent 跳转到对应平台的下载直链(应用内更新失败时客户端提示打开的链接) |
| `GET` | `/healthz` | 健康检查 |

Windows / macOS 的下载直链取第一个填了的安装包(x86_64 优先),Linux / Android 用面板里的下载直链。

### 管理接口(需登录会话 cookie)

| 方法 | 路径 | 说明 |
|------|------|------|
| `POST` | `/admin/login` | body `{ "password": "..." }`,成功后下发会话 cookie |
| `GET` | `/admin/api/state` | 读取当前全部配置和更新公钥 |
| `POST` | `/admin/api/server` | 保存 host/key/api |
| `POST` | `/admin/api/pan123` | body `{ "auth_key": "..." }`,保存 123 云盘直链鉴权密钥 |
| `POST` | `/admin/api/update` | 保存某平台:开关、版本号、Linux / Android 下载直链、Windows / macOS 各架构安装包直链,以及每个直链的 123 鉴权开关;版本号或直链变了的安装包需重新同步 |
| `POST` | `/admin/api/sync` | body `{ "platform": "windows", "arch": "x86_64" }`,下载该安装包直链并保存大小和 sha256 |
| `POST` | `/admin/api/logout` | 退出登录 |

---

## 四、配合 rdgen 生成客户端

1. 先按上面的部署要求部署好本服务,记下 `CLIENT_TOKEN` 和外部访问地址(如 `https://cfg.example.com`)。
2. 在面板「服务端设置」里填好 host / key / api;在「更新设置」顶部复制**更新公钥**。
3. 打开 rdgen 生成页,在 **Remote Config Center** 一栏填:
   - **Config Server URL** = `https://cfg.example.com`
   - **Config Server Token** = 与 `CLIENT_TOKEN` 相同
   - **Update Public Key** = 面板上的更新公钥(留空则只用远程配置,不启用应用内更新)
   - **Custom App Version**(可选)= 用同一个 RustDesk 版本重新生成并推送更新时填,如 `1.5.3+1`
4. 选择 RustDesk 1.5.1 及以后的版本(或 nightly),生成。

生成的客户端行为:

- 启动时先 `GET /config`:拉到则直接用,并**加密**写入 `LocalConfig` `_rdc`(设置页不显示;机器 UUID + Config Token 绑定)。
- 启动拉取失败才解密本地 last-good;从未成功拉过才退回编译期 Host/Key/API。
- 随后每 60 秒在后台拉取一次(每个进程只启动一个刷新线程,单次请求超时 6 秒),不阻塞界面、不弹提示。刷新失败保留当前配置,下个周期重试;配置未变化时不重复写缓存或输出成功日志。
- API 更新后供后续登录、通讯录和心跳请求使用;不强制中断已有会话或正在进行的请求。host/key 同步更新到隐藏层,已有 ID 服务器连接在重连时使用新值。空字段沿用当前值,不清空已有配置。
- 填了 Update Public Key 时,启动后及定期请求 `/api/version/latest`,有新版本:
  - Windows 64 位 / macOS(已安装):首页提示更新,点「更新」在应用内下载,校验签名、大小和 sha256 后安装。
  - Linux、Android、未安装的 Windows / macOS:提示有新版本,点「下载」用浏览器打开 `/download/<平台>`。
  - Windows 32 位(Sciter):提示有新版本,需开启「自动更新」。
  - 后台静默更新:Windows 已安装且开启「自动更新」(默认关闭,可在 rdgen 的默认设置里加 `allow-auto-update=Y`)时自动下载安装;macOS 定制客户端没有后台静默更新。

> 注意:启用 **Config Server** 时,不要在 **手动 override/default** 里写 `custom-rendezvous-server` / `api-server` / `key`(会进 OVERWRITE_SETTINGS,优先级高于运行时下发)。host / key / api 交给面板下发即可;其它品牌化字段(appname、图标、公司名、urlLink 等)不受影响,可正常使用。

---

## 五、发布新版本

1. 用 rdgen 生成各平台客户端,把安装包放到能直链下载的地方(直链需长期有效)。
2. 面板「更新设置」里,每个平台:
   - 打开开关,版本号填这次生成的客户端版本(rdgen 选的 RustDesk 版本,填了 Custom App Version 就用它)。**必须与客户端实际版本一致**,否则装完新版仍会提示更新。
   - Windows / macOS:每个架构填安装包直链,点「同步」(会先保存)。只有同步过的安装包才会推送;改了版本号或直链要重新同步。Windows 填 `.exe`(不是 `.msi`),32 位对应 rdgen 的 Windows 32Bit;macOS 填 `.dmg`。
   - Linux / Android:填下载直链(支持 `{version}`),保存。
   - 直链在开了直链鉴权的 123 云盘上时:先在「更新设置」顶部填 123 云盘鉴权密钥并保存,再打开该直链的「123 鉴权」开关。直链填 123 云盘复制的原始地址(不带 `auth_key`),同步和每次下载跳转时由服务端现签,有效期 15 分钟,UID 取直链路径的第一段。
3. 版本号比较:`1.5.3+1` 比 `1.5.3` 新。构建号会直接加到版本数值上,`1.5.3+12` 会被当成比 `1.5.4` 还新,所以构建号用 1–9。

---

## 六、本地构建(可选)

无需本地环境即可用 CI/Docker 构建。如需本地:

```bash
CLIENT_TOKEN=dev-token ADMIN_PASSWORD=dev-pass DATA_DIR=./data cargo run --release
```
