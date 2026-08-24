---
status: superseded
date: 2026-08-24
superseded_by: 0011-llm-authored-local-music
---

# 统一 Tool Runtime，并把 MCP 作为受控 Client Adapter 接入

> 历史说明：统一 Tool Runtime 与 MCP trust 原则被保留，但 Generation Adapter 和 Music Provider 特例已被 [ADR-0011](./0011-llm-authored-local-music.md) 删除。当前 Tool Runtime 以本地 Music/Audio Tool 为主路径，MCP 仍是后续受控扩展。

## 背景

Auto Studio 长期需要调用三类能力：Core 内的媒体与工程操作、可能计费且持续数分钟的生成 Provider，以及用户主动连接的外部 MCP Server。如果让 Agent 分别理解 Rust 函数、FFmpeg 参数、Provider HTTP 和 MCP wire payload，权限、恢复、审计和客户端状态会分散到每个调用点；如果把它们全部压成一个没有领域语义的通用调用，又会丢失音乐生成的 submit/observe/reconcile/cancel 和 Unknown Outcome 规则。

当前代码没有通用 Tool Registry、MCP Client、MCP Server、MCP Connection 存储或 `/mcp` TUI 命令。M3 只有固定 `music.generate` 路径和 release 外 Fixture，生产仍是 planning-only。本文记录目标 seam 和进入顺序，不把目标架构描述为当前能力。

MCP 协议仍在演进。2026-07-28 规范方向移除了旧的 `initialize`/`initialized` 与 `Mcp-Session-Id` 核心会话，采用每请求携带版本、客户端与能力元数据的无状态模型，并提供可选 `server/discover`；2025-06-18 等旧 revision 仍使用初始化握手。因此 Auto Studio 不能隐式混用 revision，必须固定并记录所协商的协议路径。

## 决策

1. Agent 只产生 canonical `ToolRequest`，并只接收 canonical `ToolResult`。LLM 不接触 raw path、Shell、SQL、FFmpeg argv/filter graph、Provider HTTP、Credential 或 MCP JSON-RPC envelope。
2. 目标态只有一个 Tool Runtime seam：解析已注册 descriptor，执行 schema/revision/capability 校验，应用 Policy 与一次性 Approval，然后创建 durable `ToolExecution` 并委派给具体 Adapter。
3. Tool 来源分为三类 Adapter：
   - Built-in Adapter：固定媒体、工程、MIDI、Sampler 或 Mix operation；
   - Generation Adapter：保留 submit/observe/reconcile/cancel、费用、artifact 与 Unknown Outcome 语义；
   - MCP Client Adapter：连接外部 MCP Server，完成发现、`tools/list`、`tools/call` 和结果归一化。
4. 一个 `ToolDescriptor` 至少包含 namespaced name、schema revision、input/output JSON Schema、source、side-effect class、approval class、replay policy、capability fingerprint 和可见性。动态 MCP Tool 命名为 `mcp.<server_id>.<tool_name>`，不得覆盖内置或生成 Tool。
5. MCP Server Connection 不属于 Project Package。连接命令/URL、固定 protocol revision、transport、允许的能力和 trust policy 存在应用配置；Credential 只以 OS Vault reference 保存。Project 只能引用无秘密的 connection/tool snapshot，不能携带可执行命令并在另一台机器静默启动。
6. MCP 注册必须经过 discover/list、分页与 TTL 处理、JSON Schema/大小/深度校验、namespace、fingerprint 和用户 allowlist。descriptor 或 capability fingerprint 变化时，高风险 Tool 的既有授权失效。
7. MCP 调用必须复用 durable `ToolExecution`：调用外部 Server 前持久化 identity、request hash、descriptor fingerprint、Approval binding 和 operation phase；返回后把 `content`、`structuredContent`、`isError` 和诊断归一化为受限 `ToolResult`，再提交 Event/Projection。
8. MCP Tool 描述、annotation、resource link 和返回内容全部视为不可信外部输入。它们不能提升权限、覆盖 system/policy 指令、读取未批准资产、回显 Credential，或直接成为工程事实。
9. Generic MCP `tools/call` 不替代付费音乐生成的领域协议。长任务只有在 Server 提供可持久化 identity、状态观察、取消和结果对账时，才能由 Generation Adapter 包装后承担正式生成；否则只能作为非正式或只读 Tool，超时保持 Needs Attention，不盲目重放。
10. Auto Studio 首先是 MCP Host/Client。把 Auto Studio 自身暴露为 MCP Server 是独立产品方向，需要另行定义远程权限、Project Session、资源隔离和发布 Gate，本 ADR 不授权实现。
11. M3 不预建通用 Registry 或 generic MCP。先实例化固定 `music.generate` 和 Durable ToolExecution；Ship 0 内容质量、DAW Handoff 与设计伙伴 Gate 通过后，如果第二个 production Semantic Tool 或第一个被批准进入产品的 MCP Tool 出现，再从两条真实路径提炼 Registry。

## 结果

- Agent、TUI 和未来 GUI/Web 不需要理解工具来源；它们只理解 descriptor、Approval、ToolExecution 和 ToolResult。
- 内置、生成与 MCP 共享审计和恢复规则，但各 Adapter 保留必要的领域深度，不形成万能 Provider 接口。
- MCP 可以扩展上下文与工作流，但不会绕过 Project revision、Selection、费用授权、rights/provenance 或 Unknown Outcome。
- 当前 5 个共享 crate 不变。首次实现优先作为现有 crate 内逻辑模块；只有独立安全进程或两个稳定协议实现证明物理拆分收益时才新增 crate/process。
- Ship 0 不因缺少 MCP 被阻塞；把 MCP UI 或 server marketplace 提前加入 M3 属于范围扩张。

## 不采用

- 让 LLM 直接构造 `tools/call` 或启动任意 stdio Server；
- 把 MCP Tool 自动视为安全、只读、幂等或可重试；
- 把 Project 中的 MCP 配置当作可信命令执行；
- 为每个 Tool 来源建立独立 Approval、Event 和恢复状态机；
- 用 generic MCP 包装掩盖付费生成缺少 reconcile/cancel 的事实；
- 在第二条 production Tool 路径出现前建立插件市场、动态 crate 系统或 Multi-Agent。

## 验证

- descriptor 名称冲突、恶意/超限 schema、分页循环和 capability 变化被拒绝或要求重新授权；
- stdio command、cwd、env 和 remote URL 只能来自用户配置与 allowlist，不能来自 Project、Prompt 或 Tool result；
- MCP result 的 prompt injection、超大 payload、危险 URI/resource link、secret echo 和非法 structured content 被隔离或清洗；
- side effect 在外部 call 前存在 durable identity；Core crash 后不会因为缺少 response 盲目重复调用；
- read-only/replay-safe、approval-required 和 non-repeatable/unknown 三类 Tool 具有不同重放合同；
- 协议兼容测试分别覆盖 2026-07-28 无状态路径与仍支持的旧 initialize 路径，不用一次成功冒充全部 revision；
- release composition root 不注册 Fixture、未批准 MCP Server 或未通过 trust policy 的 Tool。

## 依据

- [MCP 2026-07-28 规范发布说明](https://blog.modelcontextprotocol.io/posts/2026-07-28/)
- [MCP Tools 规范](https://modelcontextprotocol.io/specification/draft/server/tools)
- [MCP 2025-06-18 Schema](https://modelcontextprotocol.io/specification/2025-06-18/schema)
- [ADR-0007：先验证 Agent 工程闭环](0007-progressive-product-proof-before-audio-and-vst3.md)
- [ADR-0009：Durable Creative Run Coordinator](0009-durable-creative-run-coordinator.md)
