# Rust 音频技术栈与 Auto Studio 架构适配评估

> **历史报告**：本文基于“只做简单编辑、不做专业实时引擎”的 v0.4 产品范围，因此当时不建议整体切换 Rust。产品范围现已改变，[ADR-0004](../adr/0004-rust-core-professional-audio-engine.md) 接受 Rust Core 与 Audio Engine，[ADR-0007](../adr/0007-progressive-product-proof-before-audio-and-vst3.md) 将专业 Audio Engine 后置到 Ship 1、VST3 后置到 Ship 2；新的采样、许可和技术结论见[后续调研](./instrument-sample-libraries-and-rust-audio-stack-2026-08-21.md)。本文保留的 crate 边界与风险说明仍可参考，旧架构和插件阶段结论不再适用。

> 日期：2026-08-21  
> 调研范围：`rustfft`、`ndarray`、`symphonia`、`cpal`、`fundsp`、`nih-plug`、`midir`、`rubato`、`hound`  
> 决策问题：这些 Rust 库是否意味着 Auto Studio 应将 TypeScript Core 整体切换为 Rust？  
> 来源原则：仅使用项目官方仓库、官方 API 文档、标准或运行时官方文档

## 1. 结论

**不建议因为这组库把 Auto Studio 的 Agent/Core 整体切换为 Rust。**

它们证明的是：Rust 已经具备构建原生实时音频 I/O、DSP、MIDI 和音频插件的可用基础；它们没有证明 Provider Adapter、Agent Run、项目状态、API、CLI 和内容质量评测也应改写为 Rust。

当前产品范围已经明确：MVP 只做非破坏性剪切、排列、增益、声像、淡入淡出、静音和基础响度预览，不托管 VST/AU 插件，也不实现低延迟录音或实时效果链。见[产品设计的非目标](../product/ai-creative-agent-product-design.md#32-mvp-非目标)和[简单编辑与混音](../product/ai-creative-agent-product-design.md#114-简单编辑与混音)。这些任务可以由 **TypeScript Core 编排 FFmpeg，Electron/Web Audio 负责交互试听** 完成。

因此推荐两阶段方案：

1. **MVP 保持 TypeScript Core**：Agent、Provider、项目状态、任务恢复、CLI/API、离线媒体处理继续使用 Hono + Node.js + TypeScript，正式媒体输出由 FFmpeg/ffprobe 负责。
2. **专业实时能力进入产品范围后，增加 Rust Audio Engine**：仅让 Rust 拥有音频设备、实时回调、DSP 图、MIDI 和插件宿主；TypeScript 仍是控制面。是否增加该引擎由可测量需求触发，不由 crate 清单触发。

这会修正 [ADR-0003](../adr/0003-typescript-core.md) 的未来演进路径，但当前没有足够证据推翻它。

## 2. 先纠正图片中的能力映射

图片列出的不是九个可拼装成完整 DAW 的同层组件，而是不同层级的原语和工具包。

| 需求 | 库 | 官方定位 | 不能替代什么 | 适用层 |
|---|---|---|---|---|
| FFT | `rustfft` | 纯 Rust、SIMD 加速的 FFT 计算库，支持任意长度变换 | 不提供 STFT 窗函数、overlap-add、频谱编辑、效果器或音频图 | DSP 数学原语 |
| 矩阵计算 | `ndarray` | 通用 N 维数组、view、slice、迭代与可选 BLAS | 不是音频专用 DSP，也不是完整 NumPy/科学计算运行时 | 离线分析、特征矩阵 |
| 音频解码 | `symphonia` | 音频解码、媒体 demux 和元数据读取 | 不是通用编码器，也不覆盖 FFmpeg 的完整 codec/container/filter 范围 | 文件导入与解码 |
| 实时播放 | `cpal` | **低层**跨平台音频输入/输出与设备 stream | 不提供 transport、轨道、混音器、DSP 图、自动化或时间线 | 原生音频设备 I/O |
| DSP | `fundsp` | 音频处理、合成和组合式 DSP graph | 不是制作完成的 DAW Engine；工程级 transport、automation、routing、PDC 等仍需自建 | DSP 图与原型 |
| 插件 | `nih-plug` | 开发并导出 VST3/CLAP 插件，也可生成 standalone | **不是加载第三方插件的插件宿主** | 开发 Auto Studio 自有插件 |
| MIDI | `midir` | 跨平台实时 MIDI 端口输入/输出 | 不提供高层消息模型、Standard MIDI File 编辑器、钢琴卷帘或 tempo map | MIDI 设备 I/O |
| 采样率转换 | `rubato` | 固定比率同步重采样、可变比率异步重采样 | 不是 async/await API，也不是完整音频 I/O 或媒体转换器 | 实时/离线重采样 |
| WAV 读写 | `hound` | WAVE PCM/IEEE Float 读写 | 不是通用音频编码器，不支持 MP3/AAC/FLAC 等交付格式 | 窄范围 WAV I/O |

官方依据：

- [`rustfft`](https://github.com/ejmahler/RustFFT) 的官方说明只承诺 FFT 和 SIMD 加速。
- [`ndarray`](https://github.com/rust-ndarray/ndarray) 定位为通用 N 维容器，并明确仍在持续演进、版本间可能发生破坏性变化。
- [`symphonia`](https://github.com/pdeljanov/Symphonia) 官方定位是 decoding、demuxing 和 tag reading。
- [`cpal`](https://github.com/RustAudio/cpal) README 第一行将其定义为 low-level audio input/output，并建议更高层播放使用其他库。
- [`fundsp`](https://github.com/SamiPerttu/fundsp) 面向音频处理、合成、音乐制作和 DSP 原型。
- [`nih-plug`](https://github.com/robbert-vdh/nih-plug) 的导出目标是 VST3/CLAP plugin，而不是 host。
- [`midir`](https://github.com/Boddlnagg/midir) 面向实时 MIDI I/O，并明确高层 MIDI 消息 API 仍可能在未来加入。
- [`rubato`](https://docs.rs/rubato/latest/rubato/) 将固定比率称为 synchronous、可运行时改变比率称为 asynchronous。
- [`hound`](https://github.com/ruuda/hound) 仅承诺 WAVE 的读取与写入。

## 3. 逐项可行性、维护状态与限制

### 3.1 `rustfft`

`rustfft` 是成熟的 FFT 基础原语。它支持 x86_64 的 AVX/SSE、AArch64 的 NEON，以及可选的 WASM SIMD；适合频谱分析、卷积或构建 STFT 算法的底层步骤。[官方仓库](https://github.com/ejmahler/RustFFT)当前正式版本为 [`6.4.1`](https://github.com/ejmahler/RustFFT/releases/tag/6.4.1)，发布于 2025-09-18。

限制是它只计算 FFT。Auto Studio 仍需自己实现或选择：

- 窗函数与 hop size；
- overlap-add/overlap-save；
- 多声道缓冲布局；
- 增益归一化和边界处理；
- 实时内存与调度策略；
- 面向用户的频谱编辑语义。

结论：它支持“Rust 适合写 DSP 内核”，不支持“整个业务 Core 应改成 Rust”。

### 3.2 `ndarray`

`ndarray` 提供通用多维数组、view、slice、算术和可选 BLAS/Rayon。官方 README 明确说项目仍在演进，版本间可能有 breaking changes；当前正式版本是 [`0.17.2`](https://github.com/rust-ndarray/ndarray/releases/tag/0.17.2)。

它适合离线响度分析、特征矩阵和谱图中间结果，但实时回调更常需要固定布局、预分配、无锁或 ring buffer。把 `ndarray` 放进技术清单，不等于获得矩阵算法、机器学习推理或实时安全。

结论：仅在确有多维数值计算时采用，不把它设为所有音频 buffer 的默认抽象。

### 3.3 `symphonia`

`symphonia` 0.6.1 于 2026-08-13 发布，仓库保持活跃。它提供纯 Rust 音频解码、媒体解复用、元数据、格式/解码器自动检测和基础音频 buffer。[官方支持表](https://github.com/pdeljanov/Symphonia#roadmap)显示：

- Wave demux 为 Excellent，OGG/MP4/AIFF 为 Great；CAF 和 MKV/WebM 为 Good；
- MP3、FLAC、PCM、Vorbis 解码为 Excellent；AAC-LC/ALAC 为 Great；
- HE-AAC、HE-AACv2、Opus 和 WavPack 等仍有未完成项；
- 某些非默认 codec/format 需要显式 feature；
- C API 与 WASM API 仍列在 planned features。

它的核心定位是**解码和 demux**，不是完整的编码、filter 和导出系统。专业工作站需要接受用户和 Provider 的长尾媒体格式，短期仍应保留 FFmpeg 作为兼容性事实源。

结论：未来 Rust Engine 可用它进行受控解码；MVP 不要用它替换 FFmpeg。

### 3.4 `cpal`

`cpal` 0.18.2 于 2026-08-16 发布，官方仓库仍活跃。它能枚举 host/device、查询 stream configuration、创建输入/输出 stream，并覆盖 CoreAudio、WASAPI、ALSA、JACK、PipeWire、PulseAudio、AAudio 和 Web Audio 等后端。[官方 README](https://github.com/RustAudio/cpal)也直接列出了平台差异：

- Linux 需要 ALSA development files，其他后端有各自系统依赖；
- Windows ASIO 需要 ASIO SDK、LLVM/Clang 和相应驱动；
- AudioWorklet WASM backend 需要 nightly、atomics 和 Cross-Origin headers；
- 请求更小 buffer 会降低延迟，同时提高掉音风险；
- realtime priority 在不同系统有权限和配置要求。

`cpal` 是设备与 callback 层，不解决 session transport、轨道渲染、插件图、延迟补偿、automation 或离线 bounce。

结论：只有需要原生低延迟设备 I/O 时才成为必要组件；基础试听并不要求它。

### 3.5 `fundsp`

`fundsp` 当前 crate 版本为 0.23.0。它提供静态 `AudioNode`、动态 `AudioUnit`、组合式 graph、单 sample 与 block processing、SIMD buffer，以及可分离的 frontend/realtime-safe backend。[官方文档](https://docs.rs/fundsp/latest/fundsp/)要求在进入实时上下文前预分配，并指出动态网络存在额外开销、block processing 可缓解该开销。

需要注意两个工程信号：

- 所有 `AudioNode`/`AudioUnit` 使用 `f32`；
- docs.rs 上最新 0.23.0 的文档构建当前失败，最后成功构建版本是 0.20.0；这不是“项目不可用”的证据，但应进入依赖 Spike 和供应链 Gate。

它很适合验证 EQ、filter、oscillator、envelope、合成和效果图；它没有交付完整 DAW 的 transport、时间线、sample-accurate automation、plugin latency compensation、冻结/反冻结、undo 或 project persistence。

结论：作为 Rust Engine 的 DSP 候选库，而不是把它误认为完整 Audio Engine。

### 3.6 `nih-plug`

这是图片中最关键的误判。

[`nih-plug`](https://github.com/robbert-vdh/nih-plug) 是开发音频插件的框架：实现一次 Plugin trait，再导出为 VST3/CLAP，或生成 standalone。它没有提供让 Auto Studio 扫描、加载、运行用户第三方插件的 host 层。

同时，官方 README 明确标注原框架处于 **maintenance mode**，并推荐[社区 fork](https://codeberg.org/BillyDM/nih-plug)。此外，原项目的 VST3 bindings 是 GPLv3；商业产品不能在未完成许可证评估前直接把该路径视为可发布方案。

如果需求是“让 Auto Studio 成为插件宿主”：

- CLAP 可研究 [`clack-host`](https://github.com/prokopyl/clack)，但其官方定位是低层安全 wrapper，作者明确表示目前没有成熟的高层 Rust host 替代品；
- VST3 需要单独使用和评估 [Steinberg VST3 SDK/Host API](https://steinbergmedia.github.io/vst3_dev_portal/)；
- 仍需实现 plugin scan、quarantine、state/preset、parameter automation、latency reporting/compensation、GUI embedding、线程模型和崩溃隔离。

结论：`nih-plug` 不能作为“Rust 已经解决插件宿主”的依据。MVP 也已经明确不托管 VST/AU 插件。

### 3.7 `midir`

`midir` 0.11.0 面向跨平台实时 MIDI I/O，支持 ALSA、WinMM/WinRT、CoreMIDI、JACK、Web MIDI 和 Android AMidi。官方 README 说明它支持 SysEx 和虚拟端口，但 Windows 不支持虚拟端口，而且没有 RtMidi 风格的内建 message queue；队列应由调用者基于 callback/channel 实现。[官方仓库](https://github.com/Boddlnagg/midir)还明确把高层消息 parsing/assembling 列为可能的未来能力。

结论：适合 MIDI 键盘、控制器和外部设备连接；MIDI 文件导入导出、tempo map、piano roll 与编曲语义仍需其他模块。

### 3.8 `rubato`

`rubato` 5.0.0 于 2026-08-10 发布，提供：

- 固定采样率比的同步 `Fft` resampler，适合 44.1 kHz → 48 kHz 这类离线转换；
- 运行时可改变比率的异步 `Async` resampler，适合输入/输出设备时钟漂移；
- 高质量但 CPU 更重的 sinc，以及更快但没有抗混叠滤波的 polynomial 模式；
- 为实时处理提供 `process_into_buffer()`，在预分配后避免处理期分配或阻塞。

“asynchronous” 指输入输出时钟不锁定、采样率比可变化，**不是 JavaScript/Rust async-await**。

更重要的是，[官方 real-time considerations](https://docs.rs/rubato/latest/rubato/#real-time-considerations)明确要求设备 callback 保持轻量，不应在 callback 内做重采样；callback 应把数据写入共享/ring buffer，由独立处理 loop 重采样和写盘。

结论：实时安全需要架构配合，不能只因用了 `rubato` 就认为 callback 不会掉音。

### 3.9 `hound`

`hound` 的正式 crate 版本仍为 3.5.1。它读取和写入 WAVE，覆盖 integer PCM 与 IEEE Float，适合简单、低依赖的 WAV 中间文件。[官方仓库](https://github.com/ruuda/hound)没有承诺 MP3、AAC、FLAC、OGG 或容器级兼容。

结论：可作为窄 WAV adapter；不能替代 FFmpeg、Symphonia 或完整 export pipeline。

## 4. TypeScript 方案是否仍然可行

### 4.1 Agent 与 Provider 编排

这部分继续使用 TypeScript 更合适：

- 工作负载主要是 Provider HTTP/SSE/WebSocket、schema 演进、Agent Decision、Job 状态与项目事务；
- 外部模型等待、下载和 FFmpeg 子进程是主要延迟，不是 FFT 或音频 callback；
- 与 GUI/Web 共享合同、类型和测试样本有利于快速做内容质量实验；
- 切换 Rust 不会直接改善模型生成的音乐质量、Creative Brief、候选比较或人工反馈闭环。

Rust 音频库与这一层几乎没有直接关系。

### 4.2 离线媒体处理

[FFmpeg 官方文档](https://www.ffmpeg.org/documentation.html)覆盖 codec、mux/demux、audio resampler、filter、设备与格式；[`ffmpeg` CLI](https://www.ffmpeg.org/ffmpeg.html)可以转换媒体并运行音视频 filter graph。Node 的 [`child_process.spawn()`](https://nodejs.org/api/child_process.html)是稳定、异步且不阻塞事件循环的子进程接口。

因此下列 MVP 能力不需要把 Core 改成 Rust：

- 裁切、拼接、淡入淡出、增益、声像与静音；
- stems 对齐、简单混音和 bounce；
- sample rate/sample format 转换；
- loudness、peak、silence 和格式探测；
- WAV/FLAC 等开放交付文件；
- 视频与音乐最终合成。

TypeScript 在这里不是亲自逐 sample 做 DSP，而是作为可恢复、可审计的媒体任务编排器。

### 4.3 交互试听与基础混音

[Web Audio API](https://www.w3.org/TR/webaudio-1.0/)是处理和合成音频的标准化高层 graph API，实际处理通常由浏览器底层优化实现完成；`AudioWorklet` 允许在音频渲染线程运行自定义 processor。因此 Electron/Local Web 可以继续用 Web Audio 做 waveform、A/B、gain/pan/fade 预览。

局限是浏览器音频设备、plugin host、系统路由和确定性低延迟控制不等于专业 DAW。Web Audio 适合 MVP 预览，不应成为正式 Asset/Export 的唯一事实源；这一点与[当前技术设计](../design/auto-studio-technical-design.md#13-media-module)一致。

### 4.4 TypeScript 调用原生能力

如果出现单个 CPU 密集算法，而还没有完整 Rust Engine 需求，可选两种窄桥接：

- 批处理工具：TypeScript 通过 `spawn()` 调用 Rust CLI，输入输出使用文件或有界 binary stream；
- native addon：Node 的 [`Node-API`](https://nodejs.org/api/n-api.html)是 ABI-stable 的稳定接口，可让其他语言实现 addon。

native addon 不等于免费抽象：native panic、非法内存或失控插件可能直接拖垮 Core，且需要为每个 OS/arch 构建、签名和分发 binary。对于第三方插件和常驻实时音频，更推荐独立 Rust Audio Engine 进程；对于一个纯函数式 DSP kernel，Node-API 才可能更简单。

## 5. 三种路线对比

| 维度 | Pure TypeScript + FFmpeg/Web Audio | 全量 Rust Core | TypeScript Control Plane + Rust Audio Engine |
|---|---|---|---|
| Provider/Agent 迭代 | 最快，复用 JS SDK 与前端 schema | 通常需手写更多 Provider client/schema | 保持 TypeScript 优势 |
| 离线编辑/导出 | FFmpeg 足够覆盖 MVP | 可调用 FFmpeg，也可逐步原生化 | 仍由 FFmpeg；必要 DSP 进入 Engine |
| 原生低延迟 I/O | Web Audio 有边界，不适合作为完整 DAW 保证 | `cpal`/原生线程模型合适 | `cpal` 只放在 Engine，合适 |
| DSP 与 MIDI | JS/WASM 可做原型，工程上限需实测 | Rust crate 组合自然 | Rust 拥有数据面，TS 发控制命令 |
| 第三方插件宿主 | 缺少可信直接路径 | 仍需另写 CLAP/VST3 host | 插件只进入隔离的 Engine/worker |
| 进程可靠性 | 单 Runtime 简单；addon 会扩大崩溃域 | 原生 Core 资源可控 | Engine 可重启，Core/项目状态不随插件一起崩溃 |
| 分发 | Node + FFmpeg + Electron | Rust + FFmpeg + Electron，GUI 仍是 TS | Node + Rust + FFmpeg，构建矩阵最大 |
| 团队维护 | 一种业务语言 | GUI 与 Core 两语言，Provider 迁移成本高 | 两语言但边界与必要性清晰 |
| 内容质量 | 语言无直接增益 | 语言无直接增益 | 把工程预算集中在质量与真正的音频瓶颈 |
| 当前适配 | **最佳** | 不推荐 | 未来专业实时阶段的推荐目标 |

**推荐路线不是“Pure TypeScript 永久化”，而是“TypeScript 先完成产品闭环，Rust 由音频数据面需求触发”。**

## 6. 建议的长期分层

```text
┌──────────────── TypeScript Control Plane ────────────────┐
│ Hono API / Agent Runtime / Provider Adapters / Project   │
│ Job Recovery / Quality Evaluation / CLI / OpenAPI        │
└──────────────────────┬───────────────────────────────────┘
                       │ versioned AudioEngine Port
                       │ commands/events, no PCM over JSON
              ┌────────┴─────────────────┐
              │                          │
       MVP implementation          Future implementation
       FFmpeg + ffprobe             Rust Audio Engine
       Web Audio preview            ├─ CPAL device I/O
                                    ├─ realtime scheduler/ring buffers
                                    ├─ DSP + resampling
                                    ├─ MIDI device I/O
                                    └─ isolated plugin workers
```

边界规则：

- TypeScript 是项目事实、Agent 决策、Provider Job 和用户权限的 owner；
- Rust Engine 是播放头、设备 stream、实时 DSP graph、MIDI 时序和 plugin instance 的 owner；
- 控制命令使用版本化 IPC；PCM 不通过 HTTP/JSON 来回传输；
- 音频 callback 不调用 Node、不做磁盘/网络、不等待锁、不分配大对象；
- Engine 崩溃不能破坏 Project SQLite，Core 应把它表现为可恢复的 playback/render failure；
- 离线正式渲染必须可重现，并在 Engine 与 FFmpeg 之间明确事实源；
- 不建立 Node Provider Sidecar；Rust 只服务媒体数据面，不复制 Agent/Provider 业务。

## 7. 引入 Rust Audio Engine 的触发条件

满足下面任一项，并且 TypeScript/Web Audio/FFmpeg Spike 不能达到验收指标时，才开启 Rust Engine：

1. 需要原生全双工录音、input monitoring 或稳定的小 buffer 播放；
2. 需要 sample-accurate transport、automation 或持续运行的实时效果链；
3. 需要 MIDI 硬件输入、输出、clock 或低抖动事件调度；
4. 需要成为第三方 CLAP/VST3 插件宿主；
5. Web Audio 在目标轨道数、效果数或首发硬件上出现可重复的掉音/GC/延迟阻断；
6. 某个核心质量算法必须逐 sample 运行，而 FFmpeg、Web Audio、WASM 或隔离批处理均不能满足性能/质量。

以下情况不能成为迁移理由：

- “Rust 音频 crate 看起来更全”；
- “以后可能做完整 DAW”；
- 希望通过语言选择提高 AI 模型生成质量；
- 还没有设备、buffer size、轨道数和效果图的目标工作负载；
- 只是需要 WAV 读写、FFmpeg 转码或波形绘制。

## 8. 1–2 周 Spike 与验收 Gate

Spike 的目标不是实现 DAW，而是回答“是否现在就需要 Rust Engine”以及“边界是否可维护”。只针对一个首发 OS 和一套明确硬件，之后再验证第二 OS 的可构建性。

### 8.1 第 1 周：音频数据面

实现：

- `cpal` 枚举设备、选择 sample format/buffer size、播放与可选录音；
- callback 只做预分配 buffer 的读取/写入；
- lock-free/ring buffer 连接实时 callback 与处理线程；
- 处理线程完成 gain、pan、fade、两到八轨混音和一次重采样；
- `rubato` 在 callback 外运行；
- 一个 MIDI 输入经 `midir` 转为带 timestamp 的内部事件；
- 用 FFmpeg 生成基准文件，比较离线输出。

Gate：

- 在约定的 48 kHz、目标 buffer size 和目标轨道数下连续运行 30 分钟，无 Engine 造成的 underrun/overrun；
- callback P99 执行时间小于单 buffer deadline 的 50%，且 callback 路径无文件、网络、日志和 heap allocation；
- 设备 start/stop、默认设备变化、拔插和权限拒绝都有明确状态，不导致 Core 崩溃；
- 固定输入的离线输出与 FFmpeg 基准在预先定义的峰值误差、响度和长度容差内一致；
- 重采样没有 startup silence/tail truncation，正确使用 `process_all()` 或按 chunk flush；
- MIDI 事件不从 TypeScript 定时器进入音频 callback，时间戳与调度语义有单测。

### 8.2 第 2 周：TypeScript 集成与故障边界

实现：

- TypeScript Core 启动、握手、控制和停止 Rust Engine；
- 协议包含 engine version、capabilities、device config、transport command 和 bounded event；
- Engine 内部持有 PCM、DSP 和 device；Core 不通过 JSON 搬运 sample buffer；
- 在 Engine processing、设备 callback 和 IPC 断开处注入退出；
- 如果插件宿主已经是近期范围，只增加一个 CLAP host 最小验证，不在本次承诺 VST3。

Gate：

- Engine 异常退出后，Core、Project SQLite、Agent Run 和 Provider Job 继续可用；
- Core 能识别 stale Engine、重新握手并恢复非破坏性播放状态；
- 500 次 transport/parameter 命令无协议漂移、无未界定队列增长；
- 安装包能携带正确目标的 Engine binary，启动时校验版本和完整性；
- 第二目标 OS 至少完成 CI build；系统依赖和签名要求已列清；
- 若做 CLAP host，坏插件或超时插件不会拖垮 Core，并有 scan/quarantine 设计；
- 团队能在不修改 Agent/Provider 代码的情况下替换 Fake Engine 与 Rust Engine。

### 8.3 Go/No-Go

**Go：** 产品路线已经包含上述实时触发条件，Rust Spike 通过全部 Gate，且 TypeScript/Web Audio 对照未通过相同指标。此时采用“TypeScript Control Plane + Rust Audio Engine”。

**No-Go：** 当前仍只需要基础试听、离线编辑、开放导出，或者任一关键 Gate 未通过。继续使用 TypeScript + FFmpeg/Web Audio，不把半成品 Engine 写入正式架构。

**不得以 Spike 为由改写整个 Core。** 如果未来确实要改写 Core，需要独立证据证明 Agent/Provider/项目事务也遇到了无法在 TypeScript 中消除的阻断，并新建替代 ADR。

## 9. 最终建议

| 时间 | 建议 |
|---|---|
| 当前 MVP | 保持 Hono/Node.js/TypeScript Core；FFmpeg/ffprobe 负责正式离线媒体；Web Audio 负责交互试听 |
| 架构预留 | 在技术设计中预留窄 `AudioEngine` Port，但不建立 Rust workspace、不冻结 IPC schema |
| 首次 Rust 使用 | 仅在低延迟播放/录音、MIDI 或实时 DSP 成为已确认需求后，执行 1–2 周 Spike |
| 插件阶段 | 先独立定义“开发插件”还是“宿主插件”；不要把 `nih-plug` 当 host；CLAP 与 VST3 分开评估 |
| 长期 | 若 Spike 通过，采用 TS Control Plane + Rust Audio Engine；仍不把 Provider 和 Agent 编排迁入 Rust |

一句话决策：**Rust 更适合 Auto Studio 未来的专业实时音频引擎，但目前不更适合整个 AI Agent Core。**

## 10. 官方来源

Rust 音频库：

- [RustFFT 官方仓库](https://github.com/ejmahler/RustFFT)；[6.4.1 Release](https://github.com/ejmahler/RustFFT/releases/tag/6.4.1)
- [ndarray 官方仓库](https://github.com/rust-ndarray/ndarray)；[0.17.2 Release](https://github.com/rust-ndarray/ndarray/releases/tag/0.17.2)
- [Symphonia 官方仓库与 codec/format 状态表](https://github.com/pdeljanov/Symphonia)；[0.6.1 Release](https://github.com/pdeljanov/Symphonia/releases/tag/v0.6.1)
- [CPAL 官方仓库、后端与系统依赖](https://github.com/RustAudio/cpal)；[0.18.2 Release](https://github.com/RustAudio/cpal/releases/tag/v0.18.2)
- [FunDSP 官方仓库](https://github.com/SamiPerttu/fundsp)；[FunDSP API 文档](https://docs.rs/fundsp/latest/fundsp/)
- [NIH-plug 官方仓库与 maintenance mode 声明](https://github.com/robbert-vdh/nih-plug)；[社区 fork](https://codeberg.org/BillyDM/nih-plug)
- [midir 官方仓库](https://github.com/Boddlnagg/midir)；[midir API 文档](https://docs.rs/midir/latest/midir/)
- [rubato 官方仓库](https://github.com/HEnquist/rubato)；[rubato API 与实时注意事项](https://docs.rs/rubato/latest/rubato/)；[5.0.0 Release](https://github.com/HEnquist/rubato/releases/tag/v5.0.0)
- [hound 官方仓库](https://github.com/ruuda/hound)

插件与宿主：

- [CLAP 官方规范仓库](https://github.com/free-audio/clap)
- [Clack 官方仓库与 `clack-host`](https://github.com/prokopyl/clack)
- [Steinberg VST3 Developer Portal](https://steinbergmedia.github.io/vst3_dev_portal/)
- [Steinberg VST3 SDK 文件许可证说明](https://steinbergmedia.github.io/vst3_dev_portal/pages/VST%2B3%2BLicensing/Which%2Bfiles%2Bfall%2Bunder%2Bwhich%2Blicense.html)

TypeScript 与现有媒体路径：

- [FFmpeg 官方文档](https://www.ffmpeg.org/documentation.html)
- [FFmpeg Resampler](https://www.ffmpeg.org/ffmpeg-resampler.html)
- [W3C Web Audio API Recommendation](https://www.w3.org/TR/webaudio-1.0/)
- [Node.js `child_process`](https://nodejs.org/api/child_process.html)
- [Node.js `worker_threads`](https://nodejs.org/api/worker_threads.html)
- [Node.js Node-API](https://nodejs.org/api/n-api.html)
