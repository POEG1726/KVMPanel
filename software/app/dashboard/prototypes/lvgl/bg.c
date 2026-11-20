/* Background "E2" from e2-v5.html, scaled 420 -> 1080 px (x2.57).
 * CSS: .bg inset -20%, filter blur(40px), opacity .3 + .5 * load; blobs 55% wide, mix-blend screen,
 * alternate ease-in-out drift. Here each blob is pre-rendered once as a blurred disc (ARGB8888) and
 * only its position changes per frame; it is drawn with additive blending (close to "screen" on a
 * dark base). The CSS scale() part of the drift is not reproduced (scaled image draws are much slower).
 * Grid: 24 px (62 px) cells, rgba(94,196,255,.25) x opacity .25, scrolls one cell per 8 s, fades in
 * over the top 30% and out over the bottom 30%. */
#include "bg.h"
#include <math.h>

#define SCR_W 1080
#define SCR_H 1920
#define BG_X (-SCR_W / 5) /* inset -20% */
#define BG_Y (-SCR_H / 5)
#define BG_W (SCR_W * 7 / 5)
#define BG_H (SCR_H * 7 / 5)
#define BLOB_D (BG_W * 55 / 100)
#define BLUR_SIGMA 103.0 /* 40 px CSS blur x 2.57 */
#define BLOB_PAD 280     /* alpha < 1/255 beyond ~2.7 sigma */
#define BLOB_S (BLOB_D + 2 * BLOB_PAD)
#define GRID_STEP 61.71f
#define GRID_PERIOD_MS 8000

typedef struct {
    uint32_t color;
    int x, y;       /* top-left of the disc at t = 0 */
    int dx, dy;     /* CSS translate() target */
    uint32_t period;
    lv_draw_buf_t * img;
} blob_t;

static blob_t blobs[3] = {
    {0x1e6fd9, BG_X + BG_W * 5 / 100, BG_Y + BG_H * 5 / 100, BLOB_D * 40 / 100, BLOB_D * 60 / 100, 18000, NULL},
    {0x1fa36b, BG_X + BG_W - BLOB_D, BG_Y + BG_H * 40 / 100, -BLOB_D * 50 / 100, -BLOB_D * 40 / 100, 22000, NULL},
    {0x8a4fd8, BG_X + BG_W * 15 / 100, BG_Y + BG_H - BLOB_D, BLOB_D * 30 / 100, -BLOB_D * 70 / 100, 26000, NULL},
};

static lv_obj_t * screen;
static float intensity = 0.55f, target = 0.55f;

static lv_draw_buf_t * make_blob(uint32_t rgb)
{
    lv_draw_buf_t * b = lv_draw_buf_create(BLOB_S, BLOB_S, LV_COLOR_FORMAT_ARGB8888, 0);
    const double c = BLOB_S / 2.0, r0 = BLOB_D / 2.0, k = 1.0 / (BLUR_SIGMA * sqrt(2.0));
    for(int y = 0; y < BLOB_S; y++) {
        uint8_t * row = b->data + y * b->header.stride;
        for(int x = 0; x < BLOB_S; x++) {
            double r = hypot(x + 0.5 - c, y + 0.5 - c);
            double a = 0.5 * erfc((r - r0) * k);
            row[x * 4 + 0] = rgb & 0xff;
            row[x * 4 + 1] = (rgb >> 8) & 0xff;
            row[x * 4 + 2] = (rgb >> 16) & 0xff;
            row[x * 4 + 3] = (uint8_t)lround(a * 255.0);
        }
    }
    return b;
}

/* Top/bottom fade of the grid mask: transparent -> opaque at 30%, opaque -> transparent from 70%. */
static float grid_mask(float y)
{
    float t = y / SCR_H;
    if(t <= 0 || t >= 1) return 0;
    if(t < 0.3f) return t / 0.3f;
    if(t > 0.7f) return (1 - t) / 0.3f;
    return 1;
}

static void draw_cb(lv_event_t * e)
{
    lv_layer_t * layer = lv_event_get_layer(e);
    uint32_t now = lv_tick_get();

    lv_draw_image_dsc_t id;
    lv_draw_image_dsc_init(&id);
    id.opa = (lv_opa_t)(intensity * 255);
    id.blend_mode = LV_BLEND_MODE_ADDITIVE;
    for(int i = 0; i < 3; i++) {
        blob_t * b = &blobs[i];
        float p = fmodf((float)now / b->period, 2.0f);
        if(p > 1) p = 2 - p;                       /* alternate */
        float ease = (1 - cosf((float)M_PI * p)) / 2; /* ease-in-out */
        int x = b->x + (int)(b->dx * ease) - BLOB_PAD;
        int y = b->y + (int)(b->dy * ease) - BLOB_PAD;
        lv_area_t a = {x, y, x + BLOB_S - 1, y + BLOB_S - 1};
        id.src = b->img;
        lv_draw_image(layer, &id, &a);
    }

    lv_draw_rect_dsc_t rd;
    lv_draw_rect_dsc_init(&rd);
    rd.bg_color = lv_color_hex(0x5ec4ff);
    const int max_opa = 16; /* .25 * .25 */
    float off = (now % GRID_PERIOD_MS) * GRID_STEP / GRID_PERIOD_MS;
    for(float y = off - GRID_STEP; y < SCR_H; y += GRID_STEP) {
        int yi = (int)y;
        rd.bg_opa = (lv_opa_t)(max_opa * grid_mask(yi));
        if(!rd.bg_opa) continue;
        lv_area_t a = {0, yi, SCR_W - 1, yi + 1};
        lv_draw_rect(layer, &rd, &a);
    }
    /* Vertical lines are static; draw them in short segments so the fade mask applies. */
    const int seg = 48;
    for(int s = 0; s < SCR_H; s += seg) {
        rd.bg_opa = (lv_opa_t)(max_opa * grid_mask(s + seg / 2));
        if(!rd.bg_opa) continue;
        for(float x = 0; x < SCR_W; x += GRID_STEP) {
            lv_area_t a = {(int)x, s, (int)x + 1, s + seg - 1};
            lv_draw_rect(layer, &rd, &a);
        }
    }
}

void bg_init(lv_obj_t * scr)
{
    screen = scr;
    lv_obj_set_style_bg_color(scr, lv_color_hex(0x07090d), 0);
    lv_obj_set_style_bg_opa(scr, LV_OPA_COVER, 0);
    for(int i = 0; i < 3; i++) blobs[i].img = make_blob(blobs[i].color);
    lv_obj_add_event_cb(scr, draw_cb, LV_EVENT_DRAW_MAIN_END, NULL);
}

void bg_tick(void)
{
    intensity += (target - intensity) * 0.05f;
    lv_obj_invalidate(screen);
}

void bg_set_load(float load)
{
    if(load < 0) load = 0;
    if(load > 100) load = 100;
    target = 0.3f + 0.5f * load / 100.0f;
}
