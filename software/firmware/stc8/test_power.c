/*
 * test_power.c
 * Created on Nov 22, 2026
 * Author: @POEG1726
 */

#include <assert.h>
#include <stdio.h>
#include <string.h>

#include "board.h"
#include "journal.h"
#include "power.h"
#include "protocol.h"

enum { IDLE = 338, BOTH = 187, SINGLE = 235 };
static uint8_t flash[1024], levels[3], isp_calls, writes, events_used;
static uint8_t events[16];
static uint16_t now;
static uint8_t torn_erase;

void board_output(uint8_t output, uint8_t high)
{
    levels[output] = high;
    assert(events_used < sizeof(events));
    events[events_used++] = (output << 1) | high;
}

uint8_t board_flash_read(uint16_t address, uint8_t *value)
{
    assert(address < sizeof(flash));
    *value = flash[address];
    return 1;
}

uint8_t board_flash_write(uint16_t address, uint8_t value)
{
    assert(address < sizeof(flash));
    assert((flash[address] & value) == value);
    flash[address] &= value;
    ++writes;
    return 1;
}

uint8_t board_flash_erase(uint16_t address)
{
    assert(address == 0 || address == 512);
    if (torn_erase) {
        for (int i = 0; i < 512; ++i)
            flash[address + i] |= i & 1 ? 0x88 : 0x11;
        return 0;
    }
    memset(flash + address, 0xff, 512);
    ++writes;
    return 1;
}

void board_enter_isp(void)
{
    ++isp_calls;
}

uint8_t board_isp_allowed(void)
{
    return 1;
}

/** @brief Apply safe latch values, then reset only the simulated STC CPU. */
static void boot(void)
{
    now = 0;
    isp_calls = events_used = 0;
    levels[OUT_MAIN] = levels[OUT_PD] = 0;
    levels[OUT_RESET] = 1;
    power_init(now);
}

/** @brief Run hardware timer ticks in the same service order as the firmware. */
static void run(unsigned int ms, uint16_t adc)
{
    for (unsigned int i = 0; i < ms / TICK_MS; ++i) {
        ++now;
        power_tick(now, adc, 1);
        power_save_step(now);
    }
}

/** @brief Complete one eligible short press and verify the staged outputs. */
static void short_press(void)
{
    run(100, BOTH);
    assert(!levels[OUT_MAIN]);
    for (unsigned int i = 0; i < 20 && !levels[OUT_MAIN]; ++i)
        run(5, IDLE);
    assert(levels[OUT_MAIN] && !levels[OUT_PD] && levels[OUT_RESET]);
    run(15, IDLE);
    assert(!levels[OUT_PD] && levels[OUT_RESET]);
    run(5, IDLE);
    assert(levels[OUT_PD] && levels[OUT_RESET]);
    run(125, IDLE);
    assert(levels[OUT_RESET]);
    run(5, IDLE);
    assert(!levels[OUT_RESET] && (power_status() & STATUS_TAKEOVER) == STATUS_TAKEOVER);
}

/** @brief Verify immediate forced cutoff even with no UART progress or ACK. */
static void gestures(void)
{
    uint16_t hold_start;
    uint8_t before;
    memset(flash, 0xff, sizeof(flash));
    boot();
    run(80, BOTH); /* A button already down at boot is not a fresh gesture. */
    run(100, IDLE);
    const uint16_t other_states[] = {0, 44, 47, 91, 96, 137, 142, 231, 239, 284, 293};
    for (unsigned int i = 0; i < sizeof(other_states) / sizeof(other_states[0]); ++i) {
        run(600, other_states[i]);
        run(100, IDLE);
        assert(!levels[OUT_MAIN] && !events_used);
    }
    run(100, IDLE);
    assert(!levels[OUT_MAIN] && !events_used);
    short_press();
    assert(events[0] == 1 && events[1] == 3 && events[2] == 5 && events[3] == 0);
    run(9900, BOTH);
    run(5, SINGLE); /* Cancels the first hold even though the filter has not settled. */
    run(60, BOTH);
    assert(levels[OUT_MAIN]);
    assert(power_command(CMD_PREPARE_OFF, 0, now));
    run(50, SINGLE);
    hold_start = now;
    before = writes;
    while (levels[OUT_MAIN] && now - hold_start < 2020)
        run(TICK_MS, BOTH);
    assert(now - hold_start >= 2000 && now - hold_start < 2020);
    assert(!levels[OUT_MAIN] && !levels[OUT_PD] && levels[OUT_RESET]);
    assert(events[4] == (OUT_MAIN << 1));
    assert(writes == (uint8_t)(before + 1)); /* First journal byte follows the cut. */
    run(1000, BOTH);
    run(100, IDLE);
    assert(!levels[OUT_MAIN] && !journal_flag());
    run(400000, IDLE); /* Timer wraps while off. */
    short_press();
    boot();
    assert(!levels[OUT_MAIN] && !levels[OUT_PD] && levels[OUT_RESET]);
    run(DISCHARGE_MS - TICK_MS, IDLE);
    assert(!levels[OUT_MAIN]);
    run(TICK_MS, IDLE);
    assert(levels[OUT_MAIN] && !levels[OUT_PD] && levels[OUT_RESET]);
}

/** @brief Accept both tolerance extremes with quantization of the real ladder. */
static void tolerance_edges(void)
{
    const uint16_t idle[] = {333, 343};
    const uint16_t both[] = {183, 190};
    for (unsigned int i = 0; i < sizeof(idle) / sizeof(idle[0]); ++i) {
        for (unsigned int j = 0; j < sizeof(both) / sizeof(both[0]); ++j) {
            memset(flash, 0xff, sizeof(flash));
            boot();
            run(100, idle[i]);
            run(100, both[j]);
            assert(!levels[OUT_MAIN]);
            run(100, idle[i]);
            assert(levels[OUT_MAIN]);
        }
    }
}

/** @brief Require Linux arming and final confirmation for normal poweroff. */
static void shutdown_and_isp(void)
{
    run(200, IDLE);
    assert(power_command(CMD_PREPARE_OFF, 0, now));
    assert(!power_command(CMD_OFF_READY, 0, now));
    assert(!power_command(CMD_ENTER_ISP, 1500, now));
    assert(power_command(CMD_ARM_OFF, 0, now));
    assert(journal_flag() && levels[OUT_MAIN]);
    assert(power_command(CMD_OFF_READY, 0, now));
    run(35, IDLE);
    assert(journal_flag() && levels[OUT_MAIN]);
    run(25, IDLE);
    assert(!journal_flag() && !levels[OUT_MAIN]);
    boot();
    assert(!levels[OUT_MAIN]);
    run(100, IDLE);
    short_press();
    run(9900, BOTH);
    assert(power_command(CMD_ENTER_ISP, 1500, now));
    run(1495, BOTH);
    assert(!isp_calls && levels[OUT_MAIN] && journal_flag());
    run(5, BOTH);
    assert(isp_calls == 1 && levels[OUT_MAIN]);
    boot(); /* Simulate ROM leaving Data Flash in place. */
    run(DISCHARGE_MS + RESET_RELEASE_MS + 5, BOTH);
    assert((power_status() & STATUS_TAKEOVER) == STATUS_TAKEOVER);
}

/** @brief Save a complete record through the production journal implementation. */
static void save(uint8_t value)
{
    for (int i = 0; i < 12; ++i) {
        const uint8_t result = journal_step(value, 1);
        assert(result != SAVE_FAILED);
        if (result == SAVE_DONE)
            return;
    }
    assert(!"journal did not finish");
}

/** @brief Inject power cuts through a sector rollover and each commit byte. */
static void interruption(void)
{
    uint8_t before[sizeof(flash)];
    memset(flash, 0xff, sizeof(flash));
    assert(journal_init() && !journal_flag());
    for (int i = 0; i < 128; ++i)
        save(!(i & 1));
    memcpy(before, flash, sizeof(flash));
    for (int cut = 0; cut <= 10; ++cut) {
        memcpy(flash, before, sizeof(flash));
        assert(journal_init() && !journal_flag());
        for (int step = 0; step < cut; ++step)
            journal_step(1, 1);
        assert(journal_init() && journal_flag() == (cut >= 9));
    }
    memcpy(flash, before, sizeof(flash));
    assert(journal_init() && !journal_flag());
    torn_erase = 1;
    assert(journal_step(1, 1) == SAVE_FAILED);
    torn_erase = 0;
    assert(journal_init() && !journal_flag());
    save(1);
    assert(journal_init() && journal_flag());
}

int main(void)
{
    const uint8_t final[FRAME_SIZE] = { 0xa5, 0x5a, CMD_OFF_READY, 0, 0, 0, 0, 0, 0, 0x75 };
    assert(stc_frame_valid(final));
    gestures();
    shutdown_and_isp();
    interruption();
    tolerance_edges();
    puts("STC8 gestures, power sequencing, forced and normal shutdown, ISP pause and journal passed");
    return 0;
}
