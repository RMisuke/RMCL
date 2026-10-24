/**
 * Auth store (ui-design 00 §5.1): session, account list, device-code flow.
 *
 * Tokens live inside `AuthSession` but must NEVER be rendered — components
 * read only `username` / `uuid` / provider (06 §10 red line).
 */
import { createRoot, createSignal } from "solid-js";
import { auth } from "../ipc/api";
import type { AuthSession, AccountMetadata, IpcError } from "../ipc/types";
import { EVENTS } from "../ipc/events";
import { listen } from "@tauri-apps/api/event";

export interface DeviceCodeState {
  userCode: string;
  verificationUri: string;
}

export type AuthResult = { ok: true } | { ok: false; error: IpcError };

export const authStore = createRoot(() => {
  const [session, setSession] = createSignal<AuthSession | null>(null);
  const [accounts, setAccounts] = createSignal<AccountMetadata[]>([]);
  const [deviceCode, setDeviceCode] = createSignal<DeviceCodeState | null>(null);
  const [loggingIn, setLoggingIn] = createSignal(false);
  /** Profile to launch right after a successful login (06 §6). */
  let pendingLaunchProfileId: string | null = null;
  const [pendingLaunch, setPendingLaunch] = createSignal<string | null>(null);

  // ── Per-account last-played (localStorage; launches are per profile, so the
  // account card cannot read profile.last_played — it must record its own). ──
  const ACTIVITY_KEY = "rmccc.accountLastPlayed";
  const [activityMap, setActivityMap] = createSignal<Record<string, string>>(readActivity());

  function readActivity(): Record<string, string> {
    try {
      return JSON.parse(localStorage.getItem(ACTIVITY_KEY) ?? "{}") as Record<string, string>;
    } catch {
      return {};
    }
  }

  /** Record "this account just launched a game" (now). */
  function recordAccountActivity(): void {
    const accountId = session()?.account_id;
    if (!accountId) return;
    const next = { ...activityMap(), [accountId]: new Date().toISOString() };
    setActivityMap(next);
    try {
      localStorage.setItem(ACTIVITY_KEY, JSON.stringify(next));
    } catch {
      /* localStorage full/unavailable — the in-memory value still renders */
    }
  }

  /** ISO timestamp of the account's last launch, or null. */
  function lastPlayedFor(accountId: string): string | null {
    return activityMap()[accountId] ?? null;
  }

  let loaded = false;
  const loadedAt = () => loaded;

  async function refresh(): Promise<void> {
    const [sessionRes, accountsRes] = await Promise.all([auth.getSession(), auth.listAccounts()]);
    if (sessionRes.ok) setSession(sessionRes.data);
    if (accountsRes.ok) setAccounts(accountsRes.data);
    loaded = true;
  }

  async function startMicrosoftLogin(): Promise<AuthResult> {
    setLoggingIn(true);
    const res = await auth.microsoftLogin();
    setLoggingIn(false);
    if (res.ok) {
      setSession(res.data);
      await refresh();
      return { ok: true };
    }
    return { ok: false, error: res.error };
  }

  async function startOfflineLogin(username: string): Promise<AuthResult> {
    const res = await auth.offlineLogin(username);
    if (res.ok) {
      setSession(res.data);
      await refresh();
      return { ok: true };
    }
    return { ok: false, error: res.error };
  }

  async function logout(): Promise<void> {
    await auth.logout();
    setSession(null);
    await refresh();
  }

  async function switchAccount(accountId: string): Promise<AuthResult> {
    const res = await auth.switchAccount(accountId);
    if (res.ok) {
      setSession(res.data);
      await refresh();
      return { ok: true };
    }
    return { ok: false, error: res.error };
  }

  async function removeAccount(accountId: string): Promise<AuthResult> {
    const res = await auth.removeAccount(accountId);
    if (res.ok) {
      await refresh();
      return { ok: true };
    }
    return { ok: false, error: res.error };
  }

  async function refreshSession(): Promise<AuthResult> {
    const res = await auth.refreshToken();
    if (res.ok && res.data) {
      setSession(res.data);
      return { ok: true };
    }
    if (res.ok) return { ok: false, error: { code: "not_found", message: "no session", retryable: false } as unknown as IpcError };
    return { ok: false, error: res.error };
  }

  function setPendingLaunchProfile(id: string | null): void {
    pendingLaunchProfileId = id;
    setPendingLaunch(id);
  }

  function consumePendingLaunch(): string | null {
    const id = pendingLaunchProfileId;
    pendingLaunchProfileId = null;
    setPendingLaunch(null);
    return id;
  }

  /** Register event subscriptions once (stores are the only listen() point). */
  function attachEvents(): void {
    void listen<{ user_code: string; verification_uri: string }>(
      EVENTS.AUTH_DEVICE_CODE,
      (event) => {
        setDeviceCode({
          userCode: event.payload.user_code,
          verificationUri: event.payload.verification_uri,
        });
      }
    );
  }

  function clearDeviceCode(): void {
    setDeviceCode(null);
  }

  return {
    session,
    accounts,
    deviceCode,
    clearDeviceCode,
    loggingIn,
    loadedAt,
    refresh,
    startMicrosoftLogin,
    startOfflineLogin,
    logout,
    switchAccount,
    removeAccount,
    refreshSession,
    pendingLaunch,
    setPendingLaunchProfile,
    consumePendingLaunch,
    recordAccountActivity,
    lastPlayedFor,
    attachEvents,
  };
});
