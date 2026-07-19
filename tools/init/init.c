/*
 * Minimal static init for RayNu-V M3.10.
 * Writes RAYNU-V-M3-SHELL-OK to stdout (kernel console) then pauses.
 * Built with: gcc -static -nostdlib -o init init.c
 */
typedef long long i64;
typedef unsigned long long u64;

#define SYS_write 1
#define SYS_pause 34
#define SYS_exit 60

static long syscall3(long n, long a, long b, long c) {
    long ret;
    __asm__ volatile("syscall"
                     : "=a"(ret)
                     : "a"(n), "D"(a), "S"(b), "d"(c)
                     : "rcx", "r11", "memory");
    return ret;
}

void _start(void) {
    static const char msg[] = "RAYNU-V-M3-SHELL-OK\n";
    (void)syscall3(SYS_write, 1, (long)msg, (long)(sizeof(msg) - 1));
    for (;;) {
        (void)syscall3(SYS_pause, 0, 0, 0);
    }
    (void)syscall3(SYS_exit, 0, 0, 0);
}
