---
status: superseded
superseded_by: 0004-rust-core-professional-audio-engine
---

# 采用 TypeScript 实现 Auto Studio Core

> 历史状态：本决策已被 [ADR-0004](./0004-rust-core-professional-audio-engine.md) 取代。保留本文只用于解释 v0.4 架构的历史背景，不再作为实施基线。

Auto Studio Core、CLI、Provider Adapter 和 Agent Runtime 统一采用 TypeScript，Core 运行在受控的 Node.js Runtime 上并使用 Hono 提供本机 HTTP/JSON 与 SSE Interface。当前首要目标是快速迭代 Creative Agent、Provider 能力和内容质量；TypeScript 能直接使用主要 Provider 的官方 JavaScript SDK，并与 GUI、Web、OpenAPI schema 和质量评测工具共享语言与类型，因此不为原生 Binary 或潜在资源优势引入 Rust。

## Considered Options

- TypeScript + Hono + Node.js：采用；Provider 与 Agent 迭代路径最短，整个 MVP 只有一个业务语言 Runtime。
- Rust + Axum + Tokio：不采用；独立 Core 与 CLI 技术上可行，但官方 Provider SDK 缺口、两语言维护和初期交付速度与内容质量优先目标不匹配。
- TypeScript Core + Rust Media/CLI 或 Rust Core + Node Provider Sidecar：不采用；在没有测得瓶颈前增加跨语言协议、打包和故障边界。

## Consequences

- MVP 不再执行 Rust 选型 Spike，不维护 Rust crate、Rust DTO 或第二套 Core。
- GUI、Web、CLI 与 Core 共享 TypeScript；客户端仍只能通过版本化 Core Interface 工作，不能直接复用 Core 内部 Module。
- Core 随产品分发固定版本的 Node.js Runtime，不要求用户预装 Node，也不把当前仍在演进的 Node SEA 当作唯一分发方案。
- SQLite Driver、Credential Store、FFmpeg 和跨平台打包仍需独立 Spike；选择 TypeScript 不代表这些风险已经消失。
- 只有出现经测量且 TypeScript 无法解决的资源、启动、系统集成或分发阻断，才重新评估 Rust；Provider 延迟或 FFmpeg 计算耗时本身不构成切换理由。
