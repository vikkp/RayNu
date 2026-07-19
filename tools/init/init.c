/*
 * Minimal static init for RayNu-V M3.10.
 *
 * HV latches SHELL from COM1 OUT on I/O VMEXIT. Prefer /dev/kmsg (printk →
 * polling console) so we do not depend on ttyS0's IRQ-driven TX. Also try
 * ttyS0/console; HV injects COM1 THR IRQ for that path.
 *
 * Built with: gcc -static -nostdlib -o init init.c
 */
#define SYS_write 1
#define SYS_openat 257
#define SYS_close 3
#define SYS_mknodat 259
#define SYS_mkdir 83
#define SYS_pause 34
#define SYS_exit 60

#define AT_FDCWD (-100)
#define O_RDWR 2
#define O_WRONLY 1
#define S_IFCHR 0x2000
#define TTYS0_DEV 0x440 /* makedev(4, 64) */
#define KMSG_DEV 0x10b  /* makedev(1, 11) */

static const char msg[] = "RAYNU-V-M3-SHELL-OK\n";
static const char path_kmsg[] = "/dev/kmsg";
static const char path_console[] = "/dev/console";
static const char path_ttys0[] = "/dev/ttyS0";
static const char path_dev[] = "/dev";

static long syscall3(long n, long a, long b, long c) {
    long ret;
    __asm__ volatile("syscall"
                     : "=a"(ret)
                     : "a"(n), "D"(a), "S"(b), "d"(c)
                     : "rcx", "r11", "memory");
    return ret;
}

static long syscall4(long n, long a, long b, long c, long d) {
    long ret;
    register long r10 __asm__("r10") = d;
    __asm__ volatile("syscall"
                     : "=a"(ret)
                     : "a"(n), "D"(a), "S"(b), "d"(c), "r"(r10)
                     : "rcx", "r11", "memory");
    return ret;
}

static void write_fd(long fd) {
    if (fd < 0)
        return;
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

static void ensure_node(const char *path, long dev) {
    long fd = syscall3(SYS_openat, AT_FDCWD, (long)path, O_WRONLY);
    if (fd >= 0) {
        (void)syscall3(SYS_close, fd, 0, 0);
        return;
    }
    (void)syscall3(SYS_mkdir, (long)path_dev, 0755, 0);
    (void)syscall4(SYS_mknodat, AT_FDCWD, (long)path, S_IFCHR | 0666, dev);
}

void _start(void) {
    ensure_node(path_kmsg, KMSG_DEV);
    ensure_node(path_ttys0, TTYS0_DEV);
    for (int round = 0; round < 8; round++) {
        write_path(path_kmsg);
        write_fd(1);
        write_fd(2);
        write_path(path_console);
        write_path(path_ttys0);
    }
    for (;;) {
        (void)syscall3(SYS_pause, 0, 0, 0);
    }
    (void)syscall3(SYS_exit, 0, 0, 0);
}
