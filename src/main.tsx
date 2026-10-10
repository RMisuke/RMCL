/**
 * RMCCC Tauri Entry Point
 *
 * Routes:
 *   /         → Dashboard
 *   /login    → Login
 *   /settings → Settings
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
import "./styles/app.css";

render(
  () => (
    <Router root={App}>
      <Route path="/" component={Dashboard} />
      <Route path="/login" component={Login} />
      <Route path="/settings" component={Settings} />
    </Router>
  ),
  document.getElementById("root")!
);
