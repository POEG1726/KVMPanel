#pragma once
#include "lvgl.h"

#define HIST_N 60

/* Last 60 samples, oldest first; NAN = no data. */
typedef struct {
    float v[HIST_N];
} hist_t;

void hist_init(hist_t * h);
void hist_push(hist_t * h, float v);

/* Framed chart (dark bg, thin border, faint grid lines). The plot is rendered into a canvas only when
 * data changes (1 Hz); per frame LVGL just blits it. */
typedef struct {
    lv_obj_t * frame;
    lv_obj_t * canvas;
    lv_draw_buf_t * buf;
    int w, h;          /* canvas size */
    int grid_lines;    /* evenly spaced horizontal lines */
    float top, base, bottom; /* fractions of h: value max, value min, area floor */
    int line_w;
    lv_opa_t area_opa;
} spark_t;

spark_t * spark_create(lv_obj_t * parent, int w, int h, int radius, int grid_lines);
void spark_draw(spark_t * s, int n, const hist_t * const * series, const lv_color_t * colors, float lo, float hi);
