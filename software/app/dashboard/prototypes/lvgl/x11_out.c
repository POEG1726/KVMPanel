/* Tiny X11 backend. LVGL's own X11 driver opens a window the size of the display (1080x1920 does not
 * fit on most monitors), so this one renders full resolution in DIRECT mode and box-filters it down. */
#include "x11_out.h"
#include <X11/Xlib.h>
#include <X11/Xutil.h>
#include <X11/keysym.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static Display * dpy;
static Window win;
static GC gc;
static XImage * img;
static Atom wm_delete;
static int W, H, S, WW, WH;
static uint32_t * fb;
static unsigned frames;
static bool alive = true;
/* Input: left button = touch, Return/space = OK, Tab/arrows move key focus. */
static int32_t ptr_x, ptr_y;
static bool ptr_down;
static uint32_t key_last;
static bool key_down;
static unsigned inputs;

static void pointer_read(lv_indev_t * indev, lv_indev_data_t * d)
{
    (void)indev;
    d->point.x = ptr_x;
    d->point.y = ptr_y;
    d->state = ptr_down ? LV_INDEV_STATE_PRESSED : LV_INDEV_STATE_RELEASED;
}

static void keypad_read(lv_indev_t * indev, lv_indev_data_t * d)
{
    (void)indev;
    d->key = key_last;
    d->state = key_down ? LV_INDEV_STATE_PRESSED : LV_INDEV_STATE_RELEASED;
}

static void convert(int x1, int y1, int x2, int y2)
{
    uint32_t * dst = (uint32_t *)img->data;
    int dstride = img->bytes_per_line / 4;
    if(S == 1) {
        for(int y = y1; y <= y2; y++) memcpy(dst + y * dstride + x1, fb + y * W + x1, (x2 - x1 + 1) * 4);
        return;
    }
    for(int y = y1; y <= y2; y++) {
        const uint32_t * r0 = fb + (y * 2) * W;
        const uint32_t * r1 = r0 + W;
        for(int x = x1; x <= x2; x++) {
            uint32_t a = r0[2 * x], b = r0[2 * x + 1], c = r1[2 * x], d = r1[2 * x + 1];
            uint32_t rb = ((a & 0xff00ff) + (b & 0xff00ff) + (c & 0xff00ff) + (d & 0xff00ff)) >> 2;
            uint32_t g = ((a & 0x00ff00) + (b & 0x00ff00) + (c & 0x00ff00) + (d & 0x00ff00)) >> 2;
            dst[y * dstride + x] = (rb & 0xff00ff) | (g & 0x00ff00);
        }
    }
}

static void flush_cb(lv_display_t * disp, const lv_area_t * a, uint8_t * px)
{
    (void)px;
    int x1 = a->x1 / S, y1 = a->y1 / S, x2 = a->x2 / S, y2 = a->y2 / S;
    if(x2 >= WW) x2 = WW - 1;
    if(y2 >= WH) y2 = WH - 1;
    convert(x1, y1, x2, y2);
    XPutImage(dpy, win, gc, img, x1, y1, x1, y1, x2 - x1 + 1, y2 - y1 + 1);
    if(lv_display_flush_is_last(disp)) {
        XFlush(dpy);
        frames++;
    }
    lv_display_flush_ready(disp);
}

lv_display_t * x11_out_create(int w, int h, int scale)
{
    W = w; H = h; S = scale == 1 ? 1 : 2;
    WW = W / S; WH = H / S;
    dpy = XOpenDisplay(NULL);
    if(!dpy) {
        fprintf(stderr, "cannot open X display\n");
        exit(1);
    }
    int scr = DefaultScreen(dpy);
    win = XCreateSimpleWindow(dpy, RootWindow(dpy, scr), 0, 0, WW, WH, 0, 0, 0);
    XStoreName(dpy, win, "KVMPanel dashboard (LVGL)");
    XSizeHints hints = {.flags = PMinSize | PMaxSize, .min_width = WW, .max_width = WW, .min_height = WH, .max_height = WH};
    XSetWMNormalHints(dpy, win, &hints);
    wm_delete = XInternAtom(dpy, "WM_DELETE_WINDOW", False);
    XSetWMProtocols(dpy, win, &wm_delete, 1);
    XSelectInput(dpy, win, ExposureMask | StructureNotifyMask | ButtonPressMask | ButtonReleaseMask | PointerMotionMask |
                               KeyPressMask | KeyReleaseMask);
    gc = XCreateGC(dpy, win, 0, NULL);
    img = XCreateImage(dpy, DefaultVisual(dpy, scr), DefaultDepth(dpy, scr), ZPixmap, 0,
                       calloc((size_t)WW * WH, 4), WW, WH, 32, 0);
    XMapWindow(dpy, win);
    XFlush(dpy);

    lv_display_t * disp = lv_display_create(W, H);
    lv_display_set_color_format(disp, LV_COLOR_FORMAT_XRGB8888);
    size_t sz = (size_t)W * H * 4;
    fb = aligned_alloc(64, sz);
    lv_display_set_buffers(disp, fb, NULL, sz, LV_DISPLAY_RENDER_MODE_DIRECT);
    lv_display_set_flush_cb(disp, flush_cb);

    lv_indev_t * ptr = lv_indev_create();
    lv_indev_set_type(ptr, LV_INDEV_TYPE_POINTER);
    lv_indev_set_read_cb(ptr, pointer_read);
    lv_group_t * g = lv_group_create();
    lv_group_set_default(g);
    lv_indev_t * kp = lv_indev_create();
    lv_indev_set_type(kp, LV_INDEV_TYPE_KEYPAD);
    lv_indev_set_read_cb(kp, keypad_read);
    lv_indev_set_group(kp, g);
    return disp;
}

bool x11_out_pump(void)
{
    while(XPending(dpy)) {
        XEvent ev;
        XNextEvent(dpy, &ev);
        if(ev.type == Expose) XPutImage(dpy, win, gc, img, 0, 0, 0, 0, WW, WH);
        else if(ev.type == ClientMessage && (Atom)ev.xclient.data.l[0] == wm_delete) alive = false;
        else if((ev.type == ButtonPress || ev.type == ButtonRelease) && ev.xbutton.button == Button1) {
            ptr_x = ev.xbutton.x * S;
            ptr_y = ev.xbutton.y * S;
            ptr_down = ev.type == ButtonPress;
            inputs++;
        }
        else if(ev.type == MotionNotify) {
            ptr_x = ev.xmotion.x * S;
            ptr_y = ev.xmotion.y * S;
        }
        else if(ev.type == KeyPress || ev.type == KeyRelease) {
            KeySym k = XLookupKeysym(&ev.xkey, 0);
            uint32_t key = k == XK_Return || k == XK_KP_Enter || k == XK_space ? LV_KEY_ENTER
                         : k == XK_Tab || k == XK_Down || k == XK_Right    ? LV_KEY_NEXT
                         : k == XK_Up || k == XK_Left                      ? LV_KEY_PREV
                         : k == XK_Escape                                  ? LV_KEY_ESC
                                                                           : 0;
            if(ev.type == KeyPress) inputs++;
            if(!key) continue;
            key_last = key;
            key_down = ev.type == KeyPress;
        }
    }
    return alive;
}

unsigned x11_out_frames(void)
{
    return frames;
}

unsigned x11_out_inputs(void)
{
    return inputs;
}
