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

; ---- get extensions via EDD (AH=41h)
mov ah, 0x41
mov bx, 0x55aa
mov dl, [BootDrive]
call interrupt_with_retry
mov [ExtensionsBitmap], cx
mov [EDDVersion], ah

; ---- get drive parameters via EDD (AH=48h)
mov ah, 0x48
mov dl, [BootDrive]
mov si, DriveParameters ; ---- addres for the result
call interrupt_with_retry

; ---- read stage2 via EDD (AH=42h) ----
mov ah, 0x42
mov si, dap
mov dl, [BootDrive]
call interrupt_with_retry

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
; cdecl convention to pass parameters to main
push dword [ExtensionsBitmap]
push dword [EDDVersion]
push dword DriveParameters
call 0x0010000

; precondition: ah contains the desired interrupt code
; precondition: all the other argumetns (e.g. DS,SI) are already set
interrupt_with_retry:
 int 0x13
 jc .soft_fail
 jmp short .ok
.soft_fail:
 push ax
 xor ax, ax          ; AH=0: reset disk
 int 0x13
 jc .hard_fail
 pop ax
 jmp interrupt_with_retry
.hard_fail:
 hlt
.ok:
 ret

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
align 4
EDDVersion dd 0
ExtensionsBitmap dd 0
align 2
DriveParameters dw 66

times 510 - ($ - $$) db 0
dw 0xAA55
