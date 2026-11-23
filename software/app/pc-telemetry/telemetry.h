/*
 * telemetry.h
 * Created on Nov 23, 2026
 * Author: @POEG1726
 */

#ifndef _TELEMETRY_H_
#define _TELEMETRY_H_

#include <windows.h>
#include <stdint.h>

enum {
    CPU = 1, MEMORY = 2, GPU = 4, JUNCTION = 8, NETWORK = 16,
    STORAGE = 32, TUNING = 64, HOTSPOT = 128, ALL_GROUPS = 255,
    MIN_INTERVAL = 250, MAX_INTERVAL = 60000, DEFAULT_INTERVAL = 1000,
    MAX_METRICS = 768, ID_SIZE = 48, LABEL_SIZE = 104,
};

typedef struct {
    char id[ID_SIZE];
    char label[LABEL_SIZE];
    double value;
} Metric;

typedef struct {
    unsigned count;
    unsigned junctions;
    unsigned hotspots;
    BOOL hotspot_access_denied;
    Metric metrics[MAX_METRICS];
} Sample;

typedef struct Sources Sources;
typedef struct Usb Usb;
typedef struct Radeon Radeon;
typedef struct Hotspot Hotspot;

/** Store a finite reading under a stable device and metric ID. */
void metric_put(Sample *sample, const char *id, const char *label, double value);
void device_metric(Sample *sample, const char *kind, uint64_t device,
                   const char *field, const wchar_t *label, double value);
uint64_t identity(const void *data, size_t size);

/** Create and sample reusable Windows sensor providers. */
Sources *sources_open(void);
void sources_sample(Sources *sources, Sample *sample, unsigned groups);
void sources_close(Sources *sources);

/** Read supported Radeon sensors via the installed AMD display driver. */
Radeon *radeon_open(void);
void radeon_sample(Radeon *radeon, Sample *sample, unsigned groups);
void radeon_close(Radeon *radeon);

/** Read Blackwell's hardware hotspot via an existing signed PawnIO executor. */
Hotspot *hotspot_open(BOOL *access_denied);
double hotspot_read(Hotspot *hotspot, unsigned bus, unsigned device, unsigned function);
void hotspot_close(Hotspot *hotspot);

/** Discover and update the KVMPanel HID collection. */
Usb *usb_open(void);
BOOL usb_send(Usb *usb, const Sample *sample, BOOL send_labels, HANDLE interrupt);
BOOL usb_hello(Usb *usb, uint64_t session, unsigned interval);
void usb_close(Usb *usb);

#endif
