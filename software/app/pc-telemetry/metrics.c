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

void metric_put(Sample *sample, const char *id, const char *label, double value)
{
    if (!isfinite(value) || strlen(id) >= ID_SIZE)
        return;
    for (unsigned i = 0; i < sample->count; ++i)
        if (strcmp(sample->metrics[i].id, id) == 0)
            return;
    if (sample->count == MAX_METRICS)
        return;
    Metric *metric = &sample->metrics[sample->count++];
    strcpy_s(metric->id, sizeof(metric->id), id);
    strcpy_s(metric->label, sizeof(metric->label), label ? label : "");
    metric->value = value;
    if (strstr(id, ".mem_junction") != NULL && strcmp(id, "gpu.mem_junction") != 0)
        ++sample->junctions;
    if (strstr(id, ".hotspot") != NULL)
        ++sample->hotspots;
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
