import { Component, createSignal, Show, onMount, onCleanup } from "solid-js";
import { useNavigate } from "@solidjs/router";
import { listen } from "@tauri-apps/api/event";
import { auth, safeInvoke, type AuthSession } from "../ipc/api";
import { EVENTS } from "../ipc/events";
import { t } from "../i18n";

interface DeviceCodePayload {
  user_code: string;
  verification_uri: string;
}

const Login: Component = () => {
  const navigate = useNavigate();
  const [offlineUsername, setOfflineUsername] = createSignal("");
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [deviceCode, setDeviceCode] = createSignal<DeviceCodePayload | null>(null);

  // 监听后端发射的 AUTH_DEVICE_CODE 事件，展示验证码与验证 URI（P0-01）
  onMount(async () => {
    const unlisten = await listen<DeviceCodePayload>(EVENTS.AUTH_DEVICE_CODE, (event) => {
      setDeviceCode(event.payload);
    });
    onCleanup(unlisten);
  });

  const handleOfflineLogin = async () => {
    const username = offlineUsername().trim();
    if (!username) {
      setError(t("login.error.usernameRequired"));
      return;
    }

    setLoading(true);
    setError(null);

    const result = await safeInvoke<AuthSession>("auth_offline_login", { username });
    setLoading(false);

    if (result.ok) {
      navigate("/");
    } else {
      setError(result.error.message);
    }
  };

  const handleMicrosoftLogin = async () => {
    setLoading(true);
    setError(null);
    setDeviceCode(null);

    // invoke 在设备码轮询完成前不会 resolve；验证码通过 auth://device-code 事件实时到达
    const result = await safeInvoke<AuthSession>("auth_microsoft_login");
    setLoading(false);

    if (result.ok) {
      navigate("/");
    } else {
      setError(result.error.message);
    }
  };

  const openVerificationUri = async () => {
    const dc = deviceCode();
    if (!dc) return;
    // 经 Tauri command 走 tauri-plugin-opener（带 http/https 校验），
    // 失败时在 UI 展示错误而非静默。
    const result = await safeInvoke<void>("open_external_url", {
      url: dc.verification_uri,
    });
    if (!result.ok) {
      setError(result.error.message);
    }
  };

  return (
    <div class="login-container">
      <div class="login-card">
        <div class="login-header">
          <h1 class="login-title">RMCCC</h1>
          <p class="login-subtitle">{t("login.subtitle")}</p>
        </div>

        <div class="login-section">
          <button
            class="btn-microsoft"
            onClick={handleMicrosoftLogin}
            disabled={loading()}
          >
            <Show when={!loading()} fallback={t("login.microsoft.busy")}>
              {t("login.microsoft.submit")}
            </Show>
          </button>
        </div>

        {/* 设备码展示区（P0-01）：后端经 AUTH_DEVICE_CODE 事件推送 */}
        <Show when={deviceCode()}>
          {(dc) => (
            <div class="device-code-card">
              <p class="device-code-title">{t("login.deviceCode.title")}</p>
              <button
                class="device-code-uri"
                onClick={openVerificationUri}
                title={dc().verification_uri}
              >
                {dc().verification_uri}
              </button>
              <p class="device-code-label">{t("login.deviceCode.codeLabel")}</p>
              <p class="device-code-value">{dc().user_code}</p>
            </div>
          )}
        </Show>

        <div class="login-divider">
          <span>{t("login.divider")}</span>
        </div>

        <div class="login-section">
          <input
            type="text"
            class="input-username"
            placeholder={t("login.offline.placeholder")}
            value={offlineUsername()}
            onInput={(e) => setOfflineUsername(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") handleOfflineLogin();
            }}
            disabled={loading()}
          />
          <button
            class="btn-offline"
            onClick={handleOfflineLogin}
            disabled={loading() || !offlineUsername().trim()}
          >
            <Show when={!loading()} fallback={t("login.offline.busy")}>
              {t("login.offline.submit")}
            </Show>
          </button>
        </div>

        <Show when={error()}>
          <div class="login-error">{error()}</div>
        </Show>
      </div>
    </div>
  );
};

export default Login;
