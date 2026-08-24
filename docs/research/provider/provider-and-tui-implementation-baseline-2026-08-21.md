# Provider 与 Grok Build TUI 实施基线（2026-08-21）

> 文档状态：研究快照。旧的 Desktop-first/TUI 技术预览判断已被 ADR-0008 取代；当前 Provider/TUI 实现和发布资格以代码、技术设计与统一 Roadmap 为准。  
> 核验日期：2026-08-21  
> 输入清单：[docs/research/provider.txt](../provider.txt)  
> 证据范围：Provider 官方文档、官方源码与本机只读前置条件检查；未使用第三方教程、聚合站或搜索结果摘要作为结论依据。  
> 变更范围：本报告只给出研究与实施基线，不修改 Auto Studio 代码、配置或其他文档，也没有发起任何可能计费的模型请求。

## 1. 结论先行

1. **清单中的 DeepSeek、Kimi Open Platform、Kimi Code 都是 LLM/编码推理服务，不是音乐生成服务。**它们最多能成为 Ship 0 的 Agent Model 候选，不能满足“一个真实 Music Provider”的发布门槛。Auto Studio 已接受的架构也明确要求共享 Provider Core，但分开 LLM Inference Turn 与 Media Generation Job；后者必须有 submit、observe、cancel、reconcile、download、verify 和原子 Asset commit。[本地 ADR：分离 LLM 与媒体任务](../../adr/0006-separate-llm-inference-from-media-generation.md)
2. **DeepSeek 与 Kimi Open Platform 都可按各自的 OpenAI-compatible Chat Completions 合同实现 Adapter，但不能因此假定功能等价。**两者在模型名、JSON/Structured Output、reasoning 字段、错误、用量明细和限流行为上有差异。[DeepSeek 快速开始](https://api-docs.deepseek.com/zh-cn/)；[Kimi Open Platform 概览](https://platform.kimi.com/docs/overview)
3. **Kimi Code 不宜成为 Auto Studio 的默认产品集成入口。**其官方定位是编程订阅、终端/IDE 客户端与编码模型接口；官方页面把产品或团队集成导向 Kimi Open Platform。它可作为开发者手工烟测或兼容性样本，但其订阅/OAuth 凭据不能当作 `MOONSHOT_API_KEY` 使用。[Kimi Code 官方文档](https://www.kimi.com/code/docs/)；[Kimi Open Platform 介绍](https://platform.kimi.com/docs/introduction)
4. **三者都没有在本次核验到的聊天 API 中提供 durable remote cancel/job handle。**可以实现本地 HTTP/SSE abort，但不能据此宣称服务端已取消、不会继续计算或不会计费。中断应落为 `Interrupted` 或 `Unknown Consumption`；请求是否被接受不明时不得自动重放。[DeepSeek Chat API](https://api-docs.deepseek.com/zh-cn/api/create-chat-completion/)；[Kimi Chat API](https://platform.kimi.com/docs/api/chat)；[Auto Studio ADR](../../adr/0006-separate-llm-inference-from-media-generation.md)
5. **当前没有任何 Provider 获得真实调用 PASS。**本机未发现进程环境中的 `DEEPSEEK_API_KEY` 或 `MOONSHOT_API_KEY`；虽存在 Kimi Code CLI/OAuth 元数据和本地 Provider 配置痕迹，但凭据有效性、余额、模型权限和计费授权均未验证。本报告只确认“合同可实现”与“具备/缺少测试前置条件”，没有调用收费 API。
6. **“Grok Build TUI”有明确官方产品与开源实现。**它是 xAI 的终端编码 Agent，命令为 `grok`，支持交互式 TUI、headless 和 ACP；这与 Grok 网页/移动端的 **Build Mode** 不是同一产品，也不能与模型名 `grok-build-*` 混为一谈。[Grok Build 官方仓库（固定提交）](https://github.com/xai-org/grok-build/tree/19d42e35c07a9c9244f03f6df0c4c353f970d4f9)；[xAI Grok Build Mode 公告](https://x.ai/news/grok-build-mode)
7. **原研究对 TUI 的产品判断已经过期。**状态可见性、焦点纪律和 reducer/effect 分离仍值得借鉴；ADR-0008 已把 `autostudio` TUI 设为 Ship 0 主 Client，Tauri Desktop 改为开发界面。Grok Build 的 transcript、持续状态、阻塞式审批卡、任务优先级、unknown cost 显示和响应式降级，可映射到 Candidate、Timeline 与 Run Inspector。[Auto Studio 产品布局](../../product/ai-creative-agent-product-design.md)；[Grok Build Pager 架构](https://github.com/xai-org/grok-build/blob/19d42e35c07a9c9244f03f6df0c4c353f970d4f9/crates/codegen/xai-grok-pager/README.md#L1-L36)

## 2. 证据与状态语言

本报告用下列标签约束结论强度：

| 标签 | 含义 |
|---|---|
| `【官方事实】` | 官方产品/API 文档直接声明，可能随在线文档更新 |
| `【源码事实】` | 官方 GitHub 仓库固定 commit 的类型、实现或测试所证明的行为 |
| `【本机观察】` | 2026-08-21 对本机环境做的只读检查；不证明远端凭据有效 |
| `【推断】` | 从官方事实与 Auto Studio 约束推导，必须保留不确定性 |
| `【建议】` | 面向 Auto Studio 的设计或实施选择，不冒充 Provider 承诺 |
| `CONTRACT` | 足以编写接口、解析器、Fake 和离线合同测试 |
| `LIVE-PENDING` | 需要有效 key、权限、预算和真实端点验证，不能记为 PASS |
| `BLOCKED/SKIP` | 缺少必要前置条件；SKIP 不是 PASS |
| `UNPROVEN` | 官方资料没有承诺，或仅有客户端实现证据而没有服务端保证 |

“官方文档写了支持”只形成静态能力声明；真正授权调用前仍需按 Connection + exact Model + region + Adapter version 生成 Capability Snapshot。Auto Studio 的现有技术设计也要求把静态声明、账号探测与 live 验证分开，过期快照不得授权高费用或能力敏感调用。[Provider Core 设计](../../design/auto-studio-technical-design.md)

## 3. Provider 总览

### 3.1 当前 API 与能力矩阵

| 清单项 | 当前官方入口与模型 | 鉴权及环境变量 | OpenAI-compatible | 流式 | Tool call | Structured Output | Usage | Cancel | 本次真实测试结论 |
|---|---|---|---|---|---|---|---|---|---|
| DeepSeek | `https://api.deepseek.com`；`POST /chat/completions`；当前文档列出 `deepseek-v4-flash`、`deepseek-v4-pro`，旧 `deepseek-chat`/`deepseek-reasoner` 别名已标为弃用 | Bearer；官方示例为 `DEEPSEEK_API_KEY` | 是；另有 `https://api.deepseek.com/anthropic` | 是，SSE，`[DONE]`；可请求最终 usage chunk | 是；最多 128 个 tools；`strict` schema 为 Beta | `json_object`；未文档化 `json_schema` | 是；含 prompt/completion/total、缓存命中/未命中和 completion details | 仅可中止本地连接；未见 remote cancel endpoint | `CONTRACT`；`LIVE-PENDING`，未授权调用 |
| Kimi Open Platform | `https://api.moonshot.cn/v1`；`POST /chat/completions`；当前推荐 `kimi-k3`，另列 `kimi-k2.7-code(-highspeed)`、`kimi-k2.6` | Bearer；`MOONSHOT_API_KEY` | 是 | 是，SSE；可请求最终 usage chunk | 是 | `json_object` 与 `json_schema` | 是；含 prompt/completion/total/cached tokens | 仅客户端中断；API 概览未列聊天取消端点 | `CONTRACT`；缺 `MOONSHOT_API_KEY`，`BLOCKED/SKIP` |
| Kimi Code | OpenAI-compatible：`https://api.kimi.com/coding/v1/chat/completions`；Anthropic-compatible：`https://api.kimi.com/coding/v1/messages`（base 为 `.../coding/`）；当前文档列 `k3`、`k3-256k`、`kimi-for-coding(-highspeed)` | CLI 日常用 `/login` OAuth；第三方 API 使用 Kimi Code console key。CLI 不自动读 shell 的普通 `KIMI_API_KEY`；临时测试通道是 `KIMI_MODEL_NAME` + `KIMI_MODEL_API_KEY` | 是；还提供 Anthropic-compatible | 官方客户端默认流式 | 官方客户端支持并解析 tool call | 客户端类型/实现会转发 `json_object`/`json_schema`，但产品端点的服务级保证未在本次官方页面中找到，故 `UNPROVEN` | 官方客户端统一解析 usage/cached tokens | 源码接收 `AbortSignal`；未见 durable remote cancel/job API | CLI/OAuth 使手工烟测“可能可做”，但须明确授权；Auto Studio direct key 仍 `LIVE-PENDING` |

矩阵证据：

- DeepSeek：[快速开始与当前模型](https://api-docs.deepseek.com/zh-cn/)、[Chat Completion API](https://api-docs.deepseek.com/zh-cn/api/create-chat-completion/)、[JSON Output](https://api-docs.deepseek.com/guides/json_mode/)、[Tool Calls](https://api-docs.deepseek.com/guides/tool_calls)。
- Kimi Open Platform：[概览与 OpenAI SDK 示例](https://platform.kimi.com/docs/overview)、[API 概览](https://platform.kimi.com/docs/api/overview)、[Chat API](https://platform.kimi.com/docs/api/chat)、[Tool Use](https://platform.kimi.com/docs/api/tool-use)、[JSON Mode](https://platform.kimi.com/docs/guide/use-json-mode-feature-of-kimi-api)。
- Kimi Code：[产品文档](https://www.kimi.com/code/docs/)、[模型页](https://www.kimi.com/code/docs/kimi-code/models.html)、[环境变量（固定源码）](https://github.com/MoonshotAI/kimi-code/blob/d4e0ad4b2d04d676b6d139ee320ea162289d3f4b/docs/en/configuration/env-vars.md#L1-L125)、[Provider 配置（固定源码）](https://github.com/MoonshotAI/kimi-code/blob/d4e0ad4b2d04d676b6d139ee320ea162289d3f4b/docs/en/configuration/providers.md#L1-L58)。

### 3.2 重要的“兼容”边界

`【官方事实】` DeepSeek 与 Kimi Open Platform 都允许用 OpenAI SDK 指向各自 base URL；Kimi Code 还暴露 OpenAI-compatible 和 Anthropic-compatible 两条路径。这里的“compatible”只证明可复用一部分 wire shape/SDK 调用方式，不证明全部参数、事件顺序、错误语义、reasoning、tool、usage 或取消语义一致。[DeepSeek 快速开始](https://api-docs.deepseek.com/zh-cn/)；[Kimi 概览](https://platform.kimi.com/docs/overview)；[Kimi Code 模型页](https://www.kimi.com/code/docs/kimi-code/models.html)

`【建议】` 保持三个正交概念：

```text
Provider   = 品牌、账号、区域、认证与服务状态
Protocol   = OpenAI Chat Completions / Anthropic Messages 等 wire contract
Model      = 精确模型 ID、上下文、输入输出、工具、价格与限制
```

只有第二个真实 Provider 通过同一套协议合同测试后，才从具体 Adapter 抽取共享 protocol crate；这与现有技术设计一致。[Adapter 注册与扩展](../../design/auto-studio-technical-design.md)

## 4. DeepSeek 实施基线

### 4.1 官方合同

- `【官方事实】` OpenAI-compatible base URL 为 `https://api.deepseek.com`，Anthropic-compatible base URL 为 `https://api.deepseek.com/anthropic`；Chat Completions 使用 `POST /chat/completions`，Bearer key 示例变量为 `DEEPSEEK_API_KEY`。[DeepSeek 快速开始](https://api-docs.deepseek.com/zh-cn/)
- `【官方事实】` 当前文档将 `deepseek-v4-flash` 与 `deepseek-v4-pro` 作为可选模型，并说明 `deepseek-chat`、`deepseek-reasoner` 别名自 2026-07-24 起弃用。不能在代码中把旧别名当作长期稳定模型身份。[DeepSeek 快速开始](https://api-docs.deepseek.com/zh-cn/)
- `【官方事实】` `stream=true` 返回 SSE；流结束含 `[DONE]`。`stream_options.include_usage=true` 会在终止前增加一个 usage chunk，其他 chunk 的 usage 为 `null`。解析器必须接受注释/keep-alive 行，而不能假定每行都是 JSON。[Chat API](https://api-docs.deepseek.com/zh-cn/api/create-chat-completion/)；[限流与长连接说明](https://api-docs.deepseek.com/zh-cn/quick_start/rate_limit)
- `【官方事实】` 支持 function/tool calls 与 `tool_choice`，tools 数量上限 128；模型返回的参数需要宿主按 schema 校验。`strict: true` tool schema 仍标为 Beta，不能把它当成所有模型/账号的稳定默认。[Chat API](https://api-docs.deepseek.com/zh-cn/api/create-chat-completion/)；[Tool Calls 指南](https://api-docs.deepseek.com/guides/tool_calls)
- `【官方事实】` `response_format` 文档化的是 `text` 与 `json_object`，没有 `json_schema`。JSON Output 要求 prompt 明确包含 JSON 意图，官方还提示偶发空 content，故“HTTP 200 + 空串”不能直接当成有效结构化结果。[Chat API](https://api-docs.deepseek.com/zh-cn/api/create-chat-completion/)；[JSON Output 指南](https://api-docs.deepseek.com/guides/json_mode/)
- `【官方事实】` usage 包含 prompt、completion、total tokens，并细分 prompt cache hit/miss 与 completion token details。finish reason 包括 `stop`、`length`、`content_filter`、`tool_calls`、`insufficient_system_resource`。[Chat API](https://api-docs.deepseek.com/zh-cn/api/create-chat-completion/)
- `【官方事实】` 官方错误表覆盖 400、401、402、422、429、500、503；500/503 是服务端错误/过载候选，429 需节流。是否重试还必须结合“请求是否已被接受”和是否已有部分/tool 输出。[错误码](https://api-docs.deepseek.com/zh-cn/quick_start/error_codes/)；[限流说明](https://api-docs.deepseek.com/zh-cn/quick_start/rate_limit)

### 4.2 Auto Studio 适配结论

`【建议】` DeepSeek Adapter 可以立即实现以下合同：认证头、Chat Completions 请求、SSE 解析、text/tool/reasoning 元数据、usage、finish reason、错误归类与本地 abort。Structured Output 能力只声明 `json_object`；需要严格 schema 时，由 Auto Studio 在完整输出后本地校验，并把不匹配视为结构化结果失败，而不是“模型一定遵守 schema”。

`【推断】` 关闭/中断 SSE 只证明客户端不再读取。官方聊天合同没有 durable request/job id 或服务端 cancel endpoint，所以 Adapter 必须把消费情况标成 unknown，而不能写 `cancelled=true`、`cost=0` 或安全自动重放。

`【本机观察】` 当前进程环境没有 `DEEPSEEK_API_KEY`。本机配置中有 DeepSeek Provider/key 字段痕迹，但本次没有读取/输出 secret，也没有验证 key 有效性、账户余额或模型权限。因此状态是 `CONTRACT + LIVE-PENDING`，不是 PASS。

## 5. Kimi Open Platform 实施基线

### 5.1 官方合同

- `【官方事实】` base URL 为 `https://api.moonshot.cn/v1`，Chat Completions 为 `POST /chat/completions`，Bearer key 的官方示例变量是 `MOONSHOT_API_KEY`；官方明确说明接口格式兼容 OpenAI。[Kimi Open Platform 概览](https://platform.kimi.com/docs/overview)
- `【官方事实】` 当前推荐模型为 `kimi-k3`，文档还列出 `kimi-k2.7-code`、`kimi-k2.7-code-highspeed`、`kimi-k2.6`。模型目录应在运行时/发布前刷新并冻结快照，不能从品牌名推断能力。[Kimi Open Platform 概览](https://platform.kimi.com/docs/overview)
- `【官方事实】` 支持流式 SSE、tool calls、`tool_choice`、JSON mode；Chat API 还文档化 `response_format: json_schema` 的 Structured Output。usage 可包含 prompt/completion/total 与 cached tokens，并可通过 `stream_options.include_usage` 在流末取得。[Kimi Chat API](https://platform.kimi.com/docs/api/chat)；[Tool Use](https://platform.kimi.com/docs/api/tool-use)；[JSON Mode](https://platform.kimi.com/docs/guide/use-json-mode-feature-of-kimi-api)
- `【官方事实】` API 概览列出 Chat Completions、Models、Token Count、Balance 与 Files 的 upload/list/get/delete/content；没有列出聊天请求的 cancel endpoint。Files 的 `DELETE` 不能误解释为模型推理取消。[Kimi API 概览](https://platform.kimi.com/docs/api/overview)
- `【官方事实】` 错误文档覆盖 400、401、403、404、429、500，并警示不同平台/域名的 key 不可混用；官方 benchmark 指南建议流式，并对网络、过载、限流类失败做受控重试、限制并发。[错误说明](https://platform.kimi.com/docs/api/errors)；[Benchmark 最佳实践](https://platform.kimi.com/docs/guide/benchmark-best-practice)

### 5.2 Auto Studio 适配结论

`【建议】` Kimi Open Platform Adapter 可实现与 DeepSeek 相同的 canonical LLM contract，但可额外声明 `json_schema`。仍需按 exact model live 验证 tool + thinking + structured output 的组合，因为“分别支持”不自动证明参数可以任意组合。

`【建议】` `.cn` Open Platform Connection 与 `kimi.com/coding` Kimi Code Connection 必须使用不同 Provider/Connection ID、凭据字段、base URL 白名单和计费说明；禁止静默回退或互换 key。

`【本机观察】` 当前进程环境没有 `MOONSHOT_API_KEY`。已有 Kimi Code CLI/OAuth 元数据不是 Open Platform API key，所以真实调用前置条件缺失，记为 `BLOCKED/SKIP`。

## 6. Kimi Code 实施基线

### 6.1 身份、端点与认证边界

- `【官方事实】` Kimi Code 是面向编程的订阅和客户端体系，提供终端/IDE 使用及编码 API；官方面向产品/团队集成的入口是 Kimi Open Platform。[Kimi Code 文档](https://www.kimi.com/code/docs/)；[Kimi Open Platform 介绍](https://platform.kimi.com/docs/introduction)
- `【官方事实】` OpenAI-compatible base URL 为 `https://api.kimi.com/coding/v1`，Chat Completions 路径为 `/chat/completions`；Anthropic-compatible base URL 为 `https://api.kimi.com/coding/`，Messages 路径为 `/v1/messages`。当前官方模型页列 `k3`、`k3-256k`、`kimi-for-coding` 与 `kimi-for-coding-highspeed`。[Kimi Code 模型页](https://www.kimi.com/code/docs/kimi-code/models.html)
- `【官方事实】` 交互 CLI 可通过 `/login` 使用 OAuth；第三方工具直连 Kimi Code API 时使用 console 创建的 API key。Kimi Code 订阅/API 与 Kimi Open Platform 的 API key、套餐及 endpoint 是不同产品边界。[Kimi Code 文档](https://www.kimi.com/code/docs/)
- `【源码事实】` Kimi Code CLI **不会自动读取 shell 中普通的** `KIMI_API_KEY`、`OPENAI_API_KEY`、`ANTHROPIC_API_KEY`；它们只能写在 `config.toml` 的 Provider 节。唯一显式 shell 凭据通道是同时设置 `KIMI_MODEL_NAME` 与 `KIMI_MODEL_API_KEY` 的临时模型机制。`KIMI_CODE_BASE_URL` 是 OAuth 托管服务地址，不是凭据。[官方仓库 env 文档，commit `d4e0ad4`](https://github.com/MoonshotAI/kimi-code/blob/d4e0ad4b2d04d676b6d139ee320ea162289d3f4b/docs/en/configuration/env-vars.md#L1-L125)

因此，Auto Studio 不应照搬 Kimi Code CLI 的凭据读取规则。其统一规则仍应是：用户在 Provider Connection 中录入 secret，secret 进入 OS Keychain/Vault，Project 只保存 opaque vault reference；环境变量仅用于开发/CI 明确注入。[Auto Studio 凭据边界](../../design/auto-studio-technical-design.md)

### 6.2 客户端源码能证明什么

- `【源码事实】` 官方 `ChatProvider` 类型统一了 message、tool、stream、usage、`AbortSignal` 与 `responseFormat`；后者包含 `json_object`/`json_schema`。[Provider 类型](https://github.com/MoonshotAI/kimi-code/blob/d4e0ad4b2d04d676b6d139ee320ea162289d3f4b/packages/kosong/src/provider.ts#L5-L21)；[生成合同](https://github.com/MoonshotAI/kimi-code/blob/d4e0ad4b2d04d676b6d139ee320ea162289d3f4b/packages/kosong/src/provider.ts#L84-L151)
- `【源码事实】` Kimi Adapter 构造 Chat Completions 请求时转发 tools、`response_format`、stream usage、signal 与 trace ID，并处理认证、base URL 和视频上传。[Kimi Adapter](https://github.com/MoonshotAI/kimi-code/blob/d4e0ad4b2d04d676b6d139ee320ea162289d3f4b/packages/kosong/src/providers/kimi.ts#L408-L585)
- `【源码事实】` 官方通用 OpenAI 解析器读取 prompt/completion/total/cached token usage。[Usage 解析器](https://github.com/MoonshotAI/kimi-code/blob/d4e0ad4b2d04d676b6d139ee320ea162289d3f4b/packages/kosong/src/providers/openai-common.ts#L201-L232)

`【证据边界】` 上述源码证明“官方客户端能发送/接收这些字段”，不能单独证明 Kimi Code 产品 endpoint 对每个模型稳定承诺 `json_schema`。因此 streaming/tool/usage 可列为源码支持且待 live 验证；Structured Output 先置 `UNPROVEN`，不得仅因 TypeScript union 中出现字段就开启生产 capability。

`【本机观察】` 已安装 `kimi` CLI 0.37.2，本机存在 Kimi Code OAuth/Provider 配置元数据。它使“经用户明确授权后做一个 CLI 手工烟测”具备可能性，但不证明可供 Auto Studio 直接使用，也不授权本次花费。Direct Adapter 仍需独立 API key、明确预算和 exact-model live qualification。

## 7. Auto Studio 的统一 LLM 合同

### 7.1 可以立即实现的稳定内核

`【建议】` 在 Provider Core 之下保留 `LlmInferenceAdapter`，不要与 `MediaGenerationAdapter` 合并。LLM 侧的 canonical 输入至少包括：

```text
InferenceRequest
  connection_id
  model_snapshot_id
  messages[]              // visible canonical content only
  tools[]                 // project-domain schema, no raw shell/path/credential
  response_constraint     // none | json_object | json_schema
  max_output_tokens?
  sampling?
  abort_signal
  continuity_handle?      // opaque, same compatible path only
```

canonical stream 至少包括：

```text
Started
TextDelta
ToolCallDelta
ToolCallReady             // assembled and schema-valid only
ReasoningMetadata         // provider-allowed summary/metadata, never private CoT fact
UsageUpdate { known, prompt, completion, cached?, provider_fields? }
ProviderMetadata          // redacted allowlist only
Completed { finish_reason, usage, continuity? }
Failed { category, retryability, partial, accepted_state }
```

这与现有设计的 `Started/TextDelta/ToolCallDelta/ToolCallReady/UsageUpdate/ProviderMetadata/Completed/Failed` 基线一致。[Canonical stream event](../../design/auto-studio-technical-design.md)

### 7.2 Provider 差异必须留在 Adapter

| 差异 | 统一方式 | 禁止做法 |
|---|---|---|
| endpoint / auth / region | Connection manifest + Vault reference + base URL allowlist | 把 `.cn`、`.ai`、`.com/coding` 自动互换 |
| model aliases | 保存 Provider 返回的 exact model + frozen catalog snapshot | 将 `deepseek-chat`、`kimi` 等品牌别名当永久模型 ID |
| SSE | 协议解析器接受 comments、blank line、partial JSON、`[DONE]`、末尾 usage | 假设每个 data frame 都有 text 或 usage |
| tool calls | 按 call id/index 聚合参数；完整后 schema + policy + approval | 收到参数 delta 就执行；信任模型 JSON |
| JSON/Structured Output | capability 分 `json_object` 与 `json_schema`；最终本地校验 | 把 JSON mode 宣称为 schema guarantee |
| reasoning/thinking | 仅保留公开可迁移消息和官方允许的 summary/metadata | 把私有 reasoning 当普通 assistant text 跨 Provider 重放 |
| usage/cost | `known` 明确；token 与货币分开；价格表带时间/来源/version | usage 缺失时填 0；以 UI 估算覆盖 Provider 事实 |
| finish/error | Provider reason 映射 stable category，保留清洗后的 code | 只按 HTTP status 决定安全重试 |
| cancel | abort transport；记录 provider_ack 与 consumption_known | 把连接关闭写成“服务端已取消/零费用” |

### 7.3 Tool call 与上下文转换

`【建议】` Tool call 必须经过固定管线：

```text
delta assembly
→ JSON parse
→ schema validation
→ capability/policy/budget/revision check
→ durable Approval check
→ idempotency or reconciliation rule
→ application command
→ durable ToolResult
→ next inference turn
```

跨 Provider 只转换可见 message、已经完成的 Tool Call/Tool Result、Brief/Timeline/Asset 等项目事实；Provider 私有 thinking 与未完成的增量 tool arguments 不进入新 Provider 上下文。完整媒体、二进制内容和 vendor request JSON 不直接塞入 prompt。[Context Snapshot 与 Tool 管线](../../design/auto-studio-technical-design.md)；[LLM/媒体 ADR](../../adr/0006-separate-llm-inference-from-media-generation.md)

### 7.4 错误、取消与重试

`【建议】` Adapter 至少归一化：`InvalidRequest`、`Authentication`、`Permission`、`InsufficientBalance`、`RateLimited`、`ContentPolicy`、`UnsupportedCapability`、`ProviderOverloaded`、`Transport`、`Timeout`、`Interrupted`、`UnknownConsumption`、`InternalAdapter`。

自动重试只允许在以下条件同时成立时发生：

1. 错误类别允许重试；
2. 能确认 Provider 未接受请求，或 Provider 提供幂等键/durable handle；
3. 没有已交付给应用的 tool side effect；
4. 没有已向用户展示为可接受结果的 partial output；
5. retry budget、rate-limit/backoff 和用户费用上限仍允许。

否则进入 `Interrupted` 或 `UnknownConsumption`，提示用户检查用量/对账后再决定。此规则故意比普通 SDK 的“网络错就重试”更严格，因为 Auto Studio 有 BYOK 与费用审批边界。[LLM 中断规则](../../adr/0006-separate-llm-inference-from-media-generation.md)

## 8. 真实测试可行性与测试策略

### 8.1 本机前置条件审计

| Provider | 本机可见前置条件 | 能否立即做不计费测试 | 能否立即做真实模型测试 | 判定 |
|---|---|---|---|---|
| DeepSeek | 进程环境无 `DEEPSEEK_API_KEY`；本地配置有凭据字段痕迹，但未验证 secret | 可以：请求/响应 fixture、SSE parser、Fake、schema、redaction | 不应；缺少明确授权、有效性/余额/权限证明 | `CONTRACT`, `LIVE-PENDING` |
| Kimi Open Platform | 进程环境无 `MOONSHOT_API_KEY`；Kimi Code OAuth 不能替代 | 可以：OpenAI-compatible fixture、JSON schema、tool/usage parser | 不能；缺 Open Platform key | `BLOCKED/SKIP` |
| Kimi Code | CLI 0.37.2；存在 OAuth/Provider 元数据 | 可以：官方源码类型/fixture、CLI 本地配置解析 | 可能，但只在用户明确授权订阅消耗后做；Direct Adapter key 仍缺验证 | `LIVE-PENDING` |

`【本机观察】` 检查过程中没有输出 secret 内容，也没有调用余额、模型或生成接口。这里的“可能可做”只是前置条件判断，不是权限推断。

### 8.2 分层测试

1. **离线类型/合同测试（现在可做）**
   - request mapping：role/content parts、tool schema、response format、headers、model id；
   - streaming fixture：分片 text、tool arguments、reasoning metadata、keep-alive、空行、`[DONE]`、末尾 usage；
   - malformed stream：截断 UTF-8/JSON、tool id 缺失、重复/乱序 chunk、流结束无 usage；
   - redaction：Authorization、API key、vendor request body 中敏感字段不得进入 event/log；
   - cancellation：本地 abort 后只产生 `Interrupted/UnknownConsumption`，不伪造 provider ack；
   - JSON：`json_object` parse 与本地 schema failure；`json_schema` 只在 capability 为 true 时发送；
   - retry：401/402/403/422/内容策略不重试，429/500/503 受 accepted-state 与预算控制。

2. **Mock HTTP/SSE 集成测试（现在可做）**
   - 每个 Provider 使用独立 golden wire fixture；
   - 模拟连接在 headers 前、headers 后、首 token 后、tool delta 中、usage 前断开；
   - 验证 durable event 与最终 Inference Attempt 状态一致；
   - 验证重新打开 Project 后可解释 partial、usage unknown 与下一步责任人。

3. **低成本真实合同烟测（待 key、授权与预算）**
   - exact base URL + exact model 的最小 text 回答；
   - stream event 顺序、首 token、keep-alive 和终止帧；
   - 单 tool call、多 tool call、参数分片、tool result 二轮；
   - thinking 开/关与上下文续轮；
   - `json_object`；Kimi Open 另测 `json_schema`；Kimi Code 单独验证是否真正支持；
   - usage/cached token 字段与 Provider 控制台对账；
   - 主动 abort 后观察客户端行为，但**不把它当服务端取消/不计费证明**；
   - 一次可控 429/无效 key/不支持参数测试，禁止为造错而消耗大量额度。

4. **发布资格测试（真实 Provider 才可执行）**
   - 固定 prompt/tool/schema/temperature/max token 与预算上限；
   - 保存 capability snapshot、Adapter version、request trace id 和清洗后证据；
   - 失败与 SKIP 单独记录；缺 key、缺余额或缺模型权限绝不记 PASS；
   - 第二个 Provider 通过同一合同后，才评估抽取共享 OpenAI-compatible protocol crate。

## 9. Grok Build TUI：身份与证据边界

### 9.1 被研究的对象

`【官方事实】` 本报告中的 **Grok Build** 指 xAI 官方终端编码 Agent/开源仓库 `xai-org/grok-build`。README 展示 `grok` 命令与 full-screen terminal UI，官方概览同时描述交互 TUI、headless 与 ACP 使用方式。[官方仓库 README，commit `19d42e35`](https://github.com/xai-org/grok-build/blob/19d42e35c07a9c9244f03f6df0c4c353f970d4f9/README.md#L10-L35)；[xAI Build 概览](https://docs.x.ai/build/overview)

不把下列对象混为一谈：

- **Grok Build Mode**：Grok 网页/移动应用中的 app-building 工作流，不是终端 TUI。[xAI Build Mode 公告](https://x.ai/news/grok-build-mode)
- **`grok-build-*`**：模型/版本名可能含 Build，但模型名本身不是 UI 产品。
- **“grok-cli”**：本次只在 xAI 官方范围内核验，没有找到另一个可与 Grok Build 等价、且有独立官方产品保证的 `grok-cli`；因此不对非官方同名项目作推断。

### 9.2 官方 TUI 布局

`【官方事实】` Getting Started 把交互界面概括为两个主要区：上方/主体 scrollback transcript 与底部 prompt。Transcript 包含用户提示、Markdown 回复、可折叠 thinking、tool calls/diffs 和任务列表；工具执行实时流入。[Getting Started](https://github.com/xai-org/grok-build/blob/19d42e35c07a9c9244f03f6df0c4c353f970d4f9/crates/codegen/xai-grok-pager/docs/user-guide/01-getting-started.md#L72-L130)

`【源码事实】` 实际 `AgentView` 的垂直布局还包括状态栏、可选 tasks/catalog/todo/queue/side-question、scrollback、turn status、banner/CTA/followups、voice indicator、prompt、shortcuts 与 status line；布局计算优先保护 prompt 与 scrollback，在终端高度不足时隐藏次要行。[AgentView 布局](https://github.com/xai-org/grok-build/blob/19d42e35c07a9c9244f03f6df0c4c353f970d4f9/crates/codegen/xai-grok-pager/src/views/agent.rs#L100-L179)

可以将其抽象为：

```text
┌ Context / mode / connection / working-set ┐
├ Optional task, queue or blocking card      ┤
├────────────────────────────────────────────┤
│ Scrollback / transcript / tool activity    │  flex: consumes remaining space
│                                            │
├ Turn status: phase · elapsed · tokens      ┤
├ Prompt / composer                          ┤
├ Contextual shortcuts · status line         ┤
└────────────────────────────────────────────┘
```

`【源码事实】` Pager 将 `AppView`、`AgentView`、`PromptWidget` 分层，并以 `Action → dispatch → Effect → state` 组织交互。这一模式使按键/UI intent 与副作用、状态变更分开，适合映射到 Auto Studio“Client Surface 只发 command，不直接改 Provider/Project”的边界。[Pager 架构](https://github.com/xai-org/grok-build/blob/19d42e35c07a9c9244f03f6df0c4c353f970d4f9/crates/codegen/xai-grok-pager/README.md#L1-L36)；[Auto Studio Context](../../../CONTEXT.md)

### 9.3 状态、焦点与任务交互

- `【源码事实】` Turn status 同时展示 spinner/activity、阶段、阶段计时、队列提示、turn 计时、tokens 与 stop/cancel；状态词覆盖 thinking、responding、verifying、compacting、retrying、waiting 和 cancelling，并为长时间运行保留 persistent cue。[Turn status 定义](https://github.com/xai-org/grok-build/blob/19d42e35c07a9c9244f03f6df0c4c353f970d4f9/crates/codegen/xai-grok-pager/src/views/turn_status.rs#L1-L14)；[状态标签](https://github.com/xai-org/grok-build/blob/19d42e35c07a9c9244f03f6df0c4c353f970d4f9/crates/codegen/xai-grok-pager/src/views/turn_status.rs#L655-L744)
- `【官方事实】` Blocking question、permission 与 cancel card 拥有各自的键盘处理和焦点；Tab 可切换焦点；shortcut bar 随上下文变化。Agent 运行中，输入可以排队为 follow-up，或选择 cancel-and-send；前台任务也可转后台。[键盘与焦点](https://github.com/xai-org/grok-build/blob/19d42e35c07a9c9244f03f6df0c4c353f970d4f9/crates/codegen/xai-grok-pager/docs/user-guide/03-keyboard-shortcuts.md#L1-L177)；[运行时输入](https://github.com/xai-org/grok-build/blob/19d42e35c07a9c9244f03f6df0c4c353f970d4f9/crates/codegen/xai-grok-pager/docs/user-guide/03-keyboard-shortcuts.md#L257-L283)
- `【官方事实】` Plan preview 是可滚动、可聚焦的阻塞视图，提供 approve、request changes、comment、copy、quit 等动作。[Plan Mode](https://github.com/xai-org/grok-build/blob/19d42e35c07a9c9244f03f6df0c4c353f970d4f9/crates/codegen/xai-grok-pager/docs/user-guide/19-plan-mode.md#L65-L99)
- `【官方事实】` 后台任务有独立 pane 和持续 still-running 状态，可从主会话向运行中的任务发消息/中断。[Background Tasks](https://github.com/xai-org/grok-build/blob/19d42e35c07a9c9244f03f6df0c4c353f970d4f9/crates/codegen/xai-grok-pager/docs/user-guide/20-background-tasks.md#L176-L205)
- `【官方事实】` Dashboard 按 Needs input、Working、Idle 等状态排列，并同时使用文字与符号；status line 可显示 model、context、cost、cwd/worktree。成本未知时不显示为零。[Dashboard](https://github.com/xai-org/grok-build/blob/19d42e35c07a9c9244f03f6df0c4c353f970d4f9/crates/codegen/xai-grok-pager/docs/user-guide/23-dashboard.md#L27-L72)；[Status line](https://github.com/xai-org/grok-build/blob/19d42e35c07a9c9244f03f6df0c4c353f970d4f9/crates/codegen/xai-grok-pager/docs/user-guide/25-status-line.md#L1-L25)；[未知成本规则](https://github.com/xai-org/grok-build/blob/19d42e35c07a9c9244f03f6df0c4c353f970d4f9/crates/codegen/xai-grok-pager/docs/user-guide/25-status-line.md#L79-L112)

## 10. 映射到 Auto Studio 的 TUI/桌面交互

### 10.1 借鉴项

| Grok Build 模式 | Auto Studio 映射 | 采用理由 |
|---|---|---|
| Transcript/scrollback 是持续记录 | 中部 `Timeline`/Run events，工具、Provider、生成与资产提交事件可展开 | 让长任务的因果链和当前进度同时可见 |
| 底部 prompt 始终可达 | Agent composer 固定；运行中可选择排队 follow-up 或明确“中止本轮并发送” | 避免输入与当前运行语义混淆 |
| Persistent turn status | 状态投影为 `Estimating → AwaitingApproval → Submitting → Accepted → Generating → Downloading → Verifying → Importing`，以及 `UnknownOutcome/ReconciliationRequired` | 媒体生成不能只显示 spinner；必须知道工作、费用、重试安全性与下一责任人 |
| Blocking cards 抢占焦点 | Cost Approval、Rights Declaration、Cancel、Selection 使用独立阻塞卡与明确主动作 | 审批与选择是 durable domain command，不是聊天文本 |
| Dashboard 把 Needs input 放前 | Project/Run 列表按 `UnknownOutcome/ReconciliationRequired/NeedsApproval` 优先，其次 Running、AwaitingSelection、Completed | 高风险与需人工处理任务优先 |
| Contextual shortcut/status bar | 显示 Project、Provider Connection、exact Model、Budget、Rights、Selection/Revision；仅给当前上下文合法动作 | 减少误操作并解释当前约束 |
| unknown cost 不显示 0 | `Cost: unknown · awaiting usage/reconcile` | 防止用户把缺失数据误解为免费 |
| 高度不足先隐藏次要行 | 优先保留 Candidate/Timeline、composer、Approval/Cancel；CTA、辅助说明和次级指标先折叠 | 窄窗口仍能完成关键任务 |
| Action → Effect → State | Tauri view 发 intent/command，Application 执行，Domain event/outbox 成为事实后 UI 重绘 | 符合当前 source-of-truth 与可恢复性设计 |

`【建议】` Ship 0 桌面布局可保留既定三栏语义，而把 Grok Build 的 TUI 规律嵌入其中：

```text
┌ Project / Connection / Model / Budget / Rights ─────────────────────┐
├ Brief & Run list ┬ Candidate Board / Timeline ┬ Run Inspector        ┤
│ Needs attention │ cards + durable events      │ Agent conversation   │
│ Running         │ preview / compare / select  │ plan / tool activity │
│ Awaiting select │                              │ approval / reconcile │
├─────────────────┴──────────────────────────────┴──────────────────────┤
│ Phase · elapsed · usage/cost-known? · provider ack · Cancel/Reconcile│
│ Agent composer: Queue follow-up | Stop turn & send                    │
└───────────────────────────────────────────────────────────────────────┘
```

该映射不改变既定产品边界：Selection 与 Approval 分离，Selection 只能由用户完成；Candidate Board 管候选，Timeline/Run Inspector 管执行证据。[产品设计：Selection 与 Approval](../../product/ai-creative-agent-product-design.md)；[桌面布局](../../product/ai-creative-agent-product-design.md)

### 10.2 不能直接照搬

| Grok Build 行为/概念 | 为什么不能直接照搬 | Auto Studio 必须采用的语义 |
|---|---|---|
| Plan Mode 的工具权限门 | 官方文档明确指出它主要阻止编辑工具，shell 写入与 subagent 可能绕过父级 gate | 所有有副作用 command 都在 Application/Domain 做强制 Policy + durable Approval，不能只靠 UI/提示词 |
| “Stop” 一个 agent turn | LLM 客户端 abort 与外部 Generation Job 的远端取消不是同一件事 | 区分 `Inference Interrupted`、`CancelRequested`、`ProviderCancelConfirmed`、`UnknownOutcome` |
| 自动显示 `Retrying…` | 编码 Agent 的幂等读取/推理重试，不等于可能收费、可能已接受的媒体 submit | 没有幂等键或 durable job handle 时禁止重交；先 reconcile |
| Tool permission card | 文件/命令权限不是成本、版权或资产 provenance 审批 | Cost Approval、Rights Declaration、Selection、Export 各是独立 domain object/event |
| transcript 是主要产品事实 | Agent transcript 可包含推断、临时文本与私有 reasoning | Project DB、Asset、Selection、Approval、Generation Job/Event 才是事实源 |
| token/cost status | 媒体费用可能按次、时长、分辨率、队列或结果计费，且 submit 后状态不明 | 预估、授权上限、已知实耗、unknown/reconcile 分开显示 |
| 后台 task 可“发消息打断” | 外部 Provider job 可能只支持 poll，或根本没有可修改的运行中请求 | 只在 capability 明确支持时显示 remote cancel/update；否则提示本地停止观察或人工对账 |
| TUI 全局快捷键 | 终端键位、IME、屏幕阅读器与桌面快捷键约束不同 | Tauri 使用 native、可见、可重绑定且无冲突的快捷键；所有动作也有可点击入口 |
| 直接采用 TUI runtime | Ship 0 已由 ADR-0008 冻结为 `autostudio` TUI 主 Client | TUI 只复用 Application commands/events，不复制 Core 业务状态机；Desktop 保留开发验证 |

Plan Mode 限制的官方证据见 [Plan Mode 安全边界](https://github.com/xai-org/grok-build/blob/19d42e35c07a9c9244f03f6df0c4c353f970d4f9/crates/codegen/xai-grok-pager/docs/user-guide/19-plan-mode.md#L129-L139)；Grok Build 的 permission mode 与决策顺序见 [Permissions and Safety](https://github.com/xai-org/grok-build/blob/19d42e35c07a9c9244f03f6df0c4c353f970d4f9/crates/codegen/xai-grok-pager/docs/user-guide/22-permissions-and-safety.md#L1-L42) 和 [授权顺序](https://github.com/xai-org/grok-build/blob/19d42e35c07a9c9244f03f6df0c4c353f970d4f9/crates/codegen/xai-grok-pager/docs/user-guide/22-permissions-and-safety.md#L119-L136)。

## 11. 什么现在能做，什么必须等 key/真实 Provider

### 11.1 现在可真实实现并验证

- Provider/Connection/Protocol/Model/Capability Snapshot 的独立 domain types；
- DeepSeek、Kimi Open Platform 的请求构造与 SSE/usage/tool 解析 Adapter；
- Kimi Code 的独立 experimental Adapter contract，但默认关闭未验证 capability；
- canonical message、content part、tool call、finish reason、usage-known 与错误分类；
- OS Vault reference、日志清洗、base URL allowlist；
- Deterministic Fake + 每 Provider 独立 wire fixtures + mock SSE server；
- `json_object` 与本地 JSON Schema 校验；Kimi Open 的 `json_schema` request mapping；
- 本地 AbortSignal 与 `Interrupted/UnknownConsumption` 状态；
- Grok Build 式 Timeline、persistent status、blocking card、focus、queue follow-up 和 unknown-cost UI 投影；
- reducer/command/effect/domain-event 的边界测试；
- 所有 SKIP/UNPROVEN 在 UI 与 qualification report 中显式展示。

### 11.2 只能先做合同，必须等 key 与预算验证

- 真实认证、余额、区域和 exact model 可用性；
- SSE keep-alive、delta 交错、末尾 usage 与断流表现；
- tool call 参数分片、并行调用、thinking + tool 多轮组合；
- DeepSeek `strict` Beta 在目标模型/账号上的实际行为；
- Kimi Open `json_schema` 的约束遵守度；Kimi Code `json_schema` 是否受产品端点支持；
- cached token 与货币成本的真实对账；
- abort 后服务端计算/计费是否继续；
- 429、overload、超时、内容策略等错误在真实账号上的 payload；
- Provider model catalog/price snapshot 刷新与过期策略。

### 11.3 清单内无法实现或没有证据支持

- 任何一个清单 Provider 作为真实音乐生成 Provider；
- durable media job id、submit/poll/callback/download/verify/asset commit；
- remote cancel acknowledgement；
- media generation 的 Unknown Outcome reconciliation；
- 音频候选、变奏、seed/reference、时长、格式、rights/provenance 能力；
- “客户端断开即服务端取消/不计费”的保证；
- “OpenAI-compatible 即所有参数/能力/错误等价”的保证。

这些项不是继续完善 Chat Completions Adapter 就会自然得到的能力，必须另选并核验真实音乐 Provider。[LLM 与 Media 深接口](../../design/auto-studio-technical-design.md)

## 12. 推荐实施顺序与发布 Gate

1. **冻结 LLM canonical contract 与 Fake。**先用官方 wire shape 编写三个独立 fixture，完成 stream/tool/usage/error/abort 合同测试。
2. **不要在没有 key 的情况下拍板首个真实 Agent Model。**若已有合规 DeepSeek key，可先做 DeepSeek；若已有 Kimi Open key且需要官方 `json_schema`，可先做 Kimi Open。选择依据应是用户实际持有的 Connection、区域、预算、模型能力与 live 结果，而不是文档功能数量。
3. **Kimi Code 保持 secondary/experimental。**它适合开发者编码工作流与兼容性研究，不替代 Kimi Open 的产品 API Connection。
4. **每次真实烟测先获明确授权。**固定模型、最大 tokens、最大轮数和货币上限；先查余额/模型，再做最小 text、stream、tool、structured、usage、abort 测试。
5. **生成并冻结 Capability Snapshot。**记录 connection、exact model、observed_at、source、Adapter version、static/account/live 三种证据与过期时间。
6. **第二个 Provider live PASS 后再抽协议 crate。**在此之前允许代码形状相似，但不预设行为等价。
7. **并行启动真实音乐 Provider 调研。**本清单不能关闭 Music Provider Gate；媒体 Adapter 必须单独证明 job、unknown outcome、download/verify/asset commit 与费用审批。
8. **TUI 借鉴只进入桌面交互规范。**不改变 Tauri Ship 0，不让 transcript、临时 plan 或快捷键状态成为 Project 事实源。

发布判定必须保持：缺 key、缺余额、缺模型权限、未取得用户费用授权均为 `SKIP/BLOCKED`；合同测试通过不等于真实 Provider PASS。[Auto Studio 测试与 Gate](../../design/auto-studio-technical-design.md)

## 13. 官方来源索引

### DeepSeek

- [API 文档首页/快速开始](https://api-docs.deepseek.com/zh-cn/)
- [Create Chat Completion](https://api-docs.deepseek.com/zh-cn/api/create-chat-completion/)
- [JSON Output](https://api-docs.deepseek.com/guides/json_mode/)
- [Tool Calls](https://api-docs.deepseek.com/guides/tool_calls)
- [模型与价格](https://api-docs.deepseek.com/zh-cn/quick_start/pricing)
- [错误码](https://api-docs.deepseek.com/zh-cn/quick_start/error_codes/)
- [限流与长连接](https://api-docs.deepseek.com/zh-cn/quick_start/rate_limit)

### Kimi Open Platform

- [平台概览](https://platform.kimi.com/docs/overview)
- [平台介绍与产品边界](https://platform.kimi.com/docs/introduction)
- [API 概览](https://platform.kimi.com/docs/api/overview)
- [Chat API](https://platform.kimi.com/docs/api/chat)
- [Tool Use API](https://platform.kimi.com/docs/api/tool-use)
- [Tool call 指南](https://platform.kimi.com/docs/guide/use-kimi-api-to-complete-tool-calls)
- [JSON Mode 指南](https://platform.kimi.com/docs/guide/use-json-mode-feature-of-kimi-api)
- [错误说明](https://platform.kimi.com/docs/api/errors)
- [Benchmark 最佳实践](https://platform.kimi.com/docs/guide/benchmark-best-practice)

### Kimi Code

- [官方文档](https://www.kimi.com/code/docs/)
- [模型与兼容端点](https://www.kimi.com/code/docs/kimi-code/models.html)
- [官方源码仓库，固定 commit `d4e0ad4`](https://github.com/MoonshotAI/kimi-code/tree/d4e0ad4b2d04d676b6d139ee320ea162289d3f4b)
- [Provider 配置](https://github.com/MoonshotAI/kimi-code/blob/d4e0ad4b2d04d676b6d139ee320ea162289d3f4b/docs/en/configuration/providers.md#L1-L58)
- [环境变量与 OAuth/base URL 边界](https://github.com/MoonshotAI/kimi-code/blob/d4e0ad4b2d04d676b6d139ee320ea162289d3f4b/docs/en/configuration/env-vars.md#L1-L125)
- [ChatProvider 类型](https://github.com/MoonshotAI/kimi-code/blob/d4e0ad4b2d04d676b6d139ee320ea162289d3f4b/packages/kosong/src/provider.ts#L84-L151)
- [Kimi Adapter 实现](https://github.com/MoonshotAI/kimi-code/blob/d4e0ad4b2d04d676b6d139ee320ea162289d3f4b/packages/kosong/src/providers/kimi.ts#L408-L585)
- [OpenAI-compatible usage 解析](https://github.com/MoonshotAI/kimi-code/blob/d4e0ad4b2d04d676b6d139ee320ea162289d3f4b/packages/kosong/src/providers/openai-common.ts#L201-L232)

### Grok Build

- [xAI Build 概览](https://docs.x.ai/build/overview)
- [官方仓库，固定 commit `19d42e35`](https://github.com/xai-org/grok-build/tree/19d42e35c07a9c9244f03f6df0c4c353f970d4f9)
- [README 与 full-screen TUI](https://github.com/xai-org/grok-build/blob/19d42e35c07a9c9244f03f6df0c4c353f970d4f9/README.md#L10-L35)
- [Getting Started：界面与 transcript](https://github.com/xai-org/grok-build/blob/19d42e35c07a9c9244f03f6df0c4c353f970d4f9/crates/codegen/xai-grok-pager/docs/user-guide/01-getting-started.md#L72-L130)
- [Pager 架构](https://github.com/xai-org/grok-build/blob/19d42e35c07a9c9244f03f6df0c4c353f970d4f9/crates/codegen/xai-grok-pager/README.md#L1-L36)
- [AgentView 布局](https://github.com/xai-org/grok-build/blob/19d42e35c07a9c9244f03f6df0c4c353f970d4f9/crates/codegen/xai-grok-pager/src/views/agent.rs#L100-L179)
- [Keyboard shortcuts 与 focus](https://github.com/xai-org/grok-build/blob/19d42e35c07a9c9244f03f6df0c4c353f970d4f9/crates/codegen/xai-grok-pager/docs/user-guide/03-keyboard-shortcuts.md#L1-L177)
- [Plan Mode 与安全边界](https://github.com/xai-org/grok-build/blob/19d42e35c07a9c9244f03f6df0c4c353f970d4f9/crates/codegen/xai-grok-pager/docs/user-guide/19-plan-mode.md#L65-L139)
- [Background Tasks](https://github.com/xai-org/grok-build/blob/19d42e35c07a9c9244f03f6df0c4c353f970d4f9/crates/codegen/xai-grok-pager/docs/user-guide/20-background-tasks.md#L176-L205)
- [Permissions and Safety](https://github.com/xai-org/grok-build/blob/19d42e35c07a9c9244f03f6df0c4c353f970d4f9/crates/codegen/xai-grok-pager/docs/user-guide/22-permissions-and-safety.md#L1-L136)
- [Dashboard](https://github.com/xai-org/grok-build/blob/19d42e35c07a9c9244f03f6df0c4c353f970d4f9/crates/codegen/xai-grok-pager/docs/user-guide/23-dashboard.md#L27-L72)
- [Status line 与 unknown cost](https://github.com/xai-org/grok-build/blob/19d42e35c07a9c9244f03f6df0c4c353f970d4f9/crates/codegen/xai-grok-pager/docs/user-guide/25-status-line.md#L1-L112)
- [Grok Build Mode（不同产品）](https://x.ai/news/grok-build-mode)

### Auto Studio 本地约束

- [Provider 清单](../provider.txt)
- [ADR-0006：分离 LLM 推理与媒体生成](../../adr/0006-separate-llm-inference-from-media-generation.md)
- [技术设计](../../design/auto-studio-technical-design.md)
- [产品设计](../../product/ai-creative-agent-product-design.md)
- [工程上下文](../../../CONTEXT.md)
