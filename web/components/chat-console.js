const PLAYER_COLORS = [
    [230, 77, 64],   // red
    [64, 153, 230],  // blue
    [77, 217, 102],  // green
    [242, 191, 51],  // yellow
    [204, 102, 230], // purple
    [242, 140, 64],  // orange
    [64, 217, 217],  // cyan
    [230, 115, 179], // pink
];

function colorForId(id) {
    const c = PLAYER_COLORS[id % PLAYER_COLORS.length];
    return `rgb(${c[0]},${c[1]},${c[2]})`;
}

class ChatConsole extends HTMLElement {
    constructor() {
        super();
        this._messages = [];
        this._maxMessages = 20;
        this._fadeTimer = null;
        this._visible = false;

        this.attachShadow({ mode: 'open' });
        this.shadowRoot.innerHTML = `
            <style>
                :host {
                    position: fixed;
                    bottom: 0;
                    left: 0;
                    z-index: 20;
                    padding: max(12px, env(safe-area-inset-bottom)) max(12px, env(safe-area-inset-left));
                    pointer-events: none;
                    font-family: system-ui, sans-serif;
                    font-size: 0.85rem;
                    max-width: min(400px, 80vw);
                    display: none;
                }
                :host(.active) { display: block; }
                .history {
                    max-height: 200px;
                    overflow-y: auto;
                    display: flex;
                    flex-direction: column;
                    gap: 2px;
                    transition: opacity 0.3s;
                    scrollbar-width: none;
                }
                .history::-webkit-scrollbar { display: none; }
                :host(.faded) .history { opacity: 0; }
                .msg {
                    background: rgba(0,0,0,0.45);
                    padding: 3px 8px;
                    border-radius: 4px;
                    color: #fff;
                    text-shadow: 0 1px 2px rgba(0,0,0,0.5);
                    word-break: break-word;
                }
                .msg .dot {
                    display: inline-block;
                    width: 8px; height: 8px;
                    border-radius: 50%;
                    margin-right: 4px;
                    vertical-align: middle;
                }
                .input-row {
                    margin-top: 4px;
                    display: none;
                    pointer-events: auto;
                }
                .input-row.open { display: flex; }
                .input-row input {
                    flex: 1;
                    background: rgba(0,0,0,0.6);
                    border: 1px solid rgba(255,255,255,0.25);
                    border-radius: 4px;
                    color: #fff;
                    padding: 6px 8px;
                    font-size: 0.85rem;
                    outline: none;
                    font-family: inherit;
                }
                .input-row input::placeholder { color: rgba(255,255,255,0.4); }
            </style>
            <div class="history"></div>
            <div class="input-row">
                <input type="text" maxlength="200" placeholder="Type a message...">
            </div>`;

        this._history = this.shadowRoot.querySelector('.history');
        this._inputRow = this.shadowRoot.querySelector('.input-row');
        this._input = this.shadowRoot.querySelector('input');

        // Listen for chat messages from WASM
        window.addEventListener('chat-received', (e) => {
            this._addMessage(e.detail.id, e.detail.text);
        });

        // Enter key to open/send/close
        window.addEventListener('keydown', (e) => {
            if (!this._visible) return;
            if (e.code === 'Enter') {
                if (this._inputRow.classList.contains('open')) {
                    // Input is open
                    const text = this._input.value.trim();
                    if (text) {
                        if (window.sendChat) window.sendChat(text);
                        this._input.value = '';
                    }
                    this._closeInput();
                } else {
                    e.preventDefault();
                    this._openInput();
                }
            }
            if (e.code === 'Escape' && this._inputRow.classList.contains('open')) {
                this._closeInput();
            }
        });

        // Prevent game keys while typing
        this._input.addEventListener('keydown', (e) => {
            e.stopPropagation();
        });
        this._input.addEventListener('keyup', (e) => {
            e.stopPropagation();
        });
    }

    connectedCallback() {
        this._checkVisibility();
        this._visCheck = setInterval(() => this._checkVisibility(), 1000);
    }

    disconnectedCallback() {
        clearInterval(this._visCheck);
    }

    _checkVisibility() {
        const inRoom = !!(window.__roomCode);
        if (inRoom && !this._visible) {
            this._visible = true;
            this.classList.add('active');
        } else if (!inRoom && this._visible) {
            this._visible = false;
            this.classList.remove('active');
        }
    }

    _addMessage(id, text) {
        this._messages.push({ id, text });
        if (this._messages.length > this._maxMessages) {
            this._messages.shift();
        }
        this._renderMessages();
        this._resetFade();
    }

    _renderMessages() {
        this._history.innerHTML = this._messages.map(m => {
            const color = colorForId(m.id);
            const escaped = m.text.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;');
            return `<div class="msg"><span class="dot" style="background:${color}"></span>Player ${m.id}: ${escaped}</div>`;
        }).join('');
        this._history.scrollTop = this._history.scrollHeight;
    }

    _resetFade() {
        this.classList.remove('faded');
        clearTimeout(this._fadeTimer);
        this._fadeTimer = setTimeout(() => {
            if (!this._inputRow.classList.contains('open')) {
                this.classList.add('faded');
            }
        }, 5000);
    }

    _openInput() {
        this._inputRow.classList.add('open');
        this._input.focus();
        this.classList.remove('faded');
        clearTimeout(this._fadeTimer);
    }

    _closeInput() {
        this._inputRow.classList.remove('open');
        this._input.blur();
        this._input.value = '';
        this._resetFade();
    }
}

customElements.define('chat-console', ChatConsole);
