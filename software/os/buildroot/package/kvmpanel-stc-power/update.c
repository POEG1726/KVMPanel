/*
 * update.c
 * Created on Nov 22, 2026
 * Author: @POEG1726
 */

#define _DEFAULT_SOURCE
#include <errno.h>
#include <linux/gpio.h>
#include <sys/ioctl.h>
#include <unistd.h>

#include "protocol.h"
#include "stc-power.h"

#define TAKEOVER_MASK (STATUS_TAKEOVER | STATUS_ISP_ALLOWED | STATUS_OFF_PENDING | \
                       STATUS_ISP_PENDING | STATUS_FLASH_ERROR)
#define TAKEOVER_OK (STATUS_TAKEOVER | STATUS_ISP_ALLOWED)

/** @brief Accept only the expected application version with all outputs restored. */
static int confirm_takeover(int fd, uint16_t expected)
{
    uint16_t version, status;
    unsigned int attempt;
    if (stc_serial_mode(fd, B9600, 0) < 0)
        return -1;
    for (attempt = 0; attempt < 30; ++attempt) {
        if (stc_request(fd, CMD_VERSION, 0, &version) == 0 && version == expected &&
            stc_request(fd, CMD_STATUS, 0, &status) == 0 &&
            (status & TAKEOVER_MASK) == TAKEOVER_OK)
            return 0;
        usleep(100000);
    }
    errno = ETIMEDOUT;
    return -1;
}

/** @brief Run ROM ISP and verify the new app independently of the downloader result. */
int stc_update_attempt(int fd, int hold, const uint8_t *image, size_t size,
                       uint16_t expected_version, int retry)
{
    uint16_t status = 0, delay;
    struct gpiohandle_data low = { .values = { 0 } };
    int alive;
    if (stc_serial_mode(fd, B9600, 0) < 0)
        return -1;
    alive = stc_request(fd, CMD_STATUS, 0, &status) == 0;
    if (alive && (status & TAKEOVER_MASK) == TAKEOVER_OK) {
        /* Even if the acknowledgement is lost, the ROM may start on schedule.
         * A failed factory handshake leaves HOLD asserted for recovery. */
        if (stc_request(fd, CMD_ENTER_ISP, 1500, &delay) == 0) {
            if (delay != 1500) {
                errno = EPROTO;
                return -1;
            }
        } else if (errno == EBUSY) {
            return -1;
        }
    } else if ((!retry && !(alive && (status & STATUS_ISP_PENDING))) ||
               (alive && !(status & STATUS_ISP_PENDING))) {
        errno = EBUSY;
        return -1;
    }
    if (stc_program(fd, image, size) < 0 || confirm_takeover(fd, expected_version) < 0)
        return -1;
    return ioctl(hold, GPIOHANDLE_SET_LINE_VALUES_IOCTL, &low);
}
