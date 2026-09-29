//! SPA choice for a latched spare image (outside Proven Core).
//!
//! When the 8 GiB Alpine image on RAYNU-SPARE is already present, the
//! firmware does not launch the guest until the operator picks one path
//! in the SPA: boot that Alpine as it is, clean-reinstall Alpine onto
//! that window, or place a larger disk in the free tail. An unattended
//! boot does not run `setup-disk`. UBUNTU0 is not a target. `vdc` stays
//! hidden. A tail disk does not write the installed window.
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002). The HTTPS listener is armed before
//! this wait; the PERC walk itself is earlier and cannot see the browser.

use core::sync::atomic::{AtomicU8, Ordering};

use super::api::RestMethod;

/// `GET` — current choice. Waiting is true only while the image latch is set
/// and no button has been posted.
pub const PERC_BOOT_STATUS_PATH: &str = "/perc/boot";
/// `POST` — steady-state boot of the installed Alpine. Does not arm a wipe.
pub const PERC_BOOT_INSTALLED_PATH: &str = "/perc/boot/installed";
/// `POST` — clean reinstall of Alpine onto the 8 GiB spare window.
pub const PERC_BOOT_REINSTALL_PATH: &str = "/perc/boot/reinstall";

/// No SPA post yet.
pub const CHOICE_PENDING: u8 = 0;
/// Boot the installed image. `AuditEvent::PercSpaChoice.choice`.
pub const CHOICE_BOOT: u8 = 1;
/// Clean reinstall. `AuditEvent::PercSpaChoice.choice`.
pub const CHOICE_REINSTALL: u8 = 2;
/// New disk in the free tail. `AuditEvent::PercSpaChoice.choice`.
pub const CHOICE_TAIL: u8 = 3;
/// Boot the disk already placed in the free tail. `setup-disk` stays withheld.
pub const CHOICE_TAIL_BOOT: u8 = 4;

/// COM2 while the firmware waits. Not an iron marker.
pub const PERC_SPA_WAIT_NOTE: &str =
    "boot: perc image present — waiting for SPA Guests (start the installed guest, linux_iso in the free tail, or linux_iso inside the 8 GiB window; not ISO-INSTALL-OK)";
/// COM2 after Guests Start.
pub const PERC_SPA_BOOT_NOTE: &str =
    "boot: perc SPA choice boot installed (steady state; setup-disk withheld; not ISO-INSTALL-OK)";
/// COM2 after Guests Create with linux_iso inside the window.
pub const PERC_SPA_REINSTALL_NOTE: &str =
    "boot: perc SPA choice clean reinstall (ISO on the 8 GiB spare window; UBUNTU0 untouched; not ISO-INSTALL-OK)";
/// COM2 after Guests Create with a disk in the free tail.
pub const PERC_SPA_TAIL_NOTE: &str =
    "boot: perc SPA choice tail disk (free tail after the 8 GiB window; that window is not vda; UBUNTU0 untouched; not ISO-INSTALL-OK)";
/// COM2 after Start on the free-tail guest.
pub const PERC_SPA_TAIL_BOOT_NOTE: &str =
    "boot: perc SPA choice tail boot (free tail is vda; setup-disk withheld; 8 GiB window stays; not ISO-INSTALL-OK)";

/// JUSTIFICATION: one BSP latch. The coexist HTTP handler stores it; the
/// boot wait and the RayNu-F stager load it. Not the Proven Core allocator.
static CHOICE: AtomicU8 = AtomicU8::new(CHOICE_PENDING);
/// Guest-visible size of a tail disk. Zero unless a tail choice won the race.
static TAIL_BYTES: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
/// Spare byte offset of that disk. Zero unless it is the free tail.
static TAIL_OFF: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
/// SPA Stop. The RayNu-F exit loop takes it and leaves the guest.
static GUEST_STOP: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
/// Stop already landed. Start stays refused until the next hypervisor boot.
static GUEST_STOPPED: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// What the operator has posted, if anything.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PercSpaChoice {
    Pending,
    BootInstalled,
    CleanReinstall,
    /// `disk_bytes` is the guest disk. It starts at the free tail.
    /// This choice stages the installer.
    Tail {
        disk_bytes: u64,
    },
    /// Same geometry as [`PercSpaChoice::Tail`]. The installed ESP boots.
    TailBoot {
        disk_bytes: u64,
    },
}

/// Result of one authenticated POST.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PercChoiceApply {
    Accepted,
    /// The spare image latch is clear. Do not invent a wipe target.
    NoImage,
    /// A choice was already posted this boot.
    AlreadyChosen,
}

/// REST outcome for the three `/perc/boot` paths. `NotMine` means another route.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PercChoiceHttp {
    NotMine,
    Unauthorized,
    BadMethod,
    Status(&'static [u8]),
    Accepted(&'static [u8], u8),
    NoImage,
    AlreadyChosen,
}

/// Current latch. Host tests start at [`PercSpaChoice::Pending`].
pub fn perc_spa_choice() -> PercSpaChoice {
    match CHOICE.load(Ordering::Acquire) {
        CHOICE_BOOT => PercSpaChoice::BootInstalled,
        CHOICE_REINSTALL => PercSpaChoice::CleanReinstall,
        CHOICE_TAIL => tail_choice(false),
        CHOICE_TAIL_BOOT => tail_choice(true),
        _ => PercSpaChoice::Pending,
    }
}

fn tail_choice(boot: bool) -> PercSpaChoice {
    let disk_bytes = TAIL_BYTES.load(Ordering::Acquire);
    let spare_off = TAIL_OFF.load(Ordering::Acquire);
    if disk_bytes > crate::mgmt::megaraid::PERC_IMAGE_BYTES
        && spare_off >= crate::mgmt::megaraid::PERC_IMAGE_BYTES
    {
        if boot {
            PercSpaChoice::TailBoot { disk_bytes }
        } else {
            PercSpaChoice::Tail { disk_bytes }
        }
    } else {
        PercSpaChoice::Pending
    }
}

/// `(spare_off, disk_bytes)` when a tail disk is the guest disk.
///
/// `None` for boot-installed and for a window reinstall. The offset is
/// never inside the 8 GiB window. True for an install and for a later Start.
pub fn armed_tail() -> Option<(u64, u64)> {
    match perc_spa_choice() {
        PercSpaChoice::Tail { disk_bytes } | PercSpaChoice::TailBoot { disk_bytes } => {
            Some((TAIL_OFF.load(Ordering::Acquire), disk_bytes))
        }
        _ => None,
    }
}

/// ISO staging for a window reinstall or a new tail disk. Boot-as-is and
/// Start of an existing tail disk are false.
pub fn installer_iso_forced() -> bool {
    matches!(
        perc_spa_choice(),
        PercSpaChoice::CleanReinstall | PercSpaChoice::Tail { .. }
    )
}

/// `setup-disk` may erase the tail view only while that disk is being installed.
pub fn tail_install_chosen() -> bool {
    matches!(perc_spa_choice(), PercSpaChoice::Tail { .. })
}

/// True only after the reinstall POST.
pub fn clean_reinstall_chosen() -> bool {
    matches!(perc_spa_choice(), PercSpaChoice::CleanReinstall)
}

/// The firmware waits only when the spare image is the install disk and
/// the operator has not picked a path. Host and QEMU never latch the image.
pub fn perc_spa_wait_required(image_latched: bool, choice: PercSpaChoice) -> bool {
    image_latched && matches!(choice, PercSpaChoice::Pending)
}

/// Steady-state disk boot. A clean reinstall never prefers the installed ESP.
pub fn prefer_installed_disk(would_boot_disk: bool, clean_reinstall: bool) -> bool {
    would_boot_disk && !clean_reinstall
}

/// Stage the product ISO. A clean reinstall stages it even when `EFI PART`
/// was seen. Otherwise the existing fail-safe stands.
pub fn stage_installer_iso(
    disk_entry_missing: bool,
    sticky: bool,
    pin: bool,
    iso_forbidden: bool,
    clean_reinstall: bool,
) -> bool {
    if clean_reinstall {
        return true;
    }
    disk_entry_missing && !sticky && !pin && !iso_forbidden
}

/// Which staged image becomes the guest entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StagedPick {
    Disk,
    Iso,
    Fallback,
}

/// Clean reinstall uses the ISO even if a disk entry was also staged.
/// Steady state prefers the disk.
pub fn pick_staged_entry(disk_some: bool, iso_some: bool, clean_reinstall: bool) -> StagedPick {
    if clean_reinstall {
        return if iso_some {
            StagedPick::Iso
        } else {
            StagedPick::Fallback
        };
    }
    if disk_some {
        StagedPick::Disk
    } else if iso_some {
        StagedPick::Iso
    } else {
        StagedPick::Fallback
    }
}

/// `setup-disk` may erase the latched 8 GiB window only for an explicit
/// reinstall. A reinstall flag without the image latch does not widen the
/// wipe onto USB or UBUNTU0. Boot-as-is keeps the existing `EFI PART` refusal.
pub fn spare_reinstall_wipe_allowed(image_latched: bool, clean_reinstall: bool) -> bool {
    image_latched && clean_reinstall
}

/// Store one choice. A second post does not replace the first.
/// Does not call `request_from_spa` — the HTTP layer does that on accept
/// so a host test of this function cannot arm an installer wipe.
pub fn apply_spa_choice(image_latched: bool, choice: u8) -> PercChoiceApply {
    if choice != CHOICE_BOOT && choice != CHOICE_REINSTALL {
        return PercChoiceApply::AlreadyChosen;
    }
    if !image_latched {
        return PercChoiceApply::NoImage;
    }
    match CHOICE.compare_exchange(CHOICE_PENDING, choice, Ordering::AcqRel, Ordering::Acquire) {
        Ok(_) => PercChoiceApply::Accepted,
        Err(_) => PercChoiceApply::AlreadyChosen,
    }
}

/// Store a free-tail disk. A second post does not replace the first.
///
/// Refuses a size that [`crate::mgmt::megaraid::place_in_free_tail`] would
/// not place, so the installed window cannot become this disk.
pub fn apply_tail_choice(
    image_latched: bool,
    spare_bytes: u64,
    disk_bytes: u64,
) -> PercChoiceApply {
    let Some(place) = crate::mgmt::megaraid::place_in_free_tail(
        spare_bytes,
        crate::mgmt::megaraid::PERC_IMAGE_BYTES,
        image_latched,
        disk_bytes,
    ) else {
        return PercChoiceApply::NoImage;
    };
    match CHOICE.compare_exchange(
        CHOICE_PENDING,
        CHOICE_TAIL,
        Ordering::AcqRel,
        Ordering::Acquire,
    ) {
        Ok(_) => {
            TAIL_OFF.store(place.spare_off, Ordering::Release);
            TAIL_BYTES.store(place.disk_bytes, Ordering::Release);
            PercChoiceApply::Accepted
        }
        Err(_) => PercChoiceApply::AlreadyChosen,
    }
}

/// Start the disk already in the free tail. Same placement rules as an install.
/// Does not stage the ISO and does not allow `setup-disk`.
pub fn apply_tail_boot_choice(
    image_latched: bool,
    spare_bytes: u64,
    disk_bytes: u64,
) -> PercChoiceApply {
    if guest_stopped() {
        return PercChoiceApply::AlreadyChosen;
    }
    let Some(place) = crate::mgmt::megaraid::place_in_free_tail(
        spare_bytes,
        crate::mgmt::megaraid::PERC_IMAGE_BYTES,
        image_latched,
        disk_bytes,
    ) else {
        return PercChoiceApply::NoImage;
    };
    match CHOICE.compare_exchange(
        CHOICE_PENDING,
        CHOICE_TAIL_BOOT,
        Ordering::AcqRel,
        Ordering::Acquire,
    ) {
        Ok(_) => {
            TAIL_OFF.store(place.spare_off, Ordering::Release);
            TAIL_BYTES.store(place.disk_bytes, Ordering::Release);
            PercChoiceApply::Accepted
        }
        Err(_) => PercChoiceApply::AlreadyChosen,
    }
}

/// Arm Stop for the running guest. Pending and a second Stop are idle.
pub fn request_guest_stop() -> bool {
    if guest_stopped() || matches!(perc_spa_choice(), PercSpaChoice::Pending) {
        return false;
    }
    GUEST_STOP.store(true, Ordering::Release);
    true
}

/// RayNu-F exit loop. One-shot. Also latches [`guest_stopped`].
pub fn take_guest_stop() -> bool {
    if !GUEST_STOP.swap(false, Ordering::AcqRel) {
        return false;
    }
    GUEST_STOPPED.store(true, Ordering::Release);
    true
}

/// True after Stop has landed. Start stays refused for this hypervisor boot.
pub fn guest_stopped() -> bool {
    GUEST_STOPPED.load(Ordering::Acquire)
}

fn status_body() -> &'static [u8] {
    match (
        crate::mgmt::megaraid::perc_image_boot_latched(),
        perc_spa_choice(),
    ) {
        (true, PercSpaChoice::Pending) => b"{\"choice\":\"pending\",\"waiting\":true}",
        (_, PercSpaChoice::BootInstalled) => b"{\"choice\":\"boot\",\"waiting\":false}",
        (_, PercSpaChoice::CleanReinstall) => b"{\"choice\":\"reinstall\",\"waiting\":false}",
        (_, PercSpaChoice::Tail { .. }) => b"{\"choice\":\"tail\",\"waiting\":false}",
        (_, PercSpaChoice::TailBoot { .. }) => b"{\"choice\":\"tailboot\",\"waiting\":false}",
        (false, PercSpaChoice::Pending) => b"{\"choice\":\"pending\",\"waiting\":false}",
    }
}

fn accept_body(choice: u8) -> &'static [u8] {
    if choice == CHOICE_REINSTALL {
        b"{\"ok\":true,\"choice\":\"reinstall\"}"
    } else {
        b"{\"ok\":true,\"choice\":\"boot\"}"
    }
}

/// Authenticated SPA routes for the spare-image choice.
pub fn perc_choice_rest(method: RestMethod, path: &str, authed: bool) -> PercChoiceHttp {
    let mine = path == PERC_BOOT_STATUS_PATH
        || path == PERC_BOOT_INSTALLED_PATH
        || path == PERC_BOOT_REINSTALL_PATH;
    if !mine {
        return PercChoiceHttp::NotMine;
    }
    if !authed {
        return PercChoiceHttp::Unauthorized;
    }
    if path == PERC_BOOT_STATUS_PATH {
        return if matches!(method, RestMethod::Get) {
            PercChoiceHttp::Status(status_body())
        } else {
            PercChoiceHttp::BadMethod
        };
    }
    if !matches!(method, RestMethod::Post) {
        return PercChoiceHttp::BadMethod;
    }
    let choice = if path == PERC_BOOT_REINSTALL_PATH {
        CHOICE_REINSTALL
    } else {
        CHOICE_BOOT
    };
    match apply_spa_choice(crate::mgmt::megaraid::perc_image_boot_latched(), choice) {
        PercChoiceApply::Accepted => {
            if choice == CHOICE_REINSTALL {
                crate::boot::raynu_f_flag::request_from_spa();
            } else {
                crate::boot::raynu_f_flag::request_installed_boot();
            }
            PercChoiceHttp::Accepted(accept_body(choice), choice)
        }
        PercChoiceApply::NoImage => PercChoiceHttp::NoImage,
        PercChoiceApply::AlreadyChosen => PercChoiceHttp::AlreadyChosen,
    }
}

/// After the standing SPA is armed and before RayNu-F, block while the
/// spare image is latched. Returns when Overview posts a choice. No timeout
/// into an install. Host and QEMU return immediately.
pub fn wait_perc_spa_choice_if_latched() {
    if !perc_spa_wait_required(
        crate::mgmt::megaraid::perc_image_boot_latched(),
        perc_spa_choice(),
    ) {
        return;
    }
    #[cfg(feature = "uefi-bin")]
    firmware_wait();
}

#[cfg(feature = "uefi-bin")]
fn firmware_wait() {
    use crate::boot::serial;
    serial::write_line(PERC_SPA_WAIT_NOTE);
    if !crate::mgmt::host_nic_listen::coexist_session_armed() {
        serial::write_line(
            "boot: WARN — perc SPA wait has no HTTPS listener; still waiting (will not install or boot)",
        );
    }
    let hz = {
        let h = crate::boot::raynu_f_flag::tsc_hz();
        if h == 0 {
            crate::mgmt::xhci::USB_TSC_HZ_FALLBACK
        } else {
            h
        }
    };
    let beat = crate::mgmt::xhci::usb_tsc_ticks_for_ms(hz, 60_000);
    let mut last = crate::arch::cpu::rdtsc();
    let mut t_s: u32 = 0;
    loop {
        if !perc_spa_wait_required(
            crate::mgmt::megaraid::perc_image_boot_latched(),
            perc_spa_choice(),
        ) {
            break;
        }
        crate::mgmt::maybe_tick_standing_spa();
        core::hint::spin_loop();
        let now = crate::arch::cpu::rdtsc();
        if now.wrapping_sub(last) >= beat {
            last = now;
            t_s = t_s.saturating_add(60);
            serial::write_str_nowait("boot: perc SPA wait alive t_s=");
            let mut buf = [0u8; 20];
            let s = crate::boot::raynu_f_flag::fmt_dec(u64::from(t_s), &mut buf);
            serial::write_str_nowait(s);
            serial::write_line_nowait(" (SPA ticking; not ISO-INSTALL-OK)");
        }
    }
    match perc_spa_choice() {
        PercSpaChoice::BootInstalled => serial::write_line(PERC_SPA_BOOT_NOTE),
        PercSpaChoice::CleanReinstall => serial::write_line(PERC_SPA_REINSTALL_NOTE),
        PercSpaChoice::Tail { .. } => serial::write_line(PERC_SPA_TAIL_NOTE),
        PercSpaChoice::TailBoot { .. } => serial::write_line(PERC_SPA_TAIL_BOOT_NOTE),
        PercSpaChoice::Pending => {}
    }
}

/// Host package: both Overview actions, the wait, and the wipe override
/// are in tree. Host tests do not print an iron marker.
pub fn prop_perc_spa_choice() -> bool {
    let http = include_str!("http.rs");
    let html = include_str!("../assets/webui.html");
    let main = include_str!("../src/main.rs");
    let answer = include_str!("../devices/guest_serial_answer.rs");
    let guest = include_str!("../vmx/guest_uefi.rs");
    http.contains("perc_choice_rest")
        && http.contains("PercSpaChoice")
        && html.contains("spare-list")
        && html.contains("/perc/guests/1/start")
        && !html.contains(PERC_BOOT_INSTALLED_PATH)
        && main.contains("wait_perc_spa_choice_if_latched()")
        && answer.contains("spare_reinstall_wipe_allowed")
        && guest.contains("prefer_installed_disk")
        && guest.contains("stage_installer_iso")
        && guest.contains("pick_staged_entry")
        && !include_str!("perc_boot_choice.rs").contains("println!(\"RAYNU-V-M8")
}

/// Host tests only.
#[cfg(test)]
pub fn clear_perc_spa_choice_for_test() {
    CHOICE.store(CHOICE_PENDING, Ordering::Release);
    TAIL_BYTES.store(0, Ordering::Release);
    TAIL_OFF.store(0, Ordering::Release);
    GUEST_STOP.store(false, Ordering::Release);
    GUEST_STOPPED.store(false, Ordering::Release);
}

#[cfg(test)]
mod perc_boot_choice_test {
    use super::*;
    use crate::mgmt::http::{handle_http_request, HTTP_RESPONSE_CAP};
    use crate::mgmt::iso::IsoDeployPlan;
    use crate::mgmt::iso_install::InstallToDiskPlan;
    use crate::mgmt::{ImageTable, VmTable};

    #[test]
    fn wait_and_boot_source_predicates() {
        assert!(!perc_spa_wait_required(false, PercSpaChoice::Pending));
        assert!(perc_spa_wait_required(true, PercSpaChoice::Pending));
        assert!(!perc_spa_wait_required(true, PercSpaChoice::BootInstalled));
        assert!(!perc_spa_wait_required(true, PercSpaChoice::CleanReinstall));
        assert!(!perc_spa_wait_required(
            true,
            PercSpaChoice::Tail {
                disk_bytes: 10_240 * 1024 * 1024
            }
        ));
        assert!(prefer_installed_disk(true, false));
        assert!(!prefer_installed_disk(true, true));
        assert!(!prefer_installed_disk(false, false));
        assert!(!stage_installer_iso(true, true, false, true, false));
        assert!(stage_installer_iso(true, false, false, false, false));
        assert!(!stage_installer_iso(false, false, false, false, false));
        assert!(stage_installer_iso(false, true, true, true, true));
        assert_eq!(pick_staged_entry(true, true, false), StagedPick::Disk);
        assert_eq!(pick_staged_entry(true, true, true), StagedPick::Iso);
        assert_eq!(pick_staged_entry(false, false, true), StagedPick::Fallback);
        assert!(spare_reinstall_wipe_allowed(true, true));
        assert!(!spare_reinstall_wipe_allowed(true, false));
        assert!(!spare_reinstall_wipe_allowed(false, true));
        assert!(prop_perc_spa_choice());
    }

    fn exchange(raw: &str) -> (u16, String) {
        let mut table = VmTable::new();
        let mut images = ImageTable::new();
        let mut iso_plan = IsoDeployPlan::empty();
        let mut iso_install = InstallToDiskPlan::empty();
        let mut out = [0u8; HTTP_RESPONSE_CAP];
        let n = handle_http_request(
            &mut table,
            &mut images,
            &mut iso_plan,
            &mut iso_install,
            raw,
            &mut out,
        )
        .unwrap_or(0);
        let s = core::str::from_utf8(&out[..n]).unwrap_or("").to_string();
        let status = s
            .split_whitespace()
            .nth(1)
            .unwrap_or("0")
            .parse()
            .unwrap_or(0);
        (status, s)
    }

    #[test]
    fn http_status_and_posts_need_auth_and_a_latched_image() {
        clear_perc_spa_choice_for_test();
        assert_eq!(
            apply_spa_choice(false, CHOICE_BOOT),
            PercChoiceApply::NoImage
        );
        assert_eq!(
            apply_spa_choice(true, CHOICE_BOOT),
            PercChoiceApply::Accepted
        );
        assert_eq!(perc_spa_choice(), PercSpaChoice::BootInstalled);
        assert_eq!(
            apply_spa_choice(true, CHOICE_REINSTALL),
            PercChoiceApply::AlreadyChosen
        );
        clear_perc_spa_choice_for_test();
        assert_eq!(
            apply_spa_choice(true, CHOICE_REINSTALL),
            PercChoiceApply::Accepted
        );
        assert!(clean_reinstall_chosen());
        clear_perc_spa_choice_for_test();
        assert!(!clean_reinstall_chosen());
        clear_perc_spa_choice_for_test();
        let ten = 10_240u64 * 1024 * 1024;
        assert_eq!(
            apply_tail_choice(false, crate::mgmt::megaraid::IRON_LD1_BYTES, ten),
            PercChoiceApply::NoImage
        );
        assert_eq!(
            apply_tail_choice(true, crate::mgmt::megaraid::IRON_LD1_BYTES, ten),
            PercChoiceApply::Accepted
        );
        assert!(installer_iso_forced());
        assert!(!clean_reinstall_chosen());
        assert!(!spare_reinstall_wipe_allowed(
            true,
            clean_reinstall_chosen()
        ));
        assert_eq!(
            armed_tail(),
            Some((crate::mgmt::megaraid::PERC_IMAGE_BYTES, ten))
        );
        assert_eq!(
            apply_spa_choice(true, CHOICE_BOOT),
            PercChoiceApply::AlreadyChosen
        );
        clear_perc_spa_choice_for_test();
        assert!(armed_tail().is_none());
        assert!(!installer_iso_forced());
        assert_eq!(
            apply_tail_boot_choice(true, crate::mgmt::megaraid::IRON_LD1_BYTES, ten),
            PercChoiceApply::Accepted
        );
        assert!(!installer_iso_forced());
        assert!(!tail_install_chosen());
        assert_eq!(
            armed_tail(),
            Some((crate::mgmt::megaraid::PERC_IMAGE_BYTES, ten))
        );
        assert!(request_guest_stop());
        assert!(take_guest_stop());
        assert!(guest_stopped());
        assert!(!request_guest_stop());
        clear_perc_spa_choice_for_test();
        assert!(!guest_stopped());
        assert!(!installer_iso_forced());
        let (st, body) = exchange("GET /perc/boot HTTP/1.1\r\n\r\n");
        assert_eq!(st, 401, "{body}");
        let (st, body) =
            exchange("GET /perc/boot HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n");
        assert_eq!(st, 200, "{body}");
        assert!(body.contains("\"waiting\":false"), "{body}");
        assert!(body.contains("\"choice\":\"pending\""), "{body}");
        let (st, body) = exchange(
            "POST /perc/boot/reinstall HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        );
        assert_eq!(st, 409, "{body}");
        assert!(body.contains("noimage"), "{body}");
        assert!(!clean_reinstall_chosen());
        let (st, body) = exchange(
            "POST /perc/boot/installed HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        );
        assert_eq!(st, 409, "{body}");
        assert_eq!(perc_spa_choice(), PercSpaChoice::Pending);
        clear_perc_spa_choice_for_test();
    }
}
