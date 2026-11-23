/* Dashboard layout from e2-v5.html, CSS px x 2.57 (420 px mockup frame -> 1080 px). */
#include "ui.h"
#include "offline.h"
#include "ui_parts.h"
#include "bg.h"
#include "fonts.h"
#include "spark.h"
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define TILE_BORDER 2
#define TILE_PAD_H 30
#define TILE_PAD_V 22
#define CONTENT_W (1080 - 2 * SCR_PAD - 2 * (TILE_BORDER + TILE_PAD_H))
#define SPARK_W 216
#define SPARK_H 46
#define VALUE_W 247
#define BAR_H 10
#define CORES_H 87

#define C_CPU 0x5ec4ff
#define C_CPU1 0x3a8fc0
#define C_GPU 0x7ee787
#define C_MEM 0xd2a8ff
#define C_SYS 0xffb454
#define C_HOT 0xff6b6b

typedef struct {
    const char * id;
    int dec; /* decimals shown */
    float value, min, max, warn;
    bool has_warn;
    char unit[16], label[48];
    hist_t hist;
} metric_t;

static metric_t metrics[] = {
    {"cpu.clock_eff", 2}, {"cpu.bclk", 2}, {"cpu.fclk", 0}, {"cpu.vcore", 3}, {"cpu.tctl", 1},
    {"cpu.load", 0}, {"cpu.ppt", 0},
    {"cpu.core.0", 0}, {"cpu.core.1", 0}, {"cpu.core.2", 0}, {"cpu.core.3", 0},
    {"cpu.core.4", 0}, {"cpu.core.5", 0}, {"cpu.core.6", 0}, {"cpu.core.7", 0},
    {"cpu.core.8", 0}, {"cpu.core.9", 0}, {"cpu.core.10", 0}, {"cpu.core.11", 0},
    {"cpu.core.12", 0}, {"cpu.core.13", 0}, {"cpu.core.14", 0}, {"cpu.core.15", 0},
    {"gpu.clock", 0}, {"gpu.temp", 0}, {"gpu.mem_junction", 0}, {"gpu.load", 0}, {"gpu.vram_used", 1},
    {"gpu.power", 0},
    {"mem.used", 1}, {"mem.dimm.a1", 1}, {"mem.dimm.a2", 1}, {"mem.dimm.b1", 1}, {"mem.dimm.b2", 1},
    {"mem.mclk", 0}, {"mem.uclk", 0},
    {"sys.power", 0}, {"ssd.0.temp", 0}, {"ssd.1.temp", 0}, {"net.rx", 1}, {"net.tx", 1},
    /* The board itself, from `board` messages (PC-offline page). */
    {"board.rx", 1}, {"board.tx", 1}, {"board.soc", 1}, {"board.cpu", 0}, {"board.mem", 0},
};
#define NM (sizeof metrics / sizeof metrics[0])

typedef struct {
    metric_t * m;
    lv_obj_t * label, * value;
    spark_t * spark;
    lv_color_t color;
} row_t;

typedef struct {
    metric_t * m;
    lv_obj_t * track, * fill;
} bar_t;

/* Value labels that flash red above `warn`. */
typedef struct {
    metric_t * m;
    lv_obj_t * obj;
    bool hot;
} hotval_t;

typedef struct {
    lv_obj_t * obj, * dot, * label;
} chip_t;

static row_t rows[32];
static int nrows;
static bar_t bars[8];
static int nbars;
static hotval_t hots[40];
static int nhots;
static lv_obj_t * cores[16];
static lv_obj_t * dimm_val[4], * dimm_sep[4], * dimm_label;
static spark_t * mem_chart;
static lv_obj_t * legend[4], * legend_range;
static lv_obj_t * mclk_val;
static chip_t chips[2][3]; /* [main / offline][eth, pc, usb] */
static char pc_link[16] = "off", eth_ip[48];
static lv_obj_t * main_scr;

static const uint32_t dimm_colors[4] = {0xd2a8ff, 0xff9bd2, 0x8bc8ff, 0x7ee787};

static metric_t * find(const char * id)
{
    for(size_t i = 0; i < NM; i++)
        if(!strcmp(metrics[i].id, id)) return &metrics[i];
    return NULL;
}

lv_obj_t * ui_box(lv_obj_t * parent)
{
    lv_obj_t * o = lv_obj_create(parent);
    lv_obj_set_scrollable(o, false);
    lv_obj_set_clickable(o, false);
    lv_obj_set_size(o, LV_SIZE_CONTENT, LV_SIZE_CONTENT);
    return o;
}

lv_obj_t * ui_hbox(lv_obj_t * parent, lv_flex_align_t main, int gap)
{
    lv_obj_t * o = ui_box(parent);
    lv_obj_set_flex_flow(o, LV_FLEX_FLOW_ROW);
    lv_obj_set_flex_align(o, main, LV_FLEX_ALIGN_CENTER, LV_FLEX_ALIGN_CENTER);
    lv_obj_set_style_pad_column(o, gap, 0);
    return o;
}

lv_obj_t * ui_text(lv_obj_t * parent, const char * s, const lv_font_t * f, uint32_t color)
{
    lv_obj_t * l = lv_label_create(parent);
    lv_label_set_text(l, s);
    lv_obj_set_style_text_font(l, f, 0);
    lv_obj_set_style_text_color(l, lv_color_hex(color), 0);
    return l;
}

lv_obj_t * ui_tile(lv_obj_t * parent, int grow)
{
    lv_obj_t * t = ui_box(parent);
    lv_obj_set_width(t, lv_pct(100));
    lv_obj_set_flex_grow(t, grow);
    lv_obj_set_flex_flow(t, LV_FLEX_FLOW_COLUMN);
    lv_obj_set_flex_align(t, LV_FLEX_ALIGN_SPACE_BETWEEN, LV_FLEX_ALIGN_START, LV_FLEX_ALIGN_START);
    lv_obj_set_style_bg_color(t, lv_color_hex(0x141922), 0);
    lv_obj_set_style_bg_opa(t, 153, 0); /* .6 */
    lv_obj_set_style_border_color(t, lv_color_white(), 0);
    lv_obj_set_style_border_opa(t, 15, 0); /* .06 */
    lv_obj_set_style_border_width(t, TILE_BORDER, 0);
    lv_obj_set_style_radius(t, 18, 0);
    lv_obj_set_style_pad_hor(t, TILE_PAD_H, 0);
    lv_obj_set_style_pad_ver(t, TILE_PAD_V, 0);
    const char * blur = getenv("KVM_BLUR");
    if(!blur || strcmp(blur, "0")) { /* backdrop-filter: blur(6px) */
        lv_obj_set_style_blur_backdrop(t, true, 0);
        lv_obj_set_style_blur_radius(t, 15, 0);
    }
    return t;
}

lv_obj_t * ui_title(lv_obj_t * parent, const char * s, uint32_t color)
{
    lv_obj_t * l = ui_text(parent, s, font_title, color);
    lv_obj_set_style_text_letter_space(l, 2, 0);
    return l;
}

static void add_hot(metric_t * m, lv_obj_t * obj)
{
    hots[nhots++] = (hotval_t){m, obj, false};
}

void ui_add_row_ex(lv_obj_t * parent, const char * id, uint32_t color, int label_w, int spark_w, int value_w)
{
    row_t * r = &rows[nrows++];
    r->m = find(id);
    r->color = lv_color_hex(color);
    lv_obj_t * c = ui_hbox(parent, LV_FLEX_ALIGN_START, ROW_GAP);
    lv_obj_set_width(c, lv_pct(100));
    r->label = ui_text(c, r->m->label, font_body, C_TEXT);
    if(label_w > 0) lv_obj_set_width(r->label, label_w);
    else lv_obj_set_flex_grow(r->label, 1);
    lv_label_set_long_mode(r->label, LV_LABEL_LONG_MODE_CLIP);
    r->spark = spark_create(c, label_w > 0 ? spark_w : SPARK_W, SPARK_H, 8, 2);
    r->value = ui_text(c, "--", font_value, C_TEXT);
    lv_obj_set_width(r->value, value_w);
    lv_obj_set_style_text_align(r->value, LV_TEXT_ALIGN_RIGHT, 0);
    add_hot(r->m, r->value);
}

void ui_add_row(lv_obj_t * parent, const char * id, uint32_t color)
{
    ui_add_row_ex(parent, id, color, 0, 0, VALUE_W);
}

void ui_add_bar(lv_obj_t * parent, const char * id, uint32_t color)
{
    bar_t * b = &bars[nbars++];
    b->m = find(id);
    b->track = ui_box(parent);
    lv_obj_set_size(b->track, lv_pct(100), BAR_H);
    lv_obj_set_style_bg_color(b->track, lv_color_white(), 0);
    lv_obj_set_style_bg_opa(b->track, 20, 0); /* .08 */
    lv_obj_set_style_radius(b->track, BAR_H / 2, 0);
    b->fill = ui_box(b->track);
    lv_obj_set_size(b->fill, 0, BAR_H);
    lv_obj_set_style_bg_color(b->fill, lv_color_hex(color), 0);
    lv_obj_set_style_bg_opa(b->fill, LV_OPA_COVER, 0);
    lv_obj_set_style_radius(b->fill, BAR_H / 2, 0);
}

/* ---- status chips ---- */

typedef struct {
    float x1, y1, x2, y2;
} seg_t;

typedef struct {
    float rect[4]; /* x, y, w, h in the 12x12 SVG viewBox; w = 0: none */
    int nseg;
    seg_t seg[8];
} icon_t;

static const icon_t icon_eth = {{2, 3, 8, 6}, 3, {{4, 9, 4, 10.5f}, {6, 9, 6, 10.5f}, {8, 9, 8, 10.5f}}};
static const icon_t icon_pc = {{1.5f, 2, 9, 6}, 2, {{4, 10.5f, 8, 10.5f}, {6, 8, 6, 10.5f}}};
static const icon_t icon_usb = {{0}, 7, {{6, 1, 6, 9}, {4, 3, 6, 1}, {6, 1, 8, 3}, {3, 6, 3, 7.5f}, {3, 7.5f, 6, 9},
                                        {9, 5, 9, 6.5f}, {9, 6.5f, 6, 8}}};

static void icon_draw_cb(lv_event_t * e)
{
    lv_obj_t * o = lv_event_get_target(e);
    const icon_t * ic = lv_event_get_user_data(e);
    lv_layer_t * layer = lv_event_get_layer(e);
    lv_area_t a;
    lv_obj_get_coords(o, &a);
    float k = lv_area_get_width(&a) / 12.0f;
    lv_color_t col = lv_obj_get_style_text_color(o, 0);
    if(ic->rect[2] > 0) {
        lv_draw_rect_dsc_t rd;
        lv_draw_rect_dsc_init(&rd);
        rd.bg_opa = LV_OPA_TRANSP;
        rd.border_color = col;
        rd.border_width = 3;
        rd.radius = 3;
        lv_area_t r = {a.x1 + (int)(ic->rect[0] * k - 1.5f), a.y1 + (int)(ic->rect[1] * k - 1.5f),
                       a.x1 + (int)((ic->rect[0] + ic->rect[2]) * k + 1.5f), a.y1 + (int)((ic->rect[1] + ic->rect[3]) * k + 1.5f)};
        lv_draw_rect(layer, &rd, &r);
    }
    lv_draw_line_dsc_t ld;
    lv_draw_line_dsc_init(&ld);
    ld.color = col;
    ld.width = 3;
    ld.round_start = ld.round_end = 1;
    for(int i = 0; i < ic->nseg; i++) {
        ld.p1.x = a.x1 + ic->seg[i].x1 * k;
        ld.p1.y = a.y1 + ic->seg[i].y1 * k;
        ld.p2.x = a.x1 + ic->seg[i].x2 * k;
        ld.p2.y = a.y1 + ic->seg[i].y2 * k;
        lv_draw_line(layer, &ld);
    }
}

static void make_chip(chip_t * c, lv_obj_t * parent, const icon_t * icon)
{
    c->obj = ui_hbox(parent, LV_FLEX_ALIGN_START, 8);
    lv_obj_set_style_pad_ver(c->obj, 3, 0);
    lv_obj_set_style_pad_hor(c->obj, 15, 0);
    lv_obj_set_style_radius(c->obj, LV_RADIUS_CIRCLE, 0);
    lv_obj_set_style_bg_color(c->obj, lv_color_white(), 0);
    lv_obj_set_style_bg_opa(c->obj, 18, 0); /* .07 */
    c->dot = ui_box(c->obj);
    lv_obj_set_size(c->dot, 15, 15);
    lv_obj_set_style_radius(c->dot, LV_RADIUS_CIRCLE, 0);
    lv_obj_set_style_bg_opa(c->dot, LV_OPA_COVER, 0);
    lv_obj_set_style_shadow_width(c->dot, 13, 0);
    lv_obj_t * ic = ui_box(c->obj);
    lv_obj_set_size(ic, 28, 28);
    lv_obj_add_event_cb(ic, icon_draw_cb, LV_EVENT_DRAW_MAIN, (void *)icon);
    c->label = ui_text(c->obj, "", font_small, 0xc5cfdc);
    lv_obj_set_style_max_width(c->label, 230, 0);
    lv_label_set_long_mode(c->label, LV_LABEL_LONG_MODE_DOTS);
}

enum { CHIP_OK, CHIP_WARN, CHIP_OFF };

static void set_chip(chip_t * c, int st, const char * s)
{
    static const uint32_t dot[] = {0x3fb950, 0xffb454, 0x5a6270};
    lv_obj_set_style_bg_color(c->dot, lv_color_hex(dot[st]), 0);
    lv_obj_set_style_shadow_color(c->dot, lv_color_hex(dot[st]), 0);
    lv_obj_set_style_shadow_opa(c->dot, st == CHIP_OFF ? LV_OPA_TRANSP : LV_OPA_COVER, 0);
    lv_obj_set_style_text_color(c->obj, lv_color_hex(st == CHIP_OFF ? 0x6b7585 : 0xc5cfdc), 0);
    lv_obj_set_style_text_color(c->label, lv_color_hex(st == CHIP_OFF ? 0x6b7585 : 0xc5cfdc), 0);
    lv_label_set_text(c->label, s);
}

void ui_status_bar(lv_obj_t * parent, int set)
{
    lv_obj_t * sb = ui_hbox(parent, LV_FLEX_ALIGN_END, 13);
    make_chip(&chips[set][0], sb, &icon_eth);
    make_chip(&chips[set][1], sb, &icon_pc);
    make_chip(&chips[set][2], sb, &icon_usb);
    set_chip(&chips[set][0], CHIP_OFF, "ETH");
    set_chip(&chips[set][1], CHIP_OFF, "PC");
    set_chip(&chips[set][2], CHIP_OFF, "USB");
}

/* ---- layout ---- */

static void build_cpu(lv_obj_t * scr)
{
    lv_obj_t * t = ui_tile(scr, 20);
    lv_obj_t * top = ui_hbox(t, LV_FLEX_ALIGN_SPACE_BETWEEN, 10);
    lv_obj_set_width(top, lv_pct(100));
    ui_title(top, "CPU · RYZEN 9 9950X3D", C_CPU);
    ui_status_bar(top, 0);

    lv_obj_t * cc = ui_box(t);
    lv_obj_set_size(cc, lv_pct(100), CORES_H);
    lv_obj_set_flex_flow(cc, LV_FLEX_FLOW_ROW);
    lv_obj_set_flex_align(cc, LV_FLEX_ALIGN_START, LV_FLEX_ALIGN_END, LV_FLEX_ALIGN_END);
    lv_obj_set_style_pad_column(cc, 8, 0);
    for(int i = 0; i < 16; i++) {
        lv_obj_t * b = ui_box(cc);
        lv_obj_set_flex_grow(b, 1);
        lv_obj_set_height(b, 3);
        lv_obj_set_style_radius(b, 5, 0);
        lv_obj_set_style_bg_color(b, lv_color_hex(i < 8 ? C_CPU : C_CPU1), 0);
        lv_obj_set_style_bg_opa(b, LV_OPA_COVER, 0);
        cores[i] = b;
    }
    lv_obj_t * ccd = ui_hbox(t, LV_FLEX_ALIGN_SPACE_BETWEEN, 0);
    lv_obj_set_width(ccd, lv_pct(100));
    ui_text(ccd, "CCD0 · V-Cache", font_small, C_MUTED);
    ui_text(ccd, "CCD1", font_small, C_MUTED);

    ui_add_row(t, "cpu.clock_eff", C_CPU);
    ui_add_row(t, "cpu.bclk", C_CPU);
    ui_add_row(t, "cpu.fclk", C_CPU);
    ui_add_row(t, "cpu.vcore", C_CPU);
    ui_add_row(t, "cpu.tctl", C_CPU);
    ui_add_row(t, "cpu.load", C_CPU);
    ui_add_bar(t, "cpu.load", C_CPU);
    ui_add_row(t, "cpu.ppt", C_CPU);
}

static void build_gpu(lv_obj_t * scr)
{
    lv_obj_t * t = ui_tile(scr, 16);
    ui_title(t, "GPU · RTX 5090", C_GPU);
    ui_add_row(t, "gpu.clock", C_GPU);
    ui_add_row(t, "gpu.temp", C_GPU);
    ui_add_row(t, "gpu.mem_junction", C_GPU);
    ui_add_row(t, "gpu.load", C_GPU);
    ui_add_bar(t, "gpu.load", C_GPU);
    ui_add_row(t, "gpu.vram_used", C_GPU);
    ui_add_bar(t, "gpu.vram_used", C_GPU);
    ui_add_row(t, "gpu.power", C_GPU);
}

static void build_mem(lv_obj_t * scr)
{
    lv_obj_t * t = ui_tile(scr, 14);
    ui_title(t, "MEMORY · DDR5-6000 2×32G", C_MEM);
    ui_add_row(t, "mem.used", C_MEM);
    ui_add_bar(t, "mem.used", C_MEM);

    lv_obj_t * r = ui_hbox(t, LV_FLEX_ALIGN_SPACE_BETWEEN, 0);
    lv_obj_set_width(r, lv_pct(100));
    dimm_label = ui_text(r, "DIMM", font_body, C_TEXT);
    lv_obj_t * v = ui_hbox(r, LV_FLEX_ALIGN_END, 0);
    static const char * ids[4] = {"mem.dimm.a1", "mem.dimm.a2", "mem.dimm.b1", "mem.dimm.b2"};
    for(int i = 0; i < 4; i++) {
        dimm_val[i] = ui_text(v, "--", font_value, C_NA);
        add_hot(find(ids[i]), dimm_val[i]);
        /* labels drop leading/trailing spaces in flex rows, so pad instead */
        dimm_sep[i] = ui_text(v, "/", font_value, C_TEXT);
        lv_obj_set_style_pad_left(dimm_sep[i], 9, 0);
    }

    mem_chart = spark_create(t, CONTENT_W, 149, 10, 4);
    mem_chart->top = 3.0f / 58;
    mem_chart->base = 55.0f / 58;
    mem_chart->bottom = 57.0f / 58;
    mem_chart->area_opa = 0x1f;

    lv_obj_t * lg = ui_hbox(t, LV_FLEX_ALIGN_END, 26);
    lv_obj_set_width(lg, lv_pct(100));
    for(int i = 0; i < 4; i++) {
        legend[i] = ui_text(lg, "", font_small, dimm_colors[i]);
        lv_obj_set_hidden(legend[i], true);
    }
    legend_range = ui_text(lg, "last 60 s", font_small, C_MUTED);

    lv_obj_t * mc = ui_hbox(t, LV_FLEX_ALIGN_SPACE_BETWEEN, 0);
    lv_obj_set_width(mc, lv_pct(100));
    ui_text(mc, "MCLK / UCLK", font_body, C_TEXT);
    mclk_val = ui_text(mc, "-- / --", font_value, C_TEXT);
}

static void build_sys(lv_obj_t * scr)
{
    lv_obj_t * t = ui_tile(scr, 11);
    ui_title(t, "SYSTEM · B850M AORUS ELITE ICE", C_SYS);
    ui_add_row(t, "sys.power", C_SYS);
    ui_add_row(t, "ssd.0.temp", C_SYS);
    ui_add_row(t, "ssd.1.temp", C_SYS);
    ui_add_row(t, "net.rx", C_SYS);
    ui_add_row(t, "net.tx", C_SYS);
}

static const char * default_label(const char * id)
{
    static const struct {
        const char * id, * label;
    } l[] = {
        {"cpu.clock_eff", "Effective clock"}, {"cpu.bclk", "BCLK"}, {"cpu.fclk", "FCLK"}, {"cpu.vcore", "Vcore"},
        {"cpu.tctl", "Tctl"}, {"cpu.load", "Load"}, {"cpu.ppt", "PPT"}, {"gpu.clock", "Core"}, {"gpu.temp", "Temp"},
        {"gpu.mem_junction", "Mem junction"}, {"gpu.load", "Load"}, {"gpu.vram_used", "VRAM"}, {"gpu.power", "Power"},
        {"mem.used", "Used"}, {"sys.power", "System power"}, {"ssd.0.temp", "SSD 0"}, {"ssd.1.temp", "SSD 1"},
        {"net.rx", "Net ↓"}, {"net.tx", "Net ↑"}, {"board.rx", "↓ RX"}, {"board.tx", "↑ TX"},
        {"board.soc", "SoC temp"}, {"board.cpu", "CPU"}, {"board.mem", "Memory"},
    };
    for(size_t i = 0; i < sizeof l / sizeof l[0]; i++)
        if(!strcmp(l[i].id, id)) return l[i].label;
    return id;
}

void ui_create(lv_obj_t * scr, int w, int h)
{
    for(size_t i = 0; i < NM; i++) {
        metric_t * m = &metrics[i];
        m->value = NAN;
        m->min = 0;
        m->max = 100;
        snprintf(m->label, sizeof m->label, "%s", default_label(m->id));
        hist_init(&m->hist);
    }
    lv_obj_set_scrollable(scr, false);
    lv_obj_set_style_text_color(scr, lv_color_hex(C_TEXT), 0);
    lv_obj_set_style_text_font(scr, font_body, 0);

    /* The main screen is laid out for 1080x1920. The landscape layout (B) is not built yet, so in
     * landscape the portrait screen is shown scaled down and centred. */
    main_scr = ui_box(scr);
    lv_obj_set_size(main_scr, 1080, 1920);
    lv_obj_set_flex_flow(main_scr, LV_FLEX_FLOW_COLUMN);
    lv_obj_set_style_pad_all(main_scr, SCR_PAD, 0);
    lv_obj_set_style_pad_row(main_scr, TILE_GAP, 0);
    if(w > h) {
        lv_obj_set_style_transform_pivot_x(main_scr, 0, 0);
        lv_obj_set_style_transform_pivot_y(main_scr, 0, 0);
        lv_obj_set_style_transform_scale(main_scr, 256 * h / 1920, 0);
        lv_obj_set_x(main_scr, (w - 1080 * h / 1920) / 2);
    }
    build_cpu(main_scr);
    build_gpu(main_scr);
    build_mem(main_scr);
    build_sys(main_scr);
    offline_create(scr, w, h, main_scr);
}

/* ---- data ---- */

static void fmt_value(const metric_t * m, char * out, size_t n)
{
    if(isnan(m->value)) snprintf(out, n, "--");
    else if(!strcmp(m->id, "board.rx") || !strcmp(m->id, "board.tx")) {
        if(m->value < 1) snprintf(out, n, "%.0f KB/s", m->value * 1000);
        else snprintf(out, n, "%.1f MB/s", m->value);
    }
    else if(!strcmp(m->id, "board.mem")) snprintf(out, n, "%.0f / %.0f MB", m->value, m->max);
    else if(!strcmp(m->id, "gpu.vram_used") || !strcmp(m->id, "mem.used"))
        snprintf(out, n, "%.1f / %g %s", m->value, m->max, m->unit);
    else snprintf(out, n, "%.*f %s", m->dec, m->value, m->unit);
}

static float frac(const metric_t * m)
{
    if(isnan(m->value) || m->max <= m->min) return 0;
    return fminf(1, fmaxf(0, (m->value - m->min) / (m->max - m->min)));
}

static void anim_h(void * o, int32_t v)
{
    lv_obj_set_height(o, v);
}

static void anim_w(void * o, int32_t v)
{
    lv_obj_set_width(o, v);
}

/* CSS: transition .6s ease */
static void animate(lv_obj_t * o, lv_anim_exec_xcb_t cb, int32_t from, int32_t to)
{
    lv_anim_t a;
    lv_anim_init(&a);
    lv_anim_set_var(&a, o);
    lv_anim_set_exec_cb(&a, cb);
    lv_anim_set_values(&a, from, to);
    lv_anim_set_duration(&a, 600);
    lv_anim_set_path_cb(&a, lv_anim_path_ease_out);
    lv_anim_start(&a);
}

static bool is_board(const metric_t * m)
{
    return !strncmp(m->id, "board.", 6);
}

/* Rows and bars of the main screen (board = false) or of the offline page (board = true). */
static void refresh_rows(bool board)
{
    char s[64];
    for(int i = 0; i < nrows; i++) {
        row_t * r = &rows[i];
        if(is_board(r->m) != board) continue;
        if(strcmp(lv_label_get_text(r->label), r->m->label)) lv_label_set_text(r->label, r->m->label);
        fmt_value(r->m, s, sizeof s);
        lv_label_set_text(r->value, s);
        const hist_t * h = &r->m->hist;
        spark_draw(r->spark, 1, &h, &r->color, r->m->min, r->m->max);
    }
    for(int i = 0; i < nbars; i++) {
        bar_t * b = &bars[i];
        if(is_board(b->m) != board) continue;
        int w = (int)(lv_obj_get_content_width(b->track) * frac(b->m));
        animate(b->fill, anim_w, lv_obj_get_width(b->fill), w);
    }
}

static void refresh(void)
{
    char s[64];
    refresh_rows(false);
    for(int i = 0; i < 16; i++) {
        char id[16];
        snprintf(id, sizeof id, "cpu.core.%d", i);
        int h = (int)(CORES_H * frac(find(id)));
        if(h < 3) h = 3;
        animate(cores[i], anim_h, lv_obj_get_height(cores[i]), h);
    }

    /* DIMM row, big chart and legend */
    const hist_t * series[4];
    lv_color_t colors[4];
    int n = 0;
    float lo = 30, hi = 60;
    static const char * slot[4] = {"A1", "A2", "B1", "B2"};
    static const char * ids[4] = {"mem.dimm.a1", "mem.dimm.a2", "mem.dimm.b1", "mem.dimm.b2"};
    for(int i = 0; i < 4; i++) {
        metric_t * m = find(ids[i]);
        /* Only slots the PC reports are shown (design rule); empty slots take no space. */
        lv_obj_set_hidden(dimm_val[i], isnan(m->value));
        lv_obj_set_hidden(dimm_sep[i], isnan(m->value));
        if(isnan(m->value)) continue;
        snprintf(s, sizeof s, "%.1f", m->value);
        lv_label_set_text(dimm_val[i], s);
        lv_obj_set_style_text_color(dimm_val[i], lv_color_hex(C_TEXT), 0);
        lo = m->min;
        hi = m->max;
        colors[n] = lv_color_hex(dimm_colors[n]);
        series[n] = &m->hist;
        snprintf(s, sizeof s, "— %s", slot[i]);
        lv_label_set_text(legend[n], s);
        lv_obj_set_hidden(legend[n], false);
        n++;
    }
    for(int i = n; i < 4; i++) lv_obj_set_hidden(legend[i], true);
    /* "DIMM A2 / B2" and "44.8 / 46.8 °C": the last shown separator becomes the unit. */
    char lab[48] = "DIMM";
    int last = -1;
    for(int i = 0; i < 4; i++) {
        if(lv_obj_is_hidden(dimm_val[i])) continue;
        strcat(lab, last < 0 ? " " : " / ");
        strcat(lab, slot[i]);
        lv_label_set_text(dimm_sep[i], "/");
        lv_obj_set_style_pad_right(dimm_sep[i], 9, 0);
        last = i;
    }
    if(last >= 0) {
        lv_label_set_text(dimm_sep[last], "°C");
        lv_obj_set_style_pad_right(dimm_sep[last], 0, 0);
    }
    lv_label_set_text(dimm_label, lab);
    spark_draw(mem_chart, n, series, colors, lo, hi);
    snprintf(s, sizeof s, "%g–%g °C · last 60 s", lo, hi);
    lv_label_set_text(legend_range, s);

    metric_t * mc = find("mem.mclk"), * uc = find("mem.uclk");
    snprintf(s, sizeof s, "%.0f / %.0f", mc->value, uc->value);
    lv_label_set_text(mclk_val, isnan(mc->value) ? "-- / --" : s);

    if(!offline_active()) bg_set_load(fmaxf(find("cpu.load")->value, find("gpu.load")->value));
}

void ui_board_sample(const char * id, float value, float min, float max, const char * unit)
{
    metric_t * m = find(id);
    if(!m) return;
    snprintf(m->unit, sizeof m->unit, "%s", unit);
    m->value = value;
    m->min = min;
    m->max = max;
    hist_push(&m->hist, value);
}

void ui_board_refresh(void)
{
    refresh_rows(true);
}

const char * ui_pc_link(void)
{
    return pc_link;
}

const char * ui_ip(void)
{
    return eth_ip[0] ? eth_ip : NULL;
}

static void on_telemetry(const cJSON * msg)
{
    bool seen[NM] = {0};
    const cJSON * it;
    cJSON_ArrayForEach(it, cJSON_GetObjectItem(msg, "metrics"))
    {
        const cJSON * id = cJSON_GetObjectItem(it, "id");
        if(!cJSON_IsString(id)) continue;
        metric_t * m = find(id->valuestring);
        if(!m) continue;
        seen[m - metrics] = true;
        const cJSON * v = cJSON_GetObjectItem(it, "value");
        m->value = cJSON_IsNumber(v) ? (float)v->valuedouble : NAN;
        const cJSON * x;
        if(cJSON_IsNumber(x = cJSON_GetObjectItem(it, "min"))) m->min = (float)x->valuedouble;
        if(cJSON_IsNumber(x = cJSON_GetObjectItem(it, "max"))) m->max = (float)x->valuedouble;
        x = cJSON_GetObjectItem(it, "warn");
        m->has_warn = cJSON_IsNumber(x);
        if(m->has_warn) m->warn = (float)x->valuedouble;
        if(cJSON_IsString(x = cJSON_GetObjectItem(it, "unit"))) snprintf(m->unit, sizeof m->unit, "%s", x->valuestring);
        if(cJSON_IsString(x = cJSON_GetObjectItem(it, "label"))) snprintf(m->label, sizeof m->label, "%s", x->valuestring);
    }
    for(size_t i = 0; i < NM; i++) {
        if(is_board(&metrics[i])) continue;
        if(!seen[i]) metrics[i].value = NAN;
        hist_push(&metrics[i].hist, metrics[i].value);
    }
    refresh();
    const cJSON * ts = cJSON_GetObjectItem(msg, "ts");
    offline_telemetry(cJSON_IsNumber(ts) ? ts->valuedouble : NAN);
}

static const char * str(const cJSON * o, const char * k)
{
    const cJSON * x = cJSON_GetObjectItem(o, k);
    return cJSON_IsString(x) ? x->valuestring : NULL;
}

static void on_status(const cJSON * msg)
{
    char s[64];
    const cJSON * eth = cJSON_GetObjectItem(msg, "eth");
    const char * st = str(eth, "state");
    const cJSON * mb = cJSON_GetObjectItem(eth, "mbps");
    int mbps = cJSON_IsNumber(mb) ? mb->valueint : 0;
    char speed[16];
    if(mbps >= 1000) snprintf(speed, sizeof speed, "%dG", mbps / 1000);
    else snprintf(speed, sizeof speed, "%dM", mbps);
    const char * ip = str(eth, "ip");
    snprintf(eth_ip, sizeof eth_ip, "%s", st && strcmp(st, "down") && ip ? ip : "");
    int es = CHIP_OFF;
    if(st && !strcmp(st, "up")) {
        snprintf(s, sizeof s, "%s · %s", speed, ip ? ip : "?");
        es = CHIP_OK;
    }
    else if(st && !strcmp(st, "no_ip")) {
        snprintf(s, sizeof s, "%s · No IP", speed);
        es = CHIP_WARN;
    }
    else snprintf(s, sizeof s, "ETH down");

    st = str(cJSON_GetObjectItem(msg, "pc"), "state");
    snprintf(pc_link, sizeof pc_link, "%s", st ? st : "off");
    const char * dev = str(cJSON_GetObjectItem(msg, "usb_a"), "device");
    for(int set = 0; set < 2; set++) {
        chip_t * c = chips[set];
        set_chip(&c[0], es, s);
        /* On the offline page the host program is known to be missing: partial = HID only. */
        if(!strcmp(pc_link, "ok")) set_chip(&c[1], CHIP_OK, "PC");
        else if(!strcmp(pc_link, "partial")) set_chip(&c[1], CHIP_WARN, set ? "PC HID only" : "PC partial");
        else set_chip(&c[1], CHIP_OFF, "PC off");
        if(dev) set_chip(&c[2], CHIP_OK, dev);
        else set_chip(&c[2], CHIP_OFF, "No USB");
    }
}

void ui_on_message(const cJSON * msg)
{
    const char * t = str(msg, "t");
    if(!t) return;
    if(!strcmp(t, "telemetry")) on_telemetry(msg);
    else if(!strcmp(t, "status")) on_status(msg);
    else if(!strcmp(t, "board") || !strcmp(t, "pc") || !strcmp(t, "config") || !strcmp(t, "result")) offline_on_message(t, msg);
    else if(!strcmp(t, "hello")) printf("panel: hello %s\n", str(msg, "version") ? str(msg, "version") : "?");
}

/* CSS .hot: color pulses to #ff6b6b at 50% of a 1.2 s cycle. */
void ui_tick(void)
{
    float k = (1 - cosf(2 * (float)M_PI * (lv_tick_get() % 1200) / 1200.0f)) / 2;
    lv_color_t pulse = lv_color_mix(lv_color_hex(C_HOT), lv_color_hex(C_TEXT), (uint8_t)(k * 255));
    for(int i = 0; i < nhots; i++) {
        hotval_t * h = &hots[i];
        bool hot = h->m->has_warn && !isnan(h->m->value) && h->m->value > h->m->warn;
        if(hot) lv_obj_set_style_text_color(h->obj, pulse, 0);
        else if(h->hot) lv_obj_set_style_text_color(h->obj, lv_color_hex(C_TEXT), 0);
        h->hot = hot;
    }
    offline_tick();
}
