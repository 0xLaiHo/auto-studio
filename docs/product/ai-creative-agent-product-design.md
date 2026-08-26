# Auto Studio 产品设计文档

> 基线日期：2026-08-27
> 产品方向：LLM 驱动的本地专业音乐创作 Agent  
> 当前状态：本地 Core、TUI、Project、LLM Connection/Model/Thinking 与真实 Planning 已实现；M3-A CM-0—CM-4 已落地 durable Transcript/Context、Provider Continuity、automatic compaction/spill/overflow recovery 与 long-run retrieval。2026-08-27 Approval Grant / Run Budget machine slice 进一步实现了精确 Creator 授权范围、不可由客户端提高的系统安全上限、独立累计账本和 crash-safe SQLite CAS；Grant、Run Budget、单 Tool Resource Limit 三类拒绝可区分，相同请求不会重复扣减，等待 Creator 或跨日暂停不消耗 active wall-clock。这证明安全控制合同可运行和恢复，但它尚未接入用户 Approval UI、Policy、durable ToolExecution 或 Music Project revision，因此不是已经能修改音乐工程。Q0 v2 的真实 DeepSeek → ExperimentalMusicSpec → MIDI 正式 A/B 机器 Gate 已达到 11/12，v3 全量 L4 重基线达到 6/6，Portable Handoff v1 已能生成乐器分配清单与带 Bank/Program 的 Type-1 MIDI；Cubase、Studio One Pro、FL Studio 的精确版本实测仍为 `not_run`。真人内容 Gate、Music Project Model、通用 Tool Runtime、Sampler、Audio Engine 与 VST3 Host 尚未完成，当前产品版本仍不能真实生成音乐。

## 1. 产品摘要

Auto Studio 是一个本地优先的 AI 原生音乐工作站。创作者只需要描述目标、提供参考并持续对话；LLM 负责作曲、编曲、配器、演奏和混音决策，通过受控的本地音乐工具修改可编辑工程，Rust Audio Engine 再把工程渲染为可试听、可导出、可继续制作的音乐。

产品不依赖 Mureka、Lyria、Eleven Music、Stable Audio 或其他 Music Provider。唯一必需的外部 AI 连接是创作者自备 Key 的 LLM Provider。音乐的权威来源不是一次 prompt-to-WAV 响应，而是 Project 中可审计的结构、Tempo、和声、音符、力度、articulation、乐器、路由、效果和自动化事实。

一句话定位：

> 和 LLM 一起创作音乐，并得到真正可继续制作的工程，而不是一段无法解释的生成音频。

## 2. 问题与产品机会

现有 AI 音乐产品通常把用户意图直接转换为完整音频。它们适合快速试听，但专业制作会遇到三个问题：

1. 旋律、和声、节奏、乐器和混音决策不可直接编辑；
2. 修改一个局部往往需要重新生成整首作品，版本难以合并和追踪；
3. 导出到 DAW 后只剩 WAV 或有限 stems，无法恢复创作结构。

Auto Studio 的机会不是再做一个生成模型入口，而是让 LLM 操作音乐工程语义：创作者可以要求“只重写第 17—24 小节的贝斯”“副歌弦乐换成长音并降低一个八度”“保留旋律，把鼓改成 halftime”，系统只修改相关工程事实并重新渲染受影响部分。

## 3. 目标用户

### 3.1 首发用户

首发面向具有基础音乐判断、愿意继续编辑作品，但不希望从空白工程开始的独立音乐人、视频配乐创作者和小型创作团队。他们看重：

- 创意启动速度；
- 局部可控修改；
- 可编辑 MIDI、stems 和工程快照；
- 自己拥有的 VST3 与音色资产；
- 来源、许可和版本可追溯。

### 3.2 暂非首发重点

- 只希望一次生成成品、不关心后续编辑的纯消费者；
- 需要完整替代 Cubase、Studio One、Logic Pro 的成熟录音棚；
- 需要多人云端实时协作或公共模型市场的团队；
- 首发即要求完整视频、短剧和三系统插件兼容矩阵的用户。

## 4. Jobs to Be Done

1. 当我只有一个模糊想法时，帮我形成可听的完整音乐方向，而不是让我先搭建空白 DAW 工程。
2. 当第一版不理想时，让我用自然语言精确修改结构、旋律、配器或混音，而不是整首重来。
3. 当我要进入专业制作时，交付 MIDI、stems、Tempo/marker、音色依赖和来源记录，让我能在 Auto Studio 或现有 DAW 中继续工作。
4. 当 Agent 做了大量操作时，让我看清它改了什么、为何需要授权、失败后从哪里恢复，并允许我采用或拒绝每个候选方向。

## 5. 产品原则

### 5.1 LLM 创作，Core 执行

LLM 负责音乐决策与工具选择；Core 负责验证 schema、Project Revision、权限、资源预算、执行和提交。LLM 不能直接写 SQLite、构造任意路径、执行 Shell、调用插件 ABI 或改变音频线程。

### 5.2 工程事实优先于聊天

聊天用于表达意图和解释结果。Tempo、结构、轨道、MIDI、乐器、效果、自动化、Candidate、Selection 和 Export 必须成为版本化 Project 事实。聊天文本不能被当作工程已经修改的证据。

### 5.3 可编辑性优先于“像生成的”

每个可交付音乐结果必须能追溯到 Music Project Snapshot。首发可以限制乐器数量、曲式和插件矩阵，但不能用不可编辑的外部 WAV 冒充工程闭环。

### 5.4 内容质量优先于工具数量

先用固定创作语料证明结构完整、音乐性、真实乐器表现、混音技术质量和继续编辑价值，再增加更多 LLM、MCP、音色包、插件或视频能力。

### 5.5 Approval 与 Selection 分开

Approval 允许某组工具产生副作用或使用特定内容/插件；Selection 是创作者把某个 Candidate 采用为正式工程方向。授权执行不等于接受作品。

### 5.6 Local-first 与 BYOK

Project、MIDI、音频、内容目录和插件状态保存在创作者设备。LLM 推理通过创作者自己的 Provider Key 调用；Provider Credential 不进入 Project、日志或导出包。

### 5.7 Factory Path 必须独立可用

用户没有第三方 VST3 时，Factory Pack 与内置 Sampler 仍必须完成基础作曲—试听—导出闭环。VST3 用于接入专业资产，不能成为“首次发声”的隐藏前置条件。

### 5.8 可审计不等于保存私有推理

Creator 能看到规范化消息、完整 Tool Request/Result、授权范围、预算消耗和工程变化。为继续同一 Provider 推理链所需的 opaque continuity state 只在 active Run 内加密保存，Creator 不可见，不属于 Project、聊天或 Export，Run 终态后删除。

## 6. MVP 产品范围

MVP 的证明链路是：

```text
Creative Brief
  → LLM 形成创作计划
  → 用户批准工具影响
  → LLM 调用本地作曲/编曲工具
  → Music Project Model 形成可编辑轨道与 MIDI
  → Sampler / VST3 / DSP 本地渲染
  → 技术分析与有界迭代
  → Candidate Project Snapshot
  → 用户 Selection
  → WAV + stems + MIDI + manifest
```

### 6.1 MVP 必须包含

- `autostudio` TUI 默认入口，同一 Core 可供未来 GUI/Web 使用；
- OpenAI、Anthropic、DeepSeek 等 LLM Connection、模型目录与 Thinking Level；
- Creative Brief 与参考约束；
- 单 Creative Agent 的多轮 Tool loop；
- 规范化 Inference Transcript，以及与其分离的 Provider Continuity State；
- 绑定 Project Revision、Tool fingerprint、目标和影响数量的 Approval Grant；
- 独立的 Run Budget 与单 Tool Resource Limit；
- 版本化 Music Project Model；
- Tempo、拍号、段落、轨道、MIDI note/CC、力度、基础 articulation；
- 至少一条受支持的 Factory Pack + Sampler 乐器路径；
- 音量、声像、基础 EQ/压缩/混响和有限自动化；
- 确定性离线 Preview/Authoritative Render；
- 技术分析：时长、峰值、True Peak、LUFS、静音/削波、基础 Tempo/key 一致性；
- Candidate、A/B 试听、Selection 与 Project Snapshot；
- WAV、stems、MIDI、Tempo/marker、credits/provenance manifest；
- 一个 OS 上受限且隔离的 VST3 MVP 路径：固定测试 corpus、Approved Plugin Profile、state 恢复与 freeze；
- Core 重启后的 Agent Run 和 ToolExecution 恢复。

### 6.2 MVP 明确不包含

- Music Provider 或外部 prompt-to-WAV API；
- 角色扮演式 Multi-Agent；
- 完整替代成熟 DAW 的录音、编辑和母带功能；
- 任意 VST3 自动扫描后立即交给 Agent；
- VST2、AU、CLAP 全格式支持；
- 通用 MCP 市场或自动信任远端 Tool；
- 视频、AI 短剧、多人云协作和公开 Web；
- Cubase、Studio One 等专有原生工程文件写出承诺。

## 7. 核心用户流程

### 7.1 启动与连接

1. 创作者输入 `autostudio`；TUI 连接或启动本地 Core。
2. 输入 `/connect` 选择 LLM Provider 并保存 Key。
3. Core 后台刷新模型目录；创作者从 `/model` 选择模型与 Thinking Level。
4. Key 只写入 Project 外的安全存储；正式发布使用目标 OS Credential Vault。

产品中不存在第二个“音乐 Provider Connection”。内容包和 VST3 是本地资源，不使用音乐生成 Key。

### 7.2 从对话形成 Brief

普通文本首先被整理为 Creative Brief，包括用途、时长、风格、情绪、结构、参考、乐器、歌词、交付和禁止项。缺少会显著改变作品的输入时，Agent 提问；其余细节用可见假设继续。

### 7.3 计划与授权

LLM 输出可见计划，例如：

```text
1. 建立 96 BPM、D minor、4/4 的 48 小节结构
2. 写入钢琴和弦、贝斯、鼓与弦乐轨
3. 使用 Factory Piano、Factory Strings 和 Factory Drum Kit
4. 渲染并检查 LUFS、True Peak、静音和段落时长
5. 不满足副歌能量目标时只修改副歌配器与鼓型
```

用户看到工具影响、目标实体、可能使用的 Content Pack/VST3、预计计算资源和可撤销范围。Approval Grant 绑定当前 Project Revision、Plan、Tool fingerprint、目标、影响数量和费用；请求范围改变时必须重新批准。普通 Project 内编辑可以用严格限定的会话策略批量批准；安装内容、加载新插件、覆盖已采用版本和导出仍需显式 Approval。系统 Run Budget 始终独立生效，不能被授权放大。

### 7.4 LLM 本地创作

Agent 通过 Semantic Tool 修改工程，例如：

- `project_describe`
- `arrangement.set_structure`
- `tempo.set_map`
- `track.create_instrument`
- `harmony.write_progression`
- `midi.write_notes`
- `midi.edit_region`
- `instrument.assign`
- `mix.set_level`
- `effect.insert_builtin`
- `automation.write`
- `render.preview`
- `audio.analyze`
- `candidate.create`

Tool Result 返回结构化事实与受限诊断，不把大媒体、绝对路径、插件二进制或原始日志塞回 LLM 上下文。

### 7.5 分析、试听与迭代

Core 完成渲染和技术分析后，把工程差异、可播放 Preview 和机器指标交给 Agent 与创作者。文本 LLM 不应被描述为“直接听懂所有音频”；没有经过验证的音频输入能力时，Agent 只能基于 Project facts、机器分析和创作者反馈迭代。

### 7.6 Candidate 与 Selection

Candidate 是一个尚未采用的 Project Snapshot，包含可编辑 Music Project、Preview Render、技术指标和依赖锁。Candidate Board 至少显示：

- 结构、BPM、调性和时长；
- 主要旋律/配器差异；
- 使用的 Factory Pack、VST3 和版本；
- LUFS、True Peak、削波、静音等技术检查；
- Agent 修改摘要；
- A/B 试听和采用按钮。

Selection 只由创作者发起。未采用 Candidate 保留为可追溯版本，但不会静默覆盖正式 Timeline。

### 7.7 导出与继续制作

Agent 在 Auto Studio Project 内完成轨道、乐器和音色决定；它不通过鼠标操作 Bitwig、Cubase、Studio One 或 FL Studio，也不为每个 DAW 编写脆弱的 UI Adapter。MVP Export 按能力等级交付：

**Portable Handoff（所有冻结目标 DAW 的最低层）**：

- stereo WAV；
- 按支持范围输出的 stems；
- Type-1 Standard MIDI，包含语义轨道名、Bank Select、Program Change、note 与受支持 CC；
- Tempo、拍号与 section markers；
- `instrument-assignments.json` 与 instrument/content/plugin lock；
- credits、license 与 provenance manifest；
- Cubase、Studio One Pro、FL Studio 等冻结版本的导入说明与实测兼容状态。

Portable Handoff 保证“可导入的音乐结构和可追溯的音色意图”，不保证不同 DAW 用各自内置音源时声音一致。某个 DAW 忽略 Program Change 时，用户仍可按 assignment manifest 选择本地音色；WAV/stems 保留听感参考。

**Structured Handoff（按 DAW 能力逐一验证）**：对官方支持并通过实测的目标版本输出 DAWproject；不为不支持该格式的 DAW 虚构兼容性，也不承诺写出 Cubase、Studio One 或 FL Studio 的专有原生工程。

**Sound-identical Handoff（依赖一致时）**：使用 freeze/stems，或在目标 DAW 安装相同版本的 Auto Studio Sampler VST3、Content Pack 和 preset state。依赖缺失时明确降级到 Portable Handoff，不静默换音色。

导出不包含无权再分发的底层商业采样或用户插件二进制。

## 8. 本地音色与 VST3 策略

### 8.1 Factory Pack

Factory Pack 是“安装后立即可用”的质量下限。每个 Pack 必须绑定精确版本、文件 hash、许可文本、来源、转换记录和质量批准。优先使用 CC0 或明确允许软件再分发与商业成品使用的内容；“免费使用”不等于“可以随安装包再分发”。

### 8.2 VST3 MVP

VST3 是 MVP 的专业资产接入能力，但按以下范围收敛：

- 首发一个 OS；
- 固定一组 instrument/effect corpus；
- 隔离扫描和运行，插件崩溃不能破坏 Core；
- Agent 只操作 Approved Plugin Profile 中的 preset 和有界参数；
- User-owned Plugin 不随 Auto Studio 分发；
- 缺失插件时显示并允许 freeze/fallback，不静默替换。

Factory Path 先通过后，VST3 才能成为质量增强；两者都属于同一个本地 Tool Runtime，不是 Provider。

## 9. Agent Run 产品状态

创作者可见状态统一为：

- `Planning`：LLM 正在形成下一组音乐决策；
- `Awaiting Approval`：等待用户授权影响范围；
- `Applying Tools`：Core 正在执行本地工具；
- `Quality Check`：正在渲染、分析或比较 Brief；
- `Awaiting Selection`：可编辑 Candidate 已准备；
- `Interrupted`：Core 在语义 checkpoint 停止，可恢复；
- `Needs Attention`：内容、插件、预算或输入需要处理；
- `Cancelled`、`Failed`：终止状态。

不再把 `Submitting`、`Provider Job`、`Unknown Outcome` 作为音乐创作主流程状态。产品内部区分 `Inference Interrupted` 与 `Tool Interrupted`：前者只有在精确 Provider continuity 仍可用时才能继续原推理链，后者从 ToolExecution identity/receipt 恢复。LLM 网络中断可以记录未知 token 消耗，但没有完整且通过 schema 校验的 Tool Request 时不能修改 Project。

## 10. TUI 信息架构

### 10.1 主 Composer

- 中央输入框持续接受自然语言和 `/` 命令；
- 输入框下显示当前 LLM Provider、Model 和 Thinking Level；
- 主区域显示对话、工具计划、工程差异、Preview 与阻塞；
- 状态不得要求用户先理解内部 Project ID 或后台 job。

### 10.2 命令

MVP 至少包含：

- `/connect`：配置 LLM Provider；
- `/model`：选择模型与 Thinking Level；
- `/new`、`/open`：Project；
- `/brief`：查看/编辑 Creative Brief；
- `/project`：查看结构、轨道和 revision；
- `/run`：打开 Run Inspector；
- `/candidates`：比较 Candidate；
- `/render`：创建 Preview；
- `/export`：创建交付包；
- `/plugins`：查看受支持插件与状态；
- `/exit`：安全退出 TUI。

### 10.3 Run Inspector

显示每个 Agent Step、规范化 Inference Item、ToolExecution、Approval Grant、Run Budget 使用量、输入摘要、expected revision、状态、耗时、结果和安全诊断。Provider Continuity State、私有推理、Credential、绝对路径和插件原始 state 不显示。

## 11. 质量评估

### 11.1 Q0 内容可行性 Gate

在 M3 production 实现之前，先执行 [Q0 音乐内容可行性 Spike](../planning/2026-08-24-music-quality-spike-design.md)：只用 12 个 L1—L4 Brief、结构化音乐 spec、MIDI 和固定 DAW/音色映射，判断强 LLM 结果是否被 Keep、是否发生真实继续编辑，以及分阶段/反馈是否有价值。Q0 不建设 L5 Mix、Sampler、自研 DSP 或 VST3；实验 schema 也不自动成为 production Tool Interface。

主模型的负面结果必须由第二个不同的强模型复核后才能形成产品 `NO-GO`。Q0 `GO` 只授权继续投入 M3，不替代 Factory Pack、VST3、设计伙伴和 Release 盲听 Gate。

### 11.2 发布固定 Corpus

至少覆盖：

- 结构清晰的流行/电子音乐；
- 电影感配乐与动态段落；
- 真实钢琴、弦乐、鼓和贝斯配器；
- 局部修改任务；
- 相同 Brief 的多次可复现比较；
- 从 Auto Studio 导入目标 DAW 后继续编辑。

Corpus 固定 LLM Provider/Model/Thinking、系统提示、工具版本、Factory Pack、seed、Audio Engine 和渲染参数。VST3 评价另设精确 plugin/version corpus。

### 11.3 机器 Gate

- 无非预期削波、NaN、DC、异常静音和损坏文件；
- 时长、Tempo、拍号、段落边界与 Brief 一致；
- LUFS、True Peak 和动态范围处于目标分发场景阈值；
- MIDI 音符、note-off、CC、范围与 voice budget 合法；
- 同一 Snapshot/engine/pack/seed 的离线渲染可复现；
- Export 中的 MIDI、stems、manifest 与选中 revision 一致。

### 11.4 人工 Gate

盲听至少评价：Brief 匹配、音乐性、结构、旋律/和声、节奏、配器真实性、混音清晰度、重复聆听意愿和继续编辑价值。参与者不知道作品由哪个模型、提示或版本产生。

## 12. 成功指标

首要指标：

- **Candidate 采用率**：至少一个 Candidate 被 Selection 的 Run 比例；
- **继续编辑率**：Selection 后继续局部修改、导出 MIDI/stems 或进入目标 DAW 的比例；
- **意图保持率**：局部修改没有破坏未指定区域的比例；
- **首次可用时间**：从 Brief 到第一个可试听、可编辑 Candidate 的时间；
- **恢复成功率**：Core 重启后从同一 Project Revision 恢复的非终态 Run 比例。

守护指标：崩溃、音频 underrun、插件隔离失败、非法内容依赖、无有效 Approval Grant 的副作用、工程 revision 冲突、不可解释的渲染差异和 Credential 泄漏。

模型数、工具数、插件数和音色数不是成功指标。

## 13. MVP 验收标准

1. 用户只配置一个 LLM Connection，即可发起真实音乐创作；
2. LLM 通过至少两种不同语义工具形成并修改 Music Project，而不是只输出说明文本；
3. Project 保存结构、Tempo、轨道、MIDI、乐器、Mix 和自动化，关闭应用后可恢复；
4. Factory Path 在没有第三方插件时产生可试听的本地音频；
5. 用户可以针对一个局部提出修改，未选区域保持不变；
6. Candidate 绑定 Project Snapshot、Preview、指标、内容与插件依赖；
7. Selection 由用户执行，Agent 不能自动采用；
8. 同一个 Selection 导出 WAV、stems、Type-1 MIDI、assignment/provenance manifest，并在冻结版本的 Cubase、Studio One Pro、FL Studio 中继续编辑；每个 DAW 分别声明 Portable/Structured/Sound-identical 等级；
9. 一个隔离 VST3 instrument/effect 路径通过固定 corpus、恢复、freeze 和 crash containment；
10. 固定 Brief corpus 完成机器 Gate 与人工盲听，结果达到预先冻结阈值；
11. production composition root 不包含 Music Provider、Fake 音乐生成或外部 prompt-to-WAV fallback；
12. 所有“已完成”声明有代码、测试或 live evidence；缺少人工/账号/目标 OS/DAW 输入时标记 `BLOCKED` 或 `LIVE-PENDING`。

## 14. 主要风险

| 风险 | 产品对策 |
|---|---|
| 文本 LLM 音乐理论正确但作品听感机械 | humanize、articulation、演奏模板、固定盲听 corpus；不只看 MIDI 合法性 |
| 工具 schema 太底层导致 token 成本和错误上升 | 使用段落、和声、pattern、region 级深 Tool；不要让 LLM 逐 sample 或逐字节编辑 |
| Factory Pack 音质不足 | 小而精的基础包、精确许可与盲听 Gate；质量不足时不靠增加数量掩盖 |
| VST3 兼容工作拖垮 MVP | Factory Path 先行，VST3 固定 OS/corpus/Profile，隔离并允许 freeze |
| Agent 修改破坏已有音乐 | expected revision、region scope、diff preview、Snapshot 与 Selection |
| Provider 推理链恢复要求与“不保存 private reasoning”冲突 | 规范化 Transcript 与加密、run-scoped Continuity State 分离；终态 purge |
| 用户一次授权被扩张到未预期轨道或渲染 | Approval Grant 绑定 revision/tool/target/effect，Run Budget 独立封顶 |
| Spike schema 过早固化生产 API | 只命名为 ExperimentalMusicSpec；先分析失败分布，再设计深 Tool Interface |
| LLM 无真实音频理解能力 | 明示能力边界，使用机器分析和用户反馈；音频输入模型需单独验证后才能启用 |
| 重新定义后旧 Fixture 代码造成误导 | 标记 legacy，移除 production 路径，并用架构/测试禁止 Music Provider 回流 |

## 15. 决策依据

- [共同语言](../../CONTEXT.md)
- [技术设计](../design/auto-studio-technical-design.md)
- [统一 Roadmap](../roadmap.md)
- [ADR-0011：由 LLM 通过本地工具创作音乐](../adr/0011-llm-authored-local-music.md)
- [ADR-0012：Durable Agent Harness State](../adr/0012-durable-agent-harness-state.md)
- [Q0 音乐内容可行性 Spike](../planning/2026-08-24-music-quality-spike-design.md)
- [真实乐器采样与 Rust 音频栈研究](../research/instrument-sample-libraries-and-rust-audio-stack-2026-08-21.md)
- [Agent Harness 模式研究](../research/agent/agent-run-harness-patterns-2026-08-23.md)
