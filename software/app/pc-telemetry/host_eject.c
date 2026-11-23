/*
 * host_eject.c
 * Created on Nov 23, 2026
 * Author: @POEG1726
 */

#include "host_agent.h"
#include <initguid.h>
#include <ntddscsi.h>
#include <setupapi.h>
#include <stdlib.h>
#include <winioctl.h>

enum { SHARE_LUN = 2, MAX_VOLUMES = 32 };

DEVINST host_usb_parent(DEVINST device)
{
    static const wchar_t prefix[] = L"USB\\VID_1D6B&PID_0104\\";
    for (unsigned depth = 0; device && depth < 32; ++depth) {
        wchar_t id[MAX_DEVICE_ID_LEN];
        if (CM_Get_Device_IDW(device, id, _countof(id), 0) != CR_SUCCESS)
            break;
        if (_wcsnicmp(id, prefix, _countof(prefix) - 1) == 0)
            return device;
        DEVINST parent;
        if (CM_Get_Parent(&parent, device, 0) != CR_SUCCESS)
            break;
        device = parent;
    }
    return 0;
}

/** Keep the operation inside the response deadline and drain cancellation before releasing buffers. */
static BOOL storage_control(HANDLE device, DWORD code, void *result, DWORD size, ULONGLONG deadline)
{
    if (GetTickCount64() >= deadline) {
        SetLastError(ERROR_TIMEOUT);
        return FALSE;
    }
    OVERLAPPED operation = {.hEvent = CreateEventW(NULL, TRUE, FALSE, NULL)};
    if (!operation.hEvent)
        return FALSE;
    DWORD copied = 0;
    BOOL ok = DeviceIoControl(device, code, NULL, 0, result, size, &copied, &operation);
    DWORD error = ok ? ERROR_SUCCESS : GetLastError();
    if (!ok && error == ERROR_IO_PENDING) {
        const ULONGLONG now = GetTickCount64();
        const DWORD wait = WaitForSingleObject(operation.hEvent, now < deadline ? (DWORD)(deadline - now) : 0);
        if (wait == WAIT_OBJECT_0) {
            ok = GetOverlappedResult(device, &operation, &copied, FALSE);
            error = ok ? ERROR_SUCCESS : GetLastError();
        } else {
            error = wait == WAIT_TIMEOUT ? ERROR_TIMEOUT : GetLastError();
            CancelIoEx(device, &operation);
            GetOverlappedResult(device, &operation, &copied, TRUE);
        }
    }
    CloseHandle(operation.hEvent);
    SetLastError(error);
    return ok;
}

static HANDLE storage_open(const wchar_t *path, DWORD access)
{
    return CreateFileW(path, access, FILE_SHARE_READ | FILE_SHARE_WRITE, NULL, OPEN_EXISTING,
                       FILE_FLAG_OVERLAPPED, NULL);
}

static const char *storage_error(const char *fallback, DWORD error)
{
    switch (error) {
    case ERROR_ACCESS_DENIED: case ERROR_PRIVILEGE_NOT_HELD: return "eject_access_denied";
    case ERROR_TIMEOUT: return "eject_timeout";
    case ERROR_SHARING_VIOLATION: case ERROR_LOCK_VIOLATION: case ERROR_BUSY: return "eject_busy";
    default: return fallback;
    }
}

/** Validate both the USB device instance and the disk's SCSI address before writing anything. */
static const char *open_share(DEVINST parent, ULONGLONG deadline, HANDLE *disk, DWORD *number)
{
    if (!parent)
        return "eject_device_unidentified";
    HDEVINFO devices = SetupDiGetClassDevsW(&GUID_DEVINTERFACE_DISK, NULL, NULL,
                                           DIGCF_DEVICEINTERFACE | DIGCF_PRESENT);
    if (devices == INVALID_HANDLE_VALUE)
        return "eject_device_unidentified";
    const char *error = NULL;
    for (DWORD index = 0; !error; ++index) {
        SP_DEVICE_INTERFACE_DATA entry = {.cbSize = sizeof(entry)};
        if (!SetupDiEnumDeviceInterfaces(devices, NULL, &GUID_DEVINTERFACE_DISK, index, &entry)) {
            if (GetLastError() != ERROR_NO_MORE_ITEMS)
                error = "eject_device_unidentified";
            break;
        }
        DWORD bytes = 0;
        SetupDiGetDeviceInterfaceDetailW(devices, &entry, NULL, 0, &bytes, NULL);
        SP_DEVICE_INTERFACE_DETAIL_DATA_W *detail = bytes >= sizeof(*detail) ? malloc(bytes) : NULL;
        SP_DEVINFO_DATA info = {.cbSize = sizeof(info)};
        if (!detail) {
            error = "eject_device_unidentified";
            break;
        }
        detail->cbSize = sizeof(*detail);
        if (!SetupDiGetDeviceInterfaceDetailW(devices, &entry, detail, bytes, NULL, &info)) {
            error = "eject_device_unidentified";
        } else if (host_usb_parent(info.DevInst) == parent) {
            HANDLE candidate = storage_open(detail->DevicePath, 0);
            SCSI_ADDRESS address = {0};
            if (candidate == INVALID_HANDLE_VALUE ||
                !storage_control(candidate, IOCTL_SCSI_GET_ADDRESS, &address, sizeof(address), deadline)) {
                error = storage_error("eject_device_unidentified", GetLastError());
            } else if (address.Length < sizeof(address)) {
                error = "eject_device_unidentified";
            } else if (address.Lun == SHARE_LUN) {
                if (*disk != INVALID_HANDLE_VALUE) {
                    error = "eject_device_ambiguous";
                } else {
                    *disk = storage_open(detail->DevicePath, GENERIC_READ | GENERIC_WRITE);
                    STORAGE_DEVICE_NUMBER device = {0};
                    if (*disk == INVALID_HANDLE_VALUE ||
                        !storage_control(*disk, IOCTL_STORAGE_GET_DEVICE_NUMBER, &device, sizeof(device), deadline))
                        error = storage_error("eject_device_unidentified", GetLastError());
                    else if (device.DeviceType == FILE_DEVICE_DISK && device.DeviceNumber != MAXDWORD)
                        *number = device.DeviceNumber;
                    else
                        error = "eject_device_unidentified";
                }
            }
            if (candidate != INVALID_HANDLE_VALUE)
                CloseHandle(candidate);
        }
        free(detail);
    }
    SetupDiDestroyDeviceInfoList(devices);
    return error ? error : *disk == INVALID_HANDLE_VALUE ? "eject_device_unidentified" : NULL;
}

/** Lock every usable filesystem on the target disk before dismounting any of them. */
static const char *lock_volumes(DWORD number, ULONGLONG deadline, HANDLE *volumes, unsigned *count)
{
    wchar_t name[MAX_PATH];
    HANDLE search = FindFirstVolumeW(name, _countof(name));
    if (search == INVALID_HANDLE_VALUE)
        return "eject_volumes_unknown";
    const char *error = NULL;
    do {
        const size_t length = wcslen(name);
        if (!length || name[length - 1] != L'\\') {
            error = "eject_volumes_unknown";
            break;
        }
        name[length - 1] = L'\0';
        HANDLE probe = storage_open(name, 0);
        if (probe == INVALID_HANDLE_VALUE) {
            error = storage_error("eject_volumes_unknown", GetLastError());
            break;
        }
        STORAGE_DEVICE_NUMBER device = {0};
        const BOOL found = storage_control(probe, IOCTL_STORAGE_GET_DEVICE_NUMBER, &device, sizeof(device), deadline);
        const DWORD failure = GetLastError();
        CloseHandle(probe);
        if (!found) {
            /* An empty removable-media volume has no mounted filesystem to flush. */
            if (failure != ERROR_NOT_READY && failure != ERROR_NO_MEDIA_IN_DRIVE)
                error = storage_error("eject_volumes_unknown", failure);
        } else if (device.DeviceType == FILE_DEVICE_DISK && device.DeviceNumber == number) {
            if (*count == MAX_VOLUMES) {
                error = "eject_volumes_unknown";
                break;
            }
            HANDLE volume = storage_open(name, GENERIC_READ | GENERIC_WRITE);
            if (volume == INVALID_HANDLE_VALUE) {
                error = storage_error("eject_volumes_unknown", GetLastError());
                break;
            }
            volumes[(*count)++] = volume;
            VOLUME_DISK_EXTENTS extents = {0};
            if (!storage_control(volume, IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS, &extents, sizeof(extents), deadline) ||
                extents.NumberOfDiskExtents != 1 || extents.Extents[0].DiskNumber != number) {
                error = "eject_volumes_unknown";
                break;
            }
            /* Windows flushes cached data as part of a successful exclusive volume lock. */
            if (!storage_control(volume, FSCTL_LOCK_VOLUME, NULL, 0, deadline)) {
                error = storage_error("eject_busy", GetLastError());
                break;
            }
        }
    } while (!error && FindNextVolumeW(search, name, _countof(name)));
    if (!error && (GetLastError() != ERROR_NO_MORE_FILES || !*count))
        error = "eject_volumes_unknown";
    FindVolumeClose(search);
    return error;
}

const char *host_eject(DEVINST parent, ULONGLONG deadline)
{
    HANDLE disk = INVALID_HANDLE_VALUE, volumes[MAX_VOLUMES];
    DWORD number = MAXDWORD;
    unsigned count = 0;
    const char *error = open_share(parent, deadline, &disk, &number);
    if (!error)
        error = lock_volumes(number, deadline, volumes, &count);
    for (unsigned i = 0; !error && i < count; ++i)
        if (!storage_control(volumes[i], FSCTL_DISMOUNT_VOLUME, NULL, 0, deadline))
            error = storage_error("eject_dismount_failed", GetLastError());
    if (!error && !storage_control(disk, IOCTL_STORAGE_EJECT_MEDIA, NULL, 0, deadline))
        error = storage_error("eject_rejected", GetLastError());
    for (unsigned i = 0; i < count; ++i)
        CloseHandle(volumes[i]);
    if (disk != INVALID_HANDLE_VALUE)
        CloseHandle(disk);
    return error;
}
