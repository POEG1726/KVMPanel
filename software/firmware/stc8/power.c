/*
 * power.c
 * Created on Nov 22, 2026
 * Author: @POEG1726
 */

#include "board.h"
#include "journal.h"
#include "power.h"
#include "protocol.h"

#define TICKS(ms) ((ms) / TICK_MS)

enum { OFF, STARTING, RUNNING, WAIT_OFF, ARMED_OFF, COMMIT_OFF, WAIT_ISP };
enum { KEY_OTHER, KEY_IDLE, KEY_BOTH };

static uint8_t state, outputs, want_flag, flash_error, save_pending, save_failures;
static uint8_t observed, stable, filtered_valid, armed, gesture, short_ok, discharging;
static uint8_t restore_pending;
static uint16_t started, pd_at, off_at, isp_at, retry_at;
static uint16_t filtered, raw_down_at, down_at;

/** @brief Compare a 16-bit tick deadline over intervals less than half a wrap. */
static uint8_t reached(uint16_t now, uint16_t deadline)
{
    return (int16_t)(now - deadline) >= 0;
}

/** @brief Drive a level and mirror its logical value for the host. */
static void drive(uint8_t output, uint8_t high)
{
    uint8_t mask = output == OUT_MAIN ? STATUS_MAIN :
        output == OUT_PD ? STATUS_PD : STATUS_RESET_RELEASED;
    board_output(output, high);
    if (high ^ (output == OUT_RESET))
        outputs |= mask;
    else
        outputs &= (uint8_t)~mask;
}

/** @brief Schedule a journal update without blocking the power transition. */
static void save_flag(uint8_t value, uint16_t now)
{
    want_flag = value;
    save_pending = 1;
    save_failures = 0;
    retry_at = now;
}

/** @brief Start the rails with reset held. */
static void start(uint16_t now)
{
    drive(OUT_RESET, 1);
    drive(OUT_MAIN, 1);
    state = STARTING;
    started = now;
    save_flag(1, now);
}

/** @brief Force drops main first. Normal shutdown asserts reset before cutting. */
static void stop(uint16_t now, uint8_t force)
{
    if (force)
        drive(OUT_MAIN, 0);
    drive(OUT_RESET, 1);
    drive(OUT_PD, 0);
    if (!force)
        drive(OUT_MAIN, 0);
    state = OFF;
    off_at = now + TICKS(DISCHARGE_MS);
    discharging = 1;
    if (force)
        save_flag(0, now);
    armed = gesture = short_ok = 0;
}

/** @brief Recover only after the HAL has initialized all three output latches. */
void power_init(uint16_t now)
{
    state = OFF;
    outputs = armed = gesture = short_ok = 0;
    observed = stable = KEY_OTHER;
    filtered_valid = save_pending = discharging = 0;
    flash_error = !journal_init();
    restore_pending = journal_flag();
    if (restore_pending) {
        off_at = now + TICKS(DISCHARGE_MS);
        discharging = 1;
    }
}

/** @brief Classify only measured none/both windows. Keep a little hysteresis. */
static uint8_t classify(int16_t adc, uint8_t previous)
{
#if KEY_BOTH_LOW <= KEY_BOTH_HIGH && KEY_IDLE_LOW <= KEY_IDLE_HIGH
    int16_t margin = previous == KEY_BOTH ? KEY_HYSTERESIS : 0;
    if (adc >= KEY_BOTH_LOW - margin && adc <= KEY_BOTH_HIGH + margin)
        return KEY_BOTH;
    margin = previous == KEY_IDLE ? KEY_HYSTERESIS : 0;
    if (adc >= KEY_IDLE_LOW - margin && adc <= KEY_IDLE_HIGH + margin)
        return KEY_IDLE;
#else
    (void)adc;
    (void)previous;
#endif
    return KEY_OTHER;
}

/** @brief A short gesture needs a fresh press and stable full release. */
static void keys(uint16_t now, uint16_t adc, uint8_t fresh)
{
    uint8_t next, raw;
    uint16_t duration;
    if (fresh) {
        if (!filtered_valid) {
            filtered = adc * 4;
            filtered_valid = 1;
        }
        filtered = filtered - (filtered + 2) / 4 + adc;
    }
    raw = fresh ? classify(adc, observed) : KEY_OTHER;
    if (raw == KEY_BOTH && observed != KEY_BOTH)
        raw_down_at = now;
    observed = raw;
    next = fresh ? classify((filtered + 2) / 4, stable) : KEY_OTHER;
    if (observed != next)
        next = KEY_OTHER;
    if (state == WAIT_ISP)
        return;
    if (state == OFF && short_ok && (observed == KEY_BOTH ||
                                    (uint16_t)(now - down_at) > TICKS(SHORT_MAX_MS)))
        short_ok = 0;
    if (next == KEY_BOTH && stable != KEY_BOTH && armed) {
        if (state == OFF)
            armed = 0;
        gesture = 1;
        down_at = raw_down_at;
    }
    stable = next;
    if (gesture) {
        duration = now - down_at;
        /* Raw ADC leaving the hysteresis window cancels the long hold at once.
         * Startup still requires the measured full-release window. */
        if (observed != KEY_BOTH) {
            gesture = 0;
            short_ok = fresh && state == OFF && duration >= TICKS(SHORT_MIN_MS) &&
                duration <= TICKS(SHORT_MAX_MS);
        } else if (state != OFF && duration >= TICKS(FORCE_MS)) {
            stop(now, 1);
        } else if (state == OFF && duration > TICKS(SHORT_MAX_MS)) {
            gesture = 0;
        }
    }
    if (next == KEY_IDLE) {
        if (state == OFF && short_ok && !discharging && !save_pending)
            start(now);
        armed = 1;
        short_ok = 0;
    }
}

/** @brief Service keys before timed outputs and before any foreground flash work. */
void power_tick(uint16_t now, uint16_t adc, uint8_t fresh)
{
    if (discharging && reached(now, off_at)) {
        discharging = 0;
        if (restore_pending) {
            restore_pending = 0;
            gesture = short_ok = 0;
            start(now);
        }
    }
    keys(now, adc, fresh);
    if (state == STARTING) {
        if (!(outputs & STATUS_PD) && reached(now, started + TICKS(PD_DELAY_MS))) {
            drive(OUT_PD, 1);
            pd_at = now;
        }
        if (reached(now, started + TICKS(RESET_RELEASE_MS)) &&
            reached(now, pd_at + TICKS(RESET_RELEASE_MS - PD_DELAY_MS))) {
            drive(OUT_RESET, 0);
            state = RUNNING;
        }
    } else if (state == WAIT_ISP && reached(now, isp_at)) {
        board_enter_isp();
    }
}

/** @brief Program one byte per service call. A held gesture defers sector erase. */
void power_save_step(uint16_t now)
{
    uint8_t result;
    if (!save_pending || !reached(now, retry_at))
        return;
    result = journal_step(want_flag, !gesture);
    if (result == SAVE_DONE) {
        flash_error = save_pending = 0;
        if (state == COMMIT_OFF)
            stop(now, 0);
    } else if (result == SAVE_FAILED) {
        flash_error = 1;
        retry_at = now + TICKS(250);
        if (++save_failures == 3) {
            save_pending = 0;
            if (state == COMMIT_OFF)
                state = ARMED_OFF;
        }
    }
}

/** @brief Apply commands only in compatible power states. */
uint8_t power_command(uint8_t command, uint16_t value, uint16_t now)
{
    switch (command) {
    case CMD_PREPARE_OFF:
        if (state != RUNNING && state != WAIT_OFF)
            return 0;
        state = WAIT_OFF;
        return 1;
    case CMD_CANCEL_OFF:
        if (state != WAIT_OFF && state != ARMED_OFF)
            return 0;
        state = RUNNING;
        return 1;
    case CMD_ARM_OFF:
        if (state != WAIT_OFF && state != ARMED_OFF)
            return 0;
        state = ARMED_OFF;
        return 1;
    case CMD_OFF_READY:
        if (state == COMMIT_OFF)
            return 1;
        if (state != ARMED_OFF)
            return 0;
        state = COMMIT_OFF;
        save_flag(0, now);
        return 1;
    case CMD_ENTER_ISP:
        if (state != RUNNING || !board_isp_allowed() || !journal_flag() ||
            save_pending || flash_error || value < ISP_MIN_MS || value > ISP_MAX_MS ||
            value % TICK_MS)
            return 0;
        gesture = short_ok = armed = 0;
        isp_at = now + TICKS(value);
        state = WAIT_ISP;
        return 1;
    default:
        return 0;
    }
}

/** @brief Report output takeover only with a running MCU and durable flag. */
uint8_t power_status(void)
{
    uint8_t status = outputs;
    if (journal_flag())
        status |= STATUS_FLAG;
    if (state == WAIT_OFF || state == ARMED_OFF || state == COMMIT_OFF)
        status |= STATUS_OFF_PENDING;
    if (state == WAIT_ISP)
        status |= STATUS_ISP_PENDING;
    if (flash_error)
        status |= STATUS_FLASH_ERROR;
    if (board_isp_allowed())
        status |= STATUS_ISP_ALLOWED;
    return status;
}
