---
status: accepted
date: 2026-08-24
supersedes:
  - 0005-vst3-plugin-host-in-mvp
  - 0006-separate-llm-inference-from-media-generation
  - 0007-progressive-product-proof-before-audio-and-vst3
  - 0009-durable-creative-run-coordinator
  - 0010-unified-tool-runtime-and-mcp-client
---

# 由 LLM 通过本地工具创作可编辑音乐，不使用 Music Provider

## 背景

此前 Ship 0 被定义为：LLM 形成 Plan，Core 再调用一个外部 Music Provider 生成 WAV，最后把 Audio Candidate 交接到 DAW。这能较快得到可听文件，但音乐的结构、旋律、和声、MIDI、配器和 Mix 不在 Auto Studio Project 中，局部修改仍然依赖重新生成，产品会退化成 Provider 聚合器。

产品目标现已澄清为“让 LLM 生成音乐”：Creator 与 LLM 对话，LLM 自己作曲、编曲、配器和混音，并通过本地专业工具形成可编辑工程。这里的“生成”不是要求文本 LLM 直接输出 PCM/WAV token，而是要求它产生结构化音乐决定并调用本地 Rust 音乐执行能力。

## 决策

1. Auto Studio 音乐产品不依赖 Music Provider，不接入 Mureka、Lyria、Eleven Music、Stable Audio 或其他 prompt-to-WAV 生成 API。
2. 唯一必需的外部 AI 连接是 LLM Provider。Agent Model 负责理解 Brief、形成音乐决定、请求 Semantic Tool 并观察结果。
3. Music Project Model 是音乐的权威来源，至少表达 Tempo、拍号、结构、轨道、MIDI、乐器分配、Mix 和自动化。聊天和 LLM 响应不是工程事实。
4. LLM 不能直接写 Project、SQLite、文件、MIDI byte、音频 sample、插件 ABI 或 Shell。它只能请求版本化 Semantic Tool。
5. Core 内实例化统一 Tool Runtime：Descriptor、schema、Policy、Approval、resource budget、expected revision、ToolExecution、Result 和 Event。M3 首先实现固定本地 catalog，不预建动态插件市场。
6. Agent Harness 从一次 Planning Turn 扩展为有界多轮 Tool loop。每个循环有 turn/tool/token/cost/time/render/resource 上限，并在 Candidate、需要输入、需要授权或失败时停止。
7. Rust Audio Engine、MIDI、Sampler 与 Factory Pack 从后续 Ship 移回音乐 MVP，因为没有外部音乐文件可以替代它们。M3 先证明确定性离线纵切，再进入实时设备和更完整 Mix Graph。
8. Factory Pack + Sampler 是不依赖第三方插件的基础路径。VST3 仍属于 MVP，但在 Factory Path 之后，以一个首发 OS、固定 corpus、隔离 worker、Approved Plugin Profile、state 和 freeze 收敛；不要求任意插件兼容。
9. Candidate 是 `Project Snapshot + Preview + Analysis + Dependencies`，不再只是 Provider 返回的 Audio Asset。Selection 仍只能由 Creator 执行。
10. 现有 GenerationAdapter、GenerationCoordinator、Provider Job、submit/observe/reconcile/Unknown Outcome 和确定性 WAV Fixture 属于旧方向。Fixture 只可在迁移期验证资产合同，不得进入 production composition root；迁移完成后删除或改为明确的 test support。
11. 音乐 ToolExecution 不再使用远端生成状态机。LLM Inference 中断可记录 `Interrupted` 或 `UnknownConsumption`，但没有完整且通过验证的 ToolRequest 时不能改变 Project。
12. MCP 以后通过同一 Tool Runtime 作为不可信外部 Adapter 接入；它不参与基础音乐闭环，也不能替代 Music Project、Sampler 或 Audio Engine。
13. Agent Harness 的推理记录、Provider 连续性、授权范围与系统预算由 [ADR-0012](0012-durable-agent-harness-state.md) 分离定义；“不公开 private reasoning”不等于丢弃完成 Provider tool-use chain 所需的 run-scoped opaque state。

## Considered Options

### 外部 Music Provider 产生音频

优点是较快得到有成品质感的 WAV，缺点是核心音乐事实不在 Project、局部编辑能力受限、质量和费用依赖外部模型、产品差异退化为编排与下载。否决。

### 要求通用 LLM 直接输出音频文件

当前接入的 OpenAI、Anthropic、DeepSeek 等通用 LLM 协议用于文本/工具推理，不提供可作为专业工程事实的高质量多轨 PCM 输出。即使未来某个模型支持音频 token，也必须单独验证可编辑性、格式、成本和权利，不能假设。否决为当前架构。

### LLM 编写本地 Music Project，再由 Rust 渲染

接口更复杂，必须建设音乐领域模型、语义工具、Sampler、内容和 Audio Engine；但它直接满足局部修改、可审计、可恢复、专业交接和差异化定位。采用。

## Consequences

- 产品从“LLM + 远端生成协调器”变为“LLM + 本地 AI-native DAW kernel”。工程量增加，但每一步产物可编辑和可追踪。
- `autostudio-provider` 的长期职责收敛为 LLM Connection、Catalog、Thinking 与协议 Adapter，不再拥有音乐生成业务状态。
- `music.generate` 不再是向外部模型提交 prompt 的单一 Tool；本地 catalog 使用 arrangement、tempo、harmony、MIDI、instrument、mix、render、analysis 和 candidate 等语义 Tool。
- M3 重新定基线，先完成 Music Project/Tool Runtime/LLM Tool loop/最小离线发声；旧“一个真实 Music Provider”工作项全部取消。
- 音质责任转移到 LLM 音乐决策、Tool 深度、Factory Content、Sampler、演奏语义、Mix 和质量评价，不能用更换 Provider 掩盖问题。
- 音乐路径没有远端 submit 的 Unknown Outcome；Project mutation 用 execution identity + expected revision + atomic commit 实现幂等与恢复。
- VST3 重新进入 MVP，但不阻塞 Factory Path，也不恢复“三系统、任意插件、完整 DAW”范围。
- MCP 设计保留统一 Tool Runtime、trust、allowlist 和结果清洗原则，删除 Generation Adapter 特例。

## 验证

1. production runtime 没有 Music Provider Connection、Adapter 或 prompt-to-WAV fallback；
2. 一个真实 LLM 使用至少两种本地 Semantic Tool 创建并局部修改 Music Project；
3. Project 保存 Tempo、结构、MIDI、乐器、Mix 和自动化，Core 重启后恢复；
4. 本地 Rust 路径从固定 Snapshot 渲染 Preview，并通过格式、时长、sample count、hash、silence 和 clipping 校验；
5. Candidate 引用不可变 Project Snapshot，Selection 只能由 Creator 发起；
6. 没有第三方 VST3 时 Factory Path 可用；
7. 一个受限 VST3 instrument/effect corpus 通过隔离、state、freeze 和 crash containment；
8. WAV/stems/MIDI/Tempo/manifest 在冻结目标 DAW 中继续编辑；
9. 固定 Brief corpus 完成机器 Gate 与人工盲听，Fixture 和 SKIP 不算通过。

## 关联

- [产品设计](../product/ai-creative-agent-product-design.md)
- [技术设计](../design/auto-studio-technical-design.md)
- [Roadmap](../roadmap.md)
- [共同语言](../../CONTEXT.md)
- [ADR-0012：Durable Agent Harness State](0012-durable-agent-harness-state.md)
