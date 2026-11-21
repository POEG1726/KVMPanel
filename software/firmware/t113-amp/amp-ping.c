// SPDX-License-Identifier: GPL-2.0-only
/*
 * amp-ping.c
 * Created on Nov 20, 2026
 * Author: @POEG1726
 */

#include <fcntl.h>
#include <poll.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

/**
 * @brief Send one packet and verify the echo on the same RPMsg endpoint.
 */
static int ping(int fd)
{
	static const char request[] = "kvmpanel-amp-echo";
	const ssize_t request_size = sizeof(request) - 1;
	struct pollfd ready = { .fd = fd, .events = POLLIN };
	char reply[512];
	ssize_t received;
	int ret;

	ret = write(fd, request, request_size);
	if (ret < 0) {
		perror("write");
		return 1;
	}
	if (ret != request_size) {
		fputs("RPMsg request was truncated\n", stderr);
		return 1;
	}

	ret = poll(&ready, 1, 3000);
	if (ret < 0) {
		perror("poll");
		return 1;
	}
	if (!ret) {
		fputs("RPMsg echo timed out\n", stderr);
		return 1;
	}

	received = read(fd, reply, sizeof(reply));
	if (received < 0) {
		perror("read");
		return 1;
	}
	if (received != request_size || memcmp(reply, request, request_size) != 0) {
		fputs("RPMsg echo payload mismatch\n", stderr);
		return 1;
	}
	return 0;
}

int main(int argc, char **argv)
{
	struct stat st;
	int fd, ret;

	if (argc != 2 || stat(argv[1], &st) != 0 || !S_ISCHR(st.st_mode)) {
		fputs("Usage: kvmpanel-amp-ping /dev/rpmsgN\n", stderr);
		return 2;
	}

	fd = open(argv[1], O_RDWR | O_NONBLOCK | O_CLOEXEC);
	if (fd < 0) {
		perror(argv[1]);
		return 1;
	}
	ret = ping(fd);
	close(fd);
	if (!ret)
		printf("%s echoed the payload\n", argv[1]);
	return ret;
}
