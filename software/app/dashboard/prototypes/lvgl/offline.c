/* PC-offline page. Portrait: tiles stacked top to bottom. Landscape: big state (two columns) and
 * Remote access on the first row, Network / KVM / board on the second. Sizes are the mockup's CSS px
 * x 2.57 like the main screen. */
#include "offline.h"
#include "bg.h"
#include "fonts.h"
#include "panel_sock.h"
#include "ui_parts.h"
#include "x11_out.h"
#include <math.h>
#include <stdio.h>
#include <string.h>

#define C_ORANGE 0xffb454
#define C_GREEN 0x7ee787
#define C_FOCUS 0x2f6fd6
#define TELEMETRY_TIMEOUT_MS 3000
#define ICON_P 154 /* 60 px mockup icon, portrait */
#define ICON_L 139 /* 54 px, landscape */
#define ROW1_H 270 /* landscape: height of the state / access row */

static lv_obj_t * page, * main_obj, * blank;
static bool landscape;

static lv_obj_t * host_lbl, * icon, * spinner, * state_lbl, * sub_lbl, * btn, * btn_lbl, * btn_icon;
static lv_obj_t * ip_lbl, * url_lbl, * ts_row, * ts_val;
static lv_obj_t * link_val, * mac_val, * eth_title;
static lv_obj_t * sess_val, * hdmi_val, * stream_val, * media_val, * usb_val;
static lv_obj_t * sd_val, * up_val, * cd_lbl;

/* State */
static bool have_board, pc_connected, shown_offline = true;
static char pc_power[16] = "unknown";
static double last_seen_ts = NAN, board_ts = NAN;
static uint32_t last_telemetry_tick;
static int sleep_min = 5, countdown = -1;
static uint32_t cd_tick;
static bool screen_off;
static unsigned inputs_seen;
static uint32_t btn_enable_at;
static int next_id = 1, power_id = -1;
enum { PW_IDLE, PW_PENDING, PW_FAILED };
static int power_state = PW_IDLE;
static uint32_t power_failed_until;

static void wake(void);

/* ---- widgets ---------------------------------------------------------------- */

static void set_dim(lv_obj_t * l, bool dim)
{
    lv_obj_set_style_text_color(l, lv_color_hex(dim ? C_DIM : C_TEXT), 0);
}

/* "label ........ value" row; returns the value label. */
static lv_obj_t * kv(lv_obj_t * parent, const char * label, lv_obj_t ** row)
{
    lv_obj_t * r = ui_hbox(parent, LV_FLEX_ALIGN_SPACE_BETWEEN, ROW_GAP);
    lv_obj_set_width(r, lv_pct(100));
    lv_obj_t * l = ui_text(r, label, font_body, C_LABEL);
    lv_obj_set_flex_grow(l, 0);
    lv_obj_t * v = ui_text(r, "--", font_value, C_TEXT);
    lv_obj_set_flex_grow(v, 1);
    lv_obj_set_style_text_align(v, LV_TEXT_ALIGN_RIGHT, 0);
    lv_label_set_long_mode(v, LV_LABEL_LONG_MODE_DOTS);
    if(row) *row = r;
    return v;
}

static void row(lv_obj_t * parent, const char * id)
{
    /* Landscape columns are 556 px inside a tile: fixed label, the sparkline takes what is left. */
    if(landscape) ui_add_row_ex(parent, id, C_TEAL, 170, 556 - 170 - 200 - 2 * ROW_GAP, 200);
    else ui_add_row(parent, id, C_TEAL);
}

/* PC with power symbol, from the mockup's 24x24 SVG. */
static void pc_icon_draw(lv_event_t * e)
{
    lv_obj_t * o = lv_event_get_target(e);
    lv_layer_t * layer = lv_event_get_layer(e);
    lv_area_t a;
    lv_obj_get_coords(o, &a);
    float k = lv_area_get_width(&a) / 24.0f;
    lv_color_t col = lv_obj_get_style_text_color(o, 0);
    int lw = (int)lroundf(1.6f * k);

    lv_draw_rect_dsc_t rd;
    lv_draw_rect_dsc_init(&rd);
    rd.bg_opa = LV_OPA_TRANSP;
    rd.border_color = col;
    rd.border_width = lw;
    rd.radius = (int)(1.5f * k);
    lv_area_t r = {a.x1 + (int)(3 * k) - lw / 2, a.y1 + (int)(4 * k) - lw / 2, a.x1 + (int)(21 * k) + lw / 2, a.y1 + (int)(16 * k) + lw / 2};
    lv_draw_rect(layer, &rd, &r);

    lv_draw_line_dsc_t ld;
    lv_draw_line_dsc_init(&ld);
    ld.color = col;
    ld.width = lw;
    ld.round_start = ld.round_end = 1;
    static const float seg[3][4] = {{8, 20, 16, 20}, {12, 16, 12, 20}, {12, 7.5f, 12, 10.5f}};
    for(int i = 0; i < 3; i++) {
        ld.p1.x = a.x1 + seg[i][0] * k;
        ld.p1.y = a.y1 + seg[i][1] * k;
        ld.p2.x = a.x1 + seg[i][2] * k;
        ld.p2.y = a.y1 + seg[i][3] * k;
        lv_draw_line(layer, &ld);
    }
    /* "M9.8 8.7a3 3 0 1 0 4.4 0": radius 3 around (12, 10.74), the long way through the bottom. */
    lv_draw_arc_dsc_t ad;
    lv_draw_arc_dsc_init(&ad);
    ad.color = col;
    ad.width = lw;
    ad.rounded = 1;
    ad.center.x = a.x1 + (int32_t)(12 * k);
    ad.center.y = a.y1 + (int32_t)(10.74f * k);
    ad.radius = (uint16_t)(3 * k + lw / 2.0f);
    ad.start_angle = 317;
    ad.end_angle = 223;
    lv_draw_arc(layer, &ad);
}

/* Small power symbol on the button (12x12 viewBox). */
static void power_icon_draw(lv_event_t * e)
{
    lv_obj_t * o = lv_event_get_target(e);
    lv_layer_t * layer = lv_event_get_layer(e);
    lv_area_t a;
    lv_obj_get_coords(o, &a);
    float k = lv_area_get_width(&a) / 12.0f;
    lv_color_t col = lv_obj_get_style_text_color(o, 0);
    lv_draw_line_dsc_t ld;
    lv_draw_line_dsc_init(&ld);
    ld.color = col;
    ld.width = 4;
    ld.round_start = ld.round_end = 1;
    ld.p1.x = ld.p2.x = a.x1 + 6 * k;
    ld.p1.y = a.y1 + 1.5f * k;
    ld.p2.y = a.y1 + 5.5f * k;
    lv_draw_line(layer, &ld);
    lv_draw_arc_dsc_t ad;
    lv_draw_arc_dsc_init(&ad);
    ad.color = col;
    ad.width = 4;
    ad.rounded = 1;
    ad.center.x = a.x1 + (int32_t)(6 * k);
    ad.center.y = a.y1 + (int32_t)(6.5f * k);
    ad.radius = (uint16_t)(4.2f * k);
    ad.start_angle = 300;
    ad.end_angle = 240;
    lv_draw_arc(layer, &ad);
}

static void power_on(void)
{
    if(power_state == PW_PENDING || lv_tick_get() < btn_enable_at) return;
    char line[96];
    power_id = next_id++;
    snprintf(line, sizeof line, "{\"t\":\"action\",\"id\":%d,\"action\":\"pc_power_on\"}", power_id);
    if(panel_sock_send(line)) power_state = PW_PENDING;
}

static void btn_cb(lv_event_t * e)
{
    (void)e;
    power_on();
}

static lv_obj_t * build_hero(lv_obj_t * parent)
{
    lv_obj_t * t = ui_tile(parent, 0);
    lv_obj_t * r = ui_hbox(t, LV_FLEX_ALIGN_START, 36);
    lv_obj_set_width(r, lv_pct(100));
    lv_obj_set_style_pad_ver(r, 8, 0);

    int isz = landscape ? ICON_L : ICON_P;
    lv_obj_t * wrap = ui_box(r);
    lv_obj_set_size(wrap, isz + 32, isz + 32);
    icon = ui_box(wrap);
    lv_obj_set_size(icon, isz, isz);
    lv_obj_center(icon);
    lv_obj_add_event_cb(icon, pc_icon_draw, LV_EVENT_DRAW_MAIN, NULL);
    /* Spinner ring while waiting for the host program (CSS: 2 px orange arc, 1.4 s per turn). */
    spinner = lv_spinner_create(wrap);
    lv_spinner_set_anim_params(spinner, 1400, 90);
    lv_obj_set_size(spinner, isz + 30, isz + 30);
    lv_obj_center(spinner);
    lv_obj_set_style_arc_opa(spinner, LV_OPA_TRANSP, LV_PART_MAIN);
    lv_obj_set_style_arc_width(spinner, 5, LV_PART_INDICATOR);
    lv_obj_set_style_arc_color(spinner, lv_color_hex(C_ORANGE), LV_PART_INDICATOR);
    lv_obj_set_style_arc_rounded(spinner, true, LV_PART_INDICATOR);
    lv_obj_set_clickable(spinner, false);

    lv_obj_t * col = ui_box(r);
    lv_obj_set_flex_flow(col, LV_FLEX_FLOW_COLUMN);
    lv_obj_set_flex_grow(col, 1);
    lv_obj_set_style_pad_row(col, 4, 0);
    state_lbl = ui_text(col, "PC is off", font_hero, C_TEXT);
    sub_lbl = ui_text(col, "", font_sub, C_MUTED);
    lv_obj_set_width(sub_lbl, lv_pct(100));
    lv_label_set_long_mode(sub_lbl, LV_LABEL_LONG_MODE_DOTS);

    /* Shown only while the PC is off, so pressing it can never switch the PC off. */
    btn = lv_button_create(r);
    lv_obj_set_size(btn, LV_SIZE_CONTENT, LV_SIZE_CONTENT);
    lv_obj_set_flex_flow(btn, LV_FLEX_FLOW_ROW);
    lv_obj_set_flex_align(btn, LV_FLEX_ALIGN_CENTER, LV_FLEX_ALIGN_CENTER, LV_FLEX_ALIGN_CENTER);
    lv_obj_set_style_pad_column(btn, 13, 0);
    lv_obj_set_style_pad_hor(btn, 26, 0);
    lv_obj_set_style_pad_ver(btn, 15, 0);
    lv_obj_set_style_radius(btn, 18, 0);
    lv_obj_set_style_bg_color(btn, lv_color_hex(C_GREEN), 0);
    lv_obj_set_style_bg_opa(btn, LV_OPA_TRANSP, 0);
    lv_obj_set_style_bg_opa(btn, 38, LV_STATE_PRESSED);
    lv_obj_set_style_border_width(btn, 2, 0);
    lv_obj_set_style_border_color(btn, lv_color_hex(C_GREEN), 0);
    lv_obj_set_style_border_opa(btn, 115, 0); /* .45 */
    lv_obj_set_style_border_width(btn, 5, LV_STATE_FOCUSED);
    lv_obj_set_style_border_color(btn, lv_color_hex(C_FOCUS), LV_STATE_FOCUSED);
    lv_obj_set_style_border_opa(btn, LV_OPA_COVER, LV_STATE_FOCUSED);
    lv_obj_add_event_cb(btn, btn_cb, LV_EVENT_CLICKED, NULL);
    btn_icon = ui_box(btn);
    lv_obj_set_size(btn_icon, 28, 28);
    lv_obj_add_event_cb(btn_icon, power_icon_draw, LV_EVENT_DRAW_MAIN, NULL);
    btn_lbl = ui_text(btn, "Power on PC", font_sub, C_GREEN);
    return t;
}

static lv_obj_t * build_access(lv_obj_t * parent)
{
    lv_obj_t * t = ui_tile(parent, 0);
    ui_title(t, "REMOTE ACCESS", C_TEAL);
    ip_lbl = ui_text(t, "--", landscape ? font_ip_l : font_ip, C_TEAL);
    url_lbl = ui_text(t, "--", font_url, C_LABEL);
    ts_val = kv(t, "Tailscale", &ts_row);
    lv_obj_set_hidden(ts_row, true);
    return t;
}

static lv_obj_t * build_net(lv_obj_t * parent, int grow)
{
    lv_obj_t * t = ui_tile(parent, grow);
    eth_title = ui_title(t, "NETWORK", C_TEAL);
    link_val = kv(t, "Link", NULL);
    row(t, "board.rx");
    row(t, "board.tx");
    mac_val = kv(t, "MAC", NULL);
    return t;
}

static lv_obj_t * build_kvm(lv_obj_t * parent, int grow)
{
    lv_obj_t * t = ui_tile(parent, grow);
    ui_title(t, "KVM", C_TEAL);
    sess_val = kv(t, "Sessions", NULL);
    hdmi_val = kv(t, "HDMI in", NULL);
    stream_val = kv(t, "Stream", NULL);
    media_val = kv(t, "Media", NULL);
    usb_val = kv(t, "USB", NULL);
    return t;
}

static lv_obj_t * build_board(lv_obj_t * parent, int grow)
{
    lv_obj_t * t = ui_tile(parent, grow);
    ui_title(t, "KVMPANEL · T113-I", C_TEAL);
    row(t, "board.soc");
    row(t, "board.cpu");
    row(t, "board.mem");
    ui_add_bar(t, "board.mem", C_TEAL);
    sd_val = kv(t, "SD card", NULL);
    up_val = kv(t, "Uptime / ver", NULL);
    return t;
}

static lv_obj_t * row_box(lv_obj_t * parent)
{
    lv_obj_t * r = ui_hbox(parent, LV_FLEX_ALIGN_START, TILE_GAP);
    lv_obj_set_width(r, lv_pct(100));
    lv_obj_set_flex_align(r, LV_FLEX_ALIGN_START, LV_FLEX_ALIGN_START, LV_FLEX_ALIGN_START);
    return r;
}

void offline_create(lv_obj_t * scr, int w, int h, lv_obj_t * main_scr)
{
    main_obj = main_scr;
    landscape = w > h;
    page = ui_box(scr);
    lv_obj_set_size(page, w, h);
    lv_obj_set_flex_flow(page, LV_FLEX_FLOW_COLUMN);
    lv_obj_set_style_pad_all(page, SCR_PAD, 0);
    lv_obj_set_style_pad_row(page, TILE_GAP, 0);

    lv_obj_t * top = ui_hbox(page, LV_FLEX_ALIGN_SPACE_BETWEEN, 10);
    lv_obj_set_width(top, lv_pct(100));
    lv_obj_t * brand = ui_hbox(top, LV_FLEX_ALIGN_START, 15);
    lv_obj_set_flex_align(brand, LV_FLEX_ALIGN_START, LV_FLEX_ALIGN_END, LV_FLEX_ALIGN_END);
    ui_text(brand, "KVMPanel", font_brand, C_TEXT);
    host_lbl = ui_text(brand, "", font_title, C_MUTED);
    /* Portrait has no room next to the chips; Remote access shows the name anyway. */
    lv_obj_set_hidden(host_lbl, !landscape);
    ui_status_bar(top, 1);

    if(landscape) {
        lv_obj_t * body = ui_box(page);
        lv_obj_set_width(body, lv_pct(100));
        lv_obj_set_flex_grow(body, 1);
        lv_obj_set_flex_flow(body, LV_FLEX_FLOW_COLUMN);
        lv_obj_set_style_pad_row(body, TILE_GAP, 0);
        lv_obj_t * r1 = row_box(body);
        lv_obj_set_height(r1, ROW1_H);
        lv_obj_t * hero = build_hero(r1);
        lv_obj_set_flex_grow(hero, 2);
        lv_obj_set_height(hero, lv_pct(100));
        lv_obj_t * acc = build_access(r1);
        lv_obj_set_flex_grow(acc, 1);
        lv_obj_set_height(acc, lv_pct(100));
        lv_obj_t * r2 = row_box(body);
        lv_obj_set_flex_grow(r2, 1);
        lv_obj_t * cols[3] = {build_net(r2, 1), build_kvm(r2, 1), build_board(r2, 1)};
        for(int i = 0; i < 3; i++) lv_obj_set_height(cols[i], lv_pct(100));
    }
    else {
        build_hero(page);
        build_access(page);
        build_net(page, 4);
        build_kvm(page, 5);
        build_board(page, 5);
    }

    lv_obj_t * foot = ui_hbox(page, LV_FLEX_ALIGN_SPACE_BETWEEN, 0);
    lv_obj_set_width(foot, lv_pct(100));
    lv_obj_set_style_pad_hor(foot, 10, 0);
    cd_lbl = ui_text(foot, "", font_title, C_MUTED);
    lv_label_set_recolor(cd_lbl, true);
    ui_text(foot, "FN · Settings", font_title, C_MUTED);

    /* Screen off: black layer above everything; a click or key wakes it. */
    blank = ui_box(lv_layer_top());
    lv_obj_set_size(blank, w, h);
    lv_obj_set_style_bg_color(blank, lv_color_black(), 0);
    lv_obj_set_style_bg_opa(blank, LV_OPA_COVER, 0);
    lv_obj_set_clickable(blank, true);
    lv_obj_set_hidden(blank, true);

    inputs_seen = x11_out_inputs();
    /* Start on the offline page until telemetry arrives. */
    lv_obj_set_hidden(main_obj, true);
    bg_set_calm(true);
    bg_set_load(0);
    wake();
}

/* ---- data ------------------------------------------------------------------- */

static const char * jstr(const cJSON * o, const char * k)
{
    const cJSON * x = cJSON_GetObjectItem(o, k);
    return cJSON_IsString(x) ? x->valuestring : NULL;
}

static double jnum(const cJSON * o, const char * k)
{
    const cJSON * x = cJSON_GetObjectItem(o, k);
    return cJSON_IsNumber(x) ? x->valuedouble : NAN;
}

static void ago(char * out, size_t n, double ms)
{
    if(isnan(ms) || ms < 0) {
        snprintf(out, n, "no data yet");
        return;
    }
    long s = lround(ms / 1000);
    if(s < 60) snprintf(out, n, "last data %ld s ago", s);
    else if(s < 3600) snprintf(out, n, "last data %ld min ago", lround(s / 60.0));
    else if(s < 86400) snprintf(out, n, "last data %ld h ago", lround(s / 3600.0));
    else snprintf(out, n, "last data %ld d ago", lround(s / 86400.0));
}

static void update_hero(void)
{
    bool off = !strcmp(pc_power, "off");
    lv_label_set_text(state_lbl, off ? "PC is off" : "Waiting for host app");
    lv_obj_set_style_text_color(icon, lv_color_hex(off ? C_DIM : C_ORANGE), 0);
    lv_obj_invalidate(icon);
    lv_obj_set_hidden(spinner, off);
    lv_obj_set_hidden(btn, !off);

    char when[48], s[128];
    ago(when, sizeof when, board_ts - last_seen_ts);
    const char * link = ui_pc_link();
    if(off) snprintf(s, sizeof s, "Power LED off · %s", when);
    else if(!strcmp(pc_power, "unknown")) snprintf(s, sizeof s, "Power LED unknown · %s", when);
    else snprintf(s, sizeof s, "PC is on, USB %s · %s", strcmp(link, "off") ? "connected" : "not connected", when);
    lv_label_set_text(sub_lbl, s);

    if(power_state == PW_FAILED && lv_tick_get() > power_failed_until) power_state = PW_IDLE;
    uint32_t fg = power_state == PW_FAILED ? C_ORANGE : power_state == PW_PENDING ? C_MUTED : C_GREEN;
    lv_label_set_text(btn_lbl, power_state == PW_PENDING ? "Powering on…" : power_state == PW_FAILED ? "Refused" : "Power on PC");
    lv_obj_set_style_text_color(btn_lbl, lv_color_hex(fg), 0);
    lv_obj_set_style_text_color(btn_icon, lv_color_hex(fg), 0);
    lv_obj_invalidate(btn_icon);

    const char * ip = ui_ip();
    lv_label_set_text(ip_lbl, ip ? ip : "No network");
}

static void on_board(const cJSON * m)
{
    have_board = true;
    board_ts = jnum(m, "ts");
    const char * p = jstr(m, "pc_power");
    snprintf(pc_power, sizeof pc_power, "%s", p ? p : "unknown");
    if(power_state == PW_PENDING && !strcmp(pc_power, "on")) power_state = PW_IDLE;

    char s[128];
    const char * host = jstr(m, "hostname");
    snprintf(s, sizeof s, "%s.local", host ? host : "?");
    lv_label_set_text(host_lbl, s);

    const cJSON * eth = cJSON_GetObjectItem(m, "eth");
    const cJSON * mem = cJSON_GetObjectItem(m, "mem");
    ui_board_sample("board.rx", (float)(jnum(eth, "rx_bps") / 8e6), 0, 2, "MB/s");
    ui_board_sample("board.tx", (float)(jnum(eth, "tx_bps") / 8e6), 0, 10, "MB/s");
    ui_board_sample("board.soc", (float)jnum(m, "soc_temp_c"), 30, 90, "°C");
    ui_board_sample("board.cpu", (float)jnum(m, "cpu_pct"), 0, 100, "%");
    ui_board_sample("board.mem", (float)(jnum(mem, "used") / 1e6), 0, (float)(jnum(mem, "total") / 1e6), "MB");
    ui_board_refresh();
    if(offline_active()) bg_set_load((float)jnum(m, "cpu_pct"));

    /* Remote access */
    const cJSON * acc = cJSON_GetObjectItem(m, "access");
    lv_label_set_text(url_lbl, jstr(acc, "url") ? jstr(acc, "url") : "--");
    const char * ts = jstr(acc, "tailscale_ip");
    lv_obj_set_hidden(ts_row, !ts);
    if(ts) lv_label_set_text(ts_val, ts);

    /* Network */
    const char * iface = jstr(eth, "iface");
    snprintf(s, sizeof s, "NETWORK · %s", iface ? iface : "?");
    for(char * c = s; *c; c++)
        if(*c >= 'a' && *c <= 'z') *c -= 32;
    lv_label_set_text(eth_title, s);
    double mbps = jnum(eth, "link_mbps");
    const char * dup = jstr(eth, "duplex");
    if(isnan(mbps) || mbps == 0) snprintf(s, sizeof s, "Down");
    else if(mbps >= 1000) snprintf(s, sizeof s, "%g Gbps%s%s%s", mbps / 1000, dup ? " · " : "", dup ? dup : "", dup ? " duplex" : "");
    else snprintf(s, sizeof s, "%g Mbps%s%s%s", mbps, dup ? " · " : "", dup ? dup : "", dup ? " duplex" : "");
    lv_label_set_text(link_val, s);
    set_dim(link_val, isnan(mbps) || mbps == 0);
    lv_label_set_text(mac_val, jstr(eth, "mac") ? jstr(eth, "mac") : "--");

    /* KVM */
    const cJSON * kvm = cJSON_GetObjectItem(m, "kvm");
    const cJSON * ss = cJSON_GetObjectItem(kvm, "sessions");
    int n = cJSON_GetArraySize(ss);
    if(n == 0) snprintf(s, sizeof s, "None");
    else {
        const cJSON * f = cJSON_GetArrayItem(ss, 0);
        int k = snprintf(s, sizeof s, "%d · %s @ %s", n, jstr(f, "user") ? jstr(f, "user") : "?", jstr(f, "ip") ? jstr(f, "ip") : "?");
        if(n > 1 && k > 0 && (size_t)k < sizeof s) snprintf(s + k, sizeof s - k, " +%d", n - 1);
    }
    lv_label_set_text(sess_val, s);
    set_dim(sess_val, n == 0);

    const cJSON * hd = cJSON_GetObjectItem(kvm, "hdmi");
    bool sig = cJSON_IsTrue(cJSON_GetObjectItem(hd, "signal"));
    if(sig) snprintf(s, sizeof s, "%.0f×%.0f @ %.0f", jnum(hd, "width"), jnum(hd, "height"), jnum(hd, "fps"));
    lv_label_set_text(hdmi_val, sig ? s : "No signal");
    set_dim(hdmi_val, !sig);

    const cJSON * st = cJSON_GetObjectItem(kvm, "stream");
    bool streaming = jstr(st, "state") && !strcmp(jstr(st, "state"), "streaming");
    if(streaming) snprintf(s, sizeof s, "%.0f fps · MJPEG", jnum(st, "fps"));
    lv_label_set_text(stream_val, streaming ? s : "Idle");
    set_dim(stream_val, !streaming);

    const cJSON * md = cJSON_GetObjectItem(kvm, "media");
    const char * mounted = jstr(md, "mounted");
    const char * mode = jstr(md, "mode");
    if(mounted) snprintf(s, sizeof s, "%s%s", mounted, mode && !strcmp(mode, "cdrom") ? " · CD" : "");
    lv_label_set_text(media_val, mounted ? s : "None");
    set_dim(media_val, !mounted);

    static const char * names[][2] = {{"keyboard", "Kbd"}, {"mouse_rel", "Mouse"}, {"mouse_abs", "Mouse"}, {"storage", "Disk"},
                                      {"net", "Net"}, {"acm", "Serial"}, {"telemetry", "Telemetry"}};
    s[0] = 0;
    const cJSON * f;
    cJSON_ArrayForEach(f, cJSON_GetObjectItem(kvm, "usb_functions"))
    {
        const char * nm = cJSON_IsString(f) ? f->valuestring : "?";
        for(size_t i = 0; i < sizeof names / sizeof names[0]; i++)
            if(!strcmp(nm, names[i][0])) nm = names[i][1];
        if(strstr(s, nm)) continue; /* both mice -> one "Mouse" */
        if(s[0]) strncat(s, " · ", sizeof s - strlen(s) - 1);
        strncat(s, nm, sizeof s - strlen(s) - 1);
    }
    lv_label_set_text(usb_val, s[0] ? s : "None");
    set_dim(usb_val, !s[0]);

    /* Board */
    const cJSON * sd = cJSON_GetObjectItem(m, "sd");
    if(cJSON_IsObject(sd)) snprintf(s, sizeof s, "%.1f / %.1f GB", jnum(sd, "used") / 1e9, jnum(sd, "total") / 1e9);
    lv_label_set_text(sd_val, cJSON_IsObject(sd) ? s : "No card");
    set_dim(sd_val, !cJSON_IsObject(sd));
    double up = jnum(m, "uptime_s");
    long d = (long)(up / 86400), hh = (long)fmod(up, 86400) / 3600, mm = (long)fmod(up, 3600) / 60;
    const char * ver = jstr(m, "version");
    if(d > 0) snprintf(s, sizeof s, "%ld d %02ld h · %s", d, hh, ver ? ver : "?");
    else snprintf(s, sizeof s, "%ld h %02ld min · %s", hh, mm, ver ? ver : "?");
    lv_label_set_text(up_val, s);

    update_hero();
}

void offline_on_message(const char * t, const cJSON * msg)
{
    if(!strcmp(t, "board")) on_board(msg);
    else if(!strcmp(t, "pc")) {
        double seen = jnum(msg, "last_seen_ts");
        if(!isnan(seen)) last_seen_ts = seen;
        pc_connected = cJSON_IsTrue(cJSON_GetObjectItem(msg, "connected"));
        if(pc_connected) last_telemetry_tick = lv_tick_get();
    }
    else if(!strcmp(t, "config")) {
        const cJSON * p = cJSON_GetObjectItem(cJSON_GetObjectItem(msg, "config"), "panel");
        double v = jnum(p, "offline_sleep_min");
        if(!isnan(v) && (int)v != sleep_min) {
            sleep_min = (int)v;
            if(offline_active()) countdown = sleep_min > 0 ? sleep_min * 60 : -1;
        }
    }
    else if(!strcmp(t, "result")) {
        const cJSON * id = cJSON_GetObjectItem(msg, "id");
        if(power_id < 0 || !cJSON_IsNumber(id) || id->valueint != power_id) return;
        if(!cJSON_IsTrue(cJSON_GetObjectItem(msg, "ok"))) {
            printf("panel: pc_power_on refused: %s\n", jstr(msg, "error") ? jstr(msg, "error") : "?");
            power_state = PW_FAILED;
            power_failed_until = lv_tick_get() + 3000;
        }
        power_id = -1; /* ok: stays pending until board.pc_power turns on */
        if(have_board) update_hero();
    }
}

void offline_telemetry(double ts)
{
    if(!isnan(ts)) last_seen_ts = ts;
    last_telemetry_tick = lv_tick_get();
    pc_connected = true;
}

bool offline_active(void)
{
    return shown_offline;
}

static void wake(void)
{
    countdown = sleep_min > 0 ? sleep_min * 60 : -1;
    cd_tick = lv_tick_get();
    if(screen_off) {
        screen_off = false;
        lv_obj_set_hidden(blank, true);
        bg_set_paused(false);
        /* The key or tap that woke the screen must not also press the button. */
        btn_enable_at = lv_tick_get() + 500;
    }
}

void offline_tick(void)
{
    uint32_t now = lv_tick_get();
    if(pc_connected && now - last_telemetry_tick > TELEMETRY_TIMEOUT_MS) pc_connected = false;

    bool want = !pc_connected;
    if(want != shown_offline) {
        shown_offline = want;
        lv_obj_set_hidden(page, !want);
        lv_obj_set_hidden(main_obj, want);
        bg_set_calm(want);
        wake();
        if(want && have_board) update_hero();
    }

    unsigned in = x11_out_inputs();
    if(in != inputs_seen) {
        inputs_seen = in;
        if(screen_off) wake();
    }
    if(!shown_offline) return;

    if(countdown > 0 && !screen_off && now - cd_tick >= 1000) {
        cd_tick += 1000;
        if(--countdown == 0) {
            screen_off = true;
            lv_obj_set_hidden(blank, false);
            bg_set_paused(true);
        }
    }
    static int shown_cd = -2;
    if(countdown != shown_cd) {
        shown_cd = countdown;
        char s[64];
        if(countdown < 0) snprintf(s, sizeof s, "Screen stays on");
        else snprintf(s, sizeof s, "Screen off in #c5cfdc %d:%02d#", countdown / 60, countdown % 60);
        lv_label_set_text(cd_lbl, s);
    }
}
