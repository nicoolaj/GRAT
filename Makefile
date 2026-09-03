.PHONY: help run build debug app logo-assets examples test fmt lint clean check-zigbuild \
	dist-cross dist-win-amd64 dist-win-arm64 dist-linux-amd64 dist-linux-arm64

help:
	@echo "GRAT — available targets:"
	@echo "  run         cargo run (opens the app window)"
	@echo "  build       cargo build --release (fully optimised), copy the binary into dist/"
	@echo "  debug       cargo build (dev profile) — fastest compile, no optimisation"
	@echo "  app         bundle dist/GRAT.app (Info.plist + icon + binary) — macOS only"
	@echo "  logo-assets regenerate image.icns + src/assets/logo.png from logo.svg — macOS only"
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
	cp target/release/grat dist/

# Bundles the release binary as a double-clickable macOS .app, with an Info.plist
# declaring the .gtab document type and image.icns from the repo root (regenerated
# by `make logo-assets`). The document type points CFBundleTypeIconFile at the same
# icon, so .gtab files show the logo in Finder. macOS-only: fails with a clear
# message on any other host rather than producing a broken bundle.
app: build
ifneq ($(shell uname -s),Darwin)
	$(error make app only builds a macOS .app bundle; run this on macOS)
endif
	$(eval VERSION := $(shell grep -m1 '^version = ' Cargo.toml | cut -d'"' -f2))
	@echo "Bundling dist/GRAT.app..."
	rm -rf dist/GRAT.app
	mkdir -p dist/GRAT.app/Contents/MacOS dist/GRAT.app/Contents/Resources
	cp dist/grat dist/GRAT.app/Contents/MacOS/grat
	cp image.icns dist/GRAT.app/Contents/Resources/GRAT.icns
	printf '<?xml version="1.0" encoding="UTF-8"?>\n<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">\n<plist version="1.0">\n<dict>\n<key>CFBundleName</key>\n<string>GRAT</string>\n<key>CFBundleDisplayName</key>\n<string>GRAT</string>\n<key>CFBundleIdentifier</key>\n<string>com.nicoolaj.grat</string>\n<key>CFBundleExecutable</key>\n<string>grat</string>\n<key>CFBundleIconFile</key>\n<string>GRAT</string>\n<key>CFBundlePackageType</key>\n<string>APPL</string>\n<key>CFBundleShortVersionString</key>\n<string>$(VERSION)</string>\n<key>CFBundleVersion</key>\n<string>$(VERSION)</string>\n<key>NSHighResolutionCapable</key>\n<true/>\n<key>CFBundleDocumentTypes</key>\n<array>\n<dict>\n<key>CFBundleTypeName</key>\n<string>GRAT Tablature</string>\n<key>CFBundleTypeIconFile</key>\n<string>GRAT</string>\n<key>CFBundleTypeExtensions</key>\n<array><string>gtab</string></array>\n<key>CFBundleTypeRole</key>\n<string>Editor</string>\n<key>LSHandlerRank</key>\n<string>Owner</string>\n</dict>\n</array>\n</dict>\n</plist>\n' > dist/GRAT.app/Contents/Info.plist
	@echo "Bundle ready: dist/GRAT.app"

# Regenerates image.icns (the .app / .gtab icon) and src/assets/logo.png (the
# window icon + splash texture, include_bytes!'d into the binary) from logo.svg.
# Uses QuickLook to rasterise the SVG, so it is macOS-only; run it after editing
# the logo, then commit both outputs.
logo-assets:
ifneq ($(shell uname -s),Darwin)
	$(error make logo-assets needs macOS QuickLook to rasterise logo.svg)
endif
	@tmp=$$(mktemp -d); \
	qlmanage -t -s 1024 -o "$$tmp" logo.svg >/dev/null 2>&1; \
	sips -z 512 512 "$$tmp/logo.svg.png" --out src/assets/logo.png >/dev/null; \
	mkdir -p "$$tmp/GRAT.iconset"; \
	for s in 16 32 128 256 512; do \
		d=$$((s * 2)); \
		sips -z $$s $$s "$$tmp/logo.svg.png" --out "$$tmp/GRAT.iconset/icon_$${s}x$${s}.png" >/dev/null; \
		sips -z $$d $$d "$$tmp/logo.svg.png" --out "$$tmp/GRAT.iconset/icon_$${s}x$${s}@2x.png" >/dev/null; \
	done; \
	iconutil -c icns "$$tmp/GRAT.iconset" -o image.icns; \
	rm -rf "$$tmp"; \
	echo "Regenerated image.icns and src/assets/logo.png from logo.svg"

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
	cp target/$(CROSS_$*)/release/grat$(if $(findstring win,$*),.exe,) dist/$*/
	@echo "Built dist/$*/"

# Regenerates exemples/*.pdf from exemples/*.gtab via the --export CLI -- no window.
examples: build
	dist/grat --export exemples/legato-study.gtab exemples/legato-study.pdf
	dist/grat --export exemples/slow-bend.gtab exemples/slow-bend.pdf
	dist/grat --export exemples/muted-drive.gtab exemples/muted-drive.pdf
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
