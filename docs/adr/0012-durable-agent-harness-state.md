---
status: accepted
date: 2026-08-24
---

# 分离推理记录、Provider 连续性状态、授权凭据与运行预算

> 实施状态（2026-08-26）：规范化 Transcript、完整 Tool pair、SSE partial-call assembler、固定 Planning 多轮链路、crash-safe resume 与 CM-2 Continuity Planning slice 已实现。当前 Vault 使用 Project 外的 XChaCha20-Poly1305 密文、独立本地密钥、精确 binding、TTL、启动/周期清理；OpenAI Responses 与 Anthropic Messages contract fixture 已证明捕获和原样回传，`gpt-5-mini` 也已通过两轮真实 Continuity Planning 测试。CM-3 checkpoint foundation 已把结构化摘要边界作为 append-only Context Event 原子持久化，保留完整 Transcript，并从最新 checkpoint 派生可恢复的 summary + recent tail；重复 checkpoint 必须推进且不能拆分 Tool pair。当前 Planning slice 在 purge 失败时以 `failed/HarnessUnavailable` 阻止 Plan 提交；面向所有 Run 终态的统一 `Needs Attention` 投影仍待通用 AgentStep/ToolExecution 落地。Anthropic exact-model live、OS Credential Vault、Approval Grant、Run Budget、自动 compaction 策略、长期 Run 与通用 AgentStep/ToolExecution 仍未完成。本 ADR 的“背景”保留决策发生时的历史上下文。

## 背景

Auto Studio 已交付 LLM Connection、Model Catalog、Thinking Level 与一次 typed Planning Turn，但尚无通用 `Turn`、`Message`、`ToolCall`、Tool Registry 或多轮 Tool loop。现有设计要求“不持久化 private reasoning”，同时又要求 Agent Run 可暂停和恢复。

这两个要求在 Provider 的连续工具调用协议上存在直接冲突：部分 OpenAI Responses 推理项和 Anthropic extended thinking block 必须在后续请求中按协议继续携带；如果只保存 Creator 可见消息和 Tool Result，同一条工具调用链可能无法继续；如果把完整 Provider payload 混入 Project Event、日志或聊天，又会扩大敏感数据暴露面，并让 provider-specific wire format 污染领域模型。

现有 `CostApproval` 只表达货币上限，也不能表示“只允许在 revision 42 新增 4 条轨道并渲染 3 次 Preview”。把 Creator 的同意、系统安全上限和单工具资源限制压成一个 budget，会产生越权与审计歧义。

## 决策

1. Agent Harness 持久化规范化 `InferenceTranscript`。它由带稳定 identity 和顺序的 `InferenceItem` 构成，至少表达 Creator/Agent 可见消息、完整 `ToolRequest`、对应 `ToolResult`、usage、finish reason 与中断事实。
2. 流式 token、partial JSON 和 partial tool call 只是内存中的组装状态。只有完整、通过 schema 校验的 Tool Request 才能写入 Transcript 并进入 Tool Runtime；中断的 partial call 不猜测、不执行、不补写成工程事实。
3. Provider Adapter 单独拥有 `ProviderContinuityState`。它是 opaque、provider/model/protocol-bound 的 wire payload 或安全引用，Harness 只处理 version、binding、created-at、expiry 和 opaque reference，不解释其中的 reasoning 内容。
4. Continuity State 写入 Project Package 之外的加密 `ContinuityVault`。它不进入 Project Snapshot、Project Event、Run Event 文本、普通日志、可见 Transcript、compaction summary、backup/export 或 telemetry。
5. Continuity State 只在 active Run 内保留。Provider、model、protocol、tool schema fingerprint 或安全策略不兼容时必须失效；`Awaiting Selection`、`Cancelled`、`Failed` 等终态完成最终语义提交后立即 purge。不能 purge 或密文损坏时 Run 进入可见 `Needs Attention`，不得伪装为已安全清理。
6. 只有精确兼容的 Continuity State 才能继续未完成的推理链。缺失或失效时可以从已提交 Transcript 开始一个新的 Inference Turn，但不得把它称为“恢复原 tool-use chain”，也不得重放未完成 Tool Request。
7. `ApprovalGrant` 取代 money-only 授权表达。它绑定 Creator、Project/Revision、Plan 或 Agent Step、Tool Descriptor fingerprint、目标实体/区域、允许的 side-effect class、数量上限、费用上限和失效条件。
8. `RunBudget` 是独立的系统 ceiling，至少限制 turns、tool executions、tokens、cost、wall-clock、render count、asset bytes 与并发；Creator 的 Grant 不能提高它。
9. `ToolResourceLimit` 仍属于单个 Tool Descriptor。执行前必须同时满足 Approval Grant、Run Budget 和 Tool Resource Limit，执行后以原子语义提交扣减预算并写入 `ToolExecution`/receipt。
10. `AgentStep`、`InferenceTurn` 与 `ToolExecution` 是不同身份：一个 Step 可以包含一个或多个 Turn；一个 Turn 可以产生零个或多个完整 Tool Request；每个 Tool Request 绑定独立 ToolExecution。它们不能复用 ID 或用聊天顺序推断执行完成。
11. Context compaction 只压缩可重建的对话语义，不改变 Project facts、Transcript 中完整 Tool Request/Result、Approval Grant、budget ledger、ToolExecution receipt 或 Continuity binding。

## Considered Options

### 完全不保存 Provider-specific 状态

隐私面最小，但 Provider 要求 continuity 的工具调用链无法可靠恢复，Thinking Level 只在单轮有效。否决。

### 把 Provider payload 原样写入 Transcript 或 Project Event

实现简单，却让敏感/private reasoning 数据进入可见记录、备份和导出，也把 wire protocol 变成领域事实。否决。

### 只保存 provider response ID

对部分协议足够，但不能覆盖 signed/encrypted block、client-side continuation 与协议版本差异。采用“opaque payload 或 reference”的一般形式，由 Adapter 决定精确表示。

### 用一个 Budget 同时表达同意与安全上限

无法区分“Creator 是否同意”和“系统是否允许”，也无法绑定 revision、target 与 tool fingerprint。否决。

## Consequences

- Agent Harness 新增深 Module：`InferenceTranscript` 负责规范化可审计语义，`ContinuityVault` 只负责加密保存和生命周期；Provider Adapter 是唯一理解 opaque payload 的 Adapter。
- TUI/GUI/Web 可以一致显示 Transcript、Grant 和预算使用，但永远不显示 Continuity payload 或 private reasoning。
- 切换 Provider/Model 可能开始新 Turn 并损失原链 continuity，这是可见且可解释的行为，不做隐式协议转换。
- 本地存储需要独立密钥、原子写入、TTL、启动清理和 purge 失败处理；这是 M3 Tool loop 的前置工作，不是后补日志功能。
- 调试时必须依赖规范化 items、hash、receipt 与安全诊断，不能通过记录 private reasoning 解决问题。

## 验证

1. OpenAI 与 Anthropic 协议 fixture 分别证明 continuity payload 可保存、加载、原样回传并在终态 purge；Provider/Model/Protocol mismatch 被拒绝。
2. Continuity payload 的已知 sentinel 不出现在 Project SQLite、Event、Transcript、日志、backup、Export、SSE 或 TUI snapshot 中。
3. partial tool call、损坏密文和缺失 continuity 不产生 Project mutation，也不盲目重试。
4. 一个完整 Tool Request/Result 在重启和 compaction 后仍能从 Transcript 重建上下文，identity 与顺序保持不变。
5. revision、tool fingerprint、target、effect count 或 cost 任一超出 Approval Grant 时执行被拒绝；扩大 Grant 需要新的 Creator action。
6. 即使 Grant 允许，Run Budget 或 Tool Resource Limit 超限仍停止执行并进入可见状态。
7. Candidate/Cancelled/Failed 的最终语义提交与 continuity purge 具备故障注入测试，清理失败不会被误报为成功。

## 关联

- [共同语言](../../CONTEXT.md)
- [产品设计](../product/ai-creative-agent-product-design.md)
- [技术设计](../design/auto-studio-technical-design.md)
- [Roadmap](../roadmap.md)
- [ADR-0011：由 LLM 通过本地工具创作音乐](0011-llm-authored-local-music.md)
