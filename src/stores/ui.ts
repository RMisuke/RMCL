/**
 * UI store: toasts, confirm host, overlay open state, rail collapse.
 *
 * The single place for cross-page overlay state (ui-design 10): pages never
 * own dialog open flags for global surfaces (task center, log drawer).
 */
import { createRoot, createSignal } from "solid-js";
import type { ConfirmRequest } from "../components/rm/overlays";
import { readPref, writePref } from "../lib/prefs";

export interface ToastItem {
  id: number;
  message: string;
  tone: "success" | "error" | "info";
  /** Optional follow-up action label + handler ("查看"). */
  actionLabel?: string;
  onAction?: () => void;
}

export type ToastInput = string | { message: string; tone?: ToastItem["tone"]; actionLabel?: string; onAction?: () => void };

export interface PromptRequest {
  title: string;
  label: string;
  initial?: string;
  placeholder?: string;
  resolve: (value: string | null) => void;
}

export const uiStore = createRoot(() => {
  const [toasts, setToasts] = createSignal<ToastItem[]>([]);
  const [confirmRequest, setConfirmRequest] = createSignal<ConfirmRequest | null>(null);
  const [promptRequest, setPromptRequest] = createSignal<PromptRequest | null>(null);
  const [taskCenterOpen, setTaskCenterOpen] = createSignal(false);
  const [logDrawer, setLogDrawer] = createSignal<{ profileId: string | null } | null>(null);
  const [railCollapsed, setRailCollapsed] = createSignal(readPref("rail.collapsed", false));

  let nextId = 1;

  /** Show a toast; one action per message, auto-dismiss 4s. */
  function toast(input: ToastInput): void {
    const spec: ToastItem =
      typeof input === "string"
        ? { id: nextId++, message: input, tone: "info" }
        : {
            id: nextId++,
            message: input.message,
            tone: input.tone ?? "info",
            actionLabel: input.actionLabel,
            onAction: input.onAction,
          };
    setToasts((list) => [...list.slice(-4), spec]);
    setTimeout(() => {
      setToasts((list) => list.filter((item) => item.id !== spec.id));
    }, 5000);
  }

  function dismissToast(id: number): void {
    setToasts((list) => list.filter((item) => item.id !== id));
  }

  /** Dangerous-action confirm; resolves false on dismiss/ESC. */
  function confirm(request: Omit<ConfirmRequest, "resolve">): Promise<boolean> {
    return new Promise((resolve) => {
      setConfirmRequest({ ...request, resolve: (allowed) => { resolve(allowed); setConfirmRequest(null); } });
    });
  }

  /** Resolve the pending confirmation exactly once (dialog buttons + ESC + X). */
  function resolveConfirm(allowed: boolean): void {
    const pending = confirmRequest();
    if (!pending) return;
    setConfirmRequest(null);
    pending.resolve(allowed);
  }

  /**
   * Text-input dialog. WKWebView does not support window.prompt/confirm —
   * ALL text entry must go through this (renames, ids, offline names…).
   */
  function prompt(request: Omit<PromptRequest, "resolve">): Promise<string | null> {
    return new Promise((resolve) => {
      setPromptRequest({ ...request, resolve: (value) => { resolve(value); setPromptRequest(null); } });
    });
  }

  function resolvePrompt(value: string | null): void {
    const pending = promptRequest();
    if (!pending) return;
    setPromptRequest(null);
    pending.resolve(value);
  }

  function toggleRail(): void {
    setRailCollapsed((v) => {
      writePref("rail.collapsed", !v);
      return !v;
    });
  }

  function openLogDrawer(profileId: string | null = null): void {
    setLogDrawer({ profileId });
  }

  function closeLogDrawer(): void {
    setLogDrawer(null);
  }

  return {
    toasts,
    toast,
    dismissToast,
    confirmRequest,
    confirm,
    resolveConfirm,
    promptRequest,
    prompt,
    resolvePrompt,
    taskCenterOpen,
    setTaskCenterOpen,
    toggleTaskCenter: () => setTaskCenterOpen((v) => !v),
    logDrawer,
    openLogDrawer,
    closeLogDrawer,
    railCollapsed,
    toggleRail,
  };
});

/** Convenience alias. */
export const toast = (input: ToastInput): void => uiStore.toast(input);
export const confirmDanger = (request: Omit<ConfirmRequest, "resolve">): Promise<boolean> =>
  uiStore.confirm({ ...request, danger: true });
export const promptText = (request: Omit<PromptRequest, "resolve">): Promise<string | null> =>
  uiStore.prompt(request);
