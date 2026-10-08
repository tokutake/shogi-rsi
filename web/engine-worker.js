"use strict";
const engine = fetch("./shogi_rsi.wasm")
  .then(response => {
    if (!response.ok) throw new Error("AIを読み込めませんでした");
    return response.arrayBuffer();
  })
  .then(bytes => WebAssembly.instantiate(bytes, { env: { now_ms: () => performance.now() } }))
  .then(result => result.instance.exports);
// Report initialization failures even before the first request.
engine.then(() => postMessage({ ready: true }), error => postMessage({ error: error.message }));
onmessage = async ({ data }) => {
  try {
    const api = await engine;
    const bytes = new TextEncoder().encode(data.body);
    const input = api.input_buffer(bytes.length);
    if (!input) throw new Error("対局データが大きすぎます");
    new Uint8Array(api.memory.buffer, input, bytes.length).set(bytes);
    const output = api.game_request();
    const game = JSON.parse(new TextDecoder().decode(
      new Uint8Array(api.memory.buffer, output, api.output_len()),
    ));
    if (game.error) throw new Error("対局データを処理できませんでした");
    postMessage({ id: data.id, game });
  } catch (error) {
    postMessage({ id: data.id, error: error.message });
  }
};
