/*
 * bclk.c
 * Created on Nov 23, 2026
 * Author: @POEG1726
 */

#include "telemetry.h"
#include <intrin.h>
#include <wchar.h>

/** Match the FCH clock layout on Vermeer, Raphael and Granite Ridge desktops. */
static BOOL supported_cpu(void)
{
    int regs[4];
    __cpuid(regs, 0);
    if (regs[0] < 1 || regs[1] != 0x68747541 || regs[3] != 0x69746e65 ||
        regs[2] != 0x444d4163)
        return FALSE;

    __cpuid(regs, 1);
    const unsigned id = (unsigned)regs[0];
    const unsigned family = (id >> 8) & 0xf;
    if (family != 0xf)
        return FALSE;
    const unsigned extended_family = family + ((id >> 20) & 0xff);
    const unsigned model = ((id >> 12) & 0xf0) | ((id >> 4) & 0xf);
    return (extended_family == 0x19 && (model >> 4 == 2 || model >> 4 == 6)) ||
           (extended_family == 0x1a && model >> 4 == 4);
}

/** Read the programmed internal PLL reference through an already available InpOut device. */
double bclk_read(void)
{
    if (!supported_cpu())
        return 0;

    wchar_t path[MAX_PATH];
    const DWORD length = GetModuleFileNameW(NULL, path, _countof(path));
    if (!length || length >= _countof(path))
        return 0;
    wchar_t *file = wcsrchr(path, L'\\');
    if (!file || wcscpy_s(file + 1, _countof(path) - (size_t)(file + 1 - path),
                          L"InpOutx64.dll"))
        return 0;
    if (GetFileAttributesW(path) == INVALID_FILE_ATTRIBUTES)
        return 0;

    /* The vendor DLL may try to install its driver if the device is unavailable. */
    HANDLE device = CreateFileW(L"\\\\.\\inpoutx64", GENERIC_READ | GENERIC_WRITE, 0,
                                NULL, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, NULL);
    if (device == INVALID_HANDLE_VALUE)
        return 0;
    CloseHandle(device);

    HMODULE library = LoadLibraryExW(path, NULL, LOAD_LIBRARY_SEARCH_SYSTEM32 |
                                                LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR);
    if (!library)
        return 0;
#pragma warning(push)
#pragma warning(disable: 4191)
    BOOL (WINAPI *read_register)(const BYTE *, DWORD *) =
        (BOOL (WINAPI *)(const BYTE *, DWORD *))GetProcAddress(library, "GetPhysLong");
#pragma warning(pop)
    DWORD strap, pll;
    double mhz = 0;
    /* FCH MISCx80[17] selects the internal clock. An external PLL can leave MISCx10 stale. */
    if (read_register &&
        read_register((const BYTE *)(UINT_PTR)0xfed80e80u, &strap) &&
        strap != UINT32_MAX && (strap & (1u << 17)) &&
        read_register((const BYTE *)(UINT_PTR)0xfed80e10u, &pll)) {
        /* ZenStates-Core documents the CGPLLConfig3 index and 1/16 MHz fraction. */
        const unsigned index = (pll >> 4) & 0x1ff;
        const unsigned integer = index ^ (index < 32 ? 100u : 164u);
        if (integer >= 96 && integer <= 151)
            mhz = (double)integer + ((pll >> 25) & 0xf) / 16.0;
    }
    FreeLibrary(library);
    return mhz;
}
