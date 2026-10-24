import { defineConfig } from "vite";
import solid from "vite-plugin-solid";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  define: {
    __APP_VERSION__: JSON.stringify(process.env.npm_package_version ?? "0.1.0"),
  },
  // Relative asset paths: the release WebView serves the bundle from
  // `tauri://localhost`, where absolute `/assets/*` can miss the origin root.
  base: "./",
  plugins: [solid(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    target: "esnext",
    minify: "esbuild",
    sourcemap: false,
  },
});
