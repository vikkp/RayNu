//! ADR-013 Phase F — native mgmt poll beside VMX / the credit scheduler.
//!
//! Pillar: [Z]
//! Proven Core: **outside** (ADR-002)
//!
//! Phase D first-accept listened **after `VMXOFF`**. Stage 1 completion is
//! `bounded_poll` on a scheduler quantum **while guests remain in VMX root**.
//! Do not put the NIC in the Proven Core. Do not reopen Phase D.

/// Arm BCM5720 coexist (iron). Host/`cfg(test)`: no-op false.
pub fn try_arm_native_coexist() -> bool {
    #[cfg(feature = "uefi-bin")]
    {
        crate::mgmt::host_nic_listen::arm_bcm5720_coexist()
    }
    #[cfg(not(feature = "uefi-bin"))]
    {
        false
    }
}

/// One bounded poll + HTTP step. Safe no-op if not armed.
pub fn tick_native_coexist() {
    #[cfg(feature = "uefi-bin")]
    {
        crate::mgmt::host_nic_listen::tick_bcm5720_coexist();
    }
}

/// Host/CI: launch + listen wiring for Phase F coexist.
pub fn prop_coexist_wired() -> bool {
    let launch = include_str!("../vmx/launch.rs");
    let listen = include_str!("host_nic_listen.rs");
    let coexist = include_str!("host_nic_coexist.rs");
    launch.contains("tick_native_coexist")
        && launch.contains("try_arm_native_coexist")
        && launch.contains("try_spa_vmlaunch")
        && launch.contains("fn enter_sched_coexist(")
        && launch.contains("M4_LADDER_DONE")
        && launch.contains("HOST-NIC coexist — resume G0")
        && launch.contains("G1–G3 parked")
        && listen.contains("fn arm_bcm5720_coexist(")
        && listen.contains("fn tick_bcm5720_coexist(")
        && listen.contains("HOST-NIC coexist listening")
        && listen.contains("VMX on; ADR-013 Phase F")
        && coexist.contains("while guests remain in VMX root")
        && !listen.contains("CURL NOW (post-EBS)")
}

#[cfg(test)]
#[path = "host_nic_coexist_test.rs"]
mod host_nic_coexist_test;
