DESTDIR ?=
IBUS_INSTALL_DIR ?= /usr/share
SHINRAN_DIR = $(DESTDIR)$(IBUS_INSTALL_DIR)/ibus-shinran
IBUS_COMPONENT_DIR = $(DESTDIR)$(IBUS_INSTALL_DIR)/ibus/component

all: build shinran.xml

build:
	cargo build --bin shinran_ibus --release

shinran.xml: shinran.xml.in
	sed 's:$$(SHINRAN_DIR):$(SHINRAN_DIR):g' shinran.xml.in > shinran.xml

install:
	mkdir -p '$(SHINRAN_DIR)'
	mkdir -p '$(IBUS_COMPONENT_DIR)'
	cp target/release/shinran_ibus '$(SHINRAN_DIR)/'
	cp shinran.xml '$(IBUS_COMPONENT_DIR)/'
