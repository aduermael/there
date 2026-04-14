class ChatBubbles extends HTMLElement {
    constructor() {
        super();
        this._divs = new Map(); // player id -> div element
        this._playerNames = new Map();

        this.attachShadow({ mode: 'open' });
        this.shadowRoot.innerHTML = `
            <style>
                :host {
                    position: fixed;
                    top: 0;
                    left: 0;
                    width: 100%;
                    height: 100%;
                    pointer-events: none;
                    z-index: 15;
                    overflow: hidden;
                }
                .bubble {
                    position: fixed;
                    transform: translate(-50%, -100%);
                    background: rgba(0, 0, 0, 0.65);
                    color: #fff;
                    padding: 5px 10px;
                    border-radius: 8px;
                    font-family: system-ui, sans-serif;
                    font-size: 0.75rem;
                    max-width: 200px;
                    word-wrap: break-word;
                    text-align: center;
                    text-shadow: 0 1px 2px rgba(0, 0, 0, 0.6);
                    pointer-events: none;
                    white-space: pre-wrap;
                    line-height: 1.3;
                }
                .bubble::after {
                    content: '';
                    position: absolute;
                    bottom: -6px;
                    left: 50%;
                    transform: translateX(-50%);
                    width: 0;
                    height: 0;
                    border-left: 6px solid transparent;
                    border-right: 6px solid transparent;
                    border-top: 6px solid rgba(0, 0, 0, 0.65);
                }
            </style>`;

        window.addEventListener('chat-bubbles-update', (e) => {
            this._update(JSON.parse(e.detail));
        });

        window.addEventListener('player-names-updated', (e) => {
            try {
                const pairs = JSON.parse(e.detail);
                this._playerNames.clear();
                for (const [id, name] of pairs) {
                    this._playerNames.set(id, name);
                }
            } catch {}
        });
    }

    _update(bubbles) {
        const seen = new Set();
        for (const b of bubbles) {
            seen.add(b.id);
            let div = this._divs.get(b.id);
            if (!div) {
                div = document.createElement('div');
                div.className = 'bubble';
                this.shadowRoot.appendChild(div);
                this._divs.set(b.id, div);
            }
            const name = this._playerNames.get(b.id) || (b.id === 0 && window.__playerName) || `Player ${b.id}`;
            div.textContent = `${name}: ${b.text}`;
            div.style.left = b.x + 'px';
            div.style.top = b.y + 'px';
            // Full opacity 0-3s, linear fade 3-5s
            div.style.opacity = b.age < 3 ? 1 : Math.max(0, 1 - (b.age - 3) / 2);
        }
        // Remove bubbles no longer in the data
        for (const [id, div] of this._divs) {
            if (!seen.has(id)) {
                div.remove();
                this._divs.delete(id);
            }
        }
    }
}

customElements.define('chat-bubbles', ChatBubbles);
