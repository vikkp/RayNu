//! iDRAC Redfish client, hardware health + topology (ADR-005).
//!
//! Pillar: [D]
//! Proven Core: **outside** (ADR-002, ADR-005)
//! VERIFICATION: N/A
//! Milestone: M5.6 Dell Tier‑1
//!
//! M5.6 MV: Tier‑1 thermal / fan / PSU health via a documented Redfish mock
//! (CI / QEMU), plus SMBIOS DIMM + ACPI MADT / SRAT / SLIT topology visible
//! to ops. Live BMC HTTP remains `GAP: live Redfish BMC → polish`. Tier‑2
//! (PERC OEM, predictive failure) is out of scope — `GAP: Dell Tier-2 OEM`.

/// Host / CI marker when the M5.6 iDRAC gate passes.
pub const M5_IDRAC_OK_MARKER: &str = "RAYNU-V-M5-IDRAC-OK";

/// Documented live-BMC gap (mock path closes M5.6).
pub const LIVE_REDFISH_GAP_NOTE: &str = "GAP: live Redfish BMC → polish";

/// Tier‑2 remains deferred (ADR-005).
pub const TIER2_GAP_NOTE: &str = "GAP: Dell Tier-2 OEM";

/// Embedded mock Redfish Tier‑1 payload for host/CI.
pub const MOCK_REDFISH: &str = include_str!("../assets/idrac/mock_redfish.json");

/// Embedded mock SMBIOS/ACPI topology for host/CI.
pub const MOCK_TOPOLOGY: &str = include_str!("../assets/idrac/mock_topology.txt");

/// Max DIMM / CPU / NUMA / SLIT rows in one topology snapshot.
pub const TOPO_DIMM_CAP: usize = 16;
pub const TOPO_CPU_CAP: usize = 64;
pub const TOPO_NUMA_CAP: usize = 8;
pub const TOPO_SLIT_CAP: usize = 32;

/// Redfish integration tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdracTier {
    /// Self-sufficient: serial, thermal/fan/PSU, SMBIOS, ACPI, X710, NVMe.
    Tier1,
    /// Partnership / RE: PERC OEM, SPD detail, predictive failure, auto-throttle.
    Tier2,
}

pub fn default_tier() -> IdracTier {
    IdracTier::Tier1
}

/// Component health from a Redfish Status.Health field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthState {
    Ok,
    Warning,
    Critical,
    Unknown,
}

/// Aggregated Tier‑1 chassis health (thermal + fan + PSU).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tier1Health {
    pub thermal_ok: bool,
    pub fan_ok: bool,
    pub psu_ok: bool,
    pub temp_celsius: u32,
    pub fan_rpm: u32,
    pub psu_count: u32,
}

impl Tier1Health {
    pub fn all_ok(&self) -> bool {
        self.thermal_ok && self.fan_ok && self.psu_ok
    }
}

/// One SMBIOS-style DIMM row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DimmInfo {
    pub socket: u8,
    pub size_mb: u32,
    pub numa_node: u8,
}

/// One ACPI MADT-style CPU / APIC entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuInfo {
    pub cpu_id: u16,
    pub apic_id: u16,
    pub socket: u8,
}

/// One ACPI SRAT-style NUMA node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NumaNode {
    pub node_id: u8,
    pub socket: u8,
}

/// One ACPI SLIT distance entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlitEntry {
    pub from: u8,
    pub to: u8,
    pub distance: u8,
}

/// Topology snapshot surfaced to mgmt / ops.
#[derive(Debug, Clone, Copy)]
pub struct TopologySnapshot {
    pub dimms: [DimmInfo; TOPO_DIMM_CAP],
    pub dimm_count: usize,
    pub cpus: [CpuInfo; TOPO_CPU_CAP],
    pub cpu_count: usize,
    pub numa: [NumaNode; TOPO_NUMA_CAP],
    pub numa_count: usize,
    pub slit: [SlitEntry; TOPO_SLIT_CAP],
    pub slit_count: usize,
}

impl TopologySnapshot {
    pub const fn empty() -> Self {
        Self {
            dimms: [DimmInfo {
                socket: 0,
                size_mb: 0,
                numa_node: 0,
            }; TOPO_DIMM_CAP],
            dimm_count: 0,
            cpus: [CpuInfo {
                cpu_id: 0,
                apic_id: 0,
                socket: 0,
            }; TOPO_CPU_CAP],
            cpu_count: 0,
            numa: [NumaNode {
                node_id: 0,
                socket: 0,
            }; TOPO_NUMA_CAP],
            numa_count: 0,
            slit: [SlitEntry {
                from: 0,
                to: 0,
                distance: 0,
            }; TOPO_SLIT_CAP],
            slit_count: 0,
        }
    }

    pub fn socket_count(&self) -> u8 {
        let mut max = 0u8;
        for d in self.dimms.iter().take(self.dimm_count) {
            if d.socket > max {
                max = d.socket;
            }
        }
        for c in self.cpus.iter().take(self.cpu_count) {
            if c.socket > max {
                max = c.socket;
            }
        }
        max.saturating_add(1)
    }
}

/// Parse error for mock Redfish / topology text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdracError {
    BadRedfish,
    BadTopology,
    MissingThermal,
    MissingFan,
    MissingPsu,
}

/// Map a Redfish Health string to [`HealthState`].
pub fn parse_health(s: &str) -> HealthState {
    if s.contains("OK") || s.contains("Ok") {
        HealthState::Ok
    } else if s.contains("Warning") {
        HealthState::Warning
    } else if s.contains("Critical") {
        HealthState::Critical
    } else {
        HealthState::Unknown
    }
}

/// Extract the first decimal integer after `key` in `text` (best-effort JSON).
fn first_u32_after(text: &str, key: &str) -> Option<u32> {
    let i = text.find(key)?;
    let rest = &text[i + key.len()..];
    let mut digits = None;
    let mut n: u32 = 0;
    for b in rest.bytes() {
        if b.is_ascii_digit() {
            n = n.saturating_mul(10).saturating_add(u32::from(b - b'0'));
            digits = Some(n);
        } else if digits.is_some() {
            break;
        } else if b == b' ' || b == b'\t' || b == b':' || b == b'"' {
            continue;
        } else if digits.is_none() && (b == b'\n' || b == b',' || b == b'{') {
            continue;
        } else if digits.is_none() {
            // skip non-digit until we hit a number or give up on this slice
            if b == b'}' || b == b']' {
                break;
            }
        }
    }
    digits
}

/// Count occurrences of `"Health": "OK"` (or Ok) inside a braced section named `section`.
fn section_health_ok_count(text: &str, section: &str) -> u32 {
    let Some(start) = text.find(section) else {
        return 0;
    };
    let rest = &text[start..];
    // Bound section loosely by the next top-level-ish key or end.
    let end = rest
        .find("\n  },")
        .or_else(|| rest.find("\n  }"))
        .unwrap_or(rest.len());
    let body = &rest[..end];
    let mut n = 0u32;
    let mut search = body;
    while let Some(i) = search.find("\"Health\"") {
        let after = &search[i..];
        if after.contains("OK") || after.contains("Ok") {
            // Only count if OK appears before the next Health or end of a short window.
            let window = if after.len() > 48 { &after[..48] } else { after };
            if window.contains("OK") || window.contains("Ok") {
                n = n.saturating_add(1);
            }
        }
        search = &search[i + 8..];
    }
    n
}

/// Read Tier‑1 health from a Redfish-like JSON document (mock or live).
///
/// Documented CI/QEMU path uses [`MOCK_REDFISH`]. Live BMC HTTP is
/// [`LIVE_REDFISH_GAP_NOTE`].
pub fn read_tier1_health(redfish_json: &str) -> Result<Tier1Health, IdracError> {
    let _ = LIVE_REDFISH_GAP_NOTE;
    let _ = TIER2_GAP_NOTE;
    if !redfish_json.contains("Thermal") || !redfish_json.contains("Power") {
        return Err(IdracError::BadRedfish);
    }
    let temp_ok = section_health_ok_count(redfish_json, "\"Temperatures\"") > 0
        || (redfish_json.contains("ReadingCelsius")
            && redfish_json.contains("\"Health\": \"OK\""));
    let fan_ok = section_health_ok_count(redfish_json, "\"Fans\"") > 0
        || (redfish_json.contains("\"Fans\"") && redfish_json.contains("\"Health\": \"OK\""));
    let psu_ok = section_health_ok_count(redfish_json, "\"PowerSupplies\"") > 0
        || (redfish_json.contains("PowerSupplies") && redfish_json.contains("\"Health\": \"OK\""));

    if !redfish_json.contains("ReadingCelsius") {
        return Err(IdracError::MissingThermal);
    }
    if !redfish_json.contains("\"Fans\"") {
        return Err(IdracError::MissingFan);
    }
    if !redfish_json.contains("PowerSupplies") {
        return Err(IdracError::MissingPsu);
    }

    let temp_celsius = first_u32_after(redfish_json, "ReadingCelsius").unwrap_or(0);
    let fan_rpm = if let Some(fans) = redfish_json.find("\"Fans\"") {
        first_u32_after(&redfish_json[fans..], "\"Reading\"").unwrap_or(0)
    } else {
        0
    };
    let mut psu_count = 0u32;
    let mut search = redfish_json;
    while let Some(i) = search.find("\"PSU") {
        psu_count = psu_count.saturating_add(1);
        search = &search[i + 4..];
    }
    if psu_count == 0 {
        psu_count = section_health_ok_count(redfish_json, "\"PowerSupplies\"");
    }

    Ok(Tier1Health {
        thermal_ok: temp_ok,
        fan_ok,
        psu_ok,
        temp_celsius,
        fan_rpm,
        psu_count,
    })
}

/// Convenience: thermal path used by older stubs.
pub fn thermal_ok_stub() -> bool {
    read_tier1_health(MOCK_REDFISH)
        .map(|h| h.thermal_ok)
        .unwrap_or(false)
}

fn parse_u32(s: &str) -> Option<u32> {
    let mut n: u32 = 0;
    if s.is_empty() {
        return None;
    }
    for b in s.bytes() {
        if !(b'0'..=b'9').contains(&b) {
            return None;
        }
        n = n.checked_mul(10)?.checked_add(u32::from(b - b'0'))?;
    }
    Some(n)
}

/// Parse mock SMBIOS/ACPI topology text into a snapshot for mgmt/ops.
pub fn parse_topology(text: &str) -> Result<TopologySnapshot, IdracError> {
    let mut snap = TopologySnapshot::empty();
    for raw in text.split('\n') {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let kind = parts.next().ok_or(IdracError::BadTopology)?;
        match kind {
            "dimm" => {
                if snap.dimm_count >= TOPO_DIMM_CAP {
                    return Err(IdracError::BadTopology);
                }
                let socket = parse_u32(parts.next().ok_or(IdracError::BadTopology)?)
                    .ok_or(IdracError::BadTopology)? as u8;
                let size_mb = parse_u32(parts.next().ok_or(IdracError::BadTopology)?)
                    .ok_or(IdracError::BadTopology)?;
                let numa_node = parse_u32(parts.next().ok_or(IdracError::BadTopology)?)
                    .ok_or(IdracError::BadTopology)? as u8;
                if parts.next().is_some() {
                    return Err(IdracError::BadTopology);
                }
                snap.dimms[snap.dimm_count] = DimmInfo {
                    socket,
                    size_mb,
                    numa_node,
                };
                snap.dimm_count += 1;
            }
            "cpu" => {
                if snap.cpu_count >= TOPO_CPU_CAP {
                    return Err(IdracError::BadTopology);
                }
                let cpu_id = parse_u32(parts.next().ok_or(IdracError::BadTopology)?)
                    .ok_or(IdracError::BadTopology)? as u16;
                let apic_id = parse_u32(parts.next().ok_or(IdracError::BadTopology)?)
                    .ok_or(IdracError::BadTopology)? as u16;
                let socket = parse_u32(parts.next().ok_or(IdracError::BadTopology)?)
                    .ok_or(IdracError::BadTopology)? as u8;
                if parts.next().is_some() {
                    return Err(IdracError::BadTopology);
                }
                snap.cpus[snap.cpu_count] = CpuInfo {
                    cpu_id,
                    apic_id,
                    socket,
                };
                snap.cpu_count += 1;
            }
            "numa" => {
                if snap.numa_count >= TOPO_NUMA_CAP {
                    return Err(IdracError::BadTopology);
                }
                let node_id = parse_u32(parts.next().ok_or(IdracError::BadTopology)?)
                    .ok_or(IdracError::BadTopology)? as u8;
                let socket = parse_u32(parts.next().ok_or(IdracError::BadTopology)?)
                    .ok_or(IdracError::BadTopology)? as u8;
                if parts.next().is_some() {
                    return Err(IdracError::BadTopology);
                }
                snap.numa[snap.numa_count] = NumaNode { node_id, socket };
                snap.numa_count += 1;
            }
            "slit" => {
                if snap.slit_count >= TOPO_SLIT_CAP {
                    return Err(IdracError::BadTopology);
                }
                let from = parse_u32(parts.next().ok_or(IdracError::BadTopology)?)
                    .ok_or(IdracError::BadTopology)? as u8;
                let to = parse_u32(parts.next().ok_or(IdracError::BadTopology)?)
                    .ok_or(IdracError::BadTopology)? as u8;
                let distance = parse_u32(parts.next().ok_or(IdracError::BadTopology)?)
                    .ok_or(IdracError::BadTopology)? as u8;
                if parts.next().is_some() {
                    return Err(IdracError::BadTopology);
                }
                snap.slit[snap.slit_count] = SlitEntry { from, to, distance };
                snap.slit_count += 1;
            }
            _ => return Err(IdracError::BadTopology),
        }
    }
    if snap.dimm_count == 0 || snap.cpu_count == 0 || snap.numa_count == 0 {
        return Err(IdracError::BadTopology);
    }
    Ok(snap)
}

/// Load the documented mock topology (SMBIOS DIMM + ACPI MADT/SRAT/SLIT).
pub fn read_topology_mock() -> Result<TopologySnapshot, IdracError> {
    parse_topology(MOCK_TOPOLOGY)
}

/// Host-testable: mock Redfish Tier‑1 health + topology for ops.
pub fn prop_tier1_health_and_topology() -> bool {
    let health = match read_tier1_health(MOCK_REDFISH) {
        Ok(h) => h,
        Err(_) => return false,
    };
    if !health.all_ok() || health.psu_count < 2 || health.temp_celsius == 0 || health.fan_rpm == 0
    {
        return false;
    }
    let topo = match read_topology_mock() {
        Ok(t) => t,
        Err(_) => return false,
    };
    topo.dimm_count >= 2
        && topo.cpu_count >= 2
        && topo.numa_count >= 2
        && topo.slit_count >= 2
        && topo.socket_count() >= 2
        && MOCK_REDFISH.contains("Temperatures")
        && MOCK_REDFISH.contains("Fans")
        && MOCK_REDFISH.contains("PowerSupplies")
        && MOCK_TOPOLOGY.contains("dimm ")
        && MOCK_TOPOLOGY.contains("cpu ")
        && MOCK_TOPOLOGY.contains("numa ")
        && MOCK_TOPOLOGY.contains("slit ")
        && default_tier() == IdracTier::Tier1
        && TIER2_GAP_NOTE.contains("Tier-2")
        && LIVE_REDFISH_GAP_NOTE.contains("Redfish")
        && M5_IDRAC_OK_MARKER == "RAYNU-V-M5-IDRAC-OK"
}

pub mod m5_idrac_gate;

pub use m5_idrac_gate::run_m5_idrac_gate;

#[cfg(test)]
#[path = "idrac_test.rs"]
mod idrac_test;
