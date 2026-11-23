/*
 * hotspot.c
 * Created on Nov 23, 2026
 * Author: @POEG1726
 */

#include "telemetry.h"

/** Read the highest valid temperature channel in the Blackwell GPU die. */
double hotspot_read(Pawn *pawn, unsigned bus, unsigned device, unsigned function)
{
    if (!pawn || bus > 255 || device > 31 || function > 7)
        return 0;
    const ULONG64 address[] = {bus, device, function};
    ULONG64 raw[6] = {0};
    if (!pawn_read(pawn, "ioctl_read_thermal_registers", address, _countof(address),
                   raw, _countof(raw)))
        return 0;

    /* Blackwell provides four die channels, a hardware maximum, and an average. */
    double maximum = 0;
    for (unsigned i = 0; i < 5; ++i) {
        if (!(raw[i] & (UINT64_C(1) << 30)))
            continue;
        const double degrees = (double)(raw[i] & UINT64_C(0xffff)) / 256.0;
        if (degrees > maximum && degrees < 150)
            maximum = degrees;
    }
    return maximum;
}
