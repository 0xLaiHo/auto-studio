# Pi 针对不同 Provider 的 Thinking Level 适配研究

> 文档状态：实现参考快照。Canonical level 与 Provider-specific wire mapping 已落地；未知模型只使用 Provider Default。模型能力可能变化，发布前仍需 exact-model live qualification。  
> 日期：2026-08-22  
> 研究对象：[earendil-works/pi](https://github.com/earendil-works/pi)（Pi Agent Harness）  
> 固定快照：[commit `c49906ec77788625aacbdc53ebca6fbe65bd20f5`](https://github.com/earendil-works/pi/tree/c49906ec77788625aacbdc53ebca6fbe65bd20f5)  
> 范围：`packages/ai`、`packages/agent`、`packages/coding-agent`；重点覆盖 OpenAI Responses、OpenAI Chat Completions、Anthropic Messages、DeepSeek、Moonshot/Kimi 及 TUI/持久化。  
> 方法：逐层追踪固定提交中的类型、模型目录生成、请求构造、TUI、持久化与测试，并用 Provider 官方文档交叉核对。未进行真实 Key 调用；本文的“测试事实”指固定提交内已有的自动化测试断言，不代表本次执行了 Live Test。

## 结论摘要

Pi 的关键不是在每个 Adapter 里写一套 `if provider == ...`，而是把适配拆成四层：

1. **统一选择语言**：`off / minimal / low / medium / high / xhigh / max`。
2. **模型能力元数据**：每个模型有 `reasoning`、`thinkingLevelMap`，协议差异放在 `compat`；能力是“Provider + Model + API protocol”的属性，不只是 Provider 属性。
3. **统一过滤与归一化**：TUI 只显示该模型支持的档位；外部传入或持久化的无效档位通过 `clampThinkingLevel()` 收敛。
4. **协议级线格式转换**：OpenAI Responses、Anthropic Messages、OpenAI-compatible Chat Completions 分别构造原生参数；后者再用 `thinkingFormat` 处理 DeepSeek、OpenRouter、Together、Qwen、Z.AI 等方言。

因此，Pi **没有把输出 token 上限冒充 thinking level**。只有旧 Anthropic 这类 token-budget 协议会把统一档位映射成 thinking token budget；`max_tokens` / `max_output_tokens` 仍是独立的硬上限。

另一个重要事实是：Pi 当前 `/model` 和 `/thinking` 是两个选择器。`/model` 只用上下键选模型；`/thinking` 根据当前模型过滤档位。它不是“上下选模型、左右调 effort”的同屏实现。因此 Auto Studio 可以借鉴 Pi 的能力模型和请求映射，但不需要照抄它当前的 TUI 结构。

## 1. 从 Agent 到 Provider 的完整数据流

```text
AgentState.thinkingLevel
        │  off -> undefined；其他档位原样下传
        ▼
SimpleStreamOptions.reasoning
        │
        ├─ getSupportedThinkingLevels(model)  决定 UI 可见档位
        ├─ clampThinkingLevel(model, level)   收敛无效/过期配置
        ▼
Protocol streamSimple()
        │
        ├─ model.thinkingLevelMap             模型级值映射/禁用
        └─ model.compat                       协议方言/能力开关
        ▼
Provider 原生请求参数
```

Agent 将 `off` 转成 `reasoning: undefined`，其他值通过 `SimpleStreamOptions.reasoning` 传给统一模型层；这使“关闭推理”与具体 Provider 的 wire value 解耦。[Agent 下传](https://github.com/earendil-works/pi/blob/c49906ec77788625aacbdc53ebca6fbe65bd20f5/packages/agent/src/agent.ts#L445-L456) [统一请求选项](https://github.com/earendil-works/pi/blob/c49906ec77788625aacbdc53ebca6fbe65bd20f5/packages/ai/src/types.ts#L313-L322)

## 2. 统一类型与 `thinkingLevelMap` 的精确语义

Pi AI 层定义：

```ts
type ThinkingLevel = "minimal" | "low" | "medium" | "high" | "xhigh" | "max";
type ModelThinkingLevel = "off" | ThinkingLevel;
type ThinkingLevelMap = Partial<Record<ModelThinkingLevel, string | null>>;
```

Agent/TUI 状态把 `off` 也纳入统一枚举。模型则包含 `reasoning: boolean`、可选 `thinkingLevelMap` 和协议相关 `compat`。[AI 类型](https://github.com/earendil-works/pi/blob/c49906ec77788625aacbdc53ebca6fbe65bd20f5/packages/ai/src/types.ts#L82-L110) [Agent 类型](https://github.com/earendil-works/pi/blob/c49906ec77788625aacbdc53ebca6fbe65bd20f5/packages/agent/src/types.ts#L300-L300) [Model 类型](https://github.com/earendil-works/pi/blob/c49906ec77788625aacbdc53ebca6fbe65bd20f5/packages/ai/src/types.ts#L816-L851)

### 2.1 缺省、字符串、`null` 不是一回事

| `thinkingLevelMap[level]` | 能力/UI 语义 | 请求语义 |
|---|---|---|
| 字符串，如 `max: "max"` | 支持并显式声明映射 | 发送映射后的 Provider 值 |
| `null` | 明确不支持，TUI 隐藏 | 不应构造这个档位；`off: null` 还表示不能显式关闭 |
| 缺省 `undefined`，档位为 `off..high` | 默认视为支持 | Adapter 使用同名值或协议默认映射 |
| 缺省 `undefined`，档位为 `xhigh/max` | 默认视为不支持 | 这两个高档位必须显式 opt-in |

上述规则直接写在 `getSupportedThinkingLevels()`：非 reasoning 模型只返回 `off`；reasoning 模型过滤掉映射为 `null` 的档位，而 `xhigh/max` 只有存在非空映射才出现。[支持档位与 clamp](https://github.com/earendil-works/pi/blob/c49906ec77788625aacbdc53ebca6fbe65bd20f5/packages/ai/src/models.ts#L900-L936)

这套设计支持“中间有洞”的能力，例如 `high` 和 `max` 可用但 `xhigh` 不可用。Pi 的测试明确覆盖了该情况。[高档位 opt-in 与能力空洞测试](https://github.com/earendil-works/pi/blob/c49906ec77788625aacbdc53ebca6fbe65bd20f5/packages/ai/test/max-thinking.test.ts#L14-L68)

### 2.2 `clampThinkingLevel()` 的规则

请求档位不支持时，Pi 不是简单降一级：

1. 如果档位有效，直接返回；
2. 从请求档位开始**先向更高档搜索**；
3. 没找到再向更低档搜索；
4. 最后退回第一个可用档或 `off`。

例如模型支持 `high/max` 但不支持 `xhigh` 时，请求 `xhigh` 会被收敛为 `max`；普通 reasoning 模型没有 `max` 时，请求 `max` 会收敛为 `high`。[clamp 实现](https://github.com/earendil-works/pi/blob/c49906ec77788625aacbdc53ebca6fbe65bd20f5/packages/ai/src/models.ts#L913-L936) [对应测试](https://github.com/earendil-works/pi/blob/c49906ec77788625aacbdc53ebca6fbe65bd20f5/packages/ai/test/max-thinking.test.ts#L14-L68)

这会防止旧配置、跨模型切换或 SDK 调用把不支持的值发给 Provider。但“优先向上”可能增加成本；Auto Studio 若更重视成本可选择“向下优先”，但必须作为产品策略显式决定。

## 3. 能力元数据从哪里来

Pi 的内置模型目录不是运行时只靠 `GET /models` 猜能力。生成脚本会读取 `https://models.dev/api.json`，记录模型的 `reasoning_options`，并生成随包分发的 Provider 模型数据；脚本还包含针对官方行为或上游漂移的窄范围覆盖。[models.dev 数据入口](https://github.com/earendil-works/pi/blob/c49906ec77788625aacbdc53ebca6fbe65bd20f5/packages/ai/scripts/generate-models.ts#L1403-L1411) [数据组合优先级](https://github.com/earendil-works/pi/blob/c49906ec77788625aacbdc53ebca6fbe65bd20f5/packages/ai/scripts/generate-models.ts#L2330-L2347)

`reasoning_options` 可描述 `toggle`、离散 `effort values` 或 `budget_tokens`。Pi 只把经过目录声明的离散 effort 转成 `thinkingLevelMap`；`toggle` 和 `budget_tokens` 留给协议 Adapter；`default`/JSON `null` 不被猜成 Pi 档位。[模型目录归一化](https://github.com/earendil-works/pi/blob/c49906ec77788625aacbdc53ebca6fbe65bd20f5/packages/ai/scripts/models-dev-reasoning-options.ts#L1-L35) [归一化测试](https://github.com/earendil-works/pi/blob/c49906ec77788625aacbdc53ebca6fbe65bd20f5/packages/ai/test/reasoning-options.test.ts#L4-L35)

只有协议确实能直接发送 effort 时，生成器才采用该目录映射：Anthropic 要求 `forceAdaptiveThinking`；Responses 原生支持；Chat Completions 必须是 `thinkingFormat: "openai"` 且 `supportsReasoningEffort: true`。[直接 effort 的门控](https://github.com/earendil-works/pi/blob/c49906ec77788625aacbdc53ebca6fbe65bd20f5/packages/ai/scripts/generate-models.ts#L455-L494)

生成末尾按顺序应用：协议 compat → Anthropic compat → models.dev reasoning options → Pi 窄范围 override。也就是说，Pi 同时使用外部目录和自身已验证修正，而不是把其中任何一方当成永远正确。[最终合并顺序](https://github.com/earendil-works/pi/blob/c49906ec77788625aacbdc53ebca6fbe65bd20f5/packages/ai/scripts/generate-models.ts#L2802-L2811)

自定义 Provider/模型也能提供同样的 `reasoning`、`thinkingLevelMap` 和 `compat`；模型 override 对 map 做浅合并，对 compat 做包含嵌套项的合并。[配置 schema](https://github.com/earendil-works/pi/blob/c49906ec77788625aacbdc53ebca6fbe65bd20f5/packages/coding-agent/src/core/model-config.ts#L130-L177) [自定义模型组合](https://github.com/earendil-works/pi/blob/c49906ec77788625aacbdc53ebca6fbe65bd20f5/packages/coding-agent/src/core/provider-composer.ts#L81-L125)

## 4. 各协议的请求映射

### 4.1 OpenAI Responses

`streamSimple()` 先按模型能力 clamp，再把 `off` 转为“没有 reasoningEffort”；请求构造阶段：

- 开启时发送 `reasoning: { effort, summary: "auto" }`，并请求 `reasoning.encrypted_content`；
- 映射字符串优先，否则发送 canonical level；
- 关闭时，如果 `off !== null`，发送 map 中的 off 值，缺省为 `none`；
- 如果 `off: null`，完全省略 reasoning 参数，让模型使用其合法默认；
- `max_output_tokens` 独立由 `maxTokens` 控制，不用来模拟 effort。

[OpenAI Responses 归一化](https://github.com/earendil-works/pi/blob/c49906ec77788625aacbdc53ebca6fbe65bd20f5/packages/ai/src/api/openai-responses.ts#L198-L215) [请求构造](https://github.com/earendil-works/pi/blob/c49906ec77788625aacbdc53ebca6fbe65bd20f5/packages/ai/src/api/openai-responses.ts#L288-L339)

Pi 测试还区分了两种 off：支持 `none` 的 OpenAI 模型必须发 `reasoning.effort=none`；不支持 off 的模型必须省略整个 `reasoning`。[Responses off 合约测试](https://github.com/earendil-works/pi/blob/c49906ec77788625aacbdc53ebca6fbe65bd20f5/packages/ai/test/openai-responses-compat.test.ts#L208-L287)

OpenAI 官方 API 文档确认 reasoning effort 与输出 token 上限是不同控制项，且模型支持的档位不同；Pi 因此将能力放在模型元数据而非全局枚举硬编码。[OpenAI Responses API](https://platform.openai.com/docs/api-reference/responses/create)

### 4.2 OpenAI Chat Completions / OpenAI-compatible 方言

Chat Completions 是 Pi 最“深”的 Adapter：同一统一 level 通过 `compat.thinkingFormat` 转成不同 wire schema。[compat 类型](https://github.com/earendil-works/pi/blob/c49906ec77788625aacbdc53ebca6fbe65bd20f5/packages/ai/src/types.ts#L553-L625)

| `thinkingFormat` | 开启时 | 关闭时 |
|---|---|---|
| `openai` | `reasoning_effort: mapped_or_level` | 仅在 `off` 映射为字符串时发送该值 |
| `deepseek` | `thinking:{type:"enabled"}`；能力允许时再发 `reasoning_effort` | `off !== null` 时发 `thinking:{type:"disabled"}` |
| `openrouter` | `reasoning:{effort:...}` | `reasoning:{effort: off-map-or-none}`，除非 `off:null` |
| `together` | `reasoning:{enabled:true}`，可选 `reasoning_effort` | `reasoning:{enabled:false}` |
| `zai` | `thinking:{type:"enabled",clear_thinking:false}`，可选 effort | `thinking:{type:"disabled"}` |
| `qwen` | `enable_thinking:true`，可选 effort | `enable_thinking:false` |
| `qwen-chat-template` | `chat_template_kwargs.enable_thinking` + `preserve_thinking` | 同字段为 false |
| `chat-template` / `baseten` | 用配置模板注入 enabled/effort/budget | 按模板与 `omitWhenOff` 决定 |
| `string-thinking` | 顶层 `thinking: mapped_or_level` | `thinking: off-map-or-none`，除非 `off:null` |
| `ant-ling` | 仅 mapped effort 非空时发 `reasoning:{effort}` | 省略 |

完整分支位于 [Chat Completions thinking 参数构造](https://github.com/earendil-works/pi/blob/c49906ec77788625aacbdc53ebca6fbe65bd20f5/packages/ai/src/api/openai-completions.ts#L816-L924)。URL/Provider 自动侦测提供默认 compat，模型显式 `compat` 再覆盖它；DeepSeek 会自动识别为 `deepseek`，Moonshot 默认被识别为不支持直接 effort。[自动侦测](https://github.com/earendil-works/pi/blob/c49906ec77788625aacbdc53ebca6fbe65bd20f5/packages/ai/src/api/openai-completions.ts#L1535-L1628) [显式覆盖](https://github.com/earendil-works/pi/blob/c49906ec77788625aacbdc53ebca6fbe65bd20f5/packages/ai/src/api/openai-completions.ts#L1631-L1668)

对于本地 vLLM/llama.cpp 一类按 token budget 控制思考的服务，Pi 另有 `thinkingTokenBudgetField`，并始终至少给答案预留 1024 token；默认 thinking budgets 是 `1024/2048/8192/16384`，`xhigh/max` 在 token-budget 语义下降为 `high` budget。[预算算法](https://github.com/earendil-works/pi/blob/c49906ec77788625aacbdc53ebca6fbe65bd20f5/packages/ai/src/api/simple-options.ts#L54-L95)

### 4.3 Anthropic Messages

Pi 明确区分两代协议：

- **Adaptive thinking**：模型 `compat.forceAdaptiveThinking=true` 时发送 `thinking:{type:"adaptive"}` 和 `output_config:{effort}`；`minimal/low` 默认归一为 `low`，其他普通档位同名，`thinkingLevelMap` 可覆盖 `xhigh/max` 等特殊值。
- **Legacy manual thinking**：发送 `thinking:{type:"enabled",budget_tokens:N}`；档位映射到默认 thinking budget，同时调整 `max_tokens` 并给最终答案保留空间。
- **Off**：只有 `off !== null` 时才发送 `thinking:{type:"disabled"}`；always-thinking 模型用 `off:null` 隐藏关闭选项并避免非法请求。

[Anthropic 两代归一化](https://github.com/earendil-works/pi/blob/c49906ec77788625aacbdc53ebca6fbe65bd20f5/packages/ai/src/api/anthropic-messages.ts#L800-L870) [wire payload 构造](https://github.com/earendil-works/pi/blob/c49906ec77788625aacbdc53ebca6fbe65bd20f5/packages/ai/src/api/anthropic-messages.ts#L1036-L1090)

生成器按精确模型 ID 开启 adaptive compat，并针对不同 Claude 版本加入 `max`、`xhigh`、`off:null` 等能力差异。[Anthropic 模型 override](https://github.com/earendil-works/pi/blob/c49906ec77788625aacbdc53ebca6fbe65bd20f5/packages/ai/scripts/generate-models.ts#L875-L905)

Anthropic 官方文档确认 adaptive thinking 使用 `thinking.type=adaptive`，深度由 `output_config.effort` 控制；旧模型继续使用 `budget_tokens`，且 effort 是行为信号，不是严格 token budget。[Anthropic Effort](https://platform.claude.com/docs/en/build-with-claude/effort) [Adaptive migration](https://platform.claude.com/docs/en/build-with-claude/extended-thinking)

## 5. DeepSeek 与 Kimi/Moonshot 的模型级矩阵

以下是 Pi 固定提交内测试明确断言的可见档位及实际协议路径，不是根据模型名猜测：

| Provider / Model | Pi TUI 可见档位 | Pi 协议与 wire mapping | 备注 |
|---|---|---|---|
| `deepseek/deepseek-v4-flash` | `off, low, high, max` | Chat Completions；off→`thinking.disabled`；其余→`thinking.enabled` + `reasoning_effort=low/high/max` | Flash 的 `low` 是 Pi 窄范围 override |
| `deepseek/deepseek-v4-pro` | `off, high, max` | 同上；仅 high/max | `minimal/low/medium` 被标 `null` |
| `moonshotai[-cn]/kimi-k3` | `low, high, max` | Chat Completions `thinkingFormat=openai`；发送 `reasoning_effort` | `off:null`，不能从 Pi 关闭 |
| `kimi-coding/k3` | `low, high, max` | Anthropic Messages；`thinking.adaptive` + `output_config.effort` | Kimi Coding Provider 固定走 Anthropic-compatible API |
| `kimi-coding/kimi-for-coding`（K2.7 Code） | 目录决定；官方为 Thinking ON | Anthropic Messages adaptive；开启时发 `output_config.effort` | Pi Provider 对 Kimi Coding 模型统一设置 adaptive compat |
| `moonshotai[-cn]/kimi-k2.7-code` | `minimal, low, medium, high` | Chat Completions `thinkingFormat=deepseek`；始终为 thinking enabled，不发送 effort | `off:null`，这些多个 UI 档位在 wire 层没有不同 effort，是 Pi 当前的一个不理想退化 |
| `opencode-go/kimi-k2.6` | `off, high` | `thinking.enabled/disabled`，不发 `reasoning_effort` | Pi 主动把纯开关模型收成两态，做法正确 |

能力矩阵由 [DeepSeek 硬编码 map](https://github.com/earendil-works/pi/blob/c49906ec77788625aacbdc53ebca6fbe65bd20f5/packages/ai/scripts/generate-models.ts#L256-L266)、[Kimi/Moonshot 生成逻辑](https://github.com/earendil-works/pi/blob/c49906ec77788625aacbdc53ebca6fbe65bd20f5/packages/ai/scripts/generate-models.ts#L2083-L2180) 与 [Kimi/DeepSeek 窄范围 override](https://github.com/earendil-works/pi/blob/c49906ec77788625aacbdc53ebca6fbe65bd20f5/packages/ai/scripts/generate-models.ts#L907-L965) 共同形成。固定提交中的矩阵测试验证了上述可见档位。[模型能力矩阵测试](https://github.com/earendil-works/pi/blob/c49906ec77788625aacbdc53ebca6fbe65bd20f5/packages/ai/test/supports-xhigh.test.ts#L85-L133)

Provider 注册也证明 Kimi Coding 与 Moonshot Open Platform 是两条不同协议路径：前者是 `anthropic-messages`，后者是 `openai-completions`。[Kimi Coding Provider](https://github.com/earendil-works/pi/blob/c49906ec77788625aacbdc53ebca6fbe65bd20f5/packages/ai/src/providers/kimi-coding.ts#L1-L25) [Moonshot Provider](https://github.com/earendil-works/pi/blob/c49906ec77788625aacbdc53ebca6fbe65bd20f5/packages/ai/src/providers/moonshotai.ts#L1-L16)

DeepSeek 官方当前说明 OpenAI 格式用 `thinking.type` 控制开关、用 `reasoning_effort` 控制强度，并要求含 tool call 的续轮保留 `reasoning_content`；这与 Pi 的 DeepSeek format 和 replay compat 一致。[DeepSeek Thinking Mode](https://api-docs.deepseek.com/guides/thinking_mode)

但官方通用页当前列出的强度是 `high/max`，并说明兼容输入 `low/medium` 会映射为 `high`；Pi 却对 `deepseek-v4-flash` 显式开放并原样发送 `low`。这是一个需要 exact-model contract test 或 Live Test 才能确认的边缘差异，不能仅因 Pi 已写 override 就视为 Provider 官方保证。

Kimi Code 官方说明 K3 支持 `low/high/max`、默认 `high`，未知值返回 400；`none` 表示关闭思考，并同时说明 K3/K2.7 关闭思考会路由到 K2.6。Pi 因而为 K3 只显示三个已验证档位、隐藏 off，是比直接暴露全局三档更谨慎的产品选择。[Kimi Code Model Configuration](https://www.kimi.com/code/docs/en/kimi-code/models.html)

### 值得保留的批判性结论

Pi 的机制成熟，但数据并非绝对完美：`thinkingLevelMap` 对 `off..high` 的“缺省即支持”方便兼容未知 OpenAI-style Provider，却可能让纯开关模型暴露多个实际等价档位。`moonshotai/kimi-k2.7-code` 在当前测试中就是例子。Auto Studio 应采用更显式的 capability kind，避免靠缺省推断 toggle 模型的档位。

## 6. TUI：能力过滤、切换与持久化

### 6.1 Pi 当前不是同屏左右调 effort

固定提交中的 `/model`：

- 根据已配置 Provider 的目录加载模型并后台刷新；
- fuzzy search；
- 上/下移动、Enter 选择、Ctrl+S 设为默认；
- 不处理左右键，也不在模型列表同屏修改 thinking。

[Model selector 输入处理](https://github.com/earendil-works/pi/blob/c49906ec77788625aacbdc53ebca6fbe65bd20f5/packages/coding-agent/src/modes/interactive/components/model-selector.ts#L364-L418) [打开 selector](https://github.com/earendil-works/pi/blob/c49906ec77788625aacbdc53ebca6fbe65bd20f5/packages/coding-agent/src/modes/interactive/interactive-mode.ts#L4952-L4986)

Thinking 由独立 `/thinking` selector 处理。它调用 `session.getAvailableThinkingLevels()`，所以 `null`/高档位 opt-in 会直接影响 UI；支持搜索、上下选择、Enter 当前会话应用、Ctrl+S 设全局默认。[Thinking selector](https://github.com/earendil-works/pi/blob/c49906ec77788625aacbdc53ebca6fbe65bd20f5/packages/coding-agent/src/modes/interactive/components/thinking-selector.ts#L37-L134) [命令入口](https://github.com/earendil-works/pi/blob/c49906ec77788625aacbdc53ebca6fbe65bd20f5/packages/coding-agent/src/modes/interactive/interactive-mode.ts#L4754-L4800)

模型切换时，优先级是：显式 scoped-model level → `provider/modelId` 的 per-model default → global default → 当前 level；随后再 clamp 到新模型能力。模型默认与 thinking 默认不会被隐式绑在一起。[模型切换](https://github.com/earendil-works/pi/blob/c49906ec77788625aacbdc53ebca6fbe65bd20f5/packages/coding-agent/src/core/agent-session.ts#L1592-L1672) [level 选择与 clamp](https://github.com/earendil-works/pi/blob/c49906ec77788625aacbdc53ebca6fbe65bd20f5/packages/coding-agent/src/core/agent-session.ts#L1710-L1789)

### 6.2 两层持久化

Pi 把长期偏好与会话事实分开：

- `settings.json`：`defaultThinkingLevel` 和 `modelThinkingLevels["provider/modelId"]`；
- session transcript：追加 `thinking_level_change` 和 `model_change`，恢复分支时沿父链重建当时的 model/level。

[Settings schema](https://github.com/earendil-works/pi/blob/c49906ec77788625aacbdc53ebca6fbe65bd20f5/packages/coding-agent/src/core/settings-manager.ts#L91-L105) [默认与 per-model 保存](https://github.com/earendil-works/pi/blob/c49906ec77788625aacbdc53ebca6fbe65bd20f5/packages/coding-agent/src/core/settings-manager.ts#L778-L813) [Session entry](https://github.com/earendil-works/pi/blob/c49906ec77788625aacbdc53ebca6fbe65bd20f5/packages/coding-agent/src/core/session-manager.ts#L53-L67) [恢复与追加](https://github.com/earendil-works/pi/blob/c49906ec77788625aacbdc53ebca6fbe65bd20f5/packages/coding-agent/src/core/session-manager.ts#L362-L376)

这比只在 Connection 中保存一个全局 effort 更适合多 Provider，因为不同模型的合法档位不同。

## 7. Pi 内置、模型目录、自定义 Provider 的责任边界

| 责任 | Pi 内置代码 | 生成模型目录 | 自定义 Provider/模型 |
|---|---|---|---|
| Canonical levels、filter、clamp | 是 | 否 | 复用 |
| Responses/Anthropic/Chat wire schema | 是 | 否 | 可复用或自带 `streamSimple` |
| `reasoning` 能力 | 默认结构 | 主要来源 | 可显式声明/覆盖 |
| 精确合法 effort | 少数硬编码 override | `reasoning_options` 主要来源 | `thinkingLevelMap` 声明 |
| 协议方言 | 自动侦测 + compat 默认 | 生成器写入差异 | `compat` 显式声明 |
| always-thinking / off 禁止 | 少数模型 override | map 可提供 | `off:null` |
| 运行时 Provider fallback | 不通过“偷偷换档”实现 | 无 | 无；先 clamp，再按明确 wire contract 发送 |

这里没有“Provider 报不支持后自动改成另一个 effort 并重试”的静默降级。正确性来自请求前的 catalog/filter/clamp。若目录过期，仍可能收到 400，因此 Pi 用固定请求体测试和窄范围 override 追赶真实合同。

## 8. 与 Auto Studio 当前实现的差距

Auto Studio 当前：

- `LlmModelDescriptor` 只有 `id/displayName`，没有 reasoning capability；
- 全局只有 `Low/High/Max`，所有模型共用；
- `Low/High/Max` 同时被映射为 `1024/2048/4096` 输出上限；
- 只有 DeepSeek 真正发送原生 thinking 参数，且 `Low` 被实现为 `thinking.disabled`；
- OpenAI Responses、Anthropic、Kimi 目前只改变输出 token 上限；
- Connection 持久化的是 requested model + effort，没有保存 resolved protocol mapping/capability version。

[Auto Studio 模型与 effort 类型](../../../crates/autostudio-core/src/provider.rs) [当前请求实现](../../../crates/autostudio-provider/src/llm.rs) [当前请求合约测试](../../../crates/autostudio-provider/tests/llm_provider_contract.rs)

因此严格结论是：Auto Studio 当前并未完成多 Provider thinking-level 适配；DeepSeek 是唯一原生适配，而且 `Low = Off` 的 UI 语义不成立。Pi 的代码证明输出 budget 与 thinking effort 应拆开。

## 9. 对 Auto Studio 可直接落地的设计

### 9.1 扩展模型能力，而不是扩展 Provider `match`

建议将目录项改成类似：

```rust
pub enum ThinkingLevel {
    Off,
    Minimal,
    Low,
    Medium,
    High,
    XHigh,
    Max,
}

pub enum ThinkingControl {
    Unsupported,
    Toggle {
        off_supported: bool,
    },
    Effort {
        supported: Vec<ThinkingLevel>,
        wire_values: BTreeMap<ThinkingLevel, String>,
        default: Option<ThinkingLevel>,
    },
    TokenBudget {
        budgets: BTreeMap<ThinkingLevel, u32>,
        off_supported: bool,
    },
    AdaptiveEffort {
        supported: Vec<ThinkingLevel>,
        wire_values: BTreeMap<ThinkingLevel, String>,
        off_supported: bool,
    },
}

pub struct LlmModelCapabilities {
    pub thinking: ThinkingControl,
    pub source: CapabilitySource,
    pub mapping_version: String,
}
```

这里比 Pi 更显式：toggle 不会因为 map 缺省而意外出现多个等价档位；`xhigh/max` 仍需明确声明。

### 9.2 请求层分成 canonical resolution 与 wire encoding

```text
requested level
  -> resolve(model capabilities, policy)
  -> EffectiveThinking { mode, effort, budget, output_limit }
  -> protocol encoder
```

协议 encoder 至少拆为：

- `OpenAiResponsesThinkingEncoder`
- `OpenAiChatThinkingEncoder` + 明确 compat format
- `AnthropicAdaptiveThinkingEncoder`
- `AnthropicLegacyThinkingEncoder`

不要继续让 `output_token_budget()` 承担 effort 的含义。输出上限应是独立的 `max_output_tokens` 配置；旧 Anthropic/manual budget 才需要同时计算 thinking budget 与总输出空间。

### 9.3 首批精确映射

| 目标 | Auto Studio 应发送 |
|---|---|
| OpenAI Responses | `reasoning.effort=<model-map>`；off 支持时为 `none`，不支持时隐藏 off/省略字段 |
| Anthropic adaptive | `thinking.type=adaptive` + `output_config.effort` |
| Anthropic legacy | `thinking.type=enabled` + `budget_tokens`；off 合法时发 disabled |
| DeepSeek V4 Pro | `Off / High / Max`；enabled 时发 `reasoning_effort=high/max` |
| DeepSeek V4 Flash | 是否暴露 Low 必须由 exact model contract/live test 决定，不能按 Provider 全局开放 |
| Moonshot/Kimi K3 | `Low / High / Max`；按所选协议发送 `reasoning_effort` 或 Anthropic `output_config.effort` |
| Kimi K2.7 / K2.6 toggle-only | UI 只显示 `Off/On` 或 `Thinking On`，不要伪造 Low/High/Max |

### 9.4 TUI 保留用户要求的同屏交互，但由能力驱动

Auto Studio 可以实现用户要求的“上下选模型、左右调档位”，无需复制 Pi 的两个 selector：

1. 上下改变当前模型行；
2. 每次变更模型后读取 `supported_levels(model)`；
3. 左右只在该列表内循环；toggle 模型显示 `Off/On`，always-thinking 显示只读 `On`；
4. Enter 原子保存 `(provider, model, requested_level)`；
5. 状态栏同时显示 `effective: anthropic adaptive/high` 一类真实映射；
6. 切换 effort 时提示可能破坏 prompt cache。Anthropic 与 Kimi 官方文档都明确说明中途改变 effort 会影响缓存命中。[Anthropic cache 提示](https://platform.claude.com/docs/en/build-with-claude/effort#changing-effort-mid-conversation) [Kimi cache 提示](https://www.kimi.com/code/docs/en/kimi-code/models.html)

### 9.5 持久化 requested 与 effective，而不是只存一个枚举

建议至少记录：

```text
requestedThinkingLevel
effectiveThinkingMode
effectiveEffort
effectiveBudgetTokens
maxOutputTokens
capabilityMappingVersion
```

Connection 保存用户的 per-model preference；Run provenance 保存本次实际解析结果。不要保存 Provider 私有 chain-of-thought。

### 9.6 测试门槛

每个内置模型族至少需要：

1. catalog capability matrix 测试；
2. unsupported/null/filter/clamp 测试；
3. 每个 level 的完整 wire request snapshot；
4. off supported 与 always-thinking 两类测试；
5. model switch 后 per-model preference + clamp + session restore；
6. 使用真实 Key 的 opt-in live test；无 Key 必须报告 SKIP，不能算 PASS。

## 10. 最终判断

最值得借鉴的不是 Pi 的枚举本身，而是这三个边界：

1. **能力属于模型与协议组合**，Provider 名称不足以决定 thinking 参数；
2. **UI 必须由能力目录过滤**，不能把统一控件等同于统一能力；
3. **requested、effective、output ceiling 三者必须分开**。

Auto Studio 当前把 `ModelEffort` 同时当 UI 选择、输出预算和部分 Provider thinking 参数，已经需要拆分。建议先完成 capability schema、resolution 与四类 encoder，再把同屏左右键选择接到 `supported_levels(model)`；这样既保留当前 TUI 目标，又避免把 Pi 在 Kimi toggle 模型上的缺省推断问题一起复制过来。
