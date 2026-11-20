/* Minimal LVGL config for the dashboard prototype; everything else uses lv_conf_internal.h defaults. */
#ifndef LV_CONF_H
#define LV_CONF_H

#define LV_COLOR_FORMAT_DEFAULT LV_COLOR_FORMAT_XRGB8888

#define LV_USE_STDLIB_MALLOC LV_STDLIB_CLIB
#define LV_USE_STDLIB_STRING LV_STDLIB_CLIB
#define LV_USE_STDLIB_SPRINTF LV_STDLIB_CLIB

/* Single-threaded software rendering, like the board will start with. */
#define LV_USE_OS LV_OS_NONE
#define LV_USE_DRAW_SW 1
#define LV_DRAW_SW_COMPLEX 1
#define LV_DRAW_SW_SHADOW_CACHE_SIZE 16
#define LV_DRAW_SW_CIRCLE_CACHE_SIZE 32
#define LV_DRAW_LAYER_SIMPLE_BUF_SIZE (256 * 1024)

#define LV_DEF_REFR_PERIOD 33
#define LV_DPI_DEF 130

#define LV_USE_LOG 1
#define LV_LOG_LEVEL LV_LOG_LEVEL_WARN
#define LV_LOG_PRINTF 1

#define LV_USE_SNAPSHOT 1

/* Fonts: TTF loaded at runtime (Segoe UI from Windows, DejaVu fallback), Montserrat as last resort. */
#define LV_USE_TINY_TTF 1
#define LV_TINY_TTF_FILE_SUPPORT 0
#define LV_TINY_TTF_CACHE_GLYPH_CNT 256
#define LV_FONT_MONTSERRAT_28 1

/* No theme: every style is set explicitly. */
#define LV_USE_THEME_DEFAULT 0
#define LV_USE_THEME_SIMPLE 0
#define LV_USE_THEME_MONO 0

#define LV_USE_FLEX 1
#define LV_USE_GRID 0

#define LV_BUILD_EXAMPLES 0
#define LV_BUILD_DEMOS 0

#endif
