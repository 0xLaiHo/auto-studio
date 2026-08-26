# Auto Studio

Auto Studio 是一个由 LLM 驱动的本地专业音乐创作 Agent。目标体验是：Creator 与 LLM 对话，LLM 通过本地语义工具完成作曲、编曲、MIDI、配器和混音，Rust Audio Engine 把可编辑 Music Project 渲染为 WAV、stems 和 MIDI。

Auto Studio 不接入 Music Provider，也不依赖 Mureka、Lyria、Eleven Music 或 Stable Audio。唯一必需的外部 AI 连接是用户自备 Key 的 LLM Provider。

## 当前状态

截至 2026-08-27：

- `PASS`：独立 Rust Core、本机认证 API、SQLite Project/revision/event/outbox/backup；
- `PASS`：`autostudio` Ratatui 入口、`/connect`、模型目录、`/model`、Thinking Level、`/exit`；
- `PASS（contract + DeepSeek/OpenAI live）`：OpenAI Chat、OpenAI Responses、Anthropic Messages（含 DeepSeek/Kimi 兼容端点）统一使用 SSE streaming，partial text/tool JSON 只在内存组装，完整 Turn 才可落盘；2026-08-25 `deepseek-v4-flash` 真实 Tool Call smoke 通过，2026-08-26 `gpt-5-mini` 两轮 Responses Continuity live 通过；
- `PASS（M3-A CM-0/CM-1/CM-2 planning slice）`：Run/Turn/Item identity、durable Inference Transcript、Context Manifest、canonical Provider request、完整 ToolRequest/ToolResult、SQLite CAS 与重启 replay 已进入 production Planning 路径；固定的 `project_describe → submit_creative_plan` 多轮链路会让每一步从 Project/Transcript 重新派生，并可通过 Core API、TUI 与 Desktop 恢复；OpenAI Responses reasoning item 与 Anthropic signed thinking block 只进入 Project 外的加密 Continuity Vault，按精确 Provider binding 复用并在 Run 终态清理；
- `PASS（M3-A CM-3 planning slice）`：`prepare_turn` 会测量 canonical request footprint；达到 hard/overflow 时自动选择不拆 Turn/Tool pair 的连续前缀，保留新输入和最近两轮，用有界结构化摘要建立 Checkpoint，并且只有压缩后实际变短且回到 Normal 才允许调用 Provider。Creator 新输入、Checkpoint、Manifest 与 spill 在同一 SQLite transaction 提交；失败后零落盘，重试得到相同 checkpoint 内容 hash。Provider 明确报告 context overflow 时清除旧 Continuity 并只恢复一次，第二次停止；完整 Transcript、Project facts 与 Tool Result 始终保留；
- `PASS（M3-A CM-4 planning slice / machine contract）`：同一 Run 的完整 Transcript 现在具有精确 item 查询和 SQLite FTS5/BM25 检索；命中带 source item/type/time/Project revision/hash/Tool execution/error provenance，并以有界 untrusted user context 注入。每次选择及 token 成本写入 `ContextManifest`，current tail 与摘要已引用来源会去重；索引可删除并在 Project 重开时从 Transcript 重建。冻结合同通过 100 inference steps、10 次 compaction、3 次重启和模拟跨日恢复；真实音乐 Tool 的约束保持/正确率仍等待后续纵切；
- `PASS（M3-A Grant/Budget machine contract）/ NOT WIRED（Tool Runtime）`：`ExecutionControlManager` 已实现不可变 Approval Grant、configured/system Run Budget ceiling、独立 Tool Resource Limit、Inference/Tool/active-time/cost/render/effect/asset/concurrency ledger，以及幂等 Execution Reservation/settlement/cancel；SQLite CAS、stale revision、故障零发布、重启恢复、篡改失败关闭和跨日暂停合同通过。它尚未接入固定 Planning composition root、Policy、durable ToolExecution 或 Music Project revision；
- `PARTIAL`：Audio-only Candidate/Selection/Handoff 与 WAV 资产合同，只由 Fixture 或已有资产验证；
- `PASS（Q0 实验装置）`：独立 Rust workspace、冻结的 12 Brief corpus、真实 DeepSeek V4 Pro A/B/C runner、严格 ExperimentalMusicSpec、Type-1 SMF MIDI compiler、逐轮恢复、artifact/hash 校验、匿名评审包，以及 v3 逐 Run 协议绑定/一次资源预算修订/严格验证；
- `PASS（Q0-Content machine/review apparatus）/ LIVE-PENDING（human gate）`：v2 正式 A/B 已完成，Mode B 11/12 valid + compiled；v3 全量重跑 6 个 L4 并达到 6/6 valid + compiled；六样本本地评审包已生成并验证，Creator feedback、Mode C、盲听 Keep、实际继续编辑与条件式第二模型复核仍未完成；
- `PASS（DAW qualification apparatus）/ DEFERRED（M5 human matrix）`：Portable Handoff 与证据 verifier 已实现；Cubase、Studio One Pro、FL Studio 当前保持 `0 pass / 3 not_run`。该矩阵影响专业交接声明，但不再阻塞 Q0-Content 对 M3-B 的投资判断；
- `NOT IMPLEMENTED（production execution）`：通用 Tool Registry/Policy/ToolExecution、Music Project Model、MIDI Tool、Sampler、Factory Pack、Audio Engine、VST3 Host 与 MCP Client。Grant/Budget 机器合同已完成但尚未驱动真实工具；真实音乐 Tool 的长 Run 质量 Gate、超长 single-turn、Provider-specific 精确 tokenizer 和真实 overflow live qualification 仍待完成。

因此当前 production 仍是 `planning-only`，还不能真实生成音乐。`autostudio-provider` 默认只编译 LLM Connection/Inference/Continuity/Planning 职责；仓库中的 `GenerationAdapter`、Provider Job 状态与确定性 WAV Fixture 属于旧方向的迁移代码，只能通过非默认 `legacy-generation` feature 做兼容回归，不是目标 runtime，也不能用于发布能力声明。Core/TUI/Desktop production source 由架构门禁禁止调用这条旧路径。

当前只把 [Q0 音乐内容可行性 Spike](docs/planning/2026-08-24-music-quality-spike-design.md) 的真人内容结论作为 M3-B 前置 Gate。实验代码位于 [`experiments/music-quality`](experiments/music-quality/README.md)，本地六样本评审包由 [`experiments/portable-handoff`](experiments/portable-handoff/README.md) 可复现生成；两者都不加入 production workspace。真人反馈、Mode C、盲听与继续编辑完成前仍不形成 `CONTENT-GO`。跨 DAW 真人矩阵后置到 M5，不影响是否开始 Music Project Domain，但在完成前不能宣称专业 DAW 交接通过。下一代码依赖是在 `CONTENT-GO` 后实现具有独立 revision 的 Music Project Domain，再把 Execution Reservation 接到 ToolExecution。M3 的目标是：

```text
Brief
  → 真实 LLM
  → 本地 Semantic Tool
  → 可编辑 Music Project
  → 本地离线 Render / Analysis
  → Candidate Project Snapshot
  → Creator Selection
```

详见 [Roadmap](docs/roadmap.md)、[Legacy Generation 迁移清单](docs/planning/legacy-generation-migration.md)、[ADR-0011](docs/adr/0011-llm-authored-local-music.md) 和 [ADR-0012](docs/adr/0012-durable-agent-harness-state.md)。

## 运行 TUI

开发机安装两个可执行文件：

```bash
cargo install --path apps/core-daemon --locked
cargo install --path apps/tui --locked
autostudio
```

TUI 会连接或启动独立 `core-daemon`。首次使用：

```text
输入 /
→ 选择 /connect
→ 选择 LLM Provider
→ 输入并保存 API Key
→ 后台刷新模型目录
→ 输入 /model
→ ↑↓ 选择模型
→ ←→ 选择该模型支持的 Thinking Level
→ Enter 保存
```

普通文本会进入当前 Creative Agent 流程：没有 Project 时创建 `Untitled Project`，保存 Creative Brief，再调用真实 LLM 形成 typed Plan。由于本地 Music Tool 尚未实现，当前版本不会生成音频。

Credential 不进入 Project Package、日志、Event、Export 或可读 API。当前开发构建使用 Project 外的私有配置文件并在 Unix 强制 `0600`；正式发布前必须迁移到目标 OS Credential Vault。

不安装时可直接运行：

```bash
cargo build -p core-daemon
cargo run -p autostudio-tui --bin autostudio
```

常用开发覆盖：

- `AUTOSTUDIO_HOME`
- `AUTOSTUDIO_PROJECT_PACKAGE`
- `AUTOSTUDIO_DISCOVERY_FILE`
- `AUTOSTUDIO_LLM_CONNECTION_FILE`
- `AUTOSTUDIO_CONTINUITY_ROOT`
- `AUTOSTUDIO_CONTINUITY_KEY_FILE`
- `AUTOSTUDIO_CORE_BINARY`

Continuity Vault 默认与 LLM Connection 位于同一应用私有根目录，但不在 Project Package 内；Core 会拒绝把 Vault 或 key 指向工程内部（包括经符号链接解析后落入工程的路径）。当前开发实现使用独立 `0600` 本地密钥和 XChaCha20-Poly1305；正式发布仍需迁移到目标 OS Credential Vault。

## LLM Provider

产品内 `/connect` 是默认配置方式。环境变量只在私有 Connection 文件不存在时作为开发回退：

| Provider | Credential | Model | Base URL |
|---|---|---|---|
| DeepSeek | `DEEPSEEK_API_KEY` | `DEEPSEEK_MODEL` | `DEEPSEEK_BASE_URL` |
| OpenAI | `OPENAI_API_KEY` | `OPENAI_MODEL` | `OPENAI_BASE_URL` |
| Anthropic | `ANTHROPIC_API_KEY` | `ANTHROPIC_MODEL` | `ANTHROPIC_BASE_URL` |
| Kimi Open | `MOONSHOT_API_KEY` | `MOONSHOT_MODEL` | `MOONSHOT_BASE_URL` |
| Kimi Code | `KIMI_CODE_API_KEY` | `KIMI_CODE_MODEL` | `KIMI_CODE_BASE_URL` |

Provider 只负责 LLM 推理，不生成音乐资产。

## 运行 Desktop

Desktop 是复用同一 Core Interface 的次要开发 Client：

```bash
cd apps/desktop
pnpm install --frozen-lockfile
pnpm tauri dev
```

它不拥有 Project、Credential 或 Agent Runtime，也不暴露旧 Generation 执行、刷新或对账入口。旧 Audio Candidate 只作为 Project/API 兼容数据读取，不证明目标音乐闭环。

## 单独运行 Core

```bash
export AUTOSTUDIO_PROJECT_PACKAGE=/absolute/path/to/demo.autostudio
export AUTOSTUDIO_DISCOVERY_FILE=/absolute/path/to/runtime/core.json
export AUTOSTUDIO_BIND=127.0.0.1:0
cargo run -p core-daemon
```

Core 将动态端口和 session token 写入私有 discovery record。受保护请求必须携带该 bearer token；WebView 不读取 token、Credential 或 Project 路径。

## 验证

```bash
cargo fmt --all --check
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
./scripts/check-crate-boundaries.sh
cd apps/desktop && pnpm build
```

DeepSeek 真实计费 smoke 只在显式提供 Key 时运行：

```bash
bash scripts/test-deepseek-live.sh
```

缺少 `DEEPSEEK_API_KEY` 时退出 `77` 并标记 `SKIP`，不算 PASS。

## 文档

- [文档导航与权威顺序](docs/README.md)
- [产品设计](docs/product/ai-creative-agent-product-design.md)
- [技术设计](docs/design/auto-studio-technical-design.md)
- [统一 Roadmap](docs/roadmap.md)
- [共同语言](CONTEXT.md)
- [ADR-0011](docs/adr/0011-llm-authored-local-music.md)
- [ADR-0012](docs/adr/0012-durable-agent-harness-state.md)
- [Q0 音乐内容可行性 Spike](docs/planning/2026-08-24-music-quality-spike-design.md)
- [Q0 可运行实验](experiments/music-quality/README.md)
- [Q0 结果报告](docs/research/music-quality-q0-results-2026-08-24.md)
- [Agent Harness 架构图](docs/design/agent-harness-architecture.html)
- [Agent Run 生命周期图](docs/design/agent-run-lifecycle.html)
- [Tool Runtime 与 MCP 架构图](docs/design/tool-runtime-mcp-architecture.html)
