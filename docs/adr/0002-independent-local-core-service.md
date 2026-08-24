---
status: accepted
---

# 采用独立本地 Core 服务与版本化客户端协议

Auto Studio 将 Project Session、Creative Agent、ToolExecution、LLM Connection、SQLite、音乐工程和媒体提交从 GUI 进程中移出，放入独立的 Auto Studio Core。GUI、TUI 和 Web 通过同一套版本化本机 API 使用 Core，不直接访问 Project Package、Provider Credential、LLM Provider 或音频执行实现。Local Mode 默认不引入外部数据库、对象存储或 Auto Studio 云端账户。

> 实例化说明：Ship 0 仍只选择一个主发布 Client；[ADR-0008](./0008-tui-primary-entry-and-local-provider-connection.md) 已把 TUI 冻结为主入口，Desktop 保留为开发界面。Local Web 只有出现独立用户任务后才成为产品范围。公开、多租户的 Hosted Web 属于未来 Server Mode，必须重新设计身份、租户、远程存储、TLS、配额和隔离，不能通过把本机监听地址暴露到公网实现。

## Considered Options

- GUI 内嵌 Runtime：单桌面应用最简单，但 TUI 与 Web 会复制业务逻辑，关闭 GUI 也会中断本地调度。
- 独立本地 Core + 统一 API：采用；增加服务发现、版本兼容和本机 API 安全工作，换取多客户端一致性、后台恢复和清晰进程边界。
- 立即建设托管 SaaS Core：不采用；重新引入多租户数据库、对象存储、认证、密钥托管和基础设施运维，与当前本地优先和内容质量优先目标冲突。

## Consequences

- Core 是项目唯一写入者；Client Surface 只能通过 API 提交命令和读取事件。
- 关闭 GUI、TUI 或浏览器不会停止 Core 或已授权的 Agent Run；显式停止 Core、系统关机或 Core 崩溃会暂停本地执行。
- Core 默认只绑定 loopback，要求每次请求认证、Host/Origin 校验和最小权限文件访问；不能依赖“localhost 天然安全”。
- Core API 使用明确版本、幂等键、资源 revision 和可续接事件游标；客户端版本不匹配时必须安全拒绝写操作。
- 产品安装器负责成对安装、升级和卸载 Core 与主 Client；TUI 和 Desktop 都可以启动、发现并复用健康 Core，但不能复制 Core 的领域逻辑。
- TUI 是 Core 客户端，不是第二套 Agent Runtime；`autostudio` 在没有健康 Core 时拉起 `core-daemon`，所有正式状态仍只通过 Core API 读写。
- Project Package 继续使用本地 SQLite 与媒体目录，不需要外部数据库。
- 后端语言不是本 ADR 的决定；[ADR-0004](./0004-rust-core-professional-audio-engine.md) 已确认 Core、TUI、Agent Runtime 和 Audio Engine 使用 Rust，并取代 ADR-0003 的 TypeScript Core。Core Interface 与领域语义不能被框架或 Runtime 类型污染。

## Supersedes

本 ADR 不废弃 [ADR-0001](./0001-local-first-byok-desktop.md) 的 Local-first 与 BYOK 决策，但替换其中“桌面应用关闭即停止本地 Agent Step”的进程假设。
