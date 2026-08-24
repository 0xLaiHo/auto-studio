# OpenCode 针对不同 Provider 的 Thinking Level 适配研究

> 文档状态：实现参考快照。Auto Studio 采用 capability-driven canonical level，而不是照搬 OpenCode variant；当前行为以 Rust 实现和技术设计为准。  
> 日期：2026-08-22  
> 研究对象：[anomalyco/opencode](https://github.com/anomalyco/opencode)  
> 固定快照：[commit `e00890c67261a435cee6409366a68999a93393fd`](https://github.com/anomalyco/opencode/tree/e00890c67261a435cee6409366a68999a93393fd)  
> 分支说明：该仓库没有 `main` 作为默认分支；根目录 `AGENTS.md` 明确默认分支是 `dev`，本文固定研究 `origin/dev` 的上述提交。  
> 范围：模型目录、`reasoning_options`、variant 构造、Provider 请求转换、OpenAI / Anthropic / DeepSeek / Moonshot / Kimi、TUI、持久化和自动化测试。未使用真实 API Key 发起 Live Test。

## 结论摘要

OpenCode 的核心抽象不是 Pi 那样的强类型 canonical `ThinkingLevel`，而是通用的 **model variant**：

```text
模型目录 reasoning_options
        ↓
model.variants: variant name → Provider option object
        ↓
TUI 保存 provider/model 对应的 variant name
        ↓
请求时把 variant option 合并进模型/Agent/base options
        ↓
按 AI SDK adapter namespace 写入 providerOptions
        ↓
AI SDK 编码为 Provider 原生请求
```

例如，同一个 UI 名称 `high` 可以对应：

- OpenAI：`{ reasoningEffort: "high", reasoningSummary: "auto", include: [...] }`
- Anthropic adaptive：`{ thinking: { type: "adaptive" }, effort: "high" }`
- Anthropic legacy：`{ thinking: { type: "enabled", budgetTokens: 16000 } }`
- Google：`{ thinkingConfig: { includeThoughts: true, thinkingLevel: "high" } }`
- OpenRouter：`{ reasoning: { effort: "high" } }`

因此 OpenCode 更接近“**命名配置预设**”，不是“统一思考强度枚举”。`low/high/max` 只是常用 variant 名，用户还可以添加任意名称和任意 Provider options。模型 schema 本身将 `variants` 定义成 `Record<string, Record<string, any>>`，没有强制 reasoning 语义。[模型 schema](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/opencode/src/provider/provider.ts#L1053-L1068)

另一个必须纠正的点是：**当前 OpenCode 的 `/models` 并没有实现截图里的“上下选模型、左右切 Thinking”**。固定提交中 `/models` 只负责模型选择；variant 通过 `Ctrl+T` 循环，或通过独立 `/variants` 对话框选择。首次选择一个有 variants 且没有有效历史选择的模型时，会继续打开 `Select variant` 对话框。[模型对话框](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/tui/src/component/dialog-model.tsx#L142-L182) [Variant 对话框](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/tui/src/component/dialog-variant.tsx#L6-L38)

## 1. 模型能力从哪里来

OpenCode 使用自己的 `https://models.opencode.ai/api.json` 模型目录。目录在本地缓存，默认五分钟内视为新鲜，并每 60 分钟尝试刷新；离线时可用编译快照或磁盘缓存。[目录 schema 与加载](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/core/src/models-dev.ts#L52-L120) [缓存与刷新](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/core/src/models-dev.ts#L160-L181) [周期刷新](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/core/src/models-dev.ts#L233-L257)

模型的 reasoning 能力由两层信息表达：

```ts
reasoning: boolean
reasoning_options?: Array<
  | { type: "effort"; values: Array<string | null> }
  | { type: "toggle" }
  | { type: "budget_tokens"; min?: number; max?: number }
>
```

这比只提供 `reasoning: true` 更精确：目录能区分离散 effort、开关和 token budget。但转换完成后，这些类型不会继续进入运行时；它们会被编译成无类型的 variant option object。[目录 schema](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/core/src/models-dev.ts#L52-L76)

### 1.1 目录转换的优先级

`fromModelsDevModel()` 的核心规则是：

```ts
reasoningVariants(modelsDevModel, runtimeModel)
  ?? legacyHeuristicVariants(runtimeModel)
```

[目录模型转换](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/opencode/src/provider/provider.ts#L1240-L1294)

`reasoningVariants()` 的精确语义是：

1. `reasoning_options === undefined`：返回 `undefined`，启用旧的模型名/SDK heuristic。
2. `reasoning_options === []`：返回空对象，明确不暴露 variants，不再 fallback。
3. 只要存在 `effort`：优先使用 effort；同组 `toggle` 和 `budget_tokens` 被忽略。
4. 没有 effort、有 budget：生成 `high/max`；支持 toggle 的 adapter 还会合并 `none`。
5. 只有 toggle：adapter 能编码开关就生成 `none/high`；不能编码则返回 `undefined`，再退回旧 heuristic。
6. 显式 effort 但 adapter 不支持：返回空对象，不编造 heuristic variants。

[Reasoning options 转换](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/opencode/src/provider/transform.ts#L1654-L1717) [优先级与 unsupported 测试](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/opencode/test/provider/transform.test.ts#L3597-L3645)

### 1.2 Budget 如何变成 `high/max`

对于 `budget_tokens`，OpenCode 不暴露连续数值，而是合成为两个 variant：

- `max = min(catalog max 或 31999, model.output_limit - 1, 31999)`；
- `high = max(catalog min, (max + 1) / 2)`，并限制不超过 max。

随后由协议 adapter 把预算写进 Anthropic `thinking.budgetTokens`、Google `thinkingBudget`、Bedrock `reasoningConfig.budgetTokens` 等字段。[预算生成](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/opencode/src/provider/transform.ts#L1686-L1699) [预算协议映射](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/opencode/src/provider/transform.ts#L1810-L1853)

自动化测试覆盖了最大值低于 output limit、目录省略 max、Google 显式最大值等边界。[预算测试](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/opencode/test/provider/transform.test.ts#L3497-L3595)

## 2. Variant 如何进入真实请求

请求准备阶段先取得用户消息中的 variant 名，再查 `model.variants[name]`。合并顺序是：

```text
Provider base options
  ← model.options
  ← agent.options
  ← selected variant options
```

后者覆盖前者。输出上限是独立的 `maxOutputTokens` 字段，没有拿输出 token 伪装 thinking effort。[请求合并](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/opencode/src/session/llm/request.ts#L80-L99) [LLM 调用](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/opencode/src/session/llm.ts#L313-L323)

最终 options 不直接拼原始 HTTP JSON，而是按所选 AI SDK 写进对应 namespace：OpenAI 写到 `providerOptions.openai`，Anthropic 写到 `providerOptions.anthropic`，Azure 同时写 `openai` 和 `azure`，Gateway 则按 upstream slug 分流。[Provider option namespace](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/opencode/src/provider/transform.ts#L1358-L1415)

这条边界很重要：OpenCode 源码中的 `reasoningEffort`、`budgetTokens` 是 AI SDK provider option 命名，不一定等同于最终 wire JSON 的大小写。真正的 wire 编码由对应 AI SDK adapter 完成。

## 3. 各 Provider / 模型的具体适配

### 3.1 OpenAI Responses

OpenAI 模型通过 `@ai-sdk/openai`，内置 Provider 强制使用 Responses API。[OpenAI model loader](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/opencode/src/provider/provider.ts#L208-L215)

OpenCode 会按**具体模型 ID 和发布日期**决定合法档位，而不是给所有 OpenAI 模型同一组：

- 通用集合包含 `low/medium/high`；
- 部分 GPT-5 支持 `minimal` 或 `none`；
- GPT-5.2+、部分 Codex 支持 `xhigh`；
- Pro、Chat、Deep Research 可能只有单一或更窄档位。

[OpenAI effort 矩阵](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/opencode/src/provider/transform.ts#L574-L644)

每个 effort variant 被转换为：

```ts
{
  reasoningEffort: effort,
  reasoningSummary: "auto",
  include: ["reasoning.encrypted_content"]
}
```

`include` 用于 `store:false` 下跨轮保留加密 reasoning state。[OpenAI variant 构造](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/opencode/src/provider/transform.ts#L954-L979)

### 3.2 Anthropic

OpenCode 明确区分两代 Anthropic 能力：

- 新模型/adaptive：variant 为 `thinking:{type:"adaptive"}` + `effort`；对默认省略 thinking 文本的模型补 `display:"summarized"`。
- Claude Opus 4.5：`thinking.enabled + budgetTokens` 与 `effort` 同时设置。
- 旧模型/manual budget：只生成 `high/max`，预算默认约为 16000 / 31999，并限制在 model output limit 内。

[Anthropic 模型识别](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/opencode/src/provider/transform.ts#L655-L685) [Anthropic variants](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/opencode/src/provider/transform.ts#L982-L1028) [Anthropic effort 编码](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/opencode/src/provider/transform.ts#L1786-L1807)

目录显式提供 effort 时，OpenCode只暴露目录声明的值；目录仅提供 budget 时，才合成 `high/max`。相关测试覆盖 native Anthropic、Vertex Anthropic 和 Bedrock 的不同字段。[跨协议 Anthropic 测试](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/opencode/test/provider/transform.test.ts#L3335-L3509)

### 3.3 DeepSeek

固定提交内的目录快照声明：

- `deepseek-v4-flash`：`toggle` + `effort=[high,max]`
- `deepseek-v4-pro`：`toggle` + `effort=[high,max]`
- `deepseek-reasoner`：`reasoning_options=[]`

[固定目录中的 DeepSeek](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/opencode/test/tool/fixtures/models-api.json#L169420-L169574)

但实际转换有一个重要结果：因为 effort 优先，V4 的 toggle 被忽略；OpenAI-compatible adapter 将 `high/max` 变为 `{reasoningEffort: ...}`。因此当前 V4 的 UI variants 是 **High / Max，没有 Off**。`deepseek-reasoner` 的显式空数组则完全禁止 variants。

旧 heuristic 还显式排除了 `deepseek-chat/reasoner/R1/V3` 等模型，避免给 always-thinking 或无档位模型编造通用 `low/medium/high`。[旧 heuristic 的 DeepSeek/Kimi 排除](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/opencode/src/provider/transform.ts#L727-L791) [DeepSeek 无 heuristic variant 测试](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/opencode/test/provider/transform.test.ts#L3683-L3703)

这与 Pi 的 DeepSeek adapter 差异很大：Pi 对 DeepSeek 有显式 `thinking.type=enabled/disabled` 方言；OpenCode 当前主要依赖 models catalog + openai-compatible SDK 的 `reasoningEffort`。对 Auto Studio 而言，不能只复制 OpenCode 的 DeepSeek option object，必须用真实 DeepSeek 请求合约验证开关和 effort 是否都落到 wire。

### 3.4 Moonshot Open Platform 与 Kimi for Coding

OpenCode 把两条 Kimi 路径分开：

- `moonshotai`：`@ai-sdk/openai-compatible`
- `kimi-for-coding`：`@ai-sdk/anthropic`

[固定目录 provider 定义](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/opencode/test/tool/fixtures/models-api.json#L21351-L21620) [Kimi for Coding 定义](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/opencode/test/tool/fixtures/models-api.json#L100074-L100210)

对 Anthropic-compatible Kimi，OpenCode 默认启用：

```ts
thinking: { type: "adaptive", display: "summarized" }
effort: "high"
```

如果模型有 effort variants，选中的 variant 再覆盖 `effort`。没有目录元数据时，Kimi heuristic 会生成 `low/medium/high/xhigh/max` 五档。[Kimi 默认与 variants](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/opencode/src/provider/transform.ts#L770-L777) [Kimi 默认请求](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/opencode/src/provider/transform.ts#L1235-L1244) [Kimi 测试](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/opencode/test/provider/transform.test.ts#L5592-L5648)

对 OpenAI-compatible Moonshot，`toggle` 目前没有专用 encoder，fallback 又因模型名含 `kimi` 而返回空 variants。因此目录中的 Kimi K2.5/K2.6 不会得到可调 effort；always-thinking/code 模型的显式空 `reasoning_options` 也不会得到 variants。

这说明 OpenCode 的“统一 variants”机制很灵活，但当前 Kimi 行为依赖 transport：同一个模型家族走 Anthropic-compatible 时可有五档，走 OpenAI-compatible 时可能完全不可调。能力不是 Provider 名的属性，而是：

```text
Provider + exact model + transport/SDK + catalog snapshot
```

## 4. TUI 到底怎么操作

固定提交中的真实交互是：

1. `/models` 打开模型列表；通用 `DialogSelect` 用上/下移动、Enter 选择。
2. 选择模型后立即执行 `local.model.set(...)`。
3. 如果该模型有 variants，且当前 per-model variant 不存在或已失效，再跳转到 `Select variant`。
4. `/variants` 可单独打开 variant 列表。
5. `Ctrl+T` 调用 `variant.cycle()`：无选择 → 第一项 → 下一项 → `default` → 第一项。
6. Prompt 底栏只在当前 variant 有效时显示它。

[`/models` 命令](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/tui/src/app.tsx#L630-L639) [`/variants` 与 cycle](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/tui/src/app.tsx#L705-L727) [默认快捷键](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/tui/src/config/keybind.ts#L119-L133) [通用列表按键](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/tui/src/ui/dialog-select.tsx#L369-L481)

因此，用户截图中“上下选模型、左右改 Thinking”的 Kimi Code 页面**不是当前 OpenCode 的实现**。OpenCode 可以借鉴的是 variant 数据流和 Provider option 预设，而不是那套同屏左右键布局。

还有一个产品行为值得注意：模型在 variant 对话框出现前已经被选中；若用户按 Esc 取消 variant 对话框，模型仍然保留，并使用 `default`（无 variant override）。这不是模型+thinking 的原子选择。

### `default` 不等于 `off`

OpenCode 的 `default` 是 TUI sentinel：表示“不叠加任何 variant option”。它会回到 Provider/model base options，可能仍然启用推理，例如 Kimi Anthropic 默认就是 adaptive + high，OpenAI GPT-5 base 也可能默认 medium。显式关闭只有模型 variants 中真实存在 `none` 时才成立。[Variant sentinel 与循环](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/tui/src/context/local.tsx#L362-L404) [GPT-5 base default](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/opencode/src/provider/transform.ts#L1279-L1304)

## 5. 持久化、恢复与 fallback

OpenCode 有两类持久状态：

### 5.1 本地 per-model 偏好

TUI 在 state 目录的 `model.json` 中保存：

```json
{
  "recent": [],
  "favorite": [],
  "variant": {
    "provider/model": "high"
  }
}
```

Variant 以 `provider/model` 为 key，因此同一用户可以为不同模型保留不同 variant。写入使用 atomic JSON helper。[本地 state](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/tui/src/context/local.tsx#L137-L195) [per-model variant](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/tui/src/context/local.tsx#L362-L389)

### 5.2 Session 与消息事实

每次 prompt 会携带当前 variant；服务端把 variant 放进 user message model，并更新 Session 当前 model。切换 Session 时，TUI 从最后一条 user message 恢复 agent/model/variant。[TUI 提交](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/tui/src/component/prompt/index.tsx#L989-L1008) [Prompt 请求](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/tui/src/component/prompt/index.tsx#L1083-L1112) [Session 恢复](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/tui/src/component/prompt/index.tsx#L311-L330) [Session model schema](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/opencode/src/session/session.ts#L216-L220)

无效/过期 variant 的处理是“静默回到 default”，不是 Pi 的最近合法档位 clamp：

- TUI `current()` 发现名字不在当前模型 variant list，就返回 `undefined`；
- Request 查不到 variant object，也只合并空对象；
- 选择模型时如果有 variants，会要求重新选择；
- Session prompt 测试确认显式 variant 会进入 user message。

[TUI 合法性检查](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/tui/src/context/local.tsx#L362-L404) [Request fallback](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/opencode/src/session/llm/request.ts#L80-L91) [消息 variant 测试](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/opencode/test/session/prompt.test.ts#L2313-L2355)

OpenCode 只持久化 variant 名，不持久化当时解析后的 effective Provider options 或 catalog/version hash。目录或转换代码升级后，同一历史 `high` 可能生成不同请求。这对 coding agent 可接受，但对强调工程审计和 provenance 的 Auto Studio 不够。

## 6. 配置覆盖与测试策略

用户可给模型添加任意 variant、修改内置 variant，或用 `{disabled:true}` 删除单个档位；合并后 `disabled` 字段会被剥离，不会送入 Provider。[配置文档](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/web/src/content/docs/models.mdx#L67-L134) [Variant 文档](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/web/src/content/docs/models.mdx#L138-L200) [配置合并实现](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/opencode/src/provider/provider.ts#L1676-L1686)

固定提交的测试覆盖较完整的底层合同：

- explicit / empty / fallback reasoning options；
- effort、toggle、budget 到不同 AI SDK options 的映射；
- OpenAI exact-model effort 集；
- Anthropic adaptive/manual/Bedrock/Vertex；
- Kimi transport 差异；
- variant 自定义、禁用与 merge；
- variant 进入 Session user message。

[目录优先级测试](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/opencode/test/provider/provider.test.ts#L1486-L1549) [配置覆盖测试](https://github.com/anomalyco/opencode/blob/e00890c67261a435cee6409366a68999a93393fd/packages/opencode/test/provider/provider.test.ts#L1604-L1777)

但没有在固定提交中找到直接覆盖 `/models → Select variant → Esc/Enter`、`Ctrl+T` 循环和 `model.json` 恢复的 TUI 交互测试；也没有对 DeepSeek/Moonshot 官方端点的真实请求体 Live Contract Test。对 Auto Studio 来说，这两类测试不能省略。

## 7. OpenCode 与 Pi 的关键差异

| 维度 | Pi | OpenCode |
|---|---|---|
| 产品抽象 | canonical `off/minimal/low/.../max` | 任意命名的 `variant` |
| 能力表达 | `reasoning + thinkingLevelMap + compat` | `reasoning_options` 编译成 `variants` option objects |
| Provider 映射 | 自有协议 adapter 直接构造 wire payload | Provider transform 生成 AI SDK options，由 AI SDK 编码 wire |
| 非 reasoning variant | 主要围绕 thinking | 天然支持 fast、verbosity、customField 等任意预设 |
| 无效选择 | clamp 到合法档位 | 静默回 `default`，或选择模型时再弹 variant 对话框 |
| TUI | `/model` 与 `/thinking` 分离 | `/models` 与 `/variants` 分离，`Ctrl+T` 快速循环 |
| 持久化 | 全局/per-model setting + session change history | per-model `model.json` + message/session 中的 variant 名 |
| 审计 | 仍主要保存 level | 只保存 variant 名；effective options 也未冻结 |

Pi 更适合强约束的“Thinking Level”领域模型；OpenCode 更适合可扩展的“同一模型多套运行预设”。两者都没有实现用户截图里的同屏左右键 UI。

## 8. 对 Auto Studio 的可落地 Rust 设计

建议采用 **Pi 的强类型能力 + OpenCode 的可扩展 preset**，而不是二选一。

### 8.1 分成四个概念

```rust
pub enum ReasoningControl {
    Unsupported,
    Toggle { off_supported: bool },
    Effort { levels: Vec<ReasoningLevel> },
    TokenBudget { min: u32, max: u32 },
    AdaptiveEffort { levels: Vec<ReasoningLevel> },
}

pub enum ReasoningLevel {
    Off,
    Minimal,
    Low,
    Medium,
    High,
    XHigh,
    Max,
}

pub struct ModelPreset {
    pub id: PresetId,                 // 可为 high、fast、quality 等
    pub reasoning: Option<ReasoningSelection>,
    pub output: OutputOptions,
    pub provider_overrides: ProviderOverrides,
}

pub struct EffectiveReasoning {
    pub requested: Option<ReasoningSelection>,
    pub resolved: ResolvedProviderReasoning,
    pub capability_revision: String,
    pub mapping_revision: String,
}
```

其中：

- `ReasoningControl` 决定 TUI 可以显示哪些档位；
- `ModelPreset` 保留 OpenCode 的扩展性，但不允许任意 JSON 穿透 Core；
- `ResolvedProviderReasoning` 由 Provider adapter 强类型编码；
- `max_output_tokens` 始终独立，不再和 reasoning 混用。

### 8.2 UI 仍可实现目标交互

Auto Studio 已确定的页面可以保持：

```text
↑ / ↓  选择模型
← / →  只在当前模型的 supported reasoning levels 中切换
Enter  原子提交 model + level
```

但不要照搬 OpenCode 的“先选模型、再弹 variant”，因为 Esc 会产生半提交状态。建议在 dialog draft 中维护：

```rust
struct ModelSelectionDraft {
    model: ModelRef,
    reasoning: ReasoningSelection,
}
```

只有 Enter 才写入 Connection/Session。切换模型时：

1. 优先读取该模型的 per-model preference；
2. 不合法则采用 catalog default；
3. 没有 default 时采用向下优先的安全 clamp；
4. Unsupported 显示 `Provider default` 并禁用左右键；
5. Toggle 显示 `Off / On`，不要伪装成 `Low / High / Max`。

### 8.3 Provider adapter 必须输出 effective mapping

每次请求在 provenance 中保存：

```json
{
  "provider": "anthropic",
  "model": "claude-sonnet-4-6",
  "requested_level": "high",
  "effective_control": "adaptive_effort",
  "effective_level": "high",
  "effective_budget_tokens": null,
  "catalog_revision": "...",
  "mapping_revision": "..."
}
```

不要保存 API Key，也不必保存完整 raw request；保存经脱敏、稳定化的 effective reasoning facts 即可。这样模型目录升级后仍能解释旧工程为什么产生当时的结果。

### 8.4 建议测试矩阵

至少建立：

1. catalog fixture → supported levels；
2. `model + selection` → typed effective reasoning；
3. effective reasoning → Provider request body；
4. unsupported/off/always-thinking/default 的拒绝或 fallback；
5. TUI 上下/左右/Enter/Esc 的原子状态测试；
6. per-model preference 与 Session snapshot 恢复；
7. DeepSeek、OpenAI、Anthropic、Moonshot、Kimi Code 各一条真实测试环境 contract test；
8. catalog/mapping revision 进入 provenance。

## 9. 最终判断

OpenCode 最值得借鉴的是三点：

1. 能力落在 **exact model + transport**，不是 Provider 全局；
2. catalog 的 effort/toggle/budget 先转换成模型专属 options，再进入请求；
3. 同一模型可以有可扩展 presets，而不局限于 thinking。

不建议直接复制的部分也有三点：

1. `variants: Record<string, Record<string, any>>` 太弱类型，不适合作为 Auto Studio Core 的公开领域模型；
2. `default`、`none`、`off` 的语义容易混淆，且无效 variant 静默回 default；
3. 只保存 variant 名，无法满足 Auto Studio 对工程事实和可审计 provenance 的要求。

因此 Auto Studio 的最佳方案不是“照搬 OpenCode variants”，而是：**用强类型 Reasoning Capability 管合法性，用 typed Provider Encoder 保证 wire 正确，再在其上提供 OpenCode 风格的 Model Preset 扩展能力。**
