//! Physical link state of both BCM5720 LOM ports (ADR-021 wave 2, D5/D6).
//!
//! Pillar: [Z] [D] [A]
//! Proven Core: **outside** (ADR-002).
//! VERIFICATION: L1 host tests. Host/CI never print an iron marker.
//!
//! One table, two rows: function 0 (the SPA port, LOM 3) and function 1
//! (the guest LAN port, LOM 4). The iron sampler in `bcm5720_mmio` reads
//! each PHY's `BMSR` at most twice a second and calls [`observe`]. A flip
//! of `up` after the first sample is a [`LinkChange`]; the caller writes
//! the audit event and the COM2 line. The SPA reads [`snapshot`] through
//! the `/perc/guests` body (`"link":[...]`), and virtio-net asks
//! [`guest_carrier`] for the `VIRTIO_NET_S_LINK_UP` bit.
//!
//! Before a Start arms function 1, the firmware still owns that PHY and
//! may hold it without link even with a cable in (iron `99f003c1`:
//! pre-EBS `bmsr=7949`, link only after the arm restarted AN). The row
//! says so (`driven=false`), and the guest carrier stays up until the
//! port is driven, so the first frame can arm it. Reporting that firmware
//! state as "no carrier" would stop the guest from ever sending the frame
//! that arms the port.

use core::sync::atomic::{AtomicBool, Ordering};

/// Rows in the table: function 0 and function 1.
pub const PORT_COUNT: usize = 2;
/// LOM label of function 0 on the R640 NDC (ports 1–2 are the X710s).
pub const LOM_OF_FUNC0: u8 = 3;

/// Poll each PHY no more often than this (TSC at the 2.1 GHz fallback is
/// about 500 ms). Two MDIO reads per port per poll.
pub const LINK_POLL_TSC: u64 = 1_050_000_000;

/// What one PHY read said.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkSample {
    /// `BMSR_LSTATUS` after the second (unlatched) read.
    pub up: bool,
    /// Negotiated speed in Mbit/s when up, else 0.
    pub mbps: u16,
    pub full_duplex: bool,
    pub bmsr: u16,
    /// True once RayNu-V drives the port (SPA listen or egress arm).
    pub driven: bool,
}

/// One LOM port row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortLink {
    /// PCI function on the BCM5720.
    pub func: u8,
    /// Station MAC read from the chip; zeros until the first sample.
    pub mac: [u8; 6],
    /// At least one sample has landed.
    pub seen: bool,
    pub driven: bool,
    pub up: bool,
    pub mbps: u16,
    pub full_duplex: bool,
    pub bmsr: u16,
    /// `up` flips after the first sample.
    pub changes: u32,
}

impl PortLink {
    const fn empty(func: u8) -> Self {
        Self {
            func,
            mac: [0; 6],
            seen: false,
            driven: false,
            up: false,
            mbps: 0,
            full_duplex: false,
            bmsr: 0,
            changes: 0,
        }
    }

    /// LOM number printed on the chassis and in iDRAC.
    pub fn lom(&self) -> u8 {
        LOM_OF_FUNC0.saturating_add(self.func)
    }

    /// Short role name for the page and COM2.
    pub fn role(&self) -> &'static str {
        if self.func == 0 {
            "spa"
        } else {
            "guest-lan"
        }
    }
}

/// `up` flipped on a port that already had a sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkChange {
    pub func: u8,
    pub up: bool,
    pub mbps: u16,
}

/// Two rows behind one spinlock.
///
/// JUSTIFICATION (global state): one chassis, two LOM ports, read from
/// the HTTP handler and written from the poll tick. Host tests reset it.
struct Table(core::cell::UnsafeCell<[PortLink; PORT_COUNT]>);

// SAFETY: exclusive access is enforced by `TABLE_LOCK` in `with_table`.
// KANI-TARGET: observe / guest_carrier (host).
unsafe impl Sync for Table {}

static TABLE: Table = Table(core::cell::UnsafeCell::new([
    PortLink::empty(0),
    PortLink::empty(1),
]));
static TABLE_LOCK: AtomicBool = AtomicBool::new(false);

fn with_table<R>(f: impl FnOnce(&mut [PortLink; PORT_COUNT]) -> R) -> R {
    while TABLE_LOCK.swap(true, Ordering::Acquire) {
        core::hint::spin_loop();
    }
    // SAFETY: lock held; exclusive mutable access to the two rows.
    let out = unsafe { f(&mut *TABLE.0.get()) };
    TABLE_LOCK.store(false, Ordering::Release);
    out
}

/// Whether a poll at `now` is due. First poll at once (`last == 0`), then
/// at most once per [`LINK_POLL_TSC`]. Unlike the old egress tick this
/// keeps polling after the link is up, so a dark cable is seen.
pub fn poll_due(last: u64, now: u64) -> bool {
    last == 0 || now.wrapping_sub(last) >= LINK_POLL_TSC
}

/// Record one sample for `func`. Returns the change when `up` flipped on a
/// row that already had a sample. The first sample never reports a change.
///
/// INVARIANTS:
/// - `func >= PORT_COUNT` is ignored and returns `None`
/// - `changes` grows by exactly one per returned `Some`
/// - `mac` is kept unless the sample carries a non-zero one
pub fn observe(func: u8, mac: [u8; 6], s: LinkSample) -> Option<LinkChange> {
    let i = usize::from(func);
    if i >= PORT_COUNT {
        return None;
    }
    with_table(|t| {
        let row = &mut t[i];
        let flipped = row.seen && row.up != s.up;
        if mac != [0; 6] {
            row.mac = mac;
        }
        row.seen = true;
        row.driven = s.driven;
        row.up = s.up;
        row.mbps = if s.up { s.mbps } else { 0 };
        row.full_duplex = s.up && s.full_duplex;
        row.bmsr = s.bmsr;
        if flipped {
            row.changes = row.changes.saturating_add(1);
            Some(LinkChange {
                func,
                up: s.up,
                mbps: row.mbps,
            })
        } else {
            None
        }
    })
}

/// Both rows as they are now.
pub fn snapshot() -> [PortLink; PORT_COUNT] {
    with_table(|t| *t)
}

/// `VIRTIO_NET_S_LINK_UP` for the guest NIC: the function-1 wire once
/// RayNu-V drives it, else up (so the guest's first frame can arm it).
///
/// INVARIANTS:
/// - Not driven → true
/// - Driven → the last sampled `up`
pub fn guest_carrier() -> bool {
    carrier_rule(&snapshot()[1])
}

/// The rule behind [`guest_carrier`], on one row.
pub fn carrier_rule(row: &PortLink) -> bool {
    !(row.seen && row.driven) || row.up
}

/// `,"link":[{...},{...}]` for the `/perc/guests` body. Writes nothing
/// when no row has a sample yet (host, QEMU), so older bodies are unchanged.
pub fn write_json(out: &mut dyn JsonSink) {
    let rows = snapshot();
    if !rows.iter().any(|r| r.seen) {
        return;
    }
    out.s(",\"link\":[");
    for (i, r) in rows.iter().enumerate() {
        if i > 0 {
            out.s(",");
        }
        out.s("{\"lom\":");
        out.u(u64::from(r.lom()));
        out.s(",\"func\":");
        out.u(u64::from(r.func));
        out.s(",\"role\":\"");
        out.s(r.role());
        out.s("\",\"mac\":\"");
        write_mac(out, r.mac);
        out.s("\",\"seen\":");
        out.s(if r.seen { "true" } else { "false" });
        out.s(",\"driven\":");
        out.s(if r.driven { "true" } else { "false" });
        out.s(",\"up\":");
        out.s(if r.up { "true" } else { "false" });
        out.s(",\"mbps\":");
        out.u(u64::from(r.mbps));
        out.s(",\"duplex\":\"");
        out.s(if !r.up {
            "-"
        } else if r.full_duplex {
            "full"
        } else {
            "half"
        });
        out.s("\",\"changes\":");
        out.u(u64::from(r.changes));
        out.s("}");
    }
    out.s("]");
}

/// Minimal sink so this module does not depend on the catalog's buffer type.
pub trait JsonSink {
    fn s(&mut self, t: &str);
    fn u(&mut self, v: u64);
}

fn write_mac(out: &mut dyn JsonSink, mac: [u8; 6]) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for (i, b) in mac.iter().enumerate() {
        if i > 0 {
            out.s(":");
        }
        let s = [HEX[(b >> 4) as usize], HEX[(b & 0xf) as usize]];
        out.s(core::str::from_utf8(&s).unwrap_or("00"));
    }
}

/// Host tests start from an empty table.
#[cfg(test)]
pub fn reset_for_test() {
    with_table(|t| {
        *t = [PortLink::empty(0), PortLink::empty(1)];
    });
}

#[cfg(test)]
mod host_link_test {
    use super::*;

    struct Buf(std::string::String);
    impl JsonSink for Buf {
        fn s(&mut self, t: &str) {
            self.0.push_str(t);
        }
        fn u(&mut self, v: u64) {
            self.0.push_str(&v.to_string());
        }
    }

    const SPA: [u8; 6] = [0xb0, 0x26, 0x28, 0x5c, 0x5a, 0x38];
    const LAN: [u8; 6] = [0xb0, 0x26, 0x28, 0x5c, 0x5a, 0x39];

    fn up(driven: bool) -> LinkSample {
        LinkSample {
            up: true,
            mbps: 1000,
            full_duplex: true,
            bmsr: 0x796d,
            driven,
        }
    }

    fn down(driven: bool) -> LinkSample {
        LinkSample {
            up: false,
            mbps: 1000,
            full_duplex: true,
            bmsr: 0x7949,
            driven,
        }
    }

    #[test]
    fn link_table_reports_flips_and_feeds_the_guest_carrier() {
        reset_for_test();

        // Empty: nothing written, carrier up (nothing says otherwise).
        let mut b = Buf(String::new());
        write_json(&mut b);
        assert_eq!(b.0, "");
        assert!(guest_carrier());
        assert!(!poll_due(5, 5 + LINK_POLL_TSC - 1));
        assert!(poll_due(0, 0));
        assert!(poll_due(5, 5 + LINK_POLL_TSC));

        // First samples never report a change.
        assert_eq!(observe(0, SPA, up(true)), None);
        assert_eq!(observe(1, LAN, down(false)), None);
        let rows = snapshot();
        assert_eq!(rows[0].lom(), 3);
        assert_eq!(rows[1].lom(), 4);
        assert_eq!(rows[0].role(), "spa");
        assert_eq!(rows[1].role(), "guest-lan");
        assert!(rows[0].up && rows[0].mbps == 1000 && rows[0].full_duplex);
        assert!(!rows[1].up && rows[1].mbps == 0 && !rows[1].full_duplex);
        // Function 1 not driven yet: the guest still sees carrier.
        assert!(guest_carrier());

        // The arm drives the port and the wire is dark: carrier drops, one change
        // because `up` did not flip (down → down) is NOT counted.
        assert_eq!(observe(1, [0; 6], down(true)), None);
        assert!(!guest_carrier());
        assert_eq!(snapshot()[1].mac, LAN, "zero MAC keeps the old one");

        // Cable in: flip to up with speed.
        assert_eq!(
            observe(1, LAN, up(true)),
            Some(LinkChange {
                func: 1,
                up: true,
                mbps: 1000
            })
        );
        assert!(guest_carrier());
        // Cable out: flip to down, speed 0 in the change.
        assert_eq!(
            observe(1, LAN, down(true)),
            Some(LinkChange {
                func: 1,
                up: false,
                mbps: 0
            })
        );
        assert!(!guest_carrier());
        assert_eq!(snapshot()[1].changes, 2);
        assert_eq!(snapshot()[0].changes, 0);

        // SPA port flip is reported too.
        assert_eq!(
            observe(0, SPA, down(true)),
            Some(LinkChange {
                func: 0,
                up: false,
                mbps: 0
            })
        );
        // Out of range is ignored.
        assert_eq!(observe(2, SPA, up(true)), None);

        let mut b = Buf(String::new());
        write_json(&mut b);
        let j = b.0;
        assert!(j.starts_with(",\"link\":["), "{j}");
        assert!(j.contains("{\"lom\":3,\"func\":0,\"role\":\"spa\",\"mac\":\"b0:26:28:5c:5a:38\",\"seen\":true,\"driven\":true,\"up\":false,\"mbps\":0,\"duplex\":\"-\",\"changes\":1}"), "{j}");
        assert!(j.contains("{\"lom\":4,\"func\":1,\"role\":\"guest-lan\",\"mac\":\"b0:26:28:5c:5a:39\",\"seen\":true,\"driven\":true,\"up\":false,\"mbps\":0,\"duplex\":\"-\",\"changes\":2}"), "{j}");
        assert!(j.ends_with("]"), "{j}");

        assert_eq!(observe(1, LAN, up(true)).map(|c| c.up), Some(true));
        let mut b = Buf(String::new());
        write_json(&mut b);
        assert!(b.0.contains("\"up\":true,\"mbps\":1000,\"duplex\":\"full\",\"changes\":3}"), "{}", b.0);

        // The carrier rule on its own rows.
        let mut r = PortLink::empty(1);
        assert!(carrier_rule(&r));
        r.seen = true;
        assert!(carrier_rule(&r), "seen but firmware-held: still carrier");
        r.driven = true;
        assert!(!carrier_rule(&r));
        r.up = true;
        assert!(carrier_rule(&r));
        reset_for_test();
    }

    #[test]
    fn link_change_mirror_is_not_hushed_under_a_guest() {
        // Iron `8df4407d`: the page row and the guest carrier moved, the
        // `boot: D5 link` lines reached the host ring, but no
        // `RAYNU-V-AUDIT: HostLinkChanged` did — `write_str` is silent
        // once Linux owns the console. The mirror must use the no-wait path.
        let arm = include_str!("../audit/integrity.rs")
            .split_once("AuditEvent::HostLinkChanged { func, up, mbps } => {")
            .expect("mirror arm")
            .1
            .split_once("AuditEvent::FrameAllocated")
            .expect("end of arm")
            .0;
        assert!(arm.contains("write_str_nowait(\"RAYNU-V-AUDIT: HostLinkChanged func=\")"));
        assert!(arm.contains("write_u32_nowait("));
        assert!(arm.contains("write_byte_nowait(b'\\n')"));
        assert!(!arm.contains("serial::write_str("), "{arm}");
        assert!(!arm.contains("serial::write_byte("), "{arm}");
    }
}
