/* Background "E2" from e2-v5.html, scaled 420 -> 1080 px (x2.57) on the short side.
 * CSS: .bg inset -20%, filter blur(40px), opacity .3 + .5 * load; blobs 55% wide, mix-blend screen,
 * alternate ease-in-out drift. Here each blob is pre-rendered once as a blurred disc (ARGB8888) and
 * only its position changes per frame; it is drawn with additive blending (close to "screen" on a
 * dark base). The CSS scale() part of the drift is not reproduced (scaled image draws are much slower).
 * The scrolling grid of the first mockups was removed from the design.
 * Calm mode (PC-offline page): the green blob turns teal and the intensity follows the board's CPU. */
#include "bg.h"
#include <math.h>

#define BLUR_SIGMA 103.0 /* 40 px CSS blur x 2.57 */
#define BLOB_PAD 280     /* alpha < 1/255 beyond ~2.7 sigma */

typedef struct {
    uint32_t color;
    int x, y;       /* top-left of the disc at t = 0 */
    int dx, dy;     /* CSS translate() target */
    uint32_t period;
    lv_draw_buf_t * img;
} blob_t;

static blob_t blobs[3];
static lv_draw_buf_t * teal; /* blob 2 in calm mode, made on first use */
static int blob_d, blob_s;
static lv_obj_t * screen;
static float intensity = 0.55f, target = 0.55f;
static bool calm, paused;

static lv_draw_buf_t * make_blob(uint32_t rgb)
{
    lv_draw_buf_t * b = lv_draw_buf_create(blob_s, blob_s, LV_COLOR_FORMAT_ARGB8888, 0);
    const double c = blob_s / 2.0, r0 = blob_d / 2.0, k = 1.0 / (BLUR_SIGMA * sqrt(2.0));
    for(int y = 0; y < blob_s; y++) {
        uint8_t * row = b->data + y * b->header.stride;
        for(int x = 0; x < blob_s; x++) {
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
        lv_area_t a = {x, y, x + blob_s - 1, y + blob_s - 1};
        id.src = calm && i == 1 ? teal : b->img;
        lv_draw_image(layer, &id, &a);
    }
}

void bg_init(lv_obj_t * scr, int w, int h)
{
    screen = scr;
    lv_obj_set_style_bg_color(scr, lv_color_hex(0x07090d), 0);
    lv_obj_set_style_bg_opa(scr, LV_OPA_COVER, 0);

    /* CSS box inset -20% (1.4 x the screen); blob = 55% of the box width. In landscape that would
     * be 1478 px discs (~50 MB of images), so the disc size follows the short side as in portrait. */
    const int bx = -w / 5, by = -h / 5, bw = w * 7 / 5, bh = h * 7 / 5;
    const int short_side = w < h ? w : h;
    blob_d = short_side * 7 / 5 * 55 / 100;
    blob_s = blob_d + 2 * BLOB_PAD;
    const int d = blob_d;
    blobs[0] = (blob_t){0x1e6fd9, bx + bw * 5 / 100, by + bh * 5 / 100, d * 40 / 100, d * 60 / 100, 18000, NULL};
    blobs[1] = (blob_t){0x1fa36b, bx + bw - d, by + bh * 40 / 100, -d * 50 / 100, -d * 40 / 100, 22000, NULL};
    blobs[2] = (blob_t){0x8a4fd8, bx + bw * 15 / 100, by + bh - d, d * 30 / 100, -d * 70 / 100, 26000, NULL};
    for(int i = 0; i < 3; i++) blobs[i].img = make_blob(blobs[i].color);
    lv_obj_add_event_cb(scr, draw_cb, LV_EVENT_DRAW_MAIN_END, NULL);
}

void bg_tick(void)
{
    if(paused) return;
    intensity += (target - intensity) * 0.05f;
    lv_obj_invalidate(screen);
}

void bg_set_load(float load)
{
    if(isnan(load) || load < 0) load = 0;
    if(load > 100) load = 100;
    target = calm ? 0.18f + 0.25f * load / 100.0f : 0.3f + 0.5f * load / 100.0f;
}

void bg_set_calm(bool on)
{
    if(on && !teal) teal = make_blob(0x1a8f86);
    calm = on;
}

void bg_set_paused(bool on)
{
    paused = on;
}
