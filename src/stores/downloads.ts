/**
 * Downloads store: active batches + recent results (ui-design 10 §1).
 * Driven purely by events; a batch is created lazily on first progress
 * (the backend owns batch_id) or on install start via `beginInstall`.
 */
import { createRoot, createSignal } from "solid-js";
import { createStore, produce } from "solid-js/store";
import type { DownloadProgressPayload, IpcError } from "../ipc/types";
import { EVENTS } from "../ipc/events";
import { listen } from "@tauri-apps/api/event";

export interface DownloadBatch {
  batchId: string;
  profileId: string | null;
  version: string | null;
  label: string;
  completedFiles: number;
  totalFiles: number;
  downloadedBytes: number;
  totalBytes: number;
  currentFile: string | null;
  state: "pending" | "running" | "error";
  /** EMA-smoothed bytes/second, computed from progress deltas. */
  speed: number;
  error?: string;
  startedAt: number;
}

export interface DownloadOutcome {
  id: number;
  label: string;
  ok: boolean;
  detail?: string;
  at: number;
}

export const downloadsStore = createRoot(() => {
  // Fine-grained store (perf audit 2.1): per-batch path updates mean a
  // progress tick only re-renders the row reading the changed fields, instead
  // of cloning the whole Map + recreating every `<For>` row at ~10 Hz.
  const [batches, setBatches] = createStore<Record<string, DownloadBatch>>({});
  const [recent, setRecent] = createSignal<DownloadOutcome[]>([]);
  const [paused, setPaused] = createSignal<Set<string>>(new Set());
  let lastProgressAt = new Map<string, { at: number; bytes: number }>();
  let outcomeId = 1;

  const BATCH_DEFAULTS: Omit<DownloadBatch, "batchId" | "startedAt"> = {
    profileId: null,
    version: null,
    label: "",
    completedFiles: 0,
    totalFiles: 0,
    downloadedBytes: 0,
    totalBytes: 0,
    currentFile: null,
    state: "running",
    speed: 0,
  };

  function upsertBatch(batchId: string, patch: Partial<DownloadBatch>): void {
    setBatches(batchId, (prev) => ({
      ...BATCH_DEFAULTS,
      ...prev,
      batchId,
      startedAt: prev?.startedAt ?? Date.now(),
      ...patch,
    }));
  }

  function beginInstall(opts: { batchId: string; profileId: string; version: string; label: string }): void {
    upsertBatch(opts.batchId, {
      profileId: opts.profileId,
      version: opts.version,
      label: opts.label,
      state: "pending",
    });
  }

  /**
   * Synthetic task for downloads without backend batch events (mod install,
   * dependency apply): indeterminate progress until it resolves.
   */
  function beginTask(label: string): string {
    const key = `task-${Date.now()}-${Math.floor(Math.random() * 1e6)}`;
    upsertBatch(key, { label, state: "running" });
    return key;
  }

  function finishTask(key: string, ok: boolean, detail?: string): void {
    finishBatch(key, ok, detail);
  }

  /**
   * The backend assigns real batch ids asynchronously; a wizard-created
   * "pending" entry is keyed by a synthetic id. When the first progress event
   * with the real batch id arrives, migrate the pending entry onto it.
   *
   * Progress events carry no profile id, so a blank `profileId` is only safe
   * to match when exactly one pending entry exists (P2-12: no mislabeling
   * when several installs run concurrently; the complete/error events carry
   * the profile and still close the entry).
   */
  function rekeyPending(profileId: string, realBatchId: string): void {
    if (!realBatchId || batches[realBatchId]) return;
    const pendings = Object.values(batches).filter((b) => b.state === "pending");
    let target: DownloadBatch | undefined;
    if (profileId !== "") {
      target = pendings.find((b) => b.profileId === profileId);
    } else if (pendings.length === 1) {
      target = pendings[0];
    }
    if (!target) return;
    const picked = target;
    setBatches(
      produce((b) => {
        delete b[picked.batchId];
        b[realBatchId] = { ...picked, batchId: realBatchId, state: "running" };
      })
    );
  }

  /** Push a recent outcome; kept outside signal updaters (P2-6). */
  function pushOutcome(label: string, ok: boolean, detail?: string): void {
    const id = outcomeId++;
    setRecent((list) => [{ id, label, ok, detail, at: Date.now() }, ...list].slice(0, 6));
  }

  function finishBatch(batchId: string, ok: boolean, detail?: string): void {
    const batch = batches[batchId];
    if (batch) pushOutcome(batch.label || batch.version || batchId, ok, detail);
    setBatches(produce((b) => { delete b[batchId]; }));
    lastProgressAt.delete(batchId);
    setPaused((set) => {
      const next = new Set(set);
      next.delete(batchId);
      return next;
    });
  }

  /**
   * Close a wizard-created pending entry when a batch never started (e.g. the
   * version was already installed): complete/error arrive with an empty
   * batch_id, matched here by profile.
   */
  function finishPendingFor(profileId: string, ok: boolean, detail?: string): void {
    const pending = Object.values(batches).find(
      (b) => b.state === "pending" && b.profileId === profileId
    );
    if (!pending) return;
    pushOutcome(pending.label || pending.version || pending.batchId, ok, detail);
    setBatches(produce((b) => { delete b[pending.batchId]; }));
  }

  function attachEvents(): void {
    void listen<DownloadProgressPayload>(EVENTS.DOWNLOAD_PROGRESS, (event) => {
      const p = event.payload;
      rekeyPending("", p.batch_id); // pending entries without a profile hop onto the first batch
      const prev = lastProgressAt.get(p.batch_id);
      const now = Date.now();
      let speed = 0;
      if (prev && now > prev.at) {
        const instant = ((p.downloaded_bytes - prev.bytes) / (now - prev.at)) * 1000;
        const existing = batches[p.batch_id]?.speed ?? 0;
        // EMA smoothing (α = 0.3).
        speed = existing === 0 ? instant : existing * 0.7 + instant * 0.3;
      }
      lastProgressAt.set(p.batch_id, { at: now, bytes: p.downloaded_bytes });
      upsertBatch(p.batch_id, {
        completedFiles: p.completed_files,
        totalFiles: p.total_files,
        downloadedBytes: p.downloaded_bytes,
        totalBytes: p.total_bytes,
        currentFile: p.current_file,
        speed: Math.max(0, speed),
        state: "running",
      });
    });
    void listen<{ batch_id: string; profile_id: string; version: string }>(
      EVENTS.DOWNLOAD_COMPLETE,
      (event) => {
        const p = event.payload;
        rekeyPending(p.profile_id, p.batch_id);
        if (p.batch_id) {
          finishBatch(p.batch_id, true);
        } else {
          // No download stage ran (already installed) — close the pending entry.
          finishPendingFor(p.profile_id, true);
        }
      }
    );
    void listen<{ batch_id: string; profile_id: string; version: string; error?: string; failed_files?: number }>(
      EVENTS.DOWNLOAD_ERROR,
      (event) => {
        const p = event.payload;
        rekeyPending(p.profile_id, p.batch_id);
        const detail =
          p.error ?? (p.failed_files !== undefined ? `${p.failed_files} files failed` : undefined);
        if (p.batch_id) {
          finishBatch(p.batch_id, false, detail);
        } else {
          finishPendingFor(p.profile_id, false, detail);
        }
      }
    );
  }

  return {
    batches,
    recent,
    paused,
    beginInstall,
    beginTask,
    finishTask,
    finishBatch,
    activeCount: () => Object.keys(batches).length,
    totalBytesProgress: () => {
      let downloaded = 0;
      let total = 0;
      for (const batch of Object.values(batches)) {
        if (batch.totalBytes > 0) {
          downloaded += batch.downloadedBytes;
          total += batch.totalBytes;
        }
      }
      return { downloaded, total };
    },
    /** Optimistic pause flag (backend events for paused/resumed are pending P3). */
    pause: (batchId: string) => {
      setPaused((set) => new Set(set).add(batchId));
    },
    resume: (batchId: string) => {
      setPaused((set) => {
        const next = new Set(set);
        next.delete(batchId);
        return next;
      });
    },
    attachEvents,
  };
});
