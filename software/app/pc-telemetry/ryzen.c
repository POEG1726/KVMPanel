/*
 * ryzen.c
 * Created on Nov 23, 2026
 * Author: @POEG1726
 */

#include "telemetry.h"
#include <intrin.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
    const char *brand;
    unsigned version, floats, cores;
    unsigned fclk, uclk, mclk, soc_v;
    unsigned core_power, core_voltage, core_temp;
    unsigned dram_read, dram_write;
} RyzenProfile;

static const RyzenProfile profiles[] = {
    {"AMD Ryzen 7 9800X3D", 0x620105, 457, 8, 71, 75, 79, 83, 333, 309, 317, 96, 97},
    {"AMD Ryzen 9 9950X3D", 0x620205, 613, 16, 71, 75, 79, 83, 301, 317, 333, 96, 97},
    {"AMD Ryzen 5 5600X", 0x380905, 372, 6, 48, 50, 51, 45, 0, 0, 0, 68, 69},
};

struct Ryzen {
    Pawn *smu;
    Pawn *msr;
    HANDLE pci_mutex;
    const RyzenProfile *profile;
    ULONGLONG next_sample;
    float values[614];
    BOOL valid;
    GROUP_AFFINITY affinity[64];
    unsigned core_count;
    double ratios[64];
};

/** Match the documented map to this exact processor and firmware layout. */
static const RyzenProfile *cpu_profile(void)
{
    int data[4];
    __cpuid(data, (int)0x80000000);
    if ((unsigned)data[0] < 0x80000004)
        return NULL;
    char brand[49] = "";
    for (unsigned i = 0; i < 3; ++i) {
        __cpuid(data, (int)(0x80000002u + i));
        memcpy(brand + i * sizeof(data), data, sizeof(data));
    }
    const char *name = brand + strspn(brand, " ");
    for (unsigned i = 0; i < _countof(profiles); ++i) {
        const size_t length = strlen(profiles[i].brand);
        if (strncmp(name, profiles[i].brand, length) == 0 &&
            (name[length] == ' ' || name[length] == '\0'))
            return &profiles[i];
    }
    return NULL;
}

/** Identify one logical processor per physical core for local MSR reads. */
static void core_affinities(Ryzen *ryzen)
{
    DWORD bytes = 0;
    GetLogicalProcessorInformationEx(RelationProcessorCore, NULL, &bytes);
    if (!bytes || bytes > 64 * 1024)
        return;
    unsigned char *data = malloc(bytes);
    if (!data)
        return;
    if (GetLogicalProcessorInformationEx(RelationProcessorCore, (void *)data, &bytes)) {
        for (DWORD offset = 0; offset < bytes && ryzen->core_count < _countof(ryzen->affinity);) {
            const SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX *entry = (const void *)(data + offset);
            const size_t minimum = FIELD_OFFSET(SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX, Processor.GroupMask)
                                   + sizeof(GROUP_AFFINITY);
            if (entry->Size < minimum || entry->Size > bytes - offset)
                break;
            const GROUP_AFFINITY mask = entry->Processor.GroupMask[0];
            if (entry->Relationship == RelationProcessorCore && entry->Processor.GroupCount && mask.Mask) {
                GROUP_AFFINITY *core = &ryzen->affinity[ryzen->core_count++];
                *core = mask;
                core->Mask &= (KAFFINITY)0 - core->Mask;
            }
            offset += entry->Size;
        }
    }
    free(data);
}

/** Resolve the power management table with the driver's shared PCI mutex held. */
Ryzen *ryzen_open(BOOL *access_denied)
{
    *access_denied = FALSE;
    const RyzenProfile *profile = cpu_profile();
    if (!profile)
        return NULL;
    Ryzen *ryzen = calloc(1, sizeof(*ryzen));
    if (!ryzen)
        return NULL;
    ryzen->profile = profile;
    ryzen->smu = pawn_open(L"RyzenSMU.bin", access_denied);
    if (ryzen->smu) {
        ryzen->pci_mutex = CreateMutexW(NULL, FALSE, L"Global\\Access_PCI");
        if (ryzen->pci_mutex) {
            const DWORD lock = WaitForSingleObject(ryzen->pci_mutex, 50);
            if (lock == WAIT_OBJECT_0 || lock == WAIT_ABANDONED) {
                ULONG64 result[2];
                const BOOL known = pawn_read(ryzen->smu, "ioctl_resolve_pm_table", NULL, 0,
                                             result, _countof(result)) &&
                                   result[0] == profile->version && result[1] != 0;
                ReleaseMutex(ryzen->pci_mutex);
                if (known) {
                    BOOL denied;
                    ryzen->msr = pawn_open(L"AMDFamily17.bin", &denied);
                    if (ryzen->msr)
                        core_affinities(ryzen);
                    return ryzen;
                }
            }
        } else if (GetLastError() == ERROR_ACCESS_DENIED) {
            *access_denied = TRUE;
        }
    }
    ryzen_close(ryzen);
    return NULL;
}

/** Read hardware P-state ratio without substituting nominal Windows frequency. */
static void core_ratios(Ryzen *ryzen)
{
    const ULONG64 address = 0xc0010293;
    const BOOL granite = ryzen->profile->version != 0x380905;
    for (unsigned i = 0; i < ryzen->core_count; ++i) {
        GROUP_AFFINITY previous;
        ryzen->ratios[i] = 0;
        if (!SetThreadGroupAffinity(GetCurrentThread(), &ryzen->affinity[i], &previous))
            continue;
        ULONG64 status;
        const BOOL sampled = pawn_read(ryzen->msr, "ioctl_read_msr", &address, 1, &status, 1);
        if (!SetThreadGroupAffinity(GetCurrentThread(), &previous, NULL))
            break;
        if (sampled) {
            const unsigned divider = (unsigned)((status >> 8) & 0x3f);
            ryzen->ratios[i] = granite ? (double)(status & 0xfff) / 20.0 :
                                divider ? 2.0 * (double)(status & 0xff) / divider : 0;
        }
    }
}

/** Refresh the hardware telemetry table at most once per second. */
static void refresh(Ryzen *ryzen, unsigned groups)
{
    const ULONGLONG now = GetTickCount64();
    if (now < ryzen->next_sample)
        return;
    ryzen->next_sample = now + 1000;
    const DWORD lock = WaitForSingleObject(ryzen->pci_mutex, 30);
    if (lock != WAIT_OBJECT_0 && lock != WAIT_ABANDONED) {
        ryzen->valid = FALSE;
        return;
    }
    /* Ask the SMU to copy sensor readings to RAM. No tuning commands are sent. */
    ULONG64 packed[307];
    const size_t cells = (ryzen->profile->floats + 1u) / 2u;
    ryzen->valid = pawn_read(ryzen->smu, "ioctl_update_pm_table", NULL, 0, NULL, 0) &&
                   pawn_read(ryzen->smu, "ioctl_read_pm_table", NULL, 0, packed, cells);
    ReleaseMutex(ryzen->pci_mutex);
    if (ryzen->valid)
        memcpy(ryzen->values, packed, cells * sizeof(*packed));
    if (groups & TUNING)
        core_ratios(ryzen);
}

/** Publish only fields with a known identity and a credible sensor range. */
void ryzen_sample(Ryzen *ryzen, Sample *sample, unsigned groups)
{
    if (!ryzen)
        return;
    refresh(ryzen, groups);
    if (!ryzen->valid)
        return;
    const RyzenProfile *p = ryzen->profile;
    const float *v = ryzen->values;
    const BOOL granite = p->version != 0x380905;
    const uint64_t key = identity(p->brand, strlen(p->brand));
    if (groups & MEMORY) {
        /* The SMU reports average DRAM traffic in GB/s. The panel protocol uses B/s. */
        const double read = v[p->dram_read], write = v[p->dram_write];
        if (read >= 0 && read <= 200)
            device_metric(sample, "mem", key, "dram_read", NULL, read * 1e9);
        if (write >= 0 && write <= 200)
            device_metric(sample, "mem", key, "dram_write", NULL, write * 1e9);
    }
    if ((groups & CPU) && granite && v[11] > 0 && v[11] <= 115)
        metric_put(sample, "cpu.tctl", "", v[11]);
    if (groups & TUNING) {
        if (v[p->fclk] > 0 && v[p->fclk] < 6000)
            metric_put(sample, "cpu.fclk", "", v[p->fclk]);
        if (v[p->uclk] > 0 && v[p->uclk] < 6000)
            metric_put(sample, "mem.uclk", "", v[p->uclk]);
        if (v[p->mclk] > 0 && v[p->mclk] < 6000)
            metric_put(sample, "mem.mclk", "", v[p->mclk]);
        if (v[p->soc_v] > 0 && v[p->soc_v] < 2)
            device_metric(sample, "cpu", key, "soc_v", L"CPU SoC", v[p->soc_v]);
        for (unsigned i = 0; i < ryzen->core_count; ++i) {
            const double ratio = ryzen->ratios[i];
            if (ratio > 0 && ratio < 90) {
                wchar_t label[32];
                swprintf_s(label, _countof(label), L"Windows core %u", i);
                device_metric(sample, "cpu", key ^ (UINT64_C(1) << 62) ^ i, "ratio", label, ratio);
            }
        }
    }
    if (!granite) {
        if ((groups & CPU) && v[1] > 0 && v[1] < 350)
            device_metric(sample, "cpu", key, "package_power", L"CPU", v[1]);
        return;
    }
    if ((groups & CPU) && v[3] > 0 && v[3] < 500)
        metric_put(sample, "cpu.ppt", "", v[3]);
    if (groups & TUNING) {
        static const struct { unsigned index; const char *id; float max; } fields[] = {
            {2, "ppt_limit", 500}, {8, "tdc_limit", 500}, {9, "tdc", 500},
            {10, "thermal_limit", 120}, {63, "edc_limit", 500},
        };
        for (unsigned i = 0; i < _countof(fields); ++i) {
            const float value = v[fields[i].index];
            if (value > 0 && value < fields[i].max)
                device_metric(sample, "cpu", key, fields[i].id, L"CPU", value);
        }
        if (v[2] > 0 && v[3] > 0 && v[3] < 500)
            device_metric(sample, "cpu", key, "ppt_pct", L"CPU", 100.0 * v[3] / v[2]);
        if (v[8] > 0 && v[9] > 0 && v[9] < 500)
            device_metric(sample, "cpu", key, "tdc_pct", L"CPU", 100.0 * v[9] / v[8]);
        if (p->cores == 16 && v[58] > 0 && v[58] < 2)
            device_metric(sample, "cpu", key, "misc_v", L"CPU", v[58]);
        /* The Zen 5 SVI3 table reports VDD, SoC and Misc in five-float groups. */
        static const char *const currents[] = {"core_a", "soc_a", "misc_a"};
        for (unsigned i = 0; i < _countof(currents); ++i) {
            const double current = v[50 + 5 * i];
            if (current > 0 && current < 500)
                device_metric(sample, "cpu", key, currents[i], L"CPU", current);
        }
        if (v[49] > 0 && v[49] < 2)
            metric_put(sample, "cpu.vcore", "", v[49]);
    }
    for (unsigned i = 0; i < p->cores; ++i) {
        wchar_t label[32];
        swprintf_s(label, _countof(label), L"CPU core %u", i);
        const uint64_t core = key ^ (UINT64_C(1) << 63) ^ i;
        if ((groups & CPU) && v[p->core_temp + i] > 0 && v[p->core_temp + i] < 115)
            device_metric(sample, "cpu", core, "tctl", label, v[p->core_temp + i]);
        if ((groups & TUNING) && v[p->core_voltage + i] > 0 && v[p->core_voltage + i] < 2)
            device_metric(sample, "cpu", core, "vcore", label, v[p->core_voltage + i]);
        if ((groups & TUNING) && v[p->core_power + i] > 0 && v[p->core_power + i] < 100)
            device_metric(sample, "cpu", core, "core_power", label, v[p->core_power + i]);
    }
}

/** Release the signed module and PCI mutex. */
void ryzen_close(Ryzen *ryzen)
{
    if (!ryzen)
        return;
    pawn_close(ryzen->smu);
    pawn_close(ryzen->msr);
    if (ryzen->pci_mutex)
        CloseHandle(ryzen->pci_mutex);
    free(ryzen);
}
