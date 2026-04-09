// WebGPU detection and WASM bootstrap
async function init() {
    if (!navigator.gpu) {
        document.body.innerHTML = '<div class="error">WebGPU is not supported in this browser.</div>';
        return;
    }

    const { default: initWasm } = await import('./pkg/game_client.js');
    await initWasm();
}

init().catch(console.error);
