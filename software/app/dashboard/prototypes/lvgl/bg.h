#pragma once
#include "lvgl.h"

/* E2 background drawn on `scr` (w x h): dark base and three pre-blurred colour blobs drifting. */
void bg_init(lv_obj_t * scr, int w, int h);
/* Advance the animation (call once per frame); invalidates the whole screen. */
void bg_tick(void);
/* 0..100. Main screen: max(cpu.load, gpu.load); calm mode: the board's own CPU load. */
void bg_set_load(float load);
/* Calm mode for the PC-offline page: teal instead of green, dimmer. Call before bg_set_load. */
void bg_set_calm(bool on);
/* Stop redrawing while the screen is blanked. */
void bg_set_paused(bool on);
