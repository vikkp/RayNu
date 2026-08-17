use super::{lease_is_usable, load, store, ParkedMgmtLease};

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
