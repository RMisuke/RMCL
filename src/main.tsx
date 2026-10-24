/**
 * RMCCC Tauri Entry Point
 *
 * Routes:
 *   /         → Dashboard
 *   /login    → Login
 *   /settings → Settings (incl. the AI service config, ADR-013)
 *   /instances/:id → InstanceSettings (per-instance settings + dependency scan)
 *   /plugins       → Plugins (declarative plugin manager)
 *
 * `/agent` was removed (ADR-013: no in-launcher agent UI) and redirects to
 * /settings — the AI service config lives there now.
 *
 * `root={App}` wraps every route in the application shell (title bar, rounded
 * window background, toast layer, window reveal), so the chrome stays mounted
 * across navigations.
 */
import { render } from "solid-js/web";
import { Router, Route, Navigate } from "@solidjs/router";
import App from "./App";
import Login from "./routes/Login";
import Dashboard from "./routes/Dashboard";
import Settings from "./routes/Settings";
import InstanceSettings from "./routes/InstanceSettings";
import ModBrowser from "./routes/ModBrowser";
import Plugins from "./routes/Plugins";
import "./styles/app.css";

render(
  () => (
    <Router root={App}>
      <Route path="/" component={Dashboard} />
      <Route path="/login" component={Login} />
      <Route path="/settings" component={Settings} />
      <Route path="/instances/:id" component={InstanceSettings} />
      <Route path="/mods" component={ModBrowser} />
      <Route path="/plugins" component={Plugins} />
      <Route path="/agent" component={() => <Navigate href="/settings" />} />
    </Router>
  ),
  document.getElementById("root")!
);

// Tell the inline boot guard in `index.html` that the shell rendered, so it
// drops its failure panel and stops reporting a boot timeout. The guard is
// registered before this module loads (it's an inline classic script), so the
// event cannot race the listener.
window.dispatchEvent(new Event("rmccc:shell-mounted"));
