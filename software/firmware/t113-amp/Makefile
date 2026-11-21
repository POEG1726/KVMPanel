# SPDX-License-Identifier: GPL-2.0-only
O ?= build
RISCV_CROSS_COMPILE ?= riscv64-unknown-elf-
XTENSA_CROSS_COMPILE ?= xtensa-hifi4-elf-

COMMON_FLAGS = -Os -Wall -Wextra -Werror -ffreestanding -fno-builtin \
	-fno-stack-protector -fno-asynchronous-unwind-tables -nostdlib -static \
	-Wl,--build-id=none

.PHONY: all
all: $(O)/c906-echo.elf $(O)/hifi4-echo.elf

$(O):
	mkdir -p "$@"

$(O)/c906.ld: firmware.ld.S | $(O)
	$(RISCV_CROSS_COMPILE)gcc -E -P -x assembler-with-cpp $< -o "$@"

$(O)/hifi4.ld: firmware.ld.S | $(O)
	$(XTENSA_CROSS_COMPILE)gcc -DHIFI4 -E -P -x assembler-with-cpp $< -o "$@"

$(O)/c906-echo.elf: c906-start.S echo.c $(O)/c906.ld
	$(RISCV_CROSS_COMPILE)gcc $(COMMON_FLAGS) -march=rv64imac_zicsr -mabi=lp64 \
		-mcmodel=medany -msmall-data-limit=0 -Wl,-T,$(O)/c906.ld \
		c906-start.S echo.c -o "$@"

$(O)/hifi4-echo.elf: hifi4-start.S echo.c $(O)/hifi4.ld
	$(XTENSA_CROSS_COMPILE)gcc $(COMMON_FLAGS) -DHIFI4 -mtext-section-literals \
		-Wl,-T,$(O)/hifi4.ld hifi4-start.S echo.c -o "$@"
