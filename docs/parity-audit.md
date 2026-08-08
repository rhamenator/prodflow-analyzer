# Legacy manufacturing parity audit

Evidence was inspected read-only under `E:\TheSWShop\CBS`,
`E:\TheSWShop\HL`, and selected manufacturing-oriented portions of
`E:\TheSWShop\hlmfg\SBT`. No legacy source or business data is copied here.

## Implemented in the Rust core

- multi-level bills of material with quantity multipliers
- validation for duplicate parts, unknown components, bad quantities, and cycles
- rolled material/labor cost, weight, fabrication effort, and critical-path lead time
- demand explosion that aggregates a shared part across every parent path
- inventory positions with on-hand, allocation, and on-order quantities
- reorder-point safety stock and suggested purchase quantities
- deterministic CSV and JSON planning output

## Still worth pursuing

- dated work orders and finite-capacity scheduling
- lot/serial genealogy and inspection holds
- vendor sourcing, minimum order quantities, price breaks, and purchase orders
- sales-order allocation and available-to-promise calculations
- receiving, issue-to-work-order, completion, and inventory transaction history
- unit-of-measure conversion, scrap/yield, substitutions, and engineering revisions
- accounting handoff rather than rebuilding the complete SBT accounting suite

The old SBT material includes a much wider accounting product. That is not a
parity target for this repository because BrassLedger already owns the generic
accounting product space.
