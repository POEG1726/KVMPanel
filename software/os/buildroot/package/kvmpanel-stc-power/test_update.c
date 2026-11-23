/*
 * test_update.c
 * Created on Nov 22, 2026
 * Author: @POEG1726
 */

#define _DEFAULT_SOURCE
#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <linux/gpio.h>
#include <pthread.h>
#include <stdarg.h>
#include <stdatomic.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>
#include <unistd.h>

#include "protocol.h"
#include "stc-power.h"

enum scenario { GOOD, LOST_ACK, WRONG_VERSION, NOT_READY, BAD_OPTIONS };
static enum scenario scenario;
static atomic_int app_booted, gpio_releases;
static int saw_isp_mode, saw_program_mode;
static uint8_t fake_flash[8192], fake_eeprom[4096];

int stc_serial_mode(int fd, speed_t speed, int parity)
{
    (void)fd;
    if (speed == B2400) {
        assert(parity);
        saw_isp_mode = 1;
    } else if (speed == B57600) {
        assert(parity && saw_isp_mode);
        saw_program_mode = 1;
    } else {
        assert(speed == B9600 && !parity);
    }
    return 0;
}

int stc_send(int fd, const uint8_t *buf, size_t size, int timeout_ms)
{
    (void)timeout_ms;
    return write(fd, buf, size) == (ssize_t)size ? 0 : -1;
}

int stc_request(int fd, uint8_t cmd, uint16_t value, uint16_t *reply)
{
    (void)fd;
    assert(!gpio_releases);
    switch (cmd) {
    case CMD_STATUS:
        *reply = STATUS_TAKEOVER | STATUS_ISP_ALLOWED;
        if (app_booted && scenario == NOT_READY)
            *reply &= ~STATUS_PD;
        return 0;
    case CMD_VERSION:
        assert(app_booted);
        *reply = scenario == WRONG_VERSION ? 0x9999 : STC_APP_VERSION;
        return 0;
    case CMD_ENTER_ISP:
        assert(value == 1500);
        *reply = value;
        return 0;
    default:
        assert(!"unexpected app command");
        return -1;
    }
}

int __wrap_ioctl(int fd, unsigned long request, ...)
{
    struct gpiohandle_data *data;
    va_list args;
    assert(fd == 42 && request == GPIOHANDLE_SET_LINE_VALUES_IOCTL);
    assert(app_booted && scenario == GOOD);
    va_start(args, request);
    data = va_arg(args, struct gpiohandle_data *);
    va_end(args);
    assert(data->values[0] == 0);
    ++gpio_releases;
    return 0;
}

/** @brief Wrap ROM response bytes in the original STC8 ISP frame format. */
static void respond(int fd, const uint8_t *payload, size_t count)
{
    uint8_t frame[64] = { 0x46, 0xb9, 0x68, 0, count + 6 };
    unsigned int sum = 0x68 + count + 6;
    assert(count + 8 <= sizeof(frame));
    for (size_t i = 0; i < count; ++i)
        sum += payload[i];
    memcpy(frame + 5, payload, count);
    frame[5 + count] = sum >> 8;
    frame[6 + count] = sum;
    frame[7 + count] = 0x16;
    assert(write(fd, frame, count + 8) == (ssize_t)(count + 8));
}

/** @brief Model the stock ROM, with an EEPROM sentinel outside program memory. */
static void *factory_rom(void *arg)
{
    const int fd = *(int *)arg;
    uint8_t buf[255], status[40] = { 0x50, 0x00, 0xa8, 0xc0, 0x00 };
    const uint8_t write_ack[] = { 2, 'T' };
    ssize_t count;
    uint16_t address;
    int wrote = 0, erased = 0;
    status[9] = scenario == BAD_OPTIONS ? 2 : 0;
    status[11] = 0x10;
    status[17] = 0x72;
    status[20] = 0xf7;
    status[21] = 0x94;
    /* The socket is message-oriented, corresponding to the UART wire frames. */
    while ((count = read(fd, buf, sizeof(buf))) > 0) {
        if (count == 1) {
            assert(buf[0] == 0x7f && !erased);
            respond(fd, status, sizeof(status));
            continue;
        }
        assert(count == buf[4] + 2 && count >= 9);
        assert(buf[0] == 0x46 && buf[1] == 0xb9 && buf[count - 1] == 0x16);
        assert(!gpio_releases && scenario != BAD_OPTIONS);
        switch (buf[5]) {
        case 0x01:
        case 0x05:
            assert(!erased);
            respond(fd, &buf[5], 1);
            break;
        case 0x03:
            assert(count == 13 && buf[6] == 0 && buf[7] == 0);
            memset(fake_flash, 0xff, sizeof(fake_flash));
            erased = 1;
            respond(fd, &buf[5], 1);
            break;
        case 0x22:
        case 0x02:
            assert(erased);
            address = ((uint16_t)buf[6] << 8) | buf[7];
            assert(address + (size_t)count - 13 <= sizeof(fake_flash));
            memcpy(fake_flash + address, buf + 10, (size_t)count - 13);
            ++wrote;
            if (scenario != LOST_ACK)
                respond(fd, write_ack, sizeof(write_ack));
            break;
        case 0xff:
            assert(wrote && scenario != LOST_ACK);
            app_booted = 1;
            return NULL;
        default:
            assert(!"option/EEPROM write or unknown command");
        }
    }
    return NULL;
}

/** @brief Run the actual updater and original ISP backend against the fake ROM. */
static void exercise(enum scenario next)
{
    pthread_t rom;
    int pair[2], result;
    uint8_t image[STC_IMAGE_END + 6] = { 0x02, 0x00, 0x11 };
    memcpy(image + STC_IMAGE_END, "KVS8", 4);
    image[STC_IMAGE_END + 4] = STC_APP_VERSION & 0xff;
    image[STC_IMAGE_END + 5] = STC_APP_VERSION >> 8;
    scenario = next;
    app_booted = gpio_releases = saw_isp_mode = saw_program_mode = 0;
    memset(fake_flash, 0x37, sizeof(fake_flash));
    memset(fake_eeprom, 0x5a, sizeof(fake_eeprom));
    assert(socketpair(AF_UNIX, SOCK_SEQPACKET, 0, pair) == 0);
    assert(fcntl(pair[0], F_SETFL, O_NONBLOCK) == 0);
    assert(pthread_create(&rom, NULL, factory_rom, &pair[1]) == 0);
    result = stc_update_attempt(pair[0], 42, image, sizeof(image), STC_APP_VERSION, 0);
    close(pair[0]);
    pthread_join(rom, NULL);
    close(pair[1]);
    assert((result == 0) == (next == GOOD));
    assert(gpio_releases == (next == GOOD));
    assert(saw_isp_mode && saw_program_mode == (next != BAD_OPTIONS));
    assert(app_booted == (next == GOOD || next == WRONG_VERSION || next == NOT_READY));
    if (app_booted)
        assert(!memcmp(image, fake_flash, sizeof(image)));
    if (next == BAD_OPTIONS)
        assert(fake_flash[0] == 0x37);
    for (size_t i = 0; i < sizeof(fake_eeprom); ++i)
        assert(fake_eeprom[i] == 0x5a);
}

int main(void)
{
    exercise(GOOD);
    exercise(LOST_ACK);
    exercise(WRONG_VERSION);
    exercise(NOT_READY);
    exercise(BAD_OPTIONS);
    puts("Native ISP: code-only erase, ACK handling, version/output checks and HOLD release passed");
    return 0;
}
