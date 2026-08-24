# TypeScript 音频技术栈对 Rust Adapter 的替代评估

> **历史报告**：本文只回答 v0.4 的“简单编辑与试听能否不用 Rust Adapter”。它同时确认 Pure TypeScript 不能等价覆盖专业实时音频工作站。产品现已选择 Rust Core；Audio Engine 进入 Ship 1、VST3 进入 Ship 2，实施以 [ADR-0004](../adr/0004-rust-core-professional-audio-engine.md)、[ADR-0007](../adr/0007-progressive-product-proof-before-audio-and-vst3.md)和[当前技术设计](../design/auto-studio-technical-design.md)为准；本文不再是选型建议。

> 日期：2026-08-21  
> 问题：Auto Studio 不实现 Rust Adapter 时，TypeScript / JavaScript / WASM / FFmpeg 能否替代 `rustfft`、`ndarray`、`symphonia`、`cpal`、`fundsp`、`nih-plug`、`midir`、`rubato`、`hound`？  
> 来源原则：只采用标准、运行时官方文档、项目官方仓库和官方包信息；维护状态以本日期可见的发布和仓库活动为准。

## 1. 结论

**对于当前 Auto Studio MVP，可以不做 Rust Adapter；对于完整专业实时音频工作站，不能把所有 Rust 能力等价替换为 Pure TypeScript。**

可维护的 MVP 组合是：

- Hono / Node.js / TypeScript 继续负责 Agent、Tool、项目状态、任务恢复和媒体任务编排；
- `ffmpeg` / `ffprobe` 作为固定版本的外部可执行程序，负责正式解码、分析、滤镜、重采样、混音和导出；
- Electron Renderer 使用 Web Audio / AudioWorklet 做交互试听、波形与基础实时效果；
- 只有确实需要数值结果的分析算法，才在 Node Worker 或 Web Worker 中使用 `FFT.js`、Meyda、`ml-matrix` 或 WASM 解码器；
- LLM 只调用 `audio.inspect`、`audio.render_mix` 这类稳定业务 Tool，不能获得任意 shell、FFmpeg 参数、文件路径或底层库 API。

这一方案能覆盖非破坏性剪切、增益、声像、淡入淡出、基础混音、响度/峰值/频谱分析、采样率转换、WAV 等格式导出和试听。

以下三项仍然**没有可信的 Pure TypeScript 等价替代**：

1. 跨平台、可控 buffer size 的原生低延迟全双工设备 I/O；
2. VST3 / CLAP / AU 第三方原生插件宿主；
3. 可承诺低抖动、虚拟端口和系统级路由一致性的硬件 MIDI 层。

如果这三项进入产品验收范围，应重新打开原生 Audio Engine 决策；但原生实现也不必污染 Agent/Core，可以保持独立进程边界。

## 2. 逐项替代映射

| Rust 能力 | TS/JS/WASM/FFmpeg 候选 | Hono / Node Core | Electron Renderer / Web | 等价程度与建议 |
|---|---|---:|---:|---|
| `rustfft` FFT | `FFT.js`；Meyda；`AnalyserNode`；FFmpeg `showspectrum` | `FFT.js`/Meyda 放 Worker；FFmpeg 外部进程 | 可用，重计算放 Web Worker/AudioWorklet | **大部分可替代**。数值谱用 `FFT.js`，音频特征用 Meyda，实时可视化用 `AnalyserNode`；都不是完整 STFT/效果器系统 |
| `ndarray` N 维计算 | `Float32Array` / `SharedArrayBuffer`；`ml-matrix` | 可用 | 可用 | **部分替代**。音频 buffer 优先 TypedArray；`ml-matrix` 是二维线性代数，不是 N 维 ndarray 的等价物 |
| `symphonia` 解码/demux | FFmpeg/ffprobe；Mediabunny；`@audio/decode` | 默认 FFmpeg；Mediabunny Server 需要 native NodeAV；`@audio/decode` 是 JS/WASM | Mediabunny + WebCodecs 或 `@audio/decode` | **MVP 可替代**，但不是靠一个 Pure TS 库覆盖长尾 codec。正式导入仍以 FFmpeg 为事实源 |
| `cpal` 实时设备 I/O | Web Audio、AudioWorklet、`getUserMedia()`；NodeAV Device API | 无 Pure TS 等价；NodeAV 是 native addon | 可用于应用内试听和录音 | **不等价**。Web Audio 由浏览器控制设备、buffer 和后端；NodeAV capture 也不是完整低延迟 DAW Engine |
| `fundsp` DSP graph | Web Audio nodes；Tone.js；AudioWorklet；FFmpeg audio filters | 离线用 FFmpeg；Node 无标准实时 Web Audio runtime | Web Audio/Tone.js 可用 | **MVP 可替代、专业实时不可等价**。Tone.js 提供 transport、合成和效果，但建立在 Web Audio 上 |
| `nih-plug` 插件能力 | AudioWorklet / Web Audio Modules 形式的自有 Web DSP | 不可加载原生 VST3/CLAP/AU | 只能运行 Web/WASM 处理器 | **不可替代原生插件生态**；而且 `nih-plug` 本身是插件开发框架，不是 host |
| `midir` 设备 MIDI | Web MIDI；WEBMIDI.js；JZZ；`@tonejs/midi` | WEBMIDI.js/JZZ 的 Node 设备访问依赖 `jazz-midi` 原生层；`@tonejs/midi` 只读写 MIDI 文件 | Web MIDI / WEBMIDI.js 可用，需权限 | **文件 MIDI 可替代，硬件 MIDI 仅部分替代**。系统路由和平台一致性不能由 TS wrapper 保证 |
| `rubato` 重采样 | FFmpeg `aresample` / `libswresample`；Web Audio 隐式转换 | 默认外部 FFmpeg | Web Audio 可为试听适配设备采样率 | **离线完全可替代**。正式输出使用 FFmpeg；不要把浏览器隐式重采样当可重现事实源 |
| `hound` WAV I/O | `wavefile`；Mediabunny WAVE；FFmpeg | 可用；大文件/流式优先 FFmpeg 或 Mediabunny | 可用 | **可替代**。`wavefile` 适合 WAV header、cue、BWF/iXML 和小文件，不应承担全部媒体格式与大文件流水线 |

## 3. 候选库、维护状态与运行时边界

### 3.1 FFT 与音频特征

[`FFT.js`](https://github.com/indutny/fft.js) 是 Radix-4/Radix-2 的纯 JS FFT，支持实数/复数正逆变换，并明确建议复用输出 storage 以减少 GC。官方 npm 包当前仍为 `4.0.4`，最后发布于 2022 年，因此更接近“稳定但低活跃”的基础库，不应未经封装扩散到业务代码。[官方包信息](https://www.npmjs.com/package/fft.js?activeTab=versions)

[`Meyda`](https://github.com/meyda/meyda) 面向 JavaScript 音频特征提取，支持离线以及基于 Web Audio 的实时分析，并提供 TypeScript 声明；正式版本 `5.6.3` 发布于 2024-04-21，近两年没有正式版本。[官方发布页](https://github.com/meyda/meyda/releases)

建议：

- 响度、峰值、动态范围使用 FFmpeg `ebur128` / `astats`，避免重复实现标准测量；
- 只在需要结构化频谱 bins、spectral centroid、MFCC 等 LLM 可消费特征时引入 `FFT.js` 或 Meyda；
- 放入 Worker，限制窗口大小、hop size、声道数和总处理时长，并用固定测试音频建立 golden vectors；
- 不在 Node 主事件循环上逐 sample 计算。

FFmpeg 官方的 [`ebur128`](https://ffmpeg.org/ffmpeg-filters.html#ebur128) 可输出 Integrated Loudness、LRA、sample peak 和 true peak；[`astats`](https://ffmpeg.org/ffmpeg-filters.html#astats-1) 可输出 RMS、peak、动态范围和零交叉等时域统计；`showspectrum` / `showspectrumpic` 可生成频谱可视化。[FFmpeg Filters](https://ffmpeg.org/ffmpeg-filters.html)

### 3.2 矩阵与音频 buffer

[`ml-matrix`](https://github.com/mljs/matrix) 提供矩阵运算、求解和分解并自带 TypeScript 定义；官方仓库标注由 Zakodium 维护，`6.15.0` 于 2026-08-05 发布。[官方包信息](https://www.npmjs.com/package/ml-matrix?activeTab=versions)

它只能替代 `ndarray` 的一部分二维线性代数场景。PCM、频谱帧和共享缓冲应优先采用 `Float32Array` 等 TypedArray；跨 Worker 若需要共享，应建立固定 binary layout，而不是把通用 Matrix 对象放在 Agent Tool 合同中。

### 3.3 解码、demux 与编码

首选仍是固定构建的 FFmpeg/ffprobe：[`ffprobe`](https://ffmpeg.org/ffprobe.html) 能以 JSON 等机器可读格式输出容器、stream 和 metadata；FFmpeg 负责长尾格式、滤镜和输出兼容性。Node 通过稳定的异步 [`child_process.spawn()`](https://nodejs.org/api/child_process.html#child_processspawncommand-args-options) 启动受控参数模板，并可使用 `AbortSignal`、timeout 和进程回收。

可选方案：

- [`Mediabunny`](https://github.com/Vanilagy/mediabunny) 是活跃的 Pure TypeScript container/media toolkit，`1.55.1` 于 2026-08-17 发布；在浏览器中主要借助 WebCodecs 解码/编码。WebCodecs 标准明确说明底层 codec 由 User Agent 提供，配置必须通过 `isConfigSupported()` 检查，且实现可以不支持任何组合，因此它不能成为跨机器确定性的 codec 保证。[WebCodecs](https://www.w3.org/TR/webcodecs/)
- Mediabunny 在 Node 中完整编解码需要 [`@mediabunny/server`](https://github.com/Vanilagy/mediabunny/tree/main/packages/server)，该包通过 NodeAV 调用 FFmpeg C API；它提供 TS API，但部署物仍包含 native addon 和 FFmpeg。它适合作为后续替换 CLI 编译器的候选，不是 Pure JS。[Mediabunny Server 官方说明](https://github.com/Vanilagy/mediabunny/blob/main/packages/server/README.md)
- [`NodeAV`](https://github.com/seydx/node-av) 是活跃的 FFmpeg Node-API 原生 binding，`6.1.1` 于 2026-08-20 发布，并提供 Windows/Linux/macOS 预编译包与 Electron 支持。它扩大了原生崩溃域、安装/签名/平台矩阵和升级验证成本，MVP 不应同时维护 NodeAV 与 FFmpeg CLI 两条正式渲染路径。
- [`@audio/decode`](https://github.com/audiojs/decode) 使用 JS/WASM，在 Node 与浏览器解码多种音频格式，`3.12.0` 于 2026-08-11 发布。它适合受控分析流水线，但官方说明 AAC、WMA codec atom 带 GPL 许可证，必须显式 allowlist 依赖和完成许可证检查，不能无差别安装所有 codec。

因此，MVP 采用“FFmpeg/ffprobe 唯一正式媒体事实源”；Mediabunny 只在 Renderer 确实需要精细 container/sample 访问时引入；`@mediabunny/server`、NodeAV 与 `@audio/decode` 先进入 Spike，不进入首批必需依赖。

### 3.4 播放、实时 DSP 与录音

[Web Audio API](https://www.w3.org/TR/webaudio-1.0/) 规定 control thread 与 rendering thread，并以 128 sample-frames 的 render quantum 处理图；AudioWorklet 可在 rendering thread 上运行自定义处理器。它适合 Electron/Web 中的 transport 试听、gain/pan/fade、A/B、基础 EQ 和可视化。

[`Tone.js`](https://github.com/Tonejs/Tone.js) 是建立在 Web Audio 上的交互式音乐框架，提供 transport、synth、effect 和 sample-accurate parameter scheduling；`15.1.22` 于 2026-08-07 发布，当前保持活跃。[官方包信息](https://www.npmjs.com/package/tone?activeTab=versions)

但这不能等价替换 `cpal`：

- Web Audio 的设备、底层 backend、实际 latency 和 buffer 策略由 Chromium/OS 实现控制；
- Renderer 不应为了 Node module 关闭 sandbox。Electron 官方说明 main process 是 Node 环境、renderer 按 Chromium Web 标准运行，并建议启用 sandbox/context isolation，通过窄 IPC 暴露能力。[Electron Process Model](https://www.electronjs.org/docs/latest/tutorial/process-model)、[Electron Security](https://www.electronjs.org/docs/latest/tutorial/security)
- NodeAV 的 Device API 可捕获麦克风，但它是 native FFmpeg binding，不是为 sample-accurate DAW transport、input monitoring 和插件图提供保证的完整引擎。

结论：Web Audio/Tone.js 是 MVP 试听引擎；正式渲染仍由 FFmpeg；低延迟录音和持续实时效果链保持非目标。

### 3.5 MIDI

[Web MIDI](https://www.w3.org/TR/webmidi/) 能枚举 MIDI 端口并发送/接收消息，但只在 Secure Context 暴露，并受权限与 Permissions Policy 控制，User Agent 也可因平台或安全原因拒绝。

[`WEBMIDI.js`](https://github.com/djipco/webmidi) 是 Web MIDI 的高层 wrapper，`3.1.16` 于 2026-03-31 发布，支持浏览器和 Node。其官方文档明确说明 Node 支持依赖 JZZ；而 [`JZZ`](https://github.com/jazz-soft/JZZ) 的 package 直接依赖 `jazz-midi`，后者包含 native 与 node-gyp 目录。因此 Node 设备 MIDI 不是 Pure TypeScript，只是 TS/JS API 包装。[WEBMIDI.js 环境说明](https://webmidijs.org/docs/getting-started/)、[JZZ package.json](https://github.com/jazz-soft/JZZ/blob/master/package.json)、[`jazz-midi`](https://github.com/jazz-soft/jazz-midi)

[`@tonejs/midi`](https://github.com/Tonejs/Midi) 可在 Node 与浏览器读写 Standard MIDI File，但不访问硬件端口；正式版本 `2.0.28` 自 2022-04-07 未更新，因此适合通过窄 adapter 与 fixture 测试使用，不适合作为整个 MIDI runtime。[官方包信息](https://www.npmjs.com/package/@tonejs/midi?activeTab=versions)

建议 MVP 只实现 MIDI 文件导入/导出。硬件 MIDI 若要试验，放在 Electron Renderer 的 Web MIDI 权限流中，并明确“best effort”；不要承诺虚拟端口、system routing、MIDI clock 或跨平台一致时序。

### 3.6 重采样与 WAV

FFmpeg 的 [`libswresample` / `aresample`](https://www.ffmpeg.org/ffmpeg-resampler.html) 提供采样率转换、声道 rematrix、sample format 和 planar/packed 转换，是正式预览与导出的推荐路径。

[`wavefile`](https://github.com/rochars/wavefile) 可在 Node 和浏览器读取、创建和修改 WAV，支持 RIFF/RIFX、cue、BWF、iXML、bit depth 和 sample rate 等操作；官方说明文件上限为 2 GB。`11.0.0` 自 2022-05-24 未发布新版本，应只用于窄 WAV metadata/cue adapter，并以真实 fixture 回归；大文件、流式输出和最终编码继续使用 FFmpeg。[官方包信息](https://www.npmjs.com/package/wavefile?activeTab=versions)

## 4. 推荐的无 Rust Adapter 架构

```text
LLM / Agent Run
      │ semantic tool call
      ▼
TypeScript Tool Registry + Policy + Approval
      │
      ▼
Deterministic Audio Tool Executor (Hono / Node)
      ├── FFmpegCompiler ──spawn── bundled ffmpeg/ffprobe
      ├── AnalysisWorker ───────── FFT.js / Meyda / TypedArray
      ├── MidiFileAdapter ──────── @tonejs/midi (optional)
      └── WavMetadataAdapter ───── wavefile (optional)

Electron Renderer (sandboxed)
      ├── Web Audio / Tone.js preview graph
      ├── AudioWorklet / Web Worker
      └── optional Web MIDI permission flow
```

边界规则：

- Project、Timeline、Asset Version、Agent Run 与 Job 的权威状态只在 TypeScript Core；
- 正式输出由固定版本 FFmpeg 和版本化参数编译器生成；Renderer 试听不成为正式 Asset 的事实源；
- PCM 不经过 LLM、HTTP JSON 或 Agent context；只传 Asset 引用、分析摘要和有界结果；
- 每个媒体 Tool 必须有 schema、revision、input hash、idempotency key、timeout、资源上限和 staging → validation → atomic commit；
- `spawn()` 使用参数数组且 `shell: false`，参数来自 allowlist compiler；不接受任意 filter string、绝对路径或用户提供的可执行程序；
- CPU/WASM 分析放 Worker；Electron renderer 保持 sandbox/contextIsolation，不开启 Node integration；
- NodeAV/native addon 若未来采用，必须作为单独 ADR 和发布 Gate，不能被“TypeScript 包”这一表象绕过。

## 5. MVP 依赖清单

### 必需

1. 固定版本并随应用分发的 `ffmpeg` + `ffprobe`；
2. Node 标准 `child_process.spawn()`；
3. Renderer 原生 Web Audio API；
4. 项目内的 TypeScript `FFmpegCompiler`、`ProcessSupervisor`、`AudioEnginePort` 和 Tool schemas。

### 条件引入

| 依赖 | 只在何时引入 | MVP 默认 |
|---|---|---|
| `FFT.js` | LLM/质量评测确实需要数值频谱 bins | 暂不必需 |
| Meyda | 需要 spectral/MFCC 等标准特征原型 | 暂不必需 |
| `ml-matrix` | 有明确二维线性代数算法，不为“将来可能需要” | 不引入 |
| Tone.js | 原生 Web Audio graph 不能简洁表达 transport/preview | Spike 后决定 |
| Mediabunny | Renderer 需要 sample/container 级读写，HTML media element 不足 | Spike 后决定 |
| `@audio/decode` | 需要无 FFmpeg 的 Worker 解码或精细 PCM 分析 | 许可证审核后决定 |
| `@tonejs/midi` | MVP 接受 Standard MIDI File | 可选 |
| `wavefile` | 需要编辑 cue/BWF/iXML，而 FFmpeg 路径不便 | 可选 |
| `@mediabunny/server` / NodeAV | CLI 启动开销或逐 frame API 成为可测瓶颈 | MVP 不引入 |
| WEBMIDI.js/JZZ | 产品明确接受 best-effort hardware MIDI | MVP 不引入 |

不要采用已经归档的 `fluent-ffmpeg` 一类 wrapper。FFmpeg contract 应由 Auto Studio 自己的窄类型编译器控制，避免第三方命令字符串 builder 成为核心依赖。

## 6. LLM Tool 实现映射

| LLM Tool | 默认实现 | 可选 TS/JS 实现 | 说明 |
|---|---|---|---|
| `audio.inspect` | ffprobe JSON | Mediabunny / wavefile | 正式 metadata schema 由 Core 归一化 |
| `audio.measure_loudness` | FFmpeg `ebur128` + `astats` | 无需另造算法 | 返回 LUFS/LRA/peak 摘要，不返回日志全文 |
| `audio.analyze_spectrum` | FFmpeg 可视化；PCM pipe + AnalysisWorker | `FFT.js` / Meyda | numeric bins 必须固定窗口、hop、scale 和版本 |
| `audio.create_edit_plan` | Pure TypeScript Timeline command | 无 | 只改非破坏性 project state，不直接处理 PCM |
| `audio.render_preview` | FFmpeg filter graph | Renderer Web Audio 仅做即时试听 | 生成新的 preview Asset Version |
| `audio.render_mix` | FFmpeg `amix`、gain/pan/fade/filter | 后续可评估 NodeAV | LLM 不提供 `filter_complex` 字符串 |
| `audio.resample` | FFmpeg `aresample` | 无 | 固定 quality/profile，记录输入输出采样格式 |
| `audio.export` | FFmpeg | Mediabunny 仅用于特定 Web 导出 | validate 后原子提交，不覆盖源文件 |
| `audio.read_midi_file` | `@tonejs/midi` | 自有 SMF adapter | 不等同硬件 MIDI |
| `audio.capture_midi` | 暂不提供 | Electron Web MIDI / WEBMIDI.js Spike | 需要显式设备权限和能力探测 |
| `audio.read_wav_metadata` | wavefile | ffprobe | 只在需要 cue/BWF/iXML 时单独存在 |

## 7. 主要风险

1. **双重渲染差异**：Web Audio 试听与 FFmpeg 正式输出可能不同。必须用同一 Timeline semantics 编译两套 graph，并把 FFmpeg 输出作为权威结果。
2. **Codec 可用性漂移**：WebCodecs 由 User Agent 决定。启动时做 capability probe；正式导入/导出不依赖它。
3. **主线程和 GC**：FFT、全文件 decode 或大矩阵会阻塞 Node/Renderer。强制 Worker、TypedArray、transferable buffer 和资源上限。
4. **包维护强弱不一**：FFT.js、Meyda、`@tonejs/midi`、wavefile 发布较慢。全部经窄 adapter、lockfile、fixture/golden tests 隔离，不让其类型进入领域合同。
5. **WASM 许可证与体积**：`@audio/decode` 的部分 codec 有 GPL 等不同许可证。按 codec atom allowlist，并在构建产物中生成许可证与哈希清单。
6. **Native package 伪装成 TS**：`@mediabunny/server`、NodeAV、JZZ hardware MIDI 会引入 native binary。必须纳入 OS/arch、签名、公证、更新和崩溃恢复测试。
7. **FFmpeg 参数与资源攻击**：不接受 LLM 自由参数；限制时长、采样率、声道、filter 数、输出大小、超时和并发，并回收整个进程树。

## 8. 一周 Spike 与验收 Gate

### Day 1：FFmpeg Tool 骨架

- 固定并记录 ffmpeg/ffprobe version、build configuration 与 binary hash；
- 实现 `spawn(shell: false)`、取消、timeout、stderr 上限、进程树回收和 staging output；
- 跑通 WAV、FLAC、MP3、AAC/M4A 各一个 fixture 的 inspect → decode/render → validate。

### Day 2：确定性 Timeline 编译

- 实现 trim、gain、pan、fade、mute、两至八轨 `amix` 和 `aresample`；
- 相同 revision + input hash + profile 必须生成相同 command plan 与 idempotency key；
- LLM 输入中出现路径、shell、任意 filter 时必须拒绝。

### Day 3：分析 Worker

- 用 FFmpeg 解码为限定长度的 `f32le` PCM，Worker 运行 `FFT.js`/Meyda PoC；
- 对 1 kHz sine、silence、impulse、pink noise 建 golden vectors；
- Core API 在分析期间仍能通过并发健康检查，内存峰值和输出 JSON 大小受限。

### Day 4：Electron 试听

- sandbox + contextIsolation 下使用 Web Audio 播放、seek、gain/pan/fade 和 A/B；
- 不启用 Node integration；所有项目/文件操作经验证过的 IPC；
- 记录冷启动、首次出声、seek 响应、20 分钟连续播放掉音和内存增长。

### Day 5：跨平台、失败恢复与决策

- 至少在两个首发 OS/arch 验证打包后的 FFmpeg 可启动、可取消、路径含空格/Unicode 可处理；
- kill Core/ffmpeg 后重启，Project 与源 Asset 不损坏，staging 可清理，Job 可重试；
- 对比 Web Audio preview 与 FFmpeg bounce 的时长、起止点、增益和淡入淡出，误差需在预先记录的容差内。

通过条件：

- 全部 MVP Tool 不需要 Rust、native addon 或无 sandbox Renderer；
- 8 轨、48 kHz stereo、目标时长的 preview render 在产品预算内完成，过程可取消且不阻塞 Core；
- loudness/peak 和 spectrum golden tests 稳定；
- FFmpeg command plan 100% 来自 allowlist compiler；
- 安装包在目标平台包含可追溯 FFmpeg binary、许可证和 hash；
- 失败恢复不会覆盖源文件或留下“完成但无产物”的状态。

失败并不自动意味着改 Rust：先区分 FFmpeg 参数、Web Audio graph、进程监督、磁盘 I/O、codec 兼容和真正的实时设备瓶颈。只有需求明确落入低延迟全双工、原生插件宿主或稳定系统级 MIDI，才重新评估原生 Audio Engine。

## 9. 最终建议

保持 TypeScript Core，不做 Rust Adapter，并采用：

1. **正式媒体数据面：固定 FFmpeg/ffprobe 外部进程**；
2. **交互试听数据面：Electron Renderer 的 Web Audio/AudioWorklet**；
3. **数值分析：有界 Worker + TypedArray，按需采用 FFT.js/Meyda**；
4. **格式与 MIDI 辅助：只通过窄 adapter 按需引入 wavefile、Mediabunny、`@audio/decode`、`@tonejs/midi`**；
5. **不承诺 Pure TS 无法保证的专业实时能力**。

这不是“用 TypeScript 重写 FFmpeg 或音频驱动”，而是让 TypeScript 保持产品控制面，把成熟原生能力封装成确定性、可恢复、可审计的内置 Tool。对当前内容质量优先的 MVP，这是维护成本最低且边界最清楚的方案。
