# SPDX-Licence-Identifier: MIT
# Copyright The Asahi Linux Contributors

BINDIR ?= /usr/bin
UNITDIR ?= /lib/systemd/system
UDEVDIR ?= /lib/udev/rules.d
SHAREDIR ?= /usr/share/
VARDIR ?= /var/

all:
	cargo build --release

install:
	install -dDm0755 $(DESTDIR)/$(BINDIR)
	install -pm0755 target/release/speakersafetyd $(DESTDIR)/$(BINDIR)/speakersafetyd
	install -dDm0755 $(DESTDIR)/$(UNITDIR)
	install -pm0644 speakersafetyd.service $(DESTDIR)/$(UNITDIR)/speakersafetyd.service
	install -dDm0755 $(DESTDIR)/$(UDEVDIR)
	install -pm0644 95-speakersafetyd.rules $(DESTDIR)/$(UDEVDIR)/95-speakersafetyd.rules
	install -dDm0755 $(DESTDIR)/$(SHAREDIR)/speakersafetyd/apple
	install -pm0644 -t $(DESTDIR)/$(SHAREDIR)/speakersafetyd/apple $(wildcard conf/apple/*)
	install -dDm0755 $(DESTDIR)/$(VARDIR)/lib/speakersafetyd/blackbox

uninstall:
	rm -f $(DESTDIR)/$(BINDIR)/speakersafetyd $(DESTDIR)/$(UNITDIR)/speakersafetyd.service $(DESTDIR)/$(UDEVDIR)/95-speakersafetyd.rules
	rm -rf $(DESTDIR)/$(SHAREDIR)/speakersafetyd
