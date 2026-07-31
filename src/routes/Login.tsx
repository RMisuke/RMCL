import { Component, createSignal, Show } from "solid-js";
import { useNavigate } from "@solidjs/router";
import { auth, safeInvoke, type AuthSession } from "../ipc/api";

const Login: Component = () => {
  const navigate = useNavigate();
  const [offlineUsername, setOfflineUsername] = createSignal("");
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  const handleOfflineLogin = async () => {
    const username = offlineUsername().trim();
    if (!username) {
      setError("Please enter a username");
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

    const result = await safeInvoke<AuthSession>("auth_microsoft_login");
    setLoading(false);

    if (result.ok) {
      navigate("/");
    } else {
      setError(result.error.message);
    }
  };

  return (
    <div class="login-container">
      <div class="login-card">
        <div class="login-header">
          <h1 class="login-title">RMCCC</h1>
          <p class="login-subtitle">Minecraft Launcher</p>
        </div>

        <div class="login-section">
          <button
            class="btn-microsoft"
            onClick={handleMicrosoftLogin}
            disabled={loading()}
          >
            <Show when={!loading()} fallback="Connecting...">
              Sign in with Microsoft
            </Show>
          </button>
        </div>

        <div class="login-divider">
          <span>or</span>
        </div>

        <div class="login-section">
          <input
            type="text"
            class="input-username"
            placeholder="Enter username"
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
            <Show when={!loading()} fallback="Logging in...">
              Offline Login
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
