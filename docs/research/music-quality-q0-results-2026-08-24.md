# Q0 音乐内容可行性结果

> 日期：2026-08-24  
> 当前结论：`LIVE-PENDING`，不是 `GO`、`REVISE` 或 `NO-GO`  
> 已完成：工程装置、真实 DeepSeek pilot、正式 Mode A/B、机器 Gate、匿名评审包  
> 未完成：Bitwig MIDI import、真实 Creator feedback/Mode C、盲听 Keep、actual continued editing

## 1. 结论先行

Q0 的机器装置通过：冻结的 4 个 Mode A 与 12 个 Mode B 均有精确 run 记录，Mode B 中 11/12 无需人工修 JSON 即通过严格 schema/不变量并编译成可解析的 Type-1 SMF MIDI，恰好达到预设门槛。

这还不能回答“音乐是否值得保留”。当前没有 Bitwig 实际导入、固定音色试听、匿名 Keep、Creator feedback 或继续编辑数据，所以不能把“JSON/MIDI 可编译”写成内容质量 `GO`。M3 production 仍然等待 Q0 人工 Gate。

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
| Bitwig import | 固定版本导入、音色、保存/重开证据 | `LIVE-PENDING` |
| L4 内容信号 | Mode C 至少 4/6 Keep | `LIVE-PENDING`；没有真人反馈/评分 |
| Actual continued editing | Mode C 至少 3/6 | `LIVE-PENDING` |
| 反馈不退化 | Mode C 相对 B 至少 4/6 保持或提高 | `LIVE-PENDING` |
| 负面复核 | 主模型未过内容门槛时使用第二个不同强模型 | 条件尚未触发；Credential/model 未冻结 |

## 6. 完成 Q0 决策还需要什么

严格按 [`experiments/music-quality/HUMAN-GATES.md`](../../experiments/music-quality/HUMAN-GATES.md) 完成：

1. 在 Bitwig Studio 6.0.11 实际导入 pilot MIDI，固定 GeneralUser GS mapping，保存、关闭、重开并记录工程 hash/截图；
2. 对 6 个 L4 Mode B 结果写 1—2 条真实 Creator feedback，运行 Mode C；不能让另一个 LLM代写反馈；
3. 重新生成含 B/C 的匿名包，在不查看 private map 的情况下填写 Keep 与内容评分；
4. 对 Keep 的 L4 候选做真实音乐编辑、保存/重开、导出 edited MIDI，并记录操作数、时间与 hash；
5. 最后才解盲并应用冻结阈值。主模型若为负面，再用第二个不同强模型复核。

在这些证据完成之前，诚实的 Q0 状态只能是 `LIVE-PENDING`，不能授权 M3。
