use super::*;

fn cpu(id: u32, package: u32, core: u32, thread: u32, bsp: bool) -> CpuInfo {
    CpuInfo {
        apic_id: id,
        package,
        core,
        thread,
        enabled: true,
        healthy: true,
        bsp,
    }
}

#[test]
fn max_cores_matches_units() {
    assert_eq!(MAX_CORES, crate::guest::slot::MAX_UNITS);
}

#[test]
fn sixteen_threads_on_eight_cores() {
    let mut cpus = Vec::new();
    for core in 0..8u32 {
        cpus.push(cpu(core * 2, 0, core, 0, core == 0));
        cpus.push(cpu(core * 2 + 1, 0, core, 1, false));
    }
    let mut out = [0u32; MAX_CORES];
    let n = select_cores(&cpus, true, &mut out);
    assert_eq!(n, 8);
    assert_eq!(out[0], 0);
    for i in 1..8 {
        assert_eq!(out[i], (i as u32) * 2);
    }
}

#[test]
fn bsp_not_thread_zero_is_first() {
    let cpus = [
        cpu(1, 0, 0, 0, false),
        cpu(8, 0, 0, 1, true),
        cpu(2, 0, 1, 0, false),
    ];
    let mut out = [0u32; MAX_CORES];
    let n = select_cores(&cpus, true, &mut out);
    assert_eq!(n, 2);
    assert_eq!(out[0], 8);
    assert_eq!(out[1], 2);
}

#[test]
fn disabled_cpu_is_skipped() {
    let mut off = cpu(4, 0, 1, 0, false);
    off.enabled = false;
    let cpus = [cpu(0, 0, 0, 0, true), off];
    let mut out = [0u32; MAX_CORES];
    assert_eq!(select_cores(&cpus, true, &mut out), 1);
    assert_eq!(out[0], 0);
}

#[test]
fn forty_cores_cap_at_sixteen() {
    let mut cpus = Vec::new();
    for core in 0..40u32 {
        cpus.push(cpu(core, 0, core, 0, core == 0));
    }
    let mut out = [0u32; MAX_CORES];
    let n = select_cores(&cpus, true, &mut out);
    assert_eq!(n, 16);
    assert_eq!(out[0], 0);
    assert_eq!(out[15], 15);
}

#[test]
fn xapic_id_300_is_skipped() {
    let cpus = [cpu(0, 0, 0, 0, true), cpu(300, 0, 1, 0, false)];
    let mut out = [0u32; MAX_CORES];
    assert_eq!(select_cores(&cpus, false, &mut out), 1);
    assert_eq!(out[0], 0);
    assert_eq!(select_cores(&cpus, true, &mut out), 2);
}

#[test]
fn core_of_rsp_at_block_edges() {
    let base = 0x10_0000_0000u64;
    let n = 3usize;
    assert_eq!(core_of_rsp(base - 1, base, n), 0);
    assert_eq!(core_of_rsp(base, base, n), 1);
    assert_eq!(core_of_rsp(base + BLOCK_BYTES - 1, base, n), 1);
    assert_eq!(core_of_rsp(base + BLOCK_BYTES, base, n), 2);
    assert_eq!(core_of_rsp(base + 3 * BLOCK_BYTES - 1, base, n), 3);
    assert_eq!(core_of_rsp(base + 3 * BLOCK_BYTES, base, n), 0);
    assert_eq!(core_of_rsp(0x1000, 0, n), 0);
}

#[test]
fn stack_top_is_aligned_inside_the_block() {
    let base = 0x20_0000_0000u64;
    let top = stack_top(1, base, 0x8000);
    assert_eq!(top & 0xF, 0);
    assert!(top >= base && top < base + BLOCK_BYTES);
    assert_eq!(core_of_rsp(top, base, 2), 1);
    assert_eq!(stack_top(0, base, 0x8000), 0x8000);
    let top2 = stack_top(2, base, 0x8000);
    assert_eq!(core_of_rsp(top2, base, 2), 2);
}

#[test]
fn cpu_cap_keeps_b7_on_one_core() {
    assert_eq!(cpu_cap(4, 1, false), 4);
    assert_eq!(cpu_cap(4, 8, true), 4);
    assert_eq!(cpu_cap(4, 2, false), 2);
    assert_eq!(cpu_cap(4, 8, false), 4);
    assert_eq!(cpu_cap(4, 3, false), 3);
}

#[test]
fn pick_core_prefers_the_lightest_nonzero() {
    assert_eq!(pick_core(&[2, 2, 1], 3), 2);
    assert_eq!(pick_core(&[1, 1, 1], 3), 1);
    assert_eq!(pick_core(&[0, 2, 2], 3), 0);
    assert_eq!(pick_core(&[5, 3, 3], 3), 1);
    assert_eq!(pick_core(&[0], 1), 0);
}
