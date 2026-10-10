/**
 * RMCCC Tauri Entry Point
 *
 * Routes:
 *   /         → Dashboard
 *   /login    → Login
 *   /settings → Settings
 */
import { render } from "solid-js/web";
import { Router, Route } from "@solidjs/router";
import Login from "./routes/Login";
import Dashboard from "./routes/Dashboard";
import Settings from "./routes/Settings";
import "./styles/app.css";

render(
  () => (
    <Router>
      <Route path="/" component={Dashboard} />
      <Route path="/login" component={Login} />
      <Route path="/settings" component={Settings} />
    </Router>
  ),
  document.getElementById("root")!
);
