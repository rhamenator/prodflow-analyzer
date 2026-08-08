use prodflow_analyzer::Bom;

#[test]
fn robotic_arm_fixture_is_valid_and_rolls_up() {
    let bom = Bom::from_csv("test_robotic_arm_bom.csv").unwrap();
    let roots: Vec<_> = bom
        .roots()
        .iter()
        .map(|part| part.number.as_str())
        .collect();
    assert_eq!(roots, ["RA000"]);
    let rollup = bom.rollup("RA000").unwrap();
    assert!(rollup.cost > 5_000.0);
    assert!(rollup.weight > 80.0);
    assert!(rollup.lead_time_days >= 12.0);
}

#[test]
fn smart_hub_fixture_aggregates_shared_fasteners() {
    let bom = Bom::from_csv("test_smart_hub_bom.csv").unwrap();
    let requirements = bom.requirements("H000", 10.0).unwrap();
    assert_eq!(requirements["H930"], 80.0);
}
