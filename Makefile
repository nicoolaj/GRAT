.PHONY: help run build app examples test fmt lint clean

help:
	@echo "Strungin — available targets:"
	@echo "  run       cargo run (opens the app window)"
	@echo "  build     cargo build --release, copy the binary into dist/"
	@echo "  app       bundle dist/Strungin.app (Info.plist + binary) — macOS only"
	@echo "  examples  regenerate exemples/*.pdf via --export"
	@echo "  test      cargo test"
	@echo "  fmt       cargo fmt"
	@echo "  lint      cargo clippy --all-targets -- -D warnings"
	@echo "  clean     cargo clean && rm -rf dist/*"

run:
	cargo run

build:
	cargo build --release
	mkdir -p dist
	cp target/release/strungin dist/

# Bundles the release binary as a double-clickable macOS .app, with an Info.plist
# declaring the .gtab document type. Doesn't depend on the PDF backend (phase 6),
# so it's implemented for real now rather than stubbed.
app: build
	@echo "Bundling dist/Strungin.app..."
	rm -rf dist/Strungin.app
	mkdir -p dist/Strungin.app/Contents/MacOS
	cp dist/strungin dist/Strungin.app/Contents/MacOS/strungin
	printf '<?xml version="1.0" encoding="UTF-8"?>\n<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">\n<plist version="1.0">\n<dict>\n<key>CFBundleName</key>\n<string>Strungin</string>\n<key>CFBundleDisplayName</key>\n<string>Strungin</string>\n<key>CFBundleIdentifier</key>\n<string>com.nicoolaj.strungin</string>\n<key>CFBundleExecutable</key>\n<string>strungin</string>\n<key>CFBundlePackageType</key>\n<string>APPL</string>\n<key>CFBundleShortVersionString</key>\n<string>0.1.0</string>\n<key>CFBundleVersion</key>\n<string>1</string>\n<key>NSHighResolutionCapable</key>\n<true/>\n<key>CFBundleDocumentTypes</key>\n<array>\n<dict>\n<key>CFBundleTypeName</key>\n<string>Strungin Tablature</string>\n<key>CFBundleTypeExtensions</key>\n<array><string>gtab</string></array>\n<key>CFBundleTypeRole</key>\n<string>Editor</string>\n<key>LSHandlerRank</key>\n<string>Owner</string>\n</dict>\n</array>\n</dict>\n</plist>\n' > dist/Strungin.app/Contents/Info.plist
	@echo "Bundle ready: dist/Strungin.app"

# Needs the PDF backend (phase 6) to actually produce PDFs; stubbed until then.
examples:
	@echo "make examples needs the PDF backend (phase 6) — nothing to regenerate yet."

test:
	cargo test

fmt:
	cargo fmt

lint:
	cargo clippy --all-targets -- -D warnings

clean:
	cargo clean
	rm -rf dist/*
