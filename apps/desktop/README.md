# Auto Studio Desktop

Desktop 是复用同一 Rust Core 契约的次要开发界面，不是 Ship 0 的主发布 Client。Ship 0 的默认入口是 `autostudio` TUI；Desktop 用于验证 GUI、WebView 安全边界、Candidate/Timeline/Handoff 交互和未来 GUI 方向，不扩大首发 OS、安装或设计伙伴矩阵。

当前能力：

- Tauri 2 壳层自动构建、启动、发现并回收 `core-daemon`；
- Rust Client 验证权限受限的 discovery record、loopback endpoint、Core/protocol/schema 版本；
- WebView 只通过 Tauri command 使用 Project、Run、Candidate、Preview、Selection、backup 和 Handoff；
- 可复用由 TUI/Core 保存的非秘密 LLM Connection 状态；
- Core 不可用、协议不兼容或命令失败时重新加载权威 Project，而不是把本地 UI 状态当作恢复事实。

React/WebView 不直接访问 Project Package、SQLite、Provider Credential、Provider API、Core session token 或本地绝对路径。

## 开发启动

准备 Rust 1.96.1、Node.js 与 pnpm 11：

```bash
pnpm install --frozen-lockfile
pnpm tauri dev
```

开发命令会先构建 `core-daemon`，默认使用应用本地 Project、runtime、backup 和 Connection 路径。需要调试时可使用仓库根目录 README 中的 `AUTOSTUDIO_*` 开发覆盖；它们不是最终产品配置流程。

仅验证前端生产构建：

```bash
pnpm build
```

当前 Desktop 通过旧 Fixture 只能验证 Audio Asset/Candidate 等迁移合同。目标产品不使用 Music Provider；在 LLM Tool loop、Music Project、Sampler 和 Audio Engine 完成前，GUI 中出现 Fixture Candidate 不等于 LLM 已真实生成音乐。
