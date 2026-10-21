/**
 * RMCCC Tauri Entry Point
 *
 * Routes:
 *   /         → Dashboard
 *   /login    → Login
 *   /settings → Settings
 *   /instances/:id → InstanceSettings (per-instance settings)
 *   /plugins       → Plugins (declarative plugin manager)
 *
 * `root={App}` wraps every route in the application shell (title bar, rounded
 * window background, toast layer, window reveal), so the chrome stays mounted
 * across navigations.
 */
import { render } from "solid-js/web";
import { Router, Route } from "@solidjs/router";
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
    </Router>
  ),
  document.getElementById("root")!
);

// Tell the inline boot guard in `index.html` that the shell rendered, so it
// drops its failure panel and stops reporting a boot timeout. The guard is
// registered before this module loads (it is an inline classic script), so the
// event cannot race the listener.
window.dispatchEvent(new Event("rmccc:shell-mounted"));

