/*
 * serial.c
 * Created on Nov 22, 2026
 * Author: @POEG1726
 */

#define _GNU_SOURCE
#include <errno.h>
#include <poll.h>
#include <string.h>
#include <sys/random.h>
#include <time.h>
#include <unistd.h>

#include "protocol.h"
#include "stc-power.h"

/** @brief Set raw serial mode for application or ISP traffic. */
int stc_serial_mode(int fd, speed_t speed, int parity)
{
    struct termios term;
    if (tcgetattr(fd, &term) < 0)
        return -1;
    cfmakeraw(&term);
    term.c_cflag &= ~(CSIZE | CSTOPB | PARENB | PARODD | CRTSCTS);
    term.c_cflag |= CS8 | CLOCAL | CREAD;
    if (parity)
        term.c_cflag |= PARENB;
    term.c_cc[VMIN] = 0;
    term.c_cc[VTIME] = 0;
    if (cfsetspeed(&term, speed) < 0)
        return -1;
    return tcsetattr(fd, TCSAFLUSH, &term);
}

/** @brief Return the monotonic clock in milliseconds. */
static int64_t milliseconds(void)
{
    struct timespec time;
    clock_gettime(CLOCK_MONOTONIC, &time);
    return (int64_t)time.tv_sec * 1000 + time.tv_nsec / 1000000;
}

/** @brief Poll until the deadline, handling interruption and disconnected UARTs. */
static int wait_io(int fd, short event, int64_t deadline)
{
    struct pollfd pfd = { .fd = fd, .events = event };
    int64_t left;
    int ready;
    do {
        left = deadline - milliseconds();
        if (left <= 0)
            break;
        ready = poll(&pfd, 1, (int)left);
        if (ready > 0 && pfd.revents & event)
            return 0;
        if (ready > 0) {
            errno = EIO;
            return -1;
        }
        if (ready < 0 && errno != EINTR)
            return -1;
    } while (ready < 0);
    errno = ETIMEDOUT;
    return -1;
}

/** @brief Write the entire frame without blocking indefinitely. */
int stc_send(int fd, const uint8_t *buf, size_t size, int timeout_ms)
{
    int64_t deadline = milliseconds() + timeout_ms;
    ssize_t sent;
    while (size) {
        if (wait_io(fd, POLLOUT, deadline) < 0)
            return -1;
        sent = write(fd, buf, size);
        if (sent > 0) {
            buf += sent;
            size -= (size_t)sent;
        } else if (sent < 0 && errno != EAGAIN && errno != EINTR) {
            return -1;
        }
    }
    return 0;
}

/** @brief Receive a valid response to the current command, ignoring stale frames. */
int stc_request(int fd, uint8_t cmd, uint16_t value, uint16_t *reply)
{
    uint8_t msg[FRAME_SIZE] = { FRAME_SOF0, FRAME_SOF1, cmd };
    uint8_t rx[FRAME_SIZE], byte;
    size_t used = 0;
    ssize_t n;
    int64_t deadline;
    if (getrandom(msg + 3, 4, GRND_NONBLOCK) != 4)
        return -1;
    stc_set_value(msg, value);
    if (stc_send(fd, msg, sizeof(msg), 100) < 0)
        return -1;
    deadline = milliseconds() + 175;
    while (wait_io(fd, POLLIN, deadline) == 0) {
        n = read(fd, &byte, 1);
        if (n != 1) {
            if (n < 0 && errno != EINTR && errno != EAGAIN)
                return -1;
            continue;
        }
        if (!used && byte != FRAME_SOF0)
            continue;
        if (used == 1 && byte != FRAME_SOF1) {
            used = byte == FRAME_SOF0;
            continue;
        }
        rx[used++] = byte;
        if (used < sizeof(rx))
            continue;
        used = 0;
        if (!stc_frame_valid(rx) || memcmp(rx + 3, msg + 3, 4))
            continue;
        if (rx[2] == (cmd | REPLY)) {
            *reply = stc_value(rx);
            return 0;
        }
        if (rx[2] == REJECT) {
            errno = EBUSY;
            return -1;
        }
    }
    return -1;
}
