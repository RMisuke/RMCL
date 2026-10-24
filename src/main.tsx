/**
 * RMCCC entry point (ui-design 00 route table).
 *
 *   /              Home dashboard
 *   /instances     Instance list
 *   /instances/:id Instance detail (hash tabs)
 *   /download      Download center (wizard / mod store / modpack)
 *   /accounts      Accounts (merged former /login)
 *   /plugins       Plugin manager
 *   /settings      Settings (hash sections incl. #ai)
 *
 * Legacy routes redirect: /login → /accounts, /mods → /download#mods,
 * /agent → /settings#ai (ADR-013).
 */
import { render } from "solid-js/web";
import { Router, Route, Navigate } from "@solidjs/router";
import { lazy } from "solid-js";
import App from "./App";
import "./styles/app.css";

const Home = lazy(() => import("./pages/Home"));
const Instances = lazy(() => import("./pages/Instances"));
const InstanceDetail = lazy(() => import("./pages/InstanceDetail"));
const Download = lazy(() => import("./pages/Download"));
const Accounts = lazy(() => import("./pages/Accounts"));
const Plugins = lazy(() => import("./pages/Plugins"));
const Settings = lazy(() => import("./pages/Settings"));

render(
  () => (
    <Router root={App}>
      <Route path="/" component={Home} />
      <Route path="/instances" component={Instances} />
      <Route path="/instances/:id" component={InstanceDetail} />
      <Route path="/download" component={Download} />
      <Route path="/accounts" component={Accounts} />
      <Route path="/plugins" component={Plugins} />
      <Route path="/settings" component={Settings} />
      <Route path="/login" component={() => <Navigate href="/accounts" />} />
      <Route path="/mods" component={() => <Navigate href="/download#mods" />} />
      <Route path="/agent" component={() => <Navigate href="/settings#ai" />} />
      <Route path="*" component={() => <Navigate href="/" />} />
    </Router>
  ),
  document.getElementById("root")!
);

// Tell the inline boot guard in `index.html` that the shell rendered, so it
// drops its failure panel and stops reporting a boot timeout.
window.dispatchEvent(new Event("rmccc:shell-mounted"));
