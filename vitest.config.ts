import { defineConfig } from "vitest/config";

// Pure-function test bench (B5): no Solid/Vite plugins needed — the covered
// modules are plain TypeScript. Kept separate from vite.config.ts so the app
// build chain stays untouched.
export default defineConfig({
  test: {
    include: ["src/**/*.test.ts"],
    environment: "node",
  },
});
