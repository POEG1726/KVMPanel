/*
 * sources.c
 * Created on Nov 23, 2026
 * Author: @POEG1726
 */

#include <winsock2.h>
#include <ws2ipdef.h>
#include "telemetry.h"
#include <iphlpapi.h>
#include <netioapi.h>
#include <pdh.h>
#include <pdhmsg.h>
#include <wlanapi.h>
#include <winioctl.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
    GUID guid;
    uint64_t rx, tx;
} NetCounter;

typedef void *NvDevice;
typedef struct { unsigned gpu, memory; } NvUtilization;
typedef struct { unsigned long long total, free, used; } NvMemory;
typedef struct {
    char legacy_bus[16];
    unsigned domain, bus, device, pci_id, subsystem_id;
    char bus_id[32];
} NvPci;
typedef struct {
    unsigned field, scope;
    long long timestamp, latency;
    int type, status;
    union {
        double real;
        unsigned int ui;
        unsigned long ul;
        unsigned long long ull;
        long long sll;
        int si;
    } value;
} NvField;

_Static_assert(sizeof(NvField) == 40, "NVML nvmlFieldValue_t ABI");
enum { NVML_MEMORY_TEMP = 82, NVML_GRAPHICS_CLOCK = 0, NVML_MEMORY_CLOCK = 2 };

typedef struct {
    HMODULE module;
    int (*shutdown)(void);
    int (*count)(unsigned *);
    int (*handle)(unsigned, NvDevice *);
    int (*uuid)(NvDevice, char *, unsigned);
    int (*name)(NvDevice, char *, unsigned);
    int (*temperature)(NvDevice, int, unsigned *);
    int (*utilization)(NvDevice, NvUtilization *);
    int (*memory)(NvDevice, NvMemory *);
    int (*power)(NvDevice, unsigned *);
    int (*power_limit)(NvDevice, unsigned *);
    int (*clock)(NvDevice, int, unsigned *);
    int (*fields)(NvDevice, int, NvField *);
    int (*pci)(NvDevice, NvPci *);
} Nvml;

enum { MAX_GPU_DEVICES = 64 };
typedef struct {
    uint32_t version, mask;
    int32_t reserved[8], temperature[32];
} NvThermals;
_Static_assert(sizeof(NvThermals) == 168, "NVAPI thermal query ABI");

typedef struct {
    HMODULE module;
    int (*unload)(void);
    int (*enumerate)(NvDevice *, unsigned *);
    int (*name)(NvDevice, char *);
    int (*bus)(NvDevice, unsigned *);
    int (*slot)(NvDevice, unsigned *);
    int (*thermal)(NvDevice, NvThermals *);
} NvApi;

typedef struct {
    unsigned domain, bus, device, function;
    unsigned index;
    uint64_t key;
} GpuIdentity;

struct Sources {
    Nvml nv;
    NvApi nvapi;
    Radeon *radeon;
    Pawn *hotspot;
    Ryzen *ryzen;
    Ddr5 *ddr5;
    BOOL hotspot_attempted;
    BOOL hotspot_access_denied;
    BOOL ryzen_attempted;
    BOOL ryzen_access_denied;
    BOOL ddr5_attempted;
    BOOL nv_attempted;
    PDH_HQUERY query;
    PDH_HCOUNTER cpu, cpu_clock, disk_read, disk_write;
    void *pdh_buffer;
    DWORD pdh_capacity;
    ULONGLONG next_disk_temperature;
    struct { uint64_t id; short temperature; } disk_temperatures[64];
    unsigned disk_temperature_count;
    NetCounter *net;
    size_t net_count;
    ULONGLONG net_ms;
    HANDLE wlan;
    uint64_t idle, kernel, user;
    ULONGLONG next_bclk;
    double bclk_mhz;
    BOOL has_bclk;
    unsigned last_groups;
};

/** Query the drive's composite temperature without requesting write access. */
static BOOL drive_temperature(const wchar_t *instance, short *temperature)
{
    wchar_t path[48], *end;
    const unsigned long disk = wcstoul(instance, &end, 10);
    if (end == instance || (*end && *end != L' '))
        return FALSE;
    swprintf_s(path, _countof(path), L"\\\\.\\PhysicalDrive%lu", disk);
    HANDLE handle = CreateFileW(path, 0, FILE_SHARE_READ | FILE_SHARE_WRITE, NULL, OPEN_EXISTING, 0, NULL);
    if (handle == INVALID_HANDLE_VALUE)
        return FALSE;
    STORAGE_PROPERTY_QUERY query = {
        .PropertyId = StorageDeviceTemperatureProperty, .QueryType = PropertyStandardQuery
    };
    union {
        STORAGE_TEMPERATURE_DATA_DESCRIPTOR descriptor;
        unsigned char bytes[1024];
    } output;
    DWORD size;
    const BOOL ok = DeviceIoControl(handle, IOCTL_STORAGE_QUERY_PROPERTY, &query, sizeof(query),
                                    &output, sizeof(output), &size, NULL);
    CloseHandle(handle);
    if (!ok || size < sizeof(output.descriptor) || !output.descriptor.InfoCount)
        return FALSE;
    *temperature = output.descriptor.TemperatureInfo[0].Temperature;
    return *temperature > -100 && *temperature < 150;
}

/** Resolve the NVIDIA user-mode API from driver installation locations. */
static void open_nvml(Nvml *nv)
{
    wchar_t path[MAX_PATH];
    nv->module = LoadLibraryExW(L"nvml.dll", NULL, LOAD_LIBRARY_SEARCH_SYSTEM32);
    if (!nv->module &&
        ExpandEnvironmentStringsW(L"%ProgramW6432%\\NVIDIA Corporation\\NVSMI\\nvml.dll",
                                  path, _countof(path)) <= _countof(path))
        nv->module = LoadLibraryExW(path, NULL, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR |
                                               LOAD_LIBRARY_SEARCH_SYSTEM32);
    if (!nv->module)
        return;

    /* GetProcAddress exposes a generic function pointer, which the ABI defines by symbol. */
#pragma warning(push)
#pragma warning(disable: 4191)
    int (*init)(void) = (int (*)(void))GetProcAddress(nv->module, "nvmlInit_v2");
#define GET(member, symbol, type) nv->member = (type)GetProcAddress(nv->module, symbol)
    GET(shutdown, "nvmlShutdown", int (*)(void));
    GET(count, "nvmlDeviceGetCount_v2", int (*)(unsigned *));
    GET(handle, "nvmlDeviceGetHandleByIndex_v2", int (*)(unsigned, NvDevice *));
    GET(uuid, "nvmlDeviceGetUUID", int (*)(NvDevice, char *, unsigned));
    GET(name, "nvmlDeviceGetName", int (*)(NvDevice, char *, unsigned));
    GET(temperature, "nvmlDeviceGetTemperature", int (*)(NvDevice, int, unsigned *));
    GET(utilization, "nvmlDeviceGetUtilizationRates", int (*)(NvDevice, NvUtilization *));
    GET(memory, "nvmlDeviceGetMemoryInfo", int (*)(NvDevice, NvMemory *));
    GET(power, "nvmlDeviceGetPowerUsage", int (*)(NvDevice, unsigned *));
    GET(power_limit, "nvmlDeviceGetEnforcedPowerLimit", int (*)(NvDevice, unsigned *));
    GET(clock, "nvmlDeviceGetClockInfo", int (*)(NvDevice, int, unsigned *));
    GET(fields, "nvmlDeviceGetFieldValues", int (*)(NvDevice, int, NvField *));
    GET(pci, "nvmlDeviceGetPciInfo_v3", int (*)(NvDevice, NvPci *));
#undef GET
#pragma warning(pop)
    if (!init || !nv->shutdown || !nv->count || !nv->handle || !nv->uuid || init() != 0) {
        FreeLibrary(nv->module);
        memset(nv, 0, sizeof(*nv));
    }
}

/** Resolve only read-only sensor functions from the installed NVIDIA display driver. */
static void open_nvapi(NvApi *api)
{
    api->module = LoadLibraryExW(L"nvapi64.dll", NULL, LOAD_LIBRARY_SEARCH_SYSTEM32);
    if (!api->module)
        return;
#pragma warning(push)
#pragma warning(disable: 4191)
    void *(*query)(uint32_t) = (void *(*)(uint32_t))GetProcAddress(api->module, "nvapi_QueryInterface");
    if (query) {
        int (*init)(void) = (int (*)(void))query(0x0150e828);
        api->unload = (int (*)(void))query(0xd22bdd7e);
        api->enumerate = (int (*)(NvDevice *, unsigned *))query(0xe5ac921f);
        api->name = (int (*)(NvDevice, char *))query(0xceee8e9f);
        api->bus = (int (*)(NvDevice, unsigned *))query(0x1be0b8e5);
        api->slot = (int (*)(NvDevice, unsigned *))query(0x2a0a350f);
        api->thermal = (int (*)(NvDevice, NvThermals *))query(0x65fe3aad);
        if (init && api->enumerate && api->name && api->bus && api->slot && init() == 0)
            return;
    }
#pragma warning(pop)
    FreeLibrary(api->module);
    memset(api, 0, sizeof(*api));
}

static double nvapi_celsius(int32_t raw)
{
    /* NVAPI reports signed 8.8-style degrees, including 0xff00 for unavailable. */
    const double degrees = raw / 256.0;
    return degrees > 0 && degrees < 150 ? degrees : 0;
}

/** Decode the VRAM channel by architecture. On Blackwell channel 1 is the core, not hotspot. */
static BOOL nvapi_temp(Sources *sources, Sample *sample, unsigned groups,
                       const GpuIdentity *identities, unsigned known)
{
    NvApi *api = &sources->nvapi;
    NvDevice devices[MAX_GPU_DEVICES];
    unsigned count = 0;
    if (!api->module)
        return TRUE;
    if (api->enumerate(devices, &count) != 0 || count > MAX_GPU_DEVICES)
        return FALSE;
    BOOL complete = TRUE;
    for (unsigned i = 0; i < count; ++i) {
        char name[64] = "";
        wchar_t label[80] = L"NVIDIA GPU";
        unsigned pci[2];
        if (api->name(devices[i], name) != 0 ||
            api->bus(devices[i], &pci[0]) != 0 || api->slot(devices[i], &pci[1]) != 0) {
            complete = FALSE;
            continue;
        }
        const BOOL blackwell = strstr(name, "GeForce RTX 50") || strstr(name, "Blackwell");
        unsigned memory_channel;
        if (blackwell)
            memory_channel = 2;
        else if (strstr(name, "GeForce RTX 40"))
            memory_channel = 7;
        else if (strstr(name, "GeForce RTX 30"))
            memory_channel = 9;
        else
            continue;
        const uint32_t mask = UINT32_C(1) << memory_channel | UINT32_C(1) << 1;
        NvThermals block = {.version = (2u << 16) | sizeof(block), .mask = mask};
        const BOOL thermal_ok = api->thermal && api->thermal(devices[i], &block) == 0;
        uint64_t key = identity(pci, sizeof(pci));
        unsigned same_bus = 0, found = 0;
        for (unsigned j = 0; j < known; ++j) {
            if (identities[j].bus == pci[0]) {
                ++same_bus;
                found = j;
            }
        }
        if (same_bus == 1)
            key = identities[found].key;
        metric_device_seen(sample, "gpu", key);
        MultiByteToWideChar(CP_UTF8, 0, name, -1, label, _countof(label));
        if (groups & JUNCTION) {
            const double value = thermal_ok ? nvapi_celsius(block.temperature[memory_channel]) : 0;
            const double junction = value > 0 ? value : NAN;
            device_metric(sample, "gpu", key, "mem_junction", label, junction);
            if (same_bus == 1 ? identities[found].index == 0 : i == 0)
                metric_put(sample, "gpu.mem_junction", "", junction);
        }
        if ((groups & GPU) && thermal_ok && blackwell) {
            const double value = nvapi_celsius(block.temperature[1]);
            if (value)
                device_metric(sample, "gpu", key, "temp", label, value);
        }
        if (groups & HOTSPOT) {
            double value = 0;
            if (blackwell && same_bus == 1 && identities[found].domain == 0) {
                if (!sources->hotspot_attempted) {
                    sources->hotspot_attempted = TRUE;
                    sources->hotspot = pawn_open(L"Nvidia.bin", &sources->hotspot_access_denied);
                }
                sample->hotspot_access_denied = sources->hotspot_access_denied;
                value = hotspot_read(sources->hotspot, identities[found].bus,
                                     identities[found].device, identities[found].function);
            } else if (thermal_ok && !blackwell) {
                value = nvapi_celsius(block.temperature[1]);
            }
            device_metric(sample, "gpu", key, "hotspot", label, value > 0 ? value : NAN);
        }
    }
    return complete;
}

/** Decode NVML's tagged memory-temperature value. */
static double current_nv_temperature(const NvField *field)
{
    switch (field->type) {
    case 0: return field->value.real;
    case 1: return (double)field->value.ui;
    case 2: return (double)field->value.ul;
    case 3: return (double)field->value.ull;
    case 4: return (double)field->value.sll;
    case 5: return (double)field->value.si;
    default: return -1000.0;
    }
}

static void gpu_sample(Sources *sources, Sample *sample, unsigned groups)
{
    Nvml *nv = &sources->nv;
    unsigned count = 0, known = 0;
    GpuIdentity identities[MAX_GPU_DEVICES];
    if (!sources->nv_attempted) {
        sources->nv_attempted = TRUE;
        open_nvml(nv);
        open_nvapi(&sources->nvapi);
        sources->radeon = radeon_open();
    }
    BOOL complete = TRUE;
    if (nv->module && nv->count(&count) != 0) {
        count = 0;
        complete = FALSE;
    }
    for (unsigned i = 0; i < count && i < MAX_GPU_DEVICES; ++i) {
        NvDevice device;
        char uuid[96] = "", name[128] = "";
        wchar_t label[128] = L"NVIDIA GPU";
        unsigned value;
        NvUtilization load;
        NvMemory memory;
        if (nv->handle(i, &device) != 0 || nv->uuid(device, uuid, sizeof(uuid)) != 0) {
            complete = FALSE;
            continue;
        }
        if (nv->name && nv->name(device, name, sizeof(name)) == 0)
            MultiByteToWideChar(CP_UTF8, 0, name, -1, label, _countof(label));
        const uint64_t key = identity(uuid, strlen(uuid));
        metric_device_seen(sample, "gpu", key);
        NvPci pci;
        if (nv->pci && nv->pci(device, &pci) == 0) {
            unsigned function = 0;
            const char *separator = strrchr(pci.bus_id, '.');
            if (separator)
                function = (unsigned)strtoul(separator + 1, NULL, 16);
            identities[known++] = (GpuIdentity){pci.domain, pci.bus, pci.device, function, i, key};
        }
        if (groups & GPU) {
            if (nv->temperature && nv->temperature(device, 0, &value) == 0) {
                device_metric(sample, "gpu", key, "temp", label, value);
                if (i == 0) metric_put(sample, "gpu.temp", "", value);
            }
            if (nv->utilization && nv->utilization(device, &load) == 0) {
                device_metric(sample, "gpu", key, "load", label, load.gpu);
                if (i == 0) metric_put(sample, "gpu.load", "", load.gpu);
            }
            if (nv->memory && nv->memory(device, &memory) == 0) {
                device_metric(sample, "gpu", key, "vram_used", label, (double)memory.used / 1e9);
                device_metric(sample, "gpu", key, "vram_total", label, (double)memory.total / 1e9);
                if (i == 0) metric_put(sample, "gpu.vram_used", "", (double)memory.used / 1e9);
            }
            if (nv->clock && nv->clock(device, NVML_GRAPHICS_CLOCK, &value) == 0) {
                device_metric(sample, "gpu", key, "clock", label, value);
                if (i == 0) metric_put(sample, "gpu.clock", "", value);
            }
            if (nv->power && nv->power(device, &value) == 0) {
                device_metric(sample, "gpu", key, "power", label, value / 1000.0);
                if (i == 0) metric_put(sample, "gpu.power", "", value / 1000.0);
            }
        }
        if (groups & TUNING) {
            if (nv->clock && nv->clock(device, NVML_MEMORY_CLOCK, &value) == 0)
                device_metric(sample, "gpu", key, "mem_clock", label, value);
            if (nv->power_limit && nv->power_limit(device, &value) == 0)
                device_metric(sample, "gpu", key, "power_limit", label, value / 1000.0);
        }
        if ((groups & JUNCTION) && nv->fields) {
            NvField field = {.field = NVML_MEMORY_TEMP};
            if (nv->fields(device, 1, &field) == 0 && field.status == 0) {
                const double temperature = current_nv_temperature(&field);
                if (temperature > 0 && temperature < 150) {
                    device_metric(sample, "gpu", key, "mem_junction", label, temperature);
                    if (i == 0) metric_put(sample, "gpu.mem_junction", "", temperature);
                }
            }
        }
    }
    if (nvapi_temp(sources, sample, groups, identities, known) && complete &&
        count <= MAX_GPU_DEVICES)
        sample->device_scan |= GPU;
    radeon_sample(sources->radeon, sample, groups);
}

/** Reuse a single PDH query and buffer for enabled per-core and disk counters. */
static void configure_pdh(Sources *sources, unsigned groups)
{
    if (sources->query)
        PdhCloseQuery(sources->query);
    sources->query = sources->cpu = sources->cpu_clock = sources->disk_read = sources->disk_write = NULL;
    if (!(groups & (CPU | STORAGE)) || PdhOpenQueryW(NULL, 0, &sources->query) != ERROR_SUCCESS)
        return;
    if (groups & CPU) {
        PdhAddEnglishCounterW(sources->query, L"\\Processor Information(*)\\% Processor Time",
                              0, &sources->cpu);
        PdhAddEnglishCounterW(sources->query, L"\\Processor Information(_Total)\\Processor Frequency",
                              0, &sources->cpu_clock);
    }
    if (groups & STORAGE) {
        PdhAddEnglishCounterW(sources->query, L"\\PhysicalDisk(*)\\Disk Read Bytes/sec",
                              0, &sources->disk_read);
        PdhAddEnglishCounterW(sources->query, L"\\PhysicalDisk(*)\\Disk Write Bytes/sec",
                              0, &sources->disk_write);
    }
}

static void pdh_sample(Sources *sources, Sample *sample, PDH_HCOUNTER counter,
                       const char *kind, const char *field)
{
    DWORD needed = sources->pdh_capacity, count = 0;
    if (!counter)
        return;
    PDH_STATUS status = PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE, &needed, &count,
                                                    sources->pdh_buffer);
    if (status == PDH_MORE_DATA && needed <= 1024 * 1024) {
        void *buffer = realloc(sources->pdh_buffer, needed);
        if (!buffer)
            return;
        sources->pdh_buffer = buffer;
        sources->pdh_capacity = needed;
        status = PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE, &needed, &count, buffer);
    }
    if (status != ERROR_SUCCESS)
        return;
    const BOOL disk_read = strcmp(kind, "disk") == 0 && strcmp(field, "read") == 0;
    if (disk_read)
        sample->device_scan |= STORAGE;
    const ULONGLONG now = GetTickCount64();
    const BOOL refresh_temperature = disk_read && now >= sources->next_disk_temperature;
    if (refresh_temperature) {
        sources->next_disk_temperature = now + 5000;
        sources->disk_temperature_count = 0;
    }
    const PDH_FMT_COUNTERVALUE_ITEM_W *items = sources->pdh_buffer;
    for (DWORD i = 0; i < count; ++i) {
        const wchar_t *name = items[i].szName;
        const PDH_FMT_COUNTERVALUE *value = &items[i].FmtValue;
        if (wcsstr(name, L"_Total") || value->CStatus == PDH_CSTATUS_NO_INSTANCE)
            continue;
        const uint64_t id = identity(name, wcslen(name) * sizeof(*name));
        const BOOL valid = value->CStatus == PDH_CSTATUS_VALID_DATA ||
                           value->CStatus == PDH_CSTATUS_NEW_DATA;
        device_metric(sample, kind, id, field, name, valid ? value->doubleValue : NAN);
        if (!disk_read)
            continue;
        metric_device_seen(sample, kind, id);
        if (refresh_temperature && sources->disk_temperature_count < _countof(sources->disk_temperatures)) {
            short temperature;
            if (drive_temperature(name, &temperature)) {
                const unsigned slot = sources->disk_temperature_count++;
                sources->disk_temperatures[slot].id = id;
                sources->disk_temperatures[slot].temperature = temperature;
            }
        }
        for (unsigned j = 0; j < sources->disk_temperature_count; ++j)
            if (sources->disk_temperatures[j].id == id)
                device_metric(sample, kind, id, "temp", name, sources->disk_temperatures[j].temperature);
    }
}

static uint64_t filetime_value(FILETIME value)
{
    return ((uint64_t)value.dwHighDateTime << 32) | value.dwLowDateTime;
}

static void cpu_sample(Sources *sources, Sample *sample)
{
    FILETIME idle, kernel, user;
    double load = NAN;
    if (GetSystemTimes(&idle, &kernel, &user)) {
        const uint64_t i = filetime_value(idle), k = filetime_value(kernel), u = filetime_value(user);
        if (sources->kernel && k >= sources->kernel && u >= sources->user && i >= sources->idle) {
            const uint64_t total = (k - sources->kernel) + (u - sources->user);
            const uint64_t idle_ticks = i - sources->idle;
            if (total && idle_ticks <= total)
                load = 100.0 * (double)(total - idle_ticks) / (double)total;
        }
        sources->idle = i;
        sources->kernel = k;
        sources->user = u;
    }
    metric_put(sample, "cpu.load", "", load);
}

/** Read all exposed interfaces and calculate rates with monotonic time and 64-bit counters. */
static void net_sample(Sources *sources, Sample *sample)
{
    MIB_IF_TABLE2 *table = NULL;
    if (GetIfTable2(&table) != NO_ERROR) {
        sources->net_count = 0;
        return;
    }
    const ULONGLONG now = GetTickCount64();
    NetCounter *next = calloc(table->NumEntries, sizeof(*next));
    if (!next && table->NumEntries) {
        FreeMibTable(table);
        return;
    }
    size_t next_count = 0;
    for (ULONG i = 0; i < table->NumEntries; ++i) {
        const MIB_IF_ROW2 *row = &table->Table[i];
        if (row->Type == IF_TYPE_SOFTWARE_LOOPBACK)
            continue;
        const uint64_t key = identity(&row->InterfaceGuid, sizeof(row->InterfaceGuid));
        metric_device_seen(sample, "net", key);
        const wchar_t *label = row->Alias[0] ? row->Alias : row->Description;
        const BOOL up = row->OperStatus == IfOperStatusUp;
        device_metric(sample, "net", key, "link", label, up ? 1.0 : 0.0);
        device_metric(sample, "net", key, "physical", label,
                      row->InterfaceAndOperStatusFlags.HardwareInterface ? 1.0 : 0.0);
        if (!up)
            continue;
        device_metric(sample, "net", key, "rx_link", label, (double)row->ReceiveLinkSpeed);
        device_metric(sample, "net", key, "tx_link", label, (double)row->TransmitLinkSpeed);
        for (size_t j = 0; now > sources->net_ms && j < sources->net_count; ++j) {
            const NetCounter *old = &sources->net[j];
            if (memcmp(&old->guid, &row->InterfaceGuid, sizeof(old->guid)) == 0) {
                const double scale = 1000.0 / (double)(now - sources->net_ms);
                if (row->InOctets >= old->rx && row->OutOctets >= old->tx) {
                    device_metric(sample, "net", key, "rx", label, (double)(row->InOctets - old->rx) * scale);
                    device_metric(sample, "net", key, "tx", label, (double)(row->OutOctets - old->tx) * scale);
                }
                break;
            }
        }
        next[next_count++] = (NetCounter){row->InterfaceGuid, row->InOctets, row->OutOctets};
        if (row->Type == IF_TYPE_IEEE80211) {
            DWORD version, size;
            WLAN_CONNECTION_ATTRIBUTES *connection = NULL;
            if (!sources->wlan)
                WlanOpenHandle(2, NULL, &version, &sources->wlan);
            if (sources->wlan && WlanQueryInterface(sources->wlan, &row->InterfaceGuid,
                wlan_intf_opcode_current_connection, NULL, &size, (void **)&connection, NULL) == ERROR_SUCCESS) {
                if (size >= sizeof(*connection))
                    device_metric(sample, "net", key, "signal", label,
                                  connection->wlanAssociationAttributes.wlanSignalQuality);
                WlanFreeMemory(connection);
            }
        }
    }
    FreeMibTable(table);
    free(sources->net);
    sources->net = next;
    sources->net_count = next_count;
    sources->net_ms = now;
    sample->device_scan |= NETWORK;
}

Sources *sources_open(void)
{
    Sources *sources = calloc(1, sizeof(*sources));
    if (sources)
        sources->has_bclk = bclk_supported();
    return sources;
}

void sources_sample(Sources *sources, Sample *sample, unsigned groups)
{
    sample_begin(sample, (groups & ALL_GROUPS) != sources->last_groups);
    sample->hotspot_access_denied = FALSE;
    sample->ryzen_access_denied = FALSE;
    if ((groups ^ sources->last_groups) & (CPU | STORAGE))
        configure_pdh(sources, groups);
    if ((groups ^ sources->last_groups) & STORAGE)
        sources->next_disk_temperature = 0;
    if ((groups ^ sources->last_groups) & CPU)
        sources->kernel = 0;
    if ((groups ^ sources->last_groups) & TUNING)
        sources->next_bclk = 0;
    if ((groups ^ sources->last_groups) & NETWORK)
        sources->net_count = 0;
    sources->last_groups = groups & ALL_GROUPS;
    if (groups & MEMORY) {
        MEMORYSTATUSEX memory = {.dwLength = sizeof(memory)};
        if (!sources->ddr5_attempted) {
            sources->ddr5_attempted = TRUE;
            sources->ddr5 = ddr5_open();
        }
        ddr5_sample(sources->ddr5, sample, (groups & PROBE_ALL_PMIC_RAILS) != 0);
        const BOOL valid = GlobalMemoryStatusEx(&memory);
        metric_put(sample, "mem.used", "", valid ?
                   (double)(memory.ullTotalPhys - memory.ullAvailPhys) / 1e9 : NAN);
        metric_put(sample, "mem.total", "", valid ? (double)memory.ullTotalPhys / 1e9 : NAN);
    }
    if (groups & CPU)
        cpu_sample(sources, sample);
    if (groups & (CPU | MEMORY | TUNING)) {
        if (!sources->ryzen_attempted) {
            sources->ryzen_attempted = TRUE;
            sources->ryzen = ryzen_open(&sources->ryzen_access_denied);
        }
        sample->ryzen_access_denied = sources->ryzen_access_denied;
        ryzen_sample(sources->ryzen, sample, groups);
    }
    if ((groups & TUNING) && sources->has_bclk) {
        const ULONGLONG now = GetTickCount64();
        if (now >= sources->next_bclk) {
            sources->bclk_mhz = bclk_read();
            sources->next_bclk = now + 5000;
        }
        metric_put(sample, "cpu.bclk", "", sources->bclk_mhz > 0 ? sources->bclk_mhz : NAN);
    }
    double cpu_clock = NAN;
    if (sources->query && PdhCollectQueryData(sources->query) == ERROR_SUCCESS) {
        PDH_FMT_COUNTERVALUE clock;
        if (sources->cpu_clock &&
            PdhGetFormattedCounterValue(sources->cpu_clock, PDH_FMT_DOUBLE, NULL, &clock) == ERROR_SUCCESS &&
            (clock.CStatus == PDH_CSTATUS_VALID_DATA || clock.CStatus == PDH_CSTATUS_NEW_DATA) &&
            clock.doubleValue > 0)
            cpu_clock = clock.doubleValue;
        pdh_sample(sources, sample, sources->cpu, "cpu", "core_load");
        pdh_sample(sources, sample, sources->disk_read, "disk", "read");
        pdh_sample(sources, sample, sources->disk_write, "disk", "write");
    }
    if (groups & CPU)
        metric_put(sample, "cpu.clock", "", cpu_clock);
    if (groups & NETWORK)
        net_sample(sources, sample);
    if (groups & (GPU | JUNCTION | HOTSPOT | TUNING))
        gpu_sample(sources, sample, groups);
    sample_end(sample);
}

void sources_close(Sources *sources)
{
    if (!sources)
        return;
    if (sources->query)
        PdhCloseQuery(sources->query);
    if (sources->wlan)
        WlanCloseHandle(sources->wlan, NULL);
    if (sources->nv.module) {
        sources->nv.shutdown();
        FreeLibrary(sources->nv.module);
    }
    if (sources->nvapi.module) {
        if (sources->nvapi.unload)
            sources->nvapi.unload();
        FreeLibrary(sources->nvapi.module);
    }
    radeon_close(sources->radeon);
    pawn_close(sources->hotspot);
    ryzen_close(sources->ryzen);
    ddr5_close(sources->ddr5);
    free(sources->pdh_buffer);
    free(sources->net);
    free(sources);
}
