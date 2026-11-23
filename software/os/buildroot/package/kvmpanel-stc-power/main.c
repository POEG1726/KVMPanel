/*
 * main.c
 * Created on Nov 22, 2026
 * Author: @POEG1726
 */

#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <linux/gpio.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/file.h>
#include <sys/ioctl.h>
#include <unistd.h>

#include "protocol.h"
#include "stc-power.h"

#define HOLD_OFFSET 42  /* T113 PB10/F17, bank B starts at offset 32. */
#define PIO_LABEL "2000000.pinctrl"
#define IMAGE_SIZE (STC_IMAGE_END + 6)

/** @brief Open the board UART and exclude other tty users. */
static int claim_uart(int fd)
{
    if (ioctl(fd, TIOCEXCL) < 0)
        return -1;
    return stc_serial_mode(fd, B9600, 0);
}

/** @brief Request PB10 high atomically and return its line handle. */
static int hold_high(void)
{
    struct gpiochip_info info;
    struct gpiohandle_request req = {
        .lineoffsets = { HOLD_OFFSET },
        .flags = GPIOHANDLE_REQUEST_OUTPUT,
        .default_values = { 1 },
        .consumer_label = "kvmpanel-update-hold",
        .lines = 1
    };
    char path[32];
    unsigned int chip;
    int fd, result, saved;
    for (chip = 0; chip < 32; ++chip) {
        snprintf(path, sizeof(path), "/dev/gpiochip%u", chip);
        fd = open(path, O_RDONLY | O_CLOEXEC);
        if (fd < 0)
            continue;
        if (ioctl(fd, GPIO_GET_CHIPINFO_IOCTL, &info) < 0 ||
            strcmp(info.label, PIO_LABEL)) {
            close(fd);
            continue;
        }
        result = ioctl(fd, GPIO_GET_LINEHANDLE_IOCTL, &req);
        saved = errno;
        close(fd);
        errno = saved;
        return result == 0 ? req.fd : -1;
    }
    errno = ENODEV;
    return -1;
}

/** @brief Check for a writable durable mount after the init shutdown actions. */
static int mounts_quiet(int fd)
{
    static const char *const volatile_fs[] = {
        "rootfs", "ramfs", "tmpfs", "devtmpfs", "proc", "sysfs", "devpts",
        "cgroup", "cgroup2", "debugfs", "tracefs", "securityfs", "configfs",
        "mqueue", "pstore", "bpf", "hugetlbfs", "fusectl", "binfmt_misc"
    };
    FILE *file = fdopen(fd, "r");
    char *line = NULL, *options, *super, *type;
    size_t capacity = 0, i;
    int quiet = 0, saw_root = 0;
    if (!file)
        return 0;
    while (getline(&line, &capacity, file) >= 0) {
        char *separator = strstr(line, " - ");
        char *fields = line;
        char *mountpoint;
        if (!separator)
            goto done;
        *separator = '\0';
        for (i = 0; i < 4; ++i) {
            if (!strsep(&fields, " "))
                goto done;
        }
        mountpoint = strsep(&fields, " ");
        options = strsep(&fields, " ");
        fields = separator + 3;
        type = strsep(&fields, " ");
        if (!mountpoint || !options || !type || !strsep(&fields, " ") || !fields)
            goto done;
        super = fields;
        saw_root |= !strcmp(mountpoint, "/");
        if ((strncmp(options, "rw,", 3) && strcmp(options, "rw")) ||
            !strncmp(super, "ro", 2))
            continue;
        for (i = 0; i < sizeof(volatile_fs) / sizeof(volatile_fs[0]); ++i) {
            if (!strcmp(type, volatile_fs[i]))
                break;
        }
        if (i == sizeof(volatile_fs) / sizeof(volatile_fs[0])) {
            fprintf(stderr, "STC poweroff blocked by writable mount %s (%s).\n",
                    mountpoint, type);
            goto done;
        }
    }
    quiet = feof(file) && !ferror(file) && saw_root;
done:
    free(line);
    fclose(file);
    return quiet;
}

/** @brief Check that the swapoff action left no active swap. */
static int swaps_quiet(int fd)
{
    FILE *file = fdopen(fd, "r");
    char header[256];
    int quiet;
    if (!file)
        return 0;
    quiet = fgets(header, sizeof(header), file) && !strncmp(header, "Filename", 8) &&
        fgetc(file) == EOF && feof(file) && !ferror(file);
    fclose(file);
    return quiet;
}

/** @brief Run the init-only preparation or post-cleanup arm step. */
static int init_command(int argc, char **argv)
{
    int fd = atoi(argv[2]);
    uint16_t reply;
    uint8_t cmd = CMD_PREPARE_OFF;
    if (claim_uart(fd) < 0)
        return -1;
    if (argc == 5) {
        if (!mounts_quiet(atoi(argv[3])) || !swaps_quiet(atoi(argv[4]))) {
            stc_request(fd, CMD_CANCEL_OFF, 0, &reply);
            errno = EBUSY;
            return -1;
        }
        cmd = CMD_ARM_OFF;
    }
    return stc_request(fd, cmd, 0, &reply);
}

/** @brief Arm the STC after the systemd shutdown hook checks local storage. */
static int late_poweroff(int fd)
{
    int mounts, swaps, clean_mounts, clean_swaps, saved;
    uint16_t reply;

    mounts = open("/proc/self/mountinfo", O_RDONLY | O_CLOEXEC);
    if (mounts < 0)
        return -1;
    swaps = open("/proc/swaps", O_RDONLY | O_CLOEXEC);
    if (swaps < 0) {
        saved = errno;
        close(mounts);
        errno = saved;
        return -1;
    }
    clean_mounts = mounts_quiet(mounts);
    clean_swaps = swaps_quiet(swaps);
    if (!clean_mounts || !clean_swaps) {
        stc_request(fd, CMD_CANCEL_OFF, 0, &reply);
        errno = EBUSY;
        return -1;
    }
    if (stc_request(fd, CMD_PREPARE_OFF, 0, &reply) < 0)
        return -1;
    if (stc_request(fd, CMD_ARM_OFF, 0, &reply) == 0)
        return 0;

    saved = errno;
    stc_request(fd, CMD_CANCEL_OFF, 0, &reply);
    errno = saved;
    return -1;
}

/** @brief Validate the local firmware image and hold PB10 across every ISP attempt. */
static int update(int fd, const char *path)
{
    uint8_t image[IMAGE_SIZE];
    uint16_t version, status;
    int line, retry = 0, signum;
    sigset_t signals;
    FILE *file = fopen(path, "rb");
    if (!file)
        return -1;
    if (fread(image, 1, sizeof(image), file) != sizeof(image) ||
        fgetc(file) != EOF || ferror(file) ||
        image[0] != 0x02 || memcmp(image + STC_IMAGE_END, "KVS8", 4)) {
        fclose(file);
        errno = EINVAL;
        return -1;
    }
    fclose(file);
    version = image[STC_IMAGE_END + 4] | ((uint16_t)image[STC_IMAGE_END + 5] << 8);
    if (stc_request(fd, CMD_STATUS, 0, &status) < 0)
        return -1;
    if (status != (STATUS_TAKEOVER | STATUS_ISP_ALLOWED)) {
        errno = EBUSY;
        return -1;
    }
    fprintf(stderr, "10-second forced shutdown is suspended during ISP. "
            "Keep T113 running while UPDATE_HOLD is asserted.\n");
    sigemptyset(&signals);
    sigaddset(&signals, SIGUSR1);
    if (sigprocmask(SIG_BLOCK, &signals, NULL) < 0)
        return -1;
    signal(SIGHUP, SIG_IGN);
    signal(SIGINT, SIG_IGN);
    signal(SIGTERM, SIG_IGN);
    signal(SIGQUIT, SIG_IGN);
    signal(SIGPIPE, SIG_IGN);
    line = hold_high();
    if (line < 0)
        return -1;
    for (;;) {
        if (stc_update_attempt(fd, line, image, sizeof(image), version, retry) == 0) {
            close(line);
            printf("STC8 %04x restored its outputs. UPDATE_HOLD released.\n", version);
            return 0;
        }
        fprintf(stderr, "STC8 update incomplete (%s). UPDATE_HOLD stays high. "
                "Send SIGUSR1 to PID %ld to retry this image.\n",
                strerror(errno), (long)getpid());
        do {
            signum = 0;
        } while (sigwait(&signals, &signum) || signum != SIGUSR1);
        retry = 1;
    }
}

/** @brief Run an exclusive board power command. */
int main(int argc, char **argv)
{
    int fd, lock, result;
    uint16_t status, version;
    if ((argc == 3 && !strcmp(argv[1], "init-prepare")) ||
        (argc == 5 && !strcmp(argv[1], "init-arm"))) {
        /* Only BusyBox PID 1 can request these steps with its inherited FDs. */
        if (getppid() != 1)
            return 2;
        if (init_command(argc, argv) == 0)
            return 0;
        perror(argv[1]);
        return 1;
    }
    if (!((argc == 2 && (!strcmp(argv[1], "status") || !strcmp(argv[1], "late-poweroff"))) ||
          (argc == 4 && !strcmp(argv[1], "update") && !strcmp(argv[2], "--hold-wired")))) {
        fprintf(stderr, "Usage: %s status | late-poweroff | update --hold-wired IMAGE.bin\n",
                argv[0]);
        return 2;
    }
    lock = open(STC_LOCK, O_RDWR | O_CREAT | O_NOFOLLOW | O_CLOEXEC, 0600);
    if (lock < 0 || flock(lock, LOCK_EX | LOCK_NB) < 0) {
        perror("STC power lock");
        return 1;
    }
    fd = open(STC_TTY, O_RDWR | O_NOCTTY | O_NONBLOCK | O_CLOEXEC);
    if (fd < 0 || claim_uart(fd) < 0) {
        perror(STC_TTY);
        return 1;
    }
    if (argc == 4) {
        result = update(fd, argv[3]);
    } else if (!strcmp(argv[1], "late-poweroff")) {
        result = late_poweroff(fd);
    } else {
        result = stc_request(fd, CMD_VERSION, 0, &version) ||
            stc_request(fd, CMD_STATUS, 0, &status);
        if (!result)
            printf("STC8 %04x main=%d pd=%d reset_released=%d power_flag=%d "
                   "shutdown_pending=%d isp_pending=%d flash_error=%d isp_allowed=%d\n",
                   version, !!(status & STATUS_MAIN), !!(status & STATUS_PD),
                   !!(status & STATUS_RESET_RELEASED), !!(status & STATUS_FLAG),
                   !!(status & STATUS_OFF_PENDING), !!(status & STATUS_ISP_PENDING),
                   !!(status & STATUS_FLASH_ERROR), !!(status & STATUS_ISP_ALLOWED));
    }
    if (result)
        perror("STC power");
    close(fd);
    close(lock);
    return result ? 1 : 0;
}
