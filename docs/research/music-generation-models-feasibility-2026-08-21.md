# 音乐生成 Provider 接入可行性调研（2026-08-21）

> **历史路线，已不进入当前产品。** [ADR-0011](../adr/0011-llm-authored-local-music.md) 已决定由 LLM 通过本地 Music Project/MIDI/Sampler/Audio Engine 创作音乐，不实现 Music Provider Adapter。本文仅保留 2026-08-21 当时的证据，不能作为当前 backlog、支持矩阵或发布 Gate。  
> 状态：superseded research snapshot。  
> 适用范围：Auto Studio 桌面应用、用户自有 Provider Credential、托管 API、内容质量优先。  
> 不适用范围：Auto Studio 云端转售、统一生成额度、自部署模型或 GPU Worker。

## 1. 调研口径

- 只把官方 API 文档、模型卡、价格与许可条款视为产品事实来源。
- “模型能力”不等于“目标用户账户已经开放的能力”；Adapter 必须验证账号、区域、模型和配额。
- “BYOK”不自动消除第三方客户端、OEM、品牌、数据处理和媒体用途限制。
- 厂商的“高质量”“专业级”和自报指标不等于 Auto Studio 的质量结论。
- 本报告没有替代 60 条黄金任务的同条件盲听、分轨泄漏、歌词可懂度和 DAW 交付测试。
- 价格、Preview 状态和条款可能变化；连接页应链接 Provider 当前页面，生产 Adapter 保存验证日期。

## 2. 当前接入筛选条件

一个模型进入本地 BYOK MVP 候选，必须满足：

1. 有官方或正式授权的托管接口；
2. 创作者能够配置自己的 Credential 和账户范围；
3. 桌面 Runtime 可以通过 HTTPS/流式协议调用，不需要 Auto Studio 云端中转；
4. 结果能够下载并保存到本地 Project Package；
5. 任务状态、同步模糊结果或重试风险可以被 Adapter 表达；
6. 第三方客户端和目标媒体使用符合 Provider 条款；
7. 不要求 Auto Studio 部署模型或 GPU 推理基础设施。

## 3. 结论摘要

| Provider / Model | BYOK 技术可行性 | MVP 定位 | 当前主要阻断 |
|---|---|---|---|
| Mureka | 高 | 首个异步音乐 Adapter 候选 | 数据条款、模型固定、音频规格需确认 |
| Google Lyria 3 | 高 | 托管质量对照、歌词/图片到音乐 | Preview、区域/项目凭证、缺少局部编辑 |
| Eleven Music v2 | 高但有条件 | 完整歌曲、结构计划、inpainting、stems | pure-play/OEM/品牌与媒体条款 |
| Stable Audio 3 Large API | 待 PoC | 器乐、配乐、音效与编辑候选 | 当前 API 能力面、任务语义和数据设置 |
| ACE-Step 1.5 | 不符合当前范围 | 不进入 MVP | 官方路径以自部署为主，产品明确不自部署 |
| Stable Audio 3 open weights | 不符合当前范围 | 不进入 MVP | 需要本地/云端推理与模型分发 |

推荐顺序：

1. 用 **Mureka** 验证异步 Job、轮询、下载、Unknown Outcome 和本地资产闭环；
2. 用 **Lyria 3 Pro** 建立第二个质量对照和不同 Credential 形态；
3. **Eleven Music v2** 在第三方客户端与品牌条款通过后接入；
4. **Stable Audio 3 Large API** 在公开接口 PoC 明确后评估器乐和编辑；
5. ACE-Step 与 Stable Audio 开放权重仅作为未来“本地模型”决策的研究输入，不进入当前 Adapter backlog。

顺序不预设最终质量赢家。生产路由必须由统一黄金任务和真实创作者 Selection 决定。

## 4. 能力与本地接入对比

| 维度 | Eleven Music v2 | Mureka | Google Lyria 3 | Stable Audio 3 Large API | ACE-Step 1.5 |
|---|---|---|---|---|---|
| 当前形态 | 托管 REST/SSE | 托管 REST + polling | Gemini/Vertex 托管 | Stability 托管 API | 开源权重、本地 API |
| BYOK 凭证 | API Key/账户权限 | Bearer Token/API Key | API Key 或项目/云身份 | Platform Credential | 无官方托管 BYOK 路径 |
| 完整人声歌曲 | 支持 | 支持 | Pro/Clip 支持 | 非主要定位 | 支持但需自部署 |
| 指定歌词 | 支持 | 支持 | 支持 | 不适合作为主链路 | 支持 |
| 参考音频 | 支持，账户与权利需验证 | 支持多种参考 | 不支持 | audio-to-audio 需 PoC | 支持但需自部署 |
| 局部编辑/续写 | inpainting/extend | region edit/extend/remix | 不支持 | 家族支持，API 开放面需验证 | repaint/cover 等 |
| 分轨 | 2/6 stems 后处理 | 多种分轨/单轨接口 | 不支持 | 无原生分轨定位 | extract/lego 需自部署实测 |
| 任务形态 | 同步长连接/SSE | 原生异步 task id | 单轮返回；Realtime 另算 | 需 PoC | 本地异步队列 |
| 本地恢复难度 | 高：断线可能结果未知 | 低：external task id 可轮询 | 中：按接口类型处理 | 未知 | 不适用当前范围 |
| 当前判断 | 条款通过后采用 | 首个 Adapter 候选 | 第二候选 | Defer / PoC | Reject for MVP |

## 5. 分 Provider 结论

### 5.1 Mureka

**已开放能力**

- Bearer Token REST API；用户创建 API Key、充值后可调用。[Quickstart](https://platform.mureka.ai/docs/en/quickstart.html)
- Lyrics-to-song 支持歌词、prompt、性别、模型、参考曲、人声音色和旋律输入；部分输入组合受模型限制。[Song generation](https://platform.mureka.ai/docs/api/operations/post-v1-song-generate.html)
- 提供纯音乐、局部编辑、续写、Remix、分轨、单轨和画面配乐端点。[Region Edit](https://platform.mureka.ai/docs/api/operations/post-v1-song-region-edit.html)、[Extend](https://platform.mureka.ai/docs/api/operations/post-v1-song-extend.html)、[Stem](https://platform.mureka.ai/docs/api/operations/post-v1-song-stem.html)
- 原生异步 task id 和状态轮询，适合本地应用关闭后重新对账。[Task query](https://platform.mureka.ai/docs/api/operations/get-v1-song-query-%7Btask_id%7D.html)
- 结果包含普通/WAV/FLAC URL，但公开资料未固定采样率、位深和响度，需要下载样本实测。[Changelog](https://platform.mureka.ai/docs/en/changelog.html)

**BYOK 适配判断**

Mureka 最适合作为第一个真实 Adapter，因为用户 Credential、异步任务和编辑能力都能覆盖核心 Job Runner 场景。连接验证必须区分鉴权、余额、模型权限和具体 Capability；结果成功后立即下载到本地 Project Package。

API Agreement 允许在 Customer Application 使用 API，但声音、参考音频、训练/保存、地域和删除仍需形成当前结论。[API Service Agreement](https://platform.mureka.ai/service_terms.pdf)、[Privacy Policy](https://platform.mureka.ai/privacy_policy.pdf)

### 5.2 Google Lyria 3

**已开放能力**

- Gemini API 提供 Lyria 3 Clip 和 Pro Preview，支持文本、图片、自定义歌词和结构提示；Pro 可请求 WAV。[Gemini API music generation](https://ai.google.dev/gemini-api/docs/music-generation)
- 当前不支持参考音频、多轮迭代、局部编辑、续写或分轨。[Lyria 3 model card](https://docs.cloud.google.com/gemini-enterprise-agent-platform/models/lyria/lyria-3)
- Realtime 是独立的实验型器乐 WebSocket，不替代完整歌曲生成。[Realtime music generation](https://ai.google.dev/gemini-api/docs/realtime-music-generation)
- 官方 SDK 覆盖 JavaScript，同时提供 REST；当前未列官方 Rust SDK。Rust Adapter 通过 Reqwest 直接实现 REST，并承担 schema、认证和错误漂移测试。

**BYOK 适配判断**

Lyria 适合作为低集成成本的托管质量对照，但 Provider Connection 不能只画一个 `apiKey` 输入框：Gemini API 和 Vertex/Cloud 项目可能涉及不同身份、项目、区域和配额。Adapter Manifest 应按连接模式声明字段。

Preview/Experimental 状态、区域和数据规则必须在 Connection 页面可见；不要因为 Google 品牌把它预设为主模型。[Gemini API terms](https://ai.google.dev/gemini-api/terms)、[Vertex AI data retention](https://docs.cloud.google.com/vertex-ai/generative-ai/docs/vertex-ai-zero-data-retention)

### 5.3 Eleven Music v2

**已开放能力**

- Compose API 支持 prompt 或 composition plan、器乐开关和多种输出。[Compose API](https://elevenlabs.io/docs/api-reference/music/compose)
- Composition Plan 支持分段歌词、时长、正负风格和上下文控制，并有 TypeScript 示例。[Composition plans](https://elevenlabs.io/docs/eleven-api/guides/how-to/music/composition-plans)
- Detailed Stream 通过 SSE 返回计划、metadata、音频块和可选词级时间戳，但不是可恢复的异步任务账本。[Detailed stream](https://elevenlabs.io/docs/api-reference/music/compose-detailed-stream)
- v2 inpainting 支持局部替换、头尾延长和 loop；上传参考会做版权检查。[Inpainting guide](https://elevenlabs.io/docs/eleven-api/guides/how-to/music/inpainting)
- 提供 2/6 stems 后处理接口；它们必须标记为 separation，不是原生录音多轨。[Stem separation](https://elevenlabs.io/docs/api-reference/music/separate-stems)

**BYOK 适配判断**

技术可行性高，但生产准入有条件。官方 Music API Terms 对 Pure-play Music AI Creation Company、转售和品牌露出有专门要求；Auto Studio 即使不代收生成费用，仍是第三方音乐创作客户端，不能假设用户 API Key 自动解决这些限制。[Music API Terms](https://elevenlabs.io/music-api-terms)、[Model-specific terms](https://elevenlabs.io/eleven-music-model-specific-terms)

同步/SSE 请求断开时可能无法确认是否已经生成或计费。Adapter 必须把这类 Attempt 标为 Unknown Outcome，并在可用时通过历史记录或用户确认对账。

### 5.4 Stable Audio 3 Large API

Stable Audio 3 家族包含开放权重和 Large API。当前产品只考虑 Large 托管接口，开放权重不进入 MVP。[Stable Audio 3 announcement](https://stability.ai/news-updates/meet-stable-audio-3-the-model-family-built-for-artistic-experimentation-with-open-weight-models)

它适合器乐、配乐、音效和 audio-to-audio 方向，不适合作为完整歌词人声主链路。公开资料不足以固定当前 Large API 的全部编辑端点、任务查询、失败计费和输出契约，因此在进入 Adapter backlog 前必须完成账号 PoC。[API release notes](https://platform.stability.ai/docs/release-notes)、[Platform pricing](https://platform.stability.ai/pricing)

生产连接还应检查训练 opt-out、输入保存和许可证义务。[Terms of Service](https://stability.ai/terms-of-service)、[API training opt-out](https://kb.stability.ai/knowledge-base/opt-out-of-data-training-for-platform-api)

### 5.5 ACE-Step 1.5

ACE-Step 官方发布的是开源权重、Python 推理栈和本地 API，能力包括 text-to-music、reference、cover 和 repaint。[Model card](https://huggingface.co/ACE-Step/Ace-Step1.5)、[Official repository](https://github.com/ace-step/ACE-Step-1.5)

这些能力仍有研究价值，但与“当前不自部署模型、只接用户自有托管 Provider”冲突。没有官方或正式授权的托管 BYOK 接口前，ACE-Step 不进入 MVP Provider Registry，也不为它引入 Python/GPU Worker。

未来若第三方托管 ACE-Step，需要把“Provider”与“模型”分开重新评估：Credential、SLA、数据、结果权利和 API 契约以托管方为准，不能只继承开源模型许可结论。

## 6. 独立 Core Provider Adapter 架构

```text
Auto Studio Core
  ├─ Creative Agent Runtime
  ├─ Shared Provider Core
  │   ├─ Connection / Credential Lease
  │   ├─ Model Catalog / Capability Snapshot
  │   └─ Usage / Error / Diagnostics
  ├─ LLM Inference Module
  ├─ Media Generation + Local Job Runner
  │   └─ Music Provider Adapters
  ├─ Device Credential Store
  ├─ Project SQLite
  └─ Asset Sink → Project Package
```

Provider 调用由独立 Rust Core 统一承担，TUI、GUI 和未来 Web 不直连 Provider。Core 使用 Reqwest、Serde 以及 Adapter 内的 SSE/WebSocket 实现；官方 JavaScript/TypeScript 示例只作为协议参考，不为单个供应商增加 Node、Python 或 Go Sidecar。

这里的音乐 Provider Adapter 不与 Agent Model 共用一个万能执行接口。两者只共享 Provider/Connection/Model/Protocol 身份、Credential、Catalog、Capability Snapshot、usage 与错误语言；LLM 使用流式 Inference Turn，音乐生成使用可对账、下载并提交 Asset 的 Generation Job。边界见 [ADR-0006](../adr/0006-separate-llm-inference-from-media-generation.md) 与 [Pi Agent Provider 适配调研](./pi-agent-provider-adapter-design-2026-08-21.md)。

Rust Core 已由 [ADR-0004](../adr/0004-rust-core-professional-audio-engine.md)确认。选择 Rust 会损失部分官方 SDK 跟进速度，因此每个 Adapter 必须维护 recorded fixture、live account contract、schema drift、认证、费用/限流头和流式断线测试；Provider 类型不能穿透统一生命周期与 Capability 语义。

统一外部生命周期：

```text
validate connection
  → estimate
  → submit
  → reconcile / poll / stream
  → fetch artifacts
  → local hash + probe
  → commit Asset Version
```

Adapter 必须声明 Capability，而不是把所有 Provider 压成最低公共参数。音乐请求至少覆盖：

- 歌词、section、语言、BPM、key、拍号和目标时长；
- reference 类型、Rights Declaration 和允许上传区间；
- edit range、保留区间和 extend 方向；
- stem/track、输出格式和时间戳需求；
- Provider/model version、seed、Candidate 数和费用估计；
- Provider 专有参数的命名空间扩展。

Provider Credential 由设备级 Vault 保存，Project Package 只记录 Provider、模型、Adapter version 和非秘密调用事实。

## 7. BYOK 特有风险

### 7.1 账户差异

不同用户可能有不同模型、区域、余额、并发和实验能力。Capability Registry 必须带 `connection_id` 和 `observed_at`，不能把开发者测试账户结果当成所有用户能力。

### 7.2 费用可见性

Auto Studio 只能提供估计并记录 Provider 可返回的用量；最终扣费以用户 Provider 账户为准。连接页应提供账单与配额入口，不实现统一余额。

### 7.3 支持与诊断

错误必须区分 Credential 无效、模型无权、区域不支持、余额不足、限流、数据审核和 Provider 故障。Diagnostics 不能包含 Credential 或完整作品内容。

### 7.4 数据发送

每次 Tool Plan 在 Approval 前生成 Data Transfer Summary。Adapter 只能读取批准的 Asset Version 和区间，不能遍历 Project Package。

## 8. 接入阶段

### Phase 0：准入与连接契约

1. 选择一个 Agent Model Provider 和 Mureka 测试账户；
2. 固定 Credential schema、Connection validation 和错误分类；
3. 明确第三方客户端、数据、品牌和媒体用途；
4. 建立 Fake Provider fixtures，不让 CI 调用真实计费接口。

### Phase 1：Mureka 纵向闭环

- Connection → capability → estimate → submit → poll → download；
- 应用在提交、轮询和下载阶段退出后恢复；
- 两个 Candidate 保存、probe、A/B、Selection 和 Export；
- 原始 URL 过期后项目仍完整。

### Phase 2：Lyria 质量对照

- 支持不同 Credential 模式；
- 相同 12 条冒烟任务、格式检查和盲听；
- 不提供的 reference/edit/stems 能力明确显示 unavailable。

### Phase 3：专业编辑候选

- Eleven 在条款通过后接入 composition plan、inpainting 和 stems；
- Stable Large 在 API PoC 通过后接入器乐/audio-to-audio；
- 逐项验证 region edit、extend、reference、stems 和 WAV 交付，而不是一次宣称“专业编辑”。

## 9. Acceptance Gates

### A. 产品与条款

- 允许第三方本地创作客户端使用，品牌和媒体义务明确；
- BYOK、Input/Output 权利、非唯一性和用户责任有当前说明；
- Provider Connection 页面能链接账户、配额、账单和数据设置。

### B. Credential 与数据

- Credential 只通过 Vault Lease 进入 Adapter；
- Renderer、Project、Export、日志和 Diagnostics 无秘密；
- 数据上传范围、训练、保存、删除和区域对用户可见；
- 敏感 reference/voice/lyrics 绑定 Rights Declaration 和 Approval。

### C. 技术可靠性

- 记录 Adapter version、Provider model、external job、request hash、价格快照和 output checksum；
- 实测限流、断线、休眠、重启、取消、URL 过期和重复提交；
- Unknown Outcome 不自动创建新计费 Attempt；
- 所有成功媒体落入 Project Package，Provider URL 不是项目事实。

### D. 音频工程

- 验证格式、采样率、位深、响度、削波、编码伪影和 stems 重组；
- Export 可被目标 DAW 打开；
- 时间线、歌词、section、BPM/key 和 provenance 不依赖 Provider 页面。

### E. 内容质量

使用同一批 60 条黄金任务和创作者盲评：

- 首要指标是可进入下一步制作的 Candidate 比例；
- 测试歌词准确度、语言自然度、人声、结构、情绪和音质；
- 编辑测试未选区保持、边界连续、reference 可控性和分轨泄漏；
- 统计每个可用 Candidate 的 Provider 真实成本和端到端时延；
- 厂商自述只能作为待验证声明，不能成为上线结论。

## 10. 最终决策

**Adopt for first implementation**

- Rust 独立 Core 内的共享 Provider Core、独立 Media Generation Adapter 与 Local Job Runner；
- Mureka 作为首个异步音乐 Adapter 候选；
- Lyria 3 Pro 作为第二质量对照候选；
- 所有结果立即下载到本地 Project Package。

**Conditional**

- Eleven Music v2：第三方客户端、pure-play、品牌和媒体条款通过后采用；
- Stable Audio 3 Large API：实际端点、任务语义、数据和输出 PoC 通过后采用。

**Reject for current MVP**

- ACE-Step 1.5 自部署；
- Stable Audio 3 开放权重自部署；
- 为模型供应商引入 Python GPU Worker；
- Rust Core + Node/Python/Go Provider Sidecar；
- Auto Studio 代管 Provider Credential、转售生成额度或用云端代理所有请求；
- 以单一主模型覆盖所有音乐任务。

## 11. 相关资料

- [产品设计文档](../product/ai-creative-agent-product-design.md)
- [技术设计文档](../design/auto-studio-technical-design.md)
- [当前 Roadmap](../roadmap.md)
- [独立 Core 与 Rust 可行性评估](./rust-core-service-feasibility-2026-08-21.md)
- [本地优先 BYOK ADR](../adr/0001-local-first-byok-desktop.md)
- [独立本地 Core ADR](../adr/0002-independent-local-core-service.md)
- [Rust Core 与专业 Audio Engine ADR](../adr/0004-rust-core-professional-audio-engine.md)
- [分离 LLM 推理与媒体生成 ADR](../adr/0006-separate-llm-inference-from-media-generation.md)
- [Pi Agent Provider 适配调研](./pi-agent-provider-adapter-design-2026-08-21.md)
