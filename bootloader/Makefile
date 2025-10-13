.PHONY: run

boot.bin: boot.asm
	nasm -fbin $< -o $@

boot: boot.bin
	qemu-system-x86_64 -drive format=raw,file=./$<
