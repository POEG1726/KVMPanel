/* LVGL dashboard prototype (spike). Env:
 *   KVM_PANEL_ADDR   host:port of the panel socket (default 127.0.0.1:7701)
 *   KVM_SCALE        window downscale, 1 or 2 (default 2 -> 540x960 window, rendering stays 1080x1920)
 *   KVM_LANDSCAPE    1 = 1920x1080 screen (panel mounted sideways)
 *   KVM_FPS          frame cap (default 30, 0 = uncapped)
 *   KVM_BLUR         0 disables the backdrop blur on the tiles (default on)
 *   KVM_SNAPSHOT     write a full-resolution PPM of the screen here ...
 *   KVM_SNAPSHOT_AFTER  ... after this many seconds (default 65)
 *   KVM_EXIT_AFTER   quit after this many seconds */
#include "bg.h"
#include "fonts.h"
#include "lvgl.h"
#include "panel_sock.h"
#include "ui.h"
#include "x11_out.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

static uint32_t tick_ms(void)
{
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint32_t)(ts.tv_sec * 1000 + ts.tv_nsec / 1000000);
}

static int env_int(const char * k, int def)
{
    const char * v = getenv(k);
    return v ? atoi(v) : def;
}

static void frame_cb(lv_timer_t * t)
{
    (void)t;
    bg_tick();
    ui_tick();
}

static void stats_cb(lv_timer_t * t)
{
    static unsigned last;
    static uint32_t last_ms;
    unsigned f = x11_out_frames();
    uint32_t now = lv_tick_get();
    if(last_ms) printf("fps %.1f\n", (f - last) * 1000.0 / (now - last_ms));
    fflush(stdout);
    last = f;
    last_ms = now;
    (void)t;
}

static void snapshot_cb(lv_timer_t * t)
{
    const char * path = lv_timer_get_user_data(t);
    lv_draw_buf_t * b = lv_snapshot_take(lv_screen_active(), LV_COLOR_FORMAT_RGB888);
    FILE * f = b ? fopen(path, "wb") : NULL;
    if(f) {
        int w = b->header.w, h = b->header.h;
        fprintf(f, "P6\n%d %d\n255\n", w, h);
        uint8_t * row = malloc(w * 3);
        for(int y = 0; y < h; y++) {
            const uint8_t * s = b->data + y * b->header.stride;
            for(int x = 0; x < w; x++) { /* LVGL RGB888 is B,G,R in memory */
                row[x * 3] = s[x * 3 + 2];
                row[x * 3 + 1] = s[x * 3 + 1];
                row[x * 3 + 2] = s[x * 3];
            }
            fwrite(row, 1, w * 3, f);
        }
        free(row);
        fclose(f);
        printf("snapshot %dx%d -> %s\n", w, h, path);
    }
    else fprintf(stderr, "snapshot failed\n");
    if(b) lv_draw_buf_destroy(b);
    lv_timer_delete(t);
}

static void exit_cb(lv_timer_t * t)
{
    (void)t;
    exit(0);
}

int main(void)
{
    lv_init();
    lv_tick_set_cb(tick_ms);

    int fps = env_int("KVM_FPS", 30);
    uint32_t period = fps > 0 ? 1000 / fps : 1;
    const bool landscape = env_int("KVM_LANDSCAPE", 0) != 0;
    const int w = landscape ? 1920 : 1080, h = landscape ? 1080 : 1920;
    lv_display_t * disp = x11_out_create(w, h, env_int("KVM_SCALE", 2));
    lv_timer_set_period(lv_display_get_refr_timer(disp), period);

    fonts_init();
    lv_obj_t * scr = lv_screen_active();
    bg_init(scr, w, h);
    ui_create(scr, w, h);

    char host[64] = "127.0.0.1";
    int port = 7701;
    const char * addr = getenv("KVM_PANEL_ADDR");
    if(addr) sscanf(addr, "%63[^:]:%d", host, &port);
    panel_sock_start(host, port, ui_on_message);

    lv_timer_create(frame_cb, period, NULL);
    lv_timer_create(stats_cb, 5000, NULL);
    if(getenv("KVM_SNAPSHOT"))
        lv_timer_create(snapshot_cb, env_int("KVM_SNAPSHOT_AFTER", 65) * 1000, (void *)getenv("KVM_SNAPSHOT"));
    if(env_int("KVM_EXIT_AFTER", 0) > 0) lv_timer_create(exit_cb, env_int("KVM_EXIT_AFTER", 0) * 1000, NULL);

    while(x11_out_pump()) {
        uint32_t idle = lv_timer_handler();
        if(idle > 5) idle = 5;
        if(idle) usleep(idle * 1000);
    }
    return 0;
}
