TARGET := wasm32-unknown-unknown
PORT   ?= 8080

.PHONY: build serve desktop meter test clean

# Browser build. Needs:
#   cargo install wasm-bindgen-cli --version 0.2.129   (same as crates/app/Cargo.toml)
build:
	cargo build --release --target $(TARGET) -p notely-app
	wasm-bindgen target/$(TARGET)/release/notely.wasm --target web --no-typescript --remove-name-section --remove-producers-section --out-dir www/pkg
	@if command -v wasm-opt >/dev/null; then wasm-opt -Os www/pkg/notely_bg.wasm -o www/pkg/notely_bg.wasm; fi
	@ls -l www/pkg/notely_bg.wasm | awk '{printf "wasm: %.0f KB\n", $$5 / 1024}'

serve: build
	python3 -m http.server $(PORT) --directory www

# Native desktop app. Stores its document in the user data directory.
desktop:
	cargo run --release -p notely-app

# Scroll meter: the desktop app on a sample document with a speed panel.
# It logs the gesture to target/scroll-desktop.log. Open
# tools/scroll-meter.html in a browser for the same panel there.
meter:
	NOTELY_SCROLL_METER=target/scroll-desktop.log cargo run --release -p notely-app

test:
	cargo test --workspace

clean:
	cargo clean
	rm -rf www/pkg
