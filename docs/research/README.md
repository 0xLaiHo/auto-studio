# Research 索引

Research 保存“在特定日期、基于特定来源得出的证据”，不定义当前产品范围或发布资格。当前决策见 [文档导航](../README.md)、[ADR-0011](../adr/0011-llm-authored-local-music.md)、[技术设计](../design/auto-studio-technical-design.md)和[Roadmap](../roadmap.md)。

## 当前 M3 直接输入

- [Pi、Codex、DeepSeek Harness 上下文管理对比](agent-context-management-pi-codex-deepseek-2026-08-25.md)：用于 durable Transcript、derived context surface、Context Manifest、Provider Continuity Vault、compaction 与必做的 Long-Run Retrieval 实施切片；CM-0/CM-1/CM-2 Planning slice 已实现，CM-3/CM-4 未实现。
- [Agent Run Harness 源码模式与落地建议](agent/agent-run-harness-patterns-2026-08-23.md)：稳定 identity、accepted/completed 分离、Approval、checkpoint、projection、恢复和有界唤醒仍可复用；其中 Music Provider Job/Unknown Outcome 部分已被新方向删除。
- [Pi Provider 设计](pi-agent-provider-adapter-design-2026-08-21.md)：用于 LLM Inference Module 与 Provider Adapter seam。
- [Provider 与 TUI 实施基线](provider/provider-and-tui-implementation-baseline-2026-08-21.md)：用于 LLM wire contract、Connection、目录和错误语义。
- [模型目录与 OpenCode TUI](provider/provider-model-catalog-and-opencode-tui-2026-08-22.md)：已部分转化为 `/connect`、`/model` 和 TUI 交互。
- [Pi Thinking](provider/pi-thinking-level-provider-adaptation-2026-08-22.md)与 [OpenCode Thinking](provider/opencode-thinking-level-provider-adaptation-2026-08-22.md)：已转化为 capability-driven Thinking，精确模型仍需 live qualification。

## 当前本地音乐路线输入

- [真实乐器、音色内容与 Rust 音频栈](instrument-sample-libraries-and-rust-audio-stack-2026-08-21.md)：用于 Factory/Optional Pack、许可、Sampler、DSP、VST3 和质量 Gate。
- [Rust 音频技术栈](rust-audio-stack-feasibility-2026-08-21.md)：用于 MIDI、解码、FFT、重采样、播放与渲染候选；库名不等于已经采用。
- [TypeScript 音频替代评估](typescript-audio-stack-alternatives-2026-08-21.md)：保留为何采用 Rust 的反向证据。
- [Rust 独立 Core 可行性](rust-core-service-feasibility-2026-08-21.md)：保留独立 Core 与 Rust 切换的早期论证。

## 已放弃的 Music Provider 路线

- [音乐生成模型可行性](music-generation-models-feasibility-2026-08-21.md)：Eleven Music、Mureka、Google Lyria、ACE-Step、Stable Audio 等报告只解释此前为什么考虑外部生成。根据 ADR-0011，当前产品不实现这些音乐 Provider Adapter，也不把它们列入 Roadmap。

这份报告可以用于未来单独产品研究，但不能：

- 恢复 Music Provider Connection；
- 把 prompt-to-WAV 定义成 Auto Studio 的工程事实；
- 用外部音频绕过 Music Project、MIDI、Sampler 或 Audio Engine；
- 把旧“真实 Provider”Gate 写回当前 M3。

## 已取代的基础设施路线

- [ORM + Cloudflare](orm-cloudflare-feasibility-2026-08-21.md)
- [Supabase](supabase-architecture-feasibility-2026-08-21.md)

这些报告不得用于恢复 Cloudflare/Supabase、TypeScript Core、CLI 或 Desktop-first 范围，除非先形成新的产品证据与 ADR。

## Research 使用纪律

1. 保留报告日期和来源；
2. 区分厂商自述、local contract、live account evidence、human evaluation 和法律批准；
3. 当前模型、SDK、许可、价格与协议可能变化，进入实施前重新验证；
4. Research 中的“推荐/MVP/Phase”只代表当时结论；
5. 与 accepted ADR 冲突时，以 accepted ADR 为当前决策，Research 只保留历史解释。
