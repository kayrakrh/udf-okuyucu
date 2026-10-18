PREFIX ?= /usr
DESTDIR ?=

all:
	cargo build --release

# Önce `make`, sonra `sudo make install` (sudo ile derleme yapılmasın)
install:
	install -Dm755 target/release/udf-okuyucu $(DESTDIR)$(PREFIX)/bin/udf-okuyucu
	install -Dm644 data/udf-okuyucu.desktop $(DESTDIR)$(PREFIX)/share/applications/udf-okuyucu.desktop
	install -Dm644 data/udf-okuyucu-mime.xml $(DESTDIR)$(PREFIX)/share/mime/packages/udf-okuyucu.xml
	install -Dm644 data/udf-okuyucu.svg $(DESTDIR)$(PREFIX)/share/icons/hicolor/scalable/apps/udf-okuyucu.svg
	@if [ -z "$(DESTDIR)" ]; then \
		update-mime-database $(PREFIX)/share/mime || true; \
		update-desktop-database $(PREFIX)/share/applications || true; \
		gtk-update-icon-cache -q -t $(PREFIX)/share/icons/hicolor || true; \
	fi

uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/udf-okuyucu
	rm -f $(DESTDIR)$(PREFIX)/share/applications/udf-okuyucu.desktop
	rm -f $(DESTDIR)$(PREFIX)/share/mime/packages/udf-okuyucu.xml
	rm -f $(DESTDIR)$(PREFIX)/share/icons/hicolor/scalable/apps/udf-okuyucu.svg
	@if [ -z "$(DESTDIR)" ]; then \
		update-mime-database $(PREFIX)/share/mime || true; \
		update-desktop-database $(PREFIX)/share/applications || true; \
	fi

.PHONY: all install uninstall
