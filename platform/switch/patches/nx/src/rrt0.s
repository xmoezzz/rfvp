.section .text.jmp, "ax", %progbits
.align 2

.global _start
_start:
	b __nx_rrt0_entry
	.word __module_header - _start
	.ascii "HOMEBREW"
