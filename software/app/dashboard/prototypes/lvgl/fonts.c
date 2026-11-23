#include "fonts.h"
#include <stdio.h>
#include <stdlib.h>

const lv_font_t * font_body;
const lv_font_t * font_value;
const lv_font_t * font_title;
const lv_font_t * font_small;
const lv_font_t * font_brand;
const lv_font_t * font_hero;
const lv_font_t * font_sub;
const lv_font_t * font_url;
const lv_font_t * font_ip;
const lv_font_t * font_ip_l;

static void * slurp(const char * path, size_t * n)
{
    FILE * f = fopen(path, "rb");
    if(!f) return NULL;
    fseek(f, 0, SEEK_END);
    long sz = ftell(f);
    fseek(f, 0, SEEK_SET);
    void * p = malloc(sz);
    if(p && fread(p, 1, sz, f) != (size_t)sz) {
        free(p);
        p = NULL;
    }
    fclose(f);
    *n = (size_t)sz;
    return p;
}

/* Font data must stay alive for the font's lifetime, so files are loaded once and never freed. */
static const void * load_first(const char * const * paths, size_t * n, const char ** used)
{
    for(int i = 0; paths[i]; i++) {
        void * p = slurp(paths[i], n);
        if(p) {
            *used = paths[i];
            return p;
        }
    }
    return NULL;
}

void fonts_init(void)
{
    static char reg_path[256], sb_path[256];
    const char * dir = getenv("KVM_FONT_DIR");
    if(!dir) dir = "/mnt/c/Windows/Fonts";
    snprintf(reg_path, sizeof reg_path, "%s/segoeui.ttf", dir);
    snprintf(sb_path, sizeof sb_path, "%s/seguisb.ttf", dir);
    const char * reg[] = {reg_path, "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", NULL};
    const char * sb[] = {sb_path, "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf", NULL};

    size_t rn = 0, sn = 0;
    const char * ru = NULL, * su = NULL;
    const void * rd = load_first(reg, &rn, &ru);
    const void * sd = load_first(sb, &sn, &su);
    if(!rd || !sd) {
        fprintf(stderr, "fonts: no TTF found, falling back to Montserrat\n");
        font_body = font_value = font_title = font_small = &lv_font_montserrat_28;
        font_brand = font_hero = font_sub = font_url = font_ip = font_ip_l = &lv_font_montserrat_28;
        return;
    }
    printf("fonts: %s, %s\n", ru, su);
    font_body = lv_tiny_ttf_create_data(rd, rn, 32);
    font_value = lv_tiny_ttf_create_data(sd, sn, 32);
    font_title = lv_tiny_ttf_create_data(rd, rn, 26);
    font_small = lv_tiny_ttf_create_data(rd, rn, 23);
    font_brand = lv_tiny_ttf_create_data(sd, sn, 33);
    font_hero = lv_tiny_ttf_create_data(sd, sn, 53);
    font_sub = lv_tiny_ttf_create_data(rd, rn, 27);
    font_url = lv_tiny_ttf_create_data(rd, rn, 37);
    font_ip = lv_tiny_ttf_create_data(sd, sn, 84);
    font_ip_l = lv_tiny_ttf_create_data(sd, sn, 74);
}
