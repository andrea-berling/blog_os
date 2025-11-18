; boot.asm
bits 16
org 0x7C00
STAGE2_STACK_START equ 0x90000
STAGE2_ENTRYPOINT equ 0x0010000

jmp _start
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
; TODO: check if A20 is enabled before going to PE and long mode
; https://fancykillerpanda.github.io/OS-Tutorial/02_bootloader/a20-line/
; For now, YOLO

; real-mode stack before any interrupt pushing flags/CS:IP to the stack
_start:
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
mov cx, STAGE2_SECTORS      ; CX keeps track of sectors left to read
.read_sectors_loop:
; 1. Determine chunk size (cap at 127 to satisfy BIOS/DMA limits)
  mov ax, cx              ; Start with the assumption we'll read all remaining sectors
  cmp ax, 127
  jbe .set_dap_count      ; If AX <= 127, it's a valid size
  mov ax, 127             ; Otherwise, cap it at 127
.set_dap_count:
  mov [dap + 2], ax       ; Write the final, correct value to memory once
.read_sectors:
  ; 2. Perform the read
  mov ah, 0x42
  mov si, dap
  mov dl, [BootDrive]
  push cx
  call interrupt_with_retry
  pop cx

  ; 3. Update remaining count
  mov ax, [dap + 2]           ; Get the number of sectors we actually read
  sub cx, ax                  ; Subtract from total remaining
  jz .read_sectors_done       ; If 0 sectors left, we are finished

  ; 4. Advance LBA (Disk Offset)
  ; Since we are looping, we know we read exactly 127 sectors.
  add word [dap + 8],127      ; Add to lower 16 bits of LBA
  adc word [dap + 10],0        ; Propagate carry to next 16 bits (handles > 32MB locations)

  ; 5. Advance Buffer Address (Memory Offset)
  ; 127 sectors * 512 bytes = 65024 bytes = 0xFE00
  ; TODO: what if the sector size != 512?
  add word [dap + 4], 0xFE00  ; Advance Offset
  jnc .read_sectors_loop      ; If no overflow, continue
  add word [dap + 6], 0x1000  ; If overflow, add 4KB (0x1000 paragraphs) to Segment
                              ; 0x1000 * 16 = 65536 (64KB), effectively handling the carry
  jmp .read_sectors_loop

.read_sectors_done:

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
mov esp, STAGE2_STACK_START
; cdecl convention to pass parameters to main
push dword [ExtensionsBitmap]
push dword [EDDVersion]
push dword STAGE2_STACK_START
push dword KERNEL_SECTORS
push dword STAGE2_SECTORS
push dword DriveParameters
call STAGE2_ENTRYPOINT

align 8
; flat 0..4GiB code/data
gdt:
  dq 0                        ; null
  dq 0x00CF9A000000FFFF       ; code: base=0, limit=4GB, P=1, DPL=0, Code, R, 32-bit
  dq 0x00CF92000000FFFF       ; data: base=0, limit=4GB, P=1, DPL=0, Data, W, 32-bit
gdt_end:
; 6 bytes: len(gdt) - 1 (2 bytes), linear_addr(gdt) (4 bytes)
gdt_desc:
  dw gdt_end - gdt - 1
  dd gdt

; Disk Address Packet (EDD)
; size=16, reserved=0, blocks=BLOCKS, buffer=offset:segment, LBA=1
dap:
db 0x10
db 0x00
dw 0
; 0x1000*16 + 0x0000 = 0x0010000, the handover address
dw 0x0000
dw 0x1000
dq 1

BootDrive db 0
align 4
EDDVersion dd 0
ExtensionsBitmap dd 0
dw KERNEL_SECTORS
align 2
DriveParameters:
    dw 66       ; The size of this buffer structure. The BIOS reads this.
    resb 64     ; Reserve the remaining 64 bytes for the BIOS to fill.

times 510 - ($ - $$) db 0
dw 0xAA55
