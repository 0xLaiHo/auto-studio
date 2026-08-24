# Q0 音乐内容可行性 Spike

> 基线日期：2026-08-24  
> 状态：`LIVE-PENDING`；v2 真实 pilot 与正式 A/B 机器 Gate 已完成；v3 L4 重基线装置已冻结、真实运行 `IN PROGRESS`；真人/DAW Gate 尚未完成
> 决策对象：是否继续投入 M3 Agent Harness、Music Project 与本地音频执行链  
> 性质：一次性、可复现实验；不属于 production runtime，也不构成产品能力声明

## 1. 为什么先做 Q0

[ADR-0011](../adr/0011-llm-authored-local-music.md) 把产品方向改为“LLM 通过本地语义工具创作可编辑音乐”。这个方向成立需要一个尚未验证的核心前提：通用 LLM 能产生值得创作者保留并继续编辑的结构、和声、旋律、节奏与配器决定。

Q0 开工前，production 代码只证明一次 typed Planning Turn，没有真实 Tool loop、Music Project、MIDI、Sampler 或 Audio Engine。现已在 `experiments/music-quality/` 建成独立试验纵切，但它不改变 production 的 planning-only 状态。若先完成完整本地引擎再验证音乐决策质量，仍会把最昂贵的技术投入放在最大的不确定性之前。

## 2. 要回答的决策问题

1. 强 LLM 的结构化输出，在固定音色与 DAW 环境中是否值得保留？
2. Creator 是否真的继续编辑，而不只是口头认为“还可以”？
3. 分阶段生成相对一次成型是否提供可见价值？
4. 基于 Creator 反馈的后续轮次能否改善 L4 配器结果？
5. 哪些音乐语义适合成为未来 Semantic Tool，哪些只是实验字段？

Q0 不证明因果关系、市场需求、Factory Pack 音质、实时音频、VST3 兼容或正式发布质量。

## 3. 明确收窄的范围

### 3.1 只测 L1—L4

| 等级 | 新增要求 | 主要判断 |
|---|---|---|
| L1 骨架 | tempo、调性、8—16 小节和弦与主旋律 | 和声是否成立；旋律是否有辨识度 |
| L2 节奏组 | bass 与 drum/pulse | groove、重音与和声配合 |
| L3 结构 | 60—90 秒完整段落、对比与过渡 | 结构比例、推进与转折 |
| L4 配器 | 6—10 轨、声部分工、音域、密度、力度/CC | 角色清楚、互不打架、随结构发展 |

L4 是继续投入专业 Music Project/Tool Runtime 的最低证据层。Q0 不测 L5 Mix，不实现 EQ、compressor、reverb、limiter、LUFS 或自研 DSP。

### 3.2 不建设生产能力

- 不实现 Factory Pack、Sampler、Audio Engine 或 VST3 Host；
- 不把实验 JSON 直接命名为 `ToolRequest`、`MusicProject` 或 production schema；
- 不复用旧 GenerationAdapter、Fake WAV 或 Music Provider；
- 不承诺实时播放、确定性正式渲染或发布级 DAW Handoff；
- 不增加主 workspace crate 或 production dependency。

## 4. 固定 Corpus

共 12 个 Brief，运行前冻结并保存 hash：

| 等级 | 数量 | 覆盖 |
|---|---:|---|
| L1 | 2 | 歌曲旋律骨架、视频氛围动机 |
| L2 | 2 | 电子 groove、管弦 ostinato/低音脉冲 |
| L3 | 2 | verse/chorus 歌曲、60—90 秒视频 cue |
| L4 | 6 | 歌曲 2、视频配乐 2、管弦 1、电子 1 |

每个 Brief 固定用途、时长、段落、风格词、必须/禁止项、乐器角色、交付条件和评分重点。运行后不得补写有利条件；发现 Brief 歧义时记录为实验缺陷，修订后形成新 corpus version。

## 5. 实验中间表示

实验使用 `ExperimentalMusicSpec`，只表达 MIDI/DAW 评价需要的事实：

```text
ExperimentalMusicSpec
├── tempo_map       [{ bar, bpm, time_signature }]
├── key_map         [{ bar, tonic, mode }]
├── sections        [{ id, label, start_bar, length_bars, intent }]
└── tracks          [{ id, name, role, register, instrument_hint,
                       regions: [{ section_id,
                                   notes: [{ beat, duration, pitch, velocity }],
                                   cc: [{ beat, controller, value }] }] }]
```

约束：

- region 时间相对 Section，以 beat 表达，不要求 LLM 计算 MIDI tick；
- `role`、`register` 和 `instrument_hint` 用于诊断声部分工，不绑定未来 Plugin/Profile 设计；
- schema 限制轨道、Section、note、CC、音域、时长和 payload 大小；
- 编译器只产生 SMF MIDI、tempo/time-signature 和 marker 信息；
- Q0 结束后先总结证据，再单独设计 production Tool Interface。实验 schema 不自动冻结为未来 API。

## 6. 三种运行模式

| 模式 | 做法 | 覆盖 | 用途 |
|---|---|---|---|
| A 一次成型 | 一次请求输出完整 spec | 4 个代表 Brief，跨等级与风格 | 建立粗粒度基线 |
| B 分阶段 | 骨架 → 轨道/region → 校验修订；v3 L4 在严格限定的全局资源预算错误下最多增加一次可审计修订 | 全部 12 个 v2 Brief；全部 6 个 v3 L4 重基线 | 默认实验路径 |
| C Creator 反馈 | 从该 Brief 的 B 结果出发，最多 2 轮文字反馈 | 全部 6 个 L4 Brief | 判断反馈循环的实际价值 |

比较规则：

- 同一比较对使用同一个精确 Provider、Model、协议、Thinking、system prompt version 与随机性设置；
- A/B/C 尽量匹配总输出 token budget，而不是给某一模式无限上下文；
- 保存每轮 request/normalized response、usage、延迟、错误和费用；
- A/B/C 差异只能作为产品与接口设计证据。样本量小、轮次结构不同，不能写成“分阶段导致质量提升”的因果结论。

### 6.1 v3 L4 重基线修订

v2 的 `l4-orchestral-argument` 在第三轮产生 269 个 CC，超过冻结的 256 上限，因此没有合法 B spec，无法进入要求 6 个配对样本的 Mode C。v2 已达到预设 11/12 装置门槛，其证据与结论不撤销；但内容比较必须补充一组新的、内部一致的 L4 基线。

v3 在看到任何 Creator 反馈或盲评分数前冻结以下规则：

1. 在独立 evidence root 重新运行全部 6 个 L4，而不是只重跑已知失败项；
2. 只有第三轮能严格反序列化，且全部 violation 都是全局 note/CC 预算超限时，才允许第 4 个 Provider 回合；
3. 第 4 回合必须返回完整 spec；系统不得裁剪事件、扩大阈值或人工修 JSON；
4. 每个 run 持久化 protocol id/SHA-256、允许/使用修订数以及全部 turn；
5. v3 进入 Mode C 的前提提高为精确 6/6 valid + compiled；未达到时先报告 `REVISE/INVALID`，不得改分母；
6. v3 新增调用的冻结上限为 24 次、USD 6.549511112，使 v2 + v3 的 peak 累计预算仍不超过 USD 10。

## 7. 模型顺序

1. 先用当前可获得的高能力精确模型完成主实验，目的是估计能力上限；
2. 如果主模型未达到继续投入阈值，必须用第二个架构/供应商不同的高能力精确模型复核 L4；
3. 只有两个模型都在同一失败类型上得到一致负面证据，才允许形成产品方向 `NO-GO`；
4. 成立后再测试 DeepSeek/Kimi 等成本路径，成本模型不能替代上限验证。

报告必须写精确 model id、Provider、API/protocol version、Thinking、日期、prompt/schema hash 和价格来源；“GPT 顶配”“Claude 最新”等营销名称无效。

## 8. MIDI 与 DAW 评价链

```text
Frozen Brief + Prompt + Schema
             │
             ▼
        BYOK LLM calls
             │
             ▼
  ExperimentalMusicSpec JSON
             │ validate / compile
             ▼
 MIDI + tempo + markers + diagnostics
             │ fixed import recipe
             ▼
 Frozen DAW/version/template + fixed instrument mapping
             │
             ├── blind listening
             └── actual continued-editing session
```

必须冻结一个 DAW/version、project template、buffer/sample rate、instrument mapping 与导入步骤。所有候选使用同一组用户自有或本地合法使用的音色，不根据模式偷偷更换更好的 preset。

Q0 不分发音色，但仍记录每个音色/样本的来源、精确版本、license/EULA、文件 hash 与本地使用权结论。“不进入安装包”只免除产品再分发 Gate，不免除实验使用许可。

## 9. 评估协议

### 9.1 先冻结协议

先用不计入结果的 1 个 pilot 校验导入、随机化、问卷和计时流程；随后冻结 `protocol.lock.json`，包含 corpus、模型、prompt、schema、DAW/template、mapping、随机种子、阈值和 evaluator。正式运行后不改阈值。

### 9.2 匿名与随机化

候选使用无 mode/model 含义的随机 ID。评价前不显示 A/B/C、模型、费用或生成顺序。若只有产品负责人一名评价者，结论明确标为 `founder signal`，不能冒充设计伙伴或市场验证。

### 9.3 核心指标

| 指标 | 记录方式 |
|---|---|
| Keep | 在不知道来源时是否愿意保存为创作起点 |
| Actual continued editing | 是否打开工程并完成一次有意图的编辑、保存新 revision 或导出修改版；只点播放不算 |
| Time to useful | 从提交 Brief 到第一个被 Keep 的候选所需时间 |
| Edit distance | 为达到可继续制作而删除/重写的 Section、Track、region、note 数与操作数 |
| Structural errors | 无效拍号/范围、冲突 Section、越界 note、断裂 note-off、严重音域/声部冲突 |
| Content score | Brief 匹配、结构、和声、旋律、groove、配器各 1—5 |
| Cost/latency | request 数、tokens、未知 usage、费用、首 token 和总耗时 |

“我喜欢”不是 continued editing。每次继续编辑必须保存操作摘要和最终 MIDI hash。

## 10. 决策门槛

以下是 Q0 的投资门槛，不是 MVP Release Gate：

1. **装置有效**：Mode B 至少 11/12 输出无需人工修 JSON 即通过 schema 并编译为可导入 MIDI；否则先修 prompt/schema/compiler，不判断音乐能力。
2. **L4 内容信号**：主模型 Mode C 至少 4/6 被 Keep，其中至少 3/6 发生 Actual continued editing；严重结构错误不得出现在过半 L4 结果中。
3. **反馈价值**：相对各自 Mode B，Mode C 至少 4/6 保持或提高 Keep/内容评分，且没有增加严重结构错误；否则 M3 不应把长反馈 loop 当成默认价值。
4. **投入合理性**：记录达到可用结果的中位 Time to useful、编辑操作与推理成本，并在结果评审前冻结团队可接受上限。不得在看到数据后移动上限。
5. **负面复核**：主模型未过第 2 项时，必须按 §7 使用第二个模型复核，才能决定 `NO-GO`。

通过 Q0 只授权进入 M3 Harness Foundation 和 Music Project/Tool Runtime 实现；Factory Pack、VST3 和发布仍由各自 Gate 决定。

## 11. 结果状态

| 状态 | 含义 | 下一步 |
|---|---|---|
| `GO` | 装置有效且达到 L4 投资门槛 | 进入 M3；用失败分布设计深 Tool |
| `REVISE` | 有 Keep/编辑信号，但 schema、粒度或反馈策略不稳定 | 只迭代实验接口，再跑受影响子集 |
| `NO-GO` | 两个不同强模型均未达到 L4 门槛 | 停止完整本地 AI-native DAW 投入，重新评估产品方向 |
| `INVALID` | DAW、音色、协议、模型或数据记录不一致 | 修复装置并重跑，不能形成产品结论 |

## 12. 实现与证据保存

实验代码放在 `experiments/music-quality/`：

- 独立 workspace、独立 lockfile，不加入 production workspace；
- 最少包含 schema/parser/compiler 单元测试和一个 MIDI import smoke；
- BYOK Credential 只来自环境变量或 Project 外安全配置，不写入 artifact；
- 保存 Brief、prompt、schema、source、精确模型配置、normalized output、MIDI、hash、usage、费用、评价、编辑动作与结论；
- 结果目录不可含 Credential、Provider private reasoning 或不允许再分发的音色文件；
- 实验代码与证据必须提交到仓库主历史或明确的长期 research artifact，不得只留在未合并 branch；
- 任何删除旧 Generation 代码的动作必须发生在仓库建立可回退 baseline commit 之后。

## 13. 开工清单

- [x] 建立 Git baseline commit `b9db99c`，配置仓库 identity，并保留远端 `origin/main` 可回退点；
- [x] 冻结 12 个 Brief、评分表与 `protocol.lock.json`；
- [x] 冻结主模型 `deepseek-v4-pro`、Thinking `high`、协议和价格快照；
- [ ] 负面结果的第二个不同强模型仍需独立 Credential/精确 model 后冻结；
- [x] 冻结 Bitwig Studio 6.0.11、48 kHz 导入 recipe 与 instrument mapping；
- [ ] Bitwig 实际 MIDI 导入/音色装载/保存仍为 `LIVE-PENDING`；当前 accessibility provider 不暴露其自绘窗口，不以坐标猜测冒充通过；
- [x] 完成本地音色使用许可、版本和 hash 记录；GeneralUser GS 只批准本地评价，不批准产品再分发；
- [x] 创建独立 experiment workspace、严格 schema/parser、Type-1 SMF compiler、逐轮恢复、证据哈希和测试；
- [x] 完成不计入结果的真实 Mode A/Mode B DeepSeek pilot；
- [x] 保存 protocol v1 装置失败：10 个 Mode B 已完成样本中 3 个无效，主要因 32,768 output-token 截断；证据冻结于 commit `78eb675` 和 `evidence/formal-v1-invalid/`，不得混入正式结论；
- [x] 冻结 protocol v2：完整 spec 上限 65,536 output tokens，并增加全局 768 notes / 256 CC 资源预算；
- [x] protocol v2 L4 pilot 通过：679 notes、33 CC、113,342 tokens、841,105 ms，严格校验与 MIDI 编译成功；
- [x] 实现按锁定清单校验 Candidate、Provider identity、artifact hash、usage/cost 的 formal verifier；
- [x] 实现 evaluator-safe 匿名包、独立 private mapping 与评价表；
- [x] 运行正式 Mode A 4/4 与 Mode B 12/12；Mode B 11/12 valid + compiled，达到装置门槛；
- [x] 冻结 `protocol-v3-l4.lock.json`、全量 L4 重基线、逐 Run 协议绑定和一次资源预算修订规则；
- [x] 实现从任意已落盘 Mode B turn 恢复、v3 Formal Verifier 和 fixture 端到端验证；
- [ ] 在 `evidence/formal-v3-l4/` 运行全部 6 个 L4 Mode B，并达到 6/6 valid + compiled；
- [ ] 运行真实 Creator feedback Mode C、盲评和 continued-editing session；
- [ ] 输出 `GO/REVISE/NO-GO/INVALID` 报告；
- [ ] 只有 `GO` 才把 M3 从目标设计转为 production 实施。

## 14. 当前可审计证据

- `protocol.lock.json`：冻结 corpus、schema、prompt、模型、Thinking、超时、价格、环境、随机种子与投资阈值；
- `protocol-v3-l4.lock.json`：保持 v2 不变，为全部 6 个 L4 冻结独立 B/C 基线、资源预算修订、累计预算与逐 Run binding；
- Mode A pilot：1 个真实调用，11,011 tokens，142,780 ms，严格校验并编译成功；
- Mode B pilot：3 个真实调用，51,275 tokens，521,949 ms，逐轮 artifact 可恢复，最终严格校验并编译成功；
- protocol v1 正式装置在 L3/L4 暴露输出截断，按 Gate 判定为 apparatus invalid；原始结果保留，不做人为修 JSON；
- protocol v2 L4 pilot：3 个真实调用，113,342 tokens，841,105 ms，679 notes / 33 CC，最终严格校验并编译成功；正式 v2 A/B 使用全新目录；
- protocol v3 L4：apparatus、lock、runner、resume、binding、verifier 与本地 fixture Gate 已完成；真实 Provider evidence 尚未产生；
- Provider evidence 不持久化 API Key 或 private reasoning；请求固定 `Accept-Encoding: identity`，单次超时 600 秒；
- Bitwig 进程和 48 kHz PipeWire 初始化已实测，MIDI GUI import 尚未取得可验证证据；
- pilot 明确排除于正式评分，不能用于填充 11/12、Keep 或 continued-editing 门槛。

## 关联

- [产品设计](../product/ai-creative-agent-product-design.md)
- [技术设计](../design/auto-studio-technical-design.md)
- [Roadmap](../roadmap.md)
- [ADR-0011：LLM 本地创作](../adr/0011-llm-authored-local-music.md)
- [ADR-0012：Durable Agent Harness State](../adr/0012-durable-agent-harness-state.md)
