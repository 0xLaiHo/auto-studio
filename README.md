# Auto Studio

Auto Studio 是一个由 LLM 驱动的本地专业音乐创作 Agent。目标体验是：Creator 与 LLM 对话，LLM 通过本地语义工具完成作曲、编曲、MIDI、配器和混音，Rust Audio Engine 把可编辑 Music Project 渲染为 WAV、stems 和 MIDI。

Auto Studio 不接入 Music Provider，也不依赖 Mureka、Lyria、Eleven Music 或 Stable Audio。唯一必需的外部 AI 连接是用户自备 Key 的 LLM Provider。

## 当前状态

截至 2026-08-24：

- `PASS`：独立 Rust Core、本机认证 API、SQLite Project/revision/event/outbox/backup；
- `PASS`：`autostudio` Ratatui 入口、`/connect`、模型目录、`/model`、Thinking Level、`/exit`；
- `PASS（contract）`：OpenAI、Anthropic、DeepSeek、Kimi 等 LLM 协议与一次 typed Planning Turn；
- `PARTIAL`：Audio-only Candidate/Selection/Handoff 与 WAV 资产合同，只由 Fixture 或已有资产验证；
- `PASS（Q0 实验装置）`：独立 Rust workspace、冻结的 12 Brief corpus、真实 DeepSeek V4 Pro A/B/C runner、严格 ExperimentalMusicSpec、Type-1 SMF MIDI compiler、逐轮恢复、artifact/hash 校验、匿名评审包，以及 v3 逐 Run 协议绑定/一次资源预算修订/严格验证；
- `PASS（Q0 v2/v3 machine gate）/ LIVE-PENDING（human gate）`：v2 正式 A/B 已完成，Mode B 11/12 valid + compiled；v3 全量重跑 6 个 L4 并达到 6/6 valid + compiled，Bitwig MIDI 导入、盲听 Keep、Creator feedback、实际继续编辑与条件式第二模型复核仍未完成；
- `NOT IMPLEMENTED（production）`：Inference Transcript/Continuity Vault、Approval Grant/Run Budget、通用 LLM Tool loop、Music Project Model、MIDI Tool、Sampler、Factory Pack、Audio Engine、VST3 Host 与 MCP Client。

因此当前 production 仍是 `planning-only`，还不能真实生成音乐。仓库中的 `GenerationAdapter`、Provider Job 状态与确定性 WAV Fixture 属于旧方向的迁移代码，不是目标 runtime，也不能用于发布能力声明。

当前正在执行 [Q0 音乐内容可行性 Spike](docs/planning/2026-08-24-music-quality-spike-design.md)。实验代码位于 [`experiments/music-quality`](experiments/music-quality/README.md)，不加入 production workspace；v2 证明真实 LLM 可以输出严格结构化音乐并编译为 MIDI，v3 已为全部 6 个 L4 建立可比较的合法 B 基线。Bitwig 导入、真人反馈、盲听与继续编辑完成前仍不形成 `GO`。Q0 `GO` 后的 M3 目标是：

```text
Brief
  → 真实 LLM
  → 本地 Semantic Tool
  → 可编辑 Music Project
  → 本地离线 Render / Analysis
  → Candidate Project Snapshot
  → Creator Selection
```

详见 [Roadmap](docs/roadmap.md)、[ADR-0011](docs/adr/0011-llm-authored-local-music.md) 和 [ADR-0012](docs/adr/0012-durable-agent-harness-state.md)。

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
- `AUTOSTUDIO_CORE_BINARY`

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

它不拥有 Project、Credential 或 Agent Runtime。界面中由 Fixture 出现的 Audio Candidate 只证明旧本地合同，不证明目标音乐闭环。

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
