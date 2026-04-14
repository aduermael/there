# Multiplayer: Solo Default, Room Browser & Chat

## Context

Currently the server redirects `/` to a random room code (`/{CODE}`), forcing multiplayer on every visit. The client already supports solo mode (no WebSocket when no room code in URL), but the redirect prevents it. There is no room listing, no way to browse/join rooms from the UI, and no chat system.

### Architecture overview

- **Server** (`game-server/`): Axum + WebSocket, per-room game loops at 20 Hz, bincode protocol
- **Protocol** (`game-core/src/protocol.rs`): `ClientMsg::Input` and `ServerMsg::{Welcome, Snapshot, PlayerLeft}` — no chat variants
- **Client** (`game-client/`): WASM + WebGPU, `Connection` struct wraps WebSocket with auto-reconnect
- **UI** (`web/components/`): 6 web components over canvas, communicate via `window` globals and `wasm_bindgen` inline JS
- **Room management** (`game-server/src/room.rs`): `RoomManager` with `HashMap<String, Room>`, rooms created on first join, destroyed when empty

### Key files

| Purpose | Path |
|---------|------|
| Server routes & WS handler | `game-server/src/main.rs` |
| Room manager | `game-server/src/room.rs` |
| Game tick loop | `game-server/src/game_loop.rs` |
| Protocol messages | `game-core/src/protocol.rs` |
| Client networking | `game-client/src/net.rs` |
| Client game loop | `game-client/src/lib.rs` |
| Settings panel | `web/components/game-menu.js` |
| HUD overlay | `web/components/game-hud.js` |
| HTML entry | `web/index.html` |
| Boot script | `web/boot.js` |

---

## Phase 1: Solo mode default + Server room API

Goal: visiting `/` starts the game in solo mode. Server exposes room data via REST.

- [x] 1a: Remove `handle_root` redirect in `main.rs` — delete the `/` route so the fallback serves `web/index.html` directly. Visiting `/` now loads the game with no room code in the URL.
- [x] 1b: Add `GET /api/rooms` endpoint in `main.rs` — reads `RoomManager` and returns JSON array `[{code, player_count}]` for all active rooms. Add a `list_rooms()` method to `RoomManager` that returns `Vec<(String, usize)>`.
- [x] 1c: Add `GET /api/rooms/new` endpoint in `main.rs` — calls `generate_code()` and returns JSON `{code}`. Does not create the room (room is created on first WebSocket connect via `join_or_create`).
- [x] 1d: Update HUD for solo mode — when no room code in URL, hide room code and player count from `game-hud.js`. The HUD should only show FPS in solo mode. WASM side: skip calling `hud_set_room`/`hud_set_players` when connection is `None`.

### Success criteria
- `GET /` loads the game, no redirect, no WebSocket connection, player explores solo
- `GET /ABCD` still joins room ABCD as before
- `GET /api/rooms` returns JSON listing of active rooms
- `GET /api/rooms/new` returns a fresh room code

---

## Phase 2: Room browser in settings menu

Goal: players can browse, create, and join rooms from the settings panel. When in a room, they can leave.

- [x] 2a: Add a "Multiplayer" section to `game-menu.js` — positioned between the time presets and the Resume button. Contains: a room list area, a "Create Room" button, and a "Refresh" button. When the player is already in a room, show a "Leave Room" button instead.
- [x] 2b: Fetch and display room list — when the menu opens and the multiplayer section is visible, `fetch('/api/rooms')` and render each room as a row: room code + player count + "Join" button. Show "No rooms available" when the list is empty.
- [x] 2c: "Create Room" — calls `fetch('/api/rooms/new')`, gets `{code}`, navigates to `/{code}` (page reload starts multiplayer session in that room).
- [x] 2d: "Join" — navigates to `/{code}` for the selected room.
- [x] 2e: "Leave Room" — navigates to `/` (returns to solo mode). Only shown when `window.location.pathname` has a room code.
- [x] 2f: Expose room code to JS — add a `window.__roomCode` global set from WASM (or read from URL in JS) so the menu knows whether we're in solo or multiplayer mode.

### Contracts
- Menu reads room code from `window.__roomCode` (empty string = solo mode)
- Room list fetched via standard `fetch()`, no WASM involvement
- Room join/leave uses `window.location.href` navigation (page reload — keeps Connection lifecycle simple and avoids complex state cleanup)

### Open questions
- Should the room list auto-refresh on a timer while menu is open? Start with manual "Refresh" button; can add polling later.

### Success criteria
- Solo: menu shows room list + Create Room button, no Leave button
- In room: menu shows Leave Room button
- Clicking Create or Join navigates to room URL, game loads in multiplayer
- Clicking Leave navigates to `/`, game loads in solo

---

## Phase 3: Chat protocol & server relay

Goal: extend the binary protocol with chat messages. Server relays chat to all players in a room.

- [ ] 3a: Add chat variants to protocol — in `game-core/src/protocol.rs`: add `ClientMsg::Chat { text: String }` and `ServerMsg::Chat { from: PlayerId, text: String }`.
- [ ] 3b: Add `RoomEvent::Chat` variant — in `room.rs`, add `Chat { id: PlayerId, text: String }`.
- [ ] 3c: Server relays chat — in `game_loop.rs`, handle `RoomEvent::Chat`: broadcast `ServerMsg::Chat { from, text }` to all players in the room (including sender, for confirmation).
- [ ] 3d: Handle `ClientMsg::Chat` in WebSocket handler — in `main.rs`, add a match arm in the `recv_task` that converts `ClientMsg::Chat` to `RoomEvent::Chat` and sends it to the room.
- [ ] 3e: Handle `ServerMsg::Chat` on client — in `lib.rs` `process_server_messages`, store received chat in a list: `Vec<(PlayerId, String, f64)>` (id, text, timestamp). Expire entries older than ~8 seconds. Call a new JS bridge function `js_chat_received(id, text)` to notify the UI.

### Contracts
- Chat text: max 200 characters, trimmed, non-empty (validated on server before relay)
- Server does NOT persist chat — relay only
- `js_chat_received(player_id, text)` called from WASM for each incoming chat message

### Success criteria
- Player A sends chat, Player B receives it
- Own messages are echoed back by server
- Empty/over-limit messages are rejected

---

## Phase 4: Chat console UI

Goal: a web component for chat input and message history, visible only in multiplayer.

- [ ] 4a: New `<chat-console>` web component — a chat overlay at the bottom-left of the screen. Shows the last ~20 messages as scrollable semi-transparent text. Hidden in solo mode.
- [ ] 4b: Input activation — pressing Enter focuses a text input at the bottom. Pressing Enter again sends the message and blurs. Pressing Escape cancels and blurs. Input must not trigger game movement keys (stop propagation when focused).
- [ ] 4c: WASM bridge for sending — expose `send_chat(text)` from WASM (like `set_joystick_input`). `boot.js` assigns it to `window.sendChat`. The `<chat-console>` calls `window.sendChat(text)` on submit.
- [ ] 4d: WASM bridge for receiving — `js_chat_received(id, text)` (from 3e) dispatches a `CustomEvent` on `window` with detail `{id, text}`. The `<chat-console>` listens for this event and appends the message to history. Display format: player color dot + "Player {id}: {text}".
- [ ] 4e: Register in index.html — add `<chat-console>` element and script tag. Wire up `window.sendChat` in `boot.js`.
- [ ] 4f: Player color in JS — duplicate the 8-color palette from `player.rs` into the chat console component so messages can be color-coded by player ID.

### Contracts
- Chat console only visible when `window.__roomCode` is non-empty
- Input field prevents event propagation to game controls when focused
- Enter key: if input not focused, focus it. If focused and non-empty, send. If focused and empty, blur.
- Message history: max 20 messages shown, oldest scroll off top
- Fade out after 5 seconds of inactivity (messages re-appear when new one arrives or input is opened)

### Failure modes
- Input sends empty text: validate in JS before calling `sendChat`
- Input open while game needs keyboard: stop propagation prevents WASD from reaching game input
- Connection lost: messages silently dropped (Connection already handles reconnect)

### Success criteria
- Press Enter to open chat, type, Enter to send
- Messages appear in console with player colors
- Console only visible in multiplayer mode

---

## Phase 5: Chat bubbles above players

Goal: when a player sends a message, it appears as a floating text bubble above their character for ~5 seconds.

- [ ] 5a: World-to-screen projection in WASM — add a utility that takes `view_proj` matrix + world position and returns `(screen_x, screen_y, visible)`. A point is visible if it's in front of the camera (`clip.w > 0`) and within the viewport. Compute for each player that has an active chat message.
- [ ] 5b: WASM-to-JS bridge for bubble positions — each frame, WASM calls a new JS function `js_update_chat_bubbles(json_string)` with an array of `{id, x, y, text, age}` for players with active messages. `age` is seconds since the message was sent (used for fade-out). Only call when there are active bubbles.
- [ ] 5c: New `<chat-bubbles>` web component — manages a pool of absolutely-positioned `<div>` elements. On each `js_update_chat_bubbles` call: create/update/remove divs. Position each div at `(x, y)` offset upward from the player's head. Apply CSS opacity based on `age` (fade out over last 2 seconds of the 5-second window).
- [ ] 5d: Register in index.html — add `<chat-bubbles>` element and script. High z-index (above canvas, below menu).
- [ ] 5e: Style and polish — bubbles have a dark semi-transparent background, rounded corners, max-width with word wrap, text shadow for readability. Small triangle pointer at bottom (speech bubble). Font size slightly smaller than HUD text.

### Contracts
- `js_update_chat_bubbles` receives a JSON string: `[{id: number, x: number, y: number, text: string, age: number}]`
- Bubbles positioned with `position: fixed; left: {x}px; top: {y}px; transform: translate(-50%, -100%)` to center above the player's head
- Bubbles for players behind the camera or off-screen are omitted from the data
- Head offset: project world position at `player.y + 1.7` (above head height ~1.55 + margin)
- Local player's own messages also shown above their character
- Message lifetime: 5 seconds total, fade starts at 3 seconds

### Failure modes
- Player behind camera: filtered out by `clip.w > 0` check
- Many simultaneous messages: bubbles may overlap — acceptable for POC
- Large distance: bubble appears tiny — could add distance-based font size later

### Success criteria
- Send a chat message, see it float above your character
- Other players see your message above your character
- Bubbles fade and disappear after 5 seconds
- No bubbles for players behind the camera

---

## DRY & refactoring notes

- **Player color palette**: defined in `player.rs` (Rust) and will be needed in `<chat-console>` (JS). Extract as a shared constant in `game-core` and expose via a WASM function `get_player_color(id) -> [r,g,b]` rather than duplicating the array. If the overhead is unacceptable, a single JS constant is fine — just keep it in one place.
- **WASM-JS bridge pattern**: currently each bridge function is a separate `#[wasm_bindgen(inline_js)]` block. As we add chat functions, group related bridge functions together (e.g., all chat-related imports in one `inline_js` block) to keep lib.rs organized.
- **Window globals**: `__roomCode`, `__menuOpen`, `__daylightCycle`, `__sunAngle` — if this grows further, consider a single `window.__gameState` object. Not urgent for this plan.
