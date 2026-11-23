#include "panel_sock.h"
#include "lvgl.h"
#include <arpa/inet.h>
#include <errno.h>
#include <netinet/in.h>
#include <poll.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>
#include <unistd.h>

#define BUF_SZ (256 * 1024)

enum { IDLE, CONNECTING, CONNECTED };

static struct sockaddr_in addr;
static panel_msg_cb on_msg;
static int fd = -1, state = IDLE;
static uint32_t next_try, backoff = 500;
static char buf[BUF_SZ];
static size_t len;

static void drop(const char * why)
{
    if(fd >= 0) close(fd);
    fd = -1;
    state = IDLE;
    len = 0;
    next_try = lv_tick_get() + backoff;
    printf("panel: %s, retry in %u ms\n", why, (unsigned)backoff);
    backoff = backoff * 2 > 5000 ? 5000 : backoff * 2;
}

static void handle_lines(void)
{
    char * start = buf;
    char * nl;
    while((nl = memchr(start, '\n', len - (start - buf)))) {
        cJSON * msg = cJSON_ParseWithLength(start, nl - start);
        if(msg) {
            on_msg(msg);
            cJSON_Delete(msg);
        }
        start = nl + 1;
    }
    len -= start - buf;
    memmove(buf, start, len);
    if(len == BUF_SZ) len = 0; /* line longer than the buffer: discard */
}

static void poll_cb(lv_timer_t * t)
{
    (void)t;
    if(state == IDLE) {
        if((int32_t)(lv_tick_get() - next_try) < 0) return;
        fd = socket(AF_INET, SOCK_STREAM | SOCK_NONBLOCK, 0);
        if(connect(fd, (struct sockaddr *)&addr, sizeof addr) == 0) state = CONNECTING;
        else if(errno == EINPROGRESS) state = CONNECTING;
        else {
            drop(strerror(errno));
            return;
        }
    }
    if(state == CONNECTING) {
        struct pollfd p = {fd, POLLOUT, 0};
        if(poll(&p, 1, 0) <= 0) return;
        int err = 0;
        socklen_t el = sizeof err;
        getsockopt(fd, SOL_SOCKET, SO_ERROR, &err, &el);
        if(err) {
            drop(strerror(err));
            return;
        }
        state = CONNECTED;
        backoff = 500;
        printf("panel: connected\n");
    }
    for(;;) {
        ssize_t n = recv(fd, buf + len, BUF_SZ - len, 0);
        if(n == 0) {
            drop("connection closed");
            return;
        }
        if(n < 0) {
            if(errno != EAGAIN && errno != EWOULDBLOCK) drop(strerror(errno));
            return;
        }
        len += (size_t)n;
        handle_lines();
    }
}

bool panel_sock_send(const char * line)
{
    if(state != CONNECTED) return false;
    size_t n = strlen(line);
    /* Requests are tiny; the kernel buffer takes them whole. */
    if(send(fd, line, n, MSG_NOSIGNAL) != (ssize_t)n || send(fd, "\n", 1, MSG_NOSIGNAL) != 1) {
        drop("send failed");
        return false;
    }
    return true;
}

void panel_sock_start(const char * host, int port, panel_msg_cb cb)
{
    on_msg = cb;
    addr.sin_family = AF_INET;
    addr.sin_port = htons(port);
    inet_pton(AF_INET, host, &addr.sin_addr);
    lv_timer_create(poll_cb, 20, NULL);
}
