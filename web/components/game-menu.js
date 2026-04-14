class GameMenu extends HTMLElement {
    constructor() {
        super();
        this.attachShadow({ mode: 'open' });
        this.shadowRoot.innerHTML = `
            <style>
                :host {
                    position: fixed;
                    top: 0; left: 0; right: 0; bottom: 0;
                    z-index: 50;
                    pointer-events: none;
                    font-family: system-ui, sans-serif;
                }
                .trigger {
                    position: fixed;
                    top: max(12px, env(safe-area-inset-top));
                    right: max(12px, env(safe-area-inset-right));
                    margin-top: 32px;
                    width: 44px; height: 44px;
                    display: flex; align-items: center; justify-content: center;
                    cursor: pointer;
                    pointer-events: auto;
                    background: rgba(0,0,0,0.3);
                    border: 1px solid rgba(255,255,255,0.2);
                    border-radius: 8px;
                    color: #fff;
                    font-size: 20px;
                    backdrop-filter: blur(4px);
                    -webkit-backdrop-filter: blur(4px);
                    transition: background 0.15s;
                    z-index: 51;
                }
                .trigger:hover { background: rgba(0,0,0,0.5); }

                .overlay {
                    position: fixed;
                    top: 0; left: 0; right: 0; bottom: 0;
                    background: rgba(0,0,0,0.5);
                    backdrop-filter: blur(6px);
                    -webkit-backdrop-filter: blur(6px);
                    display: none;
                    align-items: center;
                    justify-content: center;
                    pointer-events: auto;
                }
                .overlay.open { display: flex; }

                .panel {
                    background: rgba(20,20,30,0.85);
                    border: 1px solid rgba(255,255,255,0.15);
                    border-radius: 12px;
                    padding: 28px 32px;
                    min-width: 260px;
                    color: #fff;
                    text-align: center;
                }
                .panel h2 {
                    margin: 0 0 20px;
                    font-size: 1.1rem;
                    font-weight: 600;
                    letter-spacing: 0.05em;
                    opacity: 0.9;
                }
                .section-label {
                    font-size: 0.75rem;
                    text-transform: uppercase;
                    letter-spacing: 0.08em;
                    opacity: 0.5;
                    margin: 16px 0 8px;
                }
                .toggle-row {
                    display: flex;
                    align-items: center;
                    justify-content: space-between;
                    gap: 12px;
                    margin-bottom: 12px;
                }
                .toggle-row label { font-size: 0.9rem; }

                .toggle {
                    position: relative;
                    width: 44px; height: 24px;
                    background: rgba(255,255,255,0.15);
                    border-radius: 12px;
                    cursor: pointer;
                    transition: background 0.2s;
                    flex-shrink: 0;
                }
                .toggle.on { background: rgba(100,180,255,0.6); }
                .toggle::after {
                    content: '';
                    position: absolute;
                    top: 2px; left: 2px;
                    width: 20px; height: 20px;
                    background: #fff;
                    border-radius: 50%;
                    transition: transform 0.2s;
                }
                .toggle.on::after { transform: translateX(20px); }

                .presets {
                    display: flex;
                    gap: 6px;
                    justify-content: center;
                    margin: 8px 0 16px;
                }
                .presets button {
                    flex: 1;
                    padding: 8px 0;
                    border: 1px solid rgba(255,255,255,0.2);
                    border-radius: 6px;
                    background: rgba(255,255,255,0.08);
                    color: #fff;
                    font-size: 0.8rem;
                    cursor: pointer;
                    transition: background 0.15s;
                    min-width: 0;
                }
                .presets button:hover { background: rgba(255,255,255,0.2); }

                .mp-section {
                    margin-top: 16px;
                    text-align: left;
                }
                .room-list {
                    max-height: 140px;
                    overflow-y: auto;
                    margin: 8px 0;
                }
                .room-row {
                    display: flex;
                    align-items: center;
                    justify-content: space-between;
                    padding: 6px 0;
                    border-bottom: 1px solid rgba(255,255,255,0.08);
                    font-size: 0.85rem;
                }
                .room-row .code { font-weight: 600; letter-spacing: 0.08em; }
                .room-row .count { opacity: 0.6; margin: 0 8px; }
                .room-row button, .mp-actions button {
                    padding: 4px 12px;
                    border: 1px solid rgba(255,255,255,0.2);
                    border-radius: 6px;
                    background: rgba(255,255,255,0.08);
                    color: #fff;
                    font-size: 0.8rem;
                    cursor: pointer;
                    transition: background 0.15s;
                }
                .room-row button:hover, .mp-actions button:hover { background: rgba(255,255,255,0.2); }
                .mp-actions {
                    display: flex;
                    gap: 6px;
                    margin-top: 8px;
                }
                .mp-actions button { flex: 1; padding: 8px 0; }
                .mp-actions .leave {
                    background: rgba(255,80,80,0.3);
                    border-color: rgba(255,80,80,0.4);
                }
                .mp-actions .leave:hover { background: rgba(255,80,80,0.5); }
                .empty-msg {
                    opacity: 0.4;
                    font-size: 0.8rem;
                    text-align: center;
                    padding: 8px 0;
                }
                .room-info {
                    display: flex;
                    align-items: center;
                    gap: 8px;
                    margin: 8px 0;
                    font-size: 0.9rem;
                }
                .room-info .room-code {
                    font-weight: 700;
                    letter-spacing: 0.1em;
                    font-size: 1rem;
                }
                .room-info .copy-btn {
                    padding: 4px 10px;
                    border: 1px solid rgba(255,255,255,0.2);
                    border-radius: 6px;
                    background: rgba(255,255,255,0.08);
                    color: #fff;
                    font-size: 0.75rem;
                    cursor: pointer;
                    transition: background 0.15s;
                }
                .room-info .copy-btn:hover { background: rgba(255,255,255,0.2); }
                .solo-label {
                    opacity: 0.5;
                    font-size: 0.85rem;
                    margin: 8px 0;
                }

                .resume {
                    margin-top: 8px;
                    padding: 10px 32px;
                    border: none;
                    border-radius: 8px;
                    background: rgba(100,180,255,0.5);
                    color: #fff;
                    font-size: 0.95rem;
                    cursor: pointer;
                    transition: background 0.15s;
                }
                .resume:hover { background: rgba(100,180,255,0.7); }
            </style>
            <div class="trigger" title="Menu (ESC)">&#9881;</div>
            <div class="overlay">
                <div class="panel">
                    <h2>Settings</h2>
                    <div class="toggle-row">
                        <label>Daylight Cycle</label>
                        <div class="toggle on" data-id="cycle"></div>
                    </div>
                    <div class="section-label">Time of Day</div>
                    <div class="presets">
                        <button data-angle="0.0">Dawn</button>
                        <button data-angle="0.25">Noon</button>
                        <button data-angle="0.5">Dusk</button>
                        <button data-angle="0.75">Night</button>
                    </div>
                    <div class="mp-section">
                        <div class="section-label">Multiplayer</div>
                        <div class="room-info-area"></div>
                        <div class="room-list"></div>
                        <div class="mp-actions"></div>
                    </div>
                    <button class="resume">Resume</button>
                </div>
            </div>`;

        this._overlay = this.shadowRoot.querySelector('.overlay');
        this._trigger = this.shadowRoot.querySelector('.trigger');
        this._cycleToggle = this.shadowRoot.querySelector('[data-id="cycle"]');
        this._roomInfoArea = this.shadowRoot.querySelector('.room-info-area');
        this._roomList = this.shadowRoot.querySelector('.room-list');
        this._mpActions = this.shadowRoot.querySelector('.mp-actions');

        // Initialize window globals
        window.__daylightCycle = true;
        window.__menuOpen = false;

        // Trigger button
        this._trigger.addEventListener('click', () => this._toggle());

        // ESC key
        window.addEventListener('keydown', (e) => {
            if (e.code === 'Escape') {
                e.preventDefault();
                this._toggle();
            }
        });

        // Resume button
        this.shadowRoot.querySelector('.resume').addEventListener('click', () => this._close());

        // Close on overlay click (outside panel)
        this._overlay.addEventListener('click', (e) => {
            if (e.target === this._overlay) this._close();
        });

        // Daylight cycle toggle
        this._cycleToggle.addEventListener('click', () => {
            const isOn = this._cycleToggle.classList.toggle('on');
            window.__daylightCycle = isOn;
        });

        // Time-of-day presets
        this.shadowRoot.querySelectorAll('.presets button').forEach(btn => {
            btn.addEventListener('click', () => {
                const angle = parseFloat(btn.dataset.angle);
                window.__sunAngle = angle;
                window.__daylightCycle = false;
                this._cycleToggle.classList.remove('on');
            });
        });
    }

    _toggle() {
        if (this._overlay.classList.contains('open')) {
            this._close();
        } else {
            this._open();
        }
    }

    _open() {
        // Sync toggle state from current window global
        if (window.__daylightCycle) {
            this._cycleToggle.classList.add('on');
        } else {
            this._cycleToggle.classList.remove('on');
        }
        this._updateRoomInfo();
        this._updateMpActions();
        this._fetchRooms();
        this._overlay.classList.add('open');
        window.__menuOpen = true;
    }

    _close() {
        this._overlay.classList.remove('open');
        window.__menuOpen = false;
    }

    _inRoom() {
        return !!(window.__roomCode);
    }

    _updateRoomInfo() {
        if (this._inRoom()) {
            this._roomInfoArea.innerHTML =
                `<div class="room-info">Room: <span class="room-code">${window.__roomCode}</span>` +
                `<button class="copy-btn">Copy URL</button></div>`;
            this._roomInfoArea.querySelector('.copy-btn').addEventListener('click', (e) => {
                navigator.clipboard.writeText(window.location.href).then(() => {
                    e.target.textContent = 'Copied!';
                    setTimeout(() => { e.target.textContent = 'Copy URL'; }, 1500);
                });
            });
        } else {
            this._roomInfoArea.innerHTML = '<div class="solo-label">Solo Mode</div>';
        }
    }

    _updateMpActions() {
        if (this._inRoom()) {
            this._mpActions.innerHTML = '<button class="leave">Leave Room</button>';
            this._mpActions.querySelector('.leave').addEventListener('click', () => {
                window.location.href = '/';
            });
        } else {
            this._mpActions.innerHTML =
                '<button class="create">Create Room</button>' +
                '<button class="refresh">Refresh</button>';
            this._mpActions.querySelector('.create').addEventListener('click', () => this._createRoom());
            this._mpActions.querySelector('.refresh').addEventListener('click', () => this._fetchRooms());
        }
    }

    async _fetchRooms() {
        try {
            const res = await fetch('/api/rooms');
            const rooms = await res.json();
            if (rooms.length === 0) {
                this._roomList.innerHTML = '<div class="empty-msg">No rooms available</div>';
            } else {
                this._roomList.innerHTML = rooms.map(r =>
                    `<div class="room-row">
                        <span class="code">${r.code}</span>
                        <span class="count">${r.player_count} player${r.player_count !== 1 ? 's' : ''}</span>
                        <button data-code="${r.code}">Join</button>
                    </div>`
                ).join('');
                this._roomList.querySelectorAll('button[data-code]').forEach(btn => {
                    btn.addEventListener('click', () => {
                        window.location.href = '/' + btn.dataset.code;
                    });
                });
            }
        } catch {
            this._roomList.innerHTML = '<div class="empty-msg">Could not load rooms</div>';
        }
    }

    async _createRoom() {
        try {
            const res = await fetch('/api/rooms/new');
            const { code } = await res.json();
            window.location.href = '/' + code;
        } catch {
            // silently fail
        }
    }
}

customElements.define('game-menu', GameMenu);
