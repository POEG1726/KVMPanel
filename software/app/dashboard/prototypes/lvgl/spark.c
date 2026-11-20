#include "spark.h"
#include <math.h>
#include <stdlib.h>
#include <string.h>

#define BORDER 2

void hist_init(hist_t * h)
{
    for(int i = 0; i < HIST_N; i++) h->v[i] = NAN;
}

void hist_push(hist_t * h, float v)
{
    memmove(h->v, h->v + 1, sizeof(float) * (HIST_N - 1));
    h->v[HIST_N - 1] = v;
}

spark_t * spark_create(lv_obj_t * parent, int w, int h, int radius, int grid_lines)
{
    spark_t * s = calloc(1, sizeof *s);
    s->frame = lv_obj_create(parent);
    lv_obj_set_scrollable(s->frame, false);
    lv_obj_set_size(s->frame, w, h);
    lv_obj_set_style_bg_color(s->frame, lv_color_black(), 0);
    lv_obj_set_style_bg_opa(s->frame, 89, 0); /* .35 */
    lv_obj_set_style_border_color(s->frame, lv_color_white(), 0);
    lv_obj_set_style_border_opa(s->frame, 33, 0); /* .13 */
    lv_obj_set_style_border_width(s->frame, BORDER, 0);
    lv_obj_set_style_radius(s->frame, radius, 0);
    lv_obj_set_style_pad_all(s->frame, 0, 0);

    s->w = w - 2 * BORDER;
    s->h = h - 2 * BORDER;
    s->grid_lines = grid_lines;
    s->top = 2.0f / 18;
    s->base = 16.0f / 18;
    s->bottom = 17.0f / 18;
    s->line_w = 3;
    s->area_opa = 0x2e;
    s->buf = lv_draw_buf_create(s->w, s->h, LV_COLOR_FORMAT_ARGB8888, 0);
    lv_draw_buf_clear(s->buf, NULL);
    s->canvas = lv_canvas_create(s->frame);
    lv_canvas_set_draw_buf(s->canvas, s->buf);
    lv_obj_center(s->canvas);
    return s;
}

/* Non-premultiplied "source over" of (rgb, a) onto an ARGB8888 pixel. */
static void px_over(uint8_t * p, lv_color_t c, int a)
{
    if(a <= 0) return;
    int da = p[3];
    int oa = a + da * (255 - a) / 255;
    if(oa == 0) return;
    p[0] = (uint8_t)((c.blue * a + p[0] * da * (255 - a) / 255) / oa);
    p[1] = (uint8_t)((c.green * a + p[1] * da * (255 - a) / 255) / oa);
    p[2] = (uint8_t)((c.red * a + p[2] * da * (255 - a) / 255) / oa);
    p[3] = (uint8_t)oa;
}

void spark_draw(spark_t * s, int n, const hist_t * const * series, const lv_color_t * colors, float lo, float hi)
{
    uint8_t * data = s->buf->data;
    uint32_t stride = s->buf->header.stride;
    lv_draw_buf_clear(s->buf, NULL);

    for(int k = 1; k <= s->grid_lines; k++) {
        int y = (int)lroundf((float)k * s->h / (s->grid_lines + 1));
        for(int x = 0; x < s->w; x++) px_over(data + y * stride + x * 4, lv_color_white(), 18);
    }

    float ytop = s->top * s->h, ybase = s->base * s->h, yfloor = s->bottom * s->h;
    float step = (float)(s->w - 2) / (HIST_N - 1);

    /* Area fills first (per pixel column, anti-aliased top edge), then the lines on top. */
    for(int si = 0; si < n; si++) {
        const float * v = series[si]->v;
        for(int i = 0; i + 1 < HIST_N; i++) {
            if(isnan(v[i]) || isnan(v[i + 1])) continue;
            float f0 = fminf(1, fmaxf(0, (v[i] - lo) / (hi - lo)));
            float f1 = fminf(1, fmaxf(0, (v[i + 1] - lo) / (hi - lo)));
            float y0 = ybase - f0 * (ybase - ytop), y1 = ybase - f1 * (ybase - ytop);
            float x0 = 1 + i * step;
            int xa = (int)ceilf(x0), xb = (int)ceilf(x0 + step) - 1;
            for(int x = xa; x <= xb && x < s->w; x++) {
                float t = (x - x0) / step;
                float yl = y0 + (y1 - y0) * t;
                int yi = (int)yl;
                float frac = 1 - (yl - yi);
                for(int y = yi; y < (int)yfloor && y < s->h; y++) {
                    int a = y == yi ? (int)(s->area_opa * frac) : s->area_opa;
                    px_over(data + y * stride + x * 4, colors[si], a);
                }
            }
        }
    }

    lv_layer_t layer;
    lv_canvas_init_layer(s->canvas, &layer);
    lv_draw_line_dsc_t ld;
    lv_draw_line_dsc_init(&ld);
    ld.width = s->line_w;
    ld.round_start = 1;
    ld.round_end = 1;
    for(int si = 0; si < n; si++) {
        const float * v = series[si]->v;
        ld.color = colors[si];
        for(int i = 0; i + 1 < HIST_N; i++) {
            if(isnan(v[i]) || isnan(v[i + 1])) continue;
            float f0 = fminf(1, fmaxf(0, (v[i] - lo) / (hi - lo)));
            float f1 = fminf(1, fmaxf(0, (v[i + 1] - lo) / (hi - lo)));
            ld.p1.x = (lv_value_precise_t)lroundf(1 + i * step);
            ld.p1.y = (lv_value_precise_t)lroundf(ybase - f0 * (ybase - ytop));
            ld.p2.x = (lv_value_precise_t)lroundf(1 + (i + 1) * step);
            ld.p2.y = (lv_value_precise_t)lroundf(ybase - f1 * (ybase - ytop));
            lv_draw_line(&layer, &ld);
        }
    }
    lv_canvas_finish_layer(s->canvas, &layer);
    lv_obj_invalidate(s->canvas);
}
