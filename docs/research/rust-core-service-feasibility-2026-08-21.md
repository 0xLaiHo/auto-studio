# Auto Studio 独立 Core 与 Rust 切换可行性评估

> **历史报告**：本文记录 v0.4 做出 TypeScript Core 决策时的范围和证据。产品随后把专业实时音乐、真实乐器采样器和音频引擎纳入长期核心范围，[ADR-0004](../adr/0004-rust-core-professional-audio-engine.md) 已取代 ADR-0003；交付顺序由 ADR-0007 收敛为 Ship 0/1/2。当前实施基线见[技术设计](../design/auto-studio-technical-design.md)；本文的旧结论不再适用，但保留用于决策追溯。

> 日期：2026-08-21  
> 范围：Local-first、BYOK、只接托管 Provider、不自部署模型  
> 目标：同一 Core 服务支持 TUI、GUI 与未来 Web，同时保持内容质量优先
> 当时决策：**Rust 不进入 v0.4 MVP；该决策现已被 ADR-0004 取代**

## 1. 结论

独立 Auto Studio Core 已成为架构基线；Rust 技术上高度可行，但本次评估后决定不用于 MVP，Core 与 CLI 统一采用 Hono + Node.js + TypeScript。

最终决定分成两层：

1. **进程边界**：TUI、GUI、未来 Web 都通过版本化本机 API 使用独立 Core，Core 独占项目写入、Agent Run、Provider Job、Credential 和 FFmpeg。
2. **实现语言**：采用 TypeScript，不再执行 Rust 对照 Spike，也不保留 Rust Core、跨语言 DTO 或 Node Provider Sidecar。

选择 **Hono + Node.js + TypeScript**，因为当前首要目标是内容质量和 Provider 迭代速度；CLI 与独立常驻 Core 通过清晰进程 Interface 和固定 Node Runtime 实现，不需要为产品形态强制引入第二种业务语言。

Rust 不会提高音乐或视频模型本身的生成质量。它改善的是启动、资源占用、分发、进程可靠性和并发错误边界；内容质量仍取决于 Provider 选择、Creative Brief、上下文组织、Candidate 评测和创作者反馈闭环。

## 2. 已确认事实、工程推断与待验证项

| 类型 | 结论 |
|---|---|
| 官方能力 | Axum 原生提供 SSE Response；Tokio 适合并发 I/O；Reqwest 支持异步 HTTP、JSON、multipart 和 stream；SQLx 支持 SQLite；Tokio 可管理 FFmpeg 子进程。 |
| 官方能力 | Hono 可运行在 Node.js，并提供 Node Adapter、静态文件和 SSE helper。 |
| 官方能力 | Google GenAI 官方 SDK 覆盖 Python、JavaScript/TypeScript、Go、Java 和 C#，未列 Rust；ElevenLabs 官方 REST SDK 为 Python 与 JavaScript/Node。 |
| 工程推断 | Rust 可以直接使用 Provider REST、SSE 或 WebSocket，不要求官方 SDK，但需要自行维护认证、错误映射、流式解析和新字段跟进。 |
| 工程推断 | 独立 Core 的瓶颈主要是外部模型延迟、媒体下载和 FFmpeg，而不是 HTTP Router；Rust 的性能优势不是切换的主要依据。 |
| 必须验证 | OS Credential Vault、目标系统安装/升级、休眠恢复、媒体打包、Provider 细节兼容和团队 Rust 交付速度。 |

官方依据：

- Axum 提供内建 SSE Response，可承载可续接的项目事件流：[Axum SSE](https://docs.rs/axum/latest/axum/response/sse/)
- Tokio 是面向网络应用的异步 Runtime，并明确区分 I/O 并发与 CPU 并行：[Tokio Tutorial](https://tokio.rs/tokio/tutorial)
- Reqwest 提供异步/阻塞 Client、JSON、multipart、TLS 与 stream：[Reqwest](https://docs.rs/reqwest/latest/reqwest/)
- SQLx 把 SQLite 列为受支持的数据库，并可默认静态链接 SQLite：[SQLx SQLite](https://docs.rs/sqlx/latest/sqlx/sqlite/)
- Tokio 的 `process::Command` 可启动和异步等待子进程：[Tokio Command](https://docs.rs/tokio/latest/tokio/process/struct.Command.html)
- Hono 官方 Node Adapter 支持长生命周期 Server 与静态文件：[Hono on Node.js](https://hono.dev/docs/getting-started/nodejs)
- Hono 提供 `streamSSE`：[Hono Streaming](https://hono.dev/docs/helpers/streaming)
- Google 当前官方 GenAI SDK 语言列表不含 Rust：[Gemini API libraries](https://ai.google.dev/gemini-api/docs/libraries)
- ElevenLabs 说明任意语言可使用 HTTP/WebSocket，但官方 REST SDK 是 Python 与 Node.js：[ElevenLabs API](https://elevenlabs.io/docs/api-reference/introduction)

## 3. 为什么独立 Core 是必要边界

如果 GUI 内嵌 Agent Runtime，CLI 和 Web 只有三种坏选择：复制业务逻辑、依赖 GUI 进程，或后来再进行高风险拆分。独立 Core 让产品从一开始只有一个事实中心：

```text
Electron GUI ─┐
CLI           ├── HTTP/JSON commands + SSE events ──► Auto Studio Core
Local Web UI ─┘                                      ├─ Project Session
                                                     ├─ Creative Agent
                                                     ├─ Job Runner
                                                     ├─ Provider Adapters
                                                     ├─ SQLite + Project files
                                                     ├─ Credential Vault
                                                     └─ FFmpeg / ffprobe
```

这个边界带来五个产品结果：

- GUI 关闭后，已授权 Run 可以继续轮询、下载和验证；
- TUI、GUI 与未来 Web 看到同一 Run、Job、Candidate 和 Selection；
- 一个 Core 独占项目写入，避免三个客户端直接并发改 SQLite；
- 客户端可以独立升级，但必须通过 API 版本协商；
- 未来 Server Mode 可以复用领域和 API 语义，但不能复用 Local Mode 的安全假设。

## 4. TypeScript Core 与 Rust Core 对比

| 维度 | Hono + Node.js + TypeScript | Axum + Tokio + Rust | 当前判断 |
|---|---|---|---|
| Provider 接入速度 | 官方 JS SDK 多，示例新，动态 JSON 适配快 | 多数通过 REST 手写 Client | TypeScript 优势明显 |
| Agent 迭代 | 与前端 schema、Prompt 工具链共享方便 | 结构化类型强，但生态和示例较少 | TypeScript 更快 |
| 常驻资源与启动 | 需要 Node Runtime，空闲开销通常更高 | 原生单 Binary，通常更可控 | Rust 有价值，须实测 |
| CLI 分发 | 可要求 Node 或连同 Runtime 分发 | 单 Binary 与 `clap` 路径成熟 | Rust 更合适 |
| SQLite | Drizzle + `node:sqlite`/Driver | SQLx + SQLite | 都可行 |
| FFmpeg | `child_process.spawn` | `tokio::process::Command` | 都可行 |
| 并发安全 | 依赖约定、schema 和测试 | 所有权与类型系统减少一类竞态 | Rust 更强，但非自动正确 |
| OpenAPI | Hono/Zod 生态，TS 客户端直接 | Utoipa 等可生成 OpenAPI | 都可行，Rust多一层生成 |
| GUI/Web | React/TypeScript 原生 | 仍需 TypeScript | Rust 会形成两语言栈 |
| 打包 | Node Runtime/原生依赖/SEA 路径需验证 | Cargo 多目标构建，OS 签名仍需处理 | Rust Binary 更简洁 |
| 团队认知成本 | 现有选择，较低 | async、所有权、crate 选择、跨平台系统 API | Rust 成本较高 |
| 内容质量 | 无直接保证 | 无直接保证 | 语言不是质量杠杆 |

Node 确实提供 Single Executable Applications，但官方仍标为 Active development，且当前文档存在单嵌入脚本与 CommonJS 等约束，因此不能把 SEA 当作 MVP 已解决的分发方案：[Node SEA](https://nodejs.org/api/single-executable-applications.html)。TypeScript 路线应按“随应用分发受控 Node Runtime”评估，而不是假设自动获得稳定单 Binary。

## 5. Rust 的主要收益

### 5.1 独立服务与 CLI 的发布形态更自然

Core 与 CLI 可各自构建为原生 Binary，不要求用户安装 Node。GUI 安装包只需携带并监督 Core；CLI 也可直接发现或启动同一 Core。

### 5.2 长运行进程的资源与错误边界

Core 会长期持有 Job、SSE、Provider 网络连接和 FFmpeg 子进程。Rust 的类型与所有权可以降低跨任务共享状态、生命周期和资源释放错误，但仍必须依赖持久状态机、幂等键和故障注入验证，不能把语言安全等同于业务 Exactly-once。

### 5.3 SQLite 与本地媒体路径

SQLx 提供 SQLite Driver、连接与 Migration；默认 bundled SQLite 有利于减少目标机器版本差异。媒体仍通过文件系统 staging + fsync/rename + SQLite transaction 提交，而不是写入数据库 BLOB。

### 5.4 Core 与 CLI 共享基础类型

Rust CLI 可以共享 ID、API Error、事件游标和配置解析类型。但 CLI 不应直接链接 Core 领域实现或访问数据库，否则会破坏单写入者边界。

## 6. Rust 的主要损失

### 6.1 Provider 官方 SDK 缺口

Google GenAI 当前没有官方 Rust SDK，ElevenLabs 官方 SDK 也只覆盖 Python/Node。Rust Adapter 可以直接走 REST，但每个新能力需要自行处理：

- 请求/响应 schema 与枚举演进；
- SSE/WebSocket framing；
- multipart 与大媒体上传；
- Provider 错误、请求 ID、限流与费用头；
- OAuth 或服务账号认证；
- SDK 已内建的重试与取消细节。

Provider Adapter 必须把这些差异隔离在内部，不允许 Rust Client 类型穿透 Tool 与 Agent 领域层。

### 6.2 内容质量迭代速度风险

MVP 的核心工作是 Creative Brief、Agent Decision schema、提示词、质量量表和 Provider Capability 实测。TypeScript 通常能更快利用官方示例和 JSON 工具链。Rust 如果让每个 Provider 改动都先解决编译与 Client 细节，就会挤占盲测和创作者反馈时间。

### 6.3 两语言栈不可避免

GUI/Web 仍然是 React + TypeScript，因此 Rust Core 会形成清晰但真实的两语言栈。收益来自稳定 API 边界和原生分发；如果团队只是把 TypeScript DTO 手工复制成 Rust Struct，维护成本会反弹。必须以 OpenAPI 为协议事实源并自动生成客户端。

### 6.4 凭证与跨平台系统集成

独立 Core 不能继续依赖 Electron `safeStorage`。无论 TypeScript 还是 Rust，都要验证 macOS Keychain、Windows Credential Manager 和 Linux Secret Service 的一致行为。Rust 可使用社区跨平台封装或 OS-specific Adapter，但生产选型必须经过维护状态、锁屏行为、无桌面会话和安装包权限验证。

## 7. 若未来重新评估 Rust 的候选技术栈

| 职责 | 候选 | 选择理由与限制 |
|---|---|---|
| HTTP/API | Axum | Router、Extractor 与 SSE 路径清晰；只在 Core 外层使用 |
| 异步 Runtime | Tokio | Provider 网络、计时器、SSE 和子进程管理 |
| 序列化 | Serde + serde_json | API、Provider payload 和持久事件 |
| HTTP Client | Reqwest + rustls | JSON、multipart、stream；Provider 认证由 Adapter 管理 |
| SQLite | SQLx SQLite | Migration、查询与事务；第一版不启用数据库抽象到 Postgres |
| API 合同 | OpenAPI 3.1 + Utoipa 候选 | 生成 TS Client；须用 contract test 防止 spec 漂移 |
| CLI | Clap + generated Core client | CLI 只调用 Core，不开数据库 |
| 观测 | tracing + tracing-subscriber | 本地结构化日志、run/job correlation 和脱敏层 |
| 媒体进程 | tokio::process::Command | 参数数组、无 shell、取消与进程树策略须单测 |
| 错误 | thiserror / anyhow 分层 | Domain/API 使用稳定错误码；Composition Root 可用上下文错误 |
| Credential | `CredentialStore` Port | 具体 crate 不在报告中直接定案，必须先过 OS Spike |

这不是依赖锁定清单。精确 crate 版本、feature、许可证、维护状态和供应链策略要在实现 Slice 0 固定。

## 8. 独立 Core API 设计

### 8.1 传输

- 命令与查询：HTTP/JSON；
- 状态更新：SSE，并支持 `Last-Event-ID` 或等价游标；
- 大媒体预览：受授权的 Asset endpoint + Range；
- 文件导入：GUI/CLI 提交明确授权路径，Web 使用流式上传；
- WebSocket：MVP 不需要，只有双向低延迟协作出现真实需求时加入。

### 8.2 一致性

- 所有写命令带 `Idempotency-Key`；
- Project 变更带 expected revision，冲突返回可恢复错误；
- Event 先进入 SQLite 事实表，再推送 SSE；
- 客户端断线后从 durable cursor 补发，SSE 不是事实源；
- 同一 Project 只有一个可写 Project Session。

### 8.3 本机安全

- 默认只绑定随机 loopback port，不监听 LAN；
- 安装时生成本机访问秘密，发现文件只包含 Core 身份、端口、PID、API 版本和受保护的认证材料引用；
- 每个请求验证 Bearer/session、Host、Origin 和 API version；
- Web UI 由 Core 同源提供，通过一次性 bootstrap secret 换取 HttpOnly、SameSite session；
- Electron 保持 sandbox、`contextIsolation` 和 `nodeIntegration: false`，只作为普通受信客户端；
- Client 不能通过通用 API 读取任意绝对路径；媒体访问使用 Project/Asset handle；
- 公网远程访问必须进入 Server Mode，不允许仅修改 bind address。

Electron 官方安全清单要求启用 sandbox/context isolation、限制导航、设置 CSP 并验证 privileged message sender；独立 Core 不取消这些要求：[Electron Security](https://www.electronjs.org/docs/latest/tutorial/security)。

## 9. Local Mode 与未来 Server Mode

| 能力 | Local Mode（MVP） | Server Mode（未来独立产品阶段） |
|---|---|---|
| 身份 | 本机安装身份与客户端配对 | 用户、组织、会话、RBAC |
| 数据库 | Project Package SQLite | PostgreSQL 等服务端数据库 |
| 媒体 | Project Package 文件系统 | 对象存储 + 生命周期管理 |
| Credential | OS Credential Vault | 服务端 Secret/KMS 与租户隔离 |
| 执行 | 单机 Core Job Runner | Durable Workflow/Queue/Worker |
| API | loopback HTTP + SSE | TLS、Gateway、限流、审计 |
| 协作 | 多客户端、单创作者事实 | 多用户并发和冲突策略 |

共享的是领域对象、Tool 语义、Provider Adapter 合同和 API 资源模型，不共享持久化与安全实现。MVP 不提前实现 Server Mode Repository 或“双数据库 ORM”。

## 10. 已取消的 Rust Spike 与未来重评 Gates

以下 Gate 是评估阶段为 Rust 准备的条件，MVP 不再执行。只有 TypeScript 出现实测且无法消除的资源、启动、系统集成或分发阻断，并新建替代 ADR 后，才重新启用这些 Gate。

### Gate A：多客户端协议

- Core 启动、发现、版本握手与优雅退出；
- GUI 测试客户端和 CLI 同时连接；
- REST 命令、SSE 断线重连和事件补发；
- revision 冲突不会覆盖项目状态。

### Gate B：持久运行与恢复

- Fake Agent Run 与 Fake Provider Job 持久化；
- 在 submitting、polling、download、file commit、DB commit 注入退出；
- 重启后 Unknown Outcome 先对账，不产生重复提交。

### Gate C：真实 Provider

- 至少一个首发音乐 Provider 完成验证、提交、轮询/流式、下载与错误映射；
- 至少一个首发 Agent Model 完成结构化 Agent Decision；
- Rust REST 路径没有官方 SDK 才能完成的关键认证或能力缺口。

### Gate D：媒体与文件系统

- FFmpeg/ffprobe 启动、取消、超时、stderr 限制和进程树清理；
- WAV 验证、staging、hash、rename 与 SQLite transaction；
- HTTP Range 播放不会暴露任意文件。

### Gate E：分发与凭证

- 首发 OS 的 Core Binary、CLI、FFmpeg 和 Migration 可安装、签名、升级与回滚；
- Credential 不进入 Project、日志、事件或子进程环境；
- GUI 退出后 Core 仍能读取已授权连接并完成 Job。

### Gate F：迭代效率

- Provider payload 改一次、Agent Decision schema 改一次、API 资源改一次；
- 记录实现、测试和调试耗时，并与当前 TypeScript Core 基线比较；
- 团队能在没有个别 Rust 专家救火的情况下定位一次故障。

当前不以这些 Gate 阻塞 MVP。若未来重评，任何 Gate 未通过都应终止 Rust 迁移；不得用临时 Sidecar 或双 Core 绕过失败条件。

## 11. 已决实施策略与被拒方案

### 已采用：TypeScript Core

```text
apps/core      Hono + Node.js + TypeScript
apps/cli       TypeScript Core client
apps/desktop   Electron + React
apps/web       React, served by Core
packages/*     domain, api-contract, provider adapters, evals
```

优点是首个 Provider 和 Agent Run 最快；缺点是 Core/CLI 分发与长期资源需要额外打包验证。不要使用 Electron IPC 作为领域接口，避免以后无法替换 Core。

### 已拒绝：MVP 切换到 Rust

```text
crates/core-server    Axum composition root
crates/domain         states, ids, invariants
crates/application    use cases and ports
crates/persistence    SQLx SQLite
crates/providers      provider adapters
crates/media          FFmpeg and file commit
crates/cli            thin Core client
packages/api-client   generated TypeScript client
apps/desktop          Electron + React
apps/web              React, served by Core
```

不要保留 Node Provider Sidecar。若 Rust Adapter 因 SDK 缺口必须长期依赖 Node，系统会重新变成多 Runtime，应判定 Rust Gate 失败，而不是把临时桥接写成正式架构。

## 12. 最终决策

- **架构**：采用独立本地 Core。
- **实现**：采用 Hono + 固定 Node.js Runtime + TypeScript strict；CLI 同样使用 TypeScript。
- **Rust**：技术可行但不进入 MVP，不执行 5 日对照 Spike。
- **原因**：Provider/Agent 高频迭代、官方 JavaScript SDK、共享 schema 与单一业务 Runtime 对当前内容质量目标更重要。
- **重评条件**：只有 TypeScript 出现实测且无法消除的资源、启动、系统集成或分发阻断，并新建替代 ADR。
- **禁止方案**：Rust Core + Node Provider Sidecar、客户端直连 Provider、GUI/CLI 直开 Project SQLite、把 Local Core 暴露公网。

相关决策见 [ADR-0002](../adr/0002-independent-local-core-service.md)，当前实施顺序见 [Roadmap](../roadmap.md)。
