; boot.asm
bits 16
org 0x7C00

; TODO: check if A20 is enabled before going to PE and long mode
; https://fancykillerpanda.github.io/OS-Tutorial/02_bootloader/a20-line/
; For now, YOLO

; real-mode stack before any interrupt pushing flags/CS:IP to the stack
xor ax, ax
mov ss, ax
mov sp, 0x7C00

mov ax, cs
mov ds, ax

; save boot drive from BIOS (already in DL)
mov [BootDrive], dl

; ---- read stage2 via EDD (AH=42h) ----
mov si, dap
mov dl, [BootDrive]

.read_retry:
 mov ah, 0x42
 int 0x13
 jc .read_fail
 jmp short .read_ok
.read_fail:
 xor ax, ax          ; AH=0: reset disk
 int 0x13
 jc .hard_fail
 jmp .read_retry
.hard_fail:
 hlt                 ; you may want a tiny "error" loop here
.read_ok:

; enter protected mode
cli
lgdt [gdt_desc]
mov eax, cr0
or  eax, 1
mov cr0, eax
; reload the CS according to the GDT code descriptor (index 1x8)
; entry 0 is null
jmp 0x08:pm_entry

bits 32
pm_entry:
mov ax, 0x10                  ; data selector
mov ds, ax
mov es, ax
mov ss, ax
mov fs, ax
mov gs, ax
mov esp, 0x90000
jmp dword 0x0010000

align 8
; flat 0..4GiB code/data
gdt:
  dq 0                        ; null
  dq 0x00CF9A000000FFFF       ; code: base=0, limit=4GB, P=1, DPL=0, Code, R, 32-bit
  dq 0x00CF92000000FFFF       ; data: base=0, limit=4GB, P=1, DPL=0, Data, W, 32-bit
gdt_desc:
  dw gdt_end - gdt - 1
  dd gdt
gdt_end:

; Disk Address Packet (EDD)
; size=16, reserved=0, blocks=BLOCKS, buffer=offset:segment, LBA=1
dap:
db 0x10
db 0x00
dw STAGE2_SECTORS
; 0x1000*16 + 0x0000 = 0x0010000, the handover address
dw 0x0000
dw 0x1000
dq 1

BootDrive db 0

times 510 - ($ - $$) db 0
dw 0xAA55
