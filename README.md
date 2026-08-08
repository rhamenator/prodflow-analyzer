# Prodflow Analyzer

Prodflow Analyzer is a fast Rust tool for validating bills of material,
calculating product rollups, exploding demand, and producing inventory-aware
material plans. It modernizes the useful manufacturing ideas found in the old
CBS/HL/SBT work without carrying forward organization-specific data or code.

The original `prodflow_analyzer.py` prototype is retained for history. The Rust
implementation is the maintained application.

## Build and test

```powershell
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
```

## Use

```powershell
cargo run -- roots --input test_robotic_arm_bom.csv
cargo run -- analyze --input test_robotic_arm_bom.csv --root RA000 --units 12
cargo run -- analyze --input test_robotic_arm_bom.csv --root RA000 --units 12 `
  --inventory inventory_example.csv --output-dir artifacts\robotic-arm
```

The inventory CSV contract is:

```text
part,on_hand,allocated,on_order
```

See `docs\parity-audit.md` for implemented and remaining legacy capabilities.
