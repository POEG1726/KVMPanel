#pragma once
#include "lvgl.h"
#include "cJSON.h"

/* Main dashboard screen (spec 4.1, mockup e2-v5.html): CPU, GPU, Memory, System tiles. */
void ui_create(lv_obj_t * scr);
/* One parsed panel-socket message (telemetry, status, ...). */
void ui_on_message(const cJSON * msg);
/* Per-frame work: warn flashing. */
void ui_tick(void);
