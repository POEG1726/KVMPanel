SDCC ?= sdcc
MAKEBIN ?= makebin
HOSTCC ?= cc
OUT ?= ../../../build/stc8
BOARD_CPPFLAGS ?=

MCU_FLAGS := -mmcs51 --model-small --stack-auto --iram-size 256 --xram-size 1024 --opt-code-size
OBJECTS := $(addprefix $(OUT)/,stc8g1k08a.rel power.rel journal.rel)

.PHONY: all test FORCE
all: $(OUT)/kvmpanel-stc8.ihx $(OUT)/kvmpanel-stc8.bin

$(OUT):
	mkdir -p $@

$(OUT)/board-flags: FORCE | $(OUT)
	@printf '%s\n' "$(MCU_FLAGS) $(BOARD_CPPFLAGS)" > $@.new
	@cmp -s $@.new $@ || cp $@.new $@
	@rm $@.new

$(OUT)/%.rel: %.c board.h protocol.h power.h journal.h $(OUT)/board-flags
	$(SDCC) $(MCU_FLAGS) $(BOARD_CPPFLAGS) -c $< -o $@

$(OUT)/kvmpanel-stc8.ihx: $(OBJECTS)
	$(SDCC) $(MCU_FLAGS) --code-size 7936 $(OBJECTS) -o $@

$(OUT)/kvmpanel-stc8.bin: $(OUT)/kvmpanel-stc8.ihx
	$(MAKEBIN) -p -s 8192 $< $@

$(OUT)/power-test: test_power.c power.c journal.c board.h protocol.h power.h journal.h | $(OUT)
	$(HOSTCC) -std=c11 -Wall -Wextra -Werror -O2 \
		-DKEY_BOTH_LOW=230 -DKEY_BOTH_HIGH=300 -DKEY_IDLE_LOW=770 \
		-DKEY_IDLE_HIGH=850 -DKEY_HYSTERESIS=10 \
		test_power.c power.c journal.c -o $@

test: $(OUT)/power-test
	$(OUT)/power-test
