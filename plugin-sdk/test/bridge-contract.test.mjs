/**
 * Cross-artifact contract test for the isolated UI bridge (ADR-006).
 *
 * The same table exists in four places on purpose (schema for tooling, Rust for
 * enforcement, SDK for authors, docs for humans). This test fails as soon as one
 * of them drifts, which is cheaper than discovering it in a plugin author's bug
 * report.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repo = join(here, "..", "..");
const schema = JSON.parse(
  readFileSync(join(repo, "schemas", "plugin-ui-bridge.schema.json"), "utf8"),
);
const rustUi = readFileSync(
  join(repo, "crates", "rmccc-plugin", "src", "ui.rs"),
  "utf8",
);
const sdkApi = readFileSync(
  join(here, "..", "packages", "plugin-api", "src", "index.ts"),
  "utf8",
);

/** Method names listed in `BRIDGE_METHODS` (Rust). */
function rustMethods() {
  const block = rustUi.slice(
    rustUi.indexOf("pub const BRIDGE_METHODS"),
    rustUi.indexOf("/// 按名字查方法表"),
  );
  return [...block.matchAll(/name:\s*"([a-z.]+)"/g)].map((m) => m[1]);
}

/** `BridgeErrorCode::as_str` arms. */
function rustWireCodes() {
  const block = rustUi.slice(rustUi.indexOf("pub fn as_str(self)"), rustUi.indexOf("pub fn retryable"));
  return [...block.matchAll(/Self::\w+\s*=>\s*"([a-z_]+)"/g)].map((m) => m[1]);
}

/** Keys of `METHOD_CAPABILITIES` in the SDK. */
function sdkMethods() {
  const block = sdkApi.slice(
    sdkApi.indexOf("export const METHOD_CAPABILITIES"),
    sdkApi.indexOf("export const METHOD_TIMEOUTS"),
  );
  return [...block.matchAll(/"([a-z.]+)":/g)].map((m) => m[1]);
}

const schemaMethods = schema.$defs.pluginRequest.properties.method.enum;
const schemaCodes = schema.$defs.wireErrorCode.enum;

test("schema, Rust and SDK agree on the method table", () => {
  const rust = rustMethods();
  assert.ok(rust.length >= 1, "failed to parse BRIDGE_METHODS");
  assert.deepEqual(rust, schemaMethods, "ui.rs BRIDGE_METHODS vs schema enum");
  assert.deepEqual(sdkMethods(), schemaMethods, "SDK METHOD_CAPABILITIES vs schema enum");
});

test("schema, Rust and SDK agree on the wire error codes", () => {
  const rust = rustWireCodes();
  assert.ok(rust.length >= 1, "failed to parse BridgeErrorCode::as_str");
  assert.deepEqual(rust, schemaCodes, "ui.rs wire codes vs schema enum");
  for (const code of schemaCodes) {
    assert.ok(sdkApi.includes(`"${code}"`), `SDK is missing wire code ${code}`);
  }
});

test("the API version is 1 everywhere", () => {
  assert.equal(schema.$defs.apiVersion.const, 1);
  assert.match(rustUi, /pub const UI_API_VERSION: u32 = 1;/);
  assert.match(sdkApi, /export const UI_API_VERSION = 1;/);
});

test("every method that needs a capability names a known one", () => {
  const block = sdkApi.slice(
    sdkApi.indexOf("export const METHOD_CAPABILITIES"),
    sdkApi.indexOf("export const METHOD_TIMEOUTS"),
  );
  const entries = [...block.matchAll(/"([a-z.]+)":\s*(null|"([a-z.]+)")/g)];
  assert.equal(entries.length, schemaMethods.length);
  const known = ["settings.read", "instance.read", "network.request"];
  for (const [, method, , capability] of entries) {
    if (capability) assert.ok(known.includes(capability), `${method} uses unknown capability ${capability}`);
  }
});
