/*
 * host_agent.c
 * Created on Nov 23, 2026
 * Author: @POEG1726
 */

#include "host_agent.h"
#include <shellscalingapi.h>
#include <stdarg.h>
#include <stdio.h>
#include <string.h>
#include <wtsapi32.h>

enum { CHUNK = 48 }; /* Room for the longest decimal fragment header in a 63-byte payload. */

typedef struct {
    HostDisplays displays;
    POINT cursor;
    BOOL cursor_valid;
    BOOL foreground;
    BOOL truncated;
    wchar_t title[1024], process[260];
} Desktop;

typedef struct {
    char *data;
    size_t length, capacity;
} Json;

typedef struct {
    HostSession (*session)(void);
    void (*desktop)(Desktop *);
    const char *(*clipboard)(wchar_t *, size_t, BOOL *);
    const char *(*eject)(DEVINST, ULONGLONG);
} Sources;

static const char *const session_names[] = {"locked", "unlocked", "secure_desktop", "screensaver"};

/** Restrict requests to the physical console, including during lock and desktop changes. */
BOOL host_local_session(void)
{
    DWORD session;
    return ProcessIdToSessionId(GetCurrentProcessId(), &session) &&
           session != 0 && session == WTSGetActiveConsoleSessionId();
}

static HostSession current_session(void)
{
    if (!host_local_session())
        return HOST_LOCKED;
    wchar_t *buffer = NULL;
    DWORD size = 0;
    if (!WTSQuerySessionInformationW(WTS_CURRENT_SERVER_HANDLE, WTS_CURRENT_SESSION,
                                    WTSSessionInfoEx, &buffer, &size))
        return HOST_LOCKED;
    const WTSINFOEXW *info = (const WTSINFOEXW *)buffer;
    const BOOL unlocked = size >= sizeof(*info) && info->Level == 1 &&
                          info->Data.WTSInfoExLevel1.SessionState == WTSActive &&
                          info->Data.WTSInfoExLevel1.SessionFlags == WTS_SESSIONSTATE_UNLOCK;
    WTSFreeMemory(buffer);
    if (!unlocked)
        return HOST_LOCKED;
    BOOL screensaver = FALSE;
    if (SystemParametersInfoW(SPI_GETSCREENSAVERRUNNING, 0, &screensaver, 0) && screensaver)
        return HOST_SCREENSAVER;

    HDESK desktop = OpenInputDesktop(0, FALSE, DESKTOP_READOBJECTS);
    if (!desktop)
        return HOST_SECURE_DESKTOP;
    wchar_t name[64];
    const BOOL normal = GetUserObjectInformationW(desktop, UOI_NAME, name, sizeof(name), NULL) &&
                        _wcsicmp(name, L"Default") == 0;
    CloseDesktop(desktop);
    return normal && host_local_session() ? HOST_UNLOCKED : HOST_SECURE_DESKTOP;
}

BOOL host_clipboard_allowed(void)
{
    return current_session() == HOST_UNLOCKED;
}

/** Prefer the monitor interface path, which survives changes to the GDI display number. */
static BOOL CALLBACK add_display(HMONITOR monitor, HDC dc, LPRECT area, LPARAM user)
{
    (void)dc;
    (void)area;
    HostDisplays *displays = (HostDisplays *)user;
    if (displays->count == HOST_MAX_DISPLAYS) {
        displays->truncated = TRUE;
        return FALSE;
    }
    MONITORINFOEXW info = {.cbSize = sizeof(info)};
    if (!GetMonitorInfoW(monitor, (MONITORINFO *)&info))
        return TRUE;
    HostDisplay *entry = &displays->items[displays->count++];
    DISPLAY_DEVICEW device = {.cb = sizeof(device)};
    EnumDisplayDevicesW(info.szDevice, 0, &device, EDD_GET_DEVICE_INTERFACE_NAME);
    wcscpy_s(entry->id, _countof(entry->id), device.DeviceID[0] ? device.DeviceID : info.szDevice);
    wcscpy_s(entry->name, _countof(entry->name), device.DeviceString[0] ? device.DeviceString : info.szDevice);
    entry->area = info.rcMonitor;
    entry->primary = (info.dwFlags & MONITORINFOF_PRIMARY) != 0;
    DEVICE_SCALE_FACTOR scale;
    if (SUCCEEDED(GetScaleFactorForMonitor(monitor, &scale)))
        entry->scale_percent = (int)scale;
    return TRUE;
}

void host_displays(HostDisplays *displays)
{
    memset(displays, 0, sizeof(*displays));
    DPI_AWARENESS_CONTEXT previous = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    EnumDisplayMonitors(NULL, NULL, add_display, (LPARAM)displays);
    if (previous)
        SetThreadDpiAwarenessContext(previous);
}

static void read_desktop(Desktop *snapshot)
{
    host_displays(&snapshot->displays);
    if (current_session() != HOST_UNLOCKED)
        return;
    snapshot->cursor_valid = GetPhysicalCursorPos(&snapshot->cursor);
    const HWND window = GetForegroundWindow();
    if (!window)
        return;
    snapshot->foreground = TRUE;
    const int title = GetWindowTextW(window, snapshot->title, (int)_countof(snapshot->title));
    snapshot->truncated = title == _countof(snapshot->title) - 1;
    DWORD pid = 0;
    GetWindowThreadProcessId(window, &pid);
    const HANDLE process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid);
    if (process) {
        wchar_t full[32768];
        DWORD length = _countof(full);
        if (QueryFullProcessImageNameW(process, 0, full, &length)) {
            const wchar_t *name = wcsrchr(full, L'\\');
            if (wcsncpy_s(snapshot->process, _countof(snapshot->process), name ? name + 1 : full,
                          _TRUNCATE) == STRUNCATE)
                snapshot->truncated = TRUE;
        }
        CloseHandle(process);
    }
}

/** Copy a bounded CF_UNICODETEXT snapshot while the clipboard is open. */
static const char *read_clipboard(wchar_t *text, size_t capacity, BOOL *truncated)
{
    if (!OpenClipboard(NULL))
        return "clipboard_busy";
    const char *error = NULL;
    if (IsClipboardFormatAvailable(CF_UNICODETEXT)) {
        const HANDLE value = GetClipboardData(CF_UNICODETEXT);
        const wchar_t *source = value ? GlobalLock(value) : NULL;
        if (source) {
            const size_t available = GlobalSize(value) / sizeof(*source);
            const size_t limit = available < capacity ? available : capacity;
            size_t count = 0;
            while (count < limit && source[count])
                ++count;
            if (count == capacity) {
                *truncated = TRUE;
                --count;
                if (count && source[count - 1] >= 0xd800 && source[count - 1] <= 0xdbff)
                    --count;
            } else if (count == available) {
                error = "clipboard_unavailable";
            }
            memcpy(text, source, count * sizeof(*text));
            text[count] = L'\0';
            GlobalUnlock(value);
        } else {
            error = "clipboard_unavailable";
        }
    }
    CloseClipboard();
    return error;
}

static void append(Json *out, const char *format, ...)
{
    if (out->length >= out->capacity)
        return;
    va_list args;
    va_start(args, format);
    const size_t available = out->capacity - out->length;
    const int n = vsnprintf(out->data + out->length, available, format, args);
    va_end(args);
    out->length = n < 0 || (size_t)n >= available ? out->capacity : out->length + (size_t)n;
}

/** Write an escaped prefix while leaving room for the closing quote and caller's suffix. */
static BOOL string(Json *out, const wchar_t *value, size_t reserve)
{
    if (!value) {
        append(out, "null");
        return FALSE;
    }
    append(out, "\"");
    BOOL truncated = FALSE;
    while (*value) {
        unsigned scalar = *value++;
        if (scalar >= 0xd800 && scalar <= 0xdbff && *value >= 0xdc00 && *value <= 0xdfff)
            scalar = 0x10000 + ((scalar - 0xd800) << 10) + (*value++ - 0xdc00);
        else if (scalar >= 0xd800 && scalar <= 0xdfff)
            scalar = 0xfffd;
        char encoded[7];
        size_t size;
        if (scalar == '"' || scalar == '\\') {
            encoded[0] = '\\';
            encoded[1] = (char)scalar;
            size = 2;
        } else if (scalar < 32) {
            snprintf(encoded, sizeof(encoded), "\\u%04x", scalar);
            size = 6;
        } else if (scalar < 0x80) {
            encoded[0] = (char)scalar;
            size = 1;
        } else if (scalar < 0x800) {
            encoded[0] = (char)(0xc0 | (scalar >> 6));
            encoded[1] = (char)(0x80 | (scalar & 0x3f));
            size = 2;
        } else if (scalar < 0x10000) {
            encoded[0] = (char)(0xe0 | (scalar >> 12));
            encoded[1] = (char)(0x80 | ((scalar >> 6) & 0x3f));
            encoded[2] = (char)(0x80 | (scalar & 0x3f));
            size = 3;
        } else {
            encoded[0] = (char)(0xf0 | (scalar >> 18));
            encoded[1] = (char)(0x80 | ((scalar >> 12) & 0x3f));
            encoded[2] = (char)(0x80 | ((scalar >> 6) & 0x3f));
            encoded[3] = (char)(0x80 | (scalar & 0x3f));
            size = 4;
        }
        if (out->length + size + reserve + 2 > out->capacity) {
            truncated = TRUE;
            break;
        }
        memcpy(out->data + out->length, encoded, size);
        out->length += size;
    }
    append(out, "\"");
    return truncated;
}

static unsigned capture_display(const HostDisplays *displays, const wchar_t *configured)
{
    unsigned primary = 0;
    for (unsigned i = 0; i < displays->count; ++i) {
        const HostDisplay *display = &displays->items[i];
        if (configured[0] && _wcsicmp(display->id, configured) == 0)
            return i;
        if (display->primary)
            primary = i;
    }
    return primary;
}

static void display_json(Json *out, const HostDisplay *display, BOOL captured)
{
    const RECT *r = &display->area;
    append(out, "{\"id\":");
    string(out, display->id, 0);
    append(out, ",\"name\":");
    string(out, display->name, 0);
    append(out, ",\"x\":%ld,\"y\":%ld,\"width\":%ld,\"height\":%ld,\"scale\":",
           r->left, r->top, r->right - r->left, r->bottom - r->top);
    if (display->scale_percent > 0)
        append(out, "%d.%02d", display->scale_percent / 100, display->scale_percent % 100);
    else
        append(out, "null");
    append(out, ",\"primary\":%s,\"captured\":%s}", display->primary ? "true" : "false",
           captured ? "true" : "false");
}

static void cursor_json(Json *out, const Desktop *desktop, unsigned capture)
{
    const RECT *rect = &desktop->displays.items[capture].area;
    const POINT point = desktop->cursor;
    if (!desktop->cursor_valid || !desktop->displays.count || !PtInRect(rect, point)) {
        append(out, "null");
        return;
    }
    const LONG width = rect->right - rect->left, height = rect->bottom - rect->top;
    const long x = width > 1 ? MulDiv(point.x - rect->left, 1000000, width - 1) : 0;
    const long y = height > 1 ? MulDiv(point.y - rect->top, 1000000, height - 1) : 0;
    append(out, "{\"x\":%ld.%06ld,\"y\":%ld.%06ld}", x / 1000000, x % 1000000,
           y / 1000000, y % 1000000);
}

static void info_reply(Json *out, const HostSettings *settings, const Sources *sources)
{
    Desktop desktop = {0};
    sources->desktop(&desktop);
    const HostSession session = sources->session();
    const unsigned capture = capture_display(&desktop.displays, settings->monitor);
    BOOL truncated = desktop.truncated || desktop.displays.truncated;
    append(out, "{\"ok\":true,\"data\":{\"foreground\":");
    if (desktop.foreground && session == HOST_UNLOCKED) {
        append(out, "{\"title\":");
        string(out, desktop.title[0] ? desktop.title : NULL, 0);
        append(out, ",\"process\":");
        string(out, desktop.process[0] ? desktop.process : NULL, 0);
        append(out, "}");
    } else {
        append(out, "null");
    }
    if (session != HOST_UNLOCKED)
        desktop.cursor_valid = FALSE;
    append(out, ",\"cursor\":");
    cursor_json(out, &desktop, capture);
    append(out, ",\"session\":\"%s\",\"displays\":[", session_names[session]);
    for (unsigned i = 0; i < desktop.displays.count; ++i) {
        /* Keep the captured display when the reply cannot fit a large layout. */
        const unsigned index = i == 0 ? capture : i <= capture ? i - 1 : i;
        char entry[2560];
        Json item = {entry, 0, sizeof(entry)};
        display_json(&item, &desktop.displays.items[index], index == capture);
        if (item.length >= item.capacity || out->length + item.length + 30 >= out->capacity) {
            truncated = TRUE;
            break;
        }
        append(out, "%s%s", i ? "," : "", entry);
    }
    append(out, "]%s}}", truncated ? ",\"truncated\":true" : "");
}

static const char *clipboard_reply(Json *out, const HostSettings *settings, const Sources *sources)
{
    if (!settings->clipboard || sources->session() != HOST_UNLOCKED)
        return "host_feature_disabled";
    wchar_t text[HOST_REPLY_LIMIT + 1] = {0};
    BOOL truncated = FALSE;
    const char *error = sources->clipboard(text, _countof(text), &truncated);
    if (error)
        return error;
    if (sources->session() != HOST_UNLOCKED)
        return "host_feature_disabled";
    append(out, "{\"ok\":true,\"data\":{\"text\":");
    truncated |= string(out, text, sizeof(",\"truncated\":false}}") - 1);
    append(out, ",\"truncated\":%s}}", truncated ? "true" : "false");
    return NULL;
}

static size_t dispatch(const char *method, const HostSettings *settings, DEVINST parent,
                       ULONGLONG deadline, char reply[HOST_REPLY_LIMIT + 1], const Sources *sources)
{
    Json out = {reply, 0, HOST_REPLY_LIMIT + 1};
    const char *error = NULL;
    if (!strcmp(method, "info")) {
        info_reply(&out, settings, sources);
    } else if (!strcmp(method, "clipboard")) {
        error = clipboard_reply(&out, settings, sources);
    } else if (!strcmp(method, "eject")) {
        error = sources->eject(parent, deadline);
        if (!error)
            append(&out, "{\"ok\":true,\"data\":{}}");
    } else {
        error = "host_feature_disabled";
    }
    if (out.length >= out.capacity)
        error = "reply_too_large";
    if (error) {
        out.length = 0;
        append(&out, "{\"ok\":false,\"error\":\"%s\"}", error);
    }
    return out.length;
}

size_t host_reply(const char *method, const HostSettings *settings, DEVINST parent,
                  ULONGLONG deadline, char reply[HOST_REPLY_LIMIT + 1])
{
    const Sources sources = {current_session, read_desktop, read_clipboard, host_eject};
    return dispatch(method, settings, parent, deadline, reply, &sources);
}

static BOOL wire_report(const unsigned char packet[HOST_REPORT_SIZE], char text[HOST_REPORT_SIZE])
{
    if (packet[0] != 4)
        return FALSE;
    memcpy(text, packet + 1, HOST_REPORT_SIZE - 1);
    text[HOST_REPORT_SIZE - 1] = '\0';
    for (size_t i = strlen(text) + 1; i < HOST_REPORT_SIZE - 1; ++i)
        if (text[i] != '\0')
            return FALSE;
    return TRUE;
}

static BOOL decimal(const char **position, unsigned *value)
{
    const char *p = *position;
    if (*p < '0' || *p > '9')
        return FALSE;
    unsigned n = 0;
    do {
        n = n * 10 + (unsigned)(*p++ - '0');
        if (n > 65535)
            return FALSE;
    } while (*p >= '0' && *p <= '9');
    *position = p;
    *value = n;
    return TRUE;
}

BOOL host_request(const unsigned char packet[HOST_REPORT_SIZE], unsigned *seq, char method[64])
{
    char text[HOST_REPORT_SIZE];
    if (!wire_report(packet, text) || text[0] != '?')
        return FALSE;
    const char *p = text + 1;
    if (!decimal(&p, seq) || *p++ != ':' || !*p)
        return FALSE;
    if (strspn(p, "abcdefghijklmnopqrstuvwxyz") != strlen(p))
        return FALSE;
    strcpy_s(method, 64, p);
    return TRUE;
}

BOOL host_ack(const unsigned char packet[HOST_REPORT_SIZE], unsigned seq, unsigned part)
{
    char text[HOST_REPORT_SIZE];
    if (!wire_report(packet, text) || text[0] != '!')
        return FALSE;
    const char *p = text + 1;
    unsigned read_seq, read_part;
    return decimal(&p, &read_seq) && *p++ == ':' && decimal(&p, &read_part) && *p == '\0' &&
           read_seq == seq && read_part == part;
}

unsigned host_parts(size_t length)
{
    return length <= HOST_REPLY_LIMIT ? (unsigned)((length + CHUNK - 1) / CHUNK) : 0;
}

size_t host_fragment(char payload[HOST_REPORT_SIZE], unsigned seq, unsigned part,
                     const char *reply, size_t length)
{
    const unsigned total = host_parts(length);
    if (seq > 65535 || !part || part > total)
        return 0;
    const int header = snprintf(payload, HOST_REPORT_SIZE, "<%u:%u/%u:", seq, part, total);
    const size_t offset = (part - 1) * CHUNK;
    const size_t size = length - offset < CHUNK ? length - offset : CHUNK;
    memcpy(payload + header, reply + offset, size);
    return (size_t)header + size;
}

static HostSession fixture_state;
static unsigned fixture_reads;
static BOOL fixture_large, fixture_lock_on_read;

static HostSession fixture_session(void)
{
    return fixture_state;
}

static void fixture_desktop(Desktop *desktop)
{
    desktop->foreground = TRUE;
    wcscpy_s(desktop->title, _countof(desktop->title), L"Test \"window\" 中文");
    wcscpy_s(desktop->process, _countof(desktop->process), L"test.exe");
    desktop->displays.count = 2;
    desktop->displays.items[0] = (HostDisplay){.id = L"primary", .name = L"First", .primary = TRUE,
                                               .area = {0, 0, 1920, 1080}, .scale_percent = 100};
    desktop->displays.items[1] = (HostDisplay){.id = L"target", .name = L"Second",
                                               .area = {-101, 0, 0, 101}, .scale_percent = 150};
    desktop->cursor_valid = TRUE;
    desktop->cursor = (POINT){-51, 100};
}

static const char *fixture_clipboard(wchar_t *text, size_t capacity, BOOL *truncated)
{
    ++fixture_reads;
    *truncated = FALSE;
    if (fixture_large) {
        for (size_t i = 0; i + 2 < capacity; i += 2) {
            text[i] = 0xd83d;
            text[i + 1] = 0xde00;
        }
    } else {
        wcscpy_s(text, capacity, L"first\r\n中文😀");
    }
    if (fixture_lock_on_read)
        fixture_state = HOST_LOCKED;
    return NULL;
}

static const char *fixture_eject(DEVINST parent, ULONGLONG deadline)
{
    if (parent != 17)
        return "eject_device_ambiguous";
    return deadline == 42 ? NULL : "eject_busy";
}

/** Assemble an input report, dispatch the request, then receive and ACK each output fragment. */
static BOOL fixture_exchange(const char *request, const HostSettings *settings, DEVINST parent,
                             ULONGLONG deadline, char received[HOST_REPLY_LIMIT + 1])
{
    const Sources provider = {fixture_session, fixture_desktop, fixture_clipboard, fixture_eject};
    unsigned char packet[HOST_REPORT_SIZE] = {4};
    strcpy_s((char *)packet + 1, sizeof(packet) - 1, request);
    unsigned seq;
    char method[HOST_REPORT_SIZE];
    if (!host_request(packet, &seq, method))
        return FALSE;
    char reply[HOST_REPLY_LIMIT + 1];
    const size_t length = dispatch(method, settings, parent, deadline, reply, &provider);
    const unsigned total = host_parts(length);
    size_t assembled = 0;
    for (unsigned part = 1; part <= total; ++part) {
        char payload[HOST_REPORT_SIZE] = {0};
        const size_t bytes = host_fragment(payload, seq, part, reply, length);
        unsigned c_seq = 0, c_part = 0, c_total = 0;
        int offset = 0;
        if (!bytes || bytes >= HOST_REPORT_SIZE ||
            sscanf_s(payload, "<%u:%u/%u:%n", &c_seq, &c_part, &c_total, &offset) != 3 ||
            seq != c_seq || part != c_part || total != c_total || offset <= 0 || (size_t)offset > bytes)
            return FALSE;
        const size_t count = bytes - (size_t)offset;
        memcpy(received + assembled, payload + offset, count);
        assembled += count;
        memset(packet + 1, 0, sizeof(packet) - 1);
        snprintf((char *)packet + 1, sizeof(packet) - 1, "!%u:%u", c_seq, c_part);
        if (!host_ack(packet, seq, part) || host_ack(packet, seq, part + 1) || host_ack(packet, seq + 1, part))
            return FALSE;
    }
    received[assembled] = '\0';
    return assembled == length && strcmp(received, reply) == 0 &&
           MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, received, -1, NULL, 0) > 0;
}

int host_agent_test(void)
{
    HostSettings settings = {.clipboard = HOST_CLIPBOARD_DEFAULT, .monitor = L"target"};
    char reply[HOST_REPLY_LIMIT + 1];
    fixture_state = HOST_UNLOCKED;
    if (!fixture_exchange("?65535:info", &settings, 0, 0, reply) ||
        !strstr(reply, "Test \\\"window\\\" 中文") || !strstr(reply, "\"process\":\"test.exe\"") ||
        !strstr(reply, "\"cursor\":{\"x\":0.500000,\"y\":1.000000}") ||
        !strstr(reply, "\"scale\":1.50,\"primary\":false,\"captured\":true"))
        return 1;
    if (!fixture_exchange("?0:clipboard", &settings, 0, 0, reply) || fixture_reads ||
        strcmp(reply, "{\"ok\":false,\"error\":\"host_feature_disabled\"}"))
        return 2;
    settings.clipboard = TRUE;
    if (!fixture_exchange("?1:clipboard", &settings, 0, 0, reply) || fixture_reads != 1 ||
        strcmp(reply, "{\"ok\":true,\"data\":{\"text\":\"first\\u000d\\u000a中文😀\",\"truncated\":false}}"))
        return 3;
    fixture_large = TRUE;
    if (!fixture_exchange("?2:clipboard", &settings, 0, 0, reply) || fixture_reads != 2 ||
        !strstr(reply, "\"truncated\":true}}") || strlen(reply) < HOST_REPLY_LIMIT - 5)
        return 4;
    fixture_state = HOST_SECURE_DESKTOP;
    if (!fixture_exchange("?3:clipboard", &settings, 0, 0, reply) || fixture_reads != 2 ||
        !strstr(reply, "host_feature_disabled"))
        return 5;
    fixture_state = HOST_UNLOCKED;
    fixture_lock_on_read = TRUE;
    if (!fixture_exchange("?4:clipboard", &settings, 0, 0, reply) || !strstr(reply, "host_feature_disabled"))
        return 6;
    if (!fixture_exchange("?5:info", &settings, 0, 0, reply) ||
        !strstr(reply, "\"foreground\":null,\"cursor\":null,\"session\":\"locked\""))
        return 7;
    if (!fixture_exchange("?6:eject", &settings, 17, 42, reply) || strcmp(reply, "{\"ok\":true,\"data\":{}}") ||
        !fixture_exchange("?7:eject", &settings, 99, 42, reply) || !strstr(reply, "eject_device_ambiguous") ||
        !fixture_exchange("?8:eject", &settings, 17, 0, reply) || !strstr(reply, "eject_busy"))
        return 8;
    if (fixture_exchange("?65536:info", &settings, 0, 0, reply) ||
        fixture_exchange("?0:info:unexpected", &settings, 0, 0, reply))
        return 9;
    return 0;
}
