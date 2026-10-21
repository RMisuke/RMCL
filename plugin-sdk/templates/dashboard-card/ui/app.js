/**
 * Template: a dashboard card rendered in the isolated UI container.
 *
 * `vendor/rmccc-plugin-ui.js` is the SDK runtime, vendored into the package so
 * the card works without a build step. Swap it for
 * `import { connect } from "@rmccc/plugin-ui"` when you have a bundler.
 */
import { connect, applyTheme, announce } from "./vendor/rmccc-plugin-ui.js";

const host = connect();
const title = document.getElementById("title");
const summary = document.getElementById("summary");
const error = document.getElementById("error");

function showError(err) {
  error.hidden = false;
  error.textContent = `${err.code ?? "error"}: ${err.message ?? err}`;
}

host.onTheme((theme) => applyTheme(theme));

host.onRevoke((capability) => {
  showError({ code: "permission_denied", message: `revoked: ${capability}` });
});

host.onInit(async (context) => {
  applyTheme(context.theme);
  title.textContent = `Hello from ${context.pluginId}`;
  try {
    const [{ instances }, settings] = await Promise.all([host.instances(), host.settings()]);
    summary.textContent = `${instances.length} instance(s) · UI API v${context.v} · locale ${settings.locale ?? "system"}`;
  } catch (e) {
    summary.textContent = "Could not read instances.";
    showError(e);
  }
});

announce();
