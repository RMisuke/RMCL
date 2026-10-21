/**
 * Isolated plugin UI for the server-dashboard example (Phase 4, ADR-006).
 *
 * Runs in an iframe served from `rmcccplugin://dev.rmccc.server-dashboard/`, a
 * different origin than the shell. No `fetch`, no Tauri API, no host DOM — the
 * CSP sends `connect-src 'none'` and every call below goes through postMessage:
 *
 *   frame --{v,type:"ready"}--> host --{v,type:"init"}--> frame
 *   frame --{v,id,method,params}--> host --...--> host --{v,id,ok|error}--> frame
 *
 * Written against the wire protocol directly (no build step) to show that the
 * SDK is a convenience, not a requirement.
 */
(function () {
  "use strict";

  var UI_API_VERSION = 1;
  var REQUEST_TIMEOUT_MS = 10000;
  var nextId = 0;
  var pending = new Map();
  var context = null;

  function post(message) {
    window.parent.postMessage(JSON.stringify(message), "*");
  }

  function request(method, params) {
    var id = "r" + ++nextId;
    return new Promise(function (resolve, reject) {
      var timer = window.setTimeout(function () {
        pending.delete(id);
        reject({ code: "timeout", message: method + " timed out", retryable: true });
      }, REQUEST_TIMEOUT_MS);
      pending.set(id, { resolve: resolve, reject: reject, timer: timer });
      post({ v: UI_API_VERSION, id: id, method: method, params: params || {} });
    });
  }

  function applyTheme(theme) {
    if (!theme) return;
    var root = document.documentElement.style;
    root.setProperty("--bg", theme.background);
    root.setProperty("--surface", theme.surface);
    root.setProperty("--text", theme.text);
    root.setProperty("--muted", theme.textMuted);
    root.setProperty("--accent", theme.accent);
    root.setProperty("--accent-contrast", theme.accentContrast);
    root.setProperty("--border", theme.border);
  }

  function text(id, value) {
    var node = document.getElementById(id);
    if (node) node.textContent = String(value);
  }

  function showError(error) {
    var node = document.getElementById("error");
    if (!node) return;
    if (!error) {
      node.hidden = true;
      node.textContent = "";
      return;
    }
    node.hidden = false;
    node.textContent = error.code + ": " + error.message;
  }

  function setBadge(state, label) {
    var badge = document.getElementById("badge");
    if (!badge) return;
    badge.textContent = label;
    badge.className = "badge badge-" + state;
  }

  function loadInstances() {
    return request("instances.list").then(function (result) {
      var instances = (result && result.instances) || [];
      text("instances", instances.length);
      if (instances.length === 0) {
        text("summary", "No instances yet.");
        setBadge("idle", "idle");
        return;
      }
      var first = instances[0];
      text(
        "summary",
        first.name + " · " + first.gameVersion + (first.loader ? " · " + first.loader.type : "")
      );
      setBadge("ok", instances.length + " instance(s)");
    });
  }

  function loadSettings() {
    return request("settings.get").then(function (settings) {
      text("locale", (settings && settings.locale) || "system");
    });
  }

  function checkServer() {
    // Denied until the user grants `network.request` — the error path is the
    // interesting one here: it must show the wire code, not a silent failure.
    return request("network.request", {
      url: "https://status.example.net/status.json",
    }).then(function (result) {
      text("summary", "HTTP " + result.status);
      setBadge("ok", "reachable");
    });
  }

  function refresh() {
    showError(null);
    setBadge("idle", "loading");
    loadInstances()
      .then(loadSettings)
      .then(function () {
        setBadge("ok", "ready");
      })
      .catch(function (error) {
        setBadge("error", "error");
        showError(error);
      });
  }

  window.addEventListener("message", function (event) {
    if (typeof event.data !== "string") return;
    var message;
    try {
      message = JSON.parse(event.data);
    } catch (e) {
      return;
    }
    if (message.v !== UI_API_VERSION) return;

    if (message.type === "init") {
      context = message;
      applyTheme(message.theme);
      text("title", "Server Status · " + (message.cardId || message.pluginId));
      refresh();
      return;
    }
    if (message.type === "theme") {
      applyTheme(message.theme);
      return;
    }
    if (message.type === "revoke") {
      setBadge("error", "revoked");
      showError({ code: "permission_denied", message: "capability revoked: " + message.capability });
      return;
    }

    var entry = pending.get(message.id);
    if (!entry) return;
    pending.delete(message.id);
    window.clearTimeout(entry.timer);
    if (message.ok) entry.resolve(message.result);
    else entry.reject(message.error);
  });

  document.getElementById("refresh").addEventListener("click", refresh);
  document.getElementById("refresh").addEventListener("dblclick", function () {
    checkServer().catch(function (error) {
      showError(error);
      setBadge("error", "error");
    });
  });

  post({ v: UI_API_VERSION, type: "ready" });
})();
