PREFIX ?= $(HOME)/.local
APP_ID = io.github.vlucas14sp.Tucano

.PHONY: build install uninstall

build:
	cargo build --release

install: build
	install -Dm755 target/release/tucano $(PREFIX)/bin/tucano
	install -Dm644 data/$(APP_ID).desktop $(PREFIX)/share/applications/$(APP_ID).desktop
	install -Dm644 data/$(APP_ID).svg $(PREFIX)/share/icons/hicolor/scalable/apps/$(APP_ID).svg
	sed -i 's|^Exec=tucano|Exec=$(PREFIX)/bin/tucano|' $(PREFIX)/share/applications/$(APP_ID).desktop
	gtk4-update-icon-cache -q -t $(PREFIX)/share/icons/hicolor 2>/dev/null || true
	update-desktop-database -q $(PREFIX)/share/applications 2>/dev/null || true
	@echo "Tucano instalado em $(PREFIX). Pode ser que precise relogar para o ícone aparecer."

uninstall:
	rm -f $(PREFIX)/bin/tucano
	rm -f $(PREFIX)/share/applications/$(APP_ID).desktop
	rm -f $(PREFIX)/share/icons/hicolor/scalable/apps/$(APP_ID).svg
	gtk4-update-icon-cache -q -t $(PREFIX)/share/icons/hicolor 2>/dev/null || true
	update-desktop-database -q $(PREFIX)/share/applications 2>/dev/null || true
