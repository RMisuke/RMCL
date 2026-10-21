/**
 * Transport-level tests for `@rmccc/plugin-ui`.
 *
 * There is no DOM here: the client only touches `window.parent.postMessage`,
 * `window.addEventListener` and timers, so a small stub is enough to exercise
 * the parts that a plugin author actually depends on — request ids, the ready
 * handshake, host error mapping, timeouts and cancellation.
 */
import test from "node:test";
import assert from "node:assert/strict";

/** Install a fake `window` and return a helper to talk back as the host. */
function harness() {
  const sent = [];
  const listeners = [];
  const win = {
    parent: {
      postMessage(data) {
        sent.push(JSON.parse(data));
      },
    },
    addEventListener(type, handler) {
      if (type === "message") listeners.push(handler);
    },
    removeEventListener(type, handler) {
      const index = listeners.indexOf(handler);
      if (index >= 0) listeners.splice(index, 1);
    },
    setTimeout,
    clearTimeout,
  };
  globalThis.window = win;
  /** Deliver a host → plugin message. */
  const host = (message) => {
    for (const listener of [...listeners]) listener({ data: JSON.stringify(message) });
  };
  /** Last request the client posted. */
  const lastRequest = () => [...sent].reverse().find((m) => m.method !== undefined);
  const lastType = (type) => [...sent].reverse().find((m) => m.type === type);
  return { sent, host, lastRequest, lastType };
}

async function loadClient() {
  // Cache-bust so each test gets fresh module state (the module keeps no global
  // state today, but the harness replaces `window` per test).
  const mod = await import(`../packages/plugin-ui/index.js?t=${Math.random()}`);
  return mod;
}

test("announce() posts the ready message", async () => {
  const { lastType } = harness();
  const { announce } = await loadClient();
  announce();
  assert.deepEqual(lastType("ready"), { v: 1, type: "ready" });
});

test("request() posts a v1 request and resolves on the matching reply", async () => {
  const { host, lastRequest } = harness();
  const { connect } = await loadClient();
  const client = connect();
  const promise = client.request("instances.list");
  const request = lastRequest();
  assert.equal(request.v, 1);
  assert.equal(request.method, "instances.list");
  assert.match(request.id, /^[A-Za-z0-9_-]{1,64}$/);

  host({ v: 1, id: request.id, ok: true, result: { instances: [] } });
  assert.deepEqual(await promise, { instances: [] });
  client.dispose();
});

test("error replies become HostError with the wire code", async () => {
  const { host, lastRequest } = harness();
  const { connect } = await loadClient();
  const client = connect();
  const promise = client.request("network.request", { url: "https://x.test" });
  const request = lastRequest();
  host({
    v: 1,
    id: request.id,
    ok: false,
    error: { code: "permission_denied", message: "no", retryable: false },
  });
  await assert.rejects(promise, (error) => {
    assert.equal(error.name, "HostError");
    assert.equal(error.code, "permission_denied");
    assert.equal(error.retryable, false);
    return true;
  });
  client.dispose();
});

test("messages with another protocol version are ignored", async () => {
  const { host, lastRequest } = harness();
  const { connect } = await loadClient();
  const client = connect({ requestTimeoutMs: 2000 });
  const promise = client.request("host.info");
  const request = lastRequest();
  host({ v: 2, id: request.id, ok: true, result: {} });
  host({ v: 1, id: request.id, ok: true, result: { uiApiVersion: 1 } });
  assert.deepEqual(await promise, { uiApiVersion: 1 });
  client.dispose();
});

test("a request that is never answered rejects with timeout", async () => {
  harness();
  const { connect } = await loadClient();
  const client = connect({ requestTimeoutMs: 20 });
  await assert.rejects(client.request("theme.get"), (error) => {
    assert.equal(error.code, "timeout");
    assert.equal(error.retryable, true);
    return true;
  });
  client.dispose();
});

test("aborting via AbortSignal posts a cancel message and rejects as cancelled", async () => {
  const { lastType } = harness();
  const { connect } = await loadClient();
  const client = connect({ requestTimeoutMs: 5000 });
  const controller = new AbortController();
  const promise = client.request("network.request", { url: "https://x.test" }, {
    signal: controller.signal,
  });
  controller.abort();
  await assert.rejects(promise, (error) => {
    assert.equal(error.code, "cancelled");
    return true;
  });
  const cancel = lastType("cancel");
  assert.equal(cancel.v, 1);
  assert.match(cancel.id, /^r\d+$/);
  client.dispose();
});

test("cancel(id) reports whether anything was pending", async () => {
  harness();
  const { connect } = await loadClient();
  const client = connect({ requestTimeoutMs: 5000 });
  assert.equal(client.cancel("r999"), false, "unknown id cancels nothing");
  const promise = client.request("settings.get");
  // Ids are per-client and start at r1 on a fresh module instance.
  assert.equal(client.cancel("r1"), true);
  await assert.rejects(promise, (error) => error.code === "cancelled");
  client.dispose();
});

test("init is delivered to a handler registered before the handshake", async () => {
  const { host } = harness();
  const { connect } = await loadClient();
  const client = connect();
  let seen = null;
  client.onInit((context) => {
    seen = context;
  });
  host({
    v: 1,
    type: "init",
    pluginId: "dev.example.card",
    cardId: "hello",
    theme: { background: "#000000" },
    limits: { maxRequestBytes: 65536, maxResponseBytes: 524288, timeoutMs: 10000 },
  });
  assert.equal(seen.pluginId, "dev.example.card");
  assert.equal(seen.cardId, "hello");
  client.dispose();
});

test("revoke messages reach their handler", async () => {
  const { host } = harness();
  const { connect } = await loadClient();
  const client = connect();
  const revoked = [];
  client.onRevoke((capability) => revoked.push(capability));
  host({ v: 1, type: "revoke", capability: "instance.read" });
  assert.deepEqual(revoked, ["instance.read"]);
  client.dispose();
});
