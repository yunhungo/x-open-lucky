# 开发约定

遵循仓库的 Rust / React / TypeScript / Tailwind / shadcn 技术栈，保持 `docs/architecture.md` 中的模块边界。修改配置合同时同步后台模型、前端 Zod 合同、版本化 SQL 迁移和相关行为测试。运行 `.myself/check.sh` 验证；禁止在测试中更改生产 DNS。

结构查询优先使用项目 CodeGraph 的 `codegraph_context` / `codegraph_explore`，符号定位使用 `codegraph_search`，调用关系使用 `codegraph_callers` / `codegraph_callees`，影响范围使用 `codegraph_impact`。字面内容查询可使用 rg。新写入后使用 `codegraph sync` 更新索引，不立即相信旧索引。

凭据与私钥不进入 Git、日志或 API 列表。第三方组件保留来源署名。自有代码文件写明真实创建日期、职责和作者。部署只操作本项目容器与标记镜像，不使用全局 Docker prune，不覆盖其他 NAS 应用。
