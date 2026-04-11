.PHONY: snapshot snapshot-build client server dev

SNAPSHOT_BIN = cargo run --release -p game-snapshot --
SNAPSHOT_DIR = snapshots

snapshot-build:
	cargo build --release -p game-snapshot

snapshot: snapshot-build
	@mkdir -p $(SNAPSHOT_DIR)
	$(SNAPSHOT_BIN) --sun-angle 0.03 --camera-pos "100,25,140" --camera-target "160,12,155" --output $(SNAPSHOT_DIR)/dawn.png
	$(SNAPSHOT_BIN) --sun-angle 0.25 --camera-pos "140,28,155" --camera-target "110,12,120" --output $(SNAPSHOT_DIR)/noon.png
	$(SNAPSHOT_BIN) --sun-angle 0.47 --camera-pos "160,25,128" --camera-target "100,12,140" --output $(SNAPSHOT_DIR)/dusk.png
	$(SNAPSHOT_BIN) --sun-angle 0.75 --output $(SNAPSHOT_DIR)/night.png
	@echo "Snapshots saved to $(SNAPSHOT_DIR)/"
	@ls -lh $(SNAPSHOT_DIR)/*.png

client:
	cd game-client && wasm-pack build --target web --out-dir ../web/pkg

server:
	cargo build --release -p game-server

dev: client server
	cargo run --release -p game-server
