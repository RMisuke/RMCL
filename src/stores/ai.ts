/**
 * AI service store (ADR-013 S2): config view, model probe, streaming log
 * analysis. Events `ai://delta/done/error` are keyed by analysisId.
 */
import { createRoot, createSignal } from "solid-js";
import { ai } from "../ipc/api";
import type { AiConfigView } from "../ipc/types";
import { EVENTS } from "../ipc/events";
import { listen } from "@tauri-apps/api/event";

export interface AnalysisState {
  analysisId: string;
  profileId: string | null;
  text: string;
  state: "running" | "done" | "error";
  error?: string;
  sentLines: number;
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

  /** Start streaming analysis of a profile's (or shared) latest game log. */
  async function analyzeLog(profileId: string | null): Promise<{ ok: boolean; message?: string }> {
    const res = await ai.analyzeLog(profileId ?? undefined);
    if (!res.ok) return { ok: false, message: res.error.message };
    setAnalysis({
      analysisId: res.data.analysisId,
      profileId,
      text: "",
      state: "running",
      sentLines: res.data.sentLines,
      startedAt: Date.now(),
    });
    return { ok: true };
  }

  async function cancelAnalysis(): Promise<void> {
    const current = analysis();
    if (current && current.state === "running") {
      await ai.cancelAnalysis(current.analysisId);
    }
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
    void listen<{ analysisId: string }>(EVENTS.AI_DONE, (event) => {
      flushDeltas();
      setAnalysis((current) =>
        current && current.analysisId === event.payload.analysisId
          ? { ...current, state: "done" }
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
    approveEgress,
    refreshConfig,
    probeModels,
    saveConfig,
    analyzeLog,
    cancelAnalysis,
    attachEvents,
  };
});
