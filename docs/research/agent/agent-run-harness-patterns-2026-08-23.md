# Agent Run Harness 源码模式与 Auto Studio M3 落地建议

> 日期：2026-08-23  
> 范围：Codex、Pi、DeepSeek Harness 的 Run/Turn/Tool/Approval/恢复机制，以及这些机制对 Auto Studio 真实音乐生成闭环的适用性。  
> 结论强度：源码事实使用固定 commit permalink；“建议/推断”是基于源码与 Auto Studio 当前实现的设计判断，不代表上游项目承诺。

> **当前适用性说明（2026-08-24）：** [ADR-0011](../../adr/0011-llm-authored-local-music.md) 已取消外部 Music Provider。本文关于稳定 identity、Run/Item 层次、Approval、checkpoint、projection、compaction 和崩溃恢复的研究仍适用；关于 `GenerateMusic` 远端 submit/observe/reconcile/download、Provider Job 与 Unknown Outcome 的落地建议已 superseded。

## 执行摘要

Auto Studio 不应复制任何一个代码 Agent 的完整 Harness。M3 最合适的是一个组合设计：

- 借 Codex 的 **Thread → Turn → Item 稳定读模型**、精确 `turnId/itemId` 关联、started/completed 通知和“持久化后再通知客户端”的恢复纪律；
- 借 Pi 已投入使用的 **简洁 Agent/Turn/Message/Tool 生命周期事件与 progress callback**；Pi 新增的 durable Harness schema 可作为术语参考，但在当前远端 HEAD 上，`prompt/resume/abort/watch` 仍明确返回 `HarnessNotImplemented`，不能当作成熟实现；
- 借 DeepSeek Harness 的 **append-only semantic event log、side effect 前 durability checkpoint、call/result 关联、crash-tail repair、纯 projection**，以及“已记录开始但无结果 = `TOOL_OUTCOME_UNKNOWN`”的安全分类；
- 保留 Auto Studio 已经正确实现的 **Generation Attempt 先落盘、Unknown Outcome 强制 reconcile-first、Candidate 数量校验、SQLite snapshot/event/outbox 同事务**。这比三者的通用 Tool abstraction 更接近付费媒体任务的真实风险。

M3 不需要先做通用多轮 Agent Loop，也不需要让 LLM 在一个 Turn 里持续轮询数分钟。LLM 只负责把 Brief 转为 typed `GenerateMusic` Plan；批准后，Core 的 durable worker 执行 `submit → reconcile/observe → download → verify → atomic commit`。TUI 从持久化投影显示进度，Provider 完成只唤醒应用状态机；仅在确实需要下一轮创作决策时才唤醒 LLM。

最关键的差异是：代码 Agent 的本地 `AbortSignal`、tool promise 和重试通常描述“本进程是否还在等”；音乐 Provider 的事实是“外部是否已受理、是否收费、是否还能取消”。因此：

1. `CancelRequested` 不能直接写成 `Cancelled`；
2. submit 超时不能自动重提；
3. crash repair 不能只生成一条错误 Tool Result，必须进入 Provider-specific reconcile；
4. LLM context summary 不能成为 Project、Approval、Job、Asset 或费用的事实来源。

## 研究版本与证据边界

三份本机仓库均来自官方 GitHub remote，但不是 2026-08-23 的远端 HEAD。本次先检查本机快照，再用官方远端 HEAD 的 shallow sparse clone/原始源码复核影响结论的文件。

| 项目 | 官方 remote | 本机 inspected baseline | 2026-08-23 复核 HEAD | 边界 |
|---|---|---|---|---|
| Codex | `openai/codex` | `57f42a81131ccf5933e7ec5dc659c381eeb5d72b` | `c9b19deb09c1841ce7acc33ddb96276030936a29` | 以远端 HEAD permalink 为结论证据；未把内部 rollout 当公开兼容协议 |
| Pi | `earendil-works/pi` | `29ad292f77400dfde14e30556859a0b1345465b7` | `a69bef789bc95abf0acee16f7b4660b70b650bb9` | 指本仓库的 Pi Agent Harness，不外推到其他同名项目；本机未跟踪文件不作证据 |
| DeepSeek Harness | `deepseek-ai/deepseek-harness` | `528c682e061696f5a160f363f236ecbf53cbd006` | `b150a551b8d465e31e418e1b2eaf5e79bbb7d28e` | 当前仍处快速演进期；借语义和测试方法，不依赖其 package topology/格式稳定性 |

Auto Studio 当前目录没有 Git 元数据，因此下面对本项目的映射只能给出当前工作区文件/行号，不能声称对应某个 commit。

## 三个 Harness 的源码机制

### Codex：面向客户端的稳定层次和严格终止边界

Codex 的 app-server 协议把一次交互投影为 `Thread → Turn → ThreadItem`。Turn 只有 `Completed / Interrupted / Failed / InProgress` 四种 coarse status；Turn 内包含带自身状态的 command、MCP tool、dynamic tool、plan、reasoning、compaction 等 item。这种“Run 粗状态 + Item 细状态”比把下载、校验、导入全部塞进一个 Run enum 更适合作为多客户端读模型。[Turn 状态](https://github.com/openai/codex/blob/c9b19deb09c1841ce7acc33ddb96276030936a29/codex-rs/app-server-protocol/src/protocol/v2/turn.rs#L27-L35) [ThreadItem 联合类型](https://github.com/openai/codex/blob/c9b19deb09c1841ce7acc33ddb96276030936a29/codex-rs/app-server-protocol/src/protocol/v2/item.rs#L227-L404) [Turn 读模型](https://github.com/openai/codex/blob/c9b19deb09c1841ce7acc33ddb96276030936a29/codex-rs/app-server-protocol/src/protocol/v2/thread_data.rs#L350-L386)

每个 item 都可发送 `ItemStarted` 和 `ItemCompleted`，并携带 `threadId + turnId + item`；流式文本和 tool progress 使用 item-specific delta。重要规则是：delta 只用于展示，completed item 才是权威结果。[item started/completed](https://github.com/openai/codex/blob/c9b19deb09c1841ce7acc33ddb96276030936a29/codex-rs/app-server-protocol/src/protocol/v2/item.rs#L1251-L1261) [completed payload](https://github.com/openai/codex/blob/c9b19deb09c1841ce7acc33ddb96276030936a29/codex-rs/app-server-protocol/src/protocol/v2/item.rs#L1325-L1335)

Steer 要求 `expectedTurnId`，interrupt 也必须指定 `turnId`，避免迟到 UI 操作影响另一轮活动。[steer/interrupt precondition](https://github.com/openai/codex/blob/c9b19deb09c1841ce7acc33ddb96276030936a29/codex-rs/app-server-protocol/src/protocol/v2/turn.rs#L170-L217) Core 终止任务时先发取消、给出 grace period、必要时强制 abort；更关键的是，interrupted marker 在 `TurnAborted` 前显式 flush，Turn terminal event 后也再次 flush，因为客户端可能收到通知后立即重新读取。[精确 turn abort](https://github.com/openai/codex/blob/c9b19deb09c1841ce7acc33ddb96276030936a29/codex-rs/core/src/tasks/mod.rs#L517-L562) [terminal flush](https://github.com/openai/codex/blob/c9b19deb09c1841ce7acc33ddb96276030936a29/codex-rs/core/src/tasks/mod.rs#L771-L839) [interrupt marker before notification](https://github.com/openai/codex/blob/c9b19deb09c1841ce7acc33ddb96276030936a29/codex-rs/core/src/tasks/mod.rs#L873-L930)

Approval request 用 `threadId + turnId + itemId`，特殊情况下另有独立 `approvalId`；这是值得采用的精确关联方式。但 Codex 允许 `AcceptForSession` 和 policy amendment，面向的是 shell/network 权限缓存，不适合直接用于可能改变模型、时长、候选数或价格的音乐消费授权。[approval identity](https://github.com/openai/codex/blob/c9b19deb09c1841ce7acc33ddb96276030936a29/codex-rs/app-server-protocol/src/protocol/v2/item.rs#L1448-L1509) [approval decisions](https://github.com/openai/codex/blob/c9b19deb09c1841ce7acc33ddb96276030936a29/codex-rs/app-server-protocol/src/protocol/v2/item.rs#L58-L80)

Codex 把 token usage 区分为 `last` 和 `total`，并保留 reasoning/cache breakdown；它没有在这些协议类型里表达媒体预估价、授权上限或“已扣费但结果未知”。[usage projection](https://github.com/openai/codex/blob/c9b19deb09c1841ce7acc33ddb96276030936a29/codex-rs/app-server-protocol/src/protocol/v2/thread.rs#L1750-L1809) Compaction 通过 replacement history 重建模型上下文；这是 LLM 历史优化，不应替代 Auto Studio 的 Project 事实。[rollout reconstruction](https://github.com/openai/codex/blob/c9b19deb09c1841ce7acc33ddb96276030936a29/codex-rs/core/src/session/rollout_reconstruction.rs#L320-L390)

**适用结论：** 直接借鉴身份层次、权威 completed item、stale action precondition 和 persist-before-notify。不能从 Codex 推导出媒体 Job 的幂等、费用审批、外部取消或 Unknown Outcome 对账能力。

### Pi：好用的流式事件面，但 durable Harness 仍是脚手架

Pi 已运行的低层 Agent API 把 UI 所需状态压缩成 `isStreaming / streamingMessage / pendingToolCalls / errorMessage`，事件为 `agent/turn/message/tool_execution` 的 start/update/end；Tool 通过 `onUpdate` 提交结构化 progress。这一套很适合 Auto Studio 的 TUI 事件命名和局部渲染。[AgentState、Tool progress、AgentEvent](https://github.com/earendil-works/pi/blob/a69bef789bc95abf0acee16f7b4660b70b650bb9/packages/agent/src/types.ts#L327-L443)

它支持 sequential/parallel Tool。并行实现先发 `tool_execution_start`，允许 body 重叠，再按 assistant source order 生成 Tool Result；这对多个只读分析工具有效，但 billable submit 和 Asset commit 不应默认并行。[parallel tool scheduling](https://github.com/earendil-works/pi/blob/a69bef789bc95abf0acee16f7b4660b70b650bb9/packages/agent/src/agent-loop.ts#L493-L553) [progress callback](https://github.com/earendil-works/pi/blob/a69bef789bc95abf0acee16f7b4660b70b650bb9/packages/agent/src/agent-loop.ts#L670-L710)

Pi 的 steering/follow-up queue 很适合区分“影响当前工作”和“当前工作结束后执行”；`abort()` 只触发当前 `AbortController`。这个语义不能被解释为远端音乐任务已取消。[queues and abort](https://github.com/earendil-works/pi/blob/a69bef789bc95abf0acee16f7b4660b70b650bb9/packages/agent/src/agent.ts#L264-L329)

当前 Coding Agent 的消息顺序是：先通知 extension/listener，再在 `message_end` 分支追加 SessionManager。它适合交互式代码 Agent，但不能复制到付费副作用边界，否则 UI 可能看到尚未持久化的“已开始/已完成”。[event then persistence](https://github.com/earendil-works/pi/blob/a69bef789bc95abf0acee16f7b4660b70b650bb9/packages/coding-agent/src/core/agent-session.ts#L620-L669) 其自动重试识别 rate limit/server error，排除 context overflow，并执行指数退避；该策略只可用于 LLM inference、GET/poll 和可证明幂等的读取，不能用于结果不明的 submit。[retry classification/backoff](https://github.com/earendil-works/pi/blob/a69bef789bc95abf0acee16f7b4660b70b650bb9/packages/coding-agent/src/core/agent-session.ts#L2762-L2861)

Pi 新的 Harness schema 有很好的概念：`operation_started/finished`、`abort_requested`、`step_attempt`、`tool_started`，并为 Tool 标注 `replay: never | safe`；Usage 按 assistant/compaction/tool/hook/adjustment 关联到 run 和 entry。[durable record schema](https://github.com/earendil-works/pi/blob/a69bef789bc95abf0acee16f7b4660b70b650bb9/packages/agent/src/harness/session/types.ts#L80-L212) JSONL storage 也是先 append durable mutation、再 apply 到内存，并能修复 torn tail。[append-before-apply](https://github.com/earendil-works/pi/blob/a69bef789bc95abf0acee16f7b4660b70b650bb9/packages/agent/src/harness/session/jsonl/storage.ts#L69-L107) [serialized durable writes](https://github.com/earendil-works/pi/blob/a69bef789bc95abf0acee16f7b4660b70b650bb9/packages/agent/src/harness/session/jsonl/storage.ts#L154-L191)

但这套新 Harness 在当前 HEAD 不能作为生产参考实现：恢复已有 records 会抛 `create.restore` 未实现，`prompt/compact/resume/abort/watch` 等核心方法仍统一返回 `HarnessNotImplemented`。[current scaffold state](https://github.com/earendil-works/pi/blob/a69bef789bc95abf0acee16f7b4660b70b650bb9/packages/agent/src/harness/agent-harness.ts#L305-L441)

**适用结论：** 直接借已运行的 event/progress vocabulary；改造 `replay` 为 Provider capability + operation-specific retry policy；不要采用当前 AgentHarness 实现，也不要把 Coding Agent 的 listener-before-persist 顺序或本地 abort 当成媒体保证。

### DeepSeek Harness：最接近“可恢复副作用”的基础机制，但仍不等于媒体对账

DeepSeek Harness 的 SessionEvent 是 append-only source of truth，明确定义 `turn/start|end`、`step/start|end`、`assistant/chunk|message`、`tool/call|result`、request header/context；模型历史只由 surface event 投影，边界、chunk、usage 等可保留在 log 而不进入模型上下文。[SessionEventMap](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/core/session/src/types.ts#L230-L337) [surface boundary](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/core/session/src/types.ts#L339-L378)

Agent 先 append turn/step/message，再请求模型，并在 `finally` 中闭合 step/turn。Tool scheduler 在 dispatch 前 append `tool/call`，result 用 call event seq 回链；并行 body 完成仍按模型顺序 commit result。[turn/step balance](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/core/agent-loop/src/agent.ts#L245-L329) [call before dispatch and correlated result](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/core/agent-loop/src/tool-calls.ts#L121-L180) [result correlation](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/core/agent-loop/src/tool-calls.ts#L261-L289)

`session-checkpoint-policy` 在 LLM dispatch、top-level Tool body 和下一 step 前显式 flush；checkpoint 失败即 fail closed，不执行下游副作用。这是 Auto Studio M3 最应该直接采用的纪律。[semantic checkpoints](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/session/session-checkpoint-policy/src/index.ts#L20-L82)

崩溃恢复时，Harness 区分两类未闭合 Tool：assistant 提出了 call 但没有 `tool/call`，记为 `TOOL_NOT_STARTED`；已有 `tool/call` 但没有 durable result，记为 `TOOL_OUTCOME_UNKNOWN`，并明确提示禁止盲重试有副作用的操作。[crash-tail classification](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/core/session/src/repair.ts#L12-L27) [unknown outcome synthetic result](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/core/session/src/repair.ts#L89-L124) Persistence coordinator 会把 closers 持久化，然后重新加载确切 revision，避免把旧内存图冒充新 durable revision。[repair commit/reload](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/session/session-persistence/src/coordinator.ts#L891-L970)

Approval 用独立 id 成对记录 `approval/asked` 与 `approval/decided`；唯一 grant 是 `allowed-once`，缺 answerer、异常 answerer 均 fail closed，abort 后迟到答案被丢弃。[one-shot approval](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/interaction/user-approval/src/types.ts#L10-L29) [paired audit and fail-closed](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/interaction/user-approval/src/index.ts#L239-L275)

Projection 采用纯同步 fold，并返回带 `asOfSeq` 的一致快照；cache 只作 fold shortcut，版本不符或超出 log end 就丢弃。[projection contract](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/session/session-projection/src/index.ts#L34-L81) [consistent snapshot/cache boundary](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/session/session-projection/src/index.ts#L84-L126)

当前 HEAD 还有 background Job seam：`running → stopping → terminal`、有界 wait、`job_output/list/kill`，完成通知对 idle Agent 的自动 wake 最多连续三轮，避免自激循环。[job lifecycle](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/jobs/jobs/src/types.ts#L13-L91) [bounded wake](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/jobs/tool-jobs/src/index.ts#L24-L53) [completion delivery](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/jobs/tool-jobs/src/index.ts#L259-L300) 但官方当前实现明确是 **process-local in-memory registry**，owner/service disposal 会取消任务；它不能直接承担跨重启的付费音乐 Job。[process-local implementation](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/jobs/jobs-local/src/index.ts#L1-L9)

**适用结论：** 直接借 durability checkpoint、not-started/unknown 分流、paired approval、projection 和 bounded wake。必须把通用 synthetic Tool error 改造为领域状态 + Provider reconcile；不能复用 process-local Job store。

## 比较矩阵

| 维度 | Codex | Pi | DeepSeek Harness | Auto Studio 决策 |
|---|---|---|---|---|
| 客户端读模型 | Thread/Turn/Item，成熟 | AgentState + lifecycle events，轻量 | event log + projection units | `Project/AgentRun/RunItem` snapshot + seq events |
| Tool call/result | typed item/status | start/update/end + Tool Result | call 先记录、result 回链 | `ToolExecution` 必须关联 Attempt/Job/Artifact |
| 持久化边界 | rollout + terminal flush | 已运行路径偏 message persistence；新 durable harness 未完成 | semantic checkpoint + write-behind | SQLite transaction + explicit pre-effect commit |
| Approval | 精确 item identity，可 session cache | beforeTool hook，未见统一 durable cost audit | paired one-shot、fail closed | one-shot cost approval，绑定不可变 request hash |
| Cancel | 精确 turn、本地 task 收敛 | AbortController | turn abort；Job seam 有 stopping | 本地停止、Provider cancel、最终事实分开 |
| Resume | thread/rollout 重建 | Coding session 可继续；新 harness resume 未完成 | log repair + replay | worker scan non-terminal items + reconcile/observe |
| Retry | 面向模型/本地工具 | LLM 分类 + backoff | request waterfall；Tool 依语义 | operation-specific matrix，submit 默认禁止重试 |
| Compaction | replacement history | summary/retained tail | surface replace + provenance | 只压 LLM context，不压 Project facts |
| Unknown Outcome | 无媒体领域模型 | 新 schema 有 replay tag但未完成 | 明确 Tool outcome unknown | 进入 `UnknownOutcome`，Provider reconcile-first |
| Usage/Cost | token last/total，无媒体费用 | Usage/cost schema，不等于媒体账单 | assistant token usage，无媒体费用 | estimate/approval/actual/unknown 独立账本 |
| 长任务 | background terminal 能力，但非媒体 contract | Tool promise/progress | process-local jobs + bounded wake | durable background worker；LLM 不 busy-poll |

## 直接采用、需要改造、不可照搬

### 可直接采用

1. **所有副作用先有 durable intent。** 在任何 Provider submit、cancel 或本地 Asset commit 前，先提交 `attemptId/toolExecutionId/requestHash` 和 operation phase；提交失败则不调用 Provider。
2. **全链路稳定身份。** `projectId/runId/turnId/itemId/toolExecutionId/attemptId/externalJobId/artifactId` 各司其职；Approval、Tool Result、Provider observation 都必须引用对应身份。
3. **事实先落库，UI 后通知。** TUI 收到 terminal/progress event 后重新读取 projection；不能依赖回调内存状态。
4. **开始未记录与结果未知分开。** 前者可重新调度，后者只能 reconcile/observe，除非 Provider 明确支持同一幂等键。
5. **一条完成通知最多触发有限 LLM turn。** 默认只更新 TUI/Project；只有新结果需要 Agent 作选择/编排时才 enqueue follow-up，并限制连续自动 wake。
6. **纯 projection。** TUI Snapshot 带 `asOfSequence/projectRevision`，SSE event 带 monotonic seq；断线先读 snapshot，再从 seq 续播。

### 需要按媒体领域改造

1. `Tool replay: safe/never` 改为按 operation 与 Provider capability 计算：`submit`、`observe`、`download`、`verify`、`commit`、`cancel` 分别判定，不能给整个 `music.generate` 一个布尔标签。
2. Approval 不只绑定 Tool 名称，而要绑定 `requestHash + providerConnectionId + model + duration + candidateCount + estimatedCost/currency + maxCost + termsRevision`。任一字段变化都使 Approval 失效。
3. 通用 `tool_execution_update` 只作展示；durable observation 还应保存 Provider status、progress、观察时间、provider request/event id 和原始状态摘要的受限哈希。
4. Tool Result 不直接携带大媒体；只返回 Candidate/Asset 引用、验证元数据、rights/provenance 和已知费用。
5. Cancel 分为 `CancelRequested`、`ProviderCancelAccepted/Rejected/Unknown`、远端 terminal observation。只有确认远端停止或领域策略允许时才进入 `Cancelled`。

### 不可照搬

- 不复制 Codex 的 shell/network session-wide approval 到付费生成；
- 不复制 Pi 的 listener-before-persist 顺序；
- 不把 `AbortSignal`、task abort 或 `job_kill requested` 当作远端已取消；
- 不对 submit 使用 Coding Agent 的通用指数重试；
- 不让并行 Tool scheduler 同时提交同一 Run 的多个 billable generation；
- 不把 DeepSeek 的 synthetic `TOOL_OUTCOME_UNKNOWN` Tool Result 当成对账完成；
- 不把 DeepSeek process-local Job registry 或 Pi JSONL 替换 Auto Studio 现有 SQLite ProjectPackage；
- 不持久化所有 LLM raw chunks、thinking 内容或 API Key；M3 只保留重建/审计所需的 semantic events 和 provider identifiers；
- 不为了采用模式而复制 DeepSeek 的插件数量、Codex rollout 内部格式或 Pi 尚未实现的 AgentHarness；
- 不在 M3 引入 Multi-Agent。一个 Creative Agent + deterministic Core worker 已覆盖当前闭环。

## 映射到 Auto Studio 当前 Rust 模块

当前代码已经有一条正确骨架：

- `crates/autostudio-core/src/agent.rs:247-264` 定义了 `AwaitingApproval → ReadyToSubmit → Submitting → Submitted/UnknownOutcome → terminal`；
- `crates/autostudio-core/src/agent.rs:314-399` 把本地 `GenerationAttempt` 与外部 `GenerationJob` 分开；
- `crates/autostudio-core/src/agent.rs:434-452` 的 Approval 已绑定 `inputHash` 与费用上限；
- `crates/autostudio-provider/src/lib.rs:312-358` 在外部 submit 前先通过 `ProjectService::prepare_generation` 持久化 Attempt，且 timeout 可进入 Unknown；
- `crates/autostudio-provider/src/lib.rs:429-500` 已有 reconcile-first；
- `crates/autostudio-provider/src/lib.rs:529-595` 已校验 Candidate 件数并经 Asset sink 提交；
- `crates/autostudio-storage/src/lib.rs:331-364` 已有 SQLite WAL/FULL、snapshot/event/outbox；`453-531` 在同一事务更新 revision、snapshot、event/outbox；
- `crates/autostudio-api/src/lib.rs:329-365` 已支持 `Last-Event-ID` SSE replay；
- `crates/autostudio-core/src/runtime.rs:12-29` 仍是 `plan/execute/reconcile/resume` 的窄应用 seam。

因此 M3 不应先替换 `CreativeRuntime` 为通用循环，而应在现有 crate 内补三个薄层：

| 当前模块 | M3 增量 | 不做 |
|---|---|---|
| `autostudio-core` | `RunItem/ToolExecution` 领域类型、phase/retry/cancel 不变量、Approval binding | 不引入 Provider HTTP、Tokio worker 细节 |
| `autostudio-provider` | 一个真实 Music Adapter、capability、submit/observe/reconcile/cancel、artifact descriptor、错误分类 | 不写 TUI 状态，不直接改 Project snapshot |
| `autostudio-storage` | durable item/event/observation/cost ledger，non-terminal recovery query | 不改为上游 JSONL/rollout 格式 |
| `autostudio-api` | Run projection、cancel endpoint、SSE phase/progress | 不把内存 progress 当权威 |
| `autostudio-tui` | 从 snapshot + event seq 投影 Run 卡片、approval/reconcile/cancel action | 不复制领域状态机 |

## 最小 M3 Agent Run 设计

### 领域层次

```text
Project
└── AgentRun                         用户目标与批准闭环
    ├── PlanningTurn                一次 Brief → typed Plan 推理
    └── GenerateMusicItem           对用户可见的工作项
        └── ToolExecution           Core 对一次批准输入的执行
            ├── GenerationAttempt   submit 前创建的本地身份
            ├── ProviderJob         Provider 接受后得到的外部身份
            └── ArtifactReceipt[]   下载、校验、Asset commit 证据
```

`AgentRun` 只保留 coarse status：`AwaitingApproval / Running / NeedsAttention / Completed / Failed / Cancelled`。当前 enum 可在 M3 保持 wire compatibility；细节新增到 `GenerateMusicItem.phase`，逐步让 UI 不再从 Run enum 猜下载/校验状态。

最小 `phase`：

```text
Prepared
→ Dispatching
→ Accepted
→ Observing
→ Downloading
→ Verifying
→ Committing
→ Completed

任意外部结果不明 → OutcomeUnknown → Reconciling → Accepted/Completed/ConfirmedNotFound
取消路径          → CancelRequested → CancelObserving → Cancelled/Completed/OutcomeUnknown
已知不可恢复错误  → Failed
```

### 最小 durable events

M3 不需要记录所有 token delta，以下事件足以恢复与投影：

| Event | 必要字段 | 写入时机 |
|---|---|---|
| `agent_run.planned` | run, plan, inference usage/provenance, requestHash | typed Plan 校验后 |
| `approval.requested` | approvalId, run/item, bindingHash, estimate, termsRevision | 显示授权前 |
| `approval.decided` | approvalId, outcome, maxCost, decidedAt | 一次性决定 |
| `tool.prepared` | toolExecutionId, attemptId, provider/model, requestHash, capabilityRevision | **submit 前事务** |
| `provider.submit_observed` | accepted/rejected/unavailable/unknown, requestId?, jobId? | submit 返回或超时后 |
| `provider.job_observed` | jobId, providerStatus, progress?, terminal?, observedAt, providerEventId? | poll/webhook 去重后 |
| `provider.cancel_requested` | job/attempt, reason, requestId | cancel API 前 |
| `provider.cancel_observed` | accepted/rejected/unknown/terminal | cancel/reconcile 后 |
| `artifact.downloaded` | artifactRef, staging path token, bytes, media type, expected hash? | 完成下载后 |
| `artifact.verified` | content hash, size, duration, codec/container, validation revision | **rename/commit 前** |
| `candidates.committed` | candidate ids, asset version ids, provenance, count | Asset + Project 事务后 |
| `tool.completed/failed` | toolExecutionId, result refs/error class, actual/unknown cost | terminal 收敛后 |

`provider.submit_observed=unknown` 不是 Tool failure，而是 `NeedsAttention`/自动 reconcile 工作。`actualCost` 缺失必须保持 `Unknown`，不得写 0。

### 执行与恢复算法

1. Planning Turn 读取固定 Project revision 和 Brief，生成唯一 typed `GenerateMusic` Plan；Plan 保存 inference usage，但不开放任意 Tool Loop。
2. Approval 绑定不可变 generation request 和费用/条款上限；批准后任何 model、duration、candidate count、reference asset 或 Provider connection 变化都必须重新计划/批准。
3. 一个 SQLite 事务写 `tool.prepared + GenerationAttempt + Project revision + outbox`；成功返回后才调用 `submit`。`attemptId` 同时作为 Provider idempotency/reconcile key（若 Provider 支持）。
4. submit 已知 rejected/unavailable：写 `Failed`；得到 job id：写 `Accepted`；连接中断且无法证明未受理：写 `OutcomeUnknown`，禁止第二次 submit。
5. background worker 扫描 `OutcomeUnknown` 先 reconcile，扫描 `Accepted/Observing` 执行有界 poll。网络临时失败只追加 observation error 和下次调度，不把 Job 误判 terminal。
6. Provider 成功后下载到受控 staging；支持安全的 range/resume 时可重试下载。对最终副本计算 hash、解析音频、校验时长/格式/件数，再原子 rename/Asset commit。
7. Candidate、AssetVersion、provenance、Tool Result、Run terminal 在一致的本地提交边界收敛；先持久化，再发 SSE。
8. Core 重启时扫描 non-terminal execution：`Prepared/Dispatching` 依 durable event 判断 not-started 或 unknown；unknown 调 reconcile，known job 调 observe，staging 文件按 receipt 恢复/清理，`Committing` 按 content hash 与 DB identity 幂等完成。
9. TUI 重连先 `GET RunProjection(asOfSequence)`，再从该 seq 接 SSE。progress event 可合并/丢弃，terminal event 不可被迟到 progress 覆盖。

### Operation-specific retry policy

| 操作 | 自动重试 | 约束 |
|---|---|---|
| LLM Plan inference | 可，有界 | 每次记录 usage/response id；结构化输出错误与 transport error 分开 |
| Provider submit | 默认否 | 仅“确认未发送”或 Provider 对同一 idempotency key 保证返回同一 Job 时可重试 |
| reconcile/observe | 可 | 指数退避 + jitter + Retry-After；不改变最后已知 Provider terminal state |
| webhook ingest | 可重放 | provider event id 去重；状态单调化，迟到事件不回退 terminal |
| artifact download | 可 | immutable version/etag/hash 或 range contract；完成后重新 hash |
| verify | 可 | 纯本地、相同字节与 validator revision 下确定性 |
| Asset/Candidate commit | 可 | content hash + transaction + unique identity，避免重复 Candidate |
| cancel | 先对账 | 只有 Provider 声明幂等时可重发；请求返回未知则保持 unknown |

## 迁移步骤

1. **冻结真实 Provider contract。** 在 `GenerationAdapter` 增加 capability manifest、`cancel`、progress/terminal/error、artifact descriptor、usage/cost 与 idempotency/reconcile 声明；先只实现一个 Provider。
2. **加入 RunItem/ToolExecution projection。** 保留现有 `AgentRunStatus` 兼容层，新增细 phase 和 stable ids；让 API/TUI 读取 projection，不从 Activity 文案推断状态。
3. **扩充 SQLite schema。** 增加 execution/observation/approval/cost/artifact receipt 的 durable 行或等价 event payload，并提供按 phase 查询 non-terminal work；继续使用单 writer 和同事务 outbox。
4. **实现 background coordinator。** Core 启动后恢复 non-terminal execution；submit、poll、reconcile、download 各自使用 operation-specific retry policy，不占用 LLM Turn。
5. **补取消和 Unknown Outcome UI。** TUI 显示“停止本地等待”“请求 Provider 取消”“需要对账”的不同事实；提供 reconcile action，不显示泛化 Retry 按钮。
6. **接入一个真实 Provider 并做 fault injection。** 先证明一条 `Brief → Plan → Approval → Candidate`，再考虑通用 Tool Registry、第二 Provider 或多轮自动改稿。

## M3 验收测试与 Gate

### 必须通过的自动化故障矩阵

- 在 `tool.prepared` 事务前/后、submit 发包前/后、接收 job id 前/后逐点 crash；恢复后最多存在一个 Provider Job；结果不明时进入 reconcile，submit 计数仍为 1。
- Provider 明确 rejected/unavailable 后，Run 必须进入可重新 Plan 的 terminal state，不能停在 `Submitting`。
- Provider 返回 job id 但本地写入失败：恢复按 Attempt reconcile 到同一 job，不得新建 job。
- poll 429/5xx/timeout、进程重启、SSE 断线不丢 terminal state；TUI snapshot + replay 与重开 Project 一致。
- cancel 在 submit 前、submit unknown、running、远端已成功、下载中分别测试；本地 abort 不得自动产生 `Cancelled`。
- webhook 重复、乱序、terminal 后迟到 progress 不回退状态。
- artifact 下载中断可恢复；最终副本 hash 后再 rename；hash/codec/duration/件数错误不产生 Candidate。
- Asset 已写、Candidate 事务失败的恢复不会产生重复 AssetVersion/Candidate，并能报告/清理 orphan staging。
- Approval 后修改 input/model/provider/duration/candidate count/cost/terms revision，执行必须 fail closed 并要求重新批准。
- usage/cost 缺失显示 `unknown`；估算、授权上限、已知实耗分别保存；任何日志/SSE/snapshot 不含 API Key、Authorization header、原始 thinking。
- compaction 后重新构建 LLM context，Project/Approval/Job/Asset/Selection 事实仍只从 SQLite 领域状态读取。

### 发布 Gate

M3 只有同时满足以下条件才算完成：

1. 一个真实 Agent Model + 一个真实 Music Provider，在目标 OS 上连续通过固定 Brief corpus；
2. fault-injection 证明 unknown submit 不重复计费/重复创建 Job；无法证明时必须保留 `UnknownOutcome`，不能伪装成功；
3. 每个 Candidate 都有可验证音频、content hash、Provider/model/job、input hash、rights/credits 和 Adapter revision；
4. Candidate 数量符合 Plan，Selection 与 Approval 分离，导出 WAV/DAW handoff 仍可继续编辑；
5. TUI 可在断线和 Core 重启后恢复相同 Run phase，并能明确显示“运行中 / 等待批准 / 需要对账 / 取消请求中 / 已完成 / 已失败”；
6. 内容质量 Gate 仍以盲测、采用率和继续编辑率为准，不能用 Harness 事件完整度替代音乐质量证据。

## 最终建议

Auto Studio 应实现“**durable creative run coordinator**”，而不是“通用代码 Agent Harness 的 Rust 复刻”。最小可靠边界是：

```text
LLM Planning Turn
  → one-shot Cost Approval
  → durable GenerateMusic ToolExecution
  → Provider Job worker
  → verified Asset/Candidate commit
  → TUI projection / optional bounded LLM follow-up
```

这条路径保留当前 Rust Core 的优势，也把三个上游最有价值的机制落到真正会产生音乐质量、费用和资产结果的地方。通用 Tool Registry、任意多轮 Loop、第二 Provider 和 Multi-Agent 都应在这条真实纵切通过 Gate 后再评估。
