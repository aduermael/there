// IndexedDB helpers for persistent settings
function openSettingsDB() {
    return new Promise((resolve, reject) => {
        const req = indexedDB.open('game-settings', 1);
        req.onupgradeneeded = () => {
            req.result.createObjectStore('settings');
        };
        req.onsuccess = () => resolve(req.result);
        req.onerror = () => reject(req.error);
    });
}

window.savePlayerName = async function(name) {
    try {
        const db = await openSettingsDB();
        const tx = db.transaction('settings', 'readwrite');
        tx.objectStore('settings').put(name, 'playerName');
    } catch (e) {
        console.warn('Failed to save player name:', e);
    }
};

window.getPlayerName = async function() {
    try {
        const db = await openSettingsDB();
        return new Promise((resolve) => {
            const tx = db.transaction('settings', 'readonly');
            const req = tx.objectStore('settings').get('playerName');
            req.onsuccess = () => resolve(req.result || null);
            req.onerror = () => resolve(null);
        });
    } catch (e) {
        return null;
    }
};

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
        window.onJumpPressed = wasm.on_jump_pressed;
        window.sendChat = wasm.send_chat;
        window.addLocalChatBubble = wasm.add_local_chat_bubble;
        window.sendPlayerName = wasm.send_player_name;

        // Load saved player name and send to server
        const savedName = await window.getPlayerName();
        if (savedName) {
            window.__playerName = savedName;
            if (window.sendPlayerName) {
                window.sendPlayerName(savedName);
            }
        }

        screen.hide();
    } catch (e) {
        console.error(e);
        screen.showError('Failed to load game: ' + e.message);
    }
}

init();
