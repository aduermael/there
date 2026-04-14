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

- [x] 3a: Add chat variants to protocol — in `game-core/src/protocol.rs`: add `ClientMsg::Chat { text: String }` and `ServerMsg::Chat { from: PlayerId, text: String }`.
- [x] 3b: Add `RoomEvent::Chat` variant — in `room.rs`, add `Chat { id: PlayerId, text: String }`.
- [x] 3c: Server relays chat — in `game_loop.rs`, handle `RoomEvent::Chat`: broadcast `ServerMsg::Chat { from, text }` to all players in the room (including sender, for confirmation).
- [x] 3d: Handle `ClientMsg::Chat` in WebSocket handler — in `main.rs`, add a match arm in the `recv_task` that converts `ClientMsg::Chat` to `RoomEvent::Chat` and sends it to the room.
- [x] 3e: Handle `ServerMsg::Chat` on client — in `lib.rs` `process_server_messages`, store received chat in a list: `Vec<(PlayerId, String, f64)>` (id, text, timestamp). Expire entries older than ~8 seconds. Call a new JS bridge function `js_chat_received(id, text)` to notify the UI.

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

- [x] 4a: New `<chat-console>` web component — a chat overlay at the bottom-left of the screen. Shows the last ~20 messages as scrollable semi-transparent text. Hidden in solo mode.
- [x] 4b: Input activation — pressing Enter focuses a text input at the bottom. Pressing Enter again sends the message and blurs. Pressing Escape cancels and blurs. Input must not trigger game movement keys (stop propagation when focused).
- [x] 4c: WASM bridge for sending — expose `send_chat(text)` from WASM (like `set_joystick_input`). `boot.js` assigns it to `window.sendChat`. The `<chat-console>` calls `window.sendChat(text)` on submit.
- [x] 4d: WASM bridge for receiving — `js_chat_received(id, text)` (from 3e) dispatches a `CustomEvent` on `window` with detail `{id, text}`. The `<chat-console>` listens for this event and appends the message to history. Display format: player color dot + "Player {id}: {text}".
- [x] 4e: Register in index.html — add `<chat-console>` element and script tag. Wire up `window.sendChat` in `boot.js`.
- [x] 4f: Player color in JS — duplicate the 8-color palette from `player.rs` into the chat console component so messages can be color-coded by player ID.

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

- [x] 5a: World-to-screen projection in WASM — add a utility that takes `view_proj` matrix + world position and returns `(screen_x, screen_y, visible)`. A point is visible if it's in front of the camera (`clip.w > 0`) and within the viewport. Compute for each player that has an active chat message.
- [x] 5b: WASM-to-JS bridge for bubble positions — each frame, WASM calls a new JS function `js_update_chat_bubbles(json_string)` with an array of `{id, x, y, text, age}` for players with active messages. `age` is seconds since the message was sent (used for fade-out). Only call when there are active bubbles.
- [x] 5c: New `<chat-bubbles>` web component — manages a pool of absolutely-positioned `<div>` elements. On each `js_update_chat_bubbles` call: create/update/remove divs. Position each div at `(x, y)` offset upward from the player's head. Apply CSS opacity based on `age` (fade out over last 2 seconds of the 5-second window).
- [x] 5d: Register in index.html — add `<chat-bubbles>` element and script. High z-index (above canvas, below menu).
- [x] 5e: Style and polish — bubbles have a dark semi-transparent background, rounded corners, max-width with word wrap, text shadow for readability. Small triangle pointer at bottom (speech bubble). Font size slightly smaller than HUD text.

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

## Phase 6: Chat fixes & UX polish

Goal: fix the Enter-to-send bug, make chat work in solo mode, and improve chat console styling.

- [x] 6a: Fix Enter key not submitting chat — the input element's `stopPropagation()` in `chat-console.js` (line 124) prevents Enter from reaching the `window`-level handler that contains the send logic. Fix: handle Enter and Escape directly inside the input's own `keydown` handler (the one that calls `stopPropagation`), so send/close works when the input is focused. Keep the `window` handler only for the "open input on first Enter press" case.
- [x] 6b: Enable chat in solo mode — remove the `window.__roomCode` visibility gate in `chat-console.js` so the console is always visible. When sending a message with no server connection (`!window.__roomCode`), dispatch a local `chat-received` CustomEvent directly from JS (using `id: 0` for the solo player) instead of relying on server echo. This gives the same chat experience in both modes.
- [x] 6c: Chat console styling — add margin around the chat area so it doesn't touch screen edges (the `padding` in `:host` needs increasing). Widen the input field and increase `max-width` from `min(400px, 80vw)` to something like `min(500px, 85vw)`. Give the input row more vertical breathing room.

### Contracts
- Enter works to send messages when the input is focused (bug fix)
- Chat console visible in both solo and multiplayer
- Solo messages echo locally with player id 0
- Chat area has visible margin from screen edges on all sides

### Success criteria
- Press Enter, type, press Enter again — message appears in chat
- In solo mode: chat is visible, messages echo locally
- Chat bar has clear spacing from screen edges

---

## Phase 7: Room info in settings menu

Goal: make it clear which room the player is in, and add a way to share the room URL.

- [x] 7a: Show current room in settings — when in a room, display the room code prominently in the Multiplayer section of `game-menu.js` (e.g., "Room: ABCD" with styled code). When in solo mode, show "Solo Mode" instead.
- [x] 7b: Copy room URL button — when in a room, add a "Copy Room URL" button next to the room code in the settings panel. Clicking it copies the full URL (`window.location.href`) to the clipboard via `navigator.clipboard.writeText()`. Show brief "Copied!" feedback on success.

### Contracts
- Room code display reads from `window.__roomCode`
- Copy uses the Clipboard API with a visual feedback flash
- Button only shown when in a room

### Success criteria
- Open settings in a room: see "Room: ABCD" clearly
- Click copy: URL is in clipboard, button briefly shows "Copied!"
- Open settings in solo: see "Solo Mode", no copy button

---

## Phase 8: Fix chat console margins

Goal: the chat message stack and input field should have visible spacing from the left and bottom screen edges, matching the screenshot feedback.

- [x] 8a: Fix `:host` padding in `chat-console.js` — the current 2-value shorthand `padding: max(16px, ...) max(16px, ...)` applies vertical/horizontal but the element is `position: fixed; bottom: 0; left: 0` with no explicit width/height, so padding has no visible effect on where child content sits relative to the viewport edge. Replace with explicit `margin` on the `:host` element: `margin: 0 0 max(16px, env(safe-area-inset-bottom)) max(16px, env(safe-area-inset-left))`. Remove the padding or convert it to a small inner padding on `.history` and `.input-row` if needed. The key is that the chat block floats inward from the screen edges.

### Contracts
- Chat messages and input field have at least 16px visible gap from left and bottom edges
- Safe-area insets still respected on notched devices

### Success criteria
- Visually confirm in browser: chat text and input don't touch the screen edges

---

## Phase 9: Fix chat bubbles above player heads

Goal: chat messages should appear as floating speech bubbles above the sending player's character.

- [x] 9a: Set `local_player_id = Some(0)` in solo mode — in `game-client/src/lib.rs`, after constructing `GameState` (~line 427), if `connection.is_none()` (solo mode), set `state.borrow_mut().local_player_id = Some(0)`. This ensures the bubble lookup `Some(bubble.player_id) == state.local_player_id` succeeds when solo-mode chat echoes with `id: 0`.
- [x] 9b: Push local chat bubbles in solo mode — currently `active_bubbles.push(...)` only happens in `GameState::process_server_messages` (line 200-205), which only runs when server messages arrive. In solo mode there's no server. Add a JS→WASM bridge: when the chat-console dispatches a solo echo (`chat-received` with `id: 0`), also call a WASM function `add_local_chat_bubble(text)` that pushes to `active_bubbles` directly. Alternatively, handle this entirely in `lib.rs` by listening for the solo echo event — but the simpler path is a new exported `#[wasm_bindgen]` function that the chat-console calls alongside the local echo dispatch.

### Contracts
- Solo mode: sending a chat message shows a bubble above the local player for 5 seconds
- Multiplayer: bubble behavior unchanged (server echo triggers bubble via `process_server_messages`)
- Local player ID is 0 in solo mode, matching the echo convention

### Success criteria
- Solo mode: type a message, see it float above your character
- Multiplayer: chat bubbles still work as before (server-echoed)
- Bubbles fade and disappear after 5 seconds

---

## Phase 10: Player names

Goal: players can set a display name that persists across sessions (IndexedDB) and is shown in chat instead of "Player {id}".

- [x] 10a: Add `ClientMsg::SetName` to protocol — in `game-core/src/protocol.rs`, add `SetName { name: String }` to `ClientMsg`. Add `NameUpdate { names: Vec<(PlayerId, String)> }` to `ServerMsg`. This message broadcasts all player names whenever one changes.
- [x] 10b: Store name in server Player struct — in `game-server/src/room.rs`, add `pub name: String` to `Player`. Add `RoomEvent::SetName { id: PlayerId, name: String }`. Default name: `"Player {id}"`.
- [x] 10c: Handle SetName in server — in `game_loop.rs`, handle `RoomEvent::SetName`: update `player.name`, broadcast `ServerMsg::NameUpdate` with all `(id, name)` pairs to every player. Also broadcast `NameUpdate` on `RoomEvent::Join` so new players get existing names.
- [x] 10d: Handle SetName in WS handler — in `main.rs`, add a match arm for `ClientMsg::SetName` that validates (trim, non-empty, max 32 chars) and sends `RoomEvent::SetName` to the room.
- [x] 10e: WASM send_player_name bridge — in `game-client/src/lib.rs`, export `send_player_name(name: &str)` that serializes and sends `ClientMsg::SetName`. Add a `HashMap<PlayerId, String>` to `GameState` for player names. Handle `ServerMsg::NameUpdate` in `process_server_messages`: update the map and call a new JS bridge `js_names_updated(json)` that dispatches a `player-names-updated` CustomEvent on `window`.
- [x] 10f: IndexedDB persistence — add a `<script>` in `index.html` (or inline in `boot.js`) that opens an `'game-settings'` IndexedDB with a `'settings'` object store. Expose `window.savePlayerName(name)` and `window.getPlayerName() -> Promise<string|null>` globally. In `boot.js`, after WASM init, load the saved name and call `window.sendPlayerName(name)` if one exists.
- [ ] 10g: Name input in settings menu — in `game-menu.js`, add a "Player" section above the Multiplayer section with a text input (maxlength 32, placeholder "Enter name..."). On blur or Enter, save to IndexedDB and call `window.sendPlayerName(name)`. On menu open, populate from IndexedDB. In solo mode, store the name in `window.__playerName` for local display.
- [ ] 10h: Display names in chat — in `chat-console.js`, listen for `player-names-updated` events and maintain a `Map<id, name>`. In `_renderMessages`, show the name instead of `"Player {id}"`. In solo mode, use `window.__playerName || "Player 0"`.
- [ ] 10i: Display names in chat bubbles — optionally include the player name in the bubble JSON from WASM (requires the name map). Or simpler: in `chat-bubbles.js`, listen for `player-names-updated` and maintain a name map. When rendering a bubble, prefix the text with the name if known.

### Contracts
- Name persists in IndexedDB key `'playerName'` in store `'settings'` of database `'game-settings'`
- `ClientMsg::SetName` validated server-side: trimmed, non-empty, max 32 chars
- `ServerMsg::NameUpdate` broadcast to all players on any name change or new join
- Chat console and bubbles display names instead of "Player {id}" when available
- Default name is `"Player {id}"` until explicitly set
- Solo mode: name stored locally via `window.__playerName`, no server interaction

### Success criteria
- Set name in settings → name persists after page reload
- Send chat in multiplayer → other players see your name, not "Player {id}"
- New player joins → sees existing players' names
- Solo mode: set name, chat shows your name

---

## DRY & refactoring notes

- **Player color palette**: defined in `player.rs` (Rust) and will be needed in `<chat-console>` (JS). Extract as a shared constant in `game-core` and expose via a WASM function `get_player_color(id) -> [r,g,b]` rather than duplicating the array. If the overhead is unacceptable, a single JS constant is fine — just keep it in one place.
- **WASM-JS bridge pattern**: currently each bridge function is a separate `#[wasm_bindgen(inline_js)]` block. As we add chat functions, group related bridge functions together (e.g., all chat-related imports in one `inline_js` block) to keep lib.rs organized.
- **Window globals**: `__roomCode`, `__menuOpen`, `__daylightCycle`, `__sunAngle` — if this grows further, consider a single `window.__gameState` object. Not urgent for this plan.
