/*
 * ddr5.c
 * Created on Nov 23, 2026
 * Author: @POEG1726
 */

#include "telemetry.h"
#include <intrin.h>
#include <stdio.h>
#include <stdlib.h>

enum { SMBIOS_PROVIDER = 0x52534d42, SMBIOS_DDR5 = 0x22, SPD_FIRST = 0x50, SPD_SLOTS = 8 };

struct Ddr5 {
    Pawn *bus;
    HANDLE mutex;
    unsigned hubs;
    unsigned active;
    unsigned pmic_active;
    unsigned pmic_hot, pmic_ov, pmic_uv;
    BOOL scanned;
    ULONGLONG next_sample;
    double temperatures[SPD_SLOTS];
};

/** Probe only machines whose SMBIOS reports an installed DDR5 module. */
static BOOL installed_ddr5(void)
{
    const UINT bytes = GetSystemFirmwareTable(SMBIOS_PROVIDER, 0, NULL, 0);
    if (bytes < 8 || bytes > 1024 * 1024)
        return FALSE;
    unsigned char *table = malloc(bytes);
    if (!table)
        return FALSE;
    BOOL result = FALSE;
    if (GetSystemFirmwareTable(SMBIOS_PROVIDER, 0, table, bytes) == bytes) {
        const unsigned char *end = table + bytes;
        for (const unsigned char *entry = table + 8; end - entry >= 4;) {
            if (entry[1] < 4 || end - entry < entry[1] || entry[0] == 127)
                break;
            if (entry[0] == 17 && entry[1] > 0x12 && entry[0x12] == SMBIOS_DDR5 &&
                (entry[0x0c] || entry[0x0d])) {
                result = TRUE;
                break;
            }
            entry += entry[1];
            while (end - entry >= 2 && (entry[0] || entry[1]))
                ++entry;
            if (end - entry < 2)
                break;
            entry += 2;
        }
    }
    free(table);
    return result;
}

/** Load the SMBus provider for the CPU vendor's supported controller. */
Ddr5 *ddr5_open(void)
{
    int cpuid[4];
    __cpuid(cpuid, 0);
    const wchar_t *filename = cpuid[1] == 0x68747541 ? L"SmbusPIIX4.bin" :
                              cpuid[1] == 0x756e6547 ? L"SmbusI801.bin" : NULL;
    if (!filename || !installed_ddr5())
        return NULL;
    Ddr5 *ddr5 = calloc(1, sizeof(*ddr5));
    if (!ddr5)
        return NULL;
    BOOL denied;
    ddr5->bus = pawn_open(filename, &denied);
    if (ddr5->bus)
        ddr5->mutex = CreateMutexW(NULL, FALSE, L"Global\\Access_SMBUS.HTP.Method");
    if (ddr5->mutex)
        return ddr5;
    ddr5_close(ddr5);
    return NULL;
}

/** SPD5118 exposes its identity and temperature through SMBus byte/word reads. */
static BOOL read_register(Ddr5 *ddr5, unsigned addr, unsigned reg, BOOL word, ULONG64 *result)
{
    const ULONG64 args[] = {addr, 1, reg, word ? 3 : 2};
    return pawn_read(ddr5->bus, "ioctl_smbus_xfer", args, _countof(args), result, 1);
}

/** Read the matched DIMM's JEDEC PMIC real-time status registers. */
static void pmic_status(Ddr5 *ddr5, unsigned slot)
{
    const unsigned address = 0x48 + slot;
    const unsigned flag = 1u << slot;
    ULONG64 vendor, enabled, input, thermal, ov, uv;
    if (!read_register(ddr5, address, 0x3c, TRUE, &vendor) || !vendor || vendor == 0xffff ||
        !read_register(ddr5, address, 0x32, FALSE, &enabled) || !(enabled & 0x80) ||
        !read_register(ddr5, address, 0x08, FALSE, &input) ||
        !read_register(ddr5, address, 0x09, FALSE, &thermal) ||
        !read_register(ddr5, address, 0x0a, FALSE, &ov) ||
        !read_register(ddr5, address, 0x0b, FALSE, &uv))
        return;
    ddr5->pmic_active |= flag;
    if ((thermal & 0x80) || (input & 0x40))
        ddr5->pmic_hot |= flag;
    if ((ov & 0xf0) || (input & 0x01))
        ddr5->pmic_ov |= flag;
    if (uv & 0x0f)
        ddr5->pmic_uv |= flag;
}

/** Read the SPD hub without selecting NVM pages or modifying settings. */
static void refresh(Ddr5 *ddr5)
{
    const ULONGLONG now = GetTickCount64();
    if (now < ddr5->next_sample)
        return;
    ddr5->next_sample = now + 4000;
    ddr5->active = 0;
    ddr5->pmic_active = ddr5->pmic_hot = ddr5->pmic_ov = ddr5->pmic_uv = 0;
    const DWORD lock = WaitForSingleObject(ddr5->mutex, 30);
    if (lock != WAIT_OBJECT_0 && lock != WAIT_ABANDONED)
        return;
    for (unsigned i = 0; i < SPD_SLOTS; ++i) {
        const unsigned addr = SPD_FIRST + i;
        const unsigned flag = 1u << i;
        ULONG64 raw;
        if (!ddr5->scanned && read_register(ddr5, addr, 0, TRUE, &raw) && raw == 0x1851)
            ddr5->hubs |= flag;
        if (!(ddr5->hubs & flag))
            continue;
        pmic_status(ddr5, i);
        if (!read_register(ddr5, addr, 5, FALSE, &raw) || !(raw & 2) ||
            !read_register(ddr5, addr, 0x1a, FALSE, &raw) || (raw & 1) ||
            !read_register(ddr5, addr, 0x31, TRUE, &raw))
            continue;
        int degrees = (int)((raw >> 2) & 0x7ff);
        if (degrees & 0x400)
            degrees -= 0x800;
        if (degrees >= -160 && degrees <= 500) {
            ddr5->temperatures[i] = degrees * 0.25;
            ddr5->active |= flag;
        }
    }
    ddr5->scanned = TRUE;
    ReleaseMutex(ddr5->mutex);
}

/** Publish temperatures keyed by hub address, without guessing motherboard slot labels. */
void ddr5_sample(Ddr5 *ddr5, Sample *sample)
{
    if (!ddr5)
        return;
    refresh(ddr5);
    for (unsigned i = 0; i < SPD_SLOTS; ++i) {
        const unsigned flag = 1u << i;
        if (ddr5->active & flag) {
            char id[24];
            snprintf(id, sizeof(id), "mem.dimm.%02x", SPD_FIRST + i);
            metric_put(sample, id, "", ddr5->temperatures[i]);
        }
        if (ddr5->pmic_active & flag) {
            wchar_t label[32];
            swprintf_s(label, _countof(label), L"DIMM 0x%02X", SPD_FIRST + i);
            const uint64_t key = SPD_FIRST + i;
            device_metric(sample, "dimm", key, "pmic_hot", label, (ddr5->pmic_hot & flag) != 0);
            device_metric(sample, "dimm", key, "pmic_ov", label, (ddr5->pmic_ov & flag) != 0);
            device_metric(sample, "dimm", key, "pmic_uv", label, (ddr5->pmic_uv & flag) != 0);
        }
    }
}

void ddr5_close(Ddr5 *ddr5)
{
    if (!ddr5)
        return;
    pawn_close(ddr5->bus);
    if (ddr5->mutex)
        CloseHandle(ddr5->mutex);
    free(ddr5);
}
