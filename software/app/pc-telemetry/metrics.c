/*
 * metrics.c
 * Created on Nov 23, 2026
 * Author: @POEG1726
 */

#include "telemetry.h"
#include <math.h>
#include <stdio.h>
#include <string.h>

/** Hash an OS-provided identity for a compact, stable USB key. */
uint64_t identity(const void *data, size_t size)
{
    const unsigned char *bytes = data;
    uint64_t hash = UINT64_C(14695981039346656037);
    for (size_t i = 0; i < size; ++i) {
        hash ^= bytes[i];
        hash *= UINT64_C(1099511628211);
    }
    return hash;
}

/** Clear prior readings before polling, keeping IDs and labels for missing rows. */
void sample_begin(Sample *sample, BOOL reset)
{
    if (reset)
        sample->count = 0;
    ++sample->sequence;
    sample->device_scan = 0;
    sample->available = sample->junctions = sample->hotspots = 0;
    for (unsigned i = 0; i < sample->count; ++i)
        sample->metrics[i].value = NAN;
}

/** Retire dynamic IDs only after a successful scan no longer finds their device. */
void sample_end(Sample *sample)
{
    unsigned kept = 0;
    for (unsigned i = 0; i < sample->count; ++i) {
        const Metric *metric = &sample->metrics[i];
        const BOOL absent = ((sample->device_scan & GPU) &&
                              strncmp(metric->id, "gpu.", 4) == 0 &&
                              strchr(metric->id + 4, '.') == metric->id + 20) ||
                            ((sample->device_scan & NETWORK) &&
                             strncmp(metric->id, "net.", 4) == 0) ||
                            ((sample->device_scan & STORAGE) &&
                             strncmp(metric->id, "disk.", 5) == 0);
        if (absent && sample->sequence - metric->last_seen >= 2)
            continue;
        if (kept != i)
            sample->metrics[kept] = *metric;
        ++kept;
    }
    sample->count = kept;
}

void metric_put(Sample *sample, const char *id, const char *label, double value)
{
    if (strlen(id) >= ID_SIZE)
        return;
    unsigned slot = 0;
    while (slot < sample->count && strcmp(sample->metrics[slot].id, id) != 0)
        ++slot;
    if (slot == MAX_METRICS)
        return;
    Metric *metric = &sample->metrics[slot];
    if (slot == sample->count) {
        ++sample->count;
        strcpy_s(metric->id, sizeof(metric->id), id);
        metric->label[0] = '\0';
        metric->value = NAN;
    }
    metric->last_seen = sample->sequence;
    if (label && *label)
        strcpy_s(metric->label, sizeof(metric->label), label);
    if (!isfinite(value) || isfinite(metric->value))
        return;
    metric->value = value;
    ++sample->available;
    if (strstr(id, ".mem_junction") != NULL && strcmp(id, "gpu.mem_junction") != 0)
        ++sample->junctions;
    if (strstr(id, ".hotspot") != NULL)
        ++sample->hotspots;
}

/** Refresh the inventory even when an enumerated device yields no sensor readings. */
void metric_device_seen(Sample *sample, const char *kind, uint64_t device)
{
    char prefix[ID_SIZE];
    const int length = snprintf(prefix, sizeof(prefix), "%s.%016llx.",
                                kind, (unsigned long long)device);
    if (length <= 0 || length >= ID_SIZE)
        return;
    for (unsigned i = 0; i < sample->count; ++i) {
        Metric *metric = &sample->metrics[i];
        if (strncmp(metric->id, prefix, (size_t)length) == 0)
            metric->last_seen = sample->sequence;
    }
}

void device_metric(Sample *sample, const char *kind, uint64_t device,
                   const char *field, const wchar_t *label, double value)
{
    char id[ID_SIZE];
    char utf8[LABEL_SIZE] = "";
    wchar_t short_label[34];

    snprintf(id, sizeof(id), "%s.%016llx.%s", kind, (unsigned long long)device, field);
    if (label && *label) {
        wcsncpy_s(short_label, _countof(short_label), label, _TRUNCATE);
        const size_t length = wcslen(short_label);
        if (length && short_label[length - 1] >= 0xd800 && short_label[length - 1] <= 0xdbff)
            short_label[length - 1] = L'\0';
        WideCharToMultiByte(CP_UTF8, 0, short_label, -1, utf8, sizeof(utf8), NULL, NULL);
    }
    metric_put(sample, id, utf8, value);
}
