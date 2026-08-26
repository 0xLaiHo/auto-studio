# Auto Studio Music Creation Domain

本文只定义 Auto Studio 的共同语言，不描述具体代码、库或实施阶段。创作者拥有 Project、素材、内容许可证、插件关系和最终作品；Creative Agent 只能通过受控语义工具协作。

## 产品与参与者

**Creator（创作者）**：发起、审阅、修改并交付作品的人；对授权、采用、权利和最终导出拥有决定权。  
_Avoid_: Operator、Model、Agent

**Auto Studio（本地音乐创作工作站）**：让 Creator 通过对话与 Creative Agent 共同形成可编辑音乐工程的本地优先产品。  
_Avoid_: Music Provider Aggregator、Prompt-to-WAV Chat、Cloud DAW

**Local-first（本地优先）**：Project、音乐事实、内容目录、插件引用与媒体资产在 Creator 设备上仍可创建、编辑、试听和导出；外部 LLM 推理不取得工程所有权。  
_Avoid_: Fully Offline、Cloud Project

**Client Surface（客户端界面）**：Creator 查看工程、输入意图、授权、比较和导出的交互界面。多个 Client Surface 共享同一个 Project 事实源。  
_Avoid_: Business Runtime、Project Database

## 工程与音乐事实

**Project（项目）**：围绕一个可交付音乐作品组织的 Brief、音乐事实、素材、版本、运行记录和交付集合。  
_Avoid_: Conversation、Generation

**Project Package（项目包）**：可以复制、备份和迁移的 Project 整体；不包含 Provider Credential、可分离商业采样副本或 Creator 插件二进制。  
_Avoid_: Folder、Cloud Workspace

**Creative Brief（创作简报）**：从 Creator 意图和参考材料整理出的结构化目标，包括用途、风格、结构、乐器、情绪、约束和交付要求。  
_Avoid_: Prompt、Tool Request

**Music Project Model（音乐工程模型）**：Project 中可编辑音乐事实的统一模型，包含 Tempo、拍号、段落、轨道、Clip、MIDI、乐器分配、Mix 和自动化。它是音乐的权威来源。  
_Avoid_: Generated WAV、Prompt、Chat History、Provider Response

**Tempo Map（速度图）**：Tempo 与拍号到音乐时间和 sample time 的可复现映射。  
_Avoid_: BPM Field

**Arrangement（编曲结构）**：由 Section、时间范围、重复与过渡组成的作品结构。  
_Avoid_: Prompt Outline、Audio Segmentation

**Section（段落）**：具有稳定身份和小节范围的音乐结构单位，例如 Intro、Verse、Chorus 或 Bridge。  
_Avoid_: Timestamp Label

**Track（轨道）**：承载演奏、音频、路由、Mix 和自动化的工程实体。  
_Avoid_: Stem File

**Instrument Track（乐器轨）**：承载 MIDI Clip、Instrument Assignment、articulation、expression 和路由的 Track。  
_Avoid_: Generated Audio、Plugin Binary

**Audio Track（音频轨）**：承载 Audio Clip、增益、声像、路由、自动化和效果链的 Track。  
_Avoid_: Stem File

**MIDI Clip（MIDI 片段）**：带时间范围、note、controller、力度和可复现 humanize 参数的非破坏性演奏数据。  
_Avoid_: MIDI File、Prompt

**Instrument Assignment（乐器分配）**：Instrument Track 对稳定 Instrument/Preset 或已批准 Plugin Instance 的可复现引用。  
_Avoid_: File Path、Plugin Binary

**Mix Graph（混音图）**：由 Track、Bus、send、effect、Plugin Instance 和自动化构成的版本化信号关系。  
_Avoid_: Filter String、Mutable Callback State

**Project Revision（项目修订）**：Project 正式事实每次成功变化后的单调版本。过期 revision 产生冲突，不隐式覆盖。  
_Avoid_: Timestamp、UI Refresh Counter

**Project Snapshot（项目快照）**：一个不可变 Project Revision 及其内容、插件与渲染依赖引用。  
_Avoid_: Current Mutable Project、Chat Context

**Project Change Set（工程变更集）**：一次成功工程修改的结构化结果，说明新 revision、变更实体、受影响范围和可见摘要。  
_Avoid_: LLM Explanation、Raw Diff

## 候选、采用与交付

**Candidate（候选）**：尚未被 Creator 采用的 Project Snapshot，连同 Preview、分析、变更摘要和依赖记录。  
_Avoid_: Generated File、Final Output、Take

**Selection（采用）**：Creator 把一个 Candidate 设为正式工程方向的决定。  
_Avoid_: Approval、Like、Agent Decision

**Asset（资产）**：音乐、视频、图片、歌词、脚本或中间渲染物的稳定身份。  
_Avoid_: File、URL

**Asset Version（资产版本）**：Asset 的一次不可变内容版本，包含媒体、技术元数据、来源和创建记录。  
_Avoid_: Mutable File

**Preview Render（试听渲染）**：从 Project Snapshot 产生、用于比较和反馈的非权威音频。  
_Avoid_: Final Export、Web Audio Capture

**Authoritative Render（正式渲染）**：从固定 Project Snapshot 与依赖锁产生并记录完整 receipt 的正式音频。  
_Avoid_: Preview Capture、Current Playback

**Render Plan（渲染计划）**：从 Project Snapshot 编译出的不可变音频执行描述。  
_Avoid_: Shell Command、Live Project

**Render Receipt（渲染回执）**：证明某个 Render Plan 产生了特定 Asset Version 的不可变记录，包含格式、时长、hash 与依赖锁。  
_Avoid_: Log、Filename

**Freeze（冻结）**：把 Instrument Track 或插件链渲染为项目内 Audio Asset，同时保留可恢复依赖。  
_Avoid_: Destructive Bounce

**Export（交付包）**：由 Selection 对应 Project Snapshot 产生的 WAV、stems、MIDI、Tempo/marker、credits 和 manifest。  
_Avoid_: Download、Single WAV

**DAW Handoff Package（DAW 交接包）**：供 Creator 在目标 DAW 继续制作的 Export；不自动等同于某个 DAW 的原生工程文件。  
_Avoid_: Native DAW Project

**Portable Handoff（可移植交接）**：不依赖某个 DAW UI 或专有工程格式的交付层；至少包含 Type-1 Standard MIDI、轨道名、Tempo/拍号/marker、Bank Select/Program Change、乐器分配清单，并在完整 MVP 中加入 WAV/stems。它表达可重建的意图，不承诺各 DAW 使用自己的音源时声音完全一致。
_Avoid_: Bitwig Adapter、Universal Native Project

**Sound-identical Handoff（同声交接）**：通过冻结音频，或由目标 DAW 加载同一版本的 Auto Studio Sampler/VST3、内容包与 preset state，复现经过验证的声音。没有相同依赖时只能承诺 Portable Handoff。
_Avoid_: MIDI-only Exact Sound

**DAW Qualification（DAW 资格验证）**：把一个不可变 Handoff Package 与精确 DAW 版本、平台、可执行文件 hash、导入检查和继续编辑证据绑定的验证过程。未安装、未冻结版本或缺少截图/工程/edited MIDI 时只能是 `not_run` 或 `fail`，不能用 `SKIP`、Fixture 或其他 DAW 的结果代替 `pass`。
_Avoid_: Compatibility Claim、Import Once、DAW Support Boolean

## 内容、乐器与插件

**Content Pack（内容包）**：可独立安装和追踪的一组采样、映射、Preset、许可与来源清单。  
_Avoid_: Asset Folder、Free Download

**Factory Pack（出厂内容包）**：随产品提供、完成法律和质量批准的基础 Content Pack。  
_Avoid_: All Free Samples、Hidden Dependency

**Optional Pack（可选内容包）**：由 Creator 明确选择并按其许可获取的 Content Pack。  
_Avoid_: Auto Download、Factory Pack

**Instrument（乐器）**：Content Catalog 中具有稳定身份、音域、分层、articulation、来源和许可的可演奏实体。  
_Avoid_: Raw Sample、Plugin Binary

**Instrument Preset（乐器预设）**：Instrument 的可复现播放配置，不包含底层采样副本。  
_Avoid_: Sample Pack、Opaque State

**Instrument Manifest（乐器清单）**：描述 zone、velocity、round robin、articulation、loop、tuning、source 和 hash 的版本化映射。  
_Avoid_: Arbitrary Folder Scan、Compatibility Claim

**Content Catalog（内容目录）**：只暴露已批准、已安装或可合法获取的 Pack、Instrument 和 Preset。  
_Avoid_: File Scanner、Internet Search

**Content License Record（内容许可记录）**：绑定精确内容版本、hash、许可证文本、来源、署名和批准事实的记录。  
_Avoid_: URL、License Name

**Pack Lock（内容包锁）**：Project Snapshot 对 Pack、Instrument、版本和文件 hash 的依赖清单。  
_Avoid_: Installed Latest Version

**VST3 Plugin（VST3 插件）**：通过 VST3 格式提供乐器或效果处理的第三方或自有插件。  
_Avoid_: VST、Built-in Effect

**User-owned Plugin（用户自有插件）**：由 Creator 自行安装并授权使用的 VST3 Plugin；Auto Studio 不重新分发其二进制或资源。  
_Avoid_: Bundled Plugin

**Plugin Catalog（插件目录）**：由已批准发现结果形成的 Plugin 身份、版本、能力、位置和 Trust Status 集合。  
_Avoid_: Plugin Folder、Marketplace

**Plugin Profile（插件语义档案）**：把 Plugin preset、参数、I/O 和已知限制映射为 Agent 可理解语义和安全范围的档案。  
_Avoid_: Prompt、Raw Plugin Metadata

**Plugin Instance（插件实例）**：Project 中某个 Plugin 的具体使用，绑定位置、I/O、参数、state 和延迟。  
_Avoid_: Plugin Binary

**Plugin Trust Status（插件信任状态）**：对某个精确 Plugin 版本记录的 Discovered、Inspected、Approved、Limited、Denied 或 Quarantined 状态。  
_Avoid_: Enabled、Installed、Safe

**Plugin Lock（插件锁）**：Project Snapshot 对 Plugin UID、版本、binary hash、Profile、state 和兼容结论的依赖清单。  
_Avoid_: Installed Plugin List

## Creative Agent 与工具

**Creative Agent（创作代理）**：基于 Project Context 理解目标、形成音乐决定、请求 Semantic Tool、观察结果并提出下一步的 AI 协作者。  
_Avoid_: Chatbot、Model、Agent Swarm

**Agent Model（代理模型）**：Creative Agent 用于理解、音乐决策和 Tool 请求的具体 LLM。它不直接拥有 Project，也不等于音乐文件生成后端。  
_Avoid_: Agent、Music Provider、Project

**Agent Run（代理运行）**：Creative Agent 为一个创作目标执行的可暂停、可恢复、可审计活动。Run identity 与 `planning` phase 必须在首次 Provider 调用前成为耐久 Project 事实；恢复时每个 Agent Step 从 Project 与 Inference Transcript 重新派生。若只存在已准备的 Provider Turn 而没有耐久输出，Run 必须明确记录 `InferenceInterrupted`，不得把自动重提当作恢复。
_Avoid_: Chat、Generation Job

**Agent Step（代理步骤）**：Agent Run 中的一次理解、计划、Tool 请求、结果观察、提议或等待。  
_Avoid_: Token、Private Reasoning

**Context Snapshot（上下文快照）**：某个 Agent Step 使用的不可变 Brief、Project facts、相关对话和 Tool Result 集合。  
_Avoid_: Prompt String、Entire Project

**Context Manifest（上下文清单）**：某个 Inference Turn 在调用 Provider 前持久化的不可变审计记录，绑定 exact instructions、included Inference Item、Tool/输出合同、Provider/Model/Protocol/Thinking、token budget、Project revision 与内容 hash；它描述“模型实际被允许看到什么”，但不是 Project 事实。
_Avoid_: Prompt Cache、Project Snapshot、Provider Continuity State

**Agent Decision（代理决策）**：Agent Step 输出的结构化可见内容或下一步 Tool 意图；不包含模型的私有推理过程。  
_Avoid_: Chain of Thought

**Semantic Tool（语义工具）**：面向音乐或工程意图的受控能力，例如 `midi.write_region`、`instrument.assign`、`render.preview`。  
_Avoid_: Crate Function、Shell Command、Plugin ABI

**Tool Descriptor（工具描述符）**：一个 Semantic Tool 的不可变注册事实，包含名称、版本、schema、影响、授权、重放、资源限制和能力指纹。  
_Avoid_: Prompt Description、Function Name

**Tool Registry（工具注册表）**：保存当前可用 Tool Descriptor Snapshot 并解析 ToolRequest 的逻辑集合。发现一个外部 Tool 不表示自动信任或开放。  
_Avoid_: Plugin Marketplace、Global Mutable Map

**Tool Request（工具请求）**：Agent 对一个 Semantic Tool 提出的结构化、尚未执行的意图。  
_Avoid_: Project Fact、Raw Command

**Tool Execution（工具执行）**：一次绑定确定输入、Project Revision、权限、Approval Grant、预算和结果的 Semantic Tool 执行。  
_Avoid_: Function Call、Inference Turn

**Tool Result（工具结果）**：Tool Execution 产生的受限结构化结果，包含 Project Change Set、Asset reference、receipt 或清洗诊断。  
_Avoid_: Raw stdout、Media Bytes、Absolute Path

**Approval（授权）**：Creator 对某组工具影响、内容安装、插件使用、权利敏感或导出动作给予的明确许可。  
_Avoid_: Selection、Generic Confirmation

**Approval Grant（授权凭据）**：Approval 的不可变执行凭据，绑定精确 Project Revision、Plan、Tool Descriptor fingerprint、目标范围、允许的副作用数量与费用上限；请求超出任一绑定条件时失效。  
_Avoid_: Session-wide Permission、Run Budget、Checkbox

**Run Budget（运行预算）**：Core 为一个 Agent Run 强制执行的 turns、tools、tokens、cost、wall-clock、render 和资源总上限。它独立于 Approval Grant，Creator 和 LLM 都不能把它提高到系统上限之外。  
_Avoid_: Approval、Provider Quota、Tool Resource Limit

**Tool Resource Limit（工具资源限制）**：Tool Descriptor 对单次 ToolExecution 声明的输入规模、目标数量、CPU、内存、时长、输出和并发上限。  
_Avoid_: Run Budget、Approval Grant、Best-effort Hint

**Run Event（运行事件）**：Agent Run 中已经发生的状态变化或重要结果的按序事实。  
_Avoid_: UI Notification、Debug Log

**Run Projection（运行投影）**：从 Project Snapshot 和 Run Event 折叠得到的客户端读取模型。  
_Avoid_: Project Truth、Activity Text

## LLM Provider

**LLM Provider（LLM 供应商）**：通过 Creator 自己的账户和凭证提供 Agent Model 推理能力的外部主体。它不生成或拥有 Auto Studio 音乐资产。  
_Avoid_: Music Provider、Agent、Tool

**LLM Connection（LLM 连接）**：Creator 为某个 LLM Provider 配置的 Credential、Model 和连接偏好；它独立于 Project Package。  
_Avoid_: Music Connection、Project Credential

**Provider Credential（供应商凭证）**：LLM Connection 所需的秘密身份材料。  
_Avoid_: Project Setting、Readable API

**Model Catalog（模型目录）**：一个 LLM Connection 当前可选择的精确模型、协议、可用性和能力集合。  
_Avoid_: Static Model Enum、Marketing Page

**Thinking Level（思考级别）**：Creator 为当前 Agent Model 选择的推理偏好；合法集合由精确 Provider、Model 和协议能力决定。  
_Avoid_: Chain of Thought、Reasoning Trace

**Inference Turn（推理轮次）**：Agent Model 基于 Context Snapshot 产生可见内容、Tool Request 或结束原因的一次请求与响应尝试。  
_Avoid_: Agent Run、Tool Execution

**Inference Transcript（推理记录）**：Agent Run 内持久化的规范化语义记录，由有序 Inference Item 构成，保存 Creator 可见消息、完整 Tool Request/Result、usage 与 finish reason；它可以进入恢复上下文，但不是 Project 事实。  
_Avoid_: Raw Provider Payload、Project Event、Private Reasoning

**Inference Item（推理条目）**：Inference Transcript 中一个有稳定身份和顺序的规范化条目，例如 Visible Message、Tool Request、Tool Result、Usage 或 Finish；partial tool call 必须组装并验证后才能成为完整条目。  
_Avoid_: Token Chunk、Provider JSON、Agent Step

**Provider Continuity State（供应商连续性状态）**：为继续同一条 Provider/Model/Protocol 推理链而保存的、Provider Adapter 所拥有的 opaque payload 或引用，例如 OpenAI response/reasoning item 或 Anthropic signed thinking block。它只在 Agent Run 内存在，Creator 不可见，不进入 Project、Event、日志、compaction 或 Export，终态语义提交后删除。  
_Avoid_: Inference Transcript、Private Reasoning Log、Project Snapshot、Cross-provider Context

**Continuity Reference（连续性引用）**：可写入 Context Manifest 的非秘密收据，记录 state identity、source turn、binding hash 和有效期，但不包含 Provider payload、private reasoning 或 Vault path。
_Avoid_: Provider Continuity State、Credential、Project Fact

**Continuity Vault（连续性保险库）**：位于 Project Package 外、按 Run 保存加密 Provider Continuity State 的短生命周期本地存储。它只向精确 binding 匹配的 Adapter 返回 payload，并负责 TTL、错配/损坏清理与终态 purge。
_Avoid_: Project Store、Inference Transcript、Long-term Memory

**Unknown Consumption（消耗未知）**：LLM Inference Turn 可能已产生 token 费用，但本地尚不能确认完整 usage。它不表示 Music Project 已改变。  
_Avoid_: Tool Completed、Project Mutation

## MCP

**MCP Server Connection（MCP 服务器连接）**：Creator 在 Project 外主动配置的外部 MCP Server 身份、transport、协议、Credential reference、trust policy 和 Tool allowlist。  
_Avoid_: LLM Connection、Project Dependency

**MCP Tool（MCP 工具）**：经发现、校验和 allowlist 后注册的外部 Semantic Tool。它的描述和结果始终是不可信输入。  
_Avoid_: Built-in Tool、Automatically Trusted Tool

**MCP Capability Snapshot（MCP 能力快照）**：绑定 Server、协议、发现时间、Tool Descriptor fingerprint 和 trust decision 的不可变观察事实。  
_Avoid_: Permanent Support Claim

## 权利与质量

**Rights Declaration（权利声明）**：Creator 对输入素材、声音、歌词和参考作品来源与允许用途的声明。  
_Avoid_: Disclaimer Checkbox

**Provenance Record（来源记录）**：连接 Rights Declaration、LLM/Tool、Project Snapshot、Content Pack、Plugin 和输出版本的可追溯记录。  
_Avoid_: Debug Log

**Quality Evaluation（质量评估）**：围绕 Brief 匹配、音乐性、结构、演奏真实性、技术质量和专业可编辑性形成的机器检查、盲听与 Creator 评价。  
_Avoid_: One Score、Marketing Claim
