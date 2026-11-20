#pragma once
#include "cJSON.h"

typedef void (*panel_msg_cb)(const cJSON * msg);

/* Non-blocking NDJSON client for the kvmd panel socket (TCP host:port on the PC mock).
 * Polled from an LVGL timer; reconnects with backoff when the connection drops. */
void panel_sock_start(const char * host, int port, panel_msg_cb cb);
