.PHONY: help run build debug app examples test fmt lint clean check-zigbuild \
	dist-cross dist-win-amd64 dist-win-arm64 dist-linux-amd64 dist-linux-arm64

help:
	@echo "Strungin — available targets:"
	@echo "  run         cargo run (opens the app window)"
	@echo "  build       cargo build --release (fully optimised), copy the binary into dist/"
	@echo "  debug       cargo build (dev profile) — fastest compile, no optimisation"
	@echo "  app         bundle dist/Strungin.app (Info.plist + icon + binary) — macOS only"
	@echo "  examples    regenerate exemples/*.pdf via --export"
	@echo "  dist-cross  cross-build Windows + Linux, amd64 + arm64, into dist/<platform>/"
	@echo "  test        cargo test"
	@echo "  fmt         cargo fmt"
	@echo "  lint        cargo clippy --all-targets -- -D warnings"
	@echo "  clean       cargo clean && rm -rf dist/* build"

run:
	cargo run

debug:
	cargo build

build:
	cargo build --release
	mkdir -p dist
	cp target/release/strungin dist/

# Bundles the release binary as a double-clickable macOS .app, with an Info.plist
# declaring the .gtab document type and the hand-made image.icns from the repo root.
# macOS-only: fails with a clear message on any other host rather than producing a
# broken bundle.
app: build
ifneq ($(shell uname -s),Darwin)
	$(error make app only builds a macOS .app bundle; run this on macOS)
endif
	@echo "Bundling dist/Strungin.app..."
	rm -rf dist/Strungin.app
	mkdir -p dist/Strungin.app/Contents/MacOS dist/Strungin.app/Contents/Resources
	cp dist/strungin dist/Strungin.app/Contents/MacOS/strungin
	cp image.icns dist/Strungin.app/Contents/Resources/Strungin.icns
	printf '<?xml version="1.0" encoding="UTF-8"?>\n<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">\n<plist version="1.0">\n<dict>\n<key>CFBundleName</key>\n<string>Strungin</string>\n<key>CFBundleDisplayName</key>\n<string>Strungin</string>\n<key>CFBundleIdentifier</key>\n<string>com.nicoolaj.strungin</string>\n<key>CFBundleExecutable</key>\n<string>strungin</string>\n<key>CFBundleIconFile</key>\n<string>Strungin</string>\n<key>CFBundlePackageType</key>\n<string>APPL</string>\n<key>CFBundleShortVersionString</key>\n<string>0.3.0</string>\n<key>CFBundleVersion</key>\n<string>1</string>\n<key>NSHighResolutionCapable</key>\n<true/>\n<key>CFBundleDocumentTypes</key>\n<array>\n<dict>\n<key>CFBundleTypeName</key>\n<string>Strungin Tablature</string>\n<key>CFBundleTypeExtensions</key>\n<array><string>gtab</string></array>\n<key>CFBundleTypeRole</key>\n<string>Editor</string>\n<key>LSHandlerRank</key>\n<string>Owner</string>\n</dict>\n</array>\n</dict>\n</plist>\n' > dist/Strungin.app/Contents/Info.plist
	@echo "Bundle ready: dist/Strungin.app"

# --- Cross-compilation: Windows + Linux, amd64 + arm64 ---------------------
# One-time setup on the build host:
#     cargo install cargo-zigbuild && brew install zig   (or: apt/pkg install zig)
# zig is the cross-linker and bundles libc + mingw-w64 headers for all four
# targets, so no target sysroot is needed. eframe's glow (OpenGL) renderer and
# winit both dlopen their system libs at runtime, so the Linux builds need no
# X11/Wayland dev packages at build time either.
CROSS_win-amd64   := x86_64-pc-windows-gnu
CROSS_win-arm64   := aarch64-pc-windows-gnullvm
CROSS_linux-amd64 := x86_64-unknown-linux-gnu
CROSS_linux-arm64 := aarch64-unknown-linux-gnu

dist-cross: dist-win-amd64 dist-win-arm64 dist-linux-amd64 dist-linux-arm64
	@echo "Cross builds ready under dist/"

check-zigbuild:
	@cargo zigbuild --help >/dev/null 2>&1 || { echo "cargo-zigbuild missing — run: cargo install cargo-zigbuild"; exit 1; }
	@command -v zig >/dev/null 2>&1 || { echo "zig missing — run: brew install zig (or apt/pkg install zig)"; exit 1; }

dist-win-amd64 dist-win-arm64 dist-linux-amd64 dist-linux-arm64: dist-%: check-zigbuild
	rustup target add $(CROSS_$*)
	cargo zigbuild --release --target $(CROSS_$*)
	mkdir -p dist/$*
	cp target/$(CROSS_$*)/release/strungin$(if $(findstring win,$*),.exe,) dist/$*/
	@echo "Built dist/$*/"

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
