/*
 * Minimal static init for RayNu-V M3.10.
 *
 * The hypervisor latches SHELL from COM1 OUT bytes (port 0x3f8), not from
 * a host-side stdout pipe. Tiny bring-up images may not have fd 1 wired to
 * ttyS0 yet, so try every path: write(1/2), /dev/console, /dev/ttyS0, then
 * iopl(3)+outb as a last resort.
 *
 * Built with: gcc -static -nostdlib -o init init.c
 */
#define SYS_write 1
#define SYS_openat 257
#define SYS_close 3
#define SYS_iopl 172
#define SYS_pause 34
#define SYS_exit 60

#define AT_FDCWD (-100)
#define O_RDWR 2
#define O_WRONLY 1

static const char msg[] = "RAYNU-V-M3-SHELL-OK\n";

static long syscall3(long n, long a, long b, long c) {
    long ret;
    __asm__ volatile("syscall"
                     : "=a"(ret)
                     : "a"(n), "D"(a), "S"(b), "d"(c)
                     : "rcx", "r11", "memory");
    return ret;
}

static void write_fd(long fd) {
    (void)syscall3(SYS_write, fd, (long)msg, (long)(sizeof(msg) - 1));
}

static void write_path(const char *path) {
    long fd = syscall3(SYS_openat, AT_FDCWD, (long)path, O_RDWR);
    if (fd < 0)
        fd = syscall3(SYS_openat, AT_FDCWD, (long)path, O_WRONLY);
    if (fd >= 0) {
        write_fd(fd);
        (void)syscall3(SYS_close, fd, 0, 0);
    }
}

static void com1_outb(void) {
    /* Only OUT after iopl(3); bare OUT without IOPL is #GP and can kill us. */
    if (syscall3(SYS_iopl, 3, 0, 0) != 0)
        return;
    for (unsigned i = 0; i < sizeof(msg) - 1; i++) {
        unsigned char b = (unsigned char)msg[i];
        __asm__ volatile("outb %0, %1" : : "a"(b), "Nd"((unsigned short)0x3f8));
    }
}

void _start(void) {
    /* Repeat: printk/IRQ noise can interrupt a single latch attempt. */
    for (int round = 0; round < 3; round++) {
        write_fd(1);
        write_fd(2);
        write_path("/dev/console");
        write_path("/dev/ttyS0");
        com1_outb();
    }
    for (;;) {
        (void)syscall3(SYS_pause, 0, 0, 0);
    }
    (void)syscall3(SYS_exit, 0, 0, 0);
}
