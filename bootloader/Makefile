.PHONY: boot boot_usb

boot.bin: stage1/boot.asm
	nasm -fbin $< -o $@

boot:
	qemu-system-x86_64 -drive format=raw,file=../disk.img -serial stdio -cpu qemu64,+pdpe1gb

boot_usb:
	qemu-system-x86_64 -drive if=none,id=usbstick,format=raw,file=../disk.img -device usb-ehci,id=ehci  -device usb-storage,bus=ehci.0,drive=usbstick -serial stdio -cpu qemu64,+pdpe1gb
