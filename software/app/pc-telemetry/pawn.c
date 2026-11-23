/*
 * pawn.c
 * Created on Nov 23, 2026
 * Author: @POEG1726
 */

#include "telemetry.h"
#include <stdlib.h>
#include <wchar.h>

struct Pawn {
    HMODULE library;
    HANDLE executor;
    HRESULT (WINAPI *close)(HANDLE);
    HRESULT (WINAPI *read)(HANDLE, PCSTR, const ULONG64 *, SIZE_T, PULONG64, SIZE_T, PSIZE_T);
};

/** Load a signed sensor module placed next to the collector. */
Pawn *pawn_open(const wchar_t *name, BOOL *access_denied)
{
    *access_denied = FALSE;
    wchar_t path[MAX_PATH];
    const DWORD length = GetModuleFileNameW(NULL, path, _countof(path));
    if (!length || length >= _countof(path))
        return NULL;
    wchar_t *file = wcsrchr(path, L'\\');
    if (!file || wcscpy_s(file + 1, _countof(path) - (size_t)(file + 1 - path), name))
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
    Pawn *result = NULL;
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
    pawn_close(result);
    result = NULL;
done:
    free(binary);
    return result;
}

/** Call a driver-validated module operation and check the returned cell count. */
BOOL pawn_read(Pawn *pawn, const char *operation, const ULONG64 *in, SIZE_T in_count,
               ULONG64 *out, SIZE_T out_count)
{
    SIZE_T received = 0;
    return pawn &&
           SUCCEEDED(pawn->read(pawn->executor, operation, in, in_count, out, out_count, &received)) &&
           received == out_count;
}

void pawn_close(Pawn *pawn)
{
    if (!pawn)
        return;
    if (pawn->executor && pawn->executor != INVALID_HANDLE_VALUE)
        pawn->close(pawn->executor);
    if (pawn->library)
        FreeLibrary(pawn->library);
    free(pawn);
}
