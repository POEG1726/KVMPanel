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
    BOOL ryzen_access_denied;
    Metric metrics[MAX_METRICS];
} Sample;

typedef struct Sources Sources;
typedef struct Usb Usb;
typedef struct Radeon Radeon;
typedef struct Pawn Pawn;
typedef struct Ryzen Ryzen;
typedef struct Ddr5 Ddr5;

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

/** Open an official signed PawnIO module from the collector directory. */
Pawn *pawn_open(const wchar_t *name, BOOL *access_denied);
BOOL pawn_read(Pawn *pawn, const char *operation, const ULONG64 *in, SIZE_T in_count,
               ULONG64 *out, SIZE_T out_count);
void pawn_close(Pawn *pawn);
double hotspot_read(Pawn *pawn, unsigned bus, unsigned device, unsigned function);

/** Read hardware telemetry from a recognized Ryzen PM table. */
Ryzen *ryzen_open(BOOL *access_denied);
void ryzen_sample(Ryzen *ryzen, Sample *sample, unsigned groups);
void ryzen_close(Ryzen *ryzen);

/** Read DDR5 SPD hub temperatures through a supported SMBus controller. */
Ddr5 *ddr5_open(void);
void ddr5_sample(Ddr5 *ddr5, Sample *sample);
void ddr5_close(Ddr5 *ddr5);

/** Discover and update the KVMPanel HID collection. */
Usb *usb_open(void);
BOOL usb_send(Usb *usb, const Sample *sample, BOOL send_labels, HANDLE interrupt);
BOOL usb_hello(Usb *usb, uint64_t session, unsigned interval);
void usb_close(Usb *usb);

#endif
