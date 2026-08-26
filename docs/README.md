# Auto Studio 文档导航

> 基线日期：2026-08-27
> 当前阶段：Q0-Content 真人 Gate；M3-A Harness architecture/migration baseline 已完成，M3-B 等待 `CONTENT-GO`
> 发布状态：Core、Project、TUI 和真实 LLM Planning 已实现；durable Transcript、Context Manifest、三种 Provider 协议的 SSE assembler、完整 ToolRequest/ToolResult、固定 `project_describe → submit_creative_plan` 多轮链路、Planning 恢复及 Project 外加密 Continuity Vault 达到 `PASS（contract + OpenAI live）`。CM-3/CM-4 已实现 automatic compaction/spill/overflow recovery 与 source-linked long-run retrieval。Approval Grant / Run Budget machine slice 已实现精确 binding、host-owned ceiling、独立累计 ledger、Execution Reservation/settlement/cancel、SQLite CAS、故障零发布、重启恢复和篡改失败关闭；legacy Generation 又被收进非默认 Cargo feature，并从 Core/TUI/Desktop production interface 移除。该控制模块尚未接固定 Planning composition root、Policy、durable ToolExecution 或 Music Project revision。Q0 v2 机器 Gate 已达到 11/12，v3 L4 重基线达到 6/6；六样本本地内容评审包已生成并验证。Creator feedback、Mode C、盲听 Keep 与真实继续编辑仍为 `LIVE-PENDING`。Portable Handoff/DAW verifier 已实现，但 Cubase、Studio One Pro、FL Studio 真人矩阵后置 M5，当前均为 `not_run`。通用 Tool Runtime、Music Project、Sampler、Audio Engine、Factory Pack、VST3 与分发 Gate 尚未通过。

## 权威顺序

同一问题出现冲突时按以下顺序判断：

1. [共同语言](../CONTEXT.md)：定义领域术语，不描述实现；
2. [产品设计](product/ai-creative-agent-product-design.md)：定义目标用户、问题、范围、体验和产品验收；
3. [技术设计](design/auto-studio-technical-design.md)：定义当前实例化架构、目标 Module/seam 和技术 Gate；
4. [统一 Roadmap](roadmap.md)：记录当前证据、依赖顺序、工作项和 Release Gate；
5. [ADR](adr/)：记录难逆转决策及 superseded 历史；
6. [Planning](planning/)：记录由 Roadmap 引用的有界实验/实施协议；不得自行改变产品范围或状态；
7. [Research](research/README.md)：保留特定日期的研究证据，不自动成为当前产品决策；
8. `docs/api/openapi-v1.json`：当前 Core HTTP 合同快照，不替代领域语义。

代码与测试证明“当前实现了什么”，产品设计和 accepted ADR 约束“允许交付什么”。Fixture、ignored test、`SKIP`、厂商宣传和 local wire contract 不能升级为 live/human/OS/DAW PASS。

## 当前唯一工程重点

当前唯一阻塞 M3-B 的产品 Gate 是 Q0-Content：用 12 个 L1—L4 Brief、结构化 spec、固定本地渲染与音色映射验证 Keep、真实继续编辑、Tool 粒度和反馈价值。M3-A 的 durable Harness 与 legacy migration boundary 已冻结；Q0-Content 不建设 Audio Engine、Factory Pack、VST3 或 production Tool Interface，机器通过也不等于 `CONTENT-GO`。跨 DAW 真人 qualification 后置到 M5/Gate E；它约束专业交接声明，不再阻塞内容投资决策。

[Q0 音乐内容可行性 Spike](planning/2026-08-24-music-quality-spike-design.md)

[Q0 可运行实验与复现命令](../experiments/music-quality/README.md)

[跨 DAW Portable Handoff 可运行实验](../experiments/portable-handoff/README.md)

[Q0 结果与未完成真人 Gate](research/music-quality-q0-results-2026-08-24.md)

[Legacy Generation 迁移清单](planning/legacy-generation-migration.md)

随后 M3 要证明：

```text
真实 LLM
  → 固定本地 Tool Registry
  → Music Project Model
  → MIDI / arrangement
  → 本地离线 Render / Analysis
  → Candidate Project Snapshot
```

音乐路径不接入 Music Provider。旧 `GenerationAdapter/Coordinator` 和确定性 WAV Fixture 是迁移代码；production 不得用它们冒充音乐能力。

M3 暂不做：MCP、Multi-Agent、视频、公开 Web、云数据库、多 OS、完整实时设备和任意 VST3 兼容。Factory Pack/Sampler 与受限 VST3 仍属于整个 MVP，但按 Roadmap 在本地基础纵切之后进入。

## 关键文档

### 产品

- [AI Creative Agent 产品设计](product/ai-creative-agent-product-design.md)
- [Roadmap](roadmap.md)
- [共同语言](../CONTEXT.md)

### 技术

- [Auto Studio 技术设计](design/auto-studio-technical-design.md)
- [Agent Harness 架构图（交互 HTML）](design/agent-harness-architecture.html)
- [Agent Run 生命周期图（交互 HTML）](design/agent-run-lifecycle.html)
- [Tool Runtime 与 MCP 架构图（交互 HTML）](design/tool-runtime-mcp-architecture.html)

三张图的 PNG 已嵌入技术设计；`.archify.json` / `.lifecycle.json` 是可验证源文件，HTML 是交互交付物。

### 当前决策

- [ADR-0011：由 LLM 通过本地工具创作可编辑音乐](adr/0011-llm-authored-local-music.md)
- [ADR-0012：分离推理记录、Provider 连续性、授权与预算](adr/0012-durable-agent-harness-state.md)
- [ADR-0004：Rust Core 与专业 Audio Engine](adr/0004-rust-core-professional-audio-engine.md)
- [ADR-0008：TUI 默认入口与 LLM Connection](adr/0008-tui-primary-entry-and-local-provider-connection.md)

ADR-0005、0006、0007、0009 和 0010 已被 ADR-0011 supersede，只解释历史和仍可复用的隔离/恢复/trust 原则，不是当前实施基线。ADR-0012 是 M3 Harness Foundation 的现行状态与隐私决策。

## 历史研究阅读规则

- 研究报告记录报告日期的候选和事实，不自动成为实施决定；
- [音乐生成模型可行性研究](research/music-generation-models-feasibility-2026-08-21.md) 是已放弃 Music Provider 路线的历史证据，不是当前接入清单；
- TypeScript、Cloudflare、Supabase 和外部生成研究不改变 Rust、Local-first、本地 Music Project 基线；
- Provider research 仍用于 LLM Connection、Model Catalog、Thinking 和 wire protocol；
- Content/Rust Audio/VST3 research 是当前本地音乐路线的重要输入，但具体库/内容只有进入代码、许可和质量 Gate 后才算采用。

## 更新纪律

- 术语变化先更新 `CONTEXT.md`；
- 产品范围变化更新产品设计，并在难逆转且有真实权衡时新增 ADR；
- 实施边界变化更新技术设计；
- 里程碑/evidence 变化更新唯一 Roadmap；
- 图与技术文档必须同步，图源通过 Archify `showcase` 校验后交付；
- 状态只能使用 `PASS`、`PARTIAL`、`IN PROGRESS`、`LIVE-PENDING`、`BLOCKED`、`NOT IMPLEMENTED`、`LEGACY` 或 `SKIP`；
- 不删除仍能解释历史决策的 Research/ADR，但必须明确 superseded/historical 状态。
