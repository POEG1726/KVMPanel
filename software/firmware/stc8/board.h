/*
 * board.h
 * Created on Nov 22, 2026
 * Author: @POEG1726
 */

#ifndef _BOARD_H_
#define _BOARD_H_

/* All state-machine durations are integral Timer 0 ticks. */
#define TICK_MS          5
#define PD_DELAY_MS      20
#define RESET_RELEASE_MS 150
#define DISCHARGE_MS     500
#define SHORT_MIN_MS     50
#define SHORT_MAX_MS     500
#define FORCE_MS         10000
#define ISP_MIN_MS       500
#define ISP_MAX_MS       3000

/* Inclusive VCC-referenced ADC counts. Boundaries come from adjacent
 * 1%-tolerance E12 keypad states; see README.md for the calculated margins. */
#ifndef KEY_BOTH_LOW
#define KEY_BOTH_LOW     163
#define KEY_BOTH_HIGH    210
#define KEY_IDLE_LOW     313
#define KEY_IDLE_HIGH    1023
#define KEY_HYSTERESIS   6
#elif !defined(KEY_BOTH_HIGH) || !defined(KEY_IDLE_LOW) || \
      !defined(KEY_IDLE_HIGH) || !defined(KEY_HYSTERESIS)
#error Set both ADC windows and hysteresis together
#endif

#if KEY_BOTH_LOW <= KEY_BOTH_HIGH && KEY_IDLE_LOW <= KEY_IDLE_HIGH
#if KEY_BOTH_LOW < 0 || KEY_IDLE_LOW < 0 || KEY_BOTH_HIGH > 1023 || KEY_IDLE_HIGH > 1023 || \
    KEY_HYSTERESIS < 0 || KEY_HYSTERESIS > 1023 || \
    !(KEY_BOTH_HIGH + 2 * KEY_HYSTERESIS < KEY_IDLE_LOW || \
      KEY_IDLE_HIGH + 2 * KEY_HYSTERESIS < KEY_BOTH_LOW)
#error Invalid or overlapping ADC windows
#endif
#endif

#endif // _BOARD_H_
