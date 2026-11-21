/* SPDX-License-Identifier: GPL-2.0-only */
ENTRY(_start)
MEMORY
{
#ifdef HIFI4
	iram (rx) : ORIGIN = 0x00400000, LENGTH = 0x10000
	ram (rw)  : ORIGIN = 0x4fc00000, LENGTH = 0x10000
#else
	ram (rwx) : ORIGIN = 0x4fa00000, LENGTH = 0x10000
#endif
}

PHDRS
{
	code PT_LOAD FLAGS(5);
	data PT_LOAD FLAGS(6);
}

SECTIONS
{
	.text : {
		KEEP(*(.text.boot))
		*(.literal .literal.*)
		*(.text .text.*)
		*(.rodata .rodata.* .srodata .srodata.*)
#ifdef HIFI4
	} > iram :code
#else
	} > ram :code
#endif

	.resource_table : ALIGN(8) {
		KEEP(*(.resource_table))
	} > ram :data

	.data : ALIGN(8) {
		*(.data .data.* .sdata .sdata.*)
	} > ram :data

	.bss (NOLOAD) : ALIGN(16) {
		*(.bss .bss.* .sbss .sbss.* COMMON)
		. = ALIGN(16);
		. += 2048;
		__stack_top = .;
	} > ram :data
}
