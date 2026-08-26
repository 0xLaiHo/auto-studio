# Auto Studio Roadmap

> 基线日期：2026-08-27
> 当前执行 Gate：Q0 真人内容 Gate；M3-A architecture/migration baseline 已完成，M3-B 等待 Q0 `GO`
> 当前结论：M1/M2 已证明本地 Core、Project、TUI 和真实 LLM Planning；旧“真实 Music Provider”路线已取消。Q0 protocol v2 正式 A/B 的 Mode B 11/12、protocol v3 的 6/6 L4 重基线和 Portable Handoff v1 机器切片已通过；DAW qualification harness 已实现并将缺少的 Cubase、Studio One Pro、FL Studio 诚实记为 3 个 `not_run`。Creator 已完成一次 Bitwig 手动 Pilot，但真人内容 Gate 与三目标 DAW 实测仍为 `LIVE-PENDING`。M3-A CM-0—CM-4、Grant/Budget machine slices 与 legacy boundary 已落地：`autostudio-provider` 默认只编译 LLM 职责，旧 Generation runtime 仅能通过非默认 feature 回归，Core/TUI/Desktop production source 受架构门禁保护。Grant/Budget 尚未接入 composition root、Policy 或 durable ToolExecution；Music Project 也没有独立 revision，因此当前产品仍不能真实生成音乐。

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
| LLM Provider | `PASS（contract + DeepSeek/OpenAI live）` | OpenAI/Anthropic/DeepSeek/Kimi 协议与目录；2026-08-25 `deepseek-v4-flash` 流式 Tool Call smoke；2026-08-26 `gpt-5-mini` 两轮 Responses Continuity live | Anthropic/Kimi exact-model `LIVE-PENDING` |
| LLM Planning | `PASS（CM-1 contract）` | typed Plan、Approval、真实 composition root、固定两轮本地 Tool 链路 | 接 Music Project Semantic Tool，而非扩大固定规划工具 |
| Harness Foundation | `PARTIAL（M3-A architecture baseline PASS）` | Durable Context/Continuity/long-run retrieval；不可变 Grant、configured/system ceiling、独立 ledger、Execution Reservation、SQLite CAS、故障/重启/篡改合同；legacy feature/production source guard | composition root/Policy/durable ToolExecution；真实音乐 Tool 长 Run 质量、exact tokenizer/live overflow qualification |
| 旧 GenerationAdapter | `LEGACY / FROZEN` | 默认 feature 不编译；显式兼容 feature 保留 Fixture、旧 Project 与 Unknown Outcome contract | 新 ToolExecution/API/migration 完成后删除 |
| Candidate/Selection | `PARTIAL` | Audio-only Fixture contract | Candidate Project Snapshot |
| Music Project Model | `NOT IMPLEMENTED` | 目标设计已冻结 | domain、commands、projection、migration |
| Tool Registry/Runtime | `NOT IMPLEMENTED` | ADR/架构图 | 固定 catalog、Policy、ToolExecution |
| MIDI/Arrangement | `PASS（Q0 v2/v3 + portable machine）` / `NOT IMPLEMENTED（production）` | 严格 ExperimentalMusicSpec → Type-1 SMF、480 PPQ、tempo/拍号/key/marker/track/note/CC；Portable v1 增加 per-track CC0/CC32/Program Change 与 assignment manifest，并由固定 GeneralUser GS 离线解析 | 真人内容 Gate、Cubase/Studio One/FL Studio 导入矩阵、production Music Project 与 Semantic Tool |
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
| Q0 Music Content Feasibility | 用可移植 MIDI/冻结 DAW matrix 验证 L1—L4 Keep 与真实继续编辑 | `PASS（v2 11/12 + v3 L4 6/6 + portable handoff machine）` / `LIVE-PENDING（human/cross-DAW）` |
| M3 LLM-Authored Local Music Foundation | Durable Harness + 本地 Tool + 可编辑 Music Project + 离线发声 | `IN PROGRESS（M3-A CM-0—CM-4 + Grant/Budget machine slices PASS；Tool Runtime 未实现；音乐纵切仍等待 Q0 GO）` |
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
- [x] 保持 v2 lock/证据不可变，冻结 `protocol-v3-l4.lock.json`，对全部 6 个 L4 重建可比较 B 基线；
- [x] 冻结主模型、DAW/version/import recipe 与 instrument mapping；
- [ ] 第二个不同强模型只在主模型负面时启用，仍需 Credential 与精确 model；
- [x] 记录音色来源、license/EULA、本地使用权和 hash。

完成定义：实验输入、阈值、环境和代码都可追溯；未建立 baseline 时不删除 legacy 文件。

### 4.2 Q0-A：实验装置

- [x] schema/parser/compiler 单元测试；
- [x] ExperimentalMusicSpec → SMF MIDI/tempo/拍号/key/marker/track/note/CC；
- [x] Portable Handoff v1：按语义解析冻结 Instrument Profile，写入 CC0/CC32/Program Change，并输出含库 hash/许可结论的 `instrument-assignments.json`；
- [x] DAW qualification harness：冻结 handoff/target plan，生成 `not_run` template，校验精确版本、checklist、PNG/JPEG、保存工程、edited MIDI 与所有 evidence hash；
- [x] 不计入结果的真实 DeepSeek Mode A/Mode B pilot；
- [x] Creator 已完成一次 Bitwig 三轨导入、手动固定音色装载和工程保存/重开 Pilot；
- [ ] 正式 Bitwig checklist/仓内截图/edited MIDI，以及 Cubase、Studio One Pro、FL Studio 导入矩阵为 `LIVE-PENDING`；当前三目标计划为 `0 pass / 0 fail / 3 not_run`，MVP Gate=false；
- [x] Provider request/response normalization、逐轮恢复、usage/cost/latency 记录；
- [x] 匿名 candidate ID、evaluator package、独立 private mapping 与编辑动作表；
- [x] Credential、private reasoning 和不允许分发音色不进入 artifact；
- [x] formal verifier 按 Plan 校验 4 个 A/12 个 B、Provider identity 与 artifact hash。
- [x] v3 runner 支持逐 Run 协议 SHA-256 绑定、从任意已落盘 Mode B 回合恢复，以及最多一次仅针对全局 note/CC 预算的可审计修订；
- [x] v3 verifier 拒绝选择性样本、协议漂移、缺失 binding、无合法 trigger 的第 4 回合和 artifact 损坏；

完成定义：Mode B 至少 11/12 可无人工修 JSON 编译并导入，否则只修实验装置，不判断产品前提。

### 4.3 Q0-B：运行与决策

- [x] Mode B 跑全部 12 个 Brief；11/12 valid + compiled；
- [x] Mode A 跑 4 个代表 Brief；4/4 valid + compiled；
- [x] 按 v3 在独立目录重新运行全部 6 个 L4 Mode B；6/6 valid + compiled，601,537 tokens，peak USD 1.933172824，0 个资源修订回合；
- [ ] Mode C 跑全部 6 个 L4 Brief，最多两轮 Creator 反馈；
- [ ] 记录 Keep、Actual continued editing、Time to useful、Edit distance、Structural errors、内容评分与成本；
- [ ] 主模型未过 L4 门槛时，用第二个不同强模型复核；
- [ ] 保存完整证据并发布 `GO/REVISE/NO-GO/INVALID` 报告。

完成定义与阈值以 [Q0 Spike 设计](planning/2026-08-24-music-quality-spike-design.md) 为准。只有 `GO` 允许开始 M3 production 实施；Q0 不替代后续 Factory/VST3/Release Gate。

## 5. M3：LLM-Authored Local Music Foundation

M3 不接入 Music Provider。完成定义是：

> 用户输入 Brief 后，一个真实 LLM 在 production runtime 中调用至少两类本地 Semantic Tool，形成可恢复的 Music Project Snapshot，由本地 Rust 路径渲染出可验证 Preview，并创建可编辑 Candidate；过程不使用外部音乐生成 API 或 Fake WAV。

### 5.1 M3-A：Harness Foundation 与迁移基线

- [x] ADR-0011、ADR-0012、产品、技术、Roadmap 与领域词汇成为唯一权威；
- [x] production composition root 保持 planning-only，不注册 Music Provider/Fixture；
- [x] `autostudio-provider` 默认职责改为 LLM-only；旧路径只在非默认 `legacy-generation` feature 下编译；
- [x] 旧 Generation 状态、API 和测试按 [迁移清单](planning/legacy-generation-migration.md) 冻结为 `LEGACY`，暂不删除；
- [x] 定义 `AgentRunId`、`InferenceTurnId`、`InferenceItemId`、完整 Tool Request/Result 条目与单调顺序；`AgentStepId`/`ToolExecutionId` 留待 Tool Runtime；
- [x] durable Inference Transcript：同一 Project SQLite actor 内按 Run append、CAS revision、重启 replay；
- [x] `ContextManifest`：持久化精确 instruction、Tool catalog、included item、Provider binding、token budget 与内容 hash，并在 Provider 调用前落盘；
- [x] OpenAI Chat、OpenAI Responses、Anthropic Messages 从统一 `InferenceTurnRequest/PreparedContext` 生成请求；
- [x] 在首次 Inference 前创建可见 `planning` Agent Run；成功后附加 typed Plan，Provider/Context 失败后写入无 Plan 的终态 `failed`，TUI/Desktop DTO 能读取该形状，失败后允许新 Run；
- [x] OpenAI Chat/Responses、Anthropic Messages 的 SSE decoder、protocol delta mapper 与 partial tool-call assembler；仅完整 canonical Turn 可进入 Transcript；
- [x] 固定 Planning Tool Module：真实只读 `project_describe` 后才开放 terminal `submit_creative_plan`；完整 Request/Result 配对持久化；
- [x] 每一步重新打开 Project 并 replay Transcript；待执行 Tool、已完成 Plan 和仅有 Manifest 的中断分别恢复、提交或安全失败；Core API、TUI 与 Desktop 暴露 Planning resume；
- [x] Provider Adapter continuity capture/replay contract；
- [x] Project 外加密 Continuity Vault、binding/TTL/purge/janitor；
- [x] Compaction checkpoint domain、稳定内容 hash、append-only Context Event 与 SQLite CAS 原子提交；
- [x] 完整 Transcript 保留、重启 replay、最新 structured summary + kept tail 的 Context Surface 与 Manifest checkpoint binding；
- [x] 拒绝非连续 prefix、重复不推进 cut、拆分/隐藏 pending Tool pair；三种 Provider wire 均把 summary 保持为 untrusted user context；
- [x] deterministic Request Footprint：测量 canonical instructions/messages/Tool schema，加上 Adapter continuity allowance；Planning 使用 16,384 token 的 host-owned 保守安全 ceiling（不是模型能力声明），按 75% soft、90% hard、超预算 overflow 分级；
- [x] 大 Tool Result deterministic spill：超过 16 KiB 时模型只见 512 字符预览、source item/hash/原始字节数引用；完整 Transcript 保留，content-addressed blob 与 Manifest 同事务提交，覆盖 hash、回滚、重启与 Project backup；
- [x] 自动 safe-cut：只选完整 Turn 边界、精确连续前缀、不拆 Tool pair、不删除新输入并至少保留最近两轮；生成有界 structured summary，只有 prepared surface 实际变短且回到 Normal 才提交；
- [x] Creator 新输入、Checkpoint、Manifest 与 spill 同事务提交；故障注入证明失败零落盘，确定性重试得到相同 checkpoint content hash，重启由完整 Transcript + checkpoint 重建；
- [x] OpenAI/Anthropic/DeepSeek-compatible 明确 overflow code/message 映射为 `ContextOverflow`；清除旧 Continuity 后最多恢复一次，第二次以可见失败停止；
- [ ] Provider-specific 精确 tokenizer 校准与真实 Provider overflow live qualification；超长 single-turn 目前在无安全 cut 时 fail closed；
- [x] Approval Grant 绑定 Creator action/run/revision/plan-or-step/tool fingerprint/target/side-effect/effect/cost/issue/expiry；
- [x] Run Budget 与 Tool Resource Limit 独立 ledger/enforcement；configured budget 不能提高 system ceiling；active wall-clock 不含等待/跨日暂停；
- [x] OpenAI/Anthropic continuity fixtures、mismatch/corruption/purge 与 secret-sentinel 测试；
- [x] 添加架构守护 Gate，禁止 Core/TUI/Desktop production source 注册、调用或暴露旧 Generation runtime，并检查默认 feature；
- [x] 删除 TUI/Desktop 中旧 `/generate`、`/recover`、Provider Job 查询和 Unknown Outcome 操作入口；历史状态只读。

CM-1 完成边界：固定 Planning 纵切可持久化规范化 Transcript、执行真实本地只读 Tool、跨进程继续，并对结果不明的 Provider Turn 明确放弃而不重提。

CM-2 Planning slice 完成边界：OpenAI Responses 的完整 reasoning/function item 与 Anthropic Messages 的 signed thinking/tool-use block 由 Adapter 捕获并原样回传；XChaCha20-Poly1305 Vault 位于 Project 外，使用独立本地密钥，绑定 run/provider/model/protocol/thinking/capability/mapping/tool catalog，支持 7 天 TTL、启动和每小时 janitor、错配/损坏删除与终态 purge。composition root 拒绝工程内或经符号链接落入工程的 Vault/key 路径。契约测试证明 sentinel 不进入 Project SQLite、Context Event、backup 或 Debug；purge 失败不会提交成功 Plan。OpenAI `gpt-5-mini` exact-model live 已用完整两轮 Planning Tool loop 通过；Anthropic exact-model live 与 OS Credential Vault 仍为 `LIVE-PENDING`。DeepSeek Chat 只走 canonical Transcript fallback。CM-4、Grant/Budget machine slices 与 legacy Generation architecture boundary 均已完成；运行产品不要求 Music Provider。Provider-specific tokenizer、真实 overflow 与真实音乐 Tool 长 Run 正确率继续作为资格项，不把 M3-A 架构基线重新打开。

CM-3 planning slice 完成边界：`prepare_turn` 由完整 Transcript 派生 current surface，先 spill 大 Tool Result，再按压力或显式 Provider overflow 触发 automatic compaction。cut 必须位于完整 Turn 边界、推进连续前缀、不拆 Tool pair、保留新输入与最近两轮；host-owned structured summary 记录 objective、Creator decisions、constraints、completed work 和 artifact execution references。只有 surface 实际缩短且回到 `Normal` 才把 Creator 新输入、Checkpoint、Manifest 和 spill 同事务发布。故障注入覆盖失败零落盘、相同 source facts 的稳定 checkpoint hash、重启恢复、完整 Transcript 不变、一次 overflow 恢复和第二次 overflow 停止。Planning 固定使用 16,384-token host safety ceiling，不把它表述为模型窗口；Provider-specific tokenizer、真实 overflow live 和超长 single-turn 自动处理仍待资格验证，但不阻塞已经完成的 CM-4 machine slice。

CM-4 Planning machine slice 完成边界：

- [x] 定义 `ContextRetrievalQuery/Hit/Selection` 与稳定 source item id、source type、Project revision、content hash、reason、token cost；
- [x] 在 SQLite Transcript 上实现可重建的 Run 内 exact 与 FTS5/BM25 projection，不引入跨项目向量记忆；
- [x] `prepare_turn` 对 summary/current raw tail/retrieval 去重，并把注入条目、query fingerprint、选择原因与 token cost 写入 `ContextManifest`；
- [x] retrieved Tool content/Creator text 在 OpenAI Chat/Responses/Anthropic wire 上保持 untrusted user context，不能覆盖 system/policy/Project facts；
- [x] 冻结 machine corpus 完成 100 inference steps、10 次 compaction、3 次进程重启和一次模拟跨日恢复；
- [x] machine recall 覆盖旧约束、Creator 决定、artifact 与未解决 Tool Result，删除 projection 后重开可得到相同 exact hit；当前无需向量检索；
- [ ] 真实 Music Project/Semantic Tool 落地后，冻结并测量 compaction/retrieval 后约束保持率与工具正确率；该项不由 Planning machine corpus 冒充。

Grant/Budget machine slice 完成边界：

- [x] 不可变 Approval Grant 精确绑定 Creator action、Run、Project revision、Plan/AgentStep、Tool fingerprint、排序后的 target set、副作用类别、累计 effects/cost 和有效期；
- [x] configured Run Budget 不能提高 host-owned system ceiling；Inference 与 Tool 累计 tokens/cost/active time/render/effect/asset/concurrency，等待 Creator、退出和跨日暂停不消耗 active time；
- [x] Grant、Run Budget、Tool Resource Limit 以三类稳定错误独立拒绝，拒绝前后 ledger revision 与 usage 不变；
- [x] Execution Reservation 以稳定 identity 保守预留；相同请求与旧 revision 的 ambiguous-commit 重放不会重复扣减，settlement 不得超过预留；cancel 仅用于已确认零影响的执行，Unknown Outcome 保留预留并先对账；
- [x] `ExecutionControlManager` 与 SQLite Store 以 CAS 发布，覆盖 stale revision、commit 故障零发布、重启恢复、损坏/授权范围篡改失败关闭；
- [ ] 尚未接固定 Planning composition root、Policy、durable ToolExecution 或 Music Project transaction。下一代码依赖是 M3-B 的独立 Music Project revision，随后在 M3-C 将 reservation 与 ToolExecution 原子关联。

OpenAI live evidence（2026-08-26）：低成本 live gate 位于 `scripts/test-openai-continuity-live.sh`，固定使用 `gpt-5-mini` 和 Low Thinking。前两次请求分别揭示 Tool name 不可移植和 `response.failed` 丢失 Provider detail，修复后 Core 会强制 model-visible Tool name 为 `^[a-zA-Z0-9_-]{1,64}$`，并安全保留 Provider error code/message。完成 organization verification 后的最终实测 PASS：`gpt-5-mini` 在 17.30 秒内完成 2 个真实 Turn，使用 777 input tokens 和 385 output tokens；第二 Turn 收到 Continuity reference，终态 Planning commit 前 Vault payload 已 purge。这证明 OpenAI Responses CM-2 Continuity Planning 纵切，不代表 Anthropic、长 Run、compaction 或通用 Tool Runtime 已通过。

Live evidence（2026-08-25）：`bash scripts/test-deepseek-live.sh` 使用默认 `deepseek-v4-flash` 完成真实流式 Tool Call。该模型开启 thinking 时不接受 `tool_choice=required`，Adapter 因而只对这一组合使用 `auto`；Core 继续校验固定 catalog、tool identity、fingerprint 与参数。这个 smoke 证明 DeepSeek CM-1 wire path，不替代其他 Provider/exact model 的 live qualification。

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

- [ ] `project_describe`；
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

Q0 中的 `InstrumentAssignment` resolver 是实验编译器资产，只验证 profile 解析、Bank/Program 与 manifest 合同；它没有注册为 production `instrument.assign`，因此本清单保持未完成。

完成定义：工具合同覆盖成功、schema 拒绝、revision conflict、budget、cancel、crash recovery 和结果清洗。

### 5.5 M3-E：有界 LLM Tool Loop

- [ ] LLM request 带 Tool Catalog Snapshot；
- [ ] 支持 streaming visible text 与完整 tool-call assembly；
- [ ] canonical Inference Item append 与 ToolRequest/ToolResult 对应关系；
- [x] continuity payload 只交回兼容 Adapter；Project Manifest 仅保存非秘密 reference，Client 不接收 payload 或 Vault path；
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
- [x] Q0 前置证据：Type-1 MIDI、语义轨道名、CC0/CC32/Program Change 与 assignment manifest 可确定生成并离线解析；
- [ ] production MIDI export 与 Selection/Project Snapshot/receipt 绑定；
- [ ] Tempo/拍号/markers；
- [ ] Pack/Plugin/engine lock；
- [ ] credits/license/provenance；
- [ ] Cubase、Studio One Pro、FL Studio 的冻结版本按同一 Portable Handoff recipe 导入和继续编辑；
- [ ] 对已验证支持的 DAW 增加 DAWproject Structured Handoff；FL Studio 继续使用 MIDI+stems，除非其官方支持状态改变并重新验证；
- [ ] 以 freeze，或同一 Auto Studio Sampler VST3 + pack/preset lock，验证 Sound-identical Handoff；
- [ ] 不承诺未验证的原生工程文件。

完成定义：设计伙伴能把 Selection 带到至少三类冻结目标 DAW，并在不重建全部结构的前提下继续制作。Portable、Structured、Sound-identical 三个能力等级分别声明，不用 MIDI Program Change 冒充同声恢复。

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

- `PASS（v2/v3/portable/qualification apparatus）/ LIVE-PENDING（human/cross-DAW）`：v2 Mode B 11/12 达到装置门槛；v3 全部 6 个 L4 valid + compiled；Portable Handoff 的 assignment/Bank/Program/离线解析与 DAW evidence verifier 通过。三目标 DAW 当前为 `not_run`；Bitwig Pilot 已手工导入和保存，但正式 checklist、Mode C、盲听与 continued-editing 尚未完成；
- 通过条件：实验装置有效，L4 达到冻结 Keep/Actual continued editing 门槛；负面主模型结果由第二个不同强模型复核。

### Gate A：产品基线

- `PASS`：一句话定位、Local-first、单 Agent、Selection/Approval 分离；
- `BLOCKED`：首发 ICP、OS、DAW/version 和设计伙伴尚未冻结。

### Gate B：LLM Agent Harness

- `PASS（contract）`：LLM Connection、Model/Thinking、typed Planning；
- `PARTIAL（CM-0—CM-4 + Grant/Budget machine slices PASS）`：Inference Transcript、Context Manifest、canonical request、SSE assembler、固定 Planning 多轮 Tool loop、重启恢复、CM-2 Continuity、CM-3 automatic compaction/spill/overflow recovery、CM-4 exact/FTS5-BM25 retrieval/100-step corpus，以及不可变 Approval Grant、Run Budget ledger、Execution Reservation 与 SQLite CAS 已实现；Grant/Budget 尚未接 composition root，通用 ToolExecution 和真实音乐 Tool 的 long-run quality 尚未实现；
- 通过条件：continuity/Transcript 分离，Grant/预算范围生效，重启/compaction/retrieval/终态 purge、真实音乐 Tool 正确率和 exact model live qualification 通过。

### Gate C：本地可编辑音乐

- `NOT IMPLEMENTED`：Music Project、MIDI Tool、Render Plan、Sampler；
- 通过条件：production 无 Music Provider/Fake，真实 LLM 创建并修改可恢复工程。

### Gate D：内容质量

- `BLOCKED`：Factory Pack 法律批准、Sampler、fixed corpus、机器阈值和人工盲听；
- 通过条件：Factory Path 达到冻结技术与听感阈值。

### Gate E：专业交接

- `PARTIAL（Fixture + Q0 machine）`：已有 Audio-only Handoff contract；Q0 可移植 symbolic precursor 已输出 Type-1 MIDI、Bank/Program 与 assignment manifest；
- `BLOCKED`：production Project Snapshot Candidate、stems/receipt、Cubase/Studio One/FL Studio 继续编辑矩阵与同声路径。

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
