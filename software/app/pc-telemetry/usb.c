/*
 * usb.c
 * Created on Nov 23, 2026
 * Author: @POEG1726
 */

#include "telemetry.h"
#include <hidsdi.h>
#include <setupapi.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

enum { REPORT_SIZE = 64, REPORT_ID = 3, REPORT_GAP_MS = 22 };
struct Usb { HANDLE handle; };

/** Match the vendor collection only, including on composite gadgets with a mouse. */
static BOOL is_kvmpanel(HANDLE handle)
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
    return status == HIDP_STATUS_SUCCESS && caps.UsagePage == 0xff00 &&
           caps.Usage == 1 && caps.OutputReportByteLength == REPORT_SIZE;
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
        HANDLE handle = INVALID_HANDLE_VALUE;
        if (SetupDiGetDeviceInterfaceDetailW(devices, &info, detail, bytes, NULL, NULL))
            handle = CreateFileW(detail->DevicePath, GENERIC_WRITE, FILE_SHARE_READ | FILE_SHARE_WRITE,
                                 NULL, OPEN_EXISTING, 0, NULL);
        free(detail);
        if (handle == INVALID_HANDLE_VALUE)
            continue;
        if (is_kvmpanel(handle)) {
            usb = malloc(sizeof(*usb));
            if (usb) {
                usb->handle = handle;
                break;
            }
        }
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

BOOL usb_hello(Usb *usb, uint64_t session, unsigned interval)
{
    char message[REPORT_SIZE];
    const int count = snprintf(message, sizeof(message), "!%016llx:%u",
                              (unsigned long long)session, interval);
    return count > 0 && report(usb, message, (size_t)count);
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

static BOOL send_device_labels(Usb *usb, const Sample *sample, HANDLE interrupt)
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
        if (!report(usb, message, (size_t)start + size) ||
            WaitForSingleObject(interrupt, REPORT_GAP_MS) != WAIT_TIMEOUT)
            return FALSE;
    }
    return TRUE;
}

BOOL usb_send(Usb *usb, const Sample *sample, BOOL send_labels, HANDLE interrupt)
{
    char message[REPORT_SIZE];
    size_t used = 0;
    /* The gadget retains one control report, so leave time for its reader between reports. */
    if (WaitForSingleObject(interrupt, REPORT_GAP_MS) != WAIT_TIMEOUT)
        return FALSE;
    for (unsigned i = 0; i < sample->count; ++i) {
        const Metric *metric = &sample->metrics[i];
        char line[REPORT_SIZE];
        const int length = snprintf(line, sizeof(line), "%s=%.10g", metric->id, metric->value);
        if (length <= 0 || length >= REPORT_SIZE)
            continue;
        const size_t count = (size_t)length;
        if (used && used + 1 + count >= REPORT_SIZE) {
            if (!report(usb, message, used) ||
                WaitForSingleObject(interrupt, REPORT_GAP_MS) != WAIT_TIMEOUT)
                return FALSE;
            used = 0;
        }
        if (used)
            message[used++] = '\n';
        memcpy(message + used, line, count);
        used += count;
    }
    if (used && (!report(usb, message, used) ||
                 WaitForSingleObject(interrupt, REPORT_GAP_MS) != WAIT_TIMEOUT))
        return FALSE;
    return !send_labels || send_device_labels(usb, sample, interrupt);
}

void usb_close(Usb *usb)
{
    if (usb) {
        CloseHandle(usb->handle);
        free(usb);
    }
}
