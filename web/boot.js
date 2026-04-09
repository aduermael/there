async function init() {
    const screen = document.querySelector('connect-screen');

    if (!navigator.gpu) {
        screen.showError('WebGPU is not supported in this browser.');
        return;
    }

    screen.show('Loading game...');

    try {
        const { default: initWasm } = await import('./pkg/game_client.js');
        await initWasm();
        screen.hide();
    } catch (e) {
        console.error(e);
        screen.showError('Failed to load game: ' + e.message);
    }
}

init();
