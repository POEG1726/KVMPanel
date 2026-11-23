/*
 * host_agent.h
 * Created on Nov 23, 2026
 * Author: @POEG1726
 */

#ifndef _HOST_AGENT_H_
#define _HOST_AGENT_H_

#include <windows.h>
#include <cfgmgr32.h>

enum {
    HOST_REPORT_SIZE = 64,
    HOST_REPLY_LIMIT = 16384,
    HOST_IDLE_MS = 2000,
    HOST_TRANSFER_MS = 10000,
    HOST_CAPABILITIES_MS = 5000,
    HOST_MAX_DISPLAYS = 32,
    HOST_CLIPBOARD_DEFAULT = FALSE,
};

typedef struct {
    wchar_t id[MAX_DEVICE_ID_LEN];
    wchar_t name[128];
    RECT area;
    int scale_percent;
    BOOL primary;
} HostDisplay;

typedef struct {
    HostDisplay items[HOST_MAX_DISPLAYS];
    unsigned count;
    BOOL truncated;
} HostDisplays;

typedef struct {
    BOOL clipboard;
    wchar_t monitor[MAX_DEVICE_ID_LEN];
} HostSettings;

typedef enum {
    HOST_LOCKED, HOST_UNLOCKED, HOST_SECURE_DESKTOP, HOST_SCREENSAVER,
} HostSession;

/** Enumerate active monitors with physical coordinates and persistent identifiers. */
void host_displays(HostDisplays *displays);

/** Determine whether this process belongs to the local console, including when locked. */
BOOL host_local_session(void);
BOOL host_clipboard_allowed(void);

/** Return the USB composite parent of a collection or zero when it cannot be identified. */
DEVINST host_usb_parent(DEVINST device);

/** Flush, dismount, and eject only the uniquely matched LUN 2. */
const char *host_eject(DEVINST parent, ULONGLONG deadline);

/** Format and validate the host-agent wire messages. */
BOOL host_request(const unsigned char packet[HOST_REPORT_SIZE], unsigned *seq, char method[64]);
BOOL host_ack(const unsigned char packet[HOST_REPORT_SIZE], unsigned seq, unsigned part);
unsigned host_parts(size_t length);
size_t host_fragment(char payload[HOST_REPORT_SIZE], unsigned seq, unsigned part,
                     const char *reply, size_t length);

/** Dispatch one request and return a complete UTF-8 reply. */
size_t host_reply(const char *method, const HostSettings *settings, DEVINST parent,
                  ULONGLONG deadline, char reply[HOST_REPLY_LIMIT + 1]);

/** Exercise the request, reply and ACK path using simulated Win32 providers. */
int host_agent_test(void);

#endif // _HOST_AGENT_H_
