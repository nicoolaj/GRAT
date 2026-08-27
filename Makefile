.PHONY: help run build app examples test fmt lint clean

help:
	@echo "Strungin — available targets:"
	@echo "  run       cargo run (opens the app window)"
	@echo "  build     cargo build --release, copy the binary into dist/"
	@echo "  app       bundle dist/Strungin.app (Info.plist + icon + binary) — macOS only"
	@echo "  examples  regenerate exemples/*.pdf via --export"
	@echo "  test      cargo test"
	@echo "  fmt       cargo fmt"
	@echo "  lint      cargo clippy --all-targets -- -D warnings"
	@echo "  clean     cargo clean && rm -rf dist/* build"

run:
	cargo run

build:
	cargo build --release
	mkdir -p dist
	cp target/release/strungin dist/

# Bundles the release binary as a double-clickable macOS .app, with an Info.plist
# declaring the .gtab document type and a real .icns built from the cover artwork
# (sips + iconutil, both ship with macOS -- no extra install needed). macOS-only:
# fails with a clear message on any other host rather than producing a broken bundle.
app: build
ifneq ($(shell uname -s),Darwin)
	$(error make app only builds a macOS .app bundle (needs sips/iconutil); run this on macOS)
endif
	@echo "Bundling dist/Strungin.app..."
	rm -rf dist/Strungin.app build/icon.iconset
	mkdir -p dist/Strungin.app/Contents/MacOS dist/Strungin.app/Contents/Resources build/icon.iconset
	cp dist/strungin dist/Strungin.app/Contents/MacOS/strungin
	for N in 16 32 128 256 512; do \
		sips -s format png -z $$N $$N src/assets/cover.jpg --out build/icon.iconset/icon_$${N}x$${N}.png >/dev/null; \
		sips -s format png -z $$((N*2)) $$((N*2)) src/assets/cover.jpg --out build/icon.iconset/icon_$${N}x$${N}@2x.png >/dev/null; \
	done
	iconutil -c icns build/icon.iconset -o dist/Strungin.app/Contents/Resources/Strungin.icns
	printf '<?xml version="1.0" encoding="UTF-8"?>\n<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">\n<plist version="1.0">\n<dict>\n<key>CFBundleName</key>\n<string>Strungin</string>\n<key>CFBundleDisplayName</key>\n<string>Strungin</string>\n<key>CFBundleIdentifier</key>\n<string>com.nicoolaj.strungin</string>\n<key>CFBundleExecutable</key>\n<string>strungin</string>\n<key>CFBundleIconFile</key>\n<string>Strungin</string>\n<key>CFBundlePackageType</key>\n<string>APPL</string>\n<key>CFBundleShortVersionString</key>\n<string>0.1.0</string>\n<key>CFBundleVersion</key>\n<string>1</string>\n<key>NSHighResolutionCapable</key>\n<true/>\n<key>CFBundleDocumentTypes</key>\n<array>\n<dict>\n<key>CFBundleTypeName</key>\n<string>Strungin Tablature</string>\n<key>CFBundleTypeExtensions</key>\n<array><string>gtab</string></array>\n<key>CFBundleTypeRole</key>\n<string>Editor</string>\n<key>LSHandlerRank</key>\n<string>Owner</string>\n</dict>\n</array>\n</dict>\n</plist>\n' > dist/Strungin.app/Contents/Info.plist
	@echo "Bundle ready: dist/Strungin.app"

# Regenerates exemples/*.pdf from exemples/*.gtab via the --export CLI -- no window.
examples: build
	dist/strungin --export exemples/legato-study.gtab exemples/legato-study.pdf
	dist/strungin --export exemples/slow-bend.gtab exemples/slow-bend.pdf
	dist/strungin --export exemples/muted-drive.gtab exemples/muted-drive.pdf
	@echo "Regenerated exemples/*.pdf"

test:
	cargo test

fmt:
	cargo fmt

lint:
	cargo clippy --all-targets -- -D warnings

clean:
	cargo clean
	rm -rf dist/* build
