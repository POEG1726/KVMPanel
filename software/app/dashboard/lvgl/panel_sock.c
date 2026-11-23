/*
 * panel_sock.c
 * Created on Nov 23, 2026
 * Author: @POEG1726
 */

#include "panel_sock.h"
#include "lvgl.h"
#include <errno.h>
#include <poll.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>

static struct sockaddr_un address;
static panel_message_fn receive;
static int descriptor = -1;
static bool connected;
static uint32_t last_attempt;
static char line[256 * 1024];
static size_t used;

/** Drop the partial message whenever kvmd disconnects or the peer stalls. */
static void disconnect(void)
{
    close(descriptor);
    descriptor = -1;
    if (connected)
        receive(NULL);
    connected = false;
    used = 0;
}

/** Service the local socket without delaying display refresh or touch input. */
static void poll_socket(lv_timer_t *timer)
{
    (void)timer;
    if (descriptor < 0) {
        if (lv_tick_elaps(last_attempt) < 500)
            return;
        last_attempt = lv_tick_get();
        descriptor = socket(AF_UNIX, SOCK_STREAM | SOCK_NONBLOCK | SOCK_CLOEXEC, 0);
        if (descriptor < 0)
            return;
        if (connect(descriptor, (struct sockaddr *)&address, sizeof(address)) != 0 && errno != EINPROGRESS) {
            disconnect();
            return;
        }
    }
    if (!connected) {
        struct pollfd pfd = {.fd = descriptor, .events = POLLOUT};
        if (poll(&pfd, 1, 0) <= 0)
            return;
        int error;
        socklen_t size = sizeof(error);
        if (getsockopt(descriptor, SOL_SOCKET, SO_ERROR, &error, &size) != 0 || error) {
            disconnect();
            return;
        }
        connected = true;
    }
    for (unsigned pass = 0; pass < 8; ++pass) {
        const ssize_t count = recv(descriptor, line + used, sizeof(line) - used, 0);
        if (count <= 0) {
            if (count == 0 || (errno != EAGAIN && errno != EWOULDBLOCK && errno != EINTR))
                disconnect();
            return;
        }
        used += (size_t)count;
        char *start = line;
        char *end;
        while ((end = memchr(start, '\n', used - (size_t)(start - line)))) {
            cJSON *message = cJSON_ParseWithLength(start, (size_t)(end - start));
            if (message) {
                receive(message);
                cJSON_Delete(message);
            }
            if (!connected)
                return;
            start = end + 1;
        }
        used -= (size_t)(start - line);
        memmove(line, start, used);
        if (used == sizeof(line)) {
            disconnect();
            return;
        }
    }
}

bool panel_sock_start(const char *path, panel_message_fn on_message)
{
    if (!path[0] || strlen(path) >= sizeof(address.sun_path))
        return false;
    address.sun_family = AF_UNIX;
    strcpy(address.sun_path, path);
    receive = on_message;
    last_attempt = lv_tick_get() - 500;
    return lv_timer_create(poll_socket, 50, NULL) != NULL;
}

bool panel_sock_send(const cJSON *request)
{
    if (!connected || !request)
        return false;
    char *json = cJSON_PrintUnformatted(request);
    if (!json)
        return false;
    const size_t size = strlen(json);
    struct iovec iov[] = {{json, size}, {(void *)"\n", 1}};
    struct msghdr msg = {.msg_iov = iov, .msg_iovlen = 2};
    const bool sent = sendmsg(descriptor, &msg, MSG_NOSIGNAL) == (ssize_t)(size + 1);
    cJSON_free(json);
    if (!sent)
        disconnect();
    return sent;
}
