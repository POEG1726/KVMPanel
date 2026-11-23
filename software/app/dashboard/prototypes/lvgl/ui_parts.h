#pragma once
/* Building blocks from ui.c shared with the PC-offline page (offline.c). */
#include "lvgl.h"

#define C_TEXT 0xe2e8f0
#define C_LABEL 0xc5cfdc
#define C_MUTED 0x8b97aa
#define C_DIM 0x6b7585
#define C_NA 0x5a6270
#define C_TEAL 0x7dd3c8

#define SCR_PAD 16
#define TILE_GAP 14
#define ROW_GAP 15

lv_obj_t * ui_box(lv_obj_t * parent);
lv_obj_t * ui_hbox(lv_obj_t * parent, lv_flex_align_t main, int gap);
lv_obj_t * ui_text(lv_obj_t * parent, const char * s, const lv_font_t * f, uint32_t color);
/* Glass tile: full width, flex column with space-between; `grow` shares the parent's free height. */
lv_obj_t * ui_tile(lv_obj_t * parent, int grow);
lv_obj_t * ui_title(lv_obj_t * parent, const char * s, uint32_t color);
/* label | sparkline | value row of metric `id`. label_w = 0: label takes the free width and the
 * sparkline is 216 px; otherwise the label is label_w wide and the sparkline spark_w. */
void ui_add_row_ex(lv_obj_t * parent, const char * id, uint32_t color, int label_w, int spark_w, int value_w);
void ui_add_row(lv_obj_t * parent, const char * id, uint32_t color);
void ui_add_bar(lv_obj_t * parent, const char * id, uint32_t color);
/* ETH / PC / USB chips; set 0 = main screen, 1 = offline page ("PC HID only" when partial). */
void ui_status_bar(lv_obj_t * parent, int set);
/* Board metrics (board.rx, board.tx, board.soc, board.cpu, board.mem): new sample + redraw. */
void ui_board_sample(const char * id, float value, float min, float max, const char * unit);
void ui_board_refresh(void);
/* Last status message: "ok" / "partial" / "off" and the board IP (NULL = none). */
const char * ui_pc_link(void);
const char * ui_ip(void);
