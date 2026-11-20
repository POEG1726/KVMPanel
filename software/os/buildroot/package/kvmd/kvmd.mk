################################################################################
#
# kvmd
#
################################################################################

KVMD_VERSION = 0.1.0
KVMD_SITE = $(BR2_EXTERNAL_KVMPANEL_PATH)/../../app/ipkvm
KVMD_SITE_METHOD = local
KVMD_DEPENDENCIES = dropbear jpeg-turbo libgpiod libv4l mdnsd
KVMD_OVERRIDE_SRCDIR_RSYNC_EXCLUSIONS = --exclude /target/ --exclude /kvmd-sim/

# The Buildroot 2025.02 Cargo is older than kvmd's dependencies. Use
# user-installed stable Rust with the same target C compiler as the rootfs.
KVMD_CARGO ?= $(HOME)/.cargo/bin/cargo
KVMD_RUSTC ?= $(dir $(KVMD_CARGO))rustc
KVMD_RUST_TARGET = armv7-unknown-linux-musleabihf
KVMD_CARGO_ENV = \
	CARGO_TARGET_DIR="$(@D)/target" \
	KVMD_API_DIR="$(abspath $(BR2_EXTERNAL_KVMPANEL_PATH)/../../app/api)" \
	CARGO_TARGET_ARMV7_UNKNOWN_LINUX_MUSLEABIHF_LINKER="$(TARGET_CC)" \
	CARGO_TARGET_ARMV7_UNKNOWN_LINUX_MUSLEABIHF_RUSTFLAGS="-C target-cpu=cortex-a7 -C link-self-contained=no -L native=$(STAGING_DIR)/usr/lib -L native=$(@D)/rust-unwind" \
	CC_armv7_unknown_linux_musleabihf="$(TARGET_CC)" \
	AR_armv7_unknown_linux_musleabihf="$(TARGET_AR)"

# Keep Bootlin's musl and CRT while using libstd's matching unwinder.
define KVMD_BUILD_CMDS
	mkdir -p $(@D)/rust-unwind
	ln -sf "$$($(KVMD_RUSTC) --print target-libdir --target $(KVMD_RUST_TARGET))/self-contained/libunwind.a" $(@D)/rust-unwind/
	cd $(@D) && $(TARGET_MAKE_ENV) $(KVMD_CARGO_ENV) \
		$(KVMD_CARGO) build --locked --release --target $(KVMD_RUST_TARGET) --bin kvmd
endef

define KVMD_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m 0755 $(@D)/target/$(KVMD_RUST_TARGET)/release/kvmd $(TARGET_DIR)/usr/sbin/kvmd
endef

define KVMD_INSTALL_INIT_SYSV
	$(INSTALL) -D -m 0755 $(KVMD_PKGDIR)/S95kvmd $(TARGET_DIR)/etc/init.d/S95kvmd
endef

# kvmd starts SSH and mDNS after reading the persistent configuration.
# Move the stock service scripts out of BusyBox rcS's boot-time glob.
ifeq ($(BR2_PACKAGE_KVMD),y)
define KVMD_INSTALL_MANAGED_SERVICES
	mkdir -p $(TARGET_DIR)/usr/libexec/kvmd
	for service in dropbear mdnsd; do \
		init=$(TARGET_DIR)/etc/init.d/S50$$service; \
		if test -f "$$init"; then mv "$$init" "$(TARGET_DIR)/usr/libexec/kvmd/$$service"; fi; \
	done
	ln -snf /data/kvmd/ssh $(TARGET_DIR)/etc/dropbear
endef
TARGET_FINALIZE_HOOKS += KVMD_INSTALL_MANAGED_SERVICES
endif

$(eval $(generic-package))
