/* SPDX-License-Identifier: GPL-2.0-only */
	.section .text.boot, "ax"
	.global _start
	.balign 4
_start:
	/* Keep interrupts masked and invalidate caches before uncached IPC. */
	csrw mie, zero
	li t0, 0x30013
	csrs 0x7c2, t0	/* C906 MCOR */
	csrw 0x7c1, zero	/* C906 MHCR, cache enables */
	fence iorw, iorw
	la sp, __stack_top
	tail echo_run
