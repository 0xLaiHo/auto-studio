---
status: superseded
superseded_by: 0011-llm-authored-local-music
supersedes: 0005-vst3-plugin-host-in-mvp
---

# 先验证 Agent 工程闭环，再进入专业音频与 VST3

> 历史说明：本文依赖“外部 Music Provider 先生成 Audio Candidate”的假设，已被 [ADR-0011](./0011-llm-authored-local-music.md) 取代。当前产品必须用本地 Music Project、MIDI、Sampler 和 Audio Engine 完成音乐闭环；受限 VST3 也属于 MVP。

Auto Studio 保留 Rust 独立 Core、专业 Audio Engine 和隔离 VST3 Host 的长期北星，但不再把它们与 Creative Agent 工程闭环绑成一个首发 Gate。Ship 0 用一个首发 OS、`autostudio` TUI 主 Client、一个 Agent Model 和一个音乐 Provider 验证 Brief → Candidate → Selection → Audio Clip Timeline → DAW Handoff Package；Tauri Desktop 只保留为同一 Core 契约的开发界面。Ship 1 在出现继续编辑证据后进入 MIDI、Sampler 和 Factory Pack，Ship 2 再验证隔离 VST3 effect，随后才扩展 instrument、PDC 和 Plugin Profile。

## Considered Options

- 一次完成 Agent、Audio Engine 与 VST3：覆盖终局最完整，但产品价值、实时系统与插件兼容任一失败都会阻断全部 MVP，无法判断失败来自需求还是工程。
- 只做生成聊天与 WAV 下载：交付最快，但不能验证“可继续制作的工程”，会退化成 Provider 聚合器。
- 分阶段产品证明：采用。Ship 0 保留 Project、Agent、Selection、恢复、来源和结构化 DAW 交接，把专业音频和插件作为有进入证据的后续 Ship。

## Consequences

- Ship 0 只冻结一个主发布 Client；客户端优先级已由 [ADR-0008](./0008-tui-primary-entry-and-local-provider-connection.md) 更新为 `autostudio` TUI 主入口，Tauri Desktop 保留为开发界面；本机 Web 继续后置。
- Ship 0 至少需要两个外部适配：一个 Agent Model Provider 和一个音乐生成 Provider；“一个 Provider”不能把两类职责混为一谈。
- M3 按 [ADR-0009](./0009-durable-creative-run-coordinator.md) 先补齐耐久 Run 协调与一个真实 Music Provider 的 production 纵切，再进行内容质量验证；通用流式 Agent Loop、更多 Tool 和更多 Provider 只有在真实纵切暴露复用需求后扩展。
- Ship 0 只提供 Audio Asset 的 Preview Playback，不承诺专业实时 callback、Mix Graph、Sampler、MIDI、Content Pack、VST3、PDC、freeze 或 Authoritative Render。
- DAW Handoff Package 必须超出单一 WAV：包含选中音频、Provider 可提供的 stems、Tempo/key/markers 和 provenance/credits manifest。
- VST3 可作为隔离、限时且不阻塞 Ship 0 的技术 Spike；只有目标用户证据表明插件是采用条件，且 Spike 通过后，才进入 Ship 2 产品范围。
- 技术设计区分 Target Architecture 与 Instantiated Architecture。逻辑 Module 可以预先命名，物理 crate 只在第二个真实 Adapter、独立安全/进程边界或独立构建测试需求出现时拆分。
