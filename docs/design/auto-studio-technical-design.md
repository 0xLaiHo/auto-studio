# Auto Studio 技术设计文档

> 基线日期：2026-08-25
> 目标：由真实 LLM 驱动本地音乐工具，产生可编辑 Music Project 与本地渲染音频  
> 当前事实：Core/TUI/Project/SQLite/LLM Connection 与 Planning Turn 已实现；Q0 v2 独立实验已完成真实 DeepSeek、多轮可恢复生成、严格 ExperimentalMusicSpec、Type-1 SMF MIDI 与 11/12 机器 Gate；Q0 v3 已完成逐 Run 协议绑定、任意已落盘 Mode B 回合恢复、一次受限资源预算修订、严格验证和真实 6/6 L4 重基线。Q0 不属于 production runtime，真人/DAW Gate 尚未完成；Agent Harness Foundation、Tool Registry、通用 Agent Tool loop、Music Project Model、Sampler、Audio Engine、Factory Pack 和 VST3 Host 尚未实现。现有 `GenerationAdapter` 与确定性 WAV Fixture 是旧方向的测试代码，不属于目标 production runtime。

## 1. 决策摘要

Auto Studio 采用单一 Rust Core。LLM 通过 BYOK Provider 完成理解、规划和音乐决策，再调用 Core 注册的 Semantic Tool；所有作曲、MIDI、乐器、DSP、渲染、分析和 Project 提交发生在本机。

| 关注点 | 决策 |
|---|---|
| 音乐生成 | LLM 生成结构化音乐决策并修改本地 Music Project，不调用 Music Provider |
| 外部依赖 | 只有 LLM Inference 需要外部 Provider；Project、内容、插件和音频执行在本地 |
| Agent | 单 Agent、有界多轮 Tool loop；不采用角色式 Multi-Agent |
| 推理状态 | 可审计 Inference Transcript 与 opaque Provider Continuity State 分开保存 |
| 授权与上限 | Approval Grant、Run Budget、Tool Resource Limit 分开判定 |
| 工具 | LLM 只调用版本化 Semantic Tool，不接触 crate 函数、Shell、路径、SQL 或 VST3 ABI |
| 工程事实 | Music Project Model、Project Snapshot、Event 和 SQLite 是事实源，聊天不是 |
| 音频 | Rust Audio Engine 编译不可变 Render Plan；M3 先离线渲染，实时 callback 后续按同一 graph semantics 进入 |
| 音色 | Factory Pack + Sampler 保证基础路径；VST3 是隔离、Profile 约束的 MVP 专业路径 |
| 客户端 | `autostudio` TUI 为首发入口，GUI/Web 通过相同 Core Interface 接入 |
| MCP | 以后作为受控 Tool Adapter 接入；不参与基础音乐闭环，也不能替代本地音乐工具 |

最重要的技术边界是：

> LLM 可以决定“写什么音乐”，但只有 Core 可以决定“这个工具请求是否有效、是否允许、如何改变工程，以及如何渲染”。

## 2. 总体架构

### 2.1 给非技术读者的解释

可以把系统理解为一间由 AI 协作的本地录音室：

- **LLM** 是作曲人与制作助理，提出结构、旋律、配器和混音决定；
- **Tool Runtime** 是录音室管理员，检查每个决定是否合法、是否得到授权；
- **Inference Transcript** 是可审计的工作记录；**Continuity Vault** 是只为继续当前 Provider 链而存在的加密临时封套；
- **Music Project Model** 是总谱和工程文件，保存所有可编辑音乐事实；
- **Instrument Runtime** 是乐手与音色，包括 Factory Pack、Sampler 和已批准 VST3；
- **Rust Audio Engine** 是调音台与渲染系统，把工程变成可以听到的音频；
- **Project Package** 是保险柜，保存版本、资产、来源和恢复记录。

外部 LLM 不返回最终音乐文件，也不能直接碰 Project。它只返回经过 schema 约束的 ToolRequest。

### 2.2 架构图

![Auto Studio Durable Agent Harness 架构：可见推理记录、Provider 连续性状态、受控 Tool Runtime 与本地 Music Project 分离；唯一云端依赖是 BYOK LLM](assets/agent-harness-architecture.png)

可交互查看、搜索并切换视图：[Agent Harness 架构图](agent-harness-architecture.html)。

图中的主路径是：

```text
Creator
  → TUI / GUI / Web
  → Core Interface
  → Agent Harness
  ↔ LLM Inference / BYOK LLM
  → Approval Grant + Run Budget
  → Tool Runtime
  → Music Project Model
  → Rust Audio Engine
  → Project Package
```

Agent Harness 同时把规范化条目写入 Inference Transcript，并让 Provider Adapter 把 opaque continuity 写入 Project 外的 Continuity Vault。两者不能互相替代。`BYOK LLM` 是唯一跨出设备的必需连接；图中没有 Music Provider、远端音乐 Job 或 prompt-to-WAV fallback。

## 3. 当前实例化架构与目标架构

必须区分“代码已经存在”和“目标设计已经确定”。

### 3.1 当前代码证据

| 能力 | 状态 | 证据边界 |
|---|---|---|
| Rust workspace 与独立 Core | `PASS` | Axum Core、版本化本机 API、discovery/session |
| Project/SQLite/revision/event/outbox | `PASS` | Project 创建、打开、提交、备份与恢复测试 |
| TUI `/connect`、`/model`、Thinking、`/exit` | `PASS` | Ratatui reducer/UI 与 Core Connection 合同 |
| LLM Adapter | `PASS（contract）` | OpenAI/Anthropic/DeepSeek 等协议合同；真实计费需对应 Key |
| LLM Planning Turn | `PASS（contract）` | typed Plan 与 Approval 已接 production composition root |
| Q0 实验 Harness | `PASS（v2/v3 machine）` / `LIVE-PENDING（human）` | 真实 DeepSeek V4 Pro、Mode A/B/C、逐轮落盘/任意已落盘 B 回合恢复、strict spec、SMF compiler；v3 protocol binding、受限资源修订与 formal verifier 通过，真实 L4 6/6 valid + compiled |
| Inference Transcript/Continuity Vault | `NOT IMPLEMENTED` | 当前没有 durable Turn/Message/ToolCall 类型或 Provider Continuity 存储 |
| Candidate/Selection/Handoff | `PASS（Fixture/已有 WAV）` | 只证明本地资产合同，不证明 LLM 已创作真实音乐 |
| Music Project Model | `NOT IMPLEMENTED` | 当前 Project 只有 Audio Clip 路径，没有完整 symbolic music facts |
| Tool Registry/Tool loop | `NOT IMPLEMENTED` | 当前 `CreativeRuntime` 是固定 plan/execute 路径 |
| MIDI/Sampler/Factory Pack | `NOT IMPLEMENTED` | workspace 未引入对应运行模块 |
| Rust Audio Engine | `NOT IMPLEMENTED` | 当前仅使用 `hound` 做 WAV 合同，没有 graph/render engine |
| VST3 Host | `NOT IMPLEMENTED` | 没有扫描、隔离、IPC、Profile 或 corpus 证据 |
| MCP Client | `NOT IMPLEMENTED` | 只有目标文档，没有注册/发现/调用代码 |

结论：当前产品仍是 `planning-only`。旧 `GenerationAdapter` 的 Fixture 可以继续帮助迁移测试，但不得进入 release composition root，也不能被计为真实音乐能力。

### 3.2 Q0 前置 Gate

M3 开工前先按 [Q0 音乐内容可行性 Spike](../planning/2026-08-24-music-quality-spike-design.md) 验证 L1—L4 结构化音乐决定。Q0 只产生 ExperimentalMusicSpec、MIDI 和固定 DAW 评价证据；不实例化 Audio Engine、Factory Pack、VST3，也不把实验 schema 当成 production Tool Interface。Q0 未得到 `GO` 前，M3 保持目标设计状态。

Q0 的当前实例化架构如下。读法是：上半部分负责“让真实模型产生可检查的音乐事实”，下半部分负责“证明证据没有漂移，再交给人听和继续编辑”。

```text
┌──────────────────────────── 冻结输入 ────────────────────────────┐
│ corpus-v1 · prompt-v1 · JSON Schema · DeepSeek V4 Pro/high      │
│ protocol v2/v3 lock · price snapshot · Bitwig recipe · mapping  │
└──────────────────────────────┬───────────────────────────────────┘
                               │ exact hash / identity
                               ▼
┌────────────────────── Q0 Runner（真实网络）──────────────────────┐
│ Mode A：一次完整 spec                                              │
│ Mode B：skeleton → arrangement → validation revision              │
│ v3 L4：仅全局 note/CC budget 错误时，最多一次完整 spec repair         │
│ Mode C：B spec + 1—2 条真实 Creator feedback                       │
│                                                                  │
│ 每个 Provider turn 先原子落盘；中断从最后已落盘 B turn 继续，不重复调用  │
└──────────────────────────────┬───────────────────────────────────┘
                               │ visible JSON only
                               ▼
┌─────────────── 严格校验与编译 ───────────────┐
│ deny unknown fields · musical/resource invariants                 │
│ ExperimentalMusicSpec → Type-1 SMF / 480 PPQ                      │
│ tempo · time signature · key · marker · track · note · CC         │
└──────────────────────────────┬───────────────────────────────────┘
                               ▼
┌──────────────────────── 证据与 Gate ─────────────────────────────┐
│ run/turn/spec/MIDI + SHA-256 + tokens/cache/latency/cost          │
│ Formal Verifier：v2 精确 4 A + 12 B；v3 精确 6 L4 B + binding        │
│ Blind Pack：evaluator 目录不含 mode；private map 与评价包分离          │
└──────────────────────────────┬───────────────────────────────────┘
                               ▼
                  Bitwig import → blind Keep → Creator edit
                     （当前三项仍需真人可验证证据）
```

安全边界：API Key 只从环境读取并在 drop 时清零；Provider private reasoning 不进入 turn、Project 或评审包；SoundFont 只被本机 recipe 引用，不复制进仓库。`experiments/music-quality/` 有独立 `[workspace]` 和 lockfile，不能被 production composition root 引用。

### 3.3 M3 实例化目标

M3 只实例化完成本地纵切所需的最小 Module：

1. Harness Foundation：Inference Transcript、Inference Item、Continuity Vault、Approval Grant、Run Budget；
2. Music Project Model；
3. 固定 Tool Registry；
4. Durable ToolExecution；
5. 有界 LLM Tool loop；
6. 最小 MIDI/arrangement 工具；
7. 确定性本地离线渲染与技术分析；
8. Candidate Project Snapshot。

M3 不需要 MCP、视频、Multi-Agent、完整实时设备、任意插件兼容或新的通用工作流平台。

## 4. 模块与 seam

### 4.1 Core Interface Module

**Interface**：版本化 command、query、event stream、expected revision、错误码与本机认证。  
**Implementation**：Axum HTTP、SSE、discovery、session 和 Client DTO。  
**不负责**：LLM 协议、音频设备、VST3 或 SQLite 细节。

Client 只知道业务资源，例如 Project、RunProjection、Candidate 和 Render；TUI/GUI 不得从日志文本推断状态。

### 4.2 Agent Harness Module

**Interface**：

```rust
pub trait CreativeAgentRuntime {
    async fn start(&self, command: StartRun) -> Result<RunProjection, AgentError>;
    async fn resume(&self, run_id: RunId) -> Result<RunProjection, AgentError>;
    async fn cancel(&self, run_id: RunId) -> Result<RunProjection, AgentError>;
}
```

调用者不需要理解 prompt construction、Provider stream、tool-call parsing、context compaction、recovery 或循环预算。Module 内部负责：

- 从 Project Snapshot 构造 Context Snapshot；
- 调用 LLM Inference Module；
- 把完整、规范化 Inference Item 追加到 Transcript；
- 保存/加载与精确 Provider 链绑定的 opaque continuity reference；
- 交给 Tool Runtime 准备/执行；
- 把 Tool Result 作为新的可见上下文；
- 在达到 Candidate、需要用户输入、预算耗尽或失败时停止。

Agent Harness 不负责音乐工程的具体不变量；这些由 Music Project Module 和 Tool implementation 持有。

### 4.3 LLM Inference Module

它是现有 `autostudio-provider` 的保留职责。窄 Interface 是：

```rust
pub trait LlmInference {
    async fn turn(
        &self,
        request: InferenceTurnRequest,
    ) -> Result<InferenceStream, InferenceError>;
}
```

`InferenceTurnRequest` 只接受 canonical messages/tool definitions、精确 Provider/Model/Protocol、Thinking Level 和兼容 continuity reference。`InferenceStream` 产生 visible delta、tool-call delta、usage、finish reason 与最终 continuity update。Adapter 负责把 wire event 组装为 canonical item；Harness 不解析 Provider JSON。

统一 Interface 表达：

- Provider/Model/Protocol；
- system/user/assistant/tool messages；
- tool definitions；
- visible text delta；
- complete tool-call delta；
- usage、finish reason 和 Provider continuity state；
- Thinking Level 的模型级能力映射。

OpenAI、Anthropic、DeepSeek、Kimi 等是这个 seam 上的真实 Adapter。Provider 专有 JSON、header、stream event 和错误停留在 Adapter 内。

LLM 中断与本地音乐工具失败分开：LLM 请求可能产生 `Interrupted` 或 `UnknownConsumption`，但没有完整 ToolRequest 就不会改变 Project。

#### 4.3.1 Inference Transcript

Transcript 是 Run 内的 durable semantic log，不是聊天字符串数组：

```rust
pub enum InferenceItem {
    VisibleMessage(VisibleMessage),
    ToolRequest(CompleteToolRequest),
    ToolResult(RecordedToolResult),
    Usage(InferenceUsage),
    Finish(InferenceFinish),
}
```

每个 item 带 `item_id`、`run_id`、`turn_id`、单调 sequence、时间与内容 hash。partial text/tool JSON 只在内存 assembler 中存在；只有完成并通过校验的 item 才能 append。Transcript 可供 Creator 审计和 Context 重建，但不能直接改变 Music Project。

#### 4.3.2 Provider Continuity State

Provider Continuity State 解决“继续同一 tool-use chain”，而不是解释或展示模型思考。OpenAI Adapter 可以保存 response/reasoning item reference，Anthropic Adapter 可以保存 signed thinking blocks；精确 payload 由 Adapter 决定并保持 opaque。

```rust
pub trait ContinuityVault {
    async fn store(&self, binding: ContinuityBinding, sealed: SealedBytes)
        -> Result<ContinuityRef, VaultError>;
    async fn load(&self, reference: &ContinuityRef, expected: &ContinuityBinding)
        -> Result<SealedBytes, VaultError>;
    async fn purge_run(&self, run_id: RunId) -> Result<(), VaultError>;
}
```

`ContinuityBinding` 至少包含 run/provider/model/protocol/tool-catalog fingerprint 与 format revision。Vault 在 Project Package 外使用独立加密密钥；payload 不进入 Project/Event/log/SSE/backup/export/compaction。Provider/Model/Protocol 切换使不兼容状态失效；Run 终态完成语义提交后 purge。

### 4.4 Tool Runtime Module

Tool Runtime 是安全和耐久执行的深 Module。其外部 Interface 保持窄：

```rust
pub trait ToolRuntime {
    fn catalog(&self, context: ToolContext) -> ToolCatalogSnapshot;
    async fn execute(
        &self,
        request: ToolRequest,
        binding: ExecutionBinding,
    ) -> Result<ToolResult, ToolError>;
}
```

`ExecutionBinding` 绑定：`run_id`、`step_id`、`turn_id`、`execution_id`、`project_id`、`expected_revision`、Tool Descriptor fingerprint、`ApprovalGrantId`、Run Budget ledger、Tool Resource Limit 和 cancellation token。

实现内部完成 schema、版本、capability、Policy、Approval Grant scope、Run Budget、Tool Resource Limit、幂等、事务、超时、结果清洗和 Event。三类检查必须分别返回稳定错误，不能用一个 money-only `CostApproval` 代替。Agent 不需要了解这些实现细节。

### 4.5 Music Project Module

这是本地音乐创作的权威领域 Module，维护以下不变量：

- Tempo Map、拍号和 sample time 映射有效；
- section、track、clip、note、CC 和 automation 的 ID 稳定；
- note-on/note-off、范围、重叠与 voice budget 合法；
- Instrument/Preset/Pack/Plugin 引用存在且被允许；
- 所有修改基于 expected revision；
- Candidate 引用不可变 Project Snapshot；
- Selection 不由 Agent 自动执行。

推荐用 typed command 修改，而不是允许 Tool 传入完整 Project JSON：

```rust
pub enum MusicProjectCommand {
    SetTempoMap(SetTempoMap),
    SetArrangement(SetArrangement),
    CreateTrack(CreateTrack),
    WriteMidiRegion(WriteMidiRegion),
    EditMidiRegion(EditMidiRegion),
    AssignInstrument(AssignInstrument),
    SetMixParameters(SetMixParameters),
    WriteAutomation(WriteAutomation),
}
```

每条 command 返回 `ProjectChangeSet`，包含新 revision、变更实体、受影响时间范围和可供 LLM/Client 使用的摘要。

### 4.6 Audio Engine Module

**Interface**：从不可变 Music Project Snapshot 编译 Render Plan，并返回受验证的 Render/Analysis receipt。

```rust
pub trait AudioEngine {
    fn compile(&self, snapshot: &MusicProjectSnapshot)
        -> Result<RenderPlan, CompileError>;

    async fn render(&self, plan: RenderPlan, target: RenderTarget)
        -> Result<RenderReceipt, RenderError>;
}
```

调用者不传裸 FFmpeg argv、插件指针或 callback。`RenderReceipt` 包含 asset identity、hash、格式、时长、sample rate、channel、engine/pack/plugin lock 和分析指标。

### 4.7 Instrument Runtime Module

统一把以下来源编译为 Audio Engine 可执行节点：

- Factory Pack / Optional Pack；
- Auto Studio Sampler；
- Approved VST3 Plugin Instance；
- 内置合成器或基础 DSP。

它不向 Agent 暴露 sample 路径、插件 binary path 或任意参数编号。Agent 只使用稳定 InstrumentId、PresetId、PluginProfile parameter semantic。

## 5. Agent Run 生命周期

![Agent Run 生命周期：LLM Planning、Approval、本地 Tool 执行、质量检查、等待采用，以及可恢复中断和取消](assets/agent-run-lifecycle.png)

可交互查看：[Agent Run 生命周期图](agent-run-lifecycle.html)。

### 5.1 主路径

1. `Planning`：LLM 读取 Brief、Project Snapshot 和允许的 Tool Catalog；
2. `AwaitingApproval`：展示精确目标、工具影响、内容/插件依赖、Grant scope 和预算；
3. `ApplyingTools`：Core 依次准备并执行本地 Tool；
4. `QualityCheck`：渲染、机器分析并判断是否需要有界迭代；
5. `AwaitingSelection`：产生可编辑 Candidate Project Snapshot 与 Preview。

`Selection` 是后续 Creator command，不是 Agent 的隐藏自动步骤。

### 5.2 Agent Step 与 ToolExecution

Agent Step 表示一次可见理解、工具请求、结果观察或等待；Inference Turn 是一次 Provider 请求/响应尝试；ToolExecution 是一次有确定副作用的工具执行。三者不能合并或复用 identity：一个 Step 可以包含多个 Turn，一个 Turn 可以产生多个完整 Tool Request，一个 ToolExecution 也可能包含编译、渲染、分析和提交等内部阶段。

建议 ToolExecution 状态：

```text
Prepared
→ AwaitingApproval
→ Running
→ Committing
→ Completed

Prepared/AwaitingApproval/Running
→ CancelRequested
→ Cancelled | Completed

任意非终态
→ NeedsAttention | Failed
```

音乐工具不再使用外部 `Submitting/Submitted/UnknownOutcome/Reconcile` 状态。LLM Inference 的网络中断只记录在 Inference Turn，不污染 Music Project Tool 状态。

### 5.3 多轮循环与上限

每个 Run 的 `RunBudget` 设置明确上限：

- 最大 Inference Turn；
- 最大 ToolExecution；
- 最大 wall-clock 时间；
- 最大 token/cost；
- 最大 Preview render 次数；
- 最大新增轨道/音符/插件实例；
- 单个 Tool 的 CPU、内存、输出大小和 deadline。

最后一项来自 Tool Descriptor 的 `ToolResourceLimit`，其余属于 Run Budget。Creator 的 Approval Grant 只表示同意范围，不能提高两者；达到上限时进入 `NeedsAttention`，不能由 LLM 自行提高预算。

### 5.4 Context 与 compaction

Context Snapshot 只包含完成当前决策所需的：Brief、选中 Project facts、变更摘要、Tool Result 和用户可见对话。大 MIDI 区域、波形、音频、插件 state 和完整 Event history 以稳定引用存在，不直接塞入 prompt。

Compaction 只能压缩可重建的对话语义，不能改变 Project Revision、完整 Tool Request/Result、ToolExecution、Approval Grant、budget ledger、Candidate 或 Selection。Provider continuity 不参与 compaction，由 Vault 独立保存和清理。

### 5.5 中断、恢复与终态清理

- `InferenceInterrupted`：已提交 Transcript item 保持有效；只有匹配的 Continuity State 才能恢复原链。缺失时可以开始新 Turn，但不能猜测 partial tool call；
- `ToolInterrupted`：根据 execution identity、transaction 与 receipt 恢复，不要求 LLM 再发一次请求；
- `NeedsAttention`：等待 Creator 输入、缩小目标、新 Grant 或预算外产品决策，不是隐式重试；
- `AwaitingSelection`、`Cancelled`、`Failed`：完成最终 Run semantic commit 后 purge continuity；purge 失败必须可见并重试清理。

## 6. Tool 注册与调用

### 6.1 Tool Descriptor

每个 Tool Descriptor 至少包含：

```text
name
revision
input_schema
output_schema
side_effect_class
approval_class
replay_policy
resource_limits
capability_fingerprint
implementation_kind
```

命名使用领域 namespace，例如 `midi.write_region@1`，而不是 `autostudio_core::midi::write_notes`。

### 6.2 M3 最小工具集合

| Tool | 作用 | 副作用 |
|---|---|---|
| `project.describe` | 读取精简工程事实 | 无 |
| `arrangement.set_structure` | 设置段落和小节范围 | Project mutation |
| `tempo.set_map` | 设置 BPM/拍号 | Project mutation |
| `track.create_instrument` | 创建乐器轨 | Project mutation |
| `harmony.write_progression` | 用和弦/voicing 写入区域 | Project mutation |
| `midi.write_region` | 批量写入旋律、bass、drum note/CC | Project mutation |
| `midi.edit_region` | 对有界区域变换、humanize、替换 | Project mutation |
| `instrument.assign` | 绑定已批准 Instrument/Preset | Project mutation |
| `mix.set_parameters` | 音量、声像、send 与基础参数 | Project mutation |
| `render.preview` | 编译并离线渲染 Preview | Asset creation |
| `audio.analyze` | 返回受限技术指标 | 无 Project mutation |
| `candidate.create` | 冻结 Candidate Snapshot | Project mutation |

Tool 应在段落、区域、pattern 和轨道层提供高杠杆能力。让 LLM 逐 note 调用成百上千次会导致浅 Interface、token 浪费和失败面膨胀；`midi.write_region` 应一次接受经过限制的 note list 或 pattern specification，并在内部完成排序、合法性与 event 编译。

### 6.3 Tool Runtime 与 MCP 图

![Tool Runtime 与 MCP：本地 Music/Audio Tool 为当前主路径，外部 MCP 是受控后续扩展](assets/tool-runtime-mcp-architecture.png)

可交互查看：[Tool Runtime 与 MCP 架构图](tool-runtime-mcp-architecture.html)。

MCP Adapter 与本地 Tool 共享 Descriptor、Policy、Approval、ToolExecution 和 Result 规则，但 MCP Server 的 schema、描述和结果是不可信输入。MCP 不参与 M3 release Gate。

## 7. 本地音乐数据模型

### 7.1 核心实体

```text
Project
├── Brief
├── TempoMap
├── Arrangement
│   └── Section[]
├── Track[]
│   ├── InstrumentTrack
│   │   ├── MidiClip[]
│   │   └── InstrumentAssignment
│   ├── AudioTrack
│   │   └── AudioClip[]
│   └── BusTrack
├── MixGraph
├── AutomationLane[]
├── Snapshot[]
├── Candidate[]
├── Selection
└── Export[]
```

### 7.2 Music Project Snapshot

Snapshot 必须不可变，并包含：

- Project Revision 与 schema version；
- Tempo/arrangement/track/MIDI/mix facts；
- Content Pack/Instrument/Preset lock；
- Plugin lock 与 state reference；
- engine version、seed、sample rate 和 render policy；
- source Snapshot 与 change summary。

Preview、Candidate、Authoritative Render 和 Export 都引用 Snapshot，不读取“当前可能正在变化的工程”。

### 7.3 Candidate

Candidate 不再只是一个下载后的 Audio Asset，而是：

```text
Candidate
├── ProjectSnapshotId
├── PreviewAssetVersionId
├── AnalysisReceiptId
├── AgentChangeSummary
├── Content/Plugin Dependencies
└── Provenance
```

## 8. 持久化、事务与恢复

### 8.1 SQLite 事实源

继续使用单 DB actor 与 rusqlite bundled。所有 Project command 经过单写入者，事务同时更新：

- current projection/snapshot；
- semantic event；
- outbox；
- ToolExecution/RunProjection 所需索引；
- 规范化 Inference Transcript、Approval Grant 与 Run Budget ledger。

音频 callback、render worker 和插件 worker 不直接写数据库。

Provider Continuity State 不写入 Project SQLite。`ContinuityVault` 位于 Project Package 外，使用独立密钥和目录/OS secure storage reference；SQLite 只保存不含 payload 的 `ContinuityRef`、binding hash、状态和清理结果。

### 8.2 Tool 幂等

本地 Project mutation 使用：

```text
(execution_id, tool_descriptor_fingerprint, input_hash, expected_revision)
```

同一 execution identity 重放时：

- 已完成：返回原 Tool Result；
- 正在运行：返回当前 projection，不启动第二份；
- expected revision 已变化：返回 conflict；
- 输入 hash/fingerprint 不同：拒绝 identity reuse。

### 8.3 崩溃恢复

Core 启动查询非终态 ToolExecution：

- 纯 Project command 在事务前崩溃：没有事实，按同一 identity 重试；
- 事务已提交但响应丢失：通过 execution receipt 返回原结果；
- render worker 中断：丢弃未验证 staging，使用同一 Render Plan 重新渲染；
- 插件 worker 崩溃：隔离实例，Project 保持原 Snapshot，进入 NeedsAttention 或使用已存在 freeze；
- LLM 中断：先保留已提交的规范化 Transcript item；有精确匹配 continuity 时由同一 Adapter 继续原链，没有时显式开始新 Turn，不猜测或重放 partial tool call；
- Run 进入终态：最终语义 commit 后 purge continuity；清理失败记录稳定 blocker 并由启动 janitor 重试。

因为没有远端 Music Provider，不存在“外部已经计费生成但本地不知道”的音乐 Unknown Outcome。外部 MCP Tool 若未来进入，必须按其 replay policy 单独处理。

### 8.4 资产原子提交

Render 输出先进入 Project 内受控 staging：

1. 写临时文件；
2. 关闭 writer 并重新打开最终副本；
3. 校验格式、时长、sample count、channel、size；
4. 对最终副本计算 hash；
5. 原子 rename 到 immutable asset path；
6. 同一 SQLite 事务提交 Asset Version、receipt 和 Event。

失败时不创建 Candidate，不保留“看起来成功”的半文件。

## 9. Rust Audio Engine

### 9.1 M3：离线优先

M3 使用确定性离线渲染证明音乐闭环，避免在 Tool schema、Music Project Model 尚未稳定时同时引入设备、buffer、underrun 和实时 graph swap 风险。离线优先不是 FFmpeg 编排：Render Plan、MIDI schedule、Sampler、Mix Graph 和 DSP semantics 仍由 Rust 模块持有。

### 9.2 目标线程模型

```text
Tokio / Control
  ├── Agent / Tool / DB commands
  ├── Render Plan compiler
  └── worker supervision

Offline Render Worker
  ├── MIDI event scheduler
  ├── Instrument nodes
  ├── Mix Graph / DSP
  └── staged asset writer

Realtime Audio Thread（后续实例化）
  ├── fixed-capacity graph snapshot
  ├── no allocation / file / network / DB / logging
  └── bounded lock-free command/event queues

Plugin Worker
  ├── VST3 ABI and unsafe isolation
  ├── shared audio/event buffers
  └── versioned control IPC
```

### 9.3 库与职责

当前 workspace 只实例化 `hound` 用于 WAV 合同。进入相应里程碑后再引入并验证：

| 需求 | 候选 | 备注 |
|---|---|---|
| WAV | `hound` | 已使用；只覆盖有限 WAV，不等于完整媒体解码 |
| 解码 | `symphonia` | 音频导入/分析候选 |
| 实时 I/O | `cpal` | 实时阶段进入，不能在 callback 中做业务工作 |
| FFT | `rustfft` | 频谱与分析基础 |
| 矩阵/缓冲 | `ndarray` 或专用 buffer | 以 allocation/control 为 Gate，不因通用性直接进入 callback |
| 重采样 | `rubato` | 离线与受控实时路径分别验证 |
| MIDI I/O | `midir` | 外部设备阶段；内部 MIDI event 不依赖它建模 |
| DSP | 自有 nodes / `fundsp` 评估 | 必须通过确定性、实时和音质 Gate |
| 自有插件开发 | `nih-plug` | 不能当第三方 VST3 Host |
| VST3 Host | Steinberg VST3 SDK/C API + 窄 FFI | 独立 unsafe/许可/线程审计 |

依赖名称不是完成证据。每个库只有在实际 Module、测试、许可和目标 OS 构建通过后才进入 workspace。

## 10. Factory Pack、Sampler 与 VST3

### 10.1 Content Pack

Pack manifest 记录 pack/version/source/license/hash/instrument/zone/velocity/round-robin/articulation/loop/tuning。Sampler 只读取 Catalog 已批准内容，不扫描任意用户目录。

Factory Pack 必须能随软件合法再分发并允许创作者产出商业音乐；若许可证只允许创作者自行下载，则归为 Optional Pack，不进入安装包。

### 10.2 VST3 Host

VST3 Host 是独立安全/故障生命周期，满足新 process/crate 的准入条件。首版范围：

- 固定一个首发 OS；
- 官方目录与稳定 Plugin UID；
- 扫描 Worker 与运行 Worker 隔离；
- instrument/effect、audio/MIDI bus、preset/state、latency、离线 render、freeze；
- Approved Plugin Profile 才允许 Agent 自主调参；
- binary hash 变化触发重新验证；
- native GUI 不阻塞 Agent 参数路径。

`nih-plug` 用于开发自有插件，不提供第三方 VST3 宿主能力。

## 11. Core Interface 与客户端投影

### 11.1 主要资源

保留现有 Connection/Project API，并逐步加入：

```text
GET  /v1/tools
POST /v1/projects/{projectId}/agent-runs
GET  /v1/projects/{projectId}/agent-runs/{runId}
POST /v1/projects/{projectId}/agent-runs/{runId}/approval
POST /v1/projects/{projectId}/agent-runs/{runId}/cancel
GET  /v1/projects/{projectId}/agent-runs/{runId}/events

GET  /v1/projects/{projectId}/music-project
GET  /v1/projects/{projectId}/candidates
POST /v1/projects/{projectId}/candidates/{candidateId}/selection
POST /v1/projects/{projectId}/renders
GET  /v1/projects/{projectId}/renders/{renderId}
POST /v1/projects/{projectId}/exports
```

### 11.2 Run Projection

```json
{
  "projectRevision": 42,
  "asOfSequence": 117,
  "runId": "run_...",
  "phase": "applying_tools",
  "steps": [],
  "transcriptItems": [],
  "toolExecutions": [],
  "activeApprovalGrant": null,
  "budgetUsage": {},
  "candidateIds": [],
  "blocker": null
}
```

Client 先获取 Snapshot，再从 `asOfSequence` 连接 SSE。断线后重新获取 Snapshot，不从 Activity 文案拼状态。Projection 永远不包含 continuity payload、private reasoning 或 Vault path。

## 12. Workspace 与 crate 收敛

当前 5 个共享 crate 保持：

| crate | M3 职责 |
|---|---|
| `autostudio-core` | Music Project domain、Agent Run、Inference item、Tool descriptor/request/result、Grant/Budget、Policy、application Interface |
| `autostudio-provider` | 仅 LLM Connection、Catalog、Thinking、stream assembly 与 continuity-aware 协议 Adapter；删除 production music generation 职责 |
| `autostudio-storage` | Project SQLite、Event/outbox、Transcript、Grant/Budget、ToolExecution 与 Snapshot；Project 外 Continuity Vault implementation |
| `autostudio-media` | staged asset、WAV、离线 render/analysis 的初始实现 |
| `autostudio-api` | Core HTTP/SSE、session/discovery 与 DTO |

应用：

- `core-daemon`：唯一业务进程与 composition root；
- `autostudio`：TUI Client；
- `desktop`：次要开发 Client。

不为每个 Tool、新 DSP 或 Instrument 新建 crate。只有满足以下之一才拆分：

1. 独立实时线程和构建/测试生命周期；
2. VST3 unsafe ABI 与崩溃隔离进程；
3. 至少两个真实 Adapter 证明稳定 seam；
4. 许可或分发要求独立产物。

旧 `GenerationCoordinator` 不应继续膨胀成 Tool Runtime。迁移完成后删除其 production seam；测试中需要的音频 Fixture 移到测试 support，不以“Provider”命名。

## 13. MCP 目标设计

Auto Studio 是 MCP Host/Client，不默认把自身公开为 MCP Server。MCP 进入条件：

- 本地 Tool Runtime 已通过 M3；
- 至少一个明确用户任务需要外部 Tool；
- Connection、Credential Vault、protocol revision、allowlist 和 trust policy 已设计；
- descriptor schema/size/depth、结果注入、危险 URI、secret echo 和重放语义有合同测试。

MCP Tool 命名 `mcp.<server_id>.<tool_name>`，不得覆盖内置 `music.*`、`midi.*`、`render.*`。打开 Project 不自动启动其中记录的任意 stdio command。

## 14. 安全与隐私

- Core 默认只监听 loopback，并使用私有 discovery/session；
- LLM Key 写入 OS Vault；当前私有 `0600` 文件只作开发 bootstrap；
- Prompt 只发送用户批准的 Brief/Project 摘要，不发送采样文件、插件 binary 或完整 Project；
- Provider continuity 只进加密 Vault，绑定 run/provider/model/protocol/tool catalog，终态 purge；
- Tool input/output 有大小、深度、数量、时间和资源预算；
- LLM 不得构造 raw path、Shell、SQL、FFmpeg argv、VST3 pointer 或任意 MCP envelope；
- 插件扫描与运行隔离，崩溃/挂死不破坏 Project；
- Project/Export 记录 Content/Plugin/engine provenance，但不包含 Credential；
- 日志不保存 Authorization header、Key、continuity payload、私有 reasoning、原始插件 state 或未清洗外部结果。

## 15. Observability

结构化字段至少包含：

```text
core_instance_id
project_id / project_revision
run_id / step_id / execution_id
tool_name / tool_revision / descriptor_fingerprint
phase / attempt / duration_ms
llm_provider / model / protocol / usage
transcript_sequence / continuity_binding_hash / continuity_status
approval_grant_id / run_budget_usage
render_plan_id / snapshot_id / asset_hash
pack_id / plugin_uid_hash
error_code / retry_decision
```

Metrics：Inference latency/token/cost、continuity load/purge success（不含 payload）、Tool duration/failure、Grant/Run Budget 拒绝、render real-time factor、CPU/memory、asset validation、Project conflict、VST3 crash/hang、audio underrun（进入实时阶段后）、Candidate adoption 和 recovery。

## 16. 测试与 Gate

### 16.1 Domain 与 Tool

- schema/descriptor fingerprint；
- expected revision 与 identity reuse；
- MIDI/Tempo/arrangement/mix invariants；
- Approval Grant 的 revision/tool fingerprint/target/effect/cost binding，与 Run Budget/Tool Resource Limit 分离；
- 每个 Tool 通过相同 Interface 的成功、拒绝、失败和恢复合同。

### 16.2 Agent Harness

- LLM 输出普通文本、单 Tool、多 Tool、partial stream、非法 JSON、未知 Tool、超限参数；
- canonical Inference Item 的 identity/order/hash 与完整 tool-call assembly；
- Tool Result 回灌后的有界下一轮；
- budget/turn/tool/render 上限；
- compaction 不改变工程事实、完整 Tool Request/Result、Grant 或 budget ledger；
- OpenAI/Anthropic fixture 的 continuity round-trip、binding mismatch、损坏/缺失与终态 purge；
- continuity sentinel 不出现在 Project/Event/log/backup/export/SSE/TUI；
- Core 重启后分别从 committed Transcript/Continuity checkpoint 与 ToolExecution receipt 恢复。

### 16.3 Music 与 Render

- note/CC 编译、sample-time 映射和边界；
- deterministic render；
- staging/final-copy hash、WAV/sample count/时长校验；
- LUFS/True Peak/clipping/silence；
- Factory Pack 音高、loop、velocity、RR、articulation、pedal 和 voice stealing；
- 固定 corpus 人工盲听。

### 16.4 VST3

- 扫描 crash/hang/畸形 metadata；
- state/preset/bus/latency/PDC/freeze；
- worker crash containment；
- fixed plugin/version corpus；
- SDK、商标、第三方 EULA 与分发 notice。

### 16.5 Release Gate

| Gate | 通过条件 |
|---|---|
| Agent | 一个真实 LLM 在 production 发起至少两种本地音乐 Tool 并完成有界循环 |
| Harness durability | Transcript、continuity、Grant 与 Budget 分离；重启、compaction、中断和终态 purge 通过 |
| Project | Music Project/Snapshot/Candidate/Selection 重启后保持一致 |
| Factory audio | 无第三方插件也能本地渲染并达到冻结技术/盲听阈值 |
| Editable handoff | WAV/stems/MIDI/Tempo/manifest 在目标 DAW 继续编辑 |
| VST3 | 一个首发 OS 的固定 instrument/effect corpus 通过隔离与恢复 |
| Security | Credential、路径、插件 ABI 与任意命令不暴露给 LLM/Client |
| Distribution | 干净机安装、升级、卸载、签名与许可完成 |

Fixture、ignored live test、厂商宣传和“代码可编译”均不能替代相应 Gate。

## 17. 从当前代码迁移

按依赖顺序实施：

1. `PASS`：已建立可回退 Git baseline `b9db99c`；继续禁止未审计的 destructive cleanup；
2. `IN PROGRESS`：执行 Q0 内容可行性 Spike，只有 `GO` 才进入 production M3；
3. 冻结 ADR-0011、ADR-0012 与新领域词汇；
4. 冻结旧 `GenerationAdapter/Coordinator`，停止扩展 submit/observe/reconcile，但暂不删除；
5. 先实现 Harness Foundation：Inference Item/Transcript、Provider continuity Adapter contract/Vault、Approval Grant、Run Budget；
6. 在 `autostudio-core` 建立 Music Project commands、Tool Descriptor/Request/Result 与 RunProjection；
7. 在 storage 增加 Music Project/Transcript/Grant/Budget/ToolExecution/Snapshot migration 与 non-terminal query；
8. 实现固定本地 Tool Registry 与两种真实 Tool Adapter：Project/MIDI 与 Render/Analysis；
9. 把现有 LLM Planning 扩展为有界 Tool loop；
10. 以最小内置音源完成离线 render，再接 Factory Pack/Sampler；
11. 把 Candidate 从 Audio-only 升级为 Project Snapshot，并让新纵切通过 production composition root；
12. 用端到端、恢复和架构守护测试证明新路径后，再删除 Generation 状态/API 与误导命名，把仍需的 WAV fixture 移入 test support；
13. Factory 质量 corpus 通过后进入 VST3 隔离 Host。

迁移期间不能把旧 Fixture 接回 production 来保持演示“能发声”。如果本地音乐 Tool 未准备好，产品应明确显示 `local music runtime not available`。

## 18. 参考与决策记录

- [产品设计](../product/ai-creative-agent-product-design.md)
- [Roadmap](../roadmap.md)
- [共同语言](../../CONTEXT.md)
- [ADR-0011：由 LLM 通过本地工具创作音乐](../adr/0011-llm-authored-local-music.md)
- [ADR-0012：Durable Agent Harness State](../adr/0012-durable-agent-harness-state.md)
- [Q0 音乐内容可行性 Spike](../planning/2026-08-24-music-quality-spike-design.md)
- [ADR-0004：Rust Core 与专业 Audio Engine](../adr/0004-rust-core-professional-audio-engine.md)
- [真实乐器采样与 Rust 音频栈研究](../research/instrument-sample-libraries-and-rust-audio-stack-2026-08-21.md)
- [Agent Harness 模式研究](../research/agent/agent-run-harness-patterns-2026-08-23.md)
