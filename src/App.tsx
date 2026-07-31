/**
 * RMCCC Tauri Frontend — Placeholder
 *
 * GUI development is deferred. Use the CLI (`rmcli`) for all launcher operations.
 * This frontend exists as a Tauri bridge skeleton for future GUI work.
 */
import { Component, onMount } from "solid-js";

const App: Component = () => {
  onMount(() => {
    console.log("RMCCC Tauri frontend loaded (placeholder)");
  });

  return (
    <div class="flex items-center justify-center min-h-screen bg-zinc-950 text-zinc-200">
      <div class="text-center max-w-md p-8">
        <h1 class="text-2xl font-bold text-red-500 mb-4">RMCCC</h1>
        <p class="text-zinc-400 mb-6">
          GUI is under construction. Please use the CLI tool for all operations.
        </p>
        <code class="block bg-zinc-900 border border-zinc-800 rounded-lg p-4 text-sm text-zinc-300 font-mono">
          $ rmcli --help
        </code>
        <p class="mt-6 text-xs text-zinc-600">
          Tauri v2 bridge active — IPC commands available at{" "}
          <code class="text-zinc-500">tauri://localhost</code>
        </p>
      </div>
    </div>
  );
};

export default App;
