# Agent Context Management：Pi、Codex、DeepSeek Harness 对比与 Auto Studio 方案

> 类型：Research，不定义发布资格
> 日期：2026-08-25
> 源码快照：Pi `c5ad7c1`、OpenAI Codex `d52478c`、DeepSeek Harness `b150a55`
> 结论状态：研究基线；截至 2026-08-26，CM-0—CM-4 Planning machine slice 已实现；CM-4 包含 Run 内 exact/FTS5-BM25 retrieval、source-linked provenance、Manifest 选择审计、可重建 projection 与 100-step long-run corpus。真实音乐 Tool 的长 Run 质量 Gate 仍待后续纵切

## 1. 结论

Auto Studio 不应完整复制其中任一实现，最合适的是一套分层组合：

- 以 **DeepSeek Harness** 的“append-only 事实日志 + 可替换 model surface”作为耐久基础；
- 以 **Pi** 的“结构化摘要 + 保留最近上下文 + 分支/重复压缩”作为跨 Provider 的默认 compaction；
- 以 **Codex / OpenAI Responses** 的完整 item replay、rollout 恢复和 opaque continuity/remote compaction 作为支持它的 Provider 优化；
- 增加三者都不能替 Auto Studio 决定的 **Project Fact Layer**：Brief、Project Revision、轨道、音符、Selection、Approval、预算、资产和 provenance 永远不以聊天摘要为真；
- 增加第一类不可变 **Context Snapshot / Manifest**，准确记录每个 Inference Turn 实际看到了哪些事实、对话、工具结果、schema 和 Provider 连续性绑定。

一句话概括：**完整事实留在本地，模型只看可重建的有界投影；摘要只替换模型视野，不改写历史，更不能改写音乐工程。**

## 2. 调研时的代码落差与当前进展

以下是 2026-08-25 调研开始时的代码落差：

- [`InferenceRequest`](../../crates/autostudio-provider/src/lib.rs) 只有 `brief` 与 `context_revision`；
- [`brief_prompt()`](../../crates/autostudio-provider/src/llm.rs) 只把 Project revision 和 Creative Brief JSON 拼成一次请求；
- 没有 durable `Turn`、`Message`、`ToolCall`、`ToolResult`、`ContextSnapshot`、`CompactionCheckpoint` 或 `ProviderContinuity` 实现；
- 因此当前不是“上下文策略需要优化”，而是 agent tool loop 的上下文地基尚未实现。

截至 2026-08-26，前述前三项已由 CM-0/CM-1 改写：代码已有 durable Inference Transcript、完整 Tool Request/Result、Context Manifest、三协议 SSE assembler、固定多轮 Planning Tool loop 与恢复。CM-2 又实现了 OpenAI Responses/Anthropic Messages continuity capture/replay 和 Project 外加密 Vault。CM-3 planning slice 已进一步实现 automatic safe-cut、bounded structured summary、有效缩短 Gate、同事务 crash/retry、大 Tool Result spill 与单次 overflow recovery。CM-4 planning slice 现已实现 Run 内 exact/FTS5-BM25 retrieval、source-linked hit、Manifest Selection、summary/current-tail 去重与可重建 SQLite projection；冻结合同完成 100 steps、10 compactions、3 restarts 与模拟 cross-day recovery，并覆盖旧约束、Creator decision、artifact 与 unresolved Tool Result 召回。真实音乐 Tool 正确率、Approval Grant、Run Budget 与通用 ToolExecution 仍只是后续规格，不能报告为已交付能力。

## 3. 三者对比

| 维度 | Pi | OpenAI Codex | DeepSeek Harness | 对 Auto Studio 的价值 |
| --- | --- | --- | --- | --- |
| 耐久历史 | JSONL session tree，entry 通过 `parentId` 形成活动分支 | rollout items，可从 compaction checkpoint、turn 和 world state 重建 | append-only typed `SessionEvent` 是单一事实源 | 使用 append-only Run Transcript，不覆盖旧事实 |
| 模型可见上下文 | 从当前 branch 构造；遇到 compaction 时使用摘要 + kept tail | `ConversationHistory` 投影 Response items；支持本地/远端 replacement history | 从 log 派生 surface；replace 只遮蔽旧 surface，不删除日志 | 明确分离 durable transcript 与 current surface |
| 压缩策略 | 阈值触发；结构化摘要；默认保留约 20k recent tokens；支持重复压缩与 split turn | 可先裁剪 function output，再做本地或 `/responses/compact` 远端压缩 | 先 pruner，再重测，再摘要；只在 surface generation 确实推进时恢复重试 | 先机械减重，再语义摘要，再验证确实节省 token |
| Tool 原子性 | cut point 不落在 tool result；split turn 有专门处理 | history 包含 tool call/output，压缩前可重写过大输出 | compaction boundary 强制 call/result 配对平衡 | `ToolRequest + ToolResult` 是不可拆分上下文原子 |
| Provider 连续性 | Provider adapter 规范化消息，主体是 host-managed replay | Responses 可通过 previous response / returned items 延续 reasoning；remote compaction 返回 opaque item | adapter 可保存 reasoning block / private replay state | 单独放进加密、短生命周期 Continuity Vault |
| 恢复 | session JSONL + branch/path 重建 | rollout reconstruction，compaction checkpoint 参与恢复 | event replay、flush barrier、cold recovery 补齐 interrupted boundary | 每次 turn 都能从 durable log 重建，不依赖内存聊天数组 |
| 扩展方式 | SessionManager 与 extension hooks，较直接 | 内部模块较深，针对 Codex 工作流优化 | 大量 capability seam、事件和可替换插件 | 只借鉴 seam 思路，不复制完整插件平台 |

## 4. 可以直接借鉴的设计

### 4.1 Pi：最适合作为跨 Provider 的默认 compaction

Pi 的关键点不是“做了一次摘要”，而是将完整 session 与当前上下文分开：

1. session entry 仍保存在 JSONL tree 中；
2. `buildContextEntries()` / `buildSessionContext()` 沿当前 leaf 回到 root；
3. 遇到 `CompactionEntry` 时，未来请求使用结构化 summary，加上 `firstKeptEntryId` 之后的最近消息；
4. 重复压缩会从上次保留边界继续计算，而不是把旧 summary 当作全部历史；
5. 一个超长 tool-heavy turn 可以切分，但 cut point 不落在 tool result 上，避免孤立结果。

默认阈值和 token 数只能作为参考，不能硬编码进 Auto Studio。真正值得复制的是：

- host-managed、Provider 无关；
- 摘要结构固定；
- 最近原文保留；
- 原始 session 不被摘要覆盖；
- 重复 compaction 和 branch/resume 都有明确语义。

来源：[Pi compaction](https://github.com/earendil-works/pi/blob/c5ad7c1b0f7623bbfdf64dd4967fa6e99c15c01a/packages/coding-agent/docs/compaction.md)、[session format](https://github.com/earendil-works/pi/blob/c5ad7c1b0f7623bbfdf64dd4967fa6e99c15c01a/packages/coding-agent/docs/session-format.md)、[compaction source](https://github.com/earendil-works/pi/blob/c5ad7c1b0f7623bbfdf64dd4967fa6e99c15c01a/packages/coding-agent/src/core/compaction/compaction.ts)。本机检查的 Pi package 为 `@earendil-works/pi-coding-agent 0.84.2`。

### 4.2 Codex：完整 item replay、恢复与 Provider 原生连续性

Codex 的价值在于长期运行时仍保留“发生过什么”和“模型下一轮看什么”之间的边界：

- `ConversationHistory` 处理 user/assistant message、reasoning、function/tool call 与 output、compaction 等 Response items；
- rollout reconstruction 会从持久化 rollout items 重建 history、turn settings、world state 和 compaction replacement baseline；
- remote compaction 在发送前可裁剪 function output，并把当前 tool schemas 与 history 一起交给 Provider compact endpoint；
- OpenAI Responses 支持用 `previous_response_id` 延续多轮状态，或在无服务端存储场景中回传所有相关 response output items；
- `/responses/compact` 返回用于继续工作的 opaque compacted item，不应解析为 Auto Studio 自己的事实或摘要格式。

值得复制的是：

- canonical item 级 replay，而不是字符串拼接；
- compaction checkpoint 可参与重启恢复；
- Provider 原生 continuity 只是一种优化，不是跨 Provider 的唯一上下文；
- 工具定义、推理 item、输出 item 和历史共同计入真实请求面。

来源：[Codex context history](https://github.com/openai/codex/blob/d52478c52ef09f001142a4b82339467c3880877f/codex-rs/core/src/context_manager/history.rs)、[remote compaction request](https://github.com/openai/codex/blob/d52478c52ef09f001142a4b82339467c3880877f/codex-rs/core/src/compact_remote_request.rs)、[rollout reconstruction](https://github.com/openai/codex/blob/d52478c52ef09f001142a4b82339467c3880877f/codex-rs/core/src/session/rollout_reconstruction.rs)、[OpenAI model guidance](https://developers.openai.com/api/docs/guides/latest-model)、[Compact a response](https://developers.openai.com/api/reference/java/resources/responses/methods/compact)。

### 4.3 DeepSeek Harness：日志、surface 与 compaction transaction

DeepSeek Harness 把边界表达得最明确：

- `SessionEvent` append-only log 是事实源；LLM messages 是从 log 派生的 projection；
- 每个 surface event 声明 append/replace，compaction checkpoint 遮蔽旧 surface，但原始 log 仍可审计；
- `request/header` 保存 provider、model、call config、system prompt 和 tool schemas，增强请求重建能力；
- token meter 固定到一个 log revision，测量 system、tools、history、tool results、steering 等完整请求面；
- compaction 是 start/summary/end transaction，能识别 crash 后未闭合的尝试；
- 自动压力和 Provider context overflow 分开处理；只有 surface replacement generation 前进后才能重试；
- 大 tool result 可先 spill/prune，再决定是否需要 LLM 摘要；
- compaction boundary 保证 tool call/result 配对。

值得复制的是 append-only log、derived surface、请求快照、分阶段压缩和“有实际进展才重试”。不应复制它当前全部 Cordis/capability/plugin 复杂度。官方仓库仍将该项目标为 Developer Preview。

来源：[DeepSeek Harness README](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/README.md)、[session](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/docs/subsystems/session.md)、[core loop](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/docs/subsystems/core.md)、[compaction](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/docs/subsystems/compaction.md)、[system prompt](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/docs/subsystems/system-prompt.md)、[spill](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/docs/subsystems/spill.md)。

## 5. Auto Studio 的目标分层

### 5.1 六类状态必须分开

| 层 | 保存什么 | 是否权威 | 生命周期 | 是否直接进入模型 |
| --- | --- | --- | --- | --- |
| Project Facts | Brief、Project Revision、Timeline、MIDI、资产、Candidate、Selection、Approval、budget ledger、provenance | 是 | Project 生命周期 | 通过有界 snapshot 投影 |
| Inference Transcript | 可见消息、完整 ToolRequest/ToolResult、usage、finish/error | 对 Run 历史权威 | Run 后可审计 | 只选择 current surface |
| Context Surface | 当前 summary、最近 tail、相关事实和引用 | 否，可重建 | 随 turn/compaction 变化 | 是 |
| Context Snapshot / Manifest | 某次 turn 实际使用的 item ids、revision、hash、预算和 schema fingerprint | 对“该轮看到了什么”权威 | Run 后可审计 | Manifest 本身不必进入 |
| Provider Continuity Vault | reasoning item、signed thinking、response reference、adapter replay state | 否，只用于协议连续性 | Run 内短期 | 由相同 Adapter 原样回传 |
| Spill / Artifact Store | 大型 tool output、MIDI、波形、音频、插件 state | 内容权威 | 按 Project/Run policy | 只传摘要、hash、locator 和读取方法 |

这解决了最关键的冲突：**不持久化 private reasoning** 与 Provider 的 tool-use continuity 要求可以同时成立。私有 payload 只进入加密 Vault，不进入 Creator transcript、Project、Export、普通日志或 compaction summary。

### 5.2 Context Manager 是一个深 Module

外部调用者只需要一个主要入口：

```rust
pub trait ContextManager {
    async fn prepare_turn(
        &self,
        request: PrepareContext,
    ) -> Result<PreparedContext, ContextError>;
}
```

`prepare_turn()` 内部完成：

1. 读取精确 Project Revision 与 Run transcript revision；
2. 固定 system/policy、Thinking、Provider/Model/Protocol 和 Tool Catalog fingerprint；
3. 计算实际 input budget；
4. 选择 Project facts、最近 transcript、未解决事项和工具结果；
5. 对大结果先引用化/裁剪；
6. 必要时生成或复用 compaction checkpoint；
7. 校验 tool call/result、Approval 和 Project fact 不变量；
8. 生成不可变 `ContextManifest` 和 canonical messages；
9. 只向 Adapter 交付与 binding 匹配的 continuity reference。

Harness 不应要求调用者依次调用 `estimate()`、`prune()`、`summarize()`、`assemble()`。这些是 Module 内部策略，否则每个 Client/Run 路径都会重新实现上下文正确性。

### 5.3 建议的数据骨架

```rust
pub struct ContextManifest {
    pub context_id: ContextId,
    pub run_id: RunId,
    pub turn_id: TurnId,
    pub project_revision: u64,
    pub transcript_revision: u64,
    pub included_item_ids: Vec<InferenceItemId>,
    pub project_fact_refs: Vec<ProjectFactRef>,
    pub compaction_checkpoint: Option<CompactionId>,
    pub tool_catalog_fingerprint: Digest,
    pub provider_binding: ProviderBinding,
    pub token_budget: TokenBudgetPlan,
    pub content_hash: Digest,
}

pub struct CompactionCheckpoint {
    pub compaction_id: CompactionId,
    pub replaces_item_ids: Vec<InferenceItemId>,
    pub summary: StructuredRunSummary,
    pub first_kept_item_id: InferenceItemId,
    pub source_revision: u64,
    pub source_hash: Digest,
}
```

`StructuredRunSummary` 至少分栏保存：

- 当前目标与 Creator 明示约束；
- 已确认的音乐决定；
- 已完成动作及其稳定 execution/artifact ids；
- 当前 Project Revision 和引用，不复制项目内容作为新事实；
- 失败、未知结果和不可重试事项；
- 尚未解决的问题；
- 下一步；
- 被摘要 transcript item 的引用范围。

## 6. 上下文组装与压缩策略

### 6.1 每轮的固定优先级

从最高到最低：

1. system、safety、approval 和 tool policy；
2. 当前 Creative Brief 与精确 Project facts；
3. 未完成/未知结果的 ToolExecution 和等待中的用户决定；
4. 本轮或最近步骤的完整 ToolRequest/ToolResult；
5. 最近用户可见对话原文；
6. 结构化 compaction summary；
7. 较旧参考资料的摘要和稳定 locator。

低优先级内容不能挤掉高优先级事实。工具 schema 和预留输出 token 也必须计入预算，不能只统计 message text。

### 6.2 两阶段 compaction

第一阶段是确定性减重：

- 把大 MIDI、音频分析、插件扫描和 stdout 保存到 Spill/Artifact Store；
- prompt 只保留类型、关键字段、hash、大小、locator 和读取方法；
- 移除重复、已经被新 snapshot 取代的动态上下文；
- 保留完整原始 Transcript，不在 durable log 内截断 ToolResult；
- 当前 step、未知结果、未闭合 tool pair 不得裁剪。

第二阶段才是语义摘要：

- 只总结已闭合的旧 transcript 区域；
- 使用固定 schema，不接受自由散文作为唯一恢复依据；
- 保留最近 tail；
- summary 必须引用被替代 item ids，并通过 Project fact、execution id、tool pairing 校验；
- 新 surface token 没有实质减少时不提交 checkpoint；
- Provider overflow 最多自动恢复一次，且只有 manifest/surface generation 确实变化才重试。

### 6.3 预算而不是固定 token 魔数

```text
input_budget = model_context_window
             - requested_output_reserve
             - provider_safety_margin

used = system + tools + project_facts + surface + continuity_overhead
```

建议初始策略：

- soft pressure：`used >= 75% * input_budget`；
- hard pressure：`used >= 90% * input_budget`；
- recent raw tail：目标为 input budget 的 25%–35%，同时设置绝对上下限；
- structured summary：不超过 input budget 的 15%；
- 输出、工具 schema、continuity overhead 必须单独预留。

这些值是起始参数，不是产品承诺；应通过真实音乐 Tool loop corpus 调整。

## 7. Provider 适配

### OpenAI / Codex 类 Responses

- 优先保存相同 Provider/Model/Protocol 返回的 response/reasoning/compaction items 或 reference；
- `previous_response_id` 或 opaque compacted output 可降低重放成本；
- 切换 Provider、Model、protocol 或 tool catalog 后使不兼容 continuity 失效；
- 失效后从 canonical transcript + host summary 重建，不尝试解释 opaque item。

### DeepSeek

- Thinking Mode 的同一 tool-call chain 需要回传该轮 `reasoning_content`；
- 这类内容进入 Continuity Vault，而不是可见 Transcript；
- 新用户轮次或 tool chain 结束后按协议和 retention policy 丢弃；
- DeepSeek Responses API 当前不替客户端提供通用 context management，host compaction 仍是基线。

来源：[DeepSeek Thinking Mode](https://api-docs.deepseek.com/guides/thinking_mode/)、[Responses API](https://api-docs.deepseek.com/guides/responses_api/)。

### Pi 风格和其他 Chat/Message Provider

- 使用 canonical message replay + host structured compaction；
- Adapter 只负责 wire mapping 和它自己需要的 opaque replay state；
- 不让 Provider 专有字段渗入 Project domain 或 Context Manager 公共接口。

### 跨 Provider 切换

跨 Provider 可迁移的只有：

- Project facts；
- visible transcript；
- complete canonical ToolRequest/ToolResult；
- host-generated structured summary；
- stable artifact references。

Provider private reasoning、signed block、response id 和 opaque compaction item 均不可迁移。

## 8. 恢复、安全与可观测性

必须满足：

- Transcript append、ToolExecution commit、Project commit、Continuity Vault write 是不同状态，恢复时不能互相猜测；
- 每个 Turn 开始前持久化 `ContextManifest`，Provider 请求后记录 request identity 和结果状态；
- crash 后从 Transcript + Project Snapshot 重建 surface；存在匹配 ContinuityRef 才恢复原 Provider chain；
- partial tool call 永不成为 durable `ToolRequest`；完整 ToolRequest 已出现后必须有 ToolResult、Interrupted 或明确 recovery 状态；
- tool output、MCP 内容和外部文件一律作为 untrusted data，不能升级成 system/policy；
- Continuity Vault 使用 OS secure storage 派生密钥、加密、TTL 和绑定校验；payload 不进入 debug log、SSE、备份或 Export；
- Run 终态语义提交后清理 continuity；清理失败是可见且可重试的维护状态。

可观测性记录 token 分类、选入/排除原因、compaction 前后大小、checkpoint id 和恢复路径，但不记录 secret 或 private reasoning 内容。

## 9. 实施顺序

### CM-0：Context Foundation

- 引入 `TurnId`、`InferenceItem`、append-only Transcript 和 revision；
- 引入 `ContextManifest`、`TokenBudgetPlan`、canonical message/tool types；
- `InferenceRequest` 改为 `InferenceTurnRequest`；
- 完成重启 replay 测试。

### CM-1：真实 Tool Loop

- streaming assembler；
- complete ToolRequest/ToolResult 持久化；
- `prepare_turn()` 组装 Project facts、recent tail 和 tool schemas；
- 每个 step 从 durable state 重新派生，不依赖 UI 或内存消息数组。

实施状态（2026-08-25）：`CM-1 Planning slice` 已完成。首次 Provider 调用前先提交可见 `planning` Run；OpenAI Chat/Responses 与 Anthropic Messages 统一走 SSE assembler；`project_describe → submit_creative_plan` 的完整 Request/Result 配对耐久化；每一步只从 Project 与 Transcript 重新派生；待执行本地 Tool 和已完成 Plan 可恢复，仅有 `ContextPrepared` 而无 Provider 输出时以 `inference_interrupted` 安全失败且不重提。默认 `deepseek-v4-flash` 已通过一次真实计费流式 Tool Call smoke；该模型开启 thinking 时由 Adapter 使用 `tool_choice=auto`，Core 仍执行 tool identity、fingerprint 与参数校验。这里的 Tool Module 是固定纵切，不等于 M3-C 通用 Tool Registry/ToolExecution。

### CM-2：Continuity Vault

- OpenAI、Anthropic、DeepSeek adapter binding；
- 加密、TTL、purge、model/provider/tool-catalog 失效；
- Provider 切换 fallback 测试。

实施状态（2026-08-26）：`CM-2 Planning slice` 已完成。OpenAI Responses 捕获完整 reasoning/function output item，Anthropic Messages 捕获完整 signed thinking/tool-use content block；同一 binding 的下一轮由对应 Adapter 原样回传。`FileContinuityVault` 在 Project Package 外使用 XChaCha20-Poly1305、独立本地密钥、随机 nonce、AAD、7 天 TTL、启动清理与每小时 janitor。binding 覆盖 run/provider/model/protocol/thinking/capability/mapping/tool catalog；错配、过期、未知 schema 和损坏密文会失败关闭并删除。测试证明 sentinel 不进入 Project SQLite、Context Event、backup 或 Debug，终态 purge 失败不会提交成功 Plan。`gpt-5-mini` 已通过两轮真实 Responses Continuity Planning 测试；Anthropic exact-model live 与 OS Credential Vault 仍为 `LIVE-PENDING`。OpenAI-compatible Chat/DeepSeek 继续依赖 canonical Transcript，不伪装成原生私密连续性恢复。

### CM-3：Compaction

- deterministic spill/prune；
- structured summary + recent tail；
- checkpoint transaction、有效缩短验证和一次 overflow recovery；
- 多次 compaction、crash、超长单 turn 和 orphan tool pair 测试。

实施状态（2026-08-26）：CM-3 Planning slice 已完成。`prepare_turn` 先从完整 Transcript 派生 latest summary + raw tail，执行大 Tool Result spill 并测量 footprint；hard/overflow 或明确的 Provider context overflow 会触发 automatic compaction。cut 只位于完整 Turn 边界，必须推进连续前缀、不拆 Tool pair、保留本轮新输入和最近两轮。Core 生成有界 structured summary，只有 candidate surface 实际变短并回到 `Normal` 才把新输入、Checkpoint、Manifest 与 spill 同事务发布。失败注入证明事务前崩溃零落盘，相同 source facts 重试得到相同 checkpoint content hash；重启由完整 Transcript 重建。明确的 HTTP/SSE overflow code/message 会落盘为 `ContextOverflow`，清除旧 Continuity 后只允许一次恢复，第二次停止。超过 16 KiB 的 Tool Result 仍只以 512 字符预览 + source item/hash/原始字节数进入 surface。

剩余资格项：Provider-specific 精确 tokenizer 校准、真实 Provider overflow live，以及超长 single-turn 的产品级缩减/拆分策略；当前无安全 cut 时会 fail closed。DeepSeek 官方 Responses API 文档明确说明超出 context window 会返回 HTTP 400 且不支持自动 truncation，因此 host-owned compaction/recovery 仍是必要基线：[DeepSeek Responses API](https://api-docs.deepseek.com/guides/responses_api/)。这些资格项不改变 CM-3 contract 已通过，也不替代 CM-4 long-run corpus Gate。

### CM-4：Long-Run Context Retrieval（必做）

长 Run 是 Auto Studio 的基本产品能力，不是观察到问题后才补的优化。Creator 必须能够在同一个 Agent Run 中持续完成多轮创作、试听、修改、比较与回退；Run 需要跨多次 compaction、进程重启和间隔恢复后继续工作。

CM-4 至少实现：

- 对同一 Run 完整 Transcript 的精确 item/id 查询和本地全文检索；
- 从已经退出 current surface 的历史中找回旧约束、Creator 决定、ToolExecution、artifact 和未解决事项；
- retrieval 结果带稳定 source item ids、来源类型、时间、Project Revision 和内容 hash；
- 每次注入都进入 `ContextManifest`，明确记录检索原因、选中片段和 token 成本，不能静默修改记忆；
- retrieved tool content 和外部文本保持 untrusted，不能覆盖 system、policy 或 Project Facts；
- 检索索引是从 durable Transcript 重建的 projection，不成为第二份事实源；
- compaction checkpoint、recent tail 与 retrieval 去重，避免同一内容重复占用上下文。

第一版优先采用 Run 内结构化过滤、精确引用和本地全文/BM25 检索。向量检索是否加入由 long-run corpus 的召回质量决定，但 **Long-Run Retrieval 本身属于必做范围**。MVP 仍不引入跨项目自动人格记忆或通用知识图谱。

## 10. 完成 Gate

以下全部通过才可把“Context Management”从 `NOT IMPLEMENTED` 改为已交付：

1. 冻结的 long-run corpus 能完成至少 100 个 inference steps、10 次 compaction、3 次进程重启和一次跨日恢复，且不超过模型上下文；
2. compaction 前后 Project Revision、Selection、Approval、budget ledger 和 artifact hash 完全一致；
3. 任意 crash point 重启后可重建相同 current surface 或进入明确 `NeedsAttention`；
4. 不产生孤立 ToolRequest/ToolResult；
5. Provider 切换不复用不兼容 continuity，仍能从 canonical state 继续；
6. Project package、Export、日志和客户端事件中找不到 private reasoning/secret payload；
7. 相同 Project/Transcript revision、catalog 和 policy 能生成相同 `ContextManifest.content_hash`；
8. overflow recovery 没有产生无限 compaction/retry；
9. `PASS（machine corpus）`：被移出 current surface 的早期约束、Creator 决定、artifact 和 unresolved Tool Result 可通过 source-linked retrieval 找回；
10. `LIVE-PENDING`：真实音乐任务盲测中，compaction 与 retrieval 后的约束保持率和工具正确率达到预先冻结的门槛。

## 11. 不建议做的事

- 不用一个不断增长的 `Vec<Message>` 作为唯一状态；
- 不把 compaction summary 写回 Project Facts；
- 不把 Provider opaque continuity 当成可审计 transcript；
- 不持久化可见 private reasoning log；
- 不把大 MIDI、音频、波形或插件 state 直接塞进 prompt；
- 不在 M3 建通用插件化 Context 平台或跨项目长期记忆；
- 不把“上下文窗口更大”当作不需要 compaction、恢复和审计的理由。

## 12. 结论性建议

CM-0—CM-4 的 Planning machine slice 已完成。CM-4 以现有 durable Transcript、ContextManifest、CompactionCheckpoint 和 Continuity binding 为基础，加入 source-linked、可重建、可审计的本地检索，没有把摘要或索引升级成第二份工程事实，也没有引入跨项目向量记忆平台。

下一步应实现 Approval Grant / Run Budget，再进入通用 ToolExecution；真实音乐 Tool 落地后，必须继续执行本报告 Gate 10 的 compaction/retrieval 约束保持率和工具正确率盲测。CM-4 的 machine PASS 不能替代该真人内容资格。
