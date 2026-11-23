/*
 * hotspot.c
 * Created on Nov 23, 2026
 * Author: @POEG1726
 */

#include "telemetry.h"
#include <stdlib.h>
#include <wchar.h>

struct Hotspot {
    HMODULE library;
    HANDLE executor;
    HRESULT (WINAPI *close)(HANDLE);
    HRESULT (WINAPI *read)(HANDLE, PCSTR, const ULONG64 *, SIZE_T, PULONG64, SIZE_T, PSIZE_T);
};

/** Open the official signed NVIDIA sensor module placed next to the collector. */
Hotspot *hotspot_open(BOOL *access_denied)
{
    *access_denied = FALSE;
    wchar_t path[MAX_PATH];
    const DWORD length = GetModuleFileNameW(NULL, path, _countof(path));
    if (!length || length >= _countof(path))
        return NULL;
    wchar_t *file = wcsrchr(path, L'\\');
    if (!file || wcscpy_s(file + 1, _countof(path) - (size_t)(file + 1 - path), L"Nvidia.bin"))
        return NULL;
    HANDLE module = CreateFileW(path, GENERIC_READ, FILE_SHARE_READ, NULL, OPEN_EXISTING, 0, NULL);
    if (module == INVALID_HANDLE_VALUE)
        return NULL;
    LARGE_INTEGER bytes;
    if (!GetFileSizeEx(module, &bytes) || bytes.QuadPart < 1 || bytes.QuadPart > 65536) {
        CloseHandle(module);
        return NULL;
    }
    const DWORD size = (DWORD)bytes.QuadPart;
    unsigned char *binary = malloc(size);
    DWORD received = 0;
    const BOOL loaded = binary && ReadFile(module, binary, size, &received, NULL) && received == size;
    CloseHandle(module);
    Hotspot *result = NULL;
    if (!loaded)
        goto done;
    const DWORD expanded = ExpandEnvironmentStringsW(
        L"%ProgramW6432%\\PawnIO\\PawnIOLib.dll", path, _countof(path));
    if (!expanded || expanded > _countof(path))
        goto done;
    result = calloc(1, sizeof(*result));
    if (!result)
        goto done;
    result->library = LoadLibraryExW(path, NULL, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR |
                                                 LOAD_LIBRARY_SEARCH_SYSTEM32);
    if (result->library) {
#pragma warning(push)
#pragma warning(disable: 4191)
        HRESULT (WINAPI *open)(PHANDLE) = (HRESULT (WINAPI *)(PHANDLE))
            GetProcAddress(result->library, "pawnio_open");
        HRESULT (WINAPI *load)(HANDLE, const UCHAR *, SIZE_T) =
            (HRESULT (WINAPI *)(HANDLE, const UCHAR *, SIZE_T))
            GetProcAddress(result->library, "pawnio_load");
        result->close = (HRESULT (WINAPI *)(HANDLE))GetProcAddress(result->library, "pawnio_close");
        result->read = (HRESULT (WINAPI *)(HANDLE, PCSTR, const ULONG64 *, SIZE_T,
                                          PULONG64, SIZE_T, PSIZE_T))
            GetProcAddress(result->library, "pawnio_execute");
#pragma warning(pop)
        if (open && load && result->close && result->read) {
            HRESULT status = open(&result->executor);
            if (SUCCEEDED(status))
                status = load(result->executor, binary, size);
            if (SUCCEEDED(status)) {
                free(binary);
                return result;
            }
            *access_denied = status == E_ACCESSDENIED;
        }
    }
    hotspot_close(result);
    result = NULL;
done:
    free(binary);
    return result;
}

double hotspot_read(Hotspot *hotspot, unsigned bus, unsigned device, unsigned function)
{
    if (!hotspot || bus > 255 || device > 31 || function > 7)
        return 0;
    const ULONG64 address[] = {bus, device, function};
    ULONG64 raw[6] = {0};
    SIZE_T count = 0;
    if (FAILED(hotspot->read(hotspot->executor, "ioctl_read_thermal_registers",
                             address, _countof(address), raw, _countof(raw), &count)) ||
        count != _countof(raw))
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

void hotspot_close(Hotspot *hotspot)
{
    if (!hotspot)
        return;
    if (hotspot->executor && hotspot->executor != INVALID_HANDLE_VALUE)
        hotspot->close(hotspot->executor);
    if (hotspot->library)
        FreeLibrary(hotspot->library);
    free(hotspot);
}
