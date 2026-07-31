/**
 * RMCCC Tauri Entry Point — Placeholder
 *
 * Loads the minimal placeholder App. Full GUI development is deferred;
 * use `rmcli` CLI for all launcher functionality.
 */
import { render } from "solid-js/web";
import App from "./App";
import "./styles/app.css";

render(() => <App />, document.getElementById("root")!);
