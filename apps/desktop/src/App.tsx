import { FormEvent, useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface CoreStatus {
  coreInstanceId: string;
  corePid: number;
  coreVersion: string;
  protocolVersion: string;
  schemaVersion: string;
}

interface CreativeBrief {
  summary: string;
  purpose: string | null;
  style: string[];
  mood: string[];
  instrumentation: string[];
  targetDurationSeconds: number | null;
  lyrics: string | null;
  constraints: string[];
}

interface AgentRun {
  id: string;
  status: "planning" | "awaiting_approval" | "ready_to_submit" | "submitting" | "submitted" | "unknown_outcome" | "completed" | "failed" | "cancelled";
  plan?: {
    visibleSummary: string;
    inputHash: string;
    estimatedCost: {
      availability: "known" | "unknown";
      currency?: string;
      upper_minor_units?: number;
    };
    usage: {
      inputTokens: number | null;
      outputTokens: number | null;
      actualCostMinorUnits: number | null;
      currency: string | null;
    };
    inference: {
      providerKind: string;
      model: string;
      protocol: string;
      responseId: string | null;
    };
  };
  failure?: {
    kind: "harness_unavailable" | "provider_rejected" | "provider_unavailable" | "invalid_provider_response" | "provider_confirmed_not_found";
    message: string;
  };
}

interface Candidate {
  id: string;
  label: string;
  note: string | null;
  asset: {
    id: string;
    relativePath: string;
    sha256: string;
    audio: {
      sampleRateHz: number;
      channels: number;
      durationMicros: number;
      bitDepth: number;
    };
  };
}

interface Project {
  id: string;
  name: string;
  revision: number;
  brief: CreativeBrief | null;
  agentRuns: AgentRun[];
  candidates: Candidate[];
  selection: { id: string; candidateId: string; projectRevision: number } | null;
  timeline: { clips: Array<{ id: string; durationMicros: number }> };
  exports: Array<{ id: string; relativePath: string; manifestSha256: string; files: Array<{ relativePath: string }> }>;
}

interface ProjectBackup {
  id: string;
  sourceProjectId: string;
  sourceProjectRevision: number;
  backupName: string;
}

interface CommandError {
  code: string;
  message: string;
}

type Connection = "checking" | "connected" | "disconnected";

function normalizeError(value: unknown): CommandError {
  if (
    typeof value === "object" &&
    value !== null &&
    "code" in value &&
    "message" in value &&
    typeof value.code === "string" &&
    typeof value.message === "string"
  ) {
    return { code: value.code, message: value.message };
  }
  return { code: "unexpected_error", message: String(value) };
}

function list(value: string): string[] {
  return value.split(",").map((item) => item.trim()).filter(Boolean);
}

function seconds(micros: number): string {
  return `${(micros / 1_000_000).toFixed(1)}s`;
}

export default function App() {
  const [connection, setConnection] = useState<Connection>("checking");
  const [core, setCore] = useState<CoreStatus | null>(null);
  const [project, setProject] = useState<Project | null>(null);
  const [projectName, setProjectName] = useState("My First Project");
  const [summary, setSummary] = useState("Nocturnal synthwave cue for a film opening");
  const [style, setStyle] = useState("synthwave, cinematic");
  const [mood, setMood] = useState("tense, propulsive");
  const [instrumentation, setInstrumentation] = useState("analog synth, drum machine");
  const [duration, setDuration] = useState(30);
  const [approvalBudgetMinor, setApprovalBudgetMinor] = useState(100);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<CommandError | null>(null);
  const [preview, setPreview] = useState<{ assetId: string; url: string } | null>(null);
  const [backup, setBackup] = useState<ProjectBackup | null>(null);

  const activeRun = useMemo(() => project?.agentRuns.at(-1) ?? null, [project]);
  const canPlanRun = Boolean(
    project?.brief &&
    (!activeRun || activeRun.status === "failed" || activeRun.status === "cancelled"),
  );

  const reconnect = useCallback(async () => {
    setConnection("checking");
    setError(null);
    try {
      const status = await invoke<CoreStatus>("core_status");
      setCore(status);
      setConnection("connected");
      try {
        setProject(await invoke<Project>("open_project"));
      } catch (openError) {
        const normalized = normalizeError(openError);
        if (normalized.code === "project_not_found") setProject(null);
        else throw normalized;
      }
      return true;
    } catch (connectError) {
      setCore(null);
      setProject(null);
      setConnection("disconnected");
      setError(normalizeError(connectError));
      return false;
    }
  }, []);

  useEffect(() => {
    let disposed = false;
    let timer: number | undefined;
    const connectManagedCore = async () => {
      for (let attempt = 0; attempt < 12 && !disposed; attempt += 1) {
        if (await reconnect()) return;
        await new Promise<void>((resolve) => { timer = window.setTimeout(resolve, 500); });
      }
    };
    void connectManagedCore();
    return () => { disposed = true; if (timer !== undefined) window.clearTimeout(timer); };
  }, [reconnect]);
  useEffect(() => () => { if (preview) URL.revokeObjectURL(preview.url); }, [preview]);

  async function command<T>(
    label: string,
    operation: () => Promise<T>,
    reloadProjectOnFailure = false,
  ): Promise<T | null> {
    setBusy(label);
    setError(null);
    try {
      return await operation();
    } catch (commandError) {
      const normalized = normalizeError(commandError);
      setError(normalized);
      if (reloadProjectOnFailure || normalized.code === "project_revision_conflict") {
        try {
          setProject(await invoke<Project>("open_project"));
        } catch {
          // Preserve the command error. The explicit resync action handles a Core failure.
        }
      }
      return null;
    } finally {
      setBusy(null);
    }
  }

  async function createProject(event: FormEvent) {
    event.preventDefault();
    const created = await command("创建工程", () =>
      invoke<Project>("create_project", { name: projectName }),
    );
    if (created) setProject(created);
  }

  async function saveBrief(event: FormEvent) {
    event.preventDefault();
    if (!project) return;
    const brief: CreativeBrief = {
      summary,
      purpose: "music production",
      style: list(style),
      mood: list(mood),
      instrumentation: list(instrumentation),
      targetDurationSeconds: duration,
      lyrics: null,
      constraints: ["editable DAW handoff"],
    };
    const updated = await command("保存 Brief", () =>
      invoke<Project>("set_brief", { expectedRevision: project.revision, brief }),
    );
    if (updated) setProject(updated);
  }

  async function planRun() {
    if (!project) return;
    const updated = await command("Agent 规划", () =>
      invoke<Project>("plan_agent_run", { expectedRevision: project.revision }),
      true,
    );
    if (updated) setProject(updated);
  }

  async function approveRun() {
    if (!project || !activeRun?.plan) return;
    const plan = activeRun.plan;
    const estimate = plan.estimatedCost;
    const updated = await command("确认费用", () =>
      invoke<Project>("approve_agent_run", {
        runId: activeRun.id,
        expectedRevision: project.revision,
        currency: estimate.currency ?? "USD",
        maxMinorUnits: estimate.upper_minor_units ?? approvalBudgetMinor,
        inputHash: plan.inputHash,
      }),
    );
    if (updated) setProject(updated);
  }

  async function resumePlanningRun() {
    if (!project || !activeRun) return;
    const updated = await command("恢复规划", () =>
      invoke<Project>("resume_planning_run", {
        runId: activeRun.id,
        expectedRevision: project.revision,
      }),
      true,
    );
    if (updated) setProject(updated);
  }

  async function selectCandidate(candidateId: string) {
    if (!project) return;
    const updated = await command("采用 Candidate", () =>
      invoke<Project>("select_candidate", {
        candidateId,
        expectedRevision: project.revision,
        startMicros: 0,
      }),
    );
    if (updated) setProject(updated);
  }

  async function exportHandoff() {
    if (!project) return;
    const updated = await command("创建 DAW Handoff", () =>
      invoke<Project>("export_handoff", { expectedRevision: project.revision }),
    );
    if (updated) setProject(updated);
  }

  async function backupProject() {
    if (!project) return;
    const created = await command("备份工程", () =>
      invoke<ProjectBackup>("backup_project", { expectedRevision: project.revision }),
    );
    if (created) setBackup(created);
  }

  async function previewCandidate(assetId: string) {
    if (preview?.assetId === assetId) return;
    const payload = await command("载入试听", () =>
      invoke<ArrayBuffer>("preview_asset", { assetVersionId: assetId }),
    );
    if (!payload) return;
    const url = URL.createObjectURL(new Blob([payload], { type: "audio/wav" }));
    setPreview({ assetId, url });
  }

  return (
    <main className="shell">
      <header className="topbar">
        <a className="brand" href="#workspace" aria-label="Auto Studio 首页">
          <span className="brand-mark" aria-hidden="true">A</span>
          <span>Auto Studio</span>
        </a>
        <div className={`status status-${connection}`}>
          <span className="status-dot" aria-hidden="true" />
          {connection === "checking" && "正在连接 Core"}
          {connection === "connected" && "本地 Core 已连接"}
          {connection === "disconnected" && "Core 未连接"}
        </div>
      </header>

      <section className="workspace" id="workspace">
        <div className="eyebrow">SHIP 0 · CREATIVE AGENT LOOP</div>
        <div className="development-banner" role="status">
          真实 LLM 规划与长 Run 控制合同已启用 · Music Project 与本地 Tool Runtime 正在建设
        </div>
        <h1>{project ? project.name : "从创作意图，进入可继续制作的工程。"}</h1>
        <p className="lead">
          Brief、Agent Run、Candidate、Selection 与 Timeline 都由独立 Core 持久化；聊天不是工程事实。
        </p>

        {!project ? (
          <section className="panel project-panel start-panel">
            <span className="panel-kicker">NEW PROJECT</span>
            <h2>创建本地工程</h2>
            <form onSubmit={createProject}>
              <label htmlFor="project-name">工程名称</label>
              <div className="input-row">
                <input id="project-name" value={projectName} onChange={(event) => setProjectName(event.target.value)} maxLength={128} />
                <button className="primary" disabled={busy !== null || !projectName.trim()}>创建工程</button>
              </div>
            </form>
          </section>
        ) : (
          <>
            <div className="stage-rail" aria-label="Ship 0 创作阶段">
              <span className={project.brief ? "done" : "active"}>01 Brief</span>
              <span className={activeRun ? "done" : project.brief ? "active" : ""}>02 Agent Plan</span>
              <span className={project.candidates.length ? "done" : activeRun ? "active" : ""}>03 Candidates</span>
              <span className={project.selection ? "done" : project.candidates.length ? "active" : ""}>04 Selection</span>
              <span className={project.exports.length ? "done" : project.selection ? "active" : ""}>05 Handoff</span>
            </div>

            <div className="creative-grid">
              <section className="panel project-panel">
                <div className="panel-heading">
                  <div><span className="panel-kicker">CREATIVE BRIEF</span><h2>创作目标</h2></div>
                  <span className="revision">REV {project.revision}</span>
                </div>
                <form className="brief-form" onSubmit={saveBrief}>
                  <label htmlFor="summary">核心描述</label>
                  <textarea id="summary" value={project.brief?.summary ?? summary} onChange={(event) => setSummary(event.target.value)} disabled={Boolean(project.brief)} />
                  {!project.brief && (
                    <>
                      <div className="field-grid">
                        <label>风格<input value={style} onChange={(event) => setStyle(event.target.value)} /></label>
                        <label>情绪<input value={mood} onChange={(event) => setMood(event.target.value)} /></label>
                        <label>乐器<input value={instrumentation} onChange={(event) => setInstrumentation(event.target.value)} /></label>
                        <label>时长（秒）<input type="number" min={1} max={900} value={duration} onChange={(event) => setDuration(Number(event.target.value))} /></label>
                      </div>
                      <button className="primary action" disabled={busy !== null || !summary.trim()}>保存 Creative Brief</button>
                    </>
                  )}
                </form>

                {canPlanRun && (
                  <button className="primary action" onClick={() => void planRun()} disabled={busy !== null}>
                    {activeRun?.status === "failed" || activeRun?.status === "cancelled"
                      ? "基于同一 Brief 重新规划"
                      : "让 Agent 制定生成计划"}
                  </button>
                )}

                {activeRun && (
                  <div className="agent-plan">
                    <div><span className="panel-kicker">AGENT RUN</span><h3>{activeRun.plan?.visibleSummary ?? (activeRun.status === "planning" ? "正在准备上下文与计划" : "本次规划未生成可执行计划")}</h3></div>
                    <span className={`run-state state-${activeRun.status}`}>{activeRun.status.replaceAll("_", " ")}</span>
                    {activeRun.plan && <p className="plan-evidence">
                      预计上限 {activeRun.plan.estimatedCost.availability === "known"
                        ? `${activeRun.plan.estimatedCost.currency} ${((activeRun.plan.estimatedCost.upper_minor_units ?? 0) / 100).toFixed(2)}`
                        : "未知"}
                      {activeRun.plan.usage.inputTokens !== null
                        ? ` · Agent ${activeRun.plan.usage.inputTokens + (activeRun.plan.usage.outputTokens ?? 0)} tokens`
                        : " · Agent 用量未知"}
                      {` · ${activeRun.plan.inference.providerKind}/${activeRun.plan.inference.model}`}
                    </p>}
                    {activeRun.status === "awaiting_approval" && activeRun.plan && (
                      <div>
                        {activeRun.plan.estimatedCost.availability === "unknown" && (
                          <label>
                            本次费用上限（USD cents）
                            <input
                              type="number"
                              min={0}
                              value={approvalBudgetMinor}
                              onChange={(event) => setApprovalBudgetMinor(Number(event.target.value))}
                            />
                          </label>
                        )}
                        <button className="primary action" onClick={() => void approveRun()} disabled={busy !== null}>确认计划与费用上限</button>
                      </div>
                    )}
                    {activeRun.status === "planning" && (
                      <button className="secondary action" onClick={() => void resumePlanningRun()} disabled={busy !== null}>从工程记录恢复规划</button>
                    )}
                    {activeRun.status === "ready_to_submit" && (
                      <p className="warning">计划已批准；本地 Music Project Tool Runtime 尚未接入，当前不会执行工程写入。</p>
                    )}
                    {activeRun.status === "submitted" && (
                      <p className="warning">这是旧 Generation 流程留下的只读 Run 状态；Desktop 不再提供继续执行入口。</p>
                    )}
                    {activeRun.status === "unknown_outcome" && (
                      <p className="warning">这是旧 Generation 流程留下的只读 Run 状态；迁移工具落地前不会重提或继续执行。</p>
                    )}
                    {activeRun.status === "failed" && (
                      <p className="warning">
                        本次 Run 已终止{activeRun.failure ? `：${activeRun.failure.message}` : "。可以重新规划，不会复用失败 Attempt。"}
                      </p>
                    )}
                  </div>
                )}
              </section>

              <aside className="panel core-panel">
                <span className="panel-kicker">PROJECT FACTS</span>
                <h2>独立 Core</h2>
                <dl>
                  <div><dt>Core</dt><dd>{core?.coreVersion ?? "—"}</dd></div>
                  <div><dt>协议</dt><dd>{core?.protocolVersion ?? "—"}</dd></div>
                  <div><dt>Schema</dt><dd>{core?.schemaVersion ?? "—"}</dd></div>
                  <div><dt>进程</dt><dd>{core ? `PID ${core.corePid}` : "—"}</dd></div>
                  <div><dt>Revision</dt><dd>{project.revision}</dd></div>
                  <div><dt>Run</dt><dd>{activeRun?.status ?? "未开始"}</dd></div>
                </dl>
                <p>会话凭据、Provider 与文件路径都不进入 WebView。</p>
              </aside>
            </div>

            {project.candidates.length > 0 && (
              <section className="candidate-section">
                <div className="section-heading"><span className="panel-kicker">CANDIDATE BOARD</span><h2>比较生成方向</h2></div>
                <div className="candidate-grid">
                  {project.candidates.map((candidate, index) => {
                    const selected = project.selection?.candidateId === candidate.id;
                    return (
                      <article className={`candidate-card ${selected ? "selected" : ""}`} key={candidate.id}>
                        <div className="candidate-wave" aria-hidden="true"><span /><span /><span /><span /><span /></div>
                        <div className="candidate-meta">
                          <span>DIRECTION {String.fromCharCode(65 + index)}</span>
                          <h3>{candidate.label}</h3>
                          <p>{candidate.asset.audio.sampleRateHz / 1000} kHz · {candidate.asset.audio.bitDepth}-bit · {seconds(candidate.asset.audio.durationMicros)}</p>
                        </div>
                        <div className="candidate-actions">
                          <button className="secondary" disabled={busy !== null} onClick={() => void previewCandidate(candidate.asset.id)}>
                            {preview?.assetId === candidate.asset.id ? "正在试听" : "试听"}
                          </button>
                          <button className={selected ? "selected-button" : "secondary"} disabled={selected || busy !== null} onClick={() => void selectCandidate(candidate.id)}>
                            {selected ? "已采用到 Timeline" : "采用此 Candidate"}
                          </button>
                        </div>
                        {preview?.assetId === candidate.asset.id && <audio className="candidate-audio" src={preview.url} controls autoPlay />}
                      </article>
                    );
                  })}
                </div>
              </section>
            )}

            {project.selection && (
              <section className="panel timeline-panel">
                <div className="panel-heading"><div><span className="panel-kicker">AUDIO CLIP TIMELINE</span><h2>已采用工程事实</h2></div><span className="revision">{project.timeline.clips.length} CLIP</span></div>
                <div className="timeline-track"><div className="timeline-clip">Selected Audio · {seconds(project.timeline.clips[0].durationMicros)}</div></div>
                {project.exports.length === 0 ? (
                  <button className="primary action" onClick={() => void exportHandoff()} disabled={busy !== null}>创建 DAW Handoff Package</button>
                ) : (
                  <div className="handoff-ready">
                    <span>HANDOFF READY</span>
                    <strong>{project.exports.at(-1)?.relativePath}</strong>
                    <small>包含选中 WAV、manifest、hash、rights/credits 与导入说明</small>
                  </div>
                )}
                <button className="secondary action" onClick={() => void backupProject()} disabled={busy !== null}>创建一致工程备份</button>
                {backup && <p className="backup-ready">已备份 Revision {backup.sourceProjectRevision} · {backup.backupName}</p>}
              </section>
            )}
          </>
        )}

        {busy && <div className="busy-toast">{busy}中…</div>}
        {error && (
          <div className="error global-error" role="alert">
            <div><strong>{error.code}</strong><p>{error.message}</p></div>
            <button type="button" onClick={() => void reconnect()}>重新同步</button>
          </div>
        )}
      </section>

      <footer><span>LOCAL-FIRST</span><span>SELECTION ≠ APPROVAL</span><span>PROTOCOL v0.1</span></footer>
    </main>
  );
}
