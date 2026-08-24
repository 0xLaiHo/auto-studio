# Q0 音乐内容可行性 Spike

> 基线日期：2026-08-24  
> 状态：`NOT IMPLEMENTED`；只有实验设计，没有结果  
> 决策对象：是否继续投入 M3 Agent Harness、Music Project 与本地音频执行链  
> 性质：一次性、可复现实验；不属于 production runtime，也不构成产品能力声明

## 1. 为什么先做 Q0

[ADR-0011](../adr/0011-llm-authored-local-music.md) 把产品方向改为“LLM 通过本地语义工具创作可编辑音乐”。这个方向成立需要一个尚未验证的核心前提：通用 LLM 能产生值得创作者保留并继续编辑的结构、和声、旋律、节奏与配器决定。

当前代码只证明一次 typed Planning Turn，没有真实 Tool loop、Music Project、MIDI、Sampler 或 Audio Engine。若先完成完整本地引擎再验证音乐决策质量，会把最昂贵的技术投入放在最大的不确定性之前。因此 Q0 只建设足以回答内容问题的最薄实验，不提前实现生产架构。

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
| B 分阶段 | 骨架 → 轨道/region → 校验修订 | 全部 12 个 Brief | 默认实验路径 |
| C Creator 反馈 | 从该 Brief 的 B 结果出发，最多 2 轮文字反馈 | 全部 6 个 L4 Brief | 判断反馈循环的实际价值 |

比较规则：

- 同一比较对使用同一个精确 Provider、Model、协议、Thinking、system prompt version 与随机性设置；
- A/B/C 尽量匹配总输出 token budget，而不是给某一模式无限上下文；
- 保存每轮 request/normalized response、usage、延迟、错误和费用；
- A/B/C 差异只能作为产品与接口设计证据。样本量小、轮次结构不同，不能写成“分阶段导致质量提升”的因果结论。

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

- [ ] 建立 Git baseline commit；当前仓库无 commit、文件未被追踪且 user identity 未配置时，不执行 destructive cleanup；
- [ ] 冻结 12 个 Brief、评分表与 `protocol.lock.json`；
- [ ] 冻结首个精确模型与负面复核模型；
- [ ] 冻结 DAW/version/template 与 instrument mapping；
- [ ] 完成内容本地使用许可与 hash 记录；
- [ ] 创建独立 experiment workspace、schema/compiler tests；
- [ ] 完成不计入结果的 pilot；
- [ ] 运行 A/B/C、盲评和 continued-editing session；
- [ ] 输出 `GO/REVISE/NO-GO/INVALID` 报告；
- [ ] 只有 `GO` 才把 M3 从目标设计转为 production 实施。

## 关联

- [产品设计](../product/ai-creative-agent-product-design.md)
- [技术设计](../design/auto-studio-technical-design.md)
- [Roadmap](../roadmap.md)
- [ADR-0011：LLM 本地创作](../adr/0011-llm-authored-local-music.md)
- [ADR-0012：Durable Agent Harness State](../adr/0012-durable-agent-harness-state.md)
