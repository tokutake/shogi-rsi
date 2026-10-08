// Run after `sh scripts/build-web.sh`: node tests/web-engine.mjs
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { performance } from "node:perf_hooks";
import vm from "node:vm";

const wasm = await readFile(new URL("../dist/shogi_rsi.wasm", import.meta.url));
const { instance } = await WebAssembly.instantiate(wasm, {
  env: { now_ms: () => performance.now() },
});
const api = instance.exports;
function request(body) {
  const bytes = new TextEncoder().encode(body);
  const input = api.input_buffer(bytes.length);
  new Uint8Array(api.memory.buffer, input, bytes.length).set(bytes);
  const output = api.game_request();
  return JSON.parse(new TextDecoder().decode(new Uint8Array(api.memory.buffer, output, api.output_len())));
}
const initial = request("0 100 state");
assert.equal(initial.board.length, 81);
assert.equal(initial.legal.length, 30);
assert.equal(request("0 100 state 7g7e").error, "Invalid game request");
assert.equal(request("0 100 state 7g7fgarbage").error, "Invalid game request");
assert.equal(request("0 100 state 7g7f 3c3d 8h2b+ 3a2b B*5e").moves.length, 5);
assert.match(request(`0 100 play ${"5i5h 5a5b 5h5i 5b5a ".repeat(3)}`).result, /千日手/);
assert.equal(api.input_buffer(8193), 0);
const started = performance.now();
const reply = request("0 100 play 7g7f");
assert.equal(reply.side, 0);
assert.equal(reply.moves.length, 2);
assert.deepEqual(request(`0 100 state ${reply.moves.join(" ")}`), reply);
assert.equal(request("1 100 play").moves.length, 1);
assert.ok(performance.now() - started < 5000, "browser clock must stop timed searches");

// Exercise the actual worker adapter, including ready and error messages.
let send;
const received = new Promise(resolve => { send = resolve; });
let ready;
const initialized = new Promise(resolve => { ready = resolve; });
const context = vm.createContext({
  WebAssembly, TextEncoder, TextDecoder, Uint8Array, performance,
  fetch: async () => ({ ok: true, arrayBuffer: async () => wasm }),
  postMessage: message => message.ready ? ready(message) : send(message),
  onmessage: null,
});
vm.runInContext(await readFile(new URL("../web/engine-worker.js", import.meta.url), "utf8"), context);
await initialized;
await context.onmessage({ data: { id: 42, body: "0 100 play 7g7f" } });
const message = await received;
assert.equal(message.id, 42);
assert.equal(message.game.moves.length, 2);
console.log("WebAssembly engine and worker checks passed");
