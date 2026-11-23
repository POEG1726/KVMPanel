/*
 * journal.c
 * Created on Nov 22, 2026
 * Author: @POEG1726
 */

#include "journal.h"
#include "power.h"
#include "protocol.h"

#define SECTOR_SIZE 512
#define SLOT_SIZE   8
#define SLOT_COUNT  (SECTOR_SIZE / SLOT_SIZE)

/* The first six bytes are three complement pairs. Partial erasure cannot
 * transform a valid tuple into a different valid tuple. Byte 7 commits it. */
static uint8_t record[SLOT_SIZE];
static uint8_t tail[2], active, target, address_slot, index;
static uint8_t durable, mounted, checking_commit;
static uint16_t sequence;

/** @brief Locate the newest committed slot and skip any torn or occupied slots. */
uint8_t journal_init(void)
{
    uint8_t sector, slot, byte, occupied, valid, seen = 0;
    uint16_t candidate;
    index = SLOT_SIZE;
    durable = active = mounted = checking_commit = 0;
    sequence = 0;
    for (sector = 0; sector < 2; ++sector) {
        tail[sector] = 0;
        for (slot = 0; slot < SLOT_COUNT; ++slot) {
            occupied = 0;
            for (byte = 0; byte < SLOT_SIZE; ++byte) {
                if (!board_flash_read(sector * SECTOR_SIZE + slot * SLOT_SIZE + byte,
                                      &record[byte]))
                    return 0;
                occupied |= record[byte] != 0xff;
            }
            if (occupied)
                tail[sector] = slot + 1;
            valid = record[7] == 0 && record[2] <= 1 &&
                (uint8_t)(record[0] ^ record[3]) == 0xff &&
                (uint8_t)(record[1] ^ record[4]) == 0xff &&
                (uint8_t)(record[2] ^ record[5]) == 0xff &&
                record[6] == stc_crc8(stc_crc8(stc_crc8(0, record[0]), record[1]), record[2]);
            if (!valid)
                continue;
            candidate = record[0] | ((uint16_t)record[1] << 8);
            if (!seen || (int16_t)(candidate - sequence) > 0) {
                seen = 1;
                sequence = candidate;
                durable = record[2];
                active = sector;
            }
        }
    }
    mounted = 1;
    return 1;
}

/** @brief Return the recovered or most recently committed flag. */
uint8_t journal_flag(void)
{
    return mounted && durable;
}

/** @brief Append a changed flag. Erase only the sector without the newest record. */
uint8_t journal_step(uint8_t flag, uint8_t allow_erase)
{
    uint16_t address, next;
    uint8_t readback;
    if (!mounted)
        return SAVE_FAILED;
    if (checking_commit) {
        address = target * SECTOR_SIZE + address_slot * SLOT_SIZE + 7;
        if (!board_flash_read(address, &readback))
            return SAVE_FAILED;
        checking_commit = 0;
        index = SLOT_SIZE;
        if (readback == 0) {
            sequence++;
            durable = record[2];
            active = target;
        } else
            return SAVE_FAILED;
    }
    if (index != SLOT_SIZE && record[2] != flag)
        index = SLOT_SIZE; /* Abandon a superseded, uncommitted slot. */
    if (index == SLOT_SIZE) {
        if (flag == durable)
            return SAVE_DONE;
        target = active;
        if (tail[target] == SLOT_COUNT) {
            target ^= 1;
            if (tail[target]) {
                if (!allow_erase)
                    return SAVE_BUSY;
                if (!board_flash_erase(target * SECTOR_SIZE))
                    return SAVE_FAILED;
                tail[target] = 0;
                return SAVE_BUSY;
            }
        }
        address_slot = tail[target]++;
        next = sequence + 1;
        record[0] = (uint8_t)next;
        record[1] = (uint8_t)(next >> 8);
        record[2] = flag;
        record[3] = (uint8_t)~record[0];
        record[4] = (uint8_t)~record[1];
        record[5] = (uint8_t)~flag;
        record[6] = stc_crc8(stc_crc8(stc_crc8(0, record[0]), record[1]), flag);
        record[7] = 0;
        index = 0;
    }

    address = target * SECTOR_SIZE + address_slot * SLOT_SIZE + index;
    if (index == SLOT_SIZE - 1) {
        /* Even a failed program command can leave a complete marker. Check it
         * before considering any new flag or assigning the next sequence. */
        checking_commit = 1;
        board_flash_write(address, 0);
        return SAVE_BUSY;
    }
    if (!board_flash_write(address, record[index]) ||
        !board_flash_read(address, &readback) || readback != record[index]) {
        index = SLOT_SIZE;
        return SAVE_FAILED;
    }
    ++index;
    return SAVE_BUSY;
}
