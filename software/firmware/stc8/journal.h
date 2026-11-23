/*
 * journal.h
 * Created on Nov 22, 2026
 * Author: @POEG1726
 */

#ifndef _JOURNAL_H_
#define _JOURNAL_H_

#include <stdint.h>

enum { SAVE_BUSY, SAVE_DONE, SAVE_FAILED };

/** @brief Scan the two Data Flash sectors and recover the latest flag. */
uint8_t journal_init(void);

/** @brief Return one only when the durable flag is one. */
uint8_t journal_flag(void);

/** @brief Append the requested flag, performing at most one erase or byte write. */
uint8_t journal_step(uint8_t flag, uint8_t allow_erase);

#endif // _JOURNAL_H_
