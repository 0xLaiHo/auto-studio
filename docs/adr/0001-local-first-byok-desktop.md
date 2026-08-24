---
status: accepted
---

# 采用本地优先 BYOK 工作站架构

Auto Studio 从托管 SaaS 转为本地 AI Agent 工作站：Project、音乐事实和媒体保存在创作者设备，LLM 推理通过创作者自己的 LLM Connection 调用，音乐作曲、MIDI、乐器、Mix 与渲染在本机执行；不建设 Auto Studio 云端数据库、对象存储、任务平台、模型转售或平台生成计费。该选择用跨设备同步、在线协作和集中运营能力换取更低的平台复杂度、更清晰的数据所有权，以及把 MVP 精力集中到内容质量、Agent 体验和专业交付。

## Considered Options

- 托管 SaaS：便于同步、协作和集中运营，但需要多租户、云端媒体、密钥托管、生成计费和持续基础设施运维。
- 本地优先 BYOK 工作站：采用；TUI、Desktop 和未来本机 Web 共享独立 Core，LLM 推理能力由用户直接持有供应商关系和凭证。
- 完全离线、自部署模型：不采用；当前不承担 GPU 推理、模型分发和跨硬件优化。

## Consequences

- Client Surface 关闭后 Core 可继续执行；Core 停止、系统关机或崩溃后从持久化 Agent Step、ToolExecution 和 Project Revision 恢复。具体进程边界由 [ADR-0002](./0002-independent-local-core-service.md) 定义。
- 项目包必须可复制、备份、迁移和独立打开，且永不包含 Provider Credential。
- Auto Studio 不承诺跨设备同步、多人实时协作、公开托管 Web 或平台统一生成额度；本机 Web 可以作为 Core 的 Client Surface。
- 未来云同步只能作为可选 Adapter 引入，不能成为打开和编辑本地项目的前置条件。
