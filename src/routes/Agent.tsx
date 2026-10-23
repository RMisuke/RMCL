/**
 * Agent page (Phase 6.5).
 *
 * Thin client over the `agent_*` IPC surface: provider config, one turn at a
 * time (blocking call with a cancel button), pending approvals, and the
 * session history. All authority lives in the backend `AgentService` /
 * `ToolRegistry` — the page never sees model text as instructions, only as
 * transcript entries the backend already recorded.
 */
import { Component, For, Show, createSignal, onMount } from "solid-js";
import { agent as agentApi } from "../ipc/api";
import type {
  AgentConfigView,
  AgentPurgeReport,
  AgentRunOutput,
  AgentSessionEvent,
  AgentSessionSummary,
  AgentTaskSnapshot,
} from "../ipc/api";
import { showToast } from "../components/Toast";
import { t } from "../i18n";

const asText = (value: unknown): string => {
  if (typeof value === "string") return value;
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
};

const eventTitle = (event: AgentSessionEvent): string => {
  const kind = event.event.kind;
  switch (kind) {
    case "session_started":
      return t("agent.event.started");
    case "user_message":
      return t("agent.event.userMessage");
    case "model_response":
      return t("agent.event.modelResponse");
    case "tool_call":
      return `${t("agent.event.toolCall")}: ${String(event.event.tool_id ?? "")}`;
    case "tool_result":
      return `${t("agent.event.toolResult")}: ${String(event.event.tool_id ?? "")}`;
    case "approval_requested":
      return `${t("agent.event.approvalRequested")}: ${String(event.event.summary ?? "")}`;
    case "approval_resolved":
      return `${t("agent.event.approvalResolved")}: ${event.event.approved ? "✓" : "✗"}`;
    case "error":
      return `${t("agent.event.error")}: ${String(event.event.message ?? "")}`;
    case "session_ended":
      return t("agent.event.ended");
    default:
      return kind;
  }
};

const Agent: Component = () => {
  const [config, setConfig] = createSignal<AgentConfigView | null>(null);
  const [baseUrl, setBaseUrl] = createSignal("");
  const [model, setModel] = createSignal("");
  const [apiKey, setApiKey] = createSignal("");

  const [running, setRunning] = createSignal(false);
  const [message, setMessage] = createSignal("");
  const [lastRun, setLastRun] = createSignal<AgentRunOutput | null>(null);
  const [transcript, setTranscript] = createSignal<AgentSessionEvent[]>([]);

  const [sessions, setSessions] = createSignal<AgentSessionSummary[]>([]);
  const [tasks, setTasks] = createSignal<AgentTaskSnapshot[]>([]);

  const refreshLists = async () => {
    const sessionResult = await agentApi.listSessions();
    if (sessionResult.ok) setSessions(sessionResult.data);
    const taskResult = await agentApi.pendingTasks();
    if (taskResult.ok) setTasks(taskResult.data);
  };

  onMount(async () => {
    const result = await agentApi.getConfig();
    if (result.ok) {
      setConfig(result.data);
      if (result.data.baseUrl) setBaseUrl(result.data.baseUrl);
      if (result.data.model) setModel(result.data.model);
    }
    await refreshLists();
  });

  const saveConfig = async () => {
    if (!baseUrl().trim() || !model().trim() || !apiKey().trim()) {
      showToast({
        code: "validation_error",
        message: t("agent.config.missingFields"),
        retryable: false,
      });
      return;
    }
    const result = await agentApi.setConfig(baseUrl().trim(), model().trim(), apiKey().trim());
    if (result.ok) {
      setConfig({ configured: true, baseUrl: baseUrl().trim(), model: model().trim() });
      setApiKey("");
      showToast({ code: "success", message: t("agent.config.saved"), retryable: false });
    } else {
      showToast(result.error);
    }
  };

  const run = async () => {
    if (!message().trim() || running()) return;
    setRunning(true);
    setTranscript([]);
    const result = await agentApi.run(message().trim());
    setRunning(false);
    if (result.ok) {
      setLastRun(result.data);
      const events = await agentApi.readSession(result.data.session_id);
      if (events.ok) setTranscript(events.data);
      showToast({
        code: "success",
        message: `${t("agent.chat.done")} (${result.data.status})`,
        retryable: false,
      });
    } else {
      showToast(result.error);
    }
    await refreshLists();
  };

  const cancelRun = async () => {
    const result = await agentApi.cancelRun();
    if (!result.ok) showToast(result.error);
  };

  const resolve = async (taskId: string, approved: boolean) => {
    const result = await agentApi.resolveTask(taskId, approved);
    if (result.ok) {
      await refreshLists();
    } else {
      showToast(result.error);
    }
  };

  const clearHistory = async () => {
    const result = await agentApi.clearHistory();
    if (result.ok) {
      const report: AgentPurgeReport = result.data;
      showToast({
        code: "success",
        message: `${t("agent.history.cleared")}: ${report.sessions_removed}`,
        retryable: false,
      });
      setLastRun(null);
      setTranscript([]);
      await refreshLists();
    } else {
      showToast(result.error);
    }
  };

  return (
    <div class="settings-section agent-page">
      <h2>{t("agent.title")}</h2>

      <section class="agent-config">
        <h3>{t("agent.config.title")}</h3>
        <Show
          when={config()?.configured}
          fallback={<p class="muted">{t("agent.config.notConfigured")}</p>}
        >
          <p class="muted">
            {t("agent.config.current")}: {config()?.model} @ {config()?.baseUrl}
          </p>
        </Show>
        <input
          placeholder={t("agent.config.baseUrl")}
          value={baseUrl()}
          onInput={(e) => setBaseUrl(e.currentTarget.value)}
        />
        <input
          placeholder={t("agent.config.model")}
          value={model()}
          onInput={(e) => setModel(e.currentTarget.value)}
        />
        <input
          type="password"
          placeholder={t("agent.config.apiKey")}
          value={apiKey()}
          onInput={(e) => setApiKey(e.currentTarget.value)}
        />
        <button onClick={saveConfig}>{t("agent.config.save")}</button>
      </section>

      <section class="agent-chat">
        <h3>{t("agent.chat.title")}</h3>
        <textarea
          rows={3}
          placeholder={t("agent.chat.placeholder")}
          value={message()}
          onInput={(e) => setMessage(e.currentTarget.value)}
        />
        <div class="agent-actions">
          <button disabled={running() || !message().trim()} onClick={run}>
            {running() ? t("agent.chat.running") : t("agent.chat.run")}
          </button>
          <Show when={running()}>
            <button onClick={cancelRun}>{t("agent.chat.cancel")}</button>
          </Show>
        </div>
        <Show when={lastRun()}>
          <div class="agent-replies">
            <For each={lastRun()?.replies ?? []}>
              {(reply) => <p class="agent-reply">{reply}</p>}
            </For>
          </div>
        </Show>
        <Show when={lastRun()?.contextSent}>
          <details>
            <summary>{t("agent.chat.contextSent")}</summary>
            <pre class="agent-context">{asText(lastRun()?.contextSent)}</pre>
          </details>
        </Show>
        <Show when={transcript().length > 0}>
          <details>
            <summary>
              {t("agent.chat.transcript")} ({transcript().length})
            </summary>
            <ul class="agent-transcript">
              <For each={transcript()}>
                {(event) => (
                  <li>
                    <span class="muted">{event.ts}</span> — {eventTitle(event)}
                  </li>
                )}
              </For>
            </ul>
          </details>
        </Show>
      </section>

      <section class="agent-tasks">
        <h3>
          {t("agent.tasks.title")}
          {tasks().length > 0 ? ` (${tasks().length})` : ""}
        </h3>
        <Show
          when={tasks().length > 0}
          fallback={<p class="muted">{t("agent.tasks.none")}</p>}
        >
          <For each={tasks()}>
            {(task) => (
              <div class="agent-task">
                <code>{task.kind}</code>
                <pre>{asText(task.payload)}</pre>
                <div class="agent-actions">
                  <button onClick={() => resolve(task.task_id, true)}>
                    {t("agent.tasks.approve")}
                  </button>
                  <button onClick={() => resolve(task.task_id, false)}>
                    {t("agent.tasks.reject")}
                  </button>
                </div>
              </div>
            )}
          </For>
        </Show>
      </section>

      <section class="agent-history">
        <h3>{t("agent.history.title")}</h3>
        <Show
          when={sessions().length > 0}
          fallback={<p class="muted">{t("agent.history.none")}</p>}
        >
          <ul>
            <For each={sessions()}>
              {(session) => (
                <li>
                  <code>{session.session_id.slice(0, 8)}</code> — {session.event_count}{" "}
                  {t("agent.history.events")}
                  {session.ended ? "" : ` · ${t("agent.history.active")}`}
                </li>
              )}
            </For>
          </ul>
        </Show>
        <button onClick={clearHistory}>{t("agent.history.clear")}</button>
      </section>
    </div>
  );
};

export default Agent;
