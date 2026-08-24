# Provider 模型目录与 OpenCode TUI 交互调研

> 文档状态：实现参考快照。`/connect`、后台 Catalog refresh、`/model` 和 per-model Thinking 已落地；精确行为以当前 Core/TUI 合同为准，live capability 仍需账号级验证。  
> 调研日期：2026-08-22  
> 范围：DeepSeek、OpenAI、Anthropic、Kimi Open Platform、Kimi Code，以及 OpenCode 的 `/connect`、`/models` 交互与状态组织。  
> 证据边界：只使用 Provider 官方 API 文档、官方产品文档和 OpenCode 官方源码。本次没有使用或验证任何真实 API Key，也没有执行计费推理请求。

## 1. 结论

Auto Studio 可以实现用户要求的流程：

```text
autostudio
  -> 在主输入框输入 /
  -> 选择 /connect
  -> 搜索并选择 Provider
  -> 输入 API Key 并保存
  -> Core 在后台刷新该 Connection 的模型目录
  -> 用户输入 /model（兼容别名 /models）
  -> 从已获取的模型中选择当前 Agent Model
```

但“保存 Key 后自动获取模型”不能被实现成一个通用的 `GET {base_url}/models` 猜测。当前官方合同分成三类：

1. **可直接按账号刷新**：OpenAI、Anthropic、DeepSeek、Kimi Open Platform 都有正式模型列表接口。
2. **需要本地能力投影**：OpenAI 和 DeepSeek 的目录只给出很少的模型元数据；即使请求成功，也不能据此断言模型适用于 Auto Studio Agent、支持 Structured Output 或 Tool Calling。
3. **没有当前公开动态目录合同**：Kimi Code 当前产品文档列出了可用 Model ID 和推理端点，但没有把远端 `GET /models` 文档化为第三方稳定 API。它应使用随应用发布并可更新的官方模型快照，不能让首发功能依赖未文档化接口。

因此，Core 应将“凭据已保存”“目录已成功刷新”“当前模型已选择”建模为三个独立事实。Key 保存成功不等于 Provider 已就绪，目录成功也不等于某个模型一定能执行后续推理。

## 2. 官方模型目录对照

| Provider Connection | 官方目录请求 | 鉴权 | 目录信息量 | Auto Studio 实施结论 |
|---|---|---|---|---|
| OpenAI | `GET https://api.openai.com/v1/models` | `Authorization: Bearer <OPENAI_API_KEY>` | `id`、`created`、`owned_by`、可选 `shutdown_date` | 可自动刷新；必须用本地 Agent 兼容性目录过滤和补充能力 |
| Anthropic | `GET https://api.anthropic.com/v1/models` | `x-api-key` 与 `anthropic-version: 2023-06-01` | 分页模型、显示名、时间和能力元数据 | 可自动刷新；必须完整处理分页和未知字段 |
| DeepSeek | `GET https://api.deepseek.com/models` | `Authorization: Bearer <DEEPSEEK_API_KEY>` | `id`、`owned_by` | 可自动刷新；不得继续硬编码已弃用的旧默认名 |
| Kimi Open（国际） | `GET https://api.moonshot.ai/v1/models` | `Authorization: Bearer <MOONSHOT_API_KEY>` | 上下文长度、图片/视频输入和 reasoning 标志 | 可自动刷新；Key 与国际区 endpoint 必须匹配 |
| Kimi Open（中国大陆） | `GET https://api.moonshot.cn/v1/models` | `Authorization: Bearer <MOONSHOT_API_KEY>` | 同上 | 可自动刷新；与国际区 Connection、Key 隔离 |
| Kimi Code | 当前公开文档未承诺第三方模型目录端点 | Kimi Code Console Key；不能与 Kimi Open Key 混用 | 官方当前列出固定 Model ID 与能力说明 | 首发使用官方快照；若未来官方文档化目录接口，再启用远端刷新 |

### 2.1 默认模型语义

上述正式动态目录都没有返回可供 Auto Studio 直接采用的“账号默认 Agent Model”字段：OpenAI、DeepSeek 的列表只有基础身份；Anthropic、Kimi Open 虽然元数据更丰富，也没有把某一项声明为调用方默认值。Kimi Code 官方模型页推荐 `k3-256k`，但“推荐”不是当前 Connection 的默认选择或权限证明。[Kimi Code model configuration](https://www.kimi.com/code/docs/en/kimi-code/models.html)

因此首发规则应是：

- 新 Connection 刷新成功后仍保持 `ModelSelectionRequired`，不自动采用列表第一项；
- Provider 或 Auto Studio 的推荐项只显示 `Recommended` 标记，不替用户作出成本、速度和上下文选择；
- 已有显式选择只有在新快照中仍存在且兼容时才能继续使用；
- 仅可为离线演示 fixture 配置 deterministic default，不能把它带入 production Connection。

### 2.2 OpenAI

OpenAI 正式提供 `GET /v1/models`，使用 Bearer API Key。返回的是“当前可用模型”的基础信息，模型项包含可用于 API 请求的 `id`，但列表合同没有提供输入模态、上下文窗口、Responses API 兼容性或 Agent Tool 能力。[OpenAI List models](https://developers.openai.com/api/reference/resources/models/methods/list)

这意味着：

- 不能把 API 返回的每一项直接放进 Auto Studio 的 Agent Model 选择器；列表可能包含 Embedding、图像、音频、微调模型或不适合当前 Agent 协议的模型。
- 应把账号目录与本地 `AgentModelCompatibility` 求交集。未知模型可以显示在“其他/未验证”分组，但默认不可选，除非兼容性规则或 live qualification 明确允许。
- 目录返回顺序不是默认模型选择规则。

OpenAI 将无效凭据归为 `401`，权限问题归为 `403`；`429` 可能代表普通限流，也可能代表余额、组织或项目限额，必须检查 `error.code`，不能对所有 `429` 盲目重试。`500/503` 属于服务端暂态错误。[OpenAI error codes](https://developers.openai.com/api/docs/guides/error-codes)

### 2.3 Anthropic

Anthropic 正式提供 `GET https://api.anthropic.com/v1/models`。直接 API Key 使用 `x-api-key`，并必须发送 `anthropic-version`；`Authorization: Bearer` 是 Workload Identity 短期令牌通道，不应拿普通 Console API Key 代替。[Claude API overview](https://platform.claude.com/docs/en/api/overview)、[Anthropic authentication](https://platform.claude.com/docs/en/manage-claude/authentication)

模型列表支持分页，因此 Adapter 必须按 `has_more` 和游标读取完整目录，而不是只取第一页。当前模型 API 还提供显示名、输入/输出限制与能力数据；这些字段可以作为 Provider 声明，但仍需要 Auto Studio 自己的适配验证。[Anthropic List Models](https://platform.claude.com/docs/en/api/models/list)

Anthropic 的主要错误语义是：

- `401 authentication_error`：Key 格式、撤销或过期问题；
- `402 billing_error`：计费问题；
- `403 permission_error`：资源权限问题；
- `429 rate_limit_error`：限流或部分消费上限；
- `500 api_error`、`504 timeout_error`、`529 overloaded_error`：暂态服务错误。

官方说明 SDK 默认只对暂态失败做有限重试。Auto Studio 应保留自身有上限的退避策略，并读取 `retry-after`；不得重试凭据、权限和计费错误。[Anthropic API errors](https://platform.claude.com/docs/en/api/errors)

Rust 反序列化必须允许未知字段和未来枚举值，避免 Provider 增加能力字段后让整个目录刷新失败。

### 2.4 DeepSeek

DeepSeek 正式提供：

```http
GET https://api.deepseek.com/models
Authorization: Bearer <DEEPSEEK_API_KEY>
Accept: application/json
```

响应是 OpenAI 风格的 `{ "object": "list", "data": [...] }`，模型项只有 `id`、`object` 和 `owned_by` 等基础字段。[DeepSeek Lists Models](https://api-docs.deepseek.com/api/list-models/)

截至本次调研，官方目录示例使用 `deepseek-v4-flash` 与 `deepseek-v4-pro`；旧 `deepseek-chat`、`deepseek-reasoner` 已经过其公布的 2026-07-24 弃用日期。因此 Auto Studio 不能再以 `deepseek-chat` 作为代码内默认值，应以实时目录为身份来源。[DeepSeek change log](https://api-docs.deepseek.com/updates/)、[DeepSeek current model and pricing](https://api-docs.deepseek.com/quick_start/pricing/)

DeepSeek 的通用错误包括 `401` 凭据错误、`402` 余额不足、`429` 限流、`500` 服务端错误和 `503` 过载。只有限流和服务端错误适合有限重试；`401/402` 应立即进入需要用户处理的状态。[DeepSeek error codes](https://api-docs.deepseek.com/quick_start/error_codes/)

目录没有声明模型能力，所以 Tool Calling、reasoning、上下文窗口等必须来自版本化兼容性快照，不能从模型名临时猜测后直接投入生产。

### 2.5 Kimi Open Platform

Kimi Open Platform 的国际区正式提供：

```http
GET https://api.moonshot.ai/v1/models
Authorization: Bearer <MOONSHOT_API_KEY>
```

中国大陆区对应 `https://api.moonshot.cn/v1/models`。响应除基础模型身份外，还包含 `context_length`、`supports_image_in`、`supports_video_in` 和 `supports_reasoning` 等字段。[Kimi international List Models](https://platform.kimi.ai/docs/api/list-models)、[Kimi China List Models](https://platform.kimi.com/docs/api/list-models)

不同区域的账号和 Key 相互隔离，错用会得到 `401`。Auto Studio 应将国际区和中国大陆区建模为不同 Connection endpoint，禁止失败后静默切换区域或携带 Key 探测另一区域。[Kimi API troubleshooting](https://www.kimi.ai/help/kimi-api/api-troubleshooting)

Kimi 错误体包含 `error.type` 与 `error.message`。`401` 是鉴权错误，`403` 是权限错误；`429` 还需要区分 `engine_overloaded_error`、`rate_limit_reached_error` 与 `exceeded_current_quota_error`，只有前两类可能通过等待恢复，余额/额度问题不能自动重试。[Kimi error reference](https://platform.kimi.com/docs/api/errors)

### 2.6 Kimi Code

Kimi Code 和 Kimi Open Platform 是不同产品。当前官方文档为第三方工具公开的是：

- OpenAI-compatible base URL：`https://api.kimi.com/coding/v1`；
- Anthropic-compatible base URL：`https://api.kimi.com/coding/`；
- 当前 Model ID：`k3`、`k3-256k`、`kimi-for-coding`、`kimi-for-coding-highspeed`；
- 第三方接入使用 Kimi Code Console 创建的 Key。

证据见 [Kimi Code overview](https://www.kimi.com/code/docs/en/) 和 [Kimi Code model configuration](https://www.kimi.com/code/docs/en/kimi-code/models.html)。官方还明确把产品/团队 API 集成导向 Kimi Open Platform，而 Kimi Code 更偏向终端与 IDE 编程工作流。

当前公开产品文档没有把 `GET https://api.kimi.com/coding/v1/models` 定义为稳定第三方 API。旧版官方 Python CLI 曾按 OpenAI-compatible 方式请求过 `/models`，但当前 Kimi Code 已迁移到新客户端，旧版不再维护；它不足以形成今天的发布合同。因此：

- Kimi Code 首发目录来源应是随 Auto Studio 发布的、标注抓取日期和官方来源的 `BundledOfficialSnapshot`；
- `/model` 应清楚显示该目录是静态快照，而不是“已由当前 Key 验证”；
- 可以在实验开关下探测 `/models`，但失败不得影响已保存 Connection，也不得把成功结果升级为官方稳定保证；
- Kimi Code Key 和 Kimi Open Key 必须使用不同 Provider Kind、base URL 白名单和存储记录，禁止混用；
- 第三方请求必须保持 Auto Studio 的真实客户端身份，不伪装成其他工具。[Kimi Code overview](https://www.kimi.com/code/docs/en/)

## 3. OpenCode TUI 真正值得借鉴的设计

本节基于 OpenCode 官方仓库 commit [`3a4c253`](https://github.com/anomalyco/opencode/tree/3a4c253969870e42d166fe6754133e848acbd81b)。该快照与本次调研日期绑定，避免把未来改动反推为当前事实。

### 3.1 Slash Command 是主输入框的一种命令投影

OpenCode 在统一命令注册表中定义命令名称、标题、分类、slash name 和执行函数。`model.list` 暴露为 `/models`，`provider.connect` 暴露为 `/connect`，执行后只是把相应选择 Dialog 投影到当前 TUI 上，而不是跳到另一个永久“设置页面”。[命令注册源码](https://github.com/anomalyco/opencode/blob/3a4c253969870e42d166fe6754133e848acbd81b/packages/tui/src/app.tsx#L629-L744)

可借鉴点：

- 普通文本与 `/` 命令共用一个 Composer；
- 输入 `/` 后展示可搜索命令列表，继续输入做前缀/模糊过滤；
- 命令动作只改变当前 Overlay/状态，不创建独立向导工作流；
- Auto Studio 使用 `/model` 作为产品文案中的单数命令，同时注册 `/models` 兼容别名，可降低用户从 OpenCode 迁移的认知成本。

### 3.2 `/connect` 是 Provider 选择、鉴权方式、Secret 输入三段式 Overlay

OpenCode 的 Provider Dialog 将常用 Provider 排在前面，其他 Provider 按名称排序，用分组、描述和 `✓` 表示已连接项；选择 Provider 后，再根据 Provider 能力进入 API Key 或 OAuth 分支。[Provider 选项与分组](https://github.com/anomalyco/opencode/blob/3a4c253969870e42d166fe6754133e848acbd81b/packages/tui/src/component/dialog-provider.tsx#L19-L78)、[Provider 选择与 auth method](https://github.com/anomalyco/opencode/blob/3a4c253969870e42d166fe6754133e848acbd81b/packages/tui/src/component/dialog-provider.tsx#L116-L230)

API Key 保存后，OpenCode 会更新 auth、销毁旧实例、重新 bootstrap 同步状态，然后直接打开该 Provider 的模型 Dialog。[API Key 提交路径](https://github.com/anomalyco/opencode/blob/3a4c253969870e42d166fe6754133e848acbd81b/packages/tui/src/component/dialog-provider.tsx#L352-L419)

Auto Studio 应借用前三段 Overlay 和“保存后重新同步”机制，但按产品要求做一个明确差异：**保存成功后关闭 `/connect` Overlay，Core 后台刷新目录；用户随后从 `/model` 主动选模型。** 不应自动把用户推进另一个必填向导。

### 3.3 `/models` 使用同步后的目录状态，不在 Dialog 内直接访问 Provider

OpenCode 的模型 Dialog 从同步 Context 读取 Provider/Model 数据，按 Favorite、Recent 和 Provider 分组，排除 deprecated 模型，支持模糊搜索；选择后更新本地 current/recent 状态。[模型选择源码](https://github.com/anomalyco/opencode/blob/3a4c253969870e42d166fe6754133e848acbd81b/packages/tui/src/component/dialog-model.tsx#L12-L183)

这是比“Dialog 自己拿 Key 请求 Provider”更重要的架构原则：

- TUI 只消费 Core 的目录投影；
- Secret 永远不进入 TUI 长期 State；
- 后台刷新、缓存、重试和 stale 状态由 Core 管理；
- `/model` 在任何时刻都能稳定展示 `Refreshing`、`Ready`、`Stale` 或具体错误，而不会阻塞终端事件循环。

### 3.4 不要误解 OpenCode 的模型来源

OpenCode 并不是对每家 Provider 都使用用户 Key 请求上游 `/models`。其 Core 会从 `models.opencode.ai/api.json` 获取公共目录，设置 5 分钟缓存并在失败时使用磁盘/内置快照；Provider 列表再把公共目录、配置和已连接凭据合并。[Models catalog fetch/cache](https://github.com/anomalyco/opencode/blob/3a4c253969870e42d166fe6754133e848acbd81b/packages/core/src/models-dev.ts#L145-L215)、[Provider list 合并](https://github.com/anomalyco/opencode/blob/3a4c253969870e42d166fe6754133e848acbd81b/packages/opencode/src/server/routes/instance/httpapi/handlers/provider.ts#L35-L61)

因此 Auto Studio 可以借鉴它的交互与状态分层，但不能以“OpenCode 这样做”为依据，把 Models.dev 当成用户账号真实可用模型。Auto Studio 的正式目录应保留两类证据：

- `AccountCatalog`：使用当前 Connection 从 Provider 官方 API 获取；
- `CompatibilityCatalog`：Auto Studio 自己维护的协议、能力与质量验证快照。

最终 `/model` 展示的是二者合并后的视图，并保留每项的证据来源。

## 4. Auto Studio Core 设计建议

### 4.1 深接口

TUI 不应认识各 Provider 的 HTTP 细节。Core 对客户端暴露一个窄的目录接口：

```rust
pub trait ProviderCatalogService {
    async fn list_provider_kinds(&self) -> Result<Vec<ProviderKindView>, CatalogError>;
    async fn store_connection(&self, input: StoreConnection) -> Result<ConnectionView, CatalogError>;
    async fn refresh(&self, connection_id: ConnectionId) -> Result<RefreshAccepted, CatalogError>;
    async fn status(&self, connection_id: ConnectionId) -> Result<CatalogStatusView, CatalogError>;
    async fn list_models(&self, connection_id: ConnectionId) -> Result<ModelCatalogView, CatalogError>;
    async fn select_model(&self, connection_id: ConnectionId, model_id: ModelId)
        -> Result<ModelSelectionView, CatalogError>;
}
```

Provider 特有实现隐藏在 `ModelCatalogAdapter` 后：

```rust
pub trait ModelCatalogAdapter {
    async fn fetch(
        &self,
        connection: &ResolvedConnection,
        credential: &CredentialLease,
    ) -> Result<ProviderCatalogSnapshot, ProviderCatalogError>;
}
```

`CredentialLease` 只在后台请求生命周期内存在，Drop 时清零；不得出现在 Event、Debug、持久化目录或 TUI 响应中。

### 4.2 状态机

```text
Unconfigured
    |
    | store key
    v
CredentialStored -----> RefreshQueued -----> Refreshing
                                                |    |
                                      success   |    | auth/permission
                                                v    v
                                              Ready  ActionRequired
                                                |
                                      later refresh fails
                                                v
                                               Stale
```

建议状态定义：

| 状态 | 含义 | `/model` 行为 |
|---|---|---|
| `Unconfigured` | 尚未保存凭据 | 提供 `/connect` 入口 |
| `CredentialStored` | Key 已安全落盘，但尚未验证目录 | 显示等待后台任务 |
| `RefreshQueued` / `Refreshing` | 刷新已排队/执行中 | 显示 spinner，可退出 Overlay |
| `Ready` | 有当前成功快照 | 展示可选模型 |
| `Stale` | 有旧快照，但最新刷新失败 | 展示旧模型、时间与重试入口 |
| `ActionRequired` | 401、权限、计费或区域不匹配 | 不显示“Connected”；引导重新连接或检查账号 |
| `UnsupportedRemoteCatalog` | 如当前 Kimi Code | 展示带来源日期的官方静态快照 |

“Connected”只适合表示凭据记录存在；面向用户更准确的文案是 `Saved`、`Fetching models`、`Ready`、`Needs attention`。

### 4.3 目录快照

建议持久化到用户级应用数据目录，不进入 Project：

```text
ModelCatalogSnapshot
  snapshot_id
  connection_id
  provider_kind
  endpoint_identity
  source: AccountApi | BundledOfficialSnapshot | CompatibilityRegistry
  fetched_at
  expires_at
  adapter_version
  models[]
  response_request_id?
```

标准化模型项至少包含：

```text
model_id                 Provider 原始、精确 ID
display_name?
owned_by?
created_at?
shutdown_at?
context_window?
input_modalities?
reasoning?
agent_compatibility      Supported | Unverified | Unsupported
compatibility_evidence   ProviderDeclared | AutoStudioQualified | NameRule | Unknown
```

严禁把 API Key、认证头或完整错误响应原样写入快照。Provider 错误 message 在持久化和日志前必须经过 secret redaction。

### 4.4 后台刷新规则

1. Key 原子保存成功后，Core 创建 `CatalogRefresh` 工作项并立即响应 TUI。
2. 每个 Connection 同时只允许一个 refresh；重复触发合并到现有任务。
3. `401/402/403` 不自动重试，进入 `ActionRequired`，但不擅自删除 Key。
4. `429` 先按 Provider `error.code/type` 区分限流和余额；只有限流读取 `Retry-After` 并有限退避。
5. 网络错误、`500/503/529` 最多有限重试；超过上限后，有旧快照则 `Stale`，无旧快照则保留明确错误。
6. 一次刷新失败不得清空上一份成功快照。
7. 目录刷新不做收费推理；但也不在产品文案中承诺“免费”，除非 Provider 明确提供该保证。
8. 应提供用户主动 Retry；不要让 TUI 只能等待下一次启动。

### 4.5 模型选择规则

连接流程不再要求用户输入 Model ID，也不在连接成功后静默选择返回列表第一项。

- 当前没有选择时，状态为 `ModelSelectionRequired`；用户仍可浏览和管理连接，但不能开始需要 LLM 的 Agent Run。
- `/model` 默认突出显示 `AutoStudioQualified` 模型，其他账号可见模型放在“未验证”分组。
- 已选模型在刷新后仍存在且仍兼容时保持选择。
- 已选模型消失、下线或被标为不兼容时，不自动切换到另一个模型；进入 `ModelSelectionRequired` 并提示用户重新选择。
- 选择记录使用 `(connection_id, exact_model_id)`，不能只保存 `model_id`，避免不同区域或 Provider 同名冲突。

OpenCode 自己具有“CLI 参数 → 配置 → recent → Provider default/first”的 fallback；源码见 [model fallback](https://github.com/anomalyco/opencode/blob/3a4c253969870e42d166fe6754133e848acbd81b/packages/tui/src/context/local.tsx#L197-L245)。Auto Studio 不应照搬“first model”fallback，因为它会绕过用户要求的 `/model` 显式选择，也无法证明首项符合 Agent 能力和成本偏好。

## 5. TUI 交互验收标准

### 5.1 `/` 命令菜单

- 主界面只有一个常驻 Composer；不因 Provider 未配置而自动进入向导。
- 输入 `/` 展示命令 Overlay，至少包括 `/connect`、`/model`、`/help`、`/exit`。
- 支持方向键选择、Enter 执行、Esc 关闭、输入过滤。
- `/models` 是 `/model` 的别名，不在菜单中重复占两行。

### 5.2 `/connect`

- 第一层：可搜索 Provider 列表，分“常用”和“全部”；已有 Connection 显示状态图标和文字。
- 第二层：API Key 输入；默认掩码，允许显隐切换，但不复制到 Activity/Debug。
- 保存成功后：Overlay 关闭，非阻塞 Toast 显示“凭据已保存，正在获取模型”。
- 后台失败：全局状态区域显示 `Needs attention`，但不抢占 Composer 或把用户重新塞进向导。
- 再次 `/connect` 可以替换 Key；替换成功后创建新目录快照，旧模型选择只有在新目录确认存在时才保留。

### 5.3 `/model`

- `Refreshing`：显示 Provider、加载状态和“可关闭，后台继续”。
- `Ready`：按 Provider/Connection 分组，支持搜索；显示当前项、兼容性和目录时间。
- `Stale`：可以选择旧快照中的模型，但必须显示“目录可能过期”和 Retry。
- `ActionRequired`：显示可执行动作（重新连接、检查区域/余额、重试），不展示空白列表。
- Kimi Code：显示 `Official snapshot` 与抓取日期，避免误导为当前账号已经验证。

## 6. Core API 建议

```text
GET    /v1/provider-kinds
GET    /v1/provider-connections
POST   /v1/provider-connections
PUT    /v1/provider-connections/{connection_id}/credential
GET    /v1/provider-connections/{connection_id}/catalog
POST   /v1/provider-connections/{connection_id}/catalog:refresh
PUT    /v1/agent-model-selection
```

关键返回规则：

- Connection API 从不返回 secret，也不返回可逆 secret reference。
- 保存凭据返回 `202 Accepted` 或等价的 `refresh_status: queued`，而不是等待远端目录。
- Catalog API 返回快照与刷新状态；TUI 无需轮询时可订阅 Core Event。
- 模型选择 API 必须带当前 catalog revision/snapshot id，避免用户在目录更新后选中已经消失的模型。
- 对替换凭据和选择模型使用 revision/optimistic concurrency，避免多个客户端互相覆盖。

## 7. 最小测试矩阵

### 合同测试

- OpenAI：Bearer、混合模型类型过滤、未知模型保留为 `Unverified`、401/403/429 code 分类。
- Anthropic：`x-api-key` 与 version header、完整分页、未知字段、529、`retry-after`。
- DeepSeek：精确 `/models` 路径、Bearer、空列表、旧别名不再作为默认、401/402/503。
- Kimi Open：国际区/中国区 endpoint 与 Key 不交叉、能力字段解析、不同 429 类型。
- Kimi Code：静态快照 schema、来源日期、无远端目录时仍可使用 `/model`；实验探测不得成为默认测试前置条件。

### 状态测试

- `CredentialStored -> Refreshing -> Ready`；
- 首次刷新失败时无虚构模型；
- 有旧快照刷新失败进入 `Stale` 且不清空目录；
- 401 不自动重试；429 限流有限重试；429 额度错误不重试；
- 替换 Key 后旧 refresh 结果不能覆盖新 Connection revision；
- 模型下线后进入 `ModelSelectionRequired`，不自动切换。

### TUI 测试

- `/` 命令过滤与 Esc；
- `/connect` Provider 搜索、Key 掩码和 secret redaction；
- 保存后立即返回主 Composer，后台状态可见；
- `/model` 的 Loading、Ready、Stale、ActionRequired、Static Snapshot 五种投影；
- 所有 Debug、panic、HTTP error、event snapshot 中不出现测试 Key。

## 8. 推荐实施顺序

1. 先删除当前强制的 `Provider -> Model -> Key` 首次向导，只保留主 Composer 与 slash command Overlay。
2. 建立 Connection、Catalog Snapshot、Model Selection 三个独立领域对象和状态。
3. 实现 `/connect` 与安全凭据保存，保存后提交后台 refresh。
4. 先接 DeepSeek 目录合同并用 `DEEPSEEK_API_KEY` 做用户授权的 live smoke；这一步验证完整状态链，不把 Key 写入日志。
5. 实现 `/model` 和目录缓存，再接 OpenAI、Anthropic、Kimi Open。
6. Kimi Code 先用官方快照上线；等官方公开目录形成稳定合同后，再把它切换成账号目录刷新。

这个顺序先完成用户看得到的正确交互和 Core 状态边界，再扩展 Provider 数量；无需引入 Multi-Agent，也无需增加新的常驻服务。
