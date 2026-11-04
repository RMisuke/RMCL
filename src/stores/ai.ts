/**
 * AI service store (ADR-013 S2): config view, model probe, streaming log
 * analysis. Events `ai://delta/done/error` are keyed by analysisId.
 */
import { createRoot, createSignal } from "solid-js";
import { ai } from "../ipc/api";
import type { AiConfigView } from "../ipc/types";
import { EVENTS } from "../ipc/events";
import { listen } from "@tauri-apps/api/event";
import { locale } from "../i18n";

/** 输出详细程度（与设置页一致）。 */
export type AnalysisDetail = "brief" | "balanced" | "detailed";
/** 日志提交模式：标准=完整日志；激进=仅 ERROR/WARNING。 */
export type LogMode = "standard" | "aggressive";

export interface AnalysisState {
  analysisId: string;
  profileId: string | null;
  /** 绑定的运行 id：UI 只展示属于当前 run 的分析。 */
  runId: string | null;
  text: string;
  state: "running" | "done" | "error" | "cancelled";
  error?: string;
  sentLines: number;
  omittedLines: number;
  /** 日志来源：captured / disk-fresh / disk-unbound。 */
  source: string;
  startedAt: number;
}

export const aiStore = createRoot(() => {
  const [config, setConfig] = createSignal<AiConfigView | null>(null);
  const [models, setModels] = createSignal<string[]>([]);
  const [probing, setProbing] = createSignal(false);
  const [probeError, setProbeError] = createSignal<string | null>(null);
  const [analysis, setAnalysis] = createSignal<AnalysisState | null>(null);
  /** One-time data-egress confirmation (ui-design 07 §2.1, localStorage-backed). */
  const [egressApproved, setEgressApproved] = createSignal(
    localStorage.getItem("rmccc.ui.ai.egressApproved") === "1"
  );

  // ── 分析行为偏好（设置 · AI 选项卡；localStorage 持久化）──
  const DETAIL_KEY = "rmccc.ai.detail";
  const LOG_MODE_KEY = "rmccc.ai.logMode";
  const AUTO_CRASH_KEY = "rmccc.ai.autoAnalyzeOnCrash";

  function readDetail(): AnalysisDetail {
    const raw = localStorage.getItem(DETAIL_KEY);
    return raw === "brief" || raw === "detailed" ? raw : "balanced";
  }
  function readLogMode(): LogMode {
    return localStorage.getItem(LOG_MODE_KEY) === "aggressive" ? "aggressive" : "standard";
  }
  function readAutoCrash(): boolean {
    return localStorage.getItem(AUTO_CRASH_KEY) === "1";
  }

  const [analysisDetail, setAnalysisDetail_] = createSignal<AnalysisDetail>(readDetail());
  const [logMode, setLogMode_] = createSignal<LogMode>(readLogMode());
  const [autoAnalyzeOnCrash, setAutoAnalyzeOnCrash_] = createSignal(readAutoCrash());

  function setAnalysisDetail(detail: AnalysisDetail): void {
    setAnalysisDetail_(detail);
    try {
      localStorage.setItem(DETAIL_KEY, detail);
    } catch {
      /* 持久化失败仅影响下次默认值 */
    }
  }
  function setLogMode(mode: LogMode): void {
    setLogMode_(mode);
    try {
      localStorage.setItem(LOG_MODE_KEY, mode);
    } catch {
      /* 同上 */
    }
  }
  function setAutoAnalyzeOnCrash(enabled: boolean): void {
    setAutoAnalyzeOnCrash_(enabled);
    try {
      localStorage.setItem(AUTO_CRASH_KEY, enabled ? "1" : "0");
    } catch {
      /* 同上 */
    }
  }

  async function refreshConfig(): Promise<void> {
    const res = await ai.getConfig();
    if (res.ok) setConfig(res.data);
  }

  async function probeModels(baseUrl?: string, apiKey?: string): Promise<string[] | null> {
    setProbing(true);
    setProbeError(null);
    const res = await ai.probeModels(baseUrl, apiKey);
    setProbing(false);
    if (res.ok) {
      setModels(res.data);
      return res.data;
    }
    setProbeError(res.error.message);
    return null;
  }

  async function saveConfig(baseUrl?: string, model?: string, apiKey?: string): Promise<{ ok: boolean; message?: string }> {
    const res = await ai.setConfig(baseUrl, model, apiKey);
    if (res.ok) {
      await refreshConfig();
      return { ok: true };
    }
    return { ok: false, message: res.error.message };
  }

  function approveEgress(): void {
    localStorage.setItem("rmccc.ui.ai.egressApproved", "1");
    setEgressApproved(true);
  }

  /** Start streaming analysis of a profile's (or shared) current-run log. */
  async function analyzeLog(
    profileId: string | null,
    runId: string | null,
    exit?: { exitCode: number; exitType: string }
  ): Promise<{ ok: boolean; message?: string }> {
    const res = await ai.analyzeLog({
      key: profileId ?? undefined,
      runId: runId ?? undefined,
      detail: analysisDetail(),
      logMode: logMode(),
      language: locale(),
      exitCode: exit?.exitCode,
      exitType: exit?.exitType,
    });
    if (!res.ok) return { ok: false, message: res.error.message };
    setAnalysis({
      analysisId: res.data.analysisId,
      profileId,
      runId: res.data.runId ?? runId,
      text: "",
      state: "running",
      sentLines: res.data.sentLines,
      omittedLines: res.data.omittedLines,
      source: res.data.source,
      startedAt: Date.now(),
    });
    return { ok: true };
  }

  async function cancelAnalysis(): Promise<void> {
    const current = analysis();
    if (current && current.state === "running") {
      // 乐观置态：立即脱离"分析中"；后端的 ai://done{cancelled} 幂等确认。
      setAnalysis({ ...current, state: "cancelled" });
      await ai.cancelAnalysis(current.analysisId);
    }
  }

  /** 新运行开始 / 切换实例时清掉旧结果。 */
  function resetAnalysis(): void {
    setAnalysis(null);
  }

  function attachEvents(): void {
    // Coalesce streaming deltas (perf audit 2.3): growing `text` once per
    // 150ms instead of per token keeps the downstream markdown re-render
    // O(n) per frame instead of O(n²) over the whole stream. AI_DONE /
    // AI_ERROR flush the buffer first so the stream tail is never lost.
    let deltaId: string | null = null;
    let deltaBuf = "";
    let deltaTimer: ReturnType<typeof setTimeout> | null = null;
    const flushDeltas = (): void => {
      if (deltaTimer) {
        clearTimeout(deltaTimer);
        deltaTimer = null;
      }
      if (!deltaBuf) return;
      const id = deltaId;
      const text = deltaBuf;
      deltaBuf = "";
      setAnalysis((current) =>
        current && current.analysisId === id ? { ...current, text: current.text + text } : current
      );
    };
    void listen<{ analysisId: string; text: string }>(EVENTS.AI_DELTA, (event) => {
      const { analysisId, text } = event.payload;
      if (deltaId !== analysisId) {
        flushDeltas();
        deltaId = analysisId;
      }
      deltaBuf += text;
      if (!deltaTimer) deltaTimer = setTimeout(flushDeltas, 150);
    });
    void listen<{ analysisId: string; reason: string }>(EVENTS.AI_DONE, (event) => {
      flushDeltas();
      setAnalysis((current) =>
        current && current.analysisId === event.payload.analysisId
          ? {
              ...current,
              state: event.payload.reason === "cancelled" ? "cancelled" : "done",
            }
          : current
      );
    });
    void listen<{ analysisId: string; message: string }>(EVENTS.AI_ERROR, (event) => {
      flushDeltas();
      setAnalysis((current) =>
        current && current.analysisId === event.payload.analysisId
          ? { ...current, state: "error", error: event.payload.message }
          : current
      );
    });
  }

  return {
    config,
    models,
    probing,
    probeError,
    analysis,
    egressApproved,
    analysisDetail,
    logMode,
    autoAnalyzeOnCrash,
    approveEgress,
    setAnalysisDetail,
    setLogMode,
    setAutoAnalyzeOnCrash,
    refreshConfig,
    probeModels,
    saveConfig,
    analyzeLog,
    cancelAnalysis,
    resetAnalysis,
    attachEvents,
  };
});
