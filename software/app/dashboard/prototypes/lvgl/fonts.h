#pragma once
#include "lvgl.h"

/* Sizes are the mockup's CSS px x 2.57 (420 px mockup frame -> 1080 px panel). */
extern const lv_font_t * font_body;  /* 32 px regular: row labels */
extern const lv_font_t * font_value; /* 32 px semibold: values */
extern const lv_font_t * font_title; /* 26 px regular: tile titles */
extern const lv_font_t * font_small; /* 23 px regular: chips, CCD labels, legend */
/* PC-offline page */
extern const lv_font_t * font_brand; /* 33 px semibold: "KVMPanel" */
extern const lv_font_t * font_hero;  /* 53 px semibold: "PC is off" */
extern const lv_font_t * font_sub;   /* 27 px regular: hero subtitle, button */
extern const lv_font_t * font_url;   /* 37 px regular: access URL */
extern const lv_font_t * font_ip;    /* 84 px semibold: IP (portrait) */
extern const lv_font_t * font_ip_l;  /* 74 px semibold: IP (landscape column) */

void fonts_init(void);
