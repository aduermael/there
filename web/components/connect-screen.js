class ConnectScreen extends HTMLElement {
    constructor() {
        super();
        this.attachShadow({ mode: 'open' });
        this.shadowRoot.innerHTML = `
            <style>
                :host {
                    position: fixed;
                    inset: 0;
                    z-index: 100;
                    display: flex;
                    align-items: center;
                    justify-content: center;
                    background: #111;
                    color: #fff;
                    font-family: system-ui, sans-serif;
                    transition: opacity 0.3s;
                }
                :host(.hidden) {
                    opacity: 0;
                    pointer-events: none;
                }
                .container { text-align: center; }
                .spinner {
                    width: 32px;
                    height: 32px;
                    border: 3px solid #333;
                    border-top-color: #fff;
                    border-radius: 50%;
                    animation: spin 0.8s linear infinite;
                    margin: 0 auto 16px;
                }
                .error .spinner { display: none; }
                .message { font-size: 1.1rem; opacity: 0.8; }
                .error .message { color: #e55; opacity: 1; }
                @keyframes spin { to { transform: rotate(360deg); } }
            </style>
            <div class="container">
                <div class="spinner"></div>
                <div class="message">Loading...</div>
            </div>`;
    }

    show(msg) {
        this.classList.remove('hidden');
        this.shadowRoot.querySelector('.container').classList.remove('error');
        this.shadowRoot.querySelector('.message').textContent = msg || 'Loading...';
    }

    showError(msg) {
        this.classList.remove('hidden');
        this.shadowRoot.querySelector('.container').classList.add('error');
        this.shadowRoot.querySelector('.message').textContent = msg;
    }

    hide() {
        this.classList.add('hidden');
        setTimeout(() => {
            if (this.classList.contains('hidden')) this.style.display = 'none';
        }, 300);
    }
}

customElements.define('connect-screen', ConnectScreen);
