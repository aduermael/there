.PHONY: snapshot snapshot-build client server

SNAPSHOT_BIN = cargo run --release -p game-snapshot --
SNAPSHOT_DIR = snapshots

snapshot-build:
	cargo build --release -p game-snapshot

snapshot: snapshot-build
	@mkdir -p $(SNAPSHOT_DIR)
	$(SNAPSHOT_BIN) --sun-angle 0.00 --output $(SNAPSHOT_DIR)/dawn.png
	$(SNAPSHOT_BIN) --sun-angle 0.25 --output $(SNAPSHOT_DIR)/noon.png
	$(SNAPSHOT_BIN) --sun-angle 0.50 --output $(SNAPSHOT_DIR)/dusk.png
	$(SNAPSHOT_BIN) --sun-angle 0.75 --output $(SNAPSHOT_DIR)/night.png
	@echo "Snapshots saved to $(SNAPSHOT_DIR)/"
	@ls -lh $(SNAPSHOT_DIR)/*.png

client:
	cd game-client && wasm-pack build --target web --out-dir ../web/pkg

server:
	cargo build --release -p game-server
