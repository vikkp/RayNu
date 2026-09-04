//! Product-ISO Alpine serial auto-answer (outside Proven Core).
//!
//! Pillar: [Z]
//! Proven Core: **outside** (ADR-002 / ADR-014)
//! VERIFICATION: L1 (runtime + host tests)
//!
//! Watches guest COM1 TX for Alpine installer prompts and queues replies
//! into RBR. Lab UART stub never calls this. Host/CI never prints
//! `ISO-INSTALL-OK`.

use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

const WIN: usize = 24;
const QCAP: usize = 448;
const YES_MAX: u8 = 4;

const LOGIN: &[u8] = b"login:";
const SHELL: &[u8] = b"~# ";
/// BusyBox ash when cwd is `/` (Alpine live overlay).
const SHELL_ROOT: &[u8] = b"/ # ";
const GRUB: &[u8] = b"GNU GRUB";
const YESN: &[u8] = b"(y/n)";
const YESN_UP: &[u8] = b"(y/N)";
/// alpine-conf `confirm_erase` (older ISOs): `[y/N]: ` not `(y/n)`.
const YESN_BRACK: &[u8] = b"[y/N]";
const YESN_BRACK_LOW: &[u8] = b"[y/n]";
const YESN_BRACK_YN: &[u8] = b"[Y/n]";
const YESN_BRACK_YY: &[u8] = b"[Y/N]";
/// Alpine `setup-disk` bootloader picker when `BOOTLOADER` is unset.
const BOOTLOADER_Q: &[u8] = b"bootloader?";
/// `ask_disk` when `-m sys /dev/vda` did not stick (no virtio yet).
const DISK_Q: &[u8] = b"Which disk";
/// alpine-conf `How would you like to use $it_them? ('sys', 'data' or 'lvm')`
/// when `-m sys` did not stick. Must not match `Which disk(s) would you like
/// to use?` (that one already queued `/dev/vda`).
const USE_Q: &[u8] = b"How would you like";
/// alpine-conf when virtio-blk is not visible; next `(y/n)` is boot-media.
const NODISK: &[u8] = b"No disks available";

pub(crate) const ROOT: &[u8] = b"root\r";
/// Alpine UEFI `setup-disk` needs `USE_EFI=1` (wiki / alpine-conf) or it
/// tries syslinux/MBR and can stall before a GPT write. `-s 0` skips swap
/// so the first write is ESP+Linux FS (partition-table detect). After live
/// boot, nlplug often mounts virtio-ISO at `/media/vdb` (not `/media/cdrom`).
/// Remounting busy `/dev/vdb` fails with Resource busy and leaves apk pointed
/// at an empty `/media/cdrom/apks`. Discover an existing `/media/*/apks`
/// first (ISO media layout is bare `…/apks`; apk appends arch →
/// `apks/x86_64/APKINDEX.tar.gz`. Mirror layout with a `main` component is
/// wrong — nested `4536b72` found nothing and clobbered live repos; nested
/// `1b48e26` bare `…/apks` read the index and only missed packages absent
/// from alpine-virt on-media, `dosfstools`/`grub-efi`). Only fall back to
/// `mkdir` + `mount -t iso9660` `/dev/vdb`||`/dev/sr0` when none is found.
/// BusyBox ash requires spaces after `{` and before `}` — `{mkdir` parses
/// as a name and dies with `unexpected "}"` (nested `50ed61c`). Write
/// `/etc/apk/repositories` only when `$R` is a real directory (do not
/// overwrite live-init's working line if discovery + mount both fail). Do
/// not append — a network mirror blocks without DHCP. `modprobe -a` +
/// `mdev -s` so `/sys/block/vda` exists before `setup-disk` `find_disks`
/// (otherwise `No disks available` exits after we answer n). Wait until
/// `/dev/vda` exists (`mdev -s` each second, up to 5s) so a slow virtio
/// probe is a block device before `setup-disk` opens it — `mdev` then
/// `sleep 1` left the node missing if the driver bound during the sleep.
/// `sr_mod` so `/dev/sr0` exists when the live image booted from
/// virtio-iso. `isofs` + `mount -t iso9660` so BusyBox does not probe a
/// virtio-blk ISO as a disk (iso9660 is absent from `/proc/filesystems`
/// until the module loads). auto-answer / # without login (3.21 emergency
/// shell has no getty). alpine-conf `find_efi_size` defaults ESP to
/// 160 MiB (512-byte FAT32 min ~34 MiB). That is larger than a 64 MiB
/// fallback disk and leaves ~96 MiB root on the 256 MiB iron disk.
/// `BOOT_SIZE=48` is above the FAT32 floor so GPT+ESP still land
/// (ISO-INSTALL-OK is the partition table, not a finished apk root). Do
/// not `apk update` first — that can hang on a missing index and never
/// reach `setup-disk`. setup-disk before apk update. Nested proof for
/// `USE_EFI=1 BOOTLOADER=grub` needs an ISO whose on-media repo ships
/// `grub-efi` + `dosfstools` (alpine-standard / alpine-extended); alpine-virt
/// lacks them. Cruzer ESP still stages alpine-virt (~63 MiB) until larger
/// media.
pub(crate) const SETUP: &[u8] =
    b"modprobe -a virtio_pci virtio_blk sr_mod isofs;for i in 0 1 2 3 4;do mdev -s;[ -b /dev/vda ]&&break;sleep 1;done;R=;for d in /media/*/apks;do [ -d $d ]&&R=$d&&break;done;[ $R ]||{ mkdir -p /media/cdrom;mount -t iso9660 /dev/vdb /media/cdrom||mount -t iso9660 /dev/sr0 /media/cdrom;R=/media/cdrom/apks; };[ -d $R ]&&echo $R>/etc/apk/repositories;ERASE_DISKS=/dev/vda BOOTLOADER=grub USE_EFI=1 BOOT_SIZE=48 setup-disk -m sys -s 0 /dev/vda\r";
const _: () = assert!(SETUP.len() <= QCAP);
/// alpine-virt 3.21 `/init` emergency shell has busybox + modprobe, **no**
/// `setup-disk`. If `/media/*/apks` already exists, `exit` so `/init`
/// `find_boot_repositories` can switch_root to live alpine (getty `login:`
/// then SETUP). Else mount ISO then exit. Avoid remounting busy vdb.
/// emergency mount+exit.
pub(crate) const MOUNT_EXIT: &[u8] =
    b"modprobe -a virtio_pci virtio_blk sr_mod isofs;mdev -s;for d in /media/*/apks;do [ -d $d ]&&exit;done;mkdir -p /media/cdrom;mount -t iso9660 /dev/vdb /media/cdrom||mount -t iso9660 /dev/sr0 /media/cdrom;exit\r";
const _: () = assert!(MOUNT_EXIT.len() <= QCAP);
pub(crate) const YES: &[u8] = b"y\r";
pub(crate) const NO: &[u8] = b"n\r";
pub(crate) const DISK: &[u8] = b"/dev/vda\r";
pub(crate) const SYS: &[u8] = b"sys\r";
pub(crate) const GRUB_ENTER: &[u8] = b"\r";
pub(crate) const BOOTLOADER: &[u8] = b"grub\r";

const PHASE_LOGIN: u8 = 0;
const PHASE_SHELL: u8 = 1;
const PHASE_CONFIRM: u8 = 2;
const PHASE_DONE: u8 = 3;

struct Answer {
    win: [u8; WIN],
    wlen: usize,
    q: [u8; QCAP],
    qh: usize,
    qn: usize,
}

impl Answer {
    const fn empty() -> Self {
        Self {
            win: [0; WIN],
            wlen: 0,
            q: [0; QCAP],
            qh: 0,
            qn: 0,
        }
    }
}

struct Box(core::cell::UnsafeCell<Answer>);
// SAFETY: exclusive access is enforced by `LOCK`.
// KANI-TARGET: guest-UEFI serial auto-answer mutex (outside Proven Core).
unsafe impl Sync for Box {}

static STATE: Box = Box(core::cell::UnsafeCell::new(Answer::empty()));
static LOCK: AtomicBool = AtomicBool::new(false);
static PHASE: AtomicU8 = AtomicU8::new(PHASE_LOGIN);
static YES_LEFT: AtomicU8 = AtomicU8::new(YES_MAX);
static GRUB_SENT: AtomicBool = AtomicBool::new(false);
/// Next `(y/n)` is "Try boot media?" after `No disks available` — answer n.
static NEXT_YES_IS_NO: AtomicBool = AtomicBool::new(false);
/// Initramfs `/ # ` already queued [`MOUNT_EXIT`]. Stay PHASE_LOGIN.
static MOUNT_SENT: AtomicBool = AtomicBool::new(false);

fn with<R>(f: impl FnOnce(&mut Answer) -> R) -> R {
    while LOCK.swap(true, Ordering::Acquire) {
        core::hint::spin_loop();
    }
    // SAFETY: lock held; exclusive mutable access.
    // KANI-TARGET: guest-UEFI serial auto-answer mutex (outside Proven Core).
    let out = unsafe { f(&mut *STATE.0.get()) };
    LOCK.store(false, Ordering::Release);
    out
}

pub fn reset() {
    with(|a| *a = Answer::empty());
    PHASE.store(PHASE_LOGIN, Ordering::Release);
    YES_LEFT.store(YES_MAX, Ordering::Release);
    GRUB_SENT.store(false, Ordering::Release);
    NEXT_YES_IS_NO.store(false, Ordering::Release);
    MOUNT_SENT.store(false, Ordering::Release);
}

fn ends_with(win: &[u8], wlen: usize, needle: &[u8]) -> bool {
    if needle.is_empty() || wlen < needle.len() {
        return false;
    }
    &win[wlen - needle.len()..wlen] == needle
}

fn is_yes_prompt(win: &[u8], wlen: usize) -> bool {
    ends_with(win, wlen, YESN)
        || ends_with(win, wlen, YESN_UP)
        || ends_with(win, wlen, YESN_BRACK)
        || ends_with(win, wlen, YESN_BRACK_LOW)
        || ends_with(win, wlen, YESN_BRACK_YN)
        || ends_with(win, wlen, YESN_BRACK_YY)
}

/// alpine-virt 3.21 `/init` `recovery_shell` is already root (`/ # `)
/// with no getty `login:`. mkinitfs has no `setup-disk`. auto-answer
/// `/ # ` without login queues [`MOUNT_EXIT`] and stays PHASE_LOGIN so
/// later getty `login:` still matches. `~# ` without login is live overlay
/// (SETUP). emergency mount+exit.
fn is_shell_prompt(win: &[u8], wlen: usize) -> bool {
    ends_with(win, wlen, SHELL) || ends_with(win, wlen, SHELL_ROOT)
}

fn enqueue(a: &mut Answer, bytes: &[u8]) {
    for &b in bytes {
        if a.qn >= QCAP {
            break;
        }
        let i = (a.qh + a.qn) % QCAP;
        a.q[i] = b;
        a.qn += 1;
    }
}

/// Observe one guest COM1 THR byte. May queue a reply for RBR.
pub fn note_tx(b: u8) {
    let phase = PHASE.load(Ordering::Acquire);
    if phase == PHASE_DONE {
        return;
    }
    with(|a| {
        if a.wlen < WIN {
            a.win[a.wlen] = b;
            a.wlen += 1;
        } else {
            a.win.copy_within(1..WIN, 0);
            a.win[WIN - 1] = b;
        }
        match phase {
            PHASE_LOGIN if !GRUB_SENT.load(Ordering::Acquire) && ends_with(&a.win, a.wlen, GRUB) => {
                enqueue(a, GRUB_ENTER);
                GRUB_SENT.store(true, Ordering::Release);
            }
            PHASE_LOGIN if ends_with(&a.win, a.wlen, LOGIN) => {
                enqueue(a, ROOT);
                PHASE.store(PHASE_SHELL, Ordering::Release);
            }
            PHASE_LOGIN if ends_with(&a.win, a.wlen, SHELL_ROOT) => {
                if !MOUNT_SENT.swap(true, Ordering::AcqRel) {
                    enqueue(a, MOUNT_EXIT);
                }
                // Stay PHASE_LOGIN so later getty `login:` still matches.
            }
            PHASE_LOGIN if ends_with(&a.win, a.wlen, SHELL) => {
                enqueue(a, SETUP);
                PHASE.store(PHASE_CONFIRM, Ordering::Release);
            }
            PHASE_SHELL if is_shell_prompt(&a.win, a.wlen) => {
                enqueue(a, SETUP);
                PHASE.store(PHASE_CONFIRM, Ordering::Release);
            }
            PHASE_CONFIRM if ends_with(&a.win, a.wlen, NODISK) => {
                NEXT_YES_IS_NO.store(true, Ordering::Release);
            }
            PHASE_CONFIRM if is_yes_prompt(&a.win, a.wlen) => {
                if NEXT_YES_IS_NO.swap(false, Ordering::AcqRel) {
                    enqueue(a, NO);
                } else {
                    let left = YES_LEFT.load(Ordering::Acquire);
                    if left > 0 {
                        enqueue(a, YES);
                        YES_LEFT.store(left - 1, Ordering::Release);
                    }
                }
                // Stay in CONFIRM so a later `bootloader?` still matches.
            }
            PHASE_CONFIRM if ends_with(&a.win, a.wlen, DISK_Q) => {
                enqueue(a, DISK);
            }
            PHASE_CONFIRM if ends_with(&a.win, a.wlen, USE_Q) => {
                enqueue(a, SYS);
                // Stay in CONFIRM so a later `bootloader?` / `Which disk` still matches.
            }
            PHASE_CONFIRM if ends_with(&a.win, a.wlen, BOOTLOADER_Q) => {
                enqueue(a, BOOTLOADER);
                PHASE.store(PHASE_DONE, Ordering::Release);
            }
            _ => {}
        }
    });
}

/// Pop one queued reply byte.
pub fn take_rx() -> Option<u8> {
    with(|a| {
        if a.qn == 0 {
            return None;
        }
        let b = a.q[a.qh];
        a.qh = (a.qh + 1) % QCAP;
        a.qn -= 1;
        Some(b)
    })
}

/// Host tests / gate: queued reply length.
pub fn queued() -> usize {
    with(|a| a.qn)
}

#[cfg(test)]
#[path = "guest_serial_answer_test.rs"]
mod guest_serial_answer_test;
