#!/usr/bin/env node
/**
 * Version bump helper (RMCCC / SNGL).
 *
 *   node scripts/bump-version.mjs sngl
 *   node scripts/bump-version.mjs rmccc
 *
 * The About page update log is generated from git at build time, so this
 * script only moves the version numbers.
 *
 * Rules:
 *  - `sngl`  — frontend / packaging change: patch +1 (0.3.0 → 0.3.1).
 *              Updates package.json, src-tauri/Cargo.toml and
 *              src-tauri/tauri.conf.json so tooling, IPC and the bundle agree.
 *  - `rmccc` — backend / core change: minor +1 (0.1.0 → 0.2.0) of the *display*
 *              version in `crates/rmccc-core/src/lib.rs` (`CORE_VERSION`). The
 *              crate semver stays 0.1.0 so plugin compatibility ranges keep
 *              working. The About page shows major.minor (0.1 → 0.2), matching
 *              "每更新一次后端就在 RMCCC 上加 0.1".
 */
import { readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const [stream] = process.argv.slice(2);

if (stream !== "sngl" && stream !== "rmccc") {
  console.error("usage: node scripts/bump-version.mjs <sngl|rmccc>");
  process.exit(1);
}

const read = (rel) => readFileSync(path.join(root, rel), "utf8");
const write = (rel, text) => writeFileSync(path.join(root, rel), text);

function bumpSemver(version, part) {
  const [major, minor, patch] = version.split(".").map(Number);
  if (![major, minor, patch].every(Number.isFinite)) {
    throw new Error(`invalid semver: ${version}`);
  }
  return part === "patch" ? `${major}.${minor}.${patch + 1}` : `${major}.${minor + 1}.0`;
}

if (stream === "sngl") {
  const pkg = JSON.parse(read("package.json"));
  const next = bumpSemver(pkg.version, "patch");
  pkg.version = next;
  write("package.json", `${JSON.stringify(pkg, null, 2)}\n`);

  write(
    "src-tauri/Cargo.toml",
    read("src-tauri/Cargo.toml").replace(/^version = "[^"]+"/m, `version = "${next}"`)
  );

  const conf = JSON.parse(read("src-tauri/tauri.conf.json"));
  conf.version = next;
  write("src-tauri/tauri.conf.json", `${JSON.stringify(conf, null, 2)}\n`);

  console.log(`SNGL -> ${next}`);
} else {
  const corePath = "crates/rmccc-core/src/lib.rs";
  const source = read(corePath);
  const match = /pub const CORE_VERSION: &str = "([^"]+)"/.exec(source);
  if (!match) throw new Error(`cannot find CORE_VERSION in ${corePath}`);
  const next = bumpSemver(match[1], "minor");
  write(
    corePath,
    source.replace(
      /pub const CORE_VERSION: &str = "[^"]+"/,
      `pub const CORE_VERSION: &str = "${next}"`
    )
  );

  const display = next.split(".").slice(0, 2).join(".");
  console.log(`RMCCC -> ${display} (display ${next}; crate semver unchanged)`);
}
