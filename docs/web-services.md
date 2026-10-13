# Web 服务配置

常用项在“Web 服务 → 编辑”或服务详情的“编辑”中设置。接入协议可选 HTTP/HTTPS、仅 HTTP、仅 HTTPS；HTTP/1.1、HTTP/2、HTTP/3 可分别启用。HTTP/3 使用 QUIC/UDP 与 TLS 1.3，需要有效证书及 UDP 可达。已有服务升级后维持 HTTP/1.1 + HTTP/2，HTTP/3 按服务开启。

“高级配置”包含最低 TLS 版本、上游 HTTP 版本、连接超时、响应头超时、HTTPS 跳转、Host 选项及自定义头。自动上游模式对 HTTPS 协商 ALPN，对明文使用 HTTP/1.1；指定 HTTP/2 时对明文使用 h2c prior knowledge。上游 HTTPS 保持可信根及主机名校验。WebSocket/Headscale 升级隧道需要上游 HTTP/1.1；尚不提供 HTTP/2/3 Extended CONNECT 隧道。

## 自定义头与动态模板

每行填写名称，选择“设置”或“删除”。设置值既支持固定文字，也支持 MiniJinja 模板；“插入变量”提供常用变量和条件示例。请求头、响应头独立配置，保存后立即生效。

| 值示例 | 结果 |
| --- | --- |
| `nas` | 固定值 |
| `{{ request.host }}` | 原始域名，不含端口 |
| `{{ client.ip }}` | 真实连接的客户端 IP |
| `{{ request.scheme }}` | `http` 或 `https` |
| `{{ request.headers["x-source"] \| default("unknown") }}` | 原始请求头，缺失时回退 |
| `{{ "secure" if request.scheme == "https" else "plain" }}` | 按条件计算 |
| `service-{{ request.host \| lower }}` | 文字与变量组合 |
| `{{ response.status }}` | 上游响应状态码，仅响应头可用 |

其他字段：`request.authority`（域名及端口）、`request.method`、`request.path`、`request.uri`（路径及查询）、`request.id`、`upstream.host`。请求头引用使用小写名称，读取修改前的原始请求；响应模板也使用同一原始请求快照。支持 `default`、`lower`、`upper`、`trim`、`replace`，条件表达式及 `{% if ... %}`。未知字段或不支持的语法保存时拒绝。可选请求头应使用 `default`。

模板由 MiniJinja 编译执行；没有模板的固定值直接复用已解析头。为保持请求路径的资源边界，模板不允许循环、赋值、导入、函数调用或算术扩展；每条模板最多 256 个 AST 节点、24 层深度、2048 条执行指令。没有文件或网络加载器。输出仍须符合 HTTP 头格式；异常返回 502 并在访问日志中显示模板执行失败，不记录变量内容。

`Connection`、`Content-Length`、`Transfer-Encoding`、转发来源、请求关联 ID、Alt-Svc 等由代理维护，不能覆盖。认证与 Cookie 头不在配置中保存；原始业务请求中的认证头继续正常转发。不要把凭据填写进普通自定义头。

## 性能与边界

请求/响应体持续流式转发。路由使用哈希表和共享快照；URL、静态头、动态模板和 TLS 配置只在刷新时解析。上游连接池持续复用，每目标最多保留 64 条空闲连接。HTTP/3 的连接、流、收发窗口和应用队列均有上限，并在客户端断开时取消对应上游工作。

HTTP/3 与 HTTP/2 的优势依赖网络丢包、延迟和客户端实现，不能把本机回环吞吐等同于公网体验。连接超时及响应头超时控制等待阶段，不截断正常长连接或下载。访问日志沿用原有有界队列，超量丢弃可在采集状态查看。
