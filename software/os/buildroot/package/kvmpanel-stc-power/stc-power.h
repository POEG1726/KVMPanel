/*
 * stc-power.h
 * Created on Nov 22, 2026
 * Author: @POEG1726
 */

#ifndef _STC_POWER_H_
#define _STC_POWER_H_

#include <stddef.h>
#include <stdint.h>
#include <termios.h>

#define STC_TTY "/dev/ttyS2"
#define STC_LOCK "/run/lock/kvmpanel-stc-power"

/** @brief Set raw 8N1 or 8E1, without changing UART ownership. */
int stc_serial_mode(int fd, speed_t speed, int parity);

/** @brief Write a complete buffer within a monotonic deadline. */
int stc_send(int fd, const uint8_t *buf, size_t size, int timeout_ms);

/** @brief Exchange a nonce-checked application command. */
int stc_request(int fd, uint8_t cmd, uint16_t value, uint16_t *reply);

/** @brief Flash the validated code image using the factory ISP tool. */
int stc_program(int fd, const uint8_t *image, size_t size);

/** @brief Attempt one complete handoff. Caller owns HOLD on all errors. */
int stc_update_attempt(int fd, int hold, const uint8_t *image, size_t size,
                       uint16_t expected_version, int retry);

#endif // _STC_POWER_H_
