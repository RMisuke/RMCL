#!/usr/bin/env node
/**
 * Stage `SNGL-logic-host` as a Tauri `externalBin`.
 *
 * Tauri only bundles an external binary that already exists at
 * `src-tauri/binaries/<name>-<target-triple>[.exe]`, and then installs it next
 * to the app executable with the triple stripped. `rmccc-logic::supervisor`
 * looks the helper up as a sibling of the current executable, so this is what
 * makes logic plugins work in a packaged build on both macOS and Windows.
 *
 * No external dependency: the host triple comes from `rustc -vV` and the file
 * is produced by the workspace's own `cargo build`. The crate keeps its
 * `rmccc-logic-host` package name; only the binary carries the product name.
 */
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { join } from "node:path";

const PACKAGE = "rmccc-logic-host";
const BINARY = "SNGL-logic-host";
const isWindows = process.platform === "win32";
const exeSuffix = isWindows ? ".exe" : "";

const rustc = execFileSync("rustc", ["-vV"], { encoding: "utf8" });
const host = /^host:\s*(.+)$/m.exec(rustc)?.[1]?.trim();
if (!host) {
  throw new Error("cannot read the host triple from `rustc -vV`");
}

const root = process.cwd();
const targetRoot = process.env.CARGO_TARGET_DIR ?? join(root, "target");
const profile = process.env.RMCCC_HOST_PROFILE === "debug" ? "debug" : "release";
const cargoArgs = ["build", "-p", PACKAGE];
if (profile === "release") cargoArgs.push("--release");

execFileSync("cargo", cargoArgs, { stdio: "inherit" });

const built = join(targetRoot, profile, `${BINARY}${exeSuffix}`);
const outDir = join(root, "src-tauri", "binaries");
mkdirSync(outDir, { recursive: true });
const staged = join(outDir, `${BINARY}-${host}${exeSuffix}`);
copyFileSync(built, staged);
console.log(`externalBin ready: ${staged}`);
