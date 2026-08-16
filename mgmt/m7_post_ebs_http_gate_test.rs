use super::{
    post_ebs_http_scripts_present, post_ebs_http_surface_present, prop_post_ebs_http_scaffold_package,
    run_m7_post_ebs_http_scaffold_gate, M7_POST_EBS_HTTP_GATE_MARKER,
};

#[test]
fn m7_post_ebs_http_scaffold_passes() {
    assert_eq!(
        M7_POST_EBS_HTTP_GATE_MARKER,
        "RAYNU-V-M7-POST-EBS-HTTP-SCAFFOLD-OK"
    );
    assert!(
        post_ebs_http_surface_present(),
        "post-EBS SNP park/probe/idle + PRE-EBS fallback must be wired"
    );
    assert!(
        post_ebs_http_scripts_present(),
        "smoke + runbook must name post-EBS markers"
    );
    assert!(prop_post_ebs_http_scaffold_package());
    assert!(run_m7_post_ebs_http_scaffold_gate());
    println!("RAYNU-V-M7-POST-EBS-HTTP-SCAFFOLD-OK");
}
