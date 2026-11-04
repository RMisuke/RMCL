/**
 * First-run wizard: data mode → portable .minecraft → HMCL/PCL import →
 * account login → welcome.
 *
 * The backend decides whether this is a fresh install (`onboarding_probe`);
 * switching to portable mode restarts the process, so the pending choice is
 * kept in localStorage and the wizard resumes on the `.minecraft` step.
 */
import type { JSX } from "solid-js";
import { For, Show, createMemo, createSignal, onMount } from "solid-js";
import { RmButton, RmInput } from "./rm/controls";
import { gameDirectories, onboarding as onboardingApi, dataPaths, external } from "../ipc/api";
import type { LauncherConfigView, OnboardingProbeView } from "../ipc/types";
import { authStore } from "../stores/auth";
import { instancesStore } from "../stores/instances";
import { clearOnboardingForce, hasOnboardingForce } from "../lib/onboarding";
import { t } from "../i18n";

type Step = "mode" | "minecraft" | "import" | "login" | "done";
type Mode = "standard" | "portable";

const PENDING_KEY = "rmccc.onboarding.pendingMode";

function samePath(a: string, b: string): boolean {
  const norm = (value: string) => {
    const trimmed = value.replace(/\\/g, "/").replace(/\/+$/, "");
    return /Windows/i.test(navigator.userAgent) ? trimmed.toLowerCase() : trimmed;
  };
  return norm(a) === norm(b);
}

function folderName(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}

/** One card can share a config file with another; the directory is the key. */
function configKey(config: LauncherConfigView): string {
  return config.directory || `${config.config_path}#${config.name}`;
}

export function Onboarding(): JSX.Element {
  const [probe, setProbe] = createSignal<OnboardingProbeView | null>(null);
  const [visible, setVisible] = createSignal(false);
  const [step, setStep] = createSignal<Step>("mode");
  const [mode, setMode] = createSignal<Mode>("standard");
  const [switching, setSwitching] = createSignal(false);
  const [finishing, setFinishing] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [offlineName, setOfflineName] = createSignal("");
  const [imported, setImported] = createSignal<Set<string>>(new Set());
  const [importing, setImporting] = createSignal<string | null>(null);

  onMount(async () => {
    // Kept until the wizard finishes: a portable-mode switch restarts the
    // process, and the debug switch must survive that restart.
    const forced = hasOnboardingForce();
    const res = await onboardingApi.probe();
    if (!res.ok) return;
    if (!res.data.needs_onboarding && !forced) {
      localStorage.removeItem(PENDING_KEY);
      return;
    }
    const data = res.data;
    setProbe(data);
    const current: Mode = data.mode === "portable" ? "portable" : "standard";
    setMode(current);
    const pending = localStorage.getItem(PENDING_KEY) as Mode | null;
    if (pending === current) {
      setStep(current === "portable" ? "minecraft" : "login");
    }
    setVisible(true);
  });

  const currentMode = (): Mode => (probe()?.mode === "portable" ? "portable" : "standard");
  const steps = createMemo<Step[]>(() => {
    const list: Step[] = ["mode"];
    if (currentMode() === "portable") list.push("minecraft");
    if (currentMode() === "portable" && (probe()?.configs.length ?? 0) > 0) list.push("import");
    list.push("login");
    return list;
  });
  const stepIndex = () => Math.max(0, steps().indexOf(step()));
  const stepTitle = (item: Step) =>
    t(
      item === "mode"
        ? "onboarding.step.mode"
        : item === "minecraft"
          ? "onboarding.step.minecraft"
          : item === "import"
            ? "onboarding.step.import"
            : "onboarding.step.login"
    );

  async function continueFromMode(): Promise<void> {
    setError(null);
    if (mode() === currentMode()) {
      setStep(mode() === "portable" ? "minecraft" : "login");
      return;
    }
    setSwitching(true);
    localStorage.setItem(PENDING_KEY, mode());
    const res = await dataPaths.switchMode(mode(), false);
    if (!res.ok) {
      localStorage.removeItem(PENDING_KEY);
      setSwitching(false);
      setError(res.error.message);
      return;
    }
    if (!res.data.restart_required) {
      setSwitching(false);
      setStep("minecraft");
      return;
    }
    // The backend restarts in ~700 ms. Recover if it never happens.
    window.setTimeout(() => {
      if (!switching()) return;
      setSwitching(false);
      setError(t("onboarding.switchTimeout"));
    }, 8000);
    // Success restarts the process; the wizard resumes from the pending mode.
  }

  function continueFromMinecraft(): void {
    setError(null);
    setStep((probe()?.configs.length ?? 0) > 0 ? "import" : "login");
  }

  async function importConfig(config: LauncherConfigView): Promise<void> {
    if (!config.directory || importing()) return;
    setImporting(configKey(config));
    setError(null);
    const list = await gameDirectories.list();
    const existing = list.ok
      ? list.data.find((dir) => samePath(dir.path, config.directory))
      : undefined;
    if (existing) {
      const selected = await gameDirectories.select(existing.id);
      if (!selected.ok) setError(selected.error.message);
      else markImported(config);
      setImporting(null);
      return;
    }
    const added = await gameDirectories.add(
      config.directory,
      `${config.launcher.toUpperCase()} · ${folderName(config.directory)}`,
      true
    );
    if (!added.ok) {
      setError(added.error.message);
    } else {
      const selected = await gameDirectories.select(added.data.id);
      if (!selected.ok) setError(selected.error.message);
      else markImported(config);
    }
    setImporting(null);
  }

  function markImported(config: LauncherConfigView): void {
    setImported((prev) => new Set(prev).add(configKey(config)));
  }

  async function finish(): Promise<void> {
    if (finishing()) return;
    setFinishing(true);
    setError(null);
    const res = await onboardingApi.complete(currentMode());
    setFinishing(false);
    if (!res.ok) {
      setError(res.error.message);
      return;
    }
    localStorage.removeItem(PENDING_KEY);
    clearOnboardingForce();
    void authStore.refresh();
    void instancesStore.refresh();
    setStep("done");
  }

  async function startMicrosoft(): Promise<void> {
    if (authStore.loggingIn()) return;
    setError(null);
    const res = await authStore.startMicrosoftLogin();
    if (res.ok) await finish();
    else setError(res.error.message);
  }

  async function startOffline(): Promise<void> {
    if (authStore.loggingIn()) return;
    const name = offlineName().trim();
    if (!name) {
      setError(t("accounts.offline.invalid"));
      return;
    }
    setError(null);
    const res = await authStore.startOfflineLogin(name);
    if (res.ok) await finish();
    else setError(res.error.message);
  }

  function back(): void {
    if (step() === "minecraft") setStep("mode");
    else if (step() === "import") setStep("minecraft");
    else if (step() === "login") {
      if (currentMode() === "portable" && (probe()?.configs.length ?? 0) > 0) setStep("import");
      else if (currentMode() === "portable") setStep("minecraft");
      else setStep("mode");
    }
  }

  const canGoBack = () => step() !== "mode" && step() !== "done" && !switching() && !finishing();

  return (
    <Show when={visible() && probe()}>
      {(data) => (
        <div class="onboarding-overlay" role="dialog" aria-modal="true">
          <div class="onboarding-card">
            <header class="onboarding-head" data-tauri-drag-region="deep">
              <div class="onboarding-brand">
                <span class="onboarding-logo">S</span>
                <div>
                  <h2>{t("onboarding.title")}</h2>
                  <p class="muted-line">{t("onboarding.subtitle")}</p>
                </div>
              </div>
              <Show when={step() !== "done"}>
                <ol class="onboarding-progress" aria-label={t("onboarding.progress")}>
                  <For each={steps()}>
                    {(item, index) => (
                      <li
                        classList={{
                          "is-active": item === step(),
                          "is-done": index() < stepIndex(),
                        }}
                        title={stepTitle(item)}
                      />
                    )}
                  </For>
                </ol>
              </Show>
            </header>

            <Show when={step() === "mode"}>
              <section class="onboarding-step">
                <h3>{t("onboarding.mode.title")}</h3>
                <p class="muted-line">{t("onboarding.mode.desc")}</p>
                <div class="onboarding-options">
                  <button
                    type="button"
                    class="onboarding-option"
                    classList={{ "is-active": mode() === "standard" }}
                    onClick={() => setMode("standard")}
                  >
                    <strong>{t("onboarding.mode.standard")}</strong>
                    <span>{t("onboarding.mode.standardDesc")}</span>
                    <code>{data().standard_root}</code>
                  </button>
                  <button
                    type="button"
                    class="onboarding-option"
                    classList={{ "is-active": mode() === "portable" }}
                    onClick={() => setMode("portable")}
                  >
                    <strong>{t("onboarding.mode.portable")}</strong>
                    <span>{t("onboarding.mode.portableDesc")}</span>
                    <code>{data().portable_root}</code>
                  </button>
                </div>
              </section>
            </Show>

            <Show when={step() === "minecraft"}>
              <section class="onboarding-step">
                <h3>{t("onboarding.minecraft.title")}</h3>
                <div class="onboarding-status" classList={{ "is-ok": data().minecraft_existed }}>
                  <span class="onboarding-status-icon">{data().minecraft_existed ? "✓" : "＋"}</span>
                  <div>
                    <strong>
                      {data().minecraft_existed
                        ? t("onboarding.minecraft.exists")
                        : t("onboarding.minecraft.missing")}
                    </strong>
                    <p class="muted-line">
                      {data().minecraft_existed
                        ? t("onboarding.minecraft.existsDesc")
                        : t("onboarding.minecraft.missingDesc")}
                    </p>
                  </div>
                </div>
                <code class="onboarding-path">{data().portable_minecraft}</code>
                <Show when={data().minecraft_existed && data().minecraft_instances > 0}>
                  <p class="muted-line">
                    {t("onboarding.minecraft.instances", { n: data().minecraft_instances })}
                  </p>
                </Show>
              </section>
            </Show>

            <Show when={step() === "import"}>
              <section class="onboarding-step">
                <h3>{t("onboarding.import.title")}</h3>
                <p class="muted-line">{t("onboarding.import.desc")}</p>
                <div class="onboarding-import-list">
                  <For each={data().configs}>
                    {(config) => (
                      <article class="onboarding-import-card">
                        <div class="onboarding-import-head">
                          <span class="onboarding-launcher">{config.launcher.toUpperCase()}</span>
                          <Show when={config.selected}>
                            <span class="onboarding-selected">{t("onboarding.import.selected")}</span>
                          </Show>
                          <Show when={config.instance_count > 0}>
                            <span class="muted-line">
                              {t("onboarding.import.instances", { n: config.instance_count })}
                            </span>
                          </Show>
                        </div>
                        <strong class="onboarding-import-name">
                          {config.name ||
                            (config.directory ? folderName(config.directory) : config.config_path)}
                        </strong>
                        <code class="onboarding-path">{config.directory || config.config_path}</code>
                        <Show when={config.directory}>
                          <code class="onboarding-path is-muted">{config.config_path}</code>
                        </Show>
                        <RmButton
                          size="sm"
                          variant={imported().has(configKey(config)) ? "ghost" : "primary"}
                          disabled={!config.directory || imported().has(configKey(config))}
                          loading={importing() === configKey(config)}
                          onClick={() => void importConfig(config)}
                        >
                          {imported().has(configKey(config))
                            ? t("onboarding.import.done")
                            : t("onboarding.import.action")}
                        </RmButton>
                      </article>
                    )}
                  </For>
                </div>
              </section>
            </Show>

            <Show when={step() === "login"}>
              <section class="onboarding-step">
                <h3>{t("onboarding.login.title")}</h3>
                <p class="muted-line">{t("onboarding.login.desc")}</p>
                <Show when={authStore.loggingIn()}>
                  <div class="onboarding-device">
                    <Show
                      when={authStore.deviceCode()}
                      fallback={<p class="muted-line">{t("onboarding.login.waiting")}</p>}
                    >
                      {(code) => (
                        <>
                          <p class="muted-line">{t("onboarding.login.code")}</p>
                          <div class="onboarding-code">{code().userCode}</div>
                          <RmButton
                            size="sm"
                            icon="external"
                            onClick={() => void external.openUrl(code().verificationUri)}
                          >
                            {t("onboarding.login.open")}
                          </RmButton>
                          <p class="muted-line">{t("onboarding.login.waiting")}</p>
                        </>
                      )}
                    </Show>
                  </div>
                </Show>
                <Show when={!authStore.loggingIn()}>
                  <div class="onboarding-login">
                    <RmButton size="lg" icon="shield" onClick={() => void startMicrosoft()}>
                      {t("onboarding.login.microsoft")}
                    </RmButton>
                    <div class="onboarding-offline">
                      <RmInput
                        value={offlineName()}
                        onInput={setOfflineName}
                        placeholder={t("accounts.offline.name")}
                        onEnter={() => void startOffline()}
                      />
                      <RmButton icon="person" onClick={() => void startOffline()}>
                        {t("onboarding.login.offline")}
                      </RmButton>
                    </div>
                  </div>
                </Show>
              </section>
            </Show>

            <Show when={step() === "done"}>
              <section class="onboarding-step onboarding-done">
                <div class="onboarding-done-mark">✓</div>
                <h3>{t("onboarding.done.title")}</h3>
                <p class="muted-line">{t("onboarding.done.desc")}</p>
              </section>
            </Show>

            <Show when={error()}>
              <p class="onboarding-error">{error()}</p>
            </Show>

            <footer class="onboarding-foot">
              <Show when={canGoBack()}>
                <RmButton variant="ghost" onClick={back}>
                  {t("onboarding.back")}
                </RmButton>
              </Show>
              <div class="onboarding-foot-right">
                <Show when={step() === "mode"}>
                  <RmButton
                    variant="primary"
                    loading={switching()}
                    disabled={switching()}
                    onClick={() => void continueFromMode()}
                  >
                    {switching() ? t("onboarding.switching") : t("onboarding.next")}
                  </RmButton>
                </Show>
                <Show when={step() === "minecraft"}>
                  <RmButton variant="primary" onClick={continueFromMinecraft}>
                    {t("onboarding.next")}
                  </RmButton>
                </Show>
                <Show when={step() === "import"}>
                  <RmButton variant="ghost" onClick={() => setStep("login")}>
                    {t("onboarding.import.skip")}
                  </RmButton>
                  <RmButton variant="primary" disabled={imported().size === 0} onClick={() => setStep("login")}>
                    {t("onboarding.next")}
                  </RmButton>
                </Show>
                <Show when={step() === "login"}>
                  <RmButton
                    variant="ghost"
                    disabled={authStore.loggingIn() || finishing()}
                    onClick={() => void finish()}
                  >
                    {t("onboarding.login.skip")}
                  </RmButton>
                </Show>
                <Show when={step() === "done"}>
                  <RmButton variant="primary" onClick={() => setVisible(false)}>
                    {t("onboarding.done.start")}
                  </RmButton>
                </Show>
              </div>
            </footer>
          </div>
        </div>
      )}
    </Show>
  );
}
