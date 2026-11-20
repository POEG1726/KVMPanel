#pragma once
#include "lvgl.h"

/* E2 background drawn on `scr`: dark base, three pre-blurred colour blobs drifting, scrolling grid. */
void bg_init(lv_obj_t * scr);
/* Advance the animation (call once per frame); invalidates the whole screen. */
void bg_tick(void);
/* 0..100: max(cpu.load, gpu.load); raises blob brightness. */
void bg_set_load(float load);
