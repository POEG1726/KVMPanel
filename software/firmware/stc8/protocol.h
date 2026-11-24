/*
 * protocol.h
 * Created on Nov 22, 2026
 * Author: @POEG1726
 */

#ifndef _PROTOCOL_H_
#define _PROTOCOL_H_

#include <stdint.h>

/* 0xa5, 0x5a, command, nonce[4], value LE[2], CRC-8/ATM over bytes 2..8. */
#define FRAME_SIZE 10
#define FRAME_SOF0 0xa5
#define FRAME_SOF1 0x5a
#define REPLY      0x80
#define REJECT     0xff

enum {
    CMD_STATUS = 1,
    CMD_VERSION,
    CMD_PREPARE_OFF,
    CMD_OFF_READY,
    CMD_CANCEL_OFF,
    CMD_ENTER_ISP,
    CMD_ARM_OFF
};

enum {
    STATUS_MAIN = 1,
    STATUS_PD = 2,
    STATUS_RESET_RELEASED = 4,
    STATUS_FLAG = 8,
    STATUS_OFF_PENDING = 16,
    STATUS_ISP_PENDING = 32,
    STATUS_FLASH_ERROR = 64,
    STATUS_ISP_ALLOWED = 128
};

#define STATUS_TAKEOVER (STATUS_MAIN | STATUS_PD | STATUS_RESET_RELEASED | STATUS_FLAG)
#define STC_APP_VERSION 0x0102
#define STC_IMAGE_END   0x1f00

/** @brief Update the CRC-8/ATM accumulator with one byte. */
static inline uint8_t stc_crc8(uint8_t crc, uint8_t byte)
{
    uint8_t bit;
    crc ^= byte;
    for (bit = 0; bit < 8; ++bit)
        crc = (crc << 1) ^ ((crc & 0x80) ? 0x07 : 0);
    return crc;
}

/** @brief Read the little-endian frame value. */
static inline uint16_t stc_value(const uint8_t *frame)
{
    return frame[7] | ((uint16_t)frame[8] << 8);
}

/** @brief Set the frame value and recompute its checksum. */
static inline void stc_set_value(uint8_t *frame, uint16_t value)
{
    uint8_t index;
    uint8_t crc = 0;
    frame[7] = (uint8_t)value;
    frame[8] = (uint8_t)(value >> 8);
    for (index = 2; index < FRAME_SIZE - 1; ++index)
        crc = stc_crc8(crc, frame[index]);
    frame[FRAME_SIZE - 1] = crc;
}

/** @brief Check the header and CRC of a full frame. */
static inline uint8_t stc_frame_valid(const uint8_t *frame)
{
    uint8_t index;
    uint8_t crc = 0;
    if (frame[0] != FRAME_SOF0 || frame[1] != FRAME_SOF1)
        return 0;
    for (index = 2; index < FRAME_SIZE - 1; ++index)
        crc = stc_crc8(crc, frame[index]);
    return crc == frame[FRAME_SIZE - 1];
}

#endif // _PROTOCOL_H_
