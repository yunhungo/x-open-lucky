# QNAP 运维

## 运行时参数

| 环境变量 | 默认值 | 用途 |
| --- | --- | --- |
| `OPENLUCKY_DATA` | `data` | SQLite、加密密钥、初始化密钥、证书 |
| `OPENLUCKY_FRONTEND` | `frontend/dist` | 静态前端路径 |
| `OPENLUCKY_ADMIN_ADDR` | 本地开发 `0.0.0.0:56002`；QNAP `192.168.1.85:56002` | 管理监听 |
| `OPENLUCKY_HTTP_ADDR` | `[::]:58081` | HTTP 代理监听 |
| `OPENLUCKY_HTTPS_ADDR` | `[::]:58444` | HTTPS 代理监听 |
| `OPENLUCKY_PUBLIC_HTTPS_PORT` | `58444` | HTTP 跳转使用的外部 HTTPS 端口 |
| `OPENLUCKY_SECURE_COOKIE` | `false` | HTTPS 管理入口设为 `true` |
| `OPENLUCKY_LEGO_BIN` | `lego` | ACME 可执行文件 |
| `OPENLUCKY_ACCESS_DAYS` | `7` | 访问明细保留 1–30 天 |
| `OPENLUCKY_ACCESS_ROWS` | `20000` | 访问明细上限 1,000–100,000 条，数据库仍限制 64 MiB |
| `RUST_LOG` | `openlucky=info,tower_http=warn` | 标准输出结构化日志级别 |

QNAP 自定义参数保存在 `/share/Container/openlucky/runtime.env`。该文件只允许管理员维护，更新不会覆盖；健康检查使用实际管理监听地址。

私网 443 接管使用 `OPENLUCKY_HTTPS_ADDR=100.64.0.5:443`、`OPENLUCKY_PUBLIC_HTTPS_PORT=443`，HTTP 监听限制为 `127.0.0.1:58081`。宿主网络沿用 QNAP 的低端口限制，镜像为 OpenLucky 二进制设置 `cap_net_bind_service=ep`，Compose 的能力上限仅保留 `NET_BIND_SERVICE`。为了允许非 root 进程在执行该二进制时获得文件能力，不能启用 `no-new-privileges`；容器仍以 UID 10001、只读根文件系统运行，无 Docker socket。验证低端口时必须使用宿主网络和空的临时数据目录，隔离网络的端口阈值不能代表宿主配置。

## 发布与恢复

`.myself/build-and-deploy-qnap.sh` 要求干净、已提交的 Git 工作区。默认执行全部检查，再构建确定提交的 amd64 镜像，经 SSH 上传并调用 `.myself/activate-qnap.sh`。ARM 开发机使用原生 Rust 加 amd64 交叉编译器，避免在 QEMU 内运行 GCC。

发布使用目录锁。旧容器停止后，通过临时受限容器读取 `/data` 并写出快照，从而保留 UID 10001 的私密文件权限。新镜像来源和提交标签须匹配；启动后 `healthz.build_sha` 须精确等于提交。任何切换阶段失败都会恢复旧镜像与快照。

成功后，旧应用镜像会按用户要求删除。需要人工回退时检出对应旧提交重新构建镜像，或从本机 Docker 缓存恢复，然后恢复 NAS `snapshots/<commit>.tar.gz`。首次部署失败不删除配置数据。

请用管理台“设置 → 备份”做可迁移的密码加密备份。NAS 发布快照保留密钥和业务数据，只存在 NAS 的受限目录；迁移备份不包含管理员密码、会话或 ACME 账户，恢复后按当前管理员账户继续管理，后续签发可以创建新的 ACME 账户。

## 功能验收

`.myself/check.sh` 覆盖 Rust 格式与 Clippy、单元测试、真实协议集成测试、前端构建及浏览器流程。测试证书由 OpenSSL 本地生成，不冒充公开 CA 签发结果。供应商适配与正式 ACME 必须用获授权的真实域名和凭据单独验收。

管理 API 使用私网监听，现有 Lucky 服务、其配置、镜像和其他容器均不参与此部署。面板首次启动需要 `/data/setup-token`；初始化成功后密钥失效并删除。

## 多入口迁移

现有 `OPENLUCKY_HTTPS_ADDR` 和 `OPENLUCKY_HTTP_ADDR` 属于 `default` 入口。额外入口示例：

```sh
OPENLUCKY_LISTENERS='[{"id":"public","addr":"[::]:41003","https_port":41003,"mixed":true}]'
```

Web 规则的 `listener` 指定入口；升级前规则迁移为 `default`。公私入口不共享路由或 SNI 证书。`mixed` 为同一端口上的明文请求提供规则配置的 HTTPS 跳转。修改监听环境后重启容器；规则和证书更新自动热加载。DDNS 支持 `*.s.example.com`；公网 IP 的 `source_value` 可填多个空白分隔的查询 URL，依次回退，单接口超时 8 秒、响应上限 4 KiB。留空使用内置 ipify，更新周期支持 10–86400 秒。

访问明细与小时统计存储在 `/data/telemetry.sqlite`，与配置数据库隔离；默认批量延迟最多约 5 秒。系统事件、访问队列、明细、统计与控制台均设置上限。概览的采集异常提示及日志页的采集状态用于识别丢失。完整保留策略、统计口径和磁盘边界见 [访问与性能观测](observability.md)。
