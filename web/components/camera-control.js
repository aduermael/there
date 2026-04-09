class CameraControl extends HTMLElement {
    constructor() {
        super();
        this._touchId = null;
        this._lastX = 0;
        this._lastY = 0;
    }

    connectedCallback() {
        window.addEventListener('touchstart', this._onStart.bind(this), { passive: true });
        window.addEventListener('touchmove', this._onMove.bind(this), { passive: true });
        window.addEventListener('touchend', this._onEnd.bind(this));
        window.addEventListener('touchcancel', this._onEnd.bind(this));
    }

    _onStart(e) {
        if (this._touchId !== null) return;
        const touch = e.changedTouches[0];
        // Only capture touches in the right half of the screen
        if (touch.clientX <= window.innerWidth * 0.5) return;

        this._touchId = touch.identifier;
        this._lastX = touch.clientX;
        this._lastY = touch.clientY;
    }

    _onMove(e) {
        if (this._touchId === null) return;
        for (const touch of e.changedTouches) {
            if (touch.identifier !== this._touchId) continue;

            const dx = touch.clientX - this._lastX;
            const dy = touch.clientY - this._lastY;
            this._lastX = touch.clientX;
            this._lastY = touch.clientY;

            if (window.onCameraDrag) {
                window.onCameraDrag(dx, dy);
            }
        }
    }

    _onEnd(e) {
        if (this._touchId === null) return;
        for (const touch of e.changedTouches) {
            if (touch.identifier !== this._touchId) continue;
            this._touchId = null;
        }
    }
}

customElements.define('camera-control', CameraControl);
