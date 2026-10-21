#!/usr/bin/env node
/**
 * `rmccc-plugin-sdk` CLI: validate a plugin directory or pack it into `.rmcpkg`.
 *
 * Validation mirrors the host rules (ADR-003/ADR-006) so a plugin author sees the
 * same rejections the installer would produce, without installing:
 *
 *   - `manifest.json` parses and declares `id` / `version` / `compatibility`
 *   - `entry.ui` (when present) lives under `ui/`, ends in `.html`, and exists
 *   - `ui/` files stay inside the asset whitelist and never leave `ui/`
 *   - `logic/`, scripts outside `ui/`, and symlinks are rejected
 *
 * Packing shells out to `rmcli plugin pack` (single implementation of the zip
 * layout); pass `--rmcli <path>` when the binary is not on PATH.
 */
import { execFileSync } from "node:child_process";
import { readFileSync, readdirSync, statSync, existsSync } from "node:fs";
import { join, relative, sep } from "node:path";

const ASSET_EXTENSIONS = new Set([
  "html", "htm", "js", "mjs", "css", "json", "md", "txt", "svg", "png", "jpg",
  "jpeg", "gif", "webp", "ttf", "otf", "woff", "woff2", "map",
]);
const FORBIDDEN_TOP = new Set(["logic"]);
const FORBIDDEN_EXT = new Set(["sh", "command", "app", "jar", "class", "py", "pl", "rb", "wasm"]);

function fail(message) {
  console.error(`✗ ${message}`);
  process.exitCode = 1;
  return false;
}

function ok(message) {
  console.log(`✓ ${message}`);
  return true;
}

function walk(dir, base = dir, out = []) {
  for (const name of readdirSync(dir)) {
    const full = join(dir, name);
    const rel = relative(base, full).split(sep).join("/");
    const info = statSync(full);
    if (info.isSymbolicLink()) {
      out.push({ rel, kind: "symlink" });
      continue;
    }
    if (info.isDirectory()) walk(full, base, out);
    else out.push({ rel, kind: "file" });
  }
  return out;
}

function validate(dir) {
  let failed = false;
  const manifestPath = join(dir, "manifest.json");
  if (!existsSync(manifestPath)) return fail("missing manifest.json");

  let manifest;
  try {
    manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
  } catch (e) {
    return fail(`manifest.json is not valid JSON: ${e.message}`);
  }

  for (const field of ["manifestVersion", "id", "name", "version", "publisher", "compatibility"]) {
    if (manifest[field] === undefined) failed = true, fail(`manifest.${field} is required`);
  }
  if (manifest.manifestVersion !== 1) failed = true, fail("manifestVersion must be 1");
  if (typeof manifest.id !== "string" || !/^[a-z][a-z0-9-]*(\.[a-z][a-z0-9-]*)+$/.test(manifest.id)) {
    failed = true, fail(`manifest.id must be reverse-DNS (got ${JSON.stringify(manifest.id)})`);
  }
  if (manifest.compatibility?.pluginApi !== "1.x") {
    failed = true, fail("compatibility.pluginApi must be \"1.x\"");
  }

  const entryUi = manifest.entry?.ui;
  const contributes = manifest.contributes ?? {};
  const cardCount = (contributes.dashboardCards ?? []).length;
  const pageCount = (contributes.settingsPages ?? []).length;

  if (entryUi !== undefined) {
    if (!/^ui\/.*\.(html|htm)$/.test(entryUi)) {
      failed = true, fail("entry.ui must be ui/<file>.html");
    } else if (!existsSync(join(dir, entryUi))) {
      failed = true, fail(`entry.ui points at a missing file: ${entryUi}`);
    }
    if (cardCount + pageCount === 0) {
      failed = true, fail("entry.ui requires at least one dashboardCards or settingsPages contribution");
    }
  }

  const files = walk(dir);
  const uiFiles = files.filter((f) => f.rel.startsWith("ui/"));
  if (uiFiles.length > 0 && entryUi === undefined) {
    failed = true, fail("ui/ exists but manifest.entry.ui is missing");
  }

  for (const file of files) {
    const top = file.rel.split("/")[0];
    if (file.kind === "symlink") {
      failed = true, fail(`symlink is not allowed: ${file.rel}`);
      continue;
    }
    if (FORBIDDEN_TOP.has(top)) {
      failed = true, fail(`logic/ is not supported before Phase 5: ${file.rel}`);
      continue;
    }
    if (!file.rel.includes(".")) continue;
    const ext = file.rel.split(".").pop().toLowerCase();
    if (FORBIDDEN_EXT.has(ext)) {
      failed = true, fail(`executable content is not allowed: ${file.rel}`);
      continue;
    }
    if (top === "ui") {
      if (!ASSET_EXTENSIONS.has(ext)) {
        failed = true, fail(`ui/ asset type is not allowed: ${file.rel}`);
      }
      if (!file.rel.startsWith("ui/")) failed = true, fail(`ui asset escaped its directory: ${file.rel}`);
    }
  }

  if (!failed) ok(`manifest valid: ${manifest.id}@${manifest.version}`);
  if (!failed) ok(`${files.length} file(s), ${uiFiles.length} UI asset(s)`);
  if (!failed && entryUi) ok(`isolated UI entry: ${entryUi}`);
  if (!failed && !entryUi) ok("declarative plugin (no isolated UI)");
  return !failed;
}

function pack(dir, out, rmcli) {
  if (!validate(dir)) return fail("refusing to pack an invalid plugin");
  const args = ["plugin", "pack", "--dir", dir, "--out", out];
  try {
    execFileSync(rmcli, args, { stdio: "inherit" });
    return ok(`packed ${out}`);
  } catch (e) {
    return fail(`rmcli plugin pack failed (${e.message}); pass --rmcli <path>`);
  }
}

const [command, ...rest] = process.argv.slice(2);
const flags = Object.fromEntries(
  rest.reduce((acc, item, index) => {
    if (item.startsWith("--")) acc.push([item.slice(2), rest[index + 1]]);
    return acc;
  }, []),
);

if (command === "validate") {
  const dir = flags.dir ?? ".";
  console.log(`validating ${dir}`);
  process.exit(validate(dir) ? 0 : 1);
} else if (command === "pack") {
  const dir = flags.dir ?? ".";
  const out = flags.out ?? "plugin.rmcpkg";
  const rmcli = flags.rmcli ?? "rmcli";
  process.exit(pack(dir, out, rmcli) ? 0 : 1);
} else {
  console.error("usage: cli.mjs <validate|pack> --dir <plugin-dir> [--out <file.rmcpkg>] [--rmcli <path>]");
  process.exit(2);
}
