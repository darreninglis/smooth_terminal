APP_NAME   := Smooth Terminal
BINARY     := smooth_terminal
BUNDLE     := $(APP_NAME).app
BUILD_DIR  := target/release
PLIST      := macos/Info.plist
ICON       := macos/AppIcon.icns

# Full path to cargo — avoids PATH issues when make is invoked from
# environments that haven't sourced .zprofile (e.g. scripts, CI).
CARGO := $(HOME)/.cargo/bin/cargo

.PHONY: all bundle install icon clean run test

# Default: build the .app bundle
all: bundle

# ── Build the release binary ──────────────────────────────────────────────────
build:
	$(CARGO) build --release

# ── Generate the app icon ─────────────────────────────────────────────────────
icon:
	@echo "==> Generating app icon…"
	bash macos/create_icon.sh

# ── Assemble the .app bundle ──────────────────────────────────────────────────
bundle: build
	@echo "==> Assembling $(BUNDLE)…"

	@# Create bundle directory structure
	mkdir -p "$(BUNDLE)/Contents/MacOS"
	mkdir -p "$(BUNDLE)/Contents/Resources"

	@# Copy release binary
	cp "$(BUILD_DIR)/$(BINARY)" "$(BUNDLE)/Contents/MacOS/$(BINARY)"

	@# Copy Info.plist
	cp "$(PLIST)" "$(BUNDLE)/Contents/Info.plist"

	@# Copy icon if present (run 'make icon' first to generate it)
	@if [ -f "$(ICON)" ]; then \
		cp "$(ICON)" "$(BUNDLE)/Contents/Resources/AppIcon.icns"; \
		echo "   Copied icon."; \
	else \
		echo "   No icon found — run 'make icon' to generate one."; \
	fi

	@# Ad-hoc code sign so Gatekeeper and the dynamic linker are satisfied
	@# Replace '-' with your Developer ID for a distributable build
	codesign --force --deep --sign - "$(BUNDLE)"

	@echo ""
	@echo "✓ Bundle ready: $(BUNDLE)"
	@echo "  Launch with:  open \"$(BUNDLE)\""
	@echo "  Or:           make install"

# ── Install to /Applications ──────────────────────────────────────────────────
install: bundle
	@echo "==> Installing to /Applications/…"
	cp -r "$(BUNDLE)" "/Applications/$(BUNDLE)"
	@echo "✓ Installed: /Applications/$(BUNDLE)"

# ── Run tests ─────────────────────────────────────────────────────────────────
test:
	$(CARGO) test

# ── Quick dev run (no bundle, just cargo run) ─────────────────────────────────
run:
	$(CARGO) run

# ── Clean everything ──────────────────────────────────────────────────────────
clean:
	$(CARGO) clean
	rm -rf "$(BUNDLE)"
	@echo "✓ Cleaned."
