// SPDX-License-Identifier: GPL-2.0-only
/*
 * echo.c
 * Created on Nov 20, 2026
 * Author: @POEG1726
 */

#include <stdbool.h>
#include <stdint.h>

#ifdef HIFI4
#define SHARED_BASE 0x4fb10000u
#define LOCAL_MBOX  0x01701000u
#define ARM_BANK    0u
#define ECHO_ADDR   2u
#define BARRIER()   __asm__ volatile("memw" ::: "memory")
#else
#define SHARED_BASE 0x4fa10000u
#define LOCAL_MBOX  0x0601f000u
#define ARM_BANK    1u
#define ECHO_ADDR   1u
#define BARRIER()   __asm__ volatile("fence iorw, iorw" ::: "memory")
#endif

#define ARM_MBOX        0x03003000u
#define MBOX_BANK_SIZE  0x100u
#define MBOX_RX_ENABLE  0x20u
#define MBOX_RX_PENDING 0x24u
#define MBOX_COUNT      0x60u
#define MBOX_FIFO       0x70u
#define MBOX_DEPTH      8u

#define RING_COUNT      16u
#define RING_ALIGN      0x1000u
#define TX_RING         SHARED_BASE
#define RX_RING         (SHARED_BASE + 0x2000u)
#define BUFFER_START    (SHARED_BASE + 0x4000u)
#define BUFFER_END      (BUFFER_START + 0x4000u)

#define DRIVER_OK       4u
#define DESC_WRITE      2u
#define NS_ADDR         53u

/* The remoteproc resource table contains one virtio RPMsg device. */
struct vring_resource {
	uint32_t da, align, num, notify, pa;
};

static volatile struct {
	uint32_t version, count, reserved[2], offset;
	uint32_t resource_type, device_id, notify_id, features, guest_features, config_len;
	uint8_t status, vring_count, padding[2];
	struct vring_resource rings[2];
} table __attribute__((section(".resource_table"), used)) = {
	.version = 1,
	.count = 1,
	.offset = 20,
	.resource_type = 3,
	.device_id = 7,
	.features = 1, /* RPMsg name service */
	.vring_count = 2,
	.rings = {
		{ TX_RING, RING_ALIGN, RING_COUNT, 0, 0 },
		{ RX_RING, RING_ALIGN, RING_COUNT, 1, 0 },
	},
};

struct descriptor {
	uint64_t addr;
	uint32_t len;
	uint16_t flags, next;
};

struct avail_ring {
	uint16_t flags, idx, entries[RING_COUNT];
};

struct used_entry {
	uint32_t id, len;
};

struct used_ring {
	uint16_t flags, idx;
	struct used_entry entries[RING_COUNT];
};

struct rpmsg_header {
	uint32_t src, dst, reserved;
	uint16_t len, flags;
	uint8_t data[];
};

struct name_service {
	char name[32];
	uint32_t addr, flags;
};

/* Linux rpmsg_char binds this service and creates /dev/rpmsgN. */
static const struct name_service announcement = { "rpmsg-raw", ECHO_ADDR, 0 };

static inline __attribute__((always_inline))
volatile uint32_t *reg(uintptr_t base, uint32_t offset)
{
	return (volatile uint32_t *)(base + offset);
}

/**
 * @brief Peek at an available descriptor after observing the producer index.
 */
static inline __attribute__((always_inline))
bool ring_peek(uintptr_t base, uint16_t tail, uint16_t *id)
{
	volatile const struct avail_ring *avail =
		(void *)(base + sizeof(struct descriptor) * RING_COUNT);

	if (tail == avail->idx)
		return false;
	BARRIER();
	*id = avail->entries[tail % RING_COUNT];
	return true;
}

/**
 * @brief Resolve a direct descriptor within this core's dedicated RPMsg pool.
 */
static inline __attribute__((always_inline))
volatile struct rpmsg_header *get_buffer(uintptr_t base, uint16_t id,
						uint16_t flags, uint32_t *size)
{
	volatile const struct descriptor *descs = (void *)base;
	uint64_t addr;
	uint32_t len;

	if (id >= RING_COUNT || descs[id].flags != flags)
		return 0;

	addr = descs[id].addr;
	len = descs[id].len;
	if (addr < BUFFER_START || addr >= BUFFER_END ||
	    (addr & 3u) || len < sizeof(struct rpmsg_header) ||
	    len > BUFFER_END - addr)
		return 0;

	*size = len;
	return (volatile struct rpmsg_header *)(uintptr_t)addr;
}

/**
 * @brief Publish one used entry before kicking the Linux virtio driver.
 */
static inline __attribute__((always_inline))
void complete(uintptr_t base, uint16_t id, uint32_t len)
{
	volatile struct used_ring *used = (void *)(base + RING_ALIGN);
	uint16_t idx = used->idx;

	used->entries[idx % RING_COUNT].id = id;
	used->entries[idx % RING_COUNT].len = len;
	BARRIER();
	used->idx = (uint16_t)(idx + 1);
}

static inline __attribute__((always_inline))
void kick_arm(void)
{
	uintptr_t mailbox = ARM_MBOX + ARM_BANK * MBOX_BANK_SIZE;

	BARRIER();
	/* A full FIFO already has a pending notification. Linux checks both rings. */
	if ((*reg(mailbox, MBOX_COUNT) & 0xfu) < MBOX_DEPTH)
		*reg(mailbox, MBOX_FIFO) = table.rings[0].notify;
}

/**
 * @brief Announce one RPMsg endpoint and echo host packets on dedicated rings.
 */
__attribute__((noreturn)) void echo_run(void)
{
	uint16_t tx_tail = 0, rx_tail = 0;
	bool announced = false;

	/* Channel 0 in the local ARM-facing bank. RPMsg is polled in the demo. */
	*reg(LOCAL_MBOX, MBOX_RX_ENABLE) |= 1u;
	for (;;) {
		volatile struct rpmsg_header *tx, *rx = 0;
		volatile const uint8_t *payload = (const uint8_t *)&announcement;
		uint32_t len = sizeof(announcement), destination = NS_ADDR;
		uint32_t tx_size, rx_size = 0;
		uint16_t tx_id, rx_id = 0;

		/* Drain the kick FIFO and acknowledge the channel's W1C status. */
		if ((*reg(LOCAL_MBOX, MBOX_COUNT) & 0xfu) ||
		    (*reg(LOCAL_MBOX, MBOX_RX_PENDING) & 1u)) {
			(void)*reg(LOCAL_MBOX, MBOX_FIFO);
			*reg(LOCAL_MBOX, MBOX_RX_PENDING) = 1u;
		}

		if (!(table.status & DRIVER_OK))
			continue;

		if (announced) {
			if (!ring_peek(RX_RING, rx_tail, &rx_id))
				continue;
			rx = get_buffer(RX_RING, rx_id, 0, &rx_size);
			if (!rx || rx->dst != ECHO_ADDR || rx->len > rx_size - sizeof(*rx)) {
				complete(RX_RING, rx_id, 0);
				rx_tail++;
				kick_arm();
				continue;
			}
			payload = rx->data;
			len = rx->len;
			destination = rx->src;
		}

		if (!ring_peek(TX_RING, tx_tail, &tx_id))
			continue;
		tx = get_buffer(TX_RING, tx_id, DESC_WRITE, &tx_size);
		tx_tail++;
		if (!tx || len > tx_size - sizeof(*tx)) {
			complete(TX_RING, tx_id, 0);
			kick_arm();
			continue;
		}

		tx->src = ECHO_ADDR;
		tx->dst = destination;
		tx->reserved = 0;
		tx->len = (uint16_t)len;
		tx->flags = 0;
		for (uint32_t i = 0; i < len; i++)
			tx->data[i] = payload[i];

		complete(TX_RING, tx_id, sizeof(*tx) + len);
		if (rx) {
			complete(RX_RING, rx_id, 0);
			rx_tail++;
		}
		announced = true;
		kick_arm();
	}
}
