/**
 * Boot guard — runs before the app bundle, from `public/` so the main window
 * can ship a strict CSP without `script-src 'unsafe-inline'`.
 *
 * The window is created borderless and transparent, and `main.rs` shows it as
 * soon as `setup` finished. If the bundle never executes (stale embedded asset
 * map, syntax error, crash during the first render) the result is a transparent
 * window with no error anywhere: release builds ship without the Web Inspector,
 * and WKWebView's console is invisible from outside the process.
 *
 * This guard paints a readable failure panel and forwards the message to the
 * backend (`frontend_log`) so it lands in `logs/rmccc.<date>.log`.
 */
(function () {
  var MAX = 4000;
  var mounted = false;
  var panel = null;

  // The UI relies on `color-mix()` (Chromium 111+) and `:has()` (105+).
  // WebView2 Evergreen normally auto-updates; locked-down/offline installs can
  // lag behind, in which case the whole palette would silently drop out.
  var MIN_CHROMIUM = 111;
  var LEGACY_WEBVIEW = false;

  function body() {
    return document.body || document.documentElement;
  }

  function report(title, detail) {
    var text = title + " :: " + detail + " :: " + location.href;
    window.__rmcccBootErrors = window.__rmcccBootErrors || [];
    window.__rmcccBootErrors.push(text);
    try {
      var internals = window.__TAURI_INTERNALS__;
      if (internals && internals.invoke) {
        internals.invoke("frontend_log", {
          level: "error",
          message: String(text).slice(0, MAX),
        });
      }
    } catch (_) {
      /* logging must never mask the original failure */
    }
  }

  function paint(title, detail) {
    if (!body()) return;
    // First failure wins: a second report (e.g. the 4s boot timeout after
    // a failed script load) must not replace the URL that actually broke.
    if (panel) return;
    panel = document.createElement("div");
    panel.setAttribute("id", "rmccc-boot-error");
    panel.style.cssText =
      "position:fixed;inset:0;z-index:2147483647;box-sizing:border-box;" +
      "display:flex;flex-direction:column;justify-content:center;gap:14px;" +
      "padding:32px;background:#141a2e;color:#e6e9f2;text-align:left;" +
      "font:13px/1.6 -apple-system,BlinkMacSystemFont,'SF Pro Text',sans-serif;";

    var heading = document.createElement("div");
    heading.style.cssText = "font-size:15px;font-weight:600;max-width:720px";
    heading.textContent = title;

    var pre = document.createElement("pre");
    pre.setAttribute("data-detail", "");
    pre.style.cssText =
      "max-width:720px;margin:0;white-space:pre-wrap;word-break:break-word;" +
      "color:#8b93a8;font:11px/1.7 ui-monospace,Menlo,monospace;";
    pre.textContent = detail;

    var hint = document.createElement("div");
    hint.style.cssText =
      "max-width:720px;color:#6b7288;font-size:11px;line-height:1.7";
    hint.textContent = LEGACY_WEBVIEW
      ? "Install the latest Microsoft Edge WebView2 Runtime, then restart SNGL: " +
        "https://go.microsoft.com/fwlink/p/?LinkId=2124703"
      : "Details were written to logs/rmccc.<date>.log. " +
        "If the UI files are missing from the build, run `pnpm tauri build --bundles app`.";

    panel.appendChild(heading);
    panel.appendChild(pre);
    panel.appendChild(hint);
    body().appendChild(panel);
    document.title = "SNGL — UI failed to load";
  }

  function fatal(title, detail) {
    report(title, detail);
    // After the shell mounted, Solid's error boundaries keep the window
    // usable: report and leave the UI alone instead of covering it.
    if (!mounted) paint(title, detail);
  }

  window.addEventListener(
    "error",
    function (event) {
      var target = event.target;
      if (target && target !== window && target.tagName === "SCRIPT") {
        return fatal(
          "Frontend asset failed to load / 前端资源加载失败",
          target.src || "(inline script)"
        );
      }
      if (target && target !== window && target.tagName === "LINK") {
        return fatal(
          "Frontend stylesheet failed to load / 前端样式加载失败",
          target.href || "(unknown)"
        );
      }
      if (target && target !== window) return;
      fatal(
        "Frontend script error / 前端脚本错误",
        (event.message || "unknown error") +
          "\n" +
          (event.filename || "") +
          ":" +
          (event.lineno || 0) +
          ":" +
          (event.colno || 0) +
          (event.error && event.error.stack ? "\n" + event.error.stack : "")
      );
    },
    true
  );

  window.addEventListener("unhandledrejection", function (event) {
    var reason = event.reason;
    fatal(
      "Unhandled promise rejection / 未处理的 Promise 拒绝",
      reason && (reason.stack || reason.message)
        ? reason.stack || reason.message
        : String(reason)
    );
  });

  // Reported by `src/main.tsx` right after `render()`.
  window.__rmcccMounted = false;
  window.addEventListener("rmccc:shell-mounted", function () {
    mounted = true;
    window.__rmcccMounted = true;
    if (!LEGACY_WEBVIEW && panel && panel.parentNode) panel.parentNode.removeChild(panel);
  });

  // Windows-only: WebView2 reports the Chromium version in its UA. An older
  // runtime cannot render the palette (`color-mix`) or the layout (`:has`), so
  // show an upgrade panel instead of a half-transparent, colourless window.
  var chrome = /Chrome\/(\d+)/.exec(navigator.userAgent);
  if (/Windows/i.test(navigator.userAgent) && chrome && Number(chrome[1]) < MIN_CHROMIUM) {
    LEGACY_WEBVIEW = true;
    fatal(
      "WebView2 runtime is too old / WebView2 版本过低",
      "SNGL requires Microsoft Edge WebView2 " +
        MIN_CHROMIUM +
        "+ (detected Chromium " +
        chrome[1] +
        ").\nUpdate the WebView2 Runtime and restart the launcher."
    );
  }

  setTimeout(function () {
    if (!window.__rmcccMounted) {
      fatal(
        "The UI did not mount / 界面未能加载",
        "The bundle did not run within 4s.\n" +
          "url=" +
          location.href +
          "\nreadyState=" +
          document.readyState +
          "\nThis usually means the binary was built before `dist/` was up to date."
      );
    }
  }, 4000);
})();
