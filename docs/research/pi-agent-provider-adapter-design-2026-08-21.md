# Pi Agent Provider 适配设计研究

> 文档状态：实现参考快照。当前 Rust LLM Adapter、Model Catalog 与 Thinking 映射以代码和技术设计为准；Pi 的通用流式能力不表示 Auto Studio 的通用 Agent Loop 已完成。  
> 研究日期：2026-08-21  
> 对象：历史入口 [badlogic/pi-mono](https://github.com/badlogic/pi-mono) 当前重定向到 canonical 仓库 [earendil-works/pi](https://github.com/earendil-works/pi)；研究范围为 `packages/ai`（原 `pi-ai`）与 `packages/agent`（原 `pi-agent-core`）  
> 固定源码快照：[5cd93f688aaab89dbb6dfa4aca535f21796ae185](https://github.com/earendil-works/pi/commit/5cd93f688aaab89dbb6dfa4aca535f21796ae185)（2026-08-20）

## 1. 结论先行

Pi 最值得 Auto Studio 借鉴的，不是某个具体 Provider SDK，而是三个清晰的边界：

1. **Provider 与 API protocol 分离**：Provider 表示运行时身份、鉴权、模型目录和协议实现的组合；API 表示可被多个 Provider 共用的线协议。`github-copilot` 甚至能按 `model.api` 在 Anthropic、OpenAI Completions、OpenAI Responses 三套实现间分发，证明这不是只写在文档里的抽象。[官方说明](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/README.md#L230-L264) [Provider 类型](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/models.ts#L88-L149) [混合协议 Provider](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/providers/github-copilot.ts#L1-L33)
2. **统一语义层与厂商转换层分离**：文本、thinking、tool call、usage、stop reason 和流事件先归一，再由每个协议适配器处理签名、工具 ID、角色、缓存和推理参数差异。[统一内容与用量](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/types.ts#L350-L419) [统一消息](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/types.ts#L421-L488) [统一流事件](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/types.ts#L527-L551)
3. **模型调用与 Agent 循环解耦**：Agent 只依赖注入的 `StreamFn`，在 LLM 边界执行 `transformContext` / `convertToLlm`，工具仍由宿主应用执行。[Agent 注入示例](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/agent/README.md#L15-L64) [StreamFn 类型](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/agent/src/types.ts#L18-L32) [LLM 边界](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/agent/src/agent-loop.ts#L281-L371)

【推断】这套设计是一个成熟的**多 LLM 对话协议归一层**，但不是通用的多媒体生成作业系统。它的主抽象仍是一次 LLM response：`Context -> AssistantMessage/EventStream`；图片生成虽另有 API，也仍是一发请求、内存中返回 base64 内容。[图片接口](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/images-models.ts#L8-L88) [OpenRouter 图片返回](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/api/openrouter-images.ts#L40-L115)

【建议】Auto Studio 应当**借鉴边界与测试方法，不直接复用 TypeScript 运行时或把 Pi 的 message/stopReason 当成 Generation Job 状态机**。本项目既定边界是 Rust Core、凭证留在系统保险库、媒体进入 Asset/AssetVersion、外部生成走 `validate -> estimate -> submit -> observe -> reconcile -> download`；这些职责已经超出 Pi Provider 的范围，参见本项目的[本地优先 BYOK ADR](../../docs/adr/0001-local-first-byok-desktop.md)、[Rust Core ADR](../../docs/adr/0004-rust-core-professional-audio-engine.md)和[技术设计](../../docs/design/auto-studio-technical-design.md)。

## 2. 证据口径

- 【源码事实】只陈述固定提交中的类型、实现、官方 README、脚本与测试能直接支持的内容。
- 【推断】从多个一手证据归纳出的架构含义；不冒充上游承诺。
- 【建议】针对 Auto Studio 的设计取舍；本地事实以项目 ADR、设计文档和共同语言为依据。
- 【源码事实】仓库根 README 的 package 表列出 `@earendil-works/pi-ai` 与 `@earendil-works/pi-agent-core`；结合 GitHub 重定向，本文将 `earendil-works/pi` 视为当前 canonical 源，将 `badlogic/pi-mono` 仅视为兼容入口。下文旧入口 permalink 会由 GitHub 重定向到同一固定提交，不影响行级证据。[根 README](https://github.com/earendil-works/pi/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/README.md#L13-L19)
- 【源码事实】当前快照的包名已是 `@earendil-works/pi-ai` / `@earendil-works/pi-agent-core`，版本均为 `0.84.2`；本文仍沿用社区常用的 `pi-ai` / `pi-agent-core` 称呼。[ai package](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/package.json#L1-L5) [agent package](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/agent/package.json#L1-L5)

## 3. 总体架构

```text
pi-agent-core
  Agent / agentLoop
    -> 注入 StreamFn
      -> pi-ai Models（实例级 Provider 注册表、鉴权、目录刷新、分发）
        -> Provider（身份 + auth + model catalog + 一个或多个 API 实现）
          -> API protocol adapter（Anthropic / OpenAI / Google / pi-messages / ...）
            -> 厂商端点

宿主应用
  <- toolCall
  -> 执行 Tool
  -> toolResult 再进入下一轮
```

【源码事实】`Models` 持有实例级 Provider `Map`，负责查模型、刷新目录、解析鉴权、调用 stream/complete，以及在 Provider 声明能力时调用 deferred fetch/cancel；`MutableModels` 才暴露 set/delete/clear Provider。[Models 接口](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/models.ts#L151-L236) [注册表实现](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/models.ts#L254-L318)

【源码事实】核心入口刻意不自动加载所有 Provider；内置 Provider 由独立聚合入口显式注册，工厂内部再懒加载 SDK。旧全局 registry 被标成兼容层并计划移除。[核心导出](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/index.ts#L1-L46) [内置 Provider 注册](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/providers/all.ts#L88-L154) [旧 registry 状态](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/README.md#L1565-L1587)

【建议】Auto Studio 也应使用**项目/运行时实例级 registry**，不要依赖全局单例；但 registry 的键至少要区分 `provider_kind`、`connection_id`、`protocol_family`，因为同一家 Provider 可能存在多个 BYOK 账户、区域或网关。

## 4. 核心设计逐项分析

### 4.1 Provider、API 与 Model 抽象

【源码事实】Pi 的 `Provider` 是具体运行单元，包含 `id`、可选显示名、`auth`、模型列表、可选动态模型刷新/过滤，以及 `stream`、`streamSimple` 和可选 deferred 方法；`Api` / `ProviderId` 都保留开放字符串联合，使第三方扩展不会被内置枚举封死。[开放 API/ProviderId](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/types.ts#L17-L80) [Provider 类型](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/models.ts#L88-L149)

【源码事实】`Model` 绑定 `id/name/api/provider/baseUrl`，并携带 reasoning、输入模态、token 单价、context window、最大输出 token、采样参数、headers 与协议兼容配置。模型目录的类型在编译期约束生成数据，再拍平成运行时列表。[Model 类型](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/types.ts#L803-L850) [目录类型](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/model-catalog.ts#L1-L27)

【推断】这是很有价值的“双轴”设计：`provider` 回答“凭证与端点归谁”，`api` 回答“线上报文怎么说”。如果只用一个 provider 枚举承载二者，OpenAI-compatible 网关、同 Provider 多协议、企业代理和自定义 base URL 都会产生分支爆炸。

【建议】Auto Studio 保留三层而不是两层：

```text
ProviderKind     厂商/网关类型，例如 openai、replicate、fal
ProviderConnection  用户持有的 BYOK 连接、区域、base URL、账户作用域
ProtocolFamily   openai-responses、vendor-job-v2、webhook-job、polling-job
```

`ModelCapability` 不能只挂在静态型号上，还必须保存“在这个 Connection 上、通过这个 ProtocolFamily 实测得到”的状态与时间戳；这与本项目共同语言中的 `Provider`、`Credential`、`Model Capability`、`Generation Job` 边界一致，见[CONTEXT](../../CONTEXT.md)。

### 4.2 统一消息与流事件

【源码事实】统一消息是 `UserMessage | AssistantMessage | ToolResultMessage`。Assistant 内容是 `text | thinking | toolCall`；ToolResult 内容是 `text | image`，同时能附加宿主应用自定义 `details`、tool usage、动态新增工具和错误标志。[消息类型](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/types.ts#L350-L380) [ToolResultMessage](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/types.ts#L421-L467)

【源码事实】流事件统一为 `start`，按 content index 区分的 `text_start/delta/end`、`thinking_start/delta/end`、`toolcall_start/delta/end`，以及 `done/error`。文档明确提示不同内容块可以交错，消费者不能只靠事件顺序，必须按 `contentIndex` 聚合。[事件联合](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/types.ts#L527-L551) [交错语义](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/README.md#L652-L671)

【推断】`contentIndex` 是对“厂商流可交错”这一事实的最小稳定语义；它比把每家 SSE chunk 原样上抛更适合 UI 和 Agent。

【建议】Auto Studio 可复用“规范事件 + adapter 私有原始事件”的模式，但至少要拆成两类：

- `AgentDelta`：短生命周期、可丢弃、用于文本/thinking/tool 参数的 UI 增量。
- `RunEvent`：可恢复、顺序稳定、落库，表达 `Submitted`、`ProviderAccepted`、`ProgressObserved`、`UnknownOutcome`、`ArtifactDiscovered`、`DownloadVerified`、`AssetCommitted`、`CancelRequested/Observed`。

不要用一个 `done/error` 事件同时承担 UI 流结束和外部 Generation Job 的事实状态。

### 4.3 Tool call 与 Agent 执行

【源码事实】Tool 以 TypeBox schema 描述参数；tool call 的流式参数允许是 partial JSON，但官方文档要求执行前验证完整参数。Agent 的工具默认可并行，也可以为全局或单个工具切换顺序执行；`beforeToolCall` 可以阻止调用，`afterToolCall` 可以改写结果。[Tool 类型](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/types.ts#L514-L525) [参数与校验](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/README.md#L526-L650) [执行模式与 hooks](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/agent/README.md#L65-L124)

【源码事实】Agent 收到 `length` 结束原因时不会执行可能已截断的 tool call，而是把它们转成失败结果；正式执行前还会再次验证参数并运行 pre-hook。[主循环保护](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/agent/src/agent-loop.ts#L155-L275) [截断处理](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/agent/src/agent-loop.ts#L374-L405) [校验与 pre-hook](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/agent/src/agent-loop.ts#L600-L668)

【推断】“执行前验证 + 可拦截 hook”是很好的 policy seam；但它只是进程内回调，没有持久化审批、授权快照或重启恢复语义。

【建议】Auto Studio 应保留 semantic tool 边界，却把付费/有副作用的调用升级为 durable preflight：绑定 `tool_input_hash + provider_connection + resolved_capability + cost_quote + asset_versions + approval_expiry`。对生成提交、项目写入和有序媒体变换默认串行或依赖图调度，不继承 Pi 的默认并行策略。

### 4.4 Thinking

【源码事实】Pi 使用供应商中立的 thinking level，并可由模型配置映射到不同 API 的具体级别；thinking 作为独立内容块和独立流事件，能保留不透明签名或 redacted 标志。[Thinking 类型与映射](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/types.ts#L82-L110) [ThinkingContent](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/types.ts#L350-L366) [官方用法](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/README.md#L786-L880)

【源码事实】上下文跨模型时，转换器会删除 redacted thinking，移除 thinking/thought signature，并把非空 thinking 降级成普通文本；同模型时才保留签名。对应测试验证了签名删除与普通文本转换。[转换实现](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/api/transform-messages.ts#L76-L156) [转换测试](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/test/transform-messages-copilot-openai-to-anthropic.test.ts#L49-L160)

【源码事实】这里存在一处值得警惕的文档漂移：README 声称跨 Provider 会把 thinking 转成 `<thinking>` 标签文本，而当前实现和测试实际转成**不带标签的普通文本**。本文以固定提交的源码与测试为准。[README 表述](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/README.md#L1297-L1306) [实际实现](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/api/transform-messages.ts#L100-L116)

【建议】借鉴“厂商签名不透明、换模型时剥离”的规则；不要把 chain-of-thought 当项目事实或长期资产。Auto Studio 持久化的是 `Decision`、理由摘要、输入快照和 provenance，而非原始 thinking。若跨模型续跑，默认只传可审计摘要，避免无意泄漏隐藏推理。

### 4.5 Usage 与 cost

【源码事实】Pi 统一记录 input/output/cache read/cache write/reasoning tokens、总 tokens，以及对应 token 成本；模型价格以每百万 token 表示，并支持 tiered pricing 和 1h cache write 价格。[Usage 类型](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/types.ts#L382-L403) [ModelCost](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/types.ts#L803-L820) [成本计算](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/models.ts#L878-L898)

【源码事实】图片生成仍用 token usage 乘模型 token 单价计算；缺失 usage 时可以不返回费用。当前图片 Provider 聚合只注册 OpenRouter Images。[图片 usage 计算](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/api/openrouter-images.ts#L165-L195) [图片 Provider 聚合](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/providers/all.ts#L143-L154)

【推断】Pi 的 cost 是**事后遥测/静态估算**，不是一个提交前、带有效期和适用条件的报价；类型中也没有 approval、currency、tax、minimum charge、按秒/像素/张数计价或“上限金额”。

【建议】Auto Studio 将它拆成：

- `CostEstimate`：区间、币种、计价维度、假设、来源、抓取时间。
- `CostQuote`：若 Provider 支持，含 quote ID、有效期、最大承诺金额。
- `CostActual`：Provider 回执/账单值及置信度。
- `CostApproval`：用户批准的上限、适用输入 hash、连接和截止时间。

任何缺价、零价或过期目录项都不能被解释成“免费”；高于阈值或无法给出上界时必须进入审批。

### 4.6 能力表与模型目录

【源码事实】LLM Model 的公共能力主要是 `reasoning`、`input: text|image`、上下文/输出 token 限制、采样参数、价格和细粒度协议 compat；README 明确该包只支持具备 tool calling 的模型。图片 Model 则另有 `output: text|image`，不含 reasoning/context window/maxTokens。[Model/ImagesModel](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/types.ts#L803-L857) [tool-call 模型范围](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/README.md#L1-L5)

【源码事实】compat 表并不薄：OpenAI Completions、Responses、Anthropic 分别能声明 developer role、reasoning 格式、usage streaming、finish reason、tool result name、strict tools、长缓存等协议差异。[compat 类型](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/types.ts#L553-L708)

【源码事实】模型既可来自静态生成目录，也可由 Provider 动态刷新；刷新允许先恢复缓存、再发网络请求，并用 generation 规则防止旧请求覆盖新结果。持久化抽象保存模型列表、Last-Modified、ETag 与检查时间，但默认实现只是内存存储。[刷新契约](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/models.ts#L39-L76) [刷新实现](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/models.ts#L320-L445) [ModelsStore](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/models-store.ts#L3-L45)

【推断】Pi 的“协议兼容矩阵”比“产品能力矩阵”更丰富。它足以决定怎样向 LLM 发报文，却不足以回答视频/音频生成是否异步、能否取消、支持哪些容器/采样率/分辨率、是否提供 seed、编辑/mask/reference、多结果、内容权利、数据保留或价格可见性。

【建议】Auto Studio 的 Capability Manifest 至少包含：operation、input/output modalities、sync/async、poll/webhook、cancel semantics、idempotency support、reconcile keys、output formats/limits、seed/reference/mask、latency band、price visibility、rights/data retention/region、最后一次真实探测状态。静态目录仅作候选；“在某连接上验证成功”才是可执行能力。

### 4.7 鉴权与 BYOK

【源码事实】Provider 自己声明 API key 或 OAuth auth；`CredentialStore` 只暴露非秘密的 credential 列表，并把 `modify` 设为唯一写路径以便串行化。默认 store 仅是进程内 Map，生产宿主需要注入自己的持久化实现。[鉴权类型](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/auth/types.ts#L3-L115) [ProviderAuth](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/auth/types.ts#L167-L240) [默认 store](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/auth/credential-store.ts#L4-L67)

【源码事实】解析优先级是“请求显式凭证优先，否则使用归属于该 Provider 的已存凭证”；若已存 OAuth 失败，不会悄悄回退环境变量。OAuth 在五分钟刷新窗口内进入 store 锁，刷新有 15 秒超时。[解析规则](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/auth/resolve.ts#L44-L109) [OAuth 刷新](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/auth/resolve.ts#L119-L177)

【源码事实】README 明确警告：浏览器中显式传 API key 有暴露风险，应使用后端代理。[安全说明](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/README.md#L1374-L1399)

【建议】借鉴 Provider-owned auth 与可注入 store，但 Auto Studio 必须以 OS Credential Vault 为唯一秘密存储；项目文件和 Agent Context 只持有不透明 `connection_id`。同时补齐 Pi 类型未表达的账户/组织/区域 scope、最小权限、撤销状态、审计和多连接选择。Provider 级 credential 不应等同于 Provider Connection。

### 4.8 模型注册与自定义 Provider

【源码事实】`createProvider` 接受静态模型、动态 `fetchModels`、过滤器、auth，以及单个 API 实现或按 `model.api` 索引的实现 Map；未知 API 不抛出到调用栈，而生成 in-band stream error。Provider 可被注入 Models collection。[createProvider](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/models.ts#L739-L862) [未知 API 测试](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/test/providers.test.ts#L501-L511)

【源码事实】官方文档给出本地/keyless、OpenAI-compatible、自定义 wrapper、混合 API 和动态模型刷新示例；直接调用某个 protocol stream 函数会绕过 Provider 的鉴权解析。[自定义 Provider](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/README.md#L994-L1134) [直接协议调用](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/README.md#L1136-L1164)

【建议】Auto Studio 采用相同“factory + instance registry + private protocol implementation”的形状，但不要让 UI、Agent 或插件能直接拿到低层 protocol client；所有真实生成都必须穿过能力校验、费用审批、幂等/Unknown Outcome policy 与 provenance 记录。

### 4.9 协议兼容与上下文转换

【源码事实】Pi 的 `pi-messages` 是一个自有统一线协议：向 `/messages` POST `{model, context, options}`，用 SSE 返回已经归一的事件；测试使用本地 HTTP/SSE fake server 验证请求体、文本/tool 流、响应诊断、服务端错误、缺 key 和提前 EOF。[协议说明/类型](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/api/pi-messages.ts#L1-L83) [事件转换](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/api/pi-messages.ts#L154-L260) [协议测试](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/test/pi-messages.test.ts#L27-L235)

【源码事实】上下文转换会对不支持视觉的模型把图片换成占位文本、修正无效/过长 tool ID、处理 null content；跨模型还会剥离厂商签名。它也为没有对应 tool result 的 tool call 合成错误结果，并在回放时跳过 `error/aborted` assistant message。[基本归一](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/api/transform-messages.ts#L12-L74) [跨模型与孤儿调用](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/api/transform-messages.ts#L76-L220)

【推断】Pi 选择“尽量把上一轮变成下一家能接受的上下文”，而不是保证逐字逐字段可逆。对会话续接合理；对项目事实、媒体 provenance 或计费记录则不可接受。

【建议】Auto Studio 需要分别定义：

- **lossy LLM context projection**：允许裁剪、摘要、图片降级，并记录转换诊断。
- **lossless domain record**：Job、Approval、AssetVersion、provider request/response metadata 与哈希不可被上下文转换覆盖。

原始大媒体只通过 AssetRef/临时签名 URL/受控上传句柄进入 adapter，不嵌入 Agent 消息 JSON。

### 4.10 错误、取消与重试

【源码事实】`StreamFunction` 约定 provider failure 通过流内 `error` 事件和最终 `AssistantMessage` 返回，不把常规请求失败抛给调用者；结束原因包含 `error`、`aborted`、`deferred`。取消使用 `AbortSignal`，取消后仍可拿到部分 response 并将其用于续接上下文。[StreamFunction 约定](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/types.ts#L324-L336) [StopReason/DeferredHandle](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/types.ts#L405-L419) [取消语义](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/README.md#L921-L978)

【源码事实】请求层默认 `maxRetries = 0`；启用后，基于 header、无状态错误、408/409/429/5xx 重试，遵守 `Retry-After`，否则使用 0.5 秒起、8 秒封顶并带 25% jitter 的指数退避，等待可被 AbortSignal 打断。[Provider retry](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/utils/provider-retry.ts#L1-L125) [重试测试](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/test/provider-retry.test.ts#L11-L80)

【源码事实】另一个 `retryAssistantCall` 工具用字符串模式区分 billing/quota 与 transient/network，但源码明确称它是 heuristic，policy 仍由调用者决定；AbortError 永不重试。[错误分类与策略](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/utils/retry.ts#L7-L104) [caller-owned policy](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/utils/retry.ts#L145-L228)

【源码事实】基础 Agent loop 本身不会自动重试 provider error/abort：收到这两类最终消息后，它发出 turn/agent end 并退出；`continue()` 或其他上层显式策略才能重启调用。[Agent 终止分支](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/agent/src/agent-loop.ts#L192-L200) [continue 入口](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/agent/src/agent-loop.ts#L56-L75)

【源码事实】`ModelsError` 为模型目录、Provider、stream、auth、OAuth 提供稳定 code；但流结果的公共错误面主要仍是 `errorMessage` 与可选、非结构化 diagnostics。[ModelsError](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/auth/resolve.ts#L16-L34) [diagnostics](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/utils/diagnostics.ts#L1-L45)

【推断】这些语义适合“一个可重放的 LLM completion”，却没有表达 `accepted-but-response-lost`、厂商 request ID 可对账性、幂等 key、已计费但结果未知、跨重启 reconcile，因而不能单独保证外部付费生成安全。

【建议】Auto Studio 按操作语义制定重试矩阵：

| 操作 | 自动重试 | 失败后状态 |
|---|---|---|
| validate / model list / capability probe | 可，短暂错误有界退避 | `ProbeFailed`，不影响既有已验证能力但标记陈旧 |
| estimate / quote | 仅幂等读；过期必须重新审批 | `EstimateUnavailable` / `QuoteExpired` |
| submit generation | 仅在确认“请求未离开本机”或 Provider 接受同一幂等 key 时 | 任何接受结果不明均为 `UnknownOutcome`，禁止盲目重提 |
| observe / reconcile | 可，有界重试并保存最后成功游标/请求 ID | 仍保持原 Job 状态，追加观察失败事实 |
| download | 可续传/重试，但最终必须 hash、格式与大小校验 | 未验证内容不能进入 AssetVersion |
| cancel | 最佳努力；记录 requested/accepted/rejected/unsupported/unknown | 取消 API 成功也不等于作业已停止或未计费 |

### 4.11 Deferred 不是完整的外部 Job

【源码事实】Pi 已定义 `deferred` 请求选项、`DeferredHandle`、`stopReason: deferred`，ProviderStreams/Models 也有可选 `fetchDeferred` 与 `cancelDeferred` seam。[deferred 类型](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/types.ts#L313-L320) [句柄与能力](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/types.ts#L405-L419) [ProviderStreams](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/types.ts#L225-L281)

【源码事实】在该固定快照的 Provider/API 源码中，能找到的 `fetchDeferred/cancelDeferred` 具象 submit/poll/cancel 实现位于确定性的 `faux` 测试 Provider；lazy API 只负责按声明暴露可选能力并在底层缺实现时报错。测试覆盖的是 scripted pending/ready/cancel/error 流程和鉴权选项传递，而不是某家真实外部生成服务的合约。[faux deferred 实现](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/providers/faux.ts#L524-L660) [lazy capability](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/api/lazy.ts#L63-L97) [deferred 测试](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/test/providers.test.ts#L574-L628) [鉴权传递测试](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/test/providers.test.ts#L426-L499)

【推断】这是一个值得借鉴的**能力 seam**，不是已经被真实厂商验证过的 external-job 子系统。句柄只有 provider/model/api/id/过期与轮询提示；没有 project、connection、idempotency key、submission evidence、成本审批、webhook、artifact 清单、reconcile history 或 Unknown Outcome。

【建议】Auto Studio 不应把 `DeferredHandle` 直接扩展成万能 Job。应以本项目 `GenerationJob` 为聚合根，持久化提交意图、审批和状态事实；Provider handle 只是其中一个可选字段。

### 4.12 图片与媒体资产边界

【源码事实】图片生成与聊天模型分成独立 `ImagesProvider/ImagesModels`，接口是一发 `generate`，错误也作为 `AssistantImages` 返回而非 reject；当前输入/输出只含 text/image。[Images Provider/collection](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/images-models.ts#L8-L88) [ImagesModel 类型](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/types.ts#L469-L488) [图片 API 文档](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/README.md#L709-L784)

【源码事实】OpenRouter Images adapter 把输入图片编码成 data URL，请求成功后只接受 data URL 图片并抽出 base64 到内存。源码没有音频、视频、分块下载、校验和、临时文件、原子资产提交或 provenance commit。[请求构造](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/api/openrouter-images.ts#L136-L163) [结果抽取](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/api/openrouter-images.ts#L83-L115)

【建议】只借鉴“聊天与图片模型不要硬塞一个接口”。Auto Studio 还应进一步按 capability/operation 分离，同时统一产出 `ArtifactDescriptor`；下载必须走 staging、流式 hash/大小/格式验证、`fsync`、原子 rename、SQLite commit，最终成为 immutable AssetVersion。Agent Context 只持 AssetRef，不持 base64、PCM 或视频字节，参见[技术设计的 Provider 与 exactly-once 边界](../../docs/design/auto-studio-technical-design.md)及[Asset 共同语言](../../CONTEXT.md)。

## 5. 可借鉴与不可直接照搬

| Pi 设计 | Auto Studio 决策 | 理由 |
|---|---|---|
| Provider 身份/auth/catalog 与 API protocol 分开 | **借鉴** | 直接支持 BYOK、多网关与同厂商多协议；再增加 Connection 层。 |
| 实例级 Models registry、Provider factory、懒加载 SDK | **借鉴形状** | 隔离项目/连接并降低耦合；用 Rust trait/factory 实现，不引入 TS 运行时。 |
| 统一 message/content/stream event | **部分借鉴** | 用于 Agent LLM 流；外部 Job 必须有独立 durable event/state。 |
| thinking 签名不透明，换模型剥离 | **借鉴并收紧** | 持久化 Decision 摘要，不持久化或跨 Provider 暴露原始 CoT。 |
| TypeBox tool schema、执行前校验、pre/post hook | **借鉴 seam** | Schema 校验与 policy hook 有用；费用审批必须持久化并绑定输入。 |
| 默认并行 tool execution | **不直接照搬** | 付费 submit、项目写入与依赖媒体操作不应无条件并行。 |
| token usage/cost | **仅作遥测参考** | 不能替代多媒体报价、预算上限、币种、有效期与费用审批。 |
| 静态 + 动态模型目录、ETag/缓存、旧刷新不覆盖新结果 | **借鉴** | 适合 capability discovery；真正可用性仍须按 Connection 探测。 |
| Provider-owned auth、可注入 CredentialStore | **借鉴接口，不借默认 store** | 凭证落 OS Vault；项目只保存 connection ID。 |
| in-band stream error + AbortSignal + partial response | **借鉴 LLM 层** | UI 流好处理；不能表达已受理但回执丢失、已计费未知等 Job 事实。 |
| HTTP 重试 helper | **按操作采用** | GET/probe/observe 可用；chargeable submit 不得仅按 HTTP 状态盲重试。 |
| deferred handle/fetch/cancel | **只借 seam** | 当前真实 Provider 未证明；缺少 Unknown Outcome、幂等、reconcile 和 durable state。 |
| base64 图片上下文/结果 | **不照搬到媒体核心** | 大图片、音频、视频必须资产化、流式下载和完整性校验。 |
| 跨 Provider 的 lossy context transform | **仅用于 LLM 投影** | 不能改写 Project/Job/Approval/Asset/provenance 的事实记录。 |
| faux Provider + 协议 fake server + live matrix | **强烈借鉴** | 可把规范、转换、故障和真实契约分层验证。 |

## 6. 建议的 Auto Studio Adapter 轮廓

【建议】不要强求聊天模型和外部媒体生成共享一个“万能 Provider trait”。共享的是 identity/auth/capability/error/telemetry 基础设施，调用契约应分开：

```rust
trait AgentModelAdapter {
    async fn stream_turn(
        &self,
        connection: ConnectionRef,
        model: ModelRef,
        context: LlmContextProjection,
        signal: CancellationToken,
    ) -> Result<AgentDeltaStream, AgentModelError>;
}

trait GenerationProviderAdapter {
    async fn validate_connection(&self, connection: ConnectionRef) -> ConnectionValidation;
    async fn probe_capabilities(&self, connection: ConnectionRef) -> CapabilitySnapshot;
    async fn estimate(&self, request: GenerationIntent) -> CostEstimate;
    async fn submit(&self, approved: ApprovedGenerationIntent) -> SubmitObservation;
    async fn observe(&self, job: ProviderJobRef) -> JobObservation;
    async fn cancel(&self, job: ProviderJobRef) -> CancelObservation;
    async fn reconcile(&self, evidence: SubmissionEvidence) -> ReconcileObservation;
    async fn download(&self, artifact: RemoteArtifactRef, sink: VerifiedStagingSink)
        -> VerifiedArtifact;
}
```

关键不变量：

1. `submit` 返回的是**观察结果**而非简单成功/失败；超时若无法证明未受理，结果必须可表达 `UnknownOutcome`。
2. Adapter 生成并持久化 provider request ID、idempotency key、响应摘要、时间戳和 connection/model/capability snapshot；厂商原始类型不得越过 adapter。
3. `download` 不直接返回 `Vec<u8>` 或 base64；只能写入受控 staging sink，经验证后由 Core 提交 AssetVersion。
4. Agent 不能凭自然语言自行绕过 cost approval；批准对象是规范化并 hash 后的具体 GenerationIntent。
5. 取消是观察事实，不是回滚；任何已产生的远端 artifact 和实际费用仍要 reconcile。

## 7. 测试策略

【源码事实】Pi 同时使用多层测试：

- `faux` Provider 能脚本化 text/thinking/tool/usage/streaming/deferred，适合完全确定性的 Agent/Provider 测试。[faux 能力](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/src/providers/faux.ts#L40-L154) [官方 faux 文档](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/README.md#L1210-L1295)
- 协议 adapter 用本地 HTTP/SSE server 检查请求体与错误边界，避免依赖真实网络。[pi-messages 测试](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/test/pi-messages.test.ts#L27-L235)
- 通用 live suite 覆盖文本、tool、stream、thinking、图片与多轮，但按凭证 `skipIf`；无 key 的测试是 SKIP，不是 Provider 已通过。[live suite](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/test/stream.test.ts#L21-L347)
- 跨 Provider handoff 有单独模型对组合测试，验证上下文转换，而不是只测单家 happy path。[handoff matrix](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/test/cross-provider-handoff.test.ts#L1-L153)
- 生成目录另测模型 ID/API 分组、schema、输入哈希与生成时间戳，防止静态数据漂移。[模型数据验证](https://github.com/badlogic/pi-mono/blob/5cd93f688aaab89dbb6dfa4aca535f21796ae185/packages/ai/test/model-data-validation.test.ts#L80-L176)

【建议】Auto Studio 延续这套分层，并补齐媒体/作业特有的故障注入：

1. **纯契约测试**：每个 adapter 的规范化请求、capability、error taxonomy、cost 与 provenance mapping。
2. **recorded fixture 测试**：厂商成功、限流、鉴权失效、schema drift、内容审核、部分 artifact、取消不支持。
3. **状态机 property tests**：任何事件序列都不能从 Unknown Outcome 自动回到“未提交”；AssetVersion 只能由 verified staging commit 产生。
4. **崩溃点测试**：提交前、请求写出后、响应读取前、Job ID 落库前、下载中、rename 前、SQLite commit 前逐点终止并恢复。
5. **真实合约 smoke**：按 Provider/Connection/Model/Capability 报告 PASS/FAIL/SKIP，设置费用预算和人工开关；缺 key 或缺前置资源必须是 SKIP。
6. **对账测试**：模拟回执丢失、重复 webhook、轮询乱序、Provider Job 消失、取消后完成，验证 reconcile 与去重。
7. **大媒体测试**：流式传输、断点续传、hash mismatch、磁盘不足、格式探测失败、超限与恶意内容，禁止整段 base64 进入 Agent transcript。

## 8. 最终判断

【推断】Pi 的 Provider 适配层在“多家 LLM 的协议差异被压缩为统一消息、流、工具和用量”这件事上设计扎实；其细粒度 compat、Provider/API 分离、实例级 registry、上下文转换、faux Provider 和跨 Provider 测试都值得作为 Auto Studio 的参考样本。

【建议】Auto Studio 的落地边界应是：

- **Agent LLM adapter** 可以高度借鉴 Pi 的抽象；
- **BYOK 连接管理**借鉴 Provider-owned auth，但增加 Connection、OS Vault、scope 和审计；
- **多媒体 Generation Provider**仅借鉴 factory/registry/capability seam，必须独立实现 durable Job、Unknown Outcome、幂等/reconcile、资产提交、费用估算与审批；
- **不要**把 Pi 的 `AssistantMessage`、`DeferredHandle`、`AbortSignal` 或 base64 `ImageContent` 当作上述领域模型的替代品。

因此，建议把 Pi 定位成“LLM adapter 参考实现与测试样板”，而不是 Auto Studio Provider Adapter 的可直接移植内核。
