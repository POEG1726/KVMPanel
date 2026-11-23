#pragma once
#include "lvgl.h"

/* LVGL display rendered at full w x h, shown in an X11 window downscaled by `scale` (1 or 2). */
lv_display_t * x11_out_create(int w, int h, int scale);
/* Handles X events; returns false once the window was closed. */
bool x11_out_pump(void);
/* Number of completed frames since start. */
unsigned x11_out_frames(void);
/* Count of button and key presses so far (wakes the blanked screen). */
unsigned x11_out_inputs(void);
