# Auto Studio Roadmap

> 基线日期：2026-08-24  
> 当前执行 Gate：Q0 音乐内容可行性 Spike  
> 当前结论：M1/M2 已证明本地 Core、Project、TUI 和真实 LLM Planning；旧“真实 Music Provider”路线已取消。Q0 实验装置与真实 DeepSeek/MIDI pilot 已完成，正式 A/B 正在执行，真人/DAW Gate 仍待完成；M3 production 尚未开工，当前产品仍不能真实生成音乐。

## 1. 状态语言

| 状态 | 含义 |
|---|---|
| `PASS` | 有当前代码和自动化/实测证据 |
| `PARTIAL` | 一部分合同已实现，关键纵切未完成 |
| `IN PROGRESS` | 正在实施，尚未达到完成定义 |
| `LIVE-PENDING` | 本地合同通过，仍需真实账号、硬件、OS、插件或 DAW 实测 |
| `BLOCKED` | 缺少必须的产品输入、法律批准或外部条件 |
| `NOT IMPLEMENTED` | 只有设计，没有可运行代码 |
| `LEGACY` | 旧方向代码，仅用于迁移/历史测试，不属于目标 production runtime |
| `SKIP` | 当前环境不具备前置条件，不能算 PASS |

## 2. 当前进度快照

| 能力 | 状态 | 已有证据 | 下一缺口 |
|---|---|---|---|
| Rust Core/API | `PASS` | Axum、discovery/session、健康检查、独立进程 | installer/OS hardening |
| Project/SQLite | `PASS` | revision、snapshot、event/outbox、backup、single writer | Music Project schema/migration |
| TUI | `PASS` | `autostudio`、Composer、`/connect`、`/model`、Thinking、`/exit` | Project/Run/Candidate/Render 视图 |
| LLM Provider | `PASS（contract）` | OpenAI/Anthropic/DeepSeek/Kimi 协议与目录 | exact model live qualification |
| LLM Planning | `PASS（contract）` | typed Plan、Approval、真实 composition root | 从一次 Plan 扩展为多轮 Tool loop |
| Harness Foundation | `NOT IMPLEMENTED` | ADR-0012 与图已冻结 | Transcript、Continuity Vault、Approval Grant、Run Budget |
| 旧 GenerationAdapter | `LEGACY` | Fixture 状态机、WAV ingest、Candidate contract | 停止 production 扩展并迁移/删除 |
| Candidate/Selection | `PARTIAL` | Audio-only Fixture contract | Candidate Project Snapshot |
| Music Project Model | `NOT IMPLEMENTED` | 目标设计已冻结 | domain、commands、projection、migration |
| Tool Registry/Runtime | `NOT IMPLEMENTED` | ADR/架构图 | 固定 catalog、Policy、ToolExecution |
| MIDI/Arrangement | `PASS（Q0 experiment）` / `NOT IMPLEMENTED（production）` | 严格 ExperimentalMusicSpec → Type-1 SMF、480 PPQ、tempo/拍号/key/marker/track/note/CC 实测 | production Music Project 与 Semantic Tool |
| Offline Audio Engine | `NOT IMPLEMENTED` | `hound` 只用于 WAV 合同 | Render Plan、instrument、mix、analysis |
| Factory Pack/Sampler | `NOT IMPLEMENTED` | 研究与候选清单 | 许可批准、manifest、sampler、盲听 |
| VST3 Host | `NOT IMPLEMENTED` | 目标隔离设计 | 目标 OS、SDK Spike、worker、corpus |
| MCP | `NOT IMPLEMENTED` | 目标设计 | Post-MVP，不阻塞本地音乐 Gate |

仓库安全状态：已配置仓库级 Git identity，`main` 跟踪 `origin/main`，并建立可回退 baseline commit `b9db99c`。远端基线仍可追溯；Q0 不删除 legacy 文件。

## 3. 里程碑总览

| 里程碑 | 目标 | 状态 |
|---|---|---|
| M0 Workspace Baseline | Rust workspace、Core、Project、API、基础 TUI | `PASS` |
| M1 Local Product Shell | `autostudio` 启动、Connection、Model/Thinking、Project | `PASS` |
| M2 LLM Planning Contract | 真实 LLM typed Plan、Approval、本地持久化合同 | `PASS（live 需 Key）` |
| Q0 Music Content Feasibility | 用 MIDI/固定 DAW 验证 L1—L4 Keep 与真实继续编辑 | `IN PROGRESS`：装置/pilot PASS，正式 A/B 运行中，真人/DAW `LIVE-PENDING` |
| M3 LLM-Authored Local Music Foundation | Durable Harness + 本地 Tool + 可编辑 Music Project + 离线发声 | `NOT IMPLEMENTED（等待 Q0 GO）` |
| M4 Factory Quality Vertical Slice | Sampler/Factory Pack/Mix/Analysis/Candidate 质量闭环 | `NOT IMPLEMENTED` |
| M5 Professional MVP Handoff | 受限 VST3、freeze、WAV/stems/MIDI、目标 DAW | `NOT IMPLEMENTED` |
| M6 Release Qualification | 固定 corpus、盲听、设计伙伴、Vault、安装签名 | `BLOCKED` |
| M7 MCP / Video / More Clients | 受控生态与后续媒体 | `NOT IMPLEMENTED（Post-MVP）` |

## 4. Q0：Music Content Feasibility

Q0 不建设 production Audio Engine。完成定义是：

> 使用冻结的 12 个 L1—L4 Brief、精确强模型、ExperimentalMusicSpec、MIDI、固定 DAW/version/template 与合法音色映射，完成匿名评价和真实 continued-editing 记录，并形成 `GO/REVISE/NO-GO/INVALID` 决策。

### 4.1 Q0-0：仓库与协议基线

- [x] 配置正确的 Git identity 并建立包含当前代码/文档的 baseline commit `b9db99c`；
- [x] 完成当前 workspace 的纳入/忽略与 secret/generated/large-file 审计；
- [x] 创建 `experiments/music-quality/` 独立 workspace 与 lockfile，不加入 production workspace；
- [x] 冻结 12 个 Brief、ExperimentalMusicSpec、prompt、评分表和 `protocol.lock.json`；
- [x] 冻结主模型、DAW/version/import recipe 与 instrument mapping；
- [ ] 第二个不同强模型只在主模型负面时启用，仍需 Credential 与精确 model；
- [x] 记录音色来源、license/EULA、本地使用权和 hash。

完成定义：实验输入、阈值、环境和代码都可追溯；未建立 baseline 时不删除 legacy 文件。

### 4.2 Q0-A：实验装置

- [x] schema/parser/compiler 单元测试；
- [x] ExperimentalMusicSpec → SMF MIDI/tempo/拍号/key/marker/track/note/CC；
- [x] 不计入结果的真实 DeepSeek Mode A/Mode B pilot；
- [ ] Bitwig MIDI import、固定音色装载和工程保存 smoke 为 `LIVE-PENDING`；
- [x] Provider request/response normalization、逐轮恢复、usage/cost/latency 记录；
- [x] 匿名 candidate ID、evaluator package、独立 private mapping 与编辑动作表；
- [x] Credential、private reasoning 和不允许分发音色不进入 artifact；
- [x] formal verifier 按 Plan 校验 4 个 A/12 个 B、Provider identity 与 artifact hash。

完成定义：Mode B 至少 11/12 可无人工修 JSON 编译并导入，否则只修实验装置，不判断产品前提。

### 4.3 Q0-B：运行与决策

- [ ] Mode B 跑全部 12 个 Brief；
- [ ] Mode A 跑 4 个代表 Brief；
- [ ] Mode C 跑全部 6 个 L4 Brief，最多两轮 Creator 反馈；
- [ ] 记录 Keep、Actual continued editing、Time to useful、Edit distance、Structural errors、内容评分与成本；
- [ ] 主模型未过 L4 门槛时，用第二个不同强模型复核；
- [ ] 保存完整证据并发布 `GO/REVISE/NO-GO/INVALID` 报告。

完成定义与阈值以 [Q0 Spike 设计](planning/2026-08-24-music-quality-spike-design.md) 为准。只有 `GO` 允许开始 M3 production 实施；Q0 不替代后续 Factory/VST3/Release Gate。

## 5. M3：LLM-Authored Local Music Foundation

M3 不接入 Music Provider。完成定义是：

> 用户输入 Brief 后，一个真实 LLM 在 production runtime 中调用至少两类本地 Semantic Tool，形成可恢复的 Music Project Snapshot，由本地 Rust 路径渲染出可验证 Preview，并创建可编辑 Candidate；过程不使用外部音乐生成 API 或 Fake WAV。

### 5.1 M3-A：Harness Foundation 与迁移基线

- [ ] ADR-0011、ADR-0012、产品、技术、Roadmap 与领域词汇成为唯一权威；
- [ ] production composition root 明确禁止 Music Provider/Fixture；
- [ ] `autostudio-provider` 的目标职责改为 LLM-only；
- [ ] 旧 Generation 状态、API 和测试建立迁移清单并冻结，标记 `LEGACY`，暂不删除；
- [ ] 定义 AgentStep、InferenceTurn、InferenceItem、ToolRequest、ToolResult 的独立 identity 与顺序；
- [ ] durable Inference Transcript 与 partial tool-call assembler；
- [ ] Provider Adapter continuity capture/replay contract；
- [ ] Project 外加密 Continuity Vault、binding/TTL/purge/janitor；
- [ ] Approval Grant 绑定 revision/plan/tool fingerprint/target/effect/cost；
- [ ] Run Budget 与 Tool Resource Limit 独立 ledger/enforcement；
- [ ] OpenAI/Anthropic continuity fixtures、mismatch/corruption/purge 与 secret-sentinel 测试；
- [ ] 添加架构守护测试，禁止新的 production `GenerationAdapter` 注册；
- [ ] 删除 TUI/Desktop 中“配置真实 Music Provider”“查询 Provider”“Unknown Outcome”产品文案。

完成定义：Run 可以持久化规范化 Transcript，安全继续或明确放弃 Provider 链，精确校验 Grant/Budget，并在终态清理 continuity；运行产品不再要求 Music Provider。

### 5.2 M3-B：Music Project Domain

- [ ] 定义 TempoMap、Arrangement/Section、Track、MidiClip、Note、CC、InstrumentAssignment、MixGraph、Automation；
- [ ] 定义 typed MusicProjectCommand 与 ProjectChangeSet；
- [ ] 为 ID、时间、拍号、note/CC、范围、voice budget 建立不变量；
- [ ] Candidate 升级为 `ProjectSnapshot + Preview + Analysis`；
- [ ] Selection 继续保持 Creator-only command；
- [ ] SQLite schema/migration 与 backup/restore；
- [ ] expected revision/conflict 与 property tests。

完成定义：不依赖 LLM 或音频引擎，Domain 可以创建、修改、快照和恢复一个确定 Music Project。

### 5.3 M3-C：Tool Registry 与 Durable ToolExecution

- [ ] `ToolDescriptor`：name/revision/schema/side-effect/approval/replay/resource/fingerprint；
- [ ] `ToolRequest`、`ExecutionBinding`、`ToolResult` 和稳定 identity；
- [ ] 固定 catalog，不实现动态插件式 registry；
- [ ] schema、capability、Policy、Approval Grant、Run Budget、Tool Resource Limit 和 expected revision；
- [ ] ToolExecution 持久化、non-terminal query、RunProjection 与 SSE；
- [ ] 幂等重放、identity reuse 拒绝、atomic result commit；
- [ ] cancel、deadline、bounded wake 和 Core 启动恢复；
- [ ] 错误与常量继续放入独立模块，不回到业务实现文件。

完成定义：两个不同的真实本地 Adapter 通过同一 Tool Runtime Interface；一个测试 Adapter 不能证明 seam。

### 5.4 M3-D：最小音乐 Tool

最小 catalog：

- [ ] `project.describe`；
- [ ] `arrangement.set_structure`；
- [ ] `tempo.set_map`；
- [ ] `track.create_instrument`；
- [ ] `harmony.write_progression`；
- [ ] `midi.write_region`；
- [ ] `midi.edit_region`；
- [ ] `instrument.assign`；
- [ ] `mix.set_parameters`；
- [ ] `render.preview`；
- [ ] `audio.analyze`；
- [ ] `candidate.create`。

Tool 以 section/region/pattern 为主要粒度，不让 LLM 逐 sample、逐 MIDI byte 或逐 crate function 调用。

完成定义：工具合同覆盖成功、schema 拒绝、revision conflict、budget、cancel、crash recovery 和结果清洗。

### 5.5 M3-E：有界 LLM Tool Loop

- [ ] LLM request 带 Tool Catalog Snapshot；
- [ ] 支持 streaming visible text 与完整 tool-call assembly；
- [ ] canonical Inference Item append 与 ToolRequest/ToolResult 对应关系；
- [ ] continuity reference 只交回兼容 Adapter，不进入 Project/Client；
- [ ] 普通文本、单 Tool、多 Tool、非法 JSON、未知 Tool、partial stream；
- [ ] Tool Result 作为下一 Inference Turn 的结构化观察；
- [ ] max turns/tools/tokens/cost/wall-clock/render；
- [ ] Context Snapshot 与 compaction，不改完整 Tool item/Grant/Budget/Project facts；
- [ ] LLM interruption/unknown token consumption 与 Tool failure 分开；
- [ ] TUI 展示 Planning/Approval/Applying/Quality/AwaitingSelection/NeedsAttention；
- [ ] Run Inspector 展示 Transcript/Grant/Budget/ToolExecution，不展示 continuity/private reasoning。

完成定义：真实 LLM 能调用至少一个 Project/MIDI Tool 和一个 Render/Analysis Tool；模型持续在线不是已提交 Tool 恢复的前置。

### 5.6 M3-F：最小本地离线发声

- [ ] Render Plan 与 deterministic offline worker；
- [ ] MIDI event → sample time；
- [ ] 一个明确受限的内置音源/测试 Instrument，用于验证引擎而不是冒充 Factory 音质；
- [ ] volume/pan 与最小 mix graph；
- [ ] staged WAV、final-copy hash、format/duration/sample count 校验；
- [ ] silence/clipping/peak/basic loudness analysis；
- [ ] Core crash、render crash、duplicate execution 故障注入；
- [ ] Preview Asset 与 Candidate Project Snapshot 原子提交。

完成定义：真实 LLM → 本地 Tool → Music Project → 本地 Preview 的 production 纵切通过。此 Gate 证明架构和可编辑性，不证明最终内容质量。

## 6. M4：Factory Quality Vertical Slice

### 6.1 M4-A：Factory Pack 法律与供应链

- [ ] 冻结一小组钢琴、弦乐、鼓、贝斯候选；
- [ ] 核对软件再分发、商业成品、署名、修改和更新权；
- [ ] 保存来源、许可证文本、hash、转换与批准记录；
- [ ] 设计 pack manifest、安装、升级、卸载与 lock；
- [ ] RED/NOT APPROVED 内容不得进入 Catalog。

### 6.2 M4-B：Sampler 与演奏质量

- [ ] zone、key/velocity、RR、loop、tuning、envelope；
- [ ] sustain/pedal、articulation、voice stealing；
- [ ] fixed-capacity voice/resource budget；
- [ ] 音高、边界、loop click、RR 与恢复测试；
- [ ] Factory Instrument/Preset semantic descriptors。

### 6.3 M4-C：Mix、分析与 Candidate

- [ ] 基础 EQ、compressor、reverb/send；
- [ ] automation；
- [ ] LUFS、True Peak、dynamic range、clipping、silence；
- [ ] Candidate A/B、工程 diff 和局部修改保持；
- [ ] Fixed Brief corpus 与机器 Gate；
- [ ] 第一轮人工盲听。

完成定义：没有第三方 VST3 时，Factory Path 能产生达到预先冻结阈值的可编辑 Candidate。

## 7. M5：Professional MVP Handoff

### 7.1 M5-A：受限 VST3 Host

- [ ] 冻结一个首发 OS；
- [ ] Steinberg SDK/C API、许可、商标和分发 Spike；
- [ ] scanner worker、binary hash、Plugin UID 与 trust status；
- [ ] plugin worker、bounded IPC、audio/MIDI bus；
- [ ] instrument/effect、preset/state、latency/PDC、offline render；
- [ ] Approved Plugin Profile 与 Agent 参数范围；
- [ ] freeze、missing plugin、crash/hang/timeout；
- [ ] 固定 plugin/version corpus。

完成定义：至少一个 instrument 和一个 effect 在目标 OS 通过扫描、运行、state 恢复、freeze 和 crash containment。不得用 `nih-plug` 冒充 Host。

### 7.2 M5-B：DAW Handoff

- [ ] stereo WAV；
- [ ] stems；
- [ ] MIDI；
- [ ] Tempo/拍号/markers；
- [ ] Pack/Plugin/engine lock；
- [ ] credits/license/provenance；
- [ ] 在一个冻结 DAW/version 中导入和继续编辑；
- [ ] 不承诺未验证的原生工程文件。

完成定义：设计伙伴能把 Selection 带到目标 DAW，并在不重建全部结构的前提下继续制作。

## 8. M6：质量与发布资格

- [ ] 冻结首发 ICP、OS、LLM Provider/Model/Thinking、DAW/version；
- [ ] 冻结 Brief corpus、机器阈值、盲听表与样本量；
- [ ] exact model 真实 token/cost/latency/Tool call 资格；
- [ ] OS Credential Vault；
- [ ] installer、Core/TUI/VST3 worker 成对安装；
- [ ] 签名、公证、升级、卸载与干净机；
- [ ] Factory Pack/VST3/FFmpeg/依赖 SBOM、notice 与许可证；
- [ ] 至少一轮设计伙伴采用与继续编辑数据；
- [ ] crash/soak/performance/security release Gate。

完成定义：所有阻塞 Gate 有 evidence；未完成 live/human/OS/DAW 输入不得用 Fixture 或 `SKIP` 填成 PASS。

## 9. Release Gates

### Gate Q0：内容可行性

- `IN PROGRESS`：实验装置与真实 pilot 已通过；正式 A/B 正在运行，Bitwig import、Mode C、盲听与 continued-editing 尚未完成；
- 通过条件：实验装置有效，L4 达到冻结 Keep/Actual continued editing 门槛；负面主模型结果由第二个不同强模型复核。

### Gate A：产品基线

- `PASS`：一句话定位、Local-first、单 Agent、Selection/Approval 分离；
- `BLOCKED`：首发 ICP、OS、DAW/version 和设计伙伴尚未冻结。

### Gate B：LLM Agent Harness

- `PASS（contract）`：LLM Connection、Model/Thinking、typed Planning；
- `NOT IMPLEMENTED`：Inference Transcript、Continuity Vault、Approval Grant、Run Budget、真实多轮 Tool loop；
- 通过条件：continuity/Transcript 分离，Grant/预算范围生效，重启/compaction/终态 purge 和 exact model live qualification 通过。

### Gate C：本地可编辑音乐

- `NOT IMPLEMENTED`：Music Project、MIDI Tool、Render Plan、Sampler；
- 通过条件：production 无 Music Provider/Fake，真实 LLM 创建并修改可恢复工程。

### Gate D：内容质量

- `BLOCKED`：Factory Pack 法律批准、Sampler、fixed corpus、机器阈值和人工盲听；
- 通过条件：Factory Path 达到冻结技术与听感阈值。

### Gate E：专业交接

- `PARTIAL（Fixture）`：已有 Audio-only Handoff contract；
- `BLOCKED`：Project Snapshot Candidate、MIDI/stems 与目标 DAW 继续编辑。

### Gate F：VST3

- `NOT IMPLEMENTED`：只有设计；
- 通过条件：一个 OS、固定 instrument/effect corpus、隔离、state、freeze、许可。

### Gate G：安全与分发

- `PARTIAL`：loopback session、私有 bootstrap、Rust test/lint；
- `BLOCKED`：OS Vault、签名、installer、SBOM、内容/SDK notices、干净机。

## 10. 明确不进入当前执行顺序

- Music Provider、Mureka、Lyria、Eleven Music、Stable Audio；
- 第二个音乐生成后端或 Provider capability matrix；
- Multi-Agent；
- 通用工作流编排平台；
- MCP marketplace 或 Auto Studio MCP Server；
- 视频与短剧；
- 公开 Web/云数据库/多租户；
- 多 OS 同时资格；
- 完整 VST3 兼容矩阵；
- Cubase/Studio One 专有原生工程写入。

这些工作只有在本地音乐纵切与内容质量出现证据后才能重新排序。

## 11. 状态更新规则

每次里程碑更新必须同时记录：

1. 具体代码/测试/live evidence；
2. Fixture、contract、live、human、OS/DAW qualification 的边界；
3. 新增依赖、crate/process 和 release surface；
4. 未通过项的 `BLOCKED/LIVE-PENDING/SKIP` 原因；
5. 产品、技术、ADR、共同语言和 Roadmap 是否仍一致。

禁止：

- 用旧 Generation Fixture 声称 LLM 已真实生成音乐；
- 用编译通过声称音质、实时或 VST3 兼容通过；
- 用“可商业使用”替代软件再分发许可；
- 用 ignored live test 或缺失 API Key 结果标记 PASS；
- 在 Music Project/Tool Runtime 未稳定前扩展 MCP/Multi-Agent。

## 12. 关联文档

- [产品设计](product/ai-creative-agent-product-design.md)
- [技术设计](design/auto-studio-technical-design.md)
- [共同语言](../CONTEXT.md)
- [ADR-0011](adr/0011-llm-authored-local-music.md)
- [ADR-0012](adr/0012-durable-agent-harness-state.md)
- [Q0 音乐内容可行性 Spike](planning/2026-08-24-music-quality-spike-design.md)
- [Agent Harness 研究](research/agent/agent-run-harness-patterns-2026-08-23.md)
- [真实乐器采样与 Rust 音频栈研究](research/instrument-sample-libraries-and-rust-audio-stack-2026-08-21.md)
