/* SPDX-License-Identifier: GPL-2.0-only */
	.section .text.boot, "ax"
	.literal_position
	.global _start
	.balign 4
_start:
	/* Start with a single window and mask interrupts in the polling demo. */
	movi a2, 0
	wsr a2, windowbase
	movi a2, 1
	wsr a2, windowstart
	movi a2, 0x4000f
	wsr a2, ps
	rsync

	/* Data TLB spanning way: keep translation and bypass the cache for
	 * mailbox MMIO (region 0) and the shared DDR aperture (region 2).
	 */
	movi a3, 0
	movi a4, 0x40000000
	rdtlb1 a2, a3
	srli a2, a2, 4
	slli a2, a2, 4
	addi a2, a2, 2
	wdtlb a2, a3
	rdtlb1 a2, a4
	srli a2, a2, 4
	slli a2, a2, 4
	addi a2, a2, 2
	wdtlb a2, a4
	dsync
	movi a1, __stack_top
	call8 echo_run
1:	j 1b
