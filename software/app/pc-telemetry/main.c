/*
 * main.c
 * Created on Nov 23, 2026
 * Author: @POEG1726
 */

#include "telemetry.h"
#include <math.h>
#include <shellapi.h>
#include <stdio.h>
#include <stdlib.h>
#include <wchar.h>

enum {
    IDC_INTERVAL = 101, IDC_APPLY, IDC_SETTINGS, IDC_EXIT, IDC_CLIPBOARD, IDC_CAPTURE, IDC_GROUP = 120,
    WM_TRAY = WM_APP + 1, WM_SAMPLE, WM_SHOW_SETTINGS,
};
static const wchar_t window_class[] = L"KVMPanelTelemetry";
static const wchar_t settings_path[] = L"Software\\KVMPanel\\Telemetry";
static const wchar_t *group_names[] = {
    L"CPU 负载与基本传感器", L"内存与 DIMM 温度", L"显卡基本指标",
    L"显存结温（Memory Junction）", L"每张网络适配器", L"磁盘温度与读写",
    L"超频读数（驱动支持的电压、时钟和功耗限制）",
    L"GPU 核心热点（Core Junction / Hot Spot）",
};

typedef struct {
    unsigned interval, groups, generation;
    HostSettings host;
} Options;

static SRWLOCK options_lock = SRWLOCK_INIT;
static const Options defaults = {.interval = DEFAULT_INTERVAL, .groups = ALL_GROUPS, .generation = 1,
                                 .host = {.clipboard = HOST_CLIPBOARD_DEFAULT}};
static Options options;
static HostDisplays capture_displays;
static wchar_t unavailable_capture[MAX_DEVICE_ID_LEN];
static HANDLE wake, worker, usb_ready, usb_cancel, usb_worker;
static volatile LONG stopping;
static SRWLOCK sample_lock = SRWLOCK_INIT;
static Sample latest;
static unsigned sample_generation;
static uint64_t sample_number;
static volatile LONG usb_connected;
static HWND status_control;
static NOTIFYICONDATAW tray;
static UINT taskbar_created, dpi = 96;
static BOOL in_tray;

static Options read_options(void)
{
    AcquireSRWLockShared(&options_lock);
    Options copy = options;
    ReleaseSRWLockShared(&options_lock);
    return copy;
}

static void load_options(void)
{
    options = defaults;
    DWORD interval = DEFAULT_INTERVAL, groups = ALL_GROUPS, size = sizeof(DWORD);
    RegGetValueW(HKEY_CURRENT_USER, settings_path, L"IntervalMs", RRF_RT_REG_DWORD,
                 NULL, &interval, &size);
    size = sizeof(DWORD);
    RegGetValueW(HKEY_CURRENT_USER, settings_path, L"Groups", RRF_RT_REG_DWORD,
                 NULL, &groups, &size);
    if (interval >= MIN_INTERVAL && interval <= MAX_INTERVAL)
        options.interval = interval;
    options.groups = groups & ALL_GROUPS;
    DWORD clipboard = HOST_CLIPBOARD_DEFAULT;
    size = sizeof(clipboard);
    if (RegGetValueW(HKEY_CURRENT_USER, settings_path, L"AllowClipboard", RRF_RT_REG_DWORD,
                     NULL, &clipboard, &size) == ERROR_SUCCESS)
        options.host.clipboard = clipboard == TRUE;
    size = sizeof(options.host.monitor);
    if (RegGetValueW(HKEY_CURRENT_USER, settings_path, L"CaptureMonitor", RRF_RT_REG_SZ,
                     NULL, options.host.monitor, &size) != ERROR_SUCCESS)
        options.host.monitor[0] = L'\0';
}

static void session_marker_name(wchar_t name[56], DWORD session)
{
    swprintf_s(name, 56, L"Global\\KVMPanelTelemetrySession.%lu", session);
}

/** Prefer the active user's instance, while a sole background instance can still report disabled. */
static BOOL active_instance_exists(void)
{
    const DWORD session = WTSGetActiveConsoleSessionId();
    if (session == MAXDWORD || host_local_session())
        return FALSE;
    wchar_t name[56];
    session_marker_name(name, session);
    HANDLE instance = OpenEventW(SYNCHRONIZE, FALSE, name);
    if (!instance)
        return GetLastError() == ERROR_ACCESS_DENIED;
    CloseHandle(instance);
    return TRUE;
}

static BOOL save_options(unsigned interval, unsigned groups, const HostSettings *host)
{
    HKEY key;
    if (RegCreateKeyExW(HKEY_CURRENT_USER, settings_path, 0, NULL, 0, KEY_SET_VALUE,
                       NULL, &key, NULL) != ERROR_SUCCESS)
        return FALSE;
    const LSTATUS interval_result = RegSetValueExW(key, L"IntervalMs", 0, REG_DWORD,
                                                   (const BYTE *)&interval, sizeof(interval));
    const LSTATUS group_result = RegSetValueExW(key, L"Groups", 0, REG_DWORD,
                                                (const BYTE *)&groups, sizeof(groups));
    const DWORD clipboard = host->clipboard ? 1u : 0u;
    const LSTATUS clipboard_result = RegSetValueExW(key, L"AllowClipboard", 0, REG_DWORD,
                                                    (const BYTE *)&clipboard, sizeof(clipboard));
    const DWORD monitor_size = (DWORD)((wcslen(host->monitor) + 1) * sizeof(*host->monitor));
    const LSTATUS monitor_result = RegSetValueExW(key, L"CaptureMonitor", 0, REG_SZ,
                                                  (const BYTE *)host->monitor, monitor_size);
    RegCloseKey(key);
    if (interval_result != ERROR_SUCCESS || group_result != ERROR_SUCCESS ||
        clipboard_result != ERROR_SUCCESS || monitor_result != ERROR_SUCCESS)
        return FALSE;
    AcquireSRWLockExclusive(&options_lock);
    options.interval = interval;
    options.groups = groups;
    options.host = *host;
    ++options.generation;
    SetEvent(usb_cancel);
    ReleaseSRWLockExclusive(&options_lock);
    SetEvent(wake);
    SetEvent(usb_ready);
    return TRUE;
}

/** Sample independently of USB transfer speed. */
static DWORD WINAPI sample_worker(void *context)
{
    const HWND window = context;
    Sources *sources = sources_open();
    Sample *sample = calloc(1, sizeof(*sample));
    if (!sources || !sample) {
        PostMessageW(window, WM_SAMPLE, 0, 8);
        goto done;
    }
    while (!InterlockedCompareExchange(&stopping, 0, 0)) {
        const Options settings = read_options();
        const ULONGLONG started = GetTickCount64();
        sources_sample(sources, sample, settings.groups);
        AcquireSRWLockExclusive(&sample_lock);
        latest.count = sample->count;
        memcpy(latest.metrics, sample->metrics, sample->count * sizeof(sample->metrics[0]));
        sample_generation = settings.generation;
        ++sample_number;
        ReleaseSRWLockExclusive(&sample_lock);
        SetEvent(usb_ready);
        PostMessageW(window, WM_SAMPLE, sample->available,
                     (InterlockedCompareExchange(&usb_connected, 0, 0) ? 1 : 0) |
                     (sample->hotspots ? 2 : 0) | (sample->junctions ? 4 : 0) |
                     (sample->hotspot_access_denied ? 16 : 0));
        const ULONGLONG elapsed = GetTickCount64() - started;
        WaitForSingleObject(wake, elapsed < settings.interval
                            ? (DWORD)(settings.interval - elapsed) : 0);
    }
done:
    sources_close(sources);
    free(sample);
    return 0;
}

/** Deliver the latest complete sample and coalesce queued samples if the USB link is slower. */
static DWORD WINAPI transmit_worker(void *context)
{
    (void)context;
    Sample *sample = calloc(1, sizeof(*sample));
    Usb *usb = NULL;
    if (!sample)
        return 1;
    ULONGLONG next_usb = 0, next_labels = 0;
    uint64_t session = GetTickCount64() ^ ((uint64_t)GetCurrentProcessId() << 32);
    uint64_t sent = 0;
    unsigned generation = 0;
    while (!InterlockedCompareExchange(&stopping, 0, 0)) {
        AcquireSRWLockShared(&options_lock);
        const Options settings = options;
        if (settings.generation != generation && !InterlockedCompareExchange(&stopping, 0, 0))
            ResetEvent(usb_cancel);
        ReleaseSRWLockShared(&options_lock);
        const ULONGLONG now = GetTickCount64();
        if (settings.generation != generation) {
            generation = settings.generation;
            ++session;
            next_labels = now;
            sent = 0;
        }
        if (active_instance_exists()) {
            usb_close(usb);
            usb = NULL;
            InterlockedExchange(&usb_connected, FALSE);
            WaitForSingleObject(usb_ready, 250);
            continue;
        }
        if (now >= next_usb && !usb) {
            usb = usb_open();
            next_usb = now + 2500;
            if (usb) {
                ++session;
                next_labels = now;
                sent = 0;
            }
        }
        if (usb) {
            const UsbResult agent = usb_service_agent(usb, &settings.host, usb_cancel);
            if (agent == USB_LOST) {
                usb_close(usb);
                usb = NULL;
            } else if (agent == USB_INTERRUPTED) {
                continue;
            }
        }
        const ULONGLONG send_now = GetTickCount64();
        if (usb) {
            usb_configure(usb, session, settings.interval, settings.host.clipboard);
            const UsbResult announcements = usb_announce(usb, usb_cancel);
            if (announcements == USB_LOST) {
                usb_close(usb);
                usb = NULL;
            } else if (announcements == USB_INTERRUPTED) {
                continue;
            }
        }
        if (usb) {
            uint64_t captured = 0;
            AcquireSRWLockShared(&sample_lock);
            if (sample_generation == generation && sent != sample_number) {
                sample->count = latest.count;
                memcpy(sample->metrics, latest.metrics, latest.count * sizeof(latest.metrics[0]));
                captured = sample_number;
            }
            ReleaseSRWLockShared(&sample_lock);
            const BOOL labels = send_now >= next_labels;
            if (captured) {
                const UsbResult result = usb_send(usb, sample, labels, usb_cancel);
                if (result == USB_LOST) {
                    usb_close(usb);
                    usb = NULL;
                }
                sent = captured;
                if (labels && result == USB_OK)
                    next_labels = send_now + 30000;
                if (result == USB_INTERRUPTED)
                    continue;
            }
        }
        InterlockedExchange(&usb_connected, usb != NULL);
        const ULONGLONG deadline = usb ? usb_announcement_deadline(usb) : next_usb;
        const ULONGLONG after = GetTickCount64();
        const HANDLE signals[] = {usb_ready, usb_input_event(usb)};
        WaitForMultipleObjects(signals[1] ? 2 : 1, signals, FALSE,
                               after < deadline ? (DWORD)(deadline - after) : 0);
    }
    usb_close(usb);
    free(sample);
    return 0;
}

static int px(int value)
{
    return MulDiv(value, (int)dpi, 96);
}

static HWND control(HWND parent, DWORD style, const wchar_t *klass, const wchar_t *text,
                    int id, int x, int y, int width, int height)
{
    HWND child = CreateWindowExW(klass[0] == L'E' ? WS_EX_CLIENTEDGE : 0,
        klass, text, WS_CHILD | WS_VISIBLE | style, px(x), px(y), px(width), px(height),
        parent, (HMENU)(INT_PTR)id, GetModuleHandleW(NULL), NULL);
    SendMessageW(child, WM_SETFONT, (WPARAM)GetStockObject(DEFAULT_GUI_FONT), TRUE);
    return child;
}

/** Keep the selected monitor ID when Windows reports a changed display arrangement. */
static void fill_capture(HWND window, const wchar_t *chosen)
{
    HWND combo = GetDlgItem(window, IDC_CAPTURE);
    SendMessageW(combo, CB_RESETCONTENT, 0, 0);
    SendMessageW(combo, CB_ADDSTRING, 0, (LPARAM)L"主显示器（自动）");
    host_displays(&capture_displays);
    unsigned selected = 0;
    for (unsigned i = 0; i < capture_displays.count; ++i) {
        const HostDisplay *display = &capture_displays.items[i];
        wchar_t label[180];
        swprintf_s(label, _countof(label), L"%ls (%ld×%ld, %ld,%ld)", display->name,
                   display->area.right - display->area.left, display->area.bottom - display->area.top,
                   display->area.left, display->area.top);
        SendMessageW(combo, CB_ADDSTRING, 0, (LPARAM)label);
        if (_wcsicmp(chosen, display->id) == 0)
            selected = i + 1;
    }
    unavailable_capture[0] = L'\0';
    if (!selected && chosen[0]) {
        wcscpy_s(unavailable_capture, _countof(unavailable_capture), chosen);
        SendMessageW(combo, CB_ADDSTRING, 0, (LPARAM)L"已保存的显示器（当前未连接）");
        selected = capture_displays.count + 1;
    }
    SendMessageW(combo, CB_SETCURSEL, selected, 0);
}

static void create_controls(HWND window)
{
    wchar_t interval[16];
    swprintf_s(interval, _countof(interval), L"%u", options.interval);
    control(window, 0, L"STATIC", L"采样间隔（毫秒）", 0, 18, 18, 150, 22);
    HWND edit = control(window, WS_TABSTOP | ES_NUMBER | ES_AUTOHSCROLL, L"EDIT", interval,
                        IDC_INTERVAL, 168, 14, 105, 25);
    SendMessageW(edit, EM_SETLIMITTEXT, 5, 0);
    control(window, 0, L"STATIC", L"250–60000", 0, 284, 18, 100, 22);
    control(window, 0, L"STATIC", L"采集内容", 0, 18, 58, 190, 22);
    for (unsigned i = 0; i < _countof(group_names); ++i) {
        HWND button = control(window, WS_TABSTOP | BS_AUTOCHECKBOX, L"BUTTON", group_names[i],
                              IDC_GROUP + (int)i, 18, 82 + (int)i * 25, 416, 22);
        SendMessageW(button, BM_SETCHECK, options.groups & (1u << i) ? BST_CHECKED : BST_UNCHECKED, 0);
    }
    HWND clipboard = control(window, WS_TABSTOP | BS_AUTOCHECKBOX, L"BUTTON", L"允许 KVM 读取剪贴板",
                             IDC_CLIPBOARD, 18, 291, 415, 22);
    SendMessageW(clipboard, BM_SETCHECK, options.host.clipboard ? BST_CHECKED : BST_UNCHECKED, 0);
    control(window, 0, L"STATIC", L"KVM 采集的显示器", 0, 18, 322, 150, 22);
    control(window, WS_TABSTOP | CBS_DROPDOWNLIST | WS_VSCROLL, L"COMBOBOX", L"", IDC_CAPTURE,
            168, 318, 264, 190);
    fill_capture(window, options.host.monitor);
    status_control = control(window, SS_LEFT, L"STATIC", L"正在采样…", 0, 18, 356, 420, 48);
    control(window, WS_TABSTOP | BS_DEFPUSHBUTTON, L"BUTTON", L"应用", IDC_APPLY,
            337, 410, 95, 30);
}

static void add_tray(HWND window)
{
    tray.cbSize = sizeof(tray);
    tray.hWnd = window;
    tray.uID = 1;
    tray.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP;
    tray.uCallbackMessage = WM_TRAY;
    tray.hIcon = LoadIconW(NULL, IDI_APPLICATION);
    wcscpy_s(tray.szTip, _countof(tray.szTip), L"KVMPanel 遥测");
    in_tray = Shell_NotifyIconW(NIM_ADD, &tray);
}

static void show_settings(HWND window)
{
    ShowWindow(window, SW_RESTORE);
    SetForegroundWindow(window);
}

static void apply_controls(HWND window)
{
    wchar_t text[16], *end;
    GetDlgItemTextW(window, IDC_INTERVAL, text, _countof(text));
    const unsigned long interval = wcstoul(text, &end, 10);
    if (end == text || *end || interval < MIN_INTERVAL || interval > MAX_INTERVAL) {
        MessageBoxW(window, L"采样间隔请输入 250–60000 毫秒。", L"KVMPanel 遥测", MB_ICONWARNING);
        return;
    }
    unsigned groups = 0;
    for (unsigned i = 0; i < _countof(group_names); ++i)
        if (IsDlgButtonChecked(window, IDC_GROUP + (int)i) == BST_CHECKED)
            groups |= 1u << i;
    HostSettings host = {.clipboard = IsDlgButtonChecked(window, IDC_CLIPBOARD) == BST_CHECKED};
    const LRESULT index = SendDlgItemMessageW(window, IDC_CAPTURE, CB_GETCURSEL, 0, 0);
    if (index > 0 && index <= (LRESULT)capture_displays.count)
        wcscpy_s(host.monitor, _countof(host.monitor), capture_displays.items[index - 1].id);
    else if (index == (LRESULT)capture_displays.count + 1)
        wcscpy_s(host.monitor, _countof(host.monitor), unavailable_capture);
    if (!save_options((unsigned)interval, groups, &host))
        MessageBoxW(window, L"无法保存当前用户设置。", L"KVMPanel 遥测", MB_ICONERROR);
}

static LRESULT CALLBACK window_proc(HWND window, UINT message, WPARAM wp, LPARAM lp)
{
    if (message == taskbar_created && taskbar_created) {
        add_tray(window);
        return 0;
    }
    switch (message) {
    case WM_CREATE:
        create_controls(window);
        add_tray(window);
        return 0;
    case WM_SHOW_SETTINGS:
        show_settings(window);
        return 0;
    case WM_DISPLAYCHANGE: {
        const Options refreshed = read_options();
        fill_capture(window, refreshed.host.monitor);
        return 0;
    }
    case WM_TRAY:
        if (lp == WM_LBUTTONDBLCLK) {
            show_settings(window);
        } else if (lp == WM_RBUTTONUP || lp == WM_CONTEXTMENU) {
            POINT pointer;
            GetCursorPos(&pointer);
            HMENU menu = CreatePopupMenu();
            AppendMenuW(menu, MF_STRING, IDC_SETTINGS, L"设置");
            AppendMenuW(menu, MF_STRING, IDC_EXIT, L"退出");
            SetForegroundWindow(window);
            TrackPopupMenu(menu, TPM_RIGHTBUTTON, pointer.x, pointer.y, 0, window, NULL);
            DestroyMenu(menu);
            PostMessageW(window, WM_NULL, 0, 0);
        }
        return 0;
    case WM_SAMPLE: {
        wchar_t text[184];
        if (lp & 8)
            wcscpy_s(text, _countof(text), L"采集器初始化失败。");
        else
            swprintf_s(text, _countof(text), L"采集 %u 项    USB %ls\r\n核心热点 %ls    显存结温 %ls",
                       (unsigned)wp, lp & 1 ? L"已连接" : L"未连接",
                       lp & 2 ? L"有读数" : lp & 16 ? L"需管理员权限" : L"无可用读数",
                       lp & 4 ? L"有读数" : L"无可用读数");
        if (IsWindowVisible(window))
            SetWindowTextW(status_control, text);
        else {
            swprintf_s(tray.szTip, _countof(tray.szTip), L"KVMPanel 遥测 · %u 项 · USB %ls",
                       (unsigned)wp, lp & 1 ? L"已连接" : L"未连接");
            Shell_NotifyIconW(NIM_MODIFY, &tray);
        }
        return 0;
    }
    case WM_COMMAND:
        switch (LOWORD(wp)) {
        case IDC_APPLY: apply_controls(window); return 0;
        case IDC_SETTINGS: show_settings(window); return 0;
        case IDC_EXIT: DestroyWindow(window); return 0;
        }
        break;
    case WM_CLOSE:
        if (in_tray) {
            ShowWindow(window, SW_HIDE);
            return 0;
        }
        break;
    case WM_DESTROY:
        InterlockedExchange(&stopping, 1);
        SetEvent(wake);
        SetEvent(usb_cancel);
        SetEvent(usb_ready);
        Shell_NotifyIconW(NIM_DELETE, &tray);
        PostQuitMessage(0);
        return 0;
    }
    return DefWindowProcW(window, message, wp, lp);
}

static void print_json_string(const char *value)
{
    putchar('"');
    for (; *value; ++value) {
        const unsigned char ch = (unsigned char)*value;
        if (ch == '"' || ch == '\\')
            putchar('\\');
        if (ch < 32)
            printf("\\u%04x", (unsigned)ch);
        else
            putchar(ch);
    }
    putchar('"');
}

/** Probe the active configuration without opening the tray or sending USB reports. */
static int probe(void)
{
    HANDLE output = GetStdHandle(STD_OUTPUT_HANDLE);
    if (!output || output == INVALID_HANDLE_VALUE) {
        if (AttachConsole(ATTACH_PARENT_PROCESS)) {
            FILE *console;
            freopen_s(&console, "CONOUT$", "w", stdout);
        }
    }
    Sources *sources = sources_open();
    Sample *sample = calloc(1, sizeof(*sample));
    if (!sources || !sample) {
        sources_close(sources);
        free(sample);
        return 1;
    }
    sources_sample(sources, sample, options.groups | PROBE_ALL_PMIC_RAILS);
    Sleep(500);
    sources_sample(sources, sample, options.groups | PROBE_ALL_PMIC_RAILS);
    printf("{\"hotspot_devices\":%u,\"memory_junction_devices\":%u,"
           "\"hotspot_access_denied\":%s,\"ryzen_access_denied\":%s,\"metrics\":[",
           sample->hotspots, sample->junctions, sample->hotspot_access_denied ? "true" : "false",
           sample->ryzen_access_denied ? "true" : "false");
    for (unsigned i = 0; i < sample->count; ++i) {
        Metric *metric = &sample->metrics[i];
        printf("%s{\"id\":", i ? "," : "");
        print_json_string(metric->id);
        printf(",\"device\":");
        print_json_string(metric->label);
        if (isfinite(metric->value))
            printf(",\"value\":%.10g}", metric->value);
        else
            printf(",\"value\":null}");
    }
    puts("]}");
    sources_close(sources);
    free(sample);
    return ferror(stdout) ? 1 : 0;
}

int WINAPI wWinMain(HINSTANCE instance, HINSTANCE previous, PWSTR arguments, int show)
{
    (void)previous;
    (void)arguments;
    int count;
    wchar_t **argv = CommandLineToArgvW(GetCommandLineW(), &count);
    if (!argv)
        return 1;
    BOOL once = count == 2 && wcscmp(argv[1], L"--once") == 0;
    BOOL hidden = count == 2 && wcscmp(argv[1], L"--tray") == 0;
    BOOL test = count == 2 && wcscmp(argv[1], L"--test-host-agent") == 0;
    LocalFree(argv);
    if (test)
        return defaults.host.clipboard == FALSE ? host_agent_test() : 1;
    load_options();
    if (once)
        return probe();

    HANDLE singleton = CreateMutexW(NULL, FALSE, L"Local\\KVMPanelTelemetry");
    if (!singleton)
        return 1;
    if (GetLastError() == ERROR_ALREADY_EXISTS) {
        HWND current = FindWindowW(window_class, NULL);
        if (current)
            PostMessageW(current, WM_SHOW_SETTINGS, 0, 0);
        CloseHandle(singleton);
        return 0;
    }

    DWORD session;
    wchar_t marker_name[56];
    HANDLE session_marker = NULL;
    if (ProcessIdToSessionId(GetCurrentProcessId(), &session)) {
        session_marker_name(marker_name, session);
        session_marker = CreateEventW(NULL, FALSE, FALSE, marker_name);
    }

    SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_SYSTEM_AWARE);
    dpi = GetDpiForSystem();
    taskbar_created = RegisterWindowMessageW(L"TaskbarCreated");
    wake = CreateEventW(NULL, FALSE, FALSE, NULL);
    usb_ready = CreateEventW(NULL, FALSE, FALSE, NULL);
    usb_cancel = CreateEventW(NULL, TRUE, FALSE, NULL);
    const WNDCLASSW definition = {.lpfnWndProc = window_proc, .hInstance = instance,
        .hIcon = LoadIconW(NULL, IDI_APPLICATION), .hCursor = LoadCursorW(NULL, IDC_ARROW),
        .hbrBackground = (HBRUSH)(COLOR_WINDOW + 1), .lpszClassName = window_class};
    HWND window = NULL;
    if (wake && usb_ready && usb_cancel && RegisterClassW(&definition)) {
        const DWORD style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
        RECT size = {0, 0, px(450), px(455)};
        AdjustWindowRectExForDpi(&size, style, FALSE, 0, dpi);
        window = CreateWindowW(window_class, L"KVMPanel 遥测设置", style,
                               CW_USEDEFAULT, CW_USEDEFAULT, size.right - size.left,
                               size.bottom - size.top, NULL, NULL, instance, NULL);
    }
    if (window) {
        usb_worker = CreateThread(NULL, 0, transmit_worker, NULL, 0, NULL);
        worker = CreateThread(NULL, 0, sample_worker, window, 0, NULL);
        if (worker && usb_worker) {
            if (!hidden || !in_tray)
                ShowWindow(window, show == SW_HIDE ? SW_SHOW : show);
            MSG message;
            while (GetMessageW(&message, NULL, 0, 0) > 0) {
                if (!IsDialogMessageW(window, &message)) {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
        } else {
            DestroyWindow(window);
        }
        InterlockedExchange(&stopping, 1);
        SetEvent(wake);
        SetEvent(usb_cancel);
        SetEvent(usb_ready);
        if (worker) {
            WaitForSingleObject(worker, INFINITE);
            CloseHandle(worker);
        }
        if (usb_worker) {
            WaitForSingleObject(usb_worker, INFINITE);
            CloseHandle(usb_worker);
        }
    }
    if (wake)
        CloseHandle(wake);
    if (usb_ready)
        CloseHandle(usb_ready);
    if (usb_cancel)
        CloseHandle(usb_cancel);
    if (session_marker)
        CloseHandle(session_marker);
    CloseHandle(singleton);
    return window && worker && usb_worker ? 0 : 1;
}
