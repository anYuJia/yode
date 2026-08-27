import React, { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Bot, BrainCircuit, CheckCircle2, CircleAlert, Cpu, GitBranch, RefreshCw, ShieldCheck } from "lucide-react";

type RouteDecision = {
  provider: string;
  model: string;
  score: number;
  reasons: string[];
  capabilities?: ModelCapabilities;
};

type ModelCapabilities = {
  context_window: number;
  max_output_tokens: number;
  supports_tools: boolean;
  supports_vision: boolean;
  supports_reasoning: boolean;
  supports_parallel_tools: boolean;
  supports_prompt_cache: boolean;
  cost_class: string;
  quality_score: number;
  speed_score: number;
};

type IntelligenceSnapshot = {
  current: { provider: string; model: string; capabilities: ModelCapabilities };
  routes: Array<{ role: string; decision: RouteDecision | null }>;
  sandbox: { backend?: string; sandboxed?: boolean; mode?: string; network?: string; degraded_reason?: string; error?: string };
  executionBackends: Array<{ kind: string; available: boolean; detail: string }>;
  learning: { postmortems: number; lessons: number; recurring_failure_patterns: number };
  runtime: Record<string, boolean>;
  workspace: string;
};

function statusText(value: boolean, t: (zh: string, en: string) => string) {
  return value ? t("可用", "Ready") : t("不可用", "Unavailable");
}

function formatRole(role: string) {
  return role.charAt(0).toUpperCase() + role.slice(1);
}

export function AgentIntelligenceSettings({
  t
}: {
  isZh?: boolean;
  t: (zh: string, en: string) => string;
}) {
  const [snapshot, setSnapshot] = useState<IntelligenceSnapshot | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");

  const load = useCallback(async () => {
    if (!("__TAURI_INTERNALS__" in window)) {
      setLoading(false);
      setError(t("Agent 智能状态仅在桌面应用中可用。", "Agent intelligence is only available in the desktop app."));
      return;
    }
    setLoading(true);
    setError("");
    try {
      setSnapshot(await invoke<IntelligenceSnapshot>("agent_intelligence_snapshot"));
    } catch (reason) {
      console.error(reason);
      setError(String(reason));
    } finally {
      setLoading(false);
    }
  }, [t]);

  useEffect(() => {
    void load();
  }, [load]);

  const readyBackends = useMemo(
    () => snapshot?.executionBackends.filter((backend) => backend.available).length ?? 0,
    [snapshot]
  );

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "14px", marginBottom: "18px" }}>
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: "12px" }}>
        <div>
          <div style={{ display: "flex", alignItems: "center", gap: "7px", fontSize: "14px", fontWeight: 650, color: "var(--text)" }}>
            <BrainCircuit size={16} />
            {t("Agent 智能控制面", "Agent intelligence")}
          </div>
          <div style={{ marginTop: "3px", fontSize: "11.5px", color: "var(--text-soft)", lineHeight: 1.5 }}>
            {t("模型路由、执行隔离、验证与学习的实时运行能力。", "Live model routing, execution isolation, verification, and learning capabilities.")}
          </div>
        </div>
        <button className="secondary-button" type="button" onClick={() => void load()} disabled={loading} style={{ display: "flex", alignItems: "center", gap: "6px", height: "28px" }}>
          <RefreshCw size={12} />
          {loading ? t("刷新中", "Refreshing") : t("刷新", "Refresh")}
        </button>
      </div>

      {error && (
        <div className="theme-card" style={{ padding: "10px 12px", fontSize: "11.5px", color: "var(--danger, #e86b6b)" }}>
          {error}
        </div>
      )}

      {snapshot && (
        <>
          <div className="theme-card" style={{ padding: "12px", display: "grid", gridTemplateColumns: "repeat(4, minmax(0, 1fr))", gap: "10px" }}>
            <Metric icon={<Bot size={14} />} label={t("当前模型", "Current model")} value={`${snapshot.current.provider} / ${snapshot.current.model}`} />
            <Metric icon={<ShieldCheck size={14} />} label={t("OS 隔离", "OS sandbox")} value={snapshot.sandbox.sandboxed ? String(snapshot.sandbox.backend ?? "sandbox") : t("降级", "Degraded")} />
            <Metric icon={<Cpu size={14} />} label={t("执行后端", "Execution backends")} value={`${readyBackends}/${snapshot.executionBackends.length}`} />
            <Metric icon={<BrainCircuit size={14} />} label={t("学习记忆", "Learned lessons")} value={`${snapshot.learning.lessons}`} />
          </div>

          <section>
            <SectionLabel>{t("模型能力与自动路由", "Model capabilities & routing")}</SectionLabel>
            <div className="theme-card" style={{ overflow: "hidden" }}>
              <div className="form-row">
                <div className="row-info">
                  <span className="row-label">{snapshot.current.model}</span>
                  <span className="row-desc">
                    {t("上下文", "Context")} {snapshot.current.capabilities.context_window.toLocaleString()} · {t("质量", "Quality")} {snapshot.current.capabilities.quality_score} · {t("速度", "Speed")} {snapshot.current.capabilities.speed_score}
                  </span>
                </div>
                <span style={{ fontSize: "11px", color: "var(--text-soft)" }}>
                  {snapshot.current.capabilities.supports_reasoning ? "Reasoning" : "Standard"}
                  {snapshot.current.capabilities.supports_vision ? " · Vision" : ""}
                  {snapshot.current.capabilities.supports_parallel_tools ? " · Parallel tools" : ""}
                </span>
              </div>
              {snapshot.routes.map((route) => (
                <React.Fragment key={route.role}>
                  <div className="divider" />
                  <div className="form-row">
                    <div className="row-info">
                      <span className="row-label">{formatRole(route.role)}</span>
                      <span className="row-desc">
                        {route.decision?.reasons?.slice(0, 2).join(" · ") || t("当前没有满足条件的候选模型", "No configured model satisfies this role")}
                      </span>
                    </div>
                    <span style={{ fontSize: "11.5px", fontFamily: "var(--font-code)", color: route.decision ? "var(--accent)" : "var(--text-soft)" }}>
                      {route.decision ? `${route.decision.provider}/${route.decision.model}` : "—"}
                    </span>
                  </div>
                </React.Fragment>
              ))}
            </div>
          </section>

          <section>
            <SectionLabel>{t("执行与隔离", "Execution & isolation")}</SectionLabel>
            <div className="theme-card" style={{ overflow: "hidden" }}>
              <div className="form-row">
                <div className="row-info">
                  <span className="row-label">{t("Shell Sandbox", "Shell sandbox")}</span>
                  <span className="row-desc">
                    {snapshot.sandbox.error || snapshot.sandbox.degraded_reason || `${snapshot.sandbox.mode ?? "auto"} · ${snapshot.sandbox.network ?? "inherit"}`}
                  </span>
                </div>
                <Status available={Boolean(snapshot.sandbox.sandboxed)} t={t} />
              </div>
              {snapshot.executionBackends.map((backend) => (
                <React.Fragment key={backend.kind}>
                  <div className="divider" />
                  <div className="form-row">
                    <div className="row-info">
                      <span className="row-label">{formatRole(backend.kind)}</span>
                      <span className="row-desc">{backend.detail}</span>
                    </div>
                    <Status available={backend.available} t={t} />
                  </div>
                </React.Fragment>
              ))}
            </div>
          </section>

          <section>
            <SectionLabel>{t("闭环能力", "Closed-loop capabilities")}</SectionLabel>
            <div className="theme-card" style={{ padding: "10px 12px", display: "grid", gridTemplateColumns: "repeat(2, minmax(0, 1fr))", gap: "8px 16px" }}>
              {Object.entries(snapshot.runtime).map(([name, ready]) => (
                <div key={name} style={{ display: "flex", alignItems: "center", gap: "7px", minWidth: 0 }}>
                  {ready ? <CheckCircle2 size={13} /> : <CircleAlert size={13} />}
                  <span style={{ fontSize: "11.5px", color: "var(--text-muted)", overflow: "hidden", textOverflow: "ellipsis" }}>{name}</span>
                </div>
              ))}
            </div>
          </section>

          <section>
            <SectionLabel>{t("Postmortem / Learning", "Postmortem / Learning")}</SectionLabel>
            <div className="theme-card">
              <div className="form-row">
                <div className="row-info">
                  <span className="row-label">{t("运行经验", "Run learning")}</span>
                  <span className="row-desc">{t("失败模式会聚合为可检索 lesson；敏感 token 会在持久化前脱敏。", "Failure patterns are promoted into retrievable lessons with secret redaction before persistence.")}</span>
                </div>
                <span style={{ fontSize: "11px", color: "var(--text-soft)", textAlign: "right" }}>
                  {snapshot.learning.postmortems} postmortems<br />{snapshot.learning.recurring_failure_patterns} recurring
                </span>
              </div>
            </div>
          </section>

          <div style={{ display: "flex", alignItems: "center", gap: "6px", fontSize: "10.5px", color: "var(--text-soft)", opacity: 0.75 }}>
            <GitBranch size={11} />
            {snapshot.workspace}
          </div>
        </>
      )}
    </div>
  );
}

function SectionLabel({ children }: { children: React.ReactNode }) {
  return <div style={{ marginBottom: "6px", fontSize: "10.5px", fontWeight: 700, color: "var(--text-soft)", letterSpacing: "0.35px", textTransform: "uppercase" }}>{children}</div>;
}

function Metric({ icon, label, value }: { icon: React.ReactNode; label: string; value: string }) {
  return (
    <div style={{ minWidth: 0 }}>
      <div style={{ display: "flex", alignItems: "center", gap: "5px", fontSize: "10.5px", color: "var(--text-soft)" }}>{icon}{label}</div>
      <div title={value} style={{ marginTop: "5px", fontSize: "11.5px", fontWeight: 600, color: "var(--text)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{value}</div>
    </div>
  );
}

function Status({ available, t }: { available: boolean; t: (zh: string, en: string) => string }) {
  return (
    <span style={{ display: "inline-flex", alignItems: "center", gap: "5px", fontSize: "11px", color: available ? "var(--accent)" : "var(--text-soft)" }}>
      {available ? <CheckCircle2 size={12} /> : <CircleAlert size={12} />}
      {statusText(available, t)}
    </span>
  );
}
