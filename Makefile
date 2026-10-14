PREFIX ?= /usr/local
BINDIR ?= $(PREFIX)/bin

all: build

build:
	cargo build --release

install: build
	install -d $(DESTDIR)$(BINDIR)
	install -m 755 target/release/mopet $(DESTDIR)$(BINDIR)/mopet
	@echo "MOPET instalado para todos los usuarios en $(DESTDIR)$(BINDIR)/mopet"

install-user: build
	mkdir -p $(HOME)/.local/bin $(HOME)/.cargo/bin
	install -m 755 target/release/mopet $(HOME)/.local/bin/mopet
	install -m 755 target/release/mopet $(HOME)/.cargo/bin/mopet
	@echo "MOPET instalado para el usuario actual en $(HOME)/.local/bin/mopet"

uninstall:
	rm -f $(DESTDIR)$(BINDIR)/mopet
	rm -f $(HOME)/.local/bin/mopet
	rm -f $(HOME)/.cargo/bin/mopet
	@echo "MOPET desinstalado."

.PHONY: all build install install-user uninstall

