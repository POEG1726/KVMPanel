/*
 * main.c
 * Created on Nov 23, 2026
 * Author: @POEG1726
 */

#include "lvgl.h"
#include "panel_sock.h"
#include <math.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

enum { SHELL = 0x101722, SURFACE = 0x182331, TEXT = 0xf0f6fc, MUTED = 0x92a7ba,
       ACCENT = 0x77ddc2, ALERT = 0xffad76, MAX_METRICS = 768, CHANGE_ID = 2,
       MCP_STOP_ID = 5 };

typedef struct {
    char id[48];
    lv_obj_t *name;
    lv_obj_t *number;
} MetricRow;

static lv_display_t *display;
static lv_obj_t *tabs, *groups[4], *note, *power, *offline, *settings_list, *rotation, *sleep_mask;
static lv_obj_t *mcp_banner, *mcp_label, *mcp_button;
LV_FONT_DECLARE(mcp_font)
static lv_font_t mcp_display_font;
static MetricRow rows[MAX_METRICS];
static const char *const keys[] = {"cpu", "gpu", "mem", "sys"};
static const char *const directions[] = {"normal", "right", "inverted", "left"};
static const lv_display_rotation_t turns[] = {
    LV_DISPLAY_ROTATION_0, LV_DISPLAY_ROTATION_90, LV_DISPLAY_ROTATION_180, LV_DISPLAY_ROTATION_270,
};
static cJSON *hidden;
static unsigned row_count;
static bool requested_change;
static bool powering_on;
static bool mcp_active, display_asleep;
static uint32_t power_request_time;
static uint32_t last_telemetry;
static volatile sig_atomic_t stop;

static uint32_t ticks(void)
{
    struct timespec stamp;
    clock_gettime(CLOCK_MONOTONIC, &stamp);
    return (uint32_t)((uint64_t)stamp.tv_sec * 1000 + (unsigned long)stamp.tv_nsec / 1000000);
}

static const cJSON *field(const cJSON *value, const char *key)
{
    return cJSON_GetObjectItemCaseSensitive(value, key);
}

static const char *str(const cJSON *value, const char *key)
{
    const char *text = cJSON_GetStringValue(field(value, key));
    return text ? text : "";
}

static double number(const cJSON *value, const char *key)
{
    return cJSON_GetNumberValue(field(value, key));
}

static lv_obj_t *label(lv_obj_t *parent, const char *text, unsigned color)
{
    lv_obj_t *obj = lv_label_create(parent);
    lv_label_set_text(obj, text);
    lv_obj_set_style_text_color(obj, lv_color_hex(color), 0);
    return obj;
}

static void vertical(lv_obj_t *object, unsigned color)
{
    lv_obj_set_flex_flow(object, LV_FLEX_FLOW_COLUMN);
    lv_obj_set_style_pad_all(object, 12, 0);
    lv_obj_set_style_pad_row(object, 8, 0);
    lv_obj_set_style_bg_color(object, lv_color_hex(color), 0);
    lv_obj_set_style_bg_opa(object, LV_OPA_COVER, 0);
    lv_obj_set_style_border_width(object, 0, 0);
}

static bool send(const char *type, unsigned id, const char *key, cJSON *data)
{
    cJSON *request = cJSON_CreateObject();
    if (request) {
        cJSON_AddStringToObject(request, "t", type);
        cJSON_AddNumberToObject(request, "id", id);
        if (data)
            cJSON_AddItemToObject(request, key, data);
    } else {
        cJSON_Delete(data);
    }
    bool sent = panel_sock_send(request);
    cJSON_Delete(request);
    return sent;
}

/** Stop the current MCP operation via the trusted local panel socket. */
static void stop_mcp(lv_event_t *event)
{
    (void)event;
    if (send("mcp_stop", MCP_STOP_ID, NULL, NULL))
        lv_obj_add_state(mcp_button, LV_STATE_DISABLED);
    else
        lv_label_set_text(mcp_label, "无法发送停止请求");
}

/** Display the latest MCP activity and keep its stop control accessible while active. */
static void mcp_state(const cJSON *message)
{
    const bool active = cJSON_IsTrue(field(message, "active"));
    if (active && !mcp_active)
        lv_obj_remove_state(mcp_button, LV_STATE_DISABLED);
    mcp_active = active;
    lv_obj_set_hidden(mcp_banner, !active);
    lv_obj_set_hidden(sleep_mask, !display_asleep || active);
    if (active && !lv_obj_has_state(mcp_button, LV_STATE_DISABLED)) {
        const char *name = str(message, "key_name");
        lv_label_set_text_fmt(mcp_label, "%s%s%s", "AI 正在操作",
                              *name ? " · " : "", name);
    }
}

/** Create a patch for the shared panel settings, including all hidden IDs. */
static bool patch(const char *key, cJSON *value)
{
    if (requested_change || !value) {
        cJSON_Delete(value);
        return false;
    }
    cJSON *root = cJSON_CreateObject();
    cJSON *panel = root ? cJSON_AddObjectToObject(root, "panel") : NULL;
    if (!panel) {
        cJSON_Delete(root);
        cJSON_Delete(value);
        return false;
    }
    cJSON_AddItemToObject(panel, key, value);
    requested_change = send("patch_config", CHANGE_ID, "patch", root);
    return requested_change;
}

static void power_on(lv_event_t *event)
{
    (void)event;
    if (!powering_on && send("action", 3, "action", cJSON_CreateString("pc_power_on"))) {
        powering_on = true;
        power_request_time = lv_tick_get();
        lv_obj_add_state(power, LV_STATE_DISABLED);
        lv_label_set_text(note, "Starting the PC…");
    }
}

static void wake(lv_event_t *event)
{
    (void)event;
    send("action", 4, "action", cJSON_CreateString("wake_display"));
}

static void set_rotation(lv_event_t *event)
{
    const uint32_t index = lv_dropdown_get_selected(lv_event_get_target_obj(event));
    if (index < sizeof(directions) / sizeof(directions[0]))
        patch("orientation", cJSON_CreateString(directions[index]));
}

static void settings_open(lv_event_t *event)
{
    if (lv_tabview_get_tab_active(lv_event_get_target_obj(event)) == 4)
        send("metric_catalogue", 1, NULL, NULL);
}

static int hidden_index(const char *id)
{
    const cJSON *item;
    int index = 0;
    cJSON_ArrayForEach(item, hidden) {
        const char *old = cJSON_GetStringValue(item);
        if (old && strcmp(old, id) == 0)
            return index;
        ++index;
    }
    return -1;
}

/** Persist the checkbox selection through the same config used by the web UI. */
static void select_metric(lv_event_t *event)
{
    lv_obj_t *check = lv_event_get_target_obj(event);
    char *id = lv_event_get_user_data(event);
    if (lv_event_get_code(event) == LV_EVENT_DELETE) {
        free(id);
        return;
    }
    const int index = hidden_index(id);
    const bool visible = lv_obj_has_state(check, LV_STATE_CHECKED);
    cJSON *next = cJSON_Duplicate(hidden, true);
    if (next) {
        if (visible && index >= 0)
            cJSON_DeleteItemFromArray(next, index);
        if (!visible && index < 0)
            cJSON_AddItemToArray(next, cJSON_CreateString(id));
    }
    if (!patch("hidden_metrics", next)) {
        if (index < 0)
            lv_obj_add_state(check, LV_STATE_CHECKED);
        else
            lv_obj_remove_state(check, LV_STATE_CHECKED);
    }
}

static void inventory(const cJSON *list)
{
    const int32_t position = lv_obj_get_scroll_y(settings_list);
    lv_obj_clean(settings_list);
    const cJSON *item;
    unsigned count = 0;
    cJSON_ArrayForEach(item, list) {
        const char *id = str(item, "id");
        if (!id[0] || ++count > MAX_METRICS)
            continue;
        char *key = strdup(id);
        if (!key)
            continue;
        char text[180];
        snprintf(text, sizeof(text), "%s · %s", str(item, "label"), key);
        lv_obj_t *check = lv_checkbox_create(settings_list);
        lv_checkbox_set_text(check, text);
        lv_obj_set_style_text_color(check, lv_color_hex(TEXT), 0);
        if (hidden_index(key) < 0)
            lv_obj_add_state(check, LV_STATE_CHECKED);
        lv_obj_add_event_cb(check, select_metric, LV_EVENT_VALUE_CHANGED, key);
        lv_obj_add_event_cb(check, select_metric, LV_EVENT_DELETE, key);
    }
    if (!count)
        label(settings_list, "Connect the PC collector to choose metrics.", MUTED);
    lv_obj_update_layout(settings_list);
    lv_obj_scroll_to_y(settings_list, position, LV_ANIM_OFF);
}

static void configure(const cJSON *message)
{
    const cJSON *panel = field(field(message, "config"), "panel");
    const cJSON *list = field(panel, "hidden_metrics");
    if (cJSON_IsArray(list)) {
        cJSON_Delete(hidden);
        hidden = cJSON_Duplicate(list, true);
    }
    const char *direction = str(panel, "orientation");
    for (unsigned i = 0; i < sizeof(directions) / sizeof(directions[0]); ++i) {
        if (strcmp(direction, directions[i]) == 0) {
            lv_dropdown_set_selected(rotation, i);
            lv_display_set_rotation(display, turns[i]);
            break;
        }
    }
    if (lv_tabview_get_tab_active(tabs) == 4)
        send("metric_catalogue", 1, NULL, NULL);
}

static void clear_metrics(void)
{
    for (unsigned i = 0; i < sizeof(groups) / sizeof(groups[0]); ++i)
        lv_obj_clean(groups[i]);
    row_count = 0;
}

static MetricRow *new_row(const cJSON *metric)
{
    const char *id = str(metric, "id");
    const char *group = str(metric, "group");
    unsigned i = 0;
    while (i + 1 < sizeof(groups) / sizeof(groups[0]) && strcmp(group, keys[i]) != 0)
        ++i;
    lv_obj_t *row = lv_obj_create(groups[i]);
    lv_obj_set_size(row, lv_pct(100), LV_SIZE_CONTENT);
    lv_obj_set_flex_flow(row, LV_FLEX_FLOW_ROW);
    lv_obj_set_flex_align(row, LV_FLEX_ALIGN_SPACE_BETWEEN, LV_FLEX_ALIGN_CENTER, LV_FLEX_ALIGN_CENTER);
    lv_obj_set_style_bg_color(row, lv_color_hex(SURFACE), 0);
    lv_obj_set_style_border_width(row, 0, 0);
    lv_obj_set_style_pad_all(row, 12, 0);
    lv_obj_set_style_pad_column(row, 8, 0);
    lv_obj_t *name = label(row, str(metric, "label"), MUTED);
    lv_obj_set_flex_grow(name, 1);
    lv_label_set_long_mode(name, LV_LABEL_LONG_MODE_DOTS);
    MetricRow *item = &rows[row_count++];
    snprintf(item->id, sizeof(item->id), "%s", id);
    item->name = name;
    item->number = label(row, "", TEXT);
    return item;
}

static void telemetry(const cJSON *list)
{
    last_telemetry = lv_tick_get();
    const cJSON *metric;
    unsigned index = 0;
    bool rebuild = (unsigned)cJSON_GetArraySize(list) != row_count;
    cJSON_ArrayForEach(metric, list) {
        if (index >= row_count || strcmp(rows[index++].id, str(metric, "id")) != 0) {
            rebuild = true;
            break;
        }
    }
    int32_t offsets[4] = {0};
    if (rebuild) {
        for (unsigned i = 0; i < 4; ++i)
            offsets[i] = lv_obj_get_scroll_y(groups[i]);
        clear_metrics();
    }
    index = 0;
    cJSON_ArrayForEach(metric, list) {
        if (index >= MAX_METRICS)
            break;
        MetricRow *row = rebuild ? new_row(metric) : &rows[index];
        ++index;
        const cJSON *value = field(metric, "value");
        const double v = cJSON_GetNumberValue(value);
        const double warn = number(metric, "warn");
        const char *unit = str(metric, "unit");
        char text[64];
        if (!isfinite(v))
            snprintf(text, sizeof(text), "—");
        else if ((strcmp(unit, "B/s") == 0 || strcmp(unit, "b/s") == 0) && v >= 1e6)
            snprintf(text, sizeof(text), "%.2f M%s", v / 1e6, unit);
        else
            snprintf(text, sizeof(text), "%.*f %s", fabs(v) >= 100 ? 0 : 2, v, unit);
        if (strcmp(lv_label_get_text(row->name), str(metric, "label")) != 0)
            lv_label_set_text(row->name, str(metric, "label"));
        lv_label_set_text(row->number, text);
        lv_obj_set_style_text_color(row->number, lv_color_hex(isfinite(warn) && v > warn ? ALERT : TEXT), 0);
    }
    if (rebuild)
        for (unsigned i = 0; i < 4; ++i) {
            lv_obj_update_layout(groups[i]);
            lv_obj_scroll_to_y(groups[i], offsets[i], LV_ANIM_OFF);
        }
}

static void board_state(const cJSON *message)
{
    char text[192];
    const char *pc = str(message, "pc_power");
    display_asleep = cJSON_IsTrue(field(message, "display_asleep"));
    lv_obj_set_hidden(sleep_mask, !display_asleep || mcp_active);
    const bool pc_off = strcmp(pc, "off") == 0;
    const bool online = last_telemetry && lv_tick_elaps(last_telemetry) < 3000;
    const char *state = online ? "PC connected" : pc_off ? "PC is off" :
                        strcmp(pc, "on") == 0 ? "Waiting for PC collector" : "PC power unknown";
    if (!pc_off || (powering_on && lv_tick_elaps(power_request_time) >= 8000))
        powering_on = false;
    if (powering_on)
        state = "Starting the PC…";
    const cJSON *eth = field(message, "eth");
    const cJSON *access = field(message, "access");
    const double speed = number(eth, "link_mbps");
    snprintf(text, sizeof(text), "%s · %s\n%s%s%s",
             state, speed > 0 ? "Ethernet linked" : "Ethernet down",
             str(access, "url"), *str(access, "tailscale_ip") ? " · " : "", str(access, "tailscale_ip"));
    lv_label_set_text(note, text);
    if (online) {
        lv_obj_set_hidden(offline, true);
    } else {
        const cJSON *ram = field(message, "mem");
        const cJSON *kvm = field(message, "kvm");
        char temperature[24] = "—";
        const double soc = number(message, "soc_temp_c");
        if (isfinite(soc))
            snprintf(temperature, sizeof(temperature), "%.1f °C", soc);
        snprintf(text, sizeof(text),
                 "Board CPU %.0f%%  ·  SoC %s\nMemory %.0f / %.0f MiB  ·  Remote sessions %d\nEthernet ↓ %.2f / ↑ %.2f Mb/s",
                 number(message, "cpu_pct"), temperature, number(ram, "used") / 1048576.0,
                 number(ram, "total") / 1048576.0, cJSON_GetArraySize(field(kvm, "sessions")),
                 number(eth, "rx_bps") / 1e6, number(eth, "tx_bps") / 1e6);
        lv_label_set_text(offline, text);
        lv_obj_set_hidden(offline, false);
    }
    lv_obj_set_hidden(power, !pc_off || online);
    if (powering_on)
        lv_obj_add_state(power, LV_STATE_DISABLED);
    else
        lv_obj_remove_state(power, LV_STATE_DISABLED);
    if (!online && row_count)
        clear_metrics();
}

/** Dispatch shared state and replies from the local kvmd service. */
static void on_message(const cJSON *message)
{
    if (!message) {
        requested_change = false;
        powering_on = false;
        last_telemetry = 0;
        display_asleep = false;
        mcp_state(NULL);
        clear_metrics();
        lv_label_set_text(note, "Connecting to KVMPanel…");
        lv_obj_set_hidden(power, true);
        lv_obj_set_hidden(offline, true);
        return;
    }
    const char *type = str(message, "t");
    if (strcmp(type, "config") == 0)
        configure(message);
    else if (strcmp(type, "board") == 0)
        board_state(message);
    else if (strcmp(type, "telemetry") == 0)
        telemetry(field(message, "metrics"));
    else if (strcmp(type, "mcp") == 0)
        mcp_state(message);
    else if (strcmp(type, "result") == 0) {
        const double id = number(message, "id");
        const bool ok = cJSON_IsTrue(field(message, "ok"));
        if (id == 1 && ok)
            inventory(field(message, "data"));
        if (id == CHANGE_ID) {
            requested_change = false;
            if (!ok)
                send("metric_catalogue", 1, NULL, NULL);
        }
        if (id == 3 && !ok) {
            powering_on = false;
            lv_label_set_text(note, "Power-on request refused");
            lv_obj_remove_state(power, LV_STATE_DISABLED);
        }
        if (id == MCP_STOP_ID) {
            if (ok) {
                mcp_state(NULL);
            } else {
                lv_obj_remove_state(mcp_button, LV_STATE_DISABLED);
                lv_label_set_text(mcp_label, "停止失败");
            }
        }
    }
}

static void create_screen(void)
{
    lv_obj_t *screen = lv_screen_active();
    vertical(screen, SHELL);
    lv_obj_set_style_text_font(screen, &lv_font_montserrat_20, 0);
    lv_obj_t *title = label(screen, "KVMPanel", ACCENT);
    lv_obj_set_style_text_font(title, &lv_font_montserrat_28, 0);
    mcp_display_font = lv_font_montserrat_20;
    mcp_display_font.fallback = &mcp_font;
    mcp_banner = lv_obj_create(screen);
    vertical(mcp_banner, SURFACE);
    lv_obj_set_size(mcp_banner, lv_pct(100), LV_SIZE_CONTENT);
    lv_obj_set_scrollable(mcp_banner, false);
    lv_obj_set_flex_flow(mcp_banner, LV_FLEX_FLOW_ROW);
    lv_obj_set_flex_align(mcp_banner, LV_FLEX_ALIGN_START, LV_FLEX_ALIGN_CENTER, LV_FLEX_ALIGN_CENTER);
    lv_obj_set_style_pad_column(mcp_banner, 12, 0);
    mcp_label = label(mcp_banner, "", ALERT);
    lv_obj_set_style_text_font(mcp_label, &mcp_display_font, 0);
    lv_obj_set_flex_grow(mcp_label, 1);
    lv_label_set_long_mode(mcp_label, LV_LABEL_LONG_DOT);
    mcp_button = lv_button_create(mcp_banner);
    lv_obj_set_style_text_font(label(mcp_button, "停止", TEXT), &mcp_display_font, 0);
    lv_obj_add_event_cb(mcp_button, stop_mcp, LV_EVENT_CLICKED, NULL);
    lv_obj_set_hidden(mcp_banner, true);
    note = label(screen, "Connecting to KVMPanel…", MUTED);
    lv_obj_set_width(note, lv_pct(100));
    power = lv_button_create(screen);
    label(power, "Power on PC", TEXT);
    lv_obj_set_hidden(power, true);
    lv_obj_add_event_cb(power, power_on, LV_EVENT_CLICKED, NULL);
    offline = label(screen, "", TEXT);
    lv_obj_set_width(offline, lv_pct(100));
    lv_obj_set_style_bg_color(offline, lv_color_hex(SURFACE), 0);
    lv_obj_set_style_bg_opa(offline, LV_OPA_COVER, 0);
    lv_obj_set_style_radius(offline, 8, 0);
    lv_obj_set_style_pad_all(offline, 12, 0);
    lv_obj_set_hidden(offline, true);
    tabs = lv_tabview_create(screen);
    lv_obj_set_width(tabs, lv_pct(100));
    lv_obj_set_flex_grow(tabs, 1);
    lv_tabview_set_tab_bar_size(tabs, 54);
    static const char *const names[] = {"CPU", "GPU", "Memory", "System"};
    for (unsigned i = 0; i < 4; ++i) {
        groups[i] = lv_tabview_add_tab(tabs, names[i]);
        vertical(groups[i], SHELL);
    }
    lv_obj_t *settings = lv_tabview_add_tab(tabs, "Display");
    vertical(settings, SHELL);
    label(settings, "Rotation", TEXT);
    rotation = lv_dropdown_create(settings);
    lv_dropdown_set_options(rotation, "Normal\n90° right\n180°\n90° left");
    lv_obj_add_event_cb(rotation, set_rotation, LV_EVENT_VALUE_CHANGED, NULL);
    label(settings, "Show metrics", TEXT);
    settings_list = lv_obj_create(settings);
    vertical(settings_list, SURFACE);
    lv_obj_set_width(settings_list, lv_pct(100));
    lv_obj_set_flex_grow(settings_list, 1);
    lv_obj_add_event_cb(tabs, settings_open, LV_EVENT_VALUE_CHANGED, NULL);
    sleep_mask = lv_obj_create(lv_layer_top());
    lv_obj_remove_style_all(sleep_mask);
    lv_obj_set_size(sleep_mask, lv_pct(100), lv_pct(100));
    lv_obj_set_style_bg_color(sleep_mask, lv_color_black(), 0);
    lv_obj_set_style_bg_opa(sleep_mask, LV_OPA_COVER, 0);
    lv_obj_set_hidden(sleep_mask, true);
    lv_obj_set_clickable(sleep_mask, true);
    lv_obj_set_scrollable(sleep_mask, false);
    lv_obj_add_event_cb(sleep_mask, wake, LV_EVENT_PRESSED, NULL);
}

static void discard_frame(lv_display_t *target, const lv_area_t *area, uint8_t *pixels)
{
    (void)area;
    (void)pixels;
    lv_display_flush_ready(target);
}

/** Open the board's framebuffer or a small framebuffer-free test surface. */
static lv_display_t *open_display(void)
{
    if (getenv("KVMPANEL_HEADLESS")) {
        static uint32_t pixels[1280 * 24];
        lv_display_t *target = lv_display_create(800, 480);
        lv_display_set_flush_cb(target, discard_frame);
        lv_display_set_buffers(target, pixels, NULL, sizeof(pixels), LV_DISPLAY_RENDER_MODE_PARTIAL);
        return target;
    }
    lv_display_t *target = lv_linux_fbdev_create();
    const char *path = getenv("KVMPANEL_FRAMEBUFFER");
    return target && lv_linux_fbdev_set_file(target, path ? path : "/dev/fb0") == LV_RESULT_OK ? target : NULL;
}

static void interrupted(int sig)
{
    (void)sig;
    stop = 1;
}

int main(void)
{
    signal(SIGINT, interrupted);
    signal(SIGTERM, interrupted);
    lv_init();
    lv_tick_set_cb(ticks);
    display = open_display();
    if (!display) {
        fputs("kvmpanel-dashboard: framebuffer unavailable\n", stderr);
        return 1;
    }
    lv_display_set_theme(display, lv_theme_default_init(display, lv_color_hex(ACCENT),
                                                        lv_color_hex(SURFACE), true,
                                                        &lv_font_montserrat_20));
    const char *touch = getenv("KVMPANEL_TOUCH");
    if (touch && *touch)
        lv_evdev_create(LV_INDEV_TYPE_POINTER, touch);
    create_screen();
    const char *socket = getenv("KVMPANEL_PANEL_SOCKET");
    if (!panel_sock_start(socket ? socket : "/run/kvmd/panel.sock", on_message))
        return 1;
    while (!stop) {
        uint32_t delay = lv_timer_handler();
        usleep((delay > 50 ? 50 : delay ? delay : 1) * 1000);
    }
    cJSON_Delete(hidden);
    return 0;
}
