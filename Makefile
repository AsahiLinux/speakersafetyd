# SPDX-Licence-Identifier: MIT
# Copyright The Asahi Linux Contributors

DESTDIR ?=
BINDIR ?= /usr/bin
UNITDIR ?= /lib/systemd/system
UDEVDIR ?= /lib/udev/rules.d
TMPFILESDIR ?= /usr/lib/tmpfiles.d
SHAREDIR ?= /usr/share/
SYSUSERSDIR ?= /lib/sysusers.d
VARDIR ?= /var/
SPEAKERSAFETYD_GROUP ?= speakersafetyd
SPEAKERSAFETYD_USER ?= speakersafetyd
INSTALL_USER_GROUP ?= -o $(SPEAKERSAFETYD_USER) -g $(SPEAKERSAFETYD_GROUP)

all:
	cargo build --release

install: install-data
	install -dDm0755 $(DESTDIR)$(BINDIR)
	install -pm0755 target/release/speakersafetyd $(DESTDIR)$(BINDIR)/speakersafetyd

install-data:
	install -dDm0755 $(DESTDIR)$(UNITDIR)
	install -pm0644 speakersafetyd.service $(DESTDIR)$(UNITDIR)/speakersafetyd.service
	install -dDm0755 $(DESTDIR)$(UDEVDIR)
	install -pm0644 95-speakersafetyd.rules $(DESTDIR)$(UDEVDIR)/95-speakersafetyd.rules
	install -dDm0755 $(DESTDIR)$(SHAREDIR)speakersafetyd/apple
	install -pm0644 -t $(DESTDIR)$(SHAREDIR)speakersafetyd/apple $(wildcard conf/apple/*)
	install -dDm0755 $(INSTALL_USER_GROUP) $(DESTDIR)$(VARDIR)/lib/speakersafetyd
	install -dDm0700 $(INSTALL_USER_GROUP) $(DESTDIR)$(VARDIR)/lib/speakersafetyd/blackbox
	install -dDm0755 $(DESTDIR)$(SYSUSERSDIR)
	install -pm0644 speakersafetyd.sysusers $(DESTDIR)$(SYSUSERSDIR)/speakersafetyd.conf
	install -dDm0755 $(DESTDIR)$(TMPFILESDIR)
	install -pm0644 speakersafetyd.tmpfiles $(DESTDIR)$(TMPFILESDIR)/speakersafetyd.conf
	install -dDm0755 $(INSTALL_USER_GROUP) $(DESTDIR)/run/speakersafetyd

uninstall:
	rm -f $(DESTDIR)$(BINDIR)/speakersafetyd $(DESTDIR)$(UNITDIR)/speakersafetyd.service $(DESTDIR)$(UDEVDIR)/95-speakersafetyd.rules $(DESTDIR)$(TMPFILESDIR)/speakersafetyd.conf
	rm -rf $(DESTDIR)$(SHAREDIR)/speakersafetyd

.PHONY: all install install-data uninstall
