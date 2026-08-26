# Q0 音乐内容可行性结果

> 日期：2026-08-24  
> 当前结论：Q0-Content 为 `LIVE-PENDING`，不是 `CONTENT-GO`、`REVISE` 或 `NO-GO`
> 已完成：工程装置、真实 DeepSeek pilot、正式 Mode A/B、机器 Gate、匿名评审包  
> 2026-08-27 更新：v2 证据保持不可变；v3 L4 全量重基线 6/6 valid + compiled；另完成 Portable Handoff v1、六样本本地内容评审包与 DAW qualification harness 机器切片
> 未完成：真实 Creator feedback/Mode C、盲听 Keep、actual continued editing；跨 DAW 真人 qualification 已独立后置 M5/Gate E

## 0. v3 L4 协议补充

v2 的 11/12 装置门槛成立，但唯一无效项 `l4-orchestral-argument` 没有合法 `spec.json`。Mode C 又要求全部 6 个 L4 以各自 Mode B 为基线，所以不能直接完成冻结的 6 组 B/C 比较。

该问题不通过手工删除 13 个 CC、提高 256 阈值或只重跑失败样本解决。已新增并冻结 `q0-protocol-v3-l4-rebaseline`：

- v2 lock、run 和 summary 不修改，v2 machine Gate 结论保留；
- 在独立 `evidence/formal-v3-l4/` 中重新运行全部 6 个 L4 Mode B，避免按结果选择重试样本；
- 第三轮必须先能解析为严格 spec，且唯一错误是全局 768 notes/256 CC 预算超限，才允许一个第 4 轮资源修订；
- 修订由 LLM 返回完整 spec，不做确定性裁剪、静默删除、人工 JSON 修改或阈值放宽；
- 每个 run 绑定 protocol SHA-256，保存修订前后完整 turn；中断恢复复用已落盘 turn，不盲目产生重复计费请求；
- v3 Formal Verifier 要求精确 6 个 L4、6/6 valid + compiled，并拒绝 protocol drift、无 trigger 的修订回合和 artifact hash 不一致。

v3 已完成真实 Provider 运行并由 Formal Verifier 通过：

| Brief | Turns | Tracks | Notes | CC | Tokens | Provider latency ms |
|---|---:|---:|---:|---:|---:|---:|
| l4-song-neon | 3 | 7 | 702 | 98 | 120,546 | 956,418 |
| l4-song-intimate | 3 | 6 | 766 | 50 | 117,640 | 819,816 |
| l4-video-chase | 3 | 7 | 461 | 61 | 88,656 | 618,952 |
| l4-video-emotional | 3 | 6 | 113 | 82 | 57,353 | 501,851 |
| l4-orchestral-argument | 3 | 7 | 408 | 166 | 107,325 | 850,175 |
| l4-electronic-microcity | 3 | 8 | 533 | 60 | 110,017 | 742,549 |

聚合结果：精确 6/6 Candidate、6/6 completed + compiled、601,537 tokens（input 164,970；output 436,567）、Provider latency 4,489,761 ms、off-peak USD 0.966586412、peak USD 1.933172824。六项均在第三轮直接通过，因此冻结的资源预算修订能力没有被正式样本使用；这不能被写成“修订策略经过真实触发验证”，其真实证据仍只有 fixture contract。

v3 protocol SHA-256 为 `080c7daf92b3d3272da5b3a27c08315d20f0aa760761b0edcfabf5e591de4a6a`，summary SHA-256 为 `7606aecebfb0d9fba49f48f0402588737950741bea8115294fc7ef7463a0f804`；apparatus 由提交 `369a20b` 与 `ce81aed` 冻结。Credential/private-reasoning sentinel 通过。v2 + v3 的 peak 累计成本为 USD 5.383661712，仍低于冻结的 USD 10 累计上限。

## 1. 结论先行

Q0 的机器装置通过：冻结的 4 个 Mode A 与 12 个 Mode B 均有精确 run 记录，Mode B 中 11/12 无需人工修 JSON 即通过严格 schema/不变量并编译成可解析的 Type-1 SMF MIDI，恰好达到预设门槛。

这还不能回答“音乐是否值得保留”。Creator 尚未提交六个 L4 的具体反馈，也没有 Mode C、匿名 Keep 或真实继续编辑数据。因此不能把“JSON/MIDI 可编译”、固定 SoundFont 能发声或一次手工 DAW smoke 写成 `CONTENT-GO`。M3-B production 仍然等待 Q0-Content 人工 Gate。跨 DAW 真人矩阵独立后置 M5/Gate E，不再阻塞这次内容投资判断。

### 1.1 Portable Handoff v1 机器证据

为避免把 Agent 乐器分配误解成 Bitwig UI 自动化，又不改写 v2/v3 冻结装置，新增了独立 `experiments/portable-handoff/` 交付前置切片；它复用 Q0 spec/compiler 输出，但在单独 crate 中追加可移植乐器意图：

- 冻结的乐器目录把语义轨道解析为稳定 profile、MIDI channel、Bank Select 和 Program Change；显式未知 profile 直接拒绝，不静默回退；
- Type-1 MIDI 的每条音乐轨在 tick 0 写入 CC0、CC32 和 Program Change，并保留轨道名、Tempo、拍号、marker、note 和 CC；
- `instrument-assignments.json` 记录 profile、匹配来源、GM 名称、GeneralUser GS 实际 preset、内容库 hash 和本地许可结论；
- Pilot 解析为 Piano `0/0/0`、Lead `0/0/80`、Bass `0/0/33`；`composition.mid` SHA-256 为 `ff67f617fed9ddbe5c531cefc7a7e868ddf663c3b1e051ebecd98d3d180bc3a9`；
- 使用相同 GeneralUser GS 的 FluidSynth 离线渲染成功，得到 48 kHz stereo PCM、20.632 秒音频。渲染文件仅用于本机验证，没有作为可分发内容提交。
- v2/v3 锁定的 schema、`instrument-mapping-v1.json`、`daw-environment-v1.json` 未改写；新 catalog 与 Creator Pilot observation 使用独立 `*-portable-v1` 文件，避免事后修改冻结输入。
- qualification harness 将 handoff manifest/artifact hash 与三类 required target 绑定，并要求精确版本、executable hash、八项检查、PNG/JPEG、保存工程和 edited MIDI 证据；Pilot plan SHA-256 为 `3f67624439e41af95011c7319635c2b969c9e720c3c3ff06433f760c4693184d`，summary SHA-256 为 `623e904c9905a9c2498a9589d9297e53e792d294bcf2745c87f96afc99ece4ab`。
- 当前主机只检测到 Bitwig；Cubase、Studio One Pro、FL Studio 未安装且未冻结精确版本。因此三项目标均为 `not_run`，`all_required_targets_passed=false`，没有以 Fixture 或 Bitwig 结果冒充兼容性。

这证明“Agent 决定乐器 → 标准 MIDI 表达 → 可审计清单”是可运行的，不证明 Cubase、Studio One、FL Studio 会自动加载相同原生音色，也不证明 production `instrument.assign`、stems、DAWproject 或 Auto Studio Sampler VST3 已实现。

### 1.2 六样本本地内容评审包

为让 Creator 在不安装多个 DAW、不再次调用付费模型的前提下完成反馈，`experiments/portable-handoff` 新增可复现的 `prepare-content-review` / `verify-content-review`：

- 只接受 protocol v3 冻结的精确 6/6 L4 Mode B corpus，并逐个核对 `protocol-binding.json`；
- 为每个 Brief 生成带 Bank/Program 的 Portable MIDI，并用同一份本地 GeneralUser GS 和 FluidSynth 2.6.0 渲染 48 kHz、stereo、16-bit WAV；
- 生成 6 个样本、44 个不可变 artifact，根 manifest 记录 protocol、formal summary、本地 SoundFont 与所有素材的 SHA-256；
- SoundFont 只读取和哈希，不复制到评审包；本地约 100 MB WAV 目录由 Git 忽略，不能当作可分发 Factory Pack；
- `feedback.json` 是唯一可变的 Creator 输入；verifier 会检查其六个身份、每项最多两条反馈和 ready 状态，同时拒绝被修改的 Brief、Spec、MIDI、WAV、assignment 或 binding。

本机生成结果是 `6 samples / 44 immutable artifacts / feedback_ready=false`。这把“如何听并写反馈”变成了可复现流程，但不会代替 Creator 的真实判断。

## 2. 协议与可追溯性

正式结果使用 `q0-protocol-v2`：

- Provider：DeepSeek；model `deepseek-v4-pro`；Thinking `high`；OpenAI Chat Completions wire；
- 非 streaming、JSON object、`Accept-Encoding: identity`、单请求 600 秒；
- 完整 spec 最大 65,536 output tokens；全局最多 768 notes、256 CC；
- protocol SHA-256：`2d9512afad075cef5c433fec7babfd5c24e9a9787a2b3ebd8aa230266c147d1c`；
- baseline commit：`b9db99c`；v1 装置证据 commit：`78eb675`；
- 价格冻结自 [DeepSeek 官方定价](https://api-docs.deepseek.com/quick_start/pricing)，历史费用不使用未来价格重算。

v1 使用 32,768 output-token 上限。已完成的 10 个 Mode B 中有 3 个无效，主要失败是 L3/L4 JSON 截断，因此按预设规则判定为 apparatus invalid；原始结果保存在 `evidence/formal-v1-invalid/`，未覆盖、未人工修 JSON，也未混入 v2 正式统计。

v2 先用一个不计分的 L4 pilot 验证：679 notes、33 CC、113,342 tokens、841,105 ms，严格校验与 MIDI 编译成功，然后才启动正式 corpus。

## 3. 正式 A/B 结果

| Mode | Brief | 状态 | Tracks | Notes | CC | Tokens | Latency ms |
|---|---|---|---:|---:|---:|---:|---:|
| A | l1-song-hook | completed | 3 | 64 | 26 | 13,459 | 152,128 |
| A | l2-electronic-groove | completed | 4 | 153 | 22 | 19,735 | 205,645 |
| A | l3-video-cue | completed | 5 | 270 | 42 | 25,668 | 276,976 |
| A | l4-song-neon | completed | 7 | 698 | 16 | 46,581 | 479,604 |
| B | l1-song-hook | completed | 3 | 61 | 6 | 33,287 | 338,749 |
| B | l1-video-motif | completed | 3 | 34 | 11 | 36,426 | 396,570 |
| B | l2-electronic-groove | completed | 4 | 117 | 12 | 50,312 | 461,932 |
| B | l2-orchestral-ostinato | completed | 4 | 101 | 30 | 48,398 | 449,339 |
| B | l3-verse-chorus | completed | 6 | 690 | 9 | 118,204 | 678,225 |
| B | l3-video-cue | completed | 6 | 387 | 61 | 86,293 | 732,659 |
| B | l4-song-neon | completed | 7 | 754 | 107 | 126,222 | 903,328 |
| B | l4-song-intimate | completed | 6 | 745 | 67 | 112,548 | 802,745 |
| B | l4-video-chase | completed | 7 | 465 | 44 | 86,445 | 650,518 |
| B | l4-video-emotional | completed | 6 | 116 | 67 | 58,827 | 565,827 |
| B | l4-orchestral-argument | invalid | — | — | — | 96,610 | 718,058 |
| B | l4-electronic-microcity | completed | 9 | 604 | 52 | 115,298 | 763,619 |

唯一无效项包含 269 个 CC，超过冻结的 256 上限。模型第三轮仍未降到预算内；系统没有静默删除 13 个 CC 或扩大阈值。

Formal Verifier 的完整聚合：

- 精确 Candidate：16/16；completed/compiled：15/16；Mode B：11/12；device gate：`PASS`；
- 总 tokens：1,074,313；input 296,176（cache hit 17,152 / miss 279,024）；output 778,137；
- 累计 Provider latency：8,575,922 ms；所有 run 中位 565,827 ms；Mode B 中位 678,225 ms；
- 冻结价格成本：off-peak USD 1.725244444；peak USD 3.450488888，低于正式实验 USD 10 上限；
- summary SHA-256：`5193918780be4ae13a5cc51e4edc7ba32f22a4408c1ec3e052bf4bcb88e8c90d`。

## 4. 证据完整性与安全

- verifier 先验证 `protocol.lock.json` 中每个冻结输入 hash，再要求精确 4 A + 12 B；
- 每个 run 的 Provider/model/Thinking、candidate identity、artifact size/SHA-256、strict spec 和 MIDI 均重新验证；
- 每个 Mode B turn 在下一次计费调用前原子落盘，第三轮中断可单独恢复；
- evidence credential sentinel 扫描通过；API Key 和 Provider private reasoning 均未进入 artifact；
- 匿名包包含 15 个可编译 Candidate；evaluator manifest SHA-256 为 `256982c41eef4659405a5e2afcc32238b79ba6f512b90da5b49090a1cf91768c`，mode mapping 单独保存；
- GeneralUser GS 只作为本机评价引用，未复制进仓库，也未批准产品再分发。

## 5. Gate 状态

| Gate | 阈值 | 当前状态 |
|---|---|---|
| 装置有效 | Mode B 至少 11/12 valid + compiled | `PASS`：11/12 |
| 正式成本 | peak 不超过 USD 10 | `PASS`：USD 3.450488888 |
| Portable instrument handoff | Type-1 MIDI 含 Bank/Program，assignment manifest 与固定音色离线解析 | `PASS（machine）`；不等于跨 DAW 同声或 production export |
| 六样本内容评审装置 | 6/6 protocol-bound、固定渲染、immutable hash、可变真实反馈 | `PASS（machine）`；feedback 尚未填写 |
| L4 内容信号 | Mode C 至少 4/6 Keep | `LIVE-PENDING`；没有真人反馈/评分 |
| Actual continued editing | Mode C 至少 3/6 | `LIVE-PENDING` |
| 反馈不退化 | Mode C 相对 B 至少 4/6 保持或提高 | `LIVE-PENDING` |
| 负面复核 | 主模型未过内容门槛时使用第二个不同强模型 | 条件尚未触发；Credential/model 未冻结 |
| 跨 DAW qualification | 精确版本 import/save/edit/export evidence | `DEFERRED（M5/Gate E）`；apparatus `PASS`，真人矩阵 `0 pass / 3 not_run` |

## 6. 完成 Q0 决策还需要什么

严格按 [`experiments/music-quality/HUMAN-GATES.md`](../../experiments/music-quality/HUMAN-GATES.md) 完成：

1. 在本地六样本评审包中逐个听 `preview.wav`，对每个 Brief 在 `feedback.json` 写 1—2 条真实、具体的 Creator feedback；不能让另一个 LLM 代写；
2. 运行 verifier，只有 `feedback_ready=true` 才启动六个 Mode C；
3. 重新生成含 v3 B/C 的匿名包，在不查看 private map 的情况下填写 Keep 与内容评分；
4. 对 Keep 的 L4 候选做真实音乐编辑、保存/重开、导出 edited MIDI，并记录操作数、时间与 hash；内容 Gate 可统一使用同一冻结编辑环境；
5. 最后才解盲并应用冻结阈值。主模型若为负面，再用第二个不同强模型复核。

在这些证据完成之前，诚实的 Q0-Content 状态只能是 `LIVE-PENDING`，不能授权 M3-B。Cubase、Studio One Pro、FL Studio 的正式 checklist/截图/project/edited MIDI 继续留在 M5/Gate E；完成前不能形成专业 DAW 兼容声明，但不改变本节的内容结论。
