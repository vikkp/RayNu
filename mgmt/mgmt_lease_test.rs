use super::{lease_is_usable, load, load_usable, prefer_mac, store, ParkedMgmtLease};

fn sample() -> ParkedMgmtLease {
    ParkedMgmtLease {
        ip: [10, 99, 99, 121],
        prefix: 24,
        router: [10, 99, 99, 1],
        has_router: true,
        mac: [0xb0, 0x26, 0x28, 0x5c, 0x5a, 0x3a],
        port: 8443,
    }
}

#[test]
fn store_load_roundtrip() {
    store(sample());
    let got = load().expect("lease");
    assert_eq!(got, sample());
    assert!(lease_is_usable(&got));
}

#[test]
fn prefer_mac_zero_without_lease_and_matches_parked() {
    store(ParkedMgmtLease {
        ip: [0, 0, 0, 0],
        prefix: 24,
        router: [0; 4],
        has_router: false,
        mac: [0; 6],
        port: 8443,
    });
    assert!(load_usable().is_none());
    assert_eq!(prefer_mac(), [0; 6]);
    store(sample());
    assert_eq!(prefer_mac(), sample().mac);
    assert!(load_usable().is_some());
}

#[test]
fn unspecified_ip_is_not_usable() {
    let mut l = sample();
    l.ip = [0, 0, 0, 0];
    assert!(!lease_is_usable(&l));
    l = sample();
    l.prefix = 0;
    assert!(!lease_is_usable(&l));
    l = sample();
    l.mac = [0; 6];
    assert!(!lease_is_usable(&l));
}
