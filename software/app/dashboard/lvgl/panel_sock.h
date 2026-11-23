/*
 * panel_sock.h
 * Created on Nov 23, 2026
 * Author: @POEG1726
 */

#ifndef _PANEL_SOCK_H_
#define _PANEL_SOCK_H_

#include "cJSON.h"
#include <stdbool.h>

/** Receive an object per line. NULL tells the application the connection dropped. */
typedef void (*panel_message_fn)(const cJSON *message);
bool panel_sock_start(const char *path, panel_message_fn on_message);
bool panel_sock_send(const cJSON *request);

#endif // _PANEL_SOCK_H_
