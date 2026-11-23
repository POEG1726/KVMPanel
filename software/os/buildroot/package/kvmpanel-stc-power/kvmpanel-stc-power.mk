KVMPANEL_STC_POWER_VERSION = 5151cba878b99024a8b4080d9a115ff03f09b74b
KVMPANEL_STC_POWER_SITE = $(call github,IOsetting,stc8prog,$(KVMPANEL_STC_POWER_VERSION))
KVMPANEL_STC_POWER_LICENSE = Apache-2.0
KVMPANEL_STC_POWER_LICENSE_FILES = LICENSE

KVMPANEL_STC_POWER_LOCAL = $(BR2_EXTERNAL_KVMPANEL_PATH)/package/kvmpanel-stc-power
KVMPANEL_STC_POWER_PROTOCOL = $(BR2_EXTERNAL_KVMPANEL_PATH)/../../firmware/stc8

define KVMPANEL_STC_POWER_BUILD_CMDS
	cd $(@D) && $(TARGET_CC) $(TARGET_CFLAGS) -Wall -Wextra -Werror \
		-I$(KVMPANEL_STC_POWER_PROTOCOL) -I$(@D)/src \
		$(KVMPANEL_STC_POWER_LOCAL)/main.c $(KVMPANEL_STC_POWER_LOCAL)/serial.c \
		$(KVMPANEL_STC_POWER_LOCAL)/update.c $(KVMPANEL_STC_POWER_LOCAL)/isp.c \
		-c
	cd $(@D) && $(TARGET_CC) $(TARGET_CFLAGS) $(TARGET_LDFLAGS) \
		$(@D)/src/stc8prog.c $(@D)/src/stc8db.c \
		main.o serial.o update.o isp.o -o $(@D)/kvmpanel-stc-power
endef

define KVMPANEL_STC_POWER_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m 0755 $(@D)/kvmpanel-stc-power $(TARGET_DIR)/usr/sbin/kvmpanel-stc-power
endef

$(eval $(generic-package))
