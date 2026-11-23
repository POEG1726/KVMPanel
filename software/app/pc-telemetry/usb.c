/*
 * usb.c
 * Created on Nov 23, 2026
 * Author: @POEG1726
 */

#include "telemetry.h"
#include <hidsdi.h>
#include <math.h>
#include <setupapi.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

enum { REPORT_SIZE = HOST_REPORT_SIZE, REPORT_ID = 3, REPORT_GAP_MS = 22 };
struct Usb {
    HANDLE handle, input;
    OVERLAPPED read;
    BOOL read_armed;
    unsigned char packet[REPORT_SIZE];
    DEVINST parent;
    uint64_t session;
    unsigned interval;
    BOOL clipboard;
    ULONGLONG next_hello, next_caps;
};

/** Match the vendor collection only, including on composite gadgets with a mouse. */
static BOOL is_kvmpanel(HANDLE handle, BOOL *agent)
{
    HIDD_ATTRIBUTES attributes = {.Size = sizeof(attributes)};
    PHIDP_PREPARSED_DATA data;
    HIDP_CAPS caps;
    wchar_t product[64];
    if (!HidD_GetAttributes(handle, &attributes) ||
        attributes.VendorID != 0x1d6b || attributes.ProductID != 0x0104 ||
        !HidD_GetProductString(handle, product, sizeof(product)) ||
        wcscmp(product, L"KVMPanel") != 0 || !HidD_GetPreparsedData(handle, &data))
        return FALSE;
    const NTSTATUS status = HidP_GetCaps(data, &caps);
    HidD_FreePreparsedData(data);
    if (status != HIDP_STATUS_SUCCESS || caps.UsagePage != 0xff00 ||
        caps.Usage != 1 || caps.OutputReportByteLength != REPORT_SIZE)
        return FALSE;
    *agent = caps.InputReportByteLength == REPORT_SIZE;
    return TRUE;
}

/** Keep one overlapped input read armed, including while sending telemetry through EP0. */
static BOOL read_start(Usb *usb)
{
    const HANDLE complete = usb->read.hEvent;
    ResetEvent(complete);
    usb->read = (OVERLAPPED){.hEvent = complete};
    usb->read_armed = ReadFile(usb->input, usb->packet, sizeof(usb->packet), NULL, &usb->read) ||
                      GetLastError() == ERROR_IO_PENDING;
    return usb->read_armed;
}

Usb *usb_open(void)
{
    GUID guid;
    HidD_GetHidGuid(&guid);
    HDEVINFO devices = SetupDiGetClassDevsW(&guid, NULL, NULL,
                                            DIGCF_PRESENT | DIGCF_DEVICEINTERFACE);
    if (devices == INVALID_HANDLE_VALUE)
        return NULL;
    Usb *usb = NULL;
    for (DWORD i = 0; ; ++i) {
        SP_DEVICE_INTERFACE_DATA info = {.cbSize = sizeof(info)};
        DWORD bytes = 0;
        if (!SetupDiEnumDeviceInterfaces(devices, NULL, &guid, i, &info))
            break;
        SetupDiGetDeviceInterfaceDetailW(devices, &info, NULL, 0, &bytes, NULL);
        if (bytes < sizeof(SP_DEVICE_INTERFACE_DETAIL_DATA_W))
            continue;
        SP_DEVICE_INTERFACE_DETAIL_DATA_W *detail = malloc(bytes);
        if (!detail)
            break;
        detail->cbSize = sizeof(*detail);
        SP_DEVINFO_DATA device = {.cbSize = sizeof(device)};
        HANDLE handle = INVALID_HANDLE_VALUE;
        if (SetupDiGetDeviceInterfaceDetailW(devices, &info, detail, bytes, NULL, &device))
            handle = CreateFileW(detail->DevicePath, GENERIC_WRITE, FILE_SHARE_READ,
                                 NULL, OPEN_EXISTING, 0, NULL);
        BOOL agent = FALSE;
        if (handle != INVALID_HANDLE_VALUE && is_kvmpanel(handle, &agent)) {
            usb = calloc(1, sizeof(*usb));
            if (usb) {
                usb->handle = handle;
                usb->input = INVALID_HANDLE_VALUE;
                if (agent) {
                    usb->parent = host_usb_parent(device.DevInst);
                    usb->input = CreateFileW(detail->DevicePath, GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE,
                                             NULL, OPEN_EXISTING, FILE_FLAG_OVERLAPPED, NULL);
                    if (usb->input != INVALID_HANDLE_VALUE) {
                        usb->read.hEvent = CreateEventW(NULL, TRUE, FALSE, NULL);
                        if (!usb->read.hEvent || !read_start(usb)) {
                            CloseHandle(usb->input);
                            usb->input = INVALID_HANDLE_VALUE;
                            if (usb->read.hEvent)
                                CloseHandle(usb->read.hEvent);
                            usb->read.hEvent = NULL;
                        }
                    }
                }
            }
        }
        free(detail);
        if (usb)
            break;
        if (handle != INVALID_HANDLE_VALUE)
            CloseHandle(handle);
    }
    SetupDiDestroyDeviceInfoList(devices);
    return usb;
}

static BOOL report(Usb *usb, const char *payload, size_t length)
{
    unsigned char packet[REPORT_SIZE] = {REPORT_ID};
    if (length > sizeof(packet) - 1)
        return FALSE;
    memcpy(packet + 1, payload, length);
    return HidD_SetOutputReport(usb->handle, packet, sizeof(packet));
}

static BOOL usb_hello(Usb *usb)
{
    char message[REPORT_SIZE];
    const int count = snprintf(message, sizeof(message), "!%016llx:%u",
                              (unsigned long long)usb->session, usb->interval);
    return count > 0 && report(usb, message, (size_t)count);
}

HANDLE usb_input_event(const Usb *usb)
{
    return usb ? usb->read.hEvent : NULL;
}

void usb_configure(Usb *usb, uint64_t session, unsigned interval, BOOL clipboard)
{
    if (usb->session != session) {
        usb->session = session;
        usb->next_hello = usb->next_caps = 0;
    }
    usb->interval = interval;
    usb->clipboard = clipboard;
}

ULONGLONG usb_announcement_deadline(const Usb *usb)
{
    return usb->read.hEvent && usb->next_caps < usb->next_hello ? usb->next_caps : usb->next_hello;
}

/** Wake at report boundaries so a queued request takes priority over another telemetry report. */
static UsbResult usb_gap(const Usb *usb, HANDLE interrupt)
{
    const HANDLE handles[] = {interrupt, usb->read.hEvent};
    const DWORD event = WaitForMultipleObjects(usb->read.hEvent ? 2 : 1, handles,
                                               FALSE, REPORT_GAP_MS);
    return event == WAIT_TIMEOUT ? USB_OK : event == WAIT_FAILED ? USB_LOST : USB_INTERRUPTED;
}

/** Pace housekeeping through the same single writer, even during a large telemetry batch. */
UsbResult usb_announce(Usb *usb, HANDLE interrupt)
{
    const ULONGLONG now = GetTickCount64();
    if (WaitForSingleObject(interrupt, 0) == WAIT_OBJECT_0)
        return USB_INTERRUPTED;
    if (now >= usb->next_hello) {
        if (!usb_hello(usb))
            return USB_LOST;
        usb->next_hello = now + 1000;
        const UsbResult gap = usb_gap(usb, interrupt);
        if (gap != USB_OK)
            return gap;
    }
    if (usb->read.hEvent && now >= usb->next_caps) {
        const char *caps = usb->clipboard ? "^caps=info,clipboard,eject" : "^caps=info,eject";
        if (!report(usb, caps, strlen(caps)))
            return USB_LOST;
        usb->next_caps = now + HOST_CAPABILITIES_MS;
        return usb_gap(usb, interrupt);
    }
    return USB_OK;
}

static UsbResult after_telemetry(Usb *usb, HANDLE interrupt)
{
    const UsbResult gap = usb_gap(usb, interrupt);
    return gap == USB_OK ? usb_announce(usb, interrupt) : gap;
}

static UsbResult read_input(Usb *usb, unsigned char packet[REPORT_SIZE], HANDLE interrupt, DWORD delay)
{
    if (!usb->read.hEvent)
        return USB_INTERRUPTED;
    const HANDLE events[] = {interrupt, usb->read.hEvent};
    const DWORD event = WaitForMultipleObjects(_countof(events), events, FALSE, delay);
    if (event == WAIT_TIMEOUT || event == WAIT_OBJECT_0)
        return USB_INTERRUPTED;
    if (event != WAIT_OBJECT_0 + 1)
        return USB_LOST;
    DWORD bytes = 0;
    const BOOL complete = GetOverlappedResult(usb->input, &usb->read, &bytes, FALSE);
    const DWORD failure = complete ? ERROR_SUCCESS : GetLastError();
    if (failure != ERROR_IO_INCOMPLETE)
        usb->read_armed = FALSE;
    if (!complete)
        return USB_LOST;
    memset(packet, 0, REPORT_SIZE);
    if (bytes == REPORT_SIZE)
        memcpy(packet, usb->packet, REPORT_SIZE);
    return read_start(usb) ? USB_OK : USB_LOST;
}

static UsbResult wait_ack(Usb *usb, unsigned seq, unsigned part, HANDLE interrupt, ULONGLONG limit)
{
    const ULONGLONG idle = GetTickCount64() + HOST_IDLE_MS;
    if (idle < limit)
        limit = idle;
    while (GetTickCount64() < limit) {
        unsigned char packet[REPORT_SIZE];
        const ULONGLONG now = GetTickCount64();
        const UsbResult result = read_input(usb, packet, interrupt, now < limit ? (DWORD)(limit - now) : 0);
        if (result != USB_OK || host_ack(packet, seq, part))
            return result;
    }
    return USB_INTERRUPTED;
}

UsbResult usb_service_agent(Usb *usb, const HostSettings *settings, HANDLE interrupt)
{
    unsigned char packet[REPORT_SIZE];
    const UsbResult ready = read_input(usb, packet, interrupt, 0);
    if (ready == USB_LOST)
        return ready;
    unsigned seq;
    char method[REPORT_SIZE];
    if (ready != USB_OK || !host_request(packet, &seq, method))
        return USB_OK;
    const ULONGLONG started = GetTickCount64();
    char reply[HOST_REPLY_LIMIT + 1];
    const size_t length = host_reply(method, settings, usb->parent, started + HOST_IDLE_MS - 50, reply);
    if (GetTickCount64() >= started + HOST_IDLE_MS)
        return USB_INTERRUPTED;
    const unsigned parts = host_parts(length);
    const BOOL private = strcmp(method, "clipboard") == 0 && strncmp(reply, "{\"ok\":true", 10) == 0;
    for (unsigned part = 1; part <= parts; ++part) {
        if (WaitForSingleObject(interrupt, 0) != WAIT_TIMEOUT ||
            GetTickCount64() >= started + HOST_TRANSFER_MS || (private && !host_clipboard_allowed()))
            return USB_INTERRUPTED;
        char payload[REPORT_SIZE];
        const size_t bytes = host_fragment(payload, seq, part, reply, length);
        if (!report(usb, payload, bytes))
            return USB_LOST;
        const UsbResult ack = wait_ack(usb, seq, part, interrupt, started + HOST_TRANSFER_MS);
        if (ack != USB_OK)
            return ack;
    }
    return USB_OK;
}

/** Send a complete UTF-8 prefix within the HID report's payload. */
static size_t label_prefix(const char *label, size_t capacity)
{
    size_t count = strlen(label);
    if (count <= capacity)
        return count;
    count = capacity;
    while (count && ((unsigned char)label[count] & 0xc0) == 0x80)
        --count;
    return count;
}

static UsbResult send_device_labels(Usb *usb, const Sample *sample, HANDLE interrupt)
{
    for (unsigned i = 0; i < sample->count; ++i) {
        const Metric *metric = &sample->metrics[i];
        const char *end = strrchr(metric->id, '.');
        if (!metric->label[0] || !end)
            continue;
        const size_t key_size = (size_t)(end - metric->id);
        BOOL known = FALSE;
        for (unsigned j = 0; j < i; ++j) {
            if (sample->metrics[j].label[0] &&
                strncmp(sample->metrics[j].id, metric->id, key_size) == 0 &&
                sample->metrics[j].id[key_size] == '.') {
                known = TRUE;
                break;
            }
        }
        if (known || key_size > REPORT_SIZE - 4)
            continue;
        char message[REPORT_SIZE];
        const int start = snprintf(message, sizeof(message), ":%.*s=", (int)key_size, metric->id);
        const size_t size = label_prefix(metric->label, REPORT_SIZE - 1 - (size_t)start);
        for (size_t j = 0; j < size; ++j) {
            const unsigned char c = (unsigned char)metric->label[j];
            message[start + j] = c < 32 || c == 127 ? ' ' : (char)c;
        }
        if (!report(usb, message, (size_t)start + size))
            return USB_LOST;
        const UsbResult gap = after_telemetry(usb, interrupt);
        if (gap != USB_OK)
            return gap;
    }
    return USB_OK;
}

UsbResult usb_send(Usb *usb, const Sample *sample, BOOL send_labels, HANDLE interrupt)
{
    char message[REPORT_SIZE];
    size_t used = 0;
    /* The gadget retains one control report, so leave time for its reader between reports. */
    UsbResult gap = usb_gap(usb, interrupt);
    if (gap != USB_OK)
        return gap;
    for (unsigned i = 0; i < sample->count; ++i) {
        const Metric *metric = &sample->metrics[i];
        char line[REPORT_SIZE];
        const int length = isfinite(metric->value)
            ? snprintf(line, sizeof(line), "%s=%.10g", metric->id, metric->value)
            : snprintf(line, sizeof(line), "%s=null", metric->id);
        if (length <= 0 || length >= REPORT_SIZE)
            continue;
        const size_t count = (size_t)length;
        if (used && used + 1 + count >= REPORT_SIZE) {
            if (!report(usb, message, used))
                return USB_LOST;
            gap = after_telemetry(usb, interrupt);
            if (gap != USB_OK)
                return gap;
            used = 0;
        }
        if (used)
            message[used++] = '\n';
        memcpy(message + used, line, count);
        used += count;
    }
    if (used) {
        if (!report(usb, message, used))
            return USB_LOST;
        gap = after_telemetry(usb, interrupt);
        if (gap != USB_OK)
            return gap;
    }
    return send_labels ? send_device_labels(usb, sample, interrupt) : USB_OK;
}

void usb_close(Usb *usb)
{
    if (usb) {
        if (usb->input != INVALID_HANDLE_VALUE) {
            if (usb->read_armed) {
                DWORD copied;
                CancelIoEx(usb->input, &usb->read);
                GetOverlappedResult(usb->input, &usb->read, &copied, TRUE);
            }
            CloseHandle(usb->input);
            CloseHandle(usb->read.hEvent);
        }
        CloseHandle(usb->handle);
        free(usb);
    }
}
