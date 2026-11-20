#pragma once
#include "lvgl.h"

/* Sizes are the mockup's CSS px x 2.57 (420 px mockup frame -> 1080 px panel). */
extern const lv_font_t * font_body;  /* 32 px regular: row labels */
extern const lv_font_t * font_value; /* 32 px semibold: values */
extern const lv_font_t * font_title; /* 26 px regular: tile titles */
extern const lv_font_t * font_small; /* 23 px regular: chips, CCD labels, legend */

void fonts_init(void);
