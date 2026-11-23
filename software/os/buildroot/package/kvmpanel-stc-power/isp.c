/*
 * isp.c
 * Created on Nov 22, 2026
 * Author: @POEG1726
 */

#define _DEFAULT_SOURCE
#include <errno.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

#include "protocol.h"
#include "stc-power.h"
#include <stc8prog.h>

#define SUPPORTED_MCU 0xf794
#define TRIM_HZ 11059200u

extern uint8_t memory[65536];  /* stc8prog's existing program buffer */
static int isp_fd;

/** @brief Route stc8prog reads through the already-owned T113 UART. */
static int32_t isp_read(userial_t *port, uint8_t *buf, uint32_t size)
{
    (void)port;
    return read(isp_fd, buf, size);
}

/** @brief Route stc8prog writes through the same descriptor with a deadline. */
static int32_t isp_write(userial_t *port, const uint8_t *buf, uint32_t size)
{
    (void)port;
    return stc_send(isp_fd, buf, size, 150) ? -1 : (int32_t)size;
}

userial_t serial = { .read = isp_read, .write = isp_write };

/** @brief Validate model, hardware options and trim before a code-only flash. */
int stc_program(int fd, const uint8_t *image, size_t size)
{
    const stc_protocol_t *protocol;
    uint8_t status[255] = {0};
    uint8_t exit_isp = 0xff;
    uint32_t hz;
    size_t padded = (size + 127) & ~(size_t)127;
    if (!size || padded > 8192) {
        errno = EFBIG;
        return -1;
    }
    isp_fd = fd;
    if (stc_serial_mode(fd, B2400, 1) < 0)
        return -1;
    if (chip_detect(status, 350) || status[20] != (SUPPORTED_MCU >> 8) ||
        status[21] != (SUPPORTED_MCU & 0xff) || status[17] < 0x72) {
        fputs("STC8G1K08A ISP handshake or boot ROM version check failed.\n", stderr);
        errno = EPROTO;
        return -1;
    }
    hz = ((uint32_t)status[1] << 24) | ((uint32_t)status[2] << 16) |
         ((uint32_t)status[3] << 8) | status[4];
    /* STC8 options in boot status: MSR0 bit 1 erases EEPROM,
     * MSR2 bit 4 clear selects P5.4 reset. Leave all option bytes alone. */
    if ((status[9] & 2) || !(status[11] & 0x10) ||
        hz < TRIM_HZ * 98 / 100 || hz > TRIM_HZ * 102 / 100) {
        fputs("ISP requires EEPROM preserve, P5.4 GPIO and 11.0592 MHz IRC.\n", stderr);
        errno = EINVAL;
        return -1;
    }
    protocol = protocol_lookup(PROTOCOL_STC8GH);
    if (baudrate_set(protocol, 57600, status) ||
        stc_serial_mode(fd, B57600, 1) ||
        baudrate_check(protocol, status, 0x72) ||
        flash_erase(protocol, status)) {
        errno = EIO;
        return -1;
    }
    memset(memory, 0xff, padded);
    memcpy(memory, image, size);
    if (flash_write(protocol, padded) || chip_write(&exit_isp, 1)) {
        errno = EIO;
        return -1;
    }
    /* Keep the ISP baud until the factory reboot frame has left the shifter. */
    usleep(25000);
    return 0;
}
