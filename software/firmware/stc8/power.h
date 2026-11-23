/*
 * power.h
 * Created on Nov 22, 2026
 * Author: @POEG1726
 */

#ifndef _POWER_H_
#define _POWER_H_

#include <stdint.h>

enum { OUT_RESET, OUT_MAIN, OUT_PD };

/** @brief Recover the flag after the HAL has applied safe GPIO levels. */
void power_init(uint16_t now);

/** @brief Run power sequencing and sample processing on the monotonic 5 ms clock. */
void power_tick(uint16_t now, uint16_t adc, uint8_t fresh);

/** @brief Run at most one background flash operation. */
void power_save_step(uint16_t now);

/** @brief Handle one validated UART command. Return 0 for a rejected command. */
uint8_t power_command(uint8_t command, uint16_t value, uint16_t now);

/** @brief Read the current driven levels and persistent flag status. */
uint8_t power_status(void);

/** @brief Drive a board output. OUT_RESET asserts reset when high. */
void board_output(uint8_t output, uint8_t high);

/** @brief Read one byte at a Data Flash address. Return 0 on IAP failure. */
uint8_t board_flash_read(uint16_t address, uint8_t *value);

/** @brief Program one Data Flash byte. Return 0 on IAP failure. */
uint8_t board_flash_write(uint16_t address, uint8_t value);

/** @brief Erase one aligned Data Flash sector. Return 0 on IAP failure. */
uint8_t board_flash_erase(uint16_t address);

/** @brief Reset into the factory ISP monitor. */
void board_enter_isp(void);

/** @brief Check the P5.4 reset-pin option. */
uint8_t board_isp_allowed(void);

#endif // _POWER_H_
