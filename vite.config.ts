import { execFileSync } from "node:child_process";
import { defineConfig, type Plugin } from "vite";
import solid from "vite-plugin-solid";

/**
 * Update log for the About page, generated from the last 10 git commits.
 *
 * Served as a virtual module so `src/lib/changelog.ts` has no committed copy
 * to maintain: every dev run / production build re-reads the repository.
 * A missing git binary (packaged sources without `.git`) degrades to an empty
 * list instead of failing the build.
 */
const CHANGELOG_ID = "virtual:changelog";
const RESOLVED_CHANGELOG_ID = `\0${CHANGELOG_ID}`;

function readGitLog(): string {
  try {
    return execFileSync(
      "git",
      [
        "log",
        "-n",
        "10",
        "--date=short",
        "--pretty=format:%H%x1f%ad%x1f%s%x1f%b%x1e",
      ],
      { encoding: "utf8", maxBuffer: 1024 * 1024 }
    );
  } catch {
    return "";
  }
}

function parseGitLog(raw: string) {
  const trailer = /^(co-authored-by|signed-off-by|reviewed-by|change-id):/i;
  return raw
    .split("\x1e")
    .map((record) => record.trim())
    .filter(Boolean)
    .map((record) => {
      const [hash, date, subject, body = ""] = record.split("\x1f");
      const match = /^([a-z]+)(?:\(([^)]+)\))?!?:\s*(.+)$/i.exec(subject ?? "");
      const notes = body
        .split("\n")
        .map((line) => line.trim().replace(/^[-*]\s+/, ""))
        .filter((line) => line && !trailer.test(line))
        .slice(0, 6)
        .map((line) => (line.length > 240 ? `${line.slice(0, 239)}…` : line));
      return {
        hash: (hash ?? "").slice(0, 7),
        date: date ?? "",
        type: match ? match[1].toLowerCase() : undefined,
        scope: match?.[2],
        title: match ? match[3] : subject ?? "",
        notes,
      };
    });
}

function changelogPlugin(): Plugin {
  return {
    name: "sngl-changelog",
    resolveId(id) {
      return id === CHANGELOG_ID ? RESOLVED_CHANGELOG_ID : undefined;
    },
    load(id) {
      if (id !== RESOLVED_CHANGELOG_ID) return undefined;
      return `export default ${JSON.stringify(parseGitLog(readGitLog()))};`;
    },
  };
}

export default defineConfig({
  define: {
    __APP_VERSION__: JSON.stringify(process.env.npm_package_version ?? "0.3.3"),
  },
  // Relative asset paths: the release WebView serves the bundle from
  // `tauri://localhost`, where absolute `/assets/*` can miss the origin root.
  base: "./",
  plugins: [solid(), changelogPlugin()],
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
    // Skip the gzip/brotli size pass; Tauri embeds the assets anyway.
    reportCompressedSize: false,
  },
});
