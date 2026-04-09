async function init() {
    const screen = document.querySelector('connect-screen');

    if (!navigator.gpu) {
        screen.showError('WebGPU is not supported in this browser.');
        return;
    }

    screen.show('Loading game...');

    try {
        const wasm = await import('./pkg/game_client.js');
        await wasm.default();

        // Expose WASM functions for web components
        window.setJoystickInput = wasm.set_joystick_input;
        window.onCameraDrag = wasm.on_camera_drag;

        screen.hide();
    } catch (e) {
        console.error(e);
        screen.showError('Failed to load game: ' + e.message);
    }
}

init();
