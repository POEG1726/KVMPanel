#pragma once
#include "lvgl.h"
#include "cJSON.h"

/* Main dashboard screen (spec 4.1, mockup e2-v5.html): CPU, GPU, Memory, System tiles, plus the
 * PC-offline page (offline.c). w x h is the screen: 1080x1920 portrait or 1920x1080 landscape. */
void ui_create(lv_obj_t * scr, int w, int h);
/* One parsed panel-socket message (telemetry, status, ...). */
void ui_on_message(const cJSON * msg);
/* Per-frame work: warn flashing. */
void ui_tick(void);
