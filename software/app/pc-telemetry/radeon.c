/*
 * radeon.c
 * Created on Nov 23, 2026
 * Author: @POEG1726
 */

#include "telemetry.h"
#include <stdlib.h>
#include <string.h>

enum { ADL_NAME_SIZE = 256, ADL_PMLOG_COUNT = 256 };
typedef struct {
    int size, index;
    char udid[ADL_NAME_SIZE];
    int bus, device, function, vendor;
    char name[ADL_NAME_SIZE], display[ADL_NAME_SIZE];
    int present, exists;
    char driver[ADL_NAME_SIZE], driver_ext[ADL_NAME_SIZE], pnp[ADL_NAME_SIZE];
    int display_index;
} Adapter;

typedef struct {
    int size;
    struct { int supported, value; } sensors[ADL_PMLOG_COUNT];
} PmLog;

struct Radeon {
    HMODULE module;
    void *context;
    int (*destroy)(void *);
    int (*query)(void *, int, PmLog *);
    int (*count)(void *, int *);
    int (*adapters)(void *, Adapter *, int);
    Adapter *list;
    int entries;
    ULONGLONG next_discovery;
};

static void *WINAPI allocate_adl(int size)
{
    return size > 0 ? malloc((size_t)size) : NULL;
}

Radeon *radeon_open(void)
{
    Radeon *radeon = calloc(1, sizeof(*radeon));
    if (!radeon)
        return NULL;
    radeon->module = LoadLibraryExW(L"atiadlxx.dll", NULL, LOAD_LIBRARY_SEARCH_SYSTEM32);
    if (radeon->module) {
#pragma warning(push)
#pragma warning(disable: 4191)
        int (*create)(void *(WINAPI *)(int), int, void **) =
            (int (*)(void *(WINAPI *)(int), int, void **))
            GetProcAddress(radeon->module, "ADL2_Main_Control_Create");
        radeon->destroy = (int (*)(void *))GetProcAddress(radeon->module, "ADL2_Main_Control_Destroy");
        radeon->query = (int (*)(void *, int, PmLog *))
            GetProcAddress(radeon->module, "ADL2_New_QueryPMLogData_Get");
        radeon->count = (int (*)(void *, int *))
            GetProcAddress(radeon->module, "ADL2_Adapter_NumberOfAdapters_Get");
        radeon->adapters = (int (*)(void *, Adapter *, int))
            GetProcAddress(radeon->module, "ADL2_Adapter_AdapterInfo_Get");
#pragma warning(pop)
        if (create && radeon->destroy && radeon->query && radeon->count && radeon->adapters &&
            create(allocate_adl, 1, &radeon->context) == 0 && radeon->context)
            return radeon;
    }
    radeon_close(radeon);
    return NULL;
}

/** Refresh active adapter topology away from the regular sampling path. */
static void discover(Radeon *radeon)
{
    int count = 0;
    if (radeon->count(radeon->context, &count) != 0 || count < 1 || count > 256)
        return;
    Adapter *list = calloc((size_t)count, sizeof(*list));
    if (!list)
        return;
    for (int i = 0; i < count; ++i)
        list[i].size = sizeof(*list);
    if (radeon->adapters(radeon->context, list, count * (int)sizeof(*list)) != 0) {
        free(list);
        return;
    }
    free(radeon->list);
    radeon->list = list;
    radeon->entries = count;
}

void radeon_sample(Radeon *radeon, Sample *sample, unsigned groups)
{
    static const struct { unsigned index, group; const char *field; double scale; } fields[] = {
        {1, GPU, "clock", 1}, {2, TUNING, "mem_clock", 1},
        {8, GPU, "temp", 1}, {9, JUNCTION, "mem_temp", 1},
        {19, GPU, "load", 1}, {21, TUNING, "core_v", 0.001},
        {22, TUNING, "memory_v", 0.001}, {27, HOTSPOT, "hotspot", 1},
        {73, GPU, "power", 1},
    };
    if (!radeon)
        return;
    const ULONGLONG now = GetTickCount64();
    if (now >= radeon->next_discovery) {
        discover(radeon);
        radeon->next_discovery = now + 10000;
    }
    for (int i = 0; i < radeon->entries; ++i) {
        const Adapter *adapter = &radeon->list[i];
        if (!adapter->exists || adapter->bus < 0 ||
            (adapter->vendor != 1002 && adapter->vendor != 0x1002))
            continue;
        unsigned seen = 0;
        for (int j = 0; j < i; ++j) {
            const Adapter *previous = &radeon->list[j];
            if (previous->exists && previous->bus == adapter->bus &&
                previous->device == adapter->device && previous->function == adapter->function)
                ++seen;
        }
        if (seen)
            continue;
        PmLog log = {.size = sizeof(log)};
        if (radeon->query(radeon->context, adapter->index, &log) != 0)
            continue;
        const int pci[] = {0x1002, adapter->bus, adapter->device, adapter->function};
        const uint64_t key = identity(pci, sizeof(pci));
        wchar_t label[80] = L"AMD Radeon";
        MultiByteToWideChar(CP_ACP, 0, adapter->name, -1, label, _countof(label));
        for (unsigned n = 0; n < _countof(fields); ++n) {
            const unsigned id = fields[n].index;
            if (!(groups & fields[n].group) || !log.sensors[id].supported)
                continue;
            const int value = log.sensors[id].value;
            if ((id == 8 || id == 9 || id == 27) && (value <= 0 || value >= 150))
                continue;
            device_metric(sample, "gpu", key, fields[n].field, label, value * fields[n].scale);
        }
    }
}

void radeon_close(Radeon *radeon)
{
    if (!radeon)
        return;
    if (radeon->context && radeon->destroy)
        radeon->destroy(radeon->context);
    if (radeon->module)
        FreeLibrary(radeon->module);
    free(radeon->list);
    free(radeon);
}
