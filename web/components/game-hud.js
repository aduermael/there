class GameHud extends HTMLElement {
    constructor() {
        super();
        this.attachShadow({ mode: 'open' });
        this.shadowRoot.innerHTML = `
            <style>
                :host {
                    position: fixed;
                    top: 0;
                    left: 0;
                    z-index: 10;
                    padding: max(12px, env(safe-area-inset-top)) max(12px, env(safe-area-inset-right)) 12px max(12px, env(safe-area-inset-left));
                    color: #fff;
                    font-family: system-ui, sans-serif;
                    font-size: 0.85rem;
                    pointer-events: none;
                    text-shadow: 0 1px 3px rgba(0,0,0,0.6);
                }
                .room { font-size: 1.1rem; font-weight: bold; letter-spacing: 0.1em; }
                .players { opacity: 0.7; margin-top: 4px; }
            </style>
            <div class="room"></div>
            <div class="players"></div>`;
    }

    set roomCode(code) {
        this.shadowRoot.querySelector('.room').textContent = code ? `Room: ${code}` : '';
    }

    set playerCount(n) {
        this.shadowRoot.querySelector('.players').textContent = n != null ? `Players: ${n}` : '';
    }
}

customElements.define('game-hud', GameHud);
