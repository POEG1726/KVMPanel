################################################################################
#
# kvmpanel-amp
#
################################################################################

KVMPANEL_AMP_VERSION = 1
KVMPANEL_AMP_SITE = $(BR2_EXTERNAL_KVMPANEL_PATH)/../../firmware/t113-amp
KVMPANEL_AMP_SITE_METHOD = local
KVMPANEL_AMP_LICENSE = GPL-2.0

RISCV_CROSS_COMPILE ?= riscv64-unknown-elf-
XTENSA_CROSS_COMPILE ?= xtensa-hifi4-elf-

define KVMPANEL_AMP_BUILD_CMDS
	$(MAKE) -C $(@D) O=output RISCV_CROSS_COMPILE="$(RISCV_CROSS_COMPILE)" \
		XTENSA_CROSS_COMPILE="$(XTENSA_CROSS_COMPILE)"
	$(TARGET_CC) $(TARGET_CFLAGS) $(TARGET_LDFLAGS) -Wall -Wextra -Werror \
		-o $(@D)/output/kvmpanel-amp-ping $(@D)/amp-ping.c
endef

define KVMPANEL_AMP_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m 0644 $(@D)/output/c906-echo.elf $(TARGET_DIR)/lib/firmware/kvmpanel/c906-echo.elf
	$(INSTALL) -D -m 0644 $(@D)/output/hifi4-echo.elf $(TARGET_DIR)/lib/firmware/kvmpanel/hifi4-echo.elf
	$(INSTALL) -D -m 0755 $(@D)/output/kvmpanel-amp-ping $(TARGET_DIR)/usr/bin/kvmpanel-amp-ping
endef

$(eval $(generic-package))
