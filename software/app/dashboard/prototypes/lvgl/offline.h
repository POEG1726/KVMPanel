#pragma once
#include "lvgl.h"
#include "cJSON.h"

/* PC-offline page (spec 4.2, mockup landscape-offline.html part 2): the board's own status while the
 * PC host program sends no telemetry. Replaces `main_scr` on the screen while active. */
void offline_create(lv_obj_t * scr, int w, int h, lv_obj_t * main_scr);
/* board / pc / config / result messages. */
void offline_on_message(const char * t, const cJSON * msg);
/* A telemetry message arrived (ts = its Unix ms timestamp). */
void offline_telemetry(double ts);
/* Per frame: switch pages, screen-off countdown, wake on input. */
void offline_tick(void);
bool offline_active(void);
