---
status: superseded
superseded_by: 0011-llm-authored-local-music
---

# 分离 LLM 推理轮次与媒体生成任务

> 历史说明：本文针对外部 Media Generation Job 的分离原则已被 [ADR-0011](./0011-llm-authored-local-music.md) 取代。当前音乐产品没有 Music Provider 或外部音乐 Generation Job；若未来视频重新引入外部媒体生成，需要重新作出范围化决策。

Auto Studio 共享 Provider、Connection、Credential、Model Catalog、Capability Snapshot、用量与错误语言，但不把所有外部模型压进一个通用 Adapter。LLM 使用流式 Inference Turn 合同，音乐、视频、语音和图像使用可恢复的 Generation Job 合同；两者由 Agent Runtime 和 Semantic Tool 串联。

## Considered Options

- 一个通用 Provider Adapter：表面统一，但会把 LLM token/tool stream、异步媒体 job、资产下载和 Unknown Outcome 混成大量可选字段与分支。
- LLM 与媒体完全独立：接口清楚，但会重复连接、凭证、模型目录、能力、费用、错误和诊断规则。
- 共享 Provider Core、分离执行模块：采用。共享身份与策略事实，分别建立适合各自生命周期的深模块。

## Consequences

- Provider、Model 与 Model Protocol 是三个不同维度；一个 Provider 可使用多个协议，一个协议实现也可被多个兼容 Provider 复用。
- LLM Inference Module 统一消息、工具调用、流事件、结束原因和用量；Agent Runtime 只在完整 Tool 请求通过 schema、Policy 和 Approval 后执行 Tool。
- Media Generation Module 保留 submit、observe、cancel、reconcile、download、verify 和原子 Asset commit；Generation Job 的 Unknown Outcome 规则不能直接套在普通 LLM token stream 上。
- LLM 中断记录为 Interrupted 或 Unknown Consumption。没有 Provider 幂等键或 durable handle 时不得自动重放可能重复计费或重复执行 Tool 的轮次。
- 跨 Provider 只移交可见消息、已完成 Tool 请求/结果和项目事实；Provider Continuity State 只在兼容的 Provider、Model、Protocol 路径内复用，私有推理不持久化、不转换成普通文本。
- Provider 专有 JSON、认证对象、SDK 类型和兼容开关停留在 Adapter 内；Project 与 Agent Domain 只引用稳定 ID、快照和归一化结果。
