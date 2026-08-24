---
status: superseded
date: 2026-08-23
superseded_by: 0011-llm-authored-local-music
---

# 使用 Durable Creative Run Coordinator 执行真实音乐生成

> 历史说明：本文的远端 submit/observe/reconcile/download 状态机已被 [ADR-0011](./0011-llm-authored-local-music.md) 取代。稳定 identity、Approval binding、Project revision、projection 和跨重启恢复原则继续适用于本地 ToolExecution，但不再存在 Music Provider Job。

## 背景

真实音乐生成是可能计费、持续数分钟并跨网络与进程故障的外部任务。LLM 适合理解 Brief 和形成 typed Plan，但不适合承担轮询、重试、下载、资产提交和跨重启恢复。若把这些动作留在聊天循环或进程内 task 中，TUI 关闭、Core 重启或 submit 响应丢失都可能造成任务丢失、重复提交或错误取消。

Codex、Pi 和 DeepSeek Harness 的源码研究表明，稳定身份、accepted/completed 分离、语义 checkpoint、结果未知分流、纯 projection 和有界唤醒具有复用价值；它们的 shell approval、listener-before-persist、process-local Job 或通用 synthetic Tool error 不适合直接复制到付费媒体生成。

## 决策

1. Ship 0 保持一个 Creative Agent。LLM 只负责 Planning Turn 和必要的有界 follow-up；不引入角色式 Multi-Agent。
2. 已批准的 `music.generate` 由 Core 内的 Durable Run Coordinator 执行。它负责 submit、observe、reconcile、cancel、download、verify 和 commit，且不依赖 LLM 或 Client 持续在线。
3. 每个外部副作用先持久化稳定 identity、request hash、operation phase 和 Approval binding；成功提交本地事务后才调用 Provider。
4. `GenerationAttempt` 与 `ProviderJob` 分开；确认未开始、已知失败和 `UnknownOutcome` 分开。Unknown Outcome 未对账前禁止盲目重提交。
5. retry policy 按 operation 定义，不能给整个 Tool 一个通用 retryable 布尔值。
6. Project snapshot、semantic Event、outbox 和 revision 是事实源；客户端使用带 `asOfSequence` 的 Run Projection，并在 snapshot 后续接 SSE。
7. 取消分为本机停止等待、Provider cancel request 和远端确认结果；前两者不自动产生 `Cancelled`。
8. Context compaction 只影响 LLM 上下文，不改变 Project、Approval、Job、Asset、Candidate 或 Selection 事实。
9. M3 只实例化 `music.generate` 的窄路径。只有第二个真实 Semantic Tool 暴露稳定复用需求后，才提炼通用 Tool Registry 或多轮 Agent Loop。

## 结果

- M3 在现有 5 个共享 crate 内增加 RunItem/ToolExecution projection、Provider observation/cost/artifact receipt、non-terminal recovery query 和后台 coordinator，不为 Harness 新建 crate。
- `autostudio-provider` 首先只接一个真实 Music Provider，并声明 idempotency、reconcile、cancel、artifact 和费用能力。
- TUI 显示 Awaiting Approval、Running、Needs Attention、Awaiting Selection 和 terminal facts，不从 Activity 文案猜状态。
- Fixture 可以继续验证本地状态机，但不进入 production composition root，也不能替代 live Provider、内容质量或 DAW Gate。

## 不采用

- 用通用聊天循环持续 poll Provider；
- 依赖进程内 Job registry 跨重启恢复；
- 对 submit 使用无条件指数重试；
- 把本地 task abort 当作远端取消；
- 为当前一个 Music Tool 预建 Multi-Agent、插件式 Tool 平台或新 crate 集群。

## 验证

- submit 前后逐点 crash，恢复后最多一个 Provider Job；
- Unknown Outcome 只 reconcile/observe，submit 次数保持 1；
- terminal 状态不被迟到 progress 或乱序 callback 回退；
- cancel 与 completion 竞态保留 Provider 最终事实；
- artifact 在最终副本 hash、格式、时长和件数校验后才形成 Candidate；
- Core 重启和 SSE 断线后，TUI 从 snapshot + sequence 恢复同一 Run phase；
- API Key、Authorization header、原始 thinking 不进入 Project、Event、SSE、日志或 Export。

## 依据

- [Agent Run Harness 源码模式与 M3 落地建议](../research/agent/agent-run-harness-patterns-2026-08-23.md)
- [ADR-0006：分离 LLM 推理与媒体生成](0006-separate-llm-inference-from-media-generation.md)
- [ADR-0007：先验证 Agent 工程闭环](0007-progressive-product-proof-before-audio-and-vst3.md)
- [ADR-0010：统一 Tool Runtime 与 MCP Client](0010-unified-tool-runtime-and-mcp-client.md)
