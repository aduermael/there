class VirtualJoystick extends HTMLElement {
    constructor() {
        super();
        this.attachShadow({ mode: 'open' });
        this.shadowRoot.innerHTML = `
            <style>
                :host {
                    position: fixed;
                    top: 0;
                    left: 0;
                    width: 100%;
                    height: 100%;
                    z-index: 20;
                    pointer-events: none;
                }
                .base {
                    position: fixed;
                    display: none;
                    width: 120px;
                    height: 120px;
                    border-radius: 50%;
                    background: rgba(255, 255, 255, 0.1);
                    border: 2px solid rgba(255, 255, 255, 0.2);
                    transform: translate(-50%, -50%);
                }
                .knob {
                    position: absolute;
                    width: 50px;
                    height: 50px;
                    border-radius: 50%;
                    background: rgba(255, 255, 255, 0.3);
                    left: 50%;
                    top: 50%;
                    transform: translate(-50%, -50%);
                }
            </style>
            <div class="base"><div class="knob"></div></div>`;

        this._touchId = null;
        this._baseX = 0;
        this._baseY = 0;
        this._maxRadius = 50;
        this._deadZone = 0.1;
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
        // Only capture touches in the left half of the screen
        if (touch.clientX > window.innerWidth * 0.5) return;

        this._touchId = touch.identifier;
        this._baseX = touch.clientX;
        this._baseY = touch.clientY;

        const base = this.shadowRoot.querySelector('.base');
        base.style.left = touch.clientX + 'px';
        base.style.top = touch.clientY + 'px';
        base.style.display = 'block';
    }

    _onMove(e) {
        if (this._touchId === null) return;
        for (const touch of e.changedTouches) {
            if (touch.identifier !== this._touchId) continue;

            const dx = touch.clientX - this._baseX;
            const dy = touch.clientY - this._baseY;
            const dist = Math.sqrt(dx * dx + dy * dy);
            const clamped = Math.min(dist, this._maxRadius);
            const angle = Math.atan2(dy, dx);
            const cx = Math.cos(angle) * clamped;
            const cy = Math.sin(angle) * clamped;

            const knob = this.shadowRoot.querySelector('.knob');
            knob.style.transform = `translate(calc(-50% + ${cx}px), calc(-50% + ${cy}px))`;

            let nx = clamped > 0 ? cx / this._maxRadius : 0;
            let ny = clamped > 0 ? cy / this._maxRadius : 0;

            // Apply dead zone
            const mag = Math.sqrt(nx * nx + ny * ny);
            if (mag < this._deadZone) {
                nx = 0; ny = 0;
            } else {
                const adj = (mag - this._deadZone) / (1 - this._deadZone);
                nx = (nx / mag) * adj;
                ny = (ny / mag) * adj;
            }

            // -Y = forward (up on screen), +X = strafe right
            if (window.setJoystickInput) {
                window.setJoystickInput(-ny, nx);
            }
        }
    }

    _onEnd(e) {
        if (this._touchId === null) return;
        for (const touch of e.changedTouches) {
            if (touch.identifier !== this._touchId) continue;
            this._touchId = null;

            const base = this.shadowRoot.querySelector('.base');
            base.style.display = 'none';
            const knob = this.shadowRoot.querySelector('.knob');
            knob.style.transform = 'translate(-50%, -50%)';

            if (window.setJoystickInput) {
                window.setJoystickInput(0, 0);
            }
        }
    }
}

customElements.define('virtual-joystick', VirtualJoystick);
