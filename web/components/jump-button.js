class JumpButton extends HTMLElement {
    constructor() {
        super();
        this.attachShadow({ mode: 'open' });
        this.shadowRoot.innerHTML = `
            <style>
                :host {
                    position: fixed;
                    bottom: max(24px, env(safe-area-inset-bottom));
                    right: max(24px, env(safe-area-inset-right));
                    z-index: 10;
                    pointer-events: none;
                    display: none;
                }
                @media (pointer: coarse) {
                    :host { display: block; }
                }
                button {
                    width: 64px; height: 64px;
                    border-radius: 50%;
                    border: 2px solid rgba(255,255,255,0.3);
                    background: rgba(255,255,255,0.12);
                    color: #fff;
                    font-size: 1.2rem;
                    font-weight: bold;
                    pointer-events: auto;
                    touch-action: none;
                    cursor: pointer;
                    backdrop-filter: blur(4px);
                    -webkit-backdrop-filter: blur(4px);
                    transition: background 0.1s;
                }
                button:active { background: rgba(255,255,255,0.3); }
            </style>
            <button>&#8593;</button>`;

        const btn = this.shadowRoot.querySelector('button');
        btn.addEventListener('touchstart', (e) => {
            e.preventDefault();
            if (window.onJumpPressed) {
                window.onJumpPressed();
            }
        }, { passive: false });
    }
}

customElements.define('jump-button', JumpButton);
