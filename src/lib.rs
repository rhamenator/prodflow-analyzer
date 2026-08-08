use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct Component {
    pub part: String,
    pub quantity: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    pub number: String,
    pub description: String,
    pub fabrication_time: f64,
    pub material_cost: f64,
    pub labor_cost: f64,
    pub weight: f64,
    pub lead_time_days: f64,
    pub packaging: String,
    pub reorder_point: f64,
    pub components: Vec<Component>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Rollup {
    pub cost: f64,
    pub weight: f64,
    pub lead_time_days: f64,
    pub fabrication_time: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct InventoryPosition {
    pub on_hand: f64,
    #[serde(default)]
    pub allocated: f64,
    #[serde(default)]
    pub on_order: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MaterialPlanLine {
    pub part: String,
    pub description: String,
    pub gross_requirement: f64,
    pub on_hand: f64,
    pub allocated: f64,
    pub on_order: f64,
    pub safety_stock: f64,
    pub projected_available: f64,
    pub suggested_order: f64,
}

#[derive(Debug, Clone)]
pub struct Bom {
    parts: BTreeMap<String, Part>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BomError(String);

impl BomError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl Display for BomError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for BomError {}

#[derive(Debug, Deserialize)]
struct RawPart {
    part: String,
    #[serde(rename = "desc")]
    description: String,
    #[serde(default)]
    fab_time: String,
    #[serde(default)]
    mat_cost: String,
    #[serde(default)]
    labor_cost: String,
    #[serde(default)]
    weight: String,
    #[serde(default)]
    lead_time: String,
    #[serde(default)]
    packaging: String,
    #[serde(default)]
    reorder_point: String,
    #[serde(default)]
    subs: String,
    #[serde(default)]
    quantities: String,
}

#[derive(Debug, Deserialize)]
struct RawInventory {
    part: String,
    on_hand: f64,
    #[serde(default)]
    allocated: f64,
    #[serde(default)]
    on_order: f64,
}

impl Bom {
    pub fn from_csv(path: impl AsRef<Path>) -> Result<Self, Box<dyn Error>> {
        let mut reader = csv::ReaderBuilder::new()
            .trim(csv::Trim::All)
            .from_path(path)?;
        let mut parts = BTreeMap::new();

        for (row_index, record) in reader.deserialize::<RawPart>().enumerate() {
            let row = record.map_err(|error| {
                BomError::new(format!("invalid BOM row {}: {error}", row_index + 2))
            })?;
            let number = row.part.trim().to_owned();
            if number.is_empty() {
                return Err(BomError::new(format!(
                    "BOM row {} has an empty part number",
                    row_index + 2
                ))
                .into());
            }
            if parts.contains_key(&number) {
                return Err(BomError::new(format!("duplicate part number: {number}")).into());
            }

            let component_numbers = split_pipe_list(&row.subs);
            let quantities = split_pipe_list(&row.quantities);
            if !quantities.is_empty() && component_numbers.len() != quantities.len() {
                return Err(BomError::new(format!(
                    "part {number} has {} components but {} quantities",
                    component_numbers.len(),
                    quantities.len()
                ))
                .into());
            }

            let mut components = Vec::with_capacity(component_numbers.len());
            for (index, component) in component_numbers.into_iter().enumerate() {
                let quantity = if quantities.is_empty() {
                    1.0
                } else {
                    parse_required_positive(&quantities[index], "component quantity", &number)?
                };
                components.push(Component {
                    part: component,
                    quantity,
                });
            }

            parts.insert(
                number.clone(),
                Part {
                    number: number.clone(),
                    description: row.description.trim().to_owned(),
                    fabrication_time: parse_optional_number(&row.fab_time, "fab_time", &number)?,
                    material_cost: parse_optional_number(&row.mat_cost, "mat_cost", &number)?,
                    labor_cost: parse_optional_number(&row.labor_cost, "labor_cost", &number)?,
                    weight: parse_optional_number(&row.weight, "weight", &number)?,
                    lead_time_days: parse_optional_number(&row.lead_time, "lead_time", &number)?,
                    packaging: row.packaging.trim().to_owned(),
                    reorder_point: parse_optional_number(
                        &row.reorder_point,
                        "reorder_point",
                        &number,
                    )?,
                    components,
                },
            );
        }

        let bom = Self { parts };
        bom.validate()?;
        Ok(bom)
    }

    pub fn part(&self, number: &str) -> Option<&Part> {
        self.parts.get(number)
    }

    pub fn parts(&self) -> impl Iterator<Item = &Part> {
        self.parts.values()
    }

    pub fn roots(&self) -> Vec<&Part> {
        let children: BTreeSet<&str> = self
            .parts
            .values()
            .flat_map(|part| {
                part.components
                    .iter()
                    .map(|component| component.part.as_str())
            })
            .collect();
        self.parts
            .values()
            .filter(|part| !children.contains(part.number.as_str()))
            .collect()
    }

    pub fn rollup(&self, root: &str) -> Result<Rollup, BomError> {
        if !self.parts.contains_key(root) {
            return Err(BomError::new(format!("unknown root part: {root}")));
        }
        let mut memo = BTreeMap::new();
        self.rollup_inner(root, &mut memo)
    }

    pub fn requirements(
        &self,
        root: &str,
        finished_units: f64,
    ) -> Result<BTreeMap<String, f64>, BomError> {
        if !finished_units.is_finite() || finished_units <= 0.0 {
            return Err(BomError::new("finished unit quantity must be positive"));
        }
        if !self.parts.contains_key(root) {
            return Err(BomError::new(format!("unknown root part: {root}")));
        }

        let mut requirements = BTreeMap::new();
        self.explode_inner(root, finished_units, true, &mut requirements);
        Ok(requirements)
    }

    pub fn material_plan(
        &self,
        root: &str,
        finished_units: f64,
        inventory: &BTreeMap<String, InventoryPosition>,
    ) -> Result<Vec<MaterialPlanLine>, BomError> {
        let requirements = self.requirements(root, finished_units)?;
        let mut lines = Vec::with_capacity(requirements.len());
        for (number, gross_requirement) in requirements {
            let part = &self.parts[&number];
            let position = inventory
                .get(&number)
                .copied()
                .unwrap_or(InventoryPosition {
                    on_hand: 0.0,
                    allocated: 0.0,
                    on_order: 0.0,
                });
            let projected_available = position.on_hand - position.allocated + position.on_order;
            let suggested_order =
                (gross_requirement + part.reorder_point - projected_available).max(0.0);
            lines.push(MaterialPlanLine {
                part: number,
                description: part.description.clone(),
                gross_requirement,
                on_hand: position.on_hand,
                allocated: position.allocated,
                on_order: position.on_order,
                safety_stock: part.reorder_point,
                projected_available,
                suggested_order,
            });
        }
        Ok(lines)
    }

    fn validate(&self) -> Result<(), BomError> {
        for part in self.parts.values() {
            for component in &part.components {
                if !self.parts.contains_key(&component.part) {
                    return Err(BomError::new(format!(
                        "part {} references unknown component {}",
                        part.number, component.part
                    )));
                }
            }
        }

        let mut complete = BTreeSet::new();
        let mut active = Vec::new();
        for number in self.parts.keys() {
            self.validate_acyclic(number, &mut active, &mut complete)?;
        }
        Ok(())
    }

    fn validate_acyclic(
        &self,
        number: &str,
        active: &mut Vec<String>,
        complete: &mut BTreeSet<String>,
    ) -> Result<(), BomError> {
        if complete.contains(number) {
            return Ok(());
        }
        if let Some(position) = active.iter().position(|part| part == number) {
            let mut cycle = active[position..].to_vec();
            cycle.push(number.to_owned());
            return Err(BomError::new(format!(
                "BOM cycle detected: {}",
                cycle.join(" -> ")
            )));
        }

        active.push(number.to_owned());
        for component in &self.parts[number].components {
            self.validate_acyclic(&component.part, active, complete)?;
        }
        active.pop();
        complete.insert(number.to_owned());
        Ok(())
    }

    fn rollup_inner(
        &self,
        number: &str,
        memo: &mut BTreeMap<String, Rollup>,
    ) -> Result<Rollup, BomError> {
        if let Some(rollup) = memo.get(number) {
            return Ok(*rollup);
        }
        let part = &self.parts[number];
        let mut result = Rollup {
            cost: part.material_cost + part.labor_cost,
            weight: part.weight,
            lead_time_days: part.lead_time_days,
            fabrication_time: part.fabrication_time,
        };
        let mut longest_component_lead: f64 = 0.0;
        for component in &part.components {
            let child = self.rollup_inner(&component.part, memo)?;
            result.cost += child.cost * component.quantity;
            result.weight += child.weight * component.quantity;
            result.fabrication_time += child.fabrication_time * component.quantity;
            longest_component_lead = longest_component_lead.max(child.lead_time_days);
        }
        result.lead_time_days += longest_component_lead;
        memo.insert(number.to_owned(), result);
        Ok(result)
    }

    fn explode_inner(
        &self,
        number: &str,
        quantity: f64,
        omit_current: bool,
        requirements: &mut BTreeMap<String, f64>,
    ) {
        if !omit_current {
            *requirements.entry(number.to_owned()).or_default() += quantity;
        }
        for component in &self.parts[number].components {
            self.explode_inner(
                &component.part,
                quantity * component.quantity,
                false,
                requirements,
            );
        }
    }
}

pub fn load_inventory(
    path: impl AsRef<Path>,
) -> Result<BTreeMap<String, InventoryPosition>, Box<dyn Error>> {
    let mut reader = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_path(path)?;
    let mut positions = BTreeMap::new();
    for (row_index, record) in reader.deserialize::<RawInventory>().enumerate() {
        let row = record.map_err(|error| {
            BomError::new(format!("invalid inventory row {}: {error}", row_index + 2))
        })?;
        if row.part.trim().is_empty() {
            return Err(BomError::new(format!(
                "inventory row {} has an empty part number",
                row_index + 2
            ))
            .into());
        }
        for (label, value) in [
            ("on_hand", row.on_hand),
            ("allocated", row.allocated),
            ("on_order", row.on_order),
        ] {
            if !value.is_finite() || value < 0.0 {
                return Err(BomError::new(format!(
                    "inventory {} for {} must be a non-negative number",
                    label, row.part
                ))
                .into());
            }
        }
        positions.insert(
            row.part.trim().to_owned(),
            InventoryPosition {
                on_hand: row.on_hand,
                allocated: row.allocated,
                on_order: row.on_order,
            },
        );
    }
    Ok(positions)
}

pub fn write_plan_csv(
    path: impl AsRef<Path>,
    lines: &[MaterialPlanLine],
) -> Result<(), Box<dyn Error>> {
    let mut writer = csv::Writer::from_path(path)?;
    for line in lines {
        writer.serialize(line)?;
    }
    writer.flush()?;
    Ok(())
}

fn split_pipe_list(value: &str) -> Vec<String> {
    value
        .split('|')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect()
}

fn parse_optional_number(value: &str, field: &str, part: &str) -> Result<f64, BomError> {
    if value.trim().is_empty() {
        return Ok(0.0);
    }
    let parsed = value
        .trim()
        .parse::<f64>()
        .map_err(|_| BomError::new(format!("part {part} has invalid {field}: {value}")))?;
    if !parsed.is_finite() || parsed < 0.0 {
        return Err(BomError::new(format!(
            "part {part} has negative or non-finite {field}: {value}"
        )));
    }
    Ok(parsed)
}

fn parse_required_positive(value: &str, field: &str, part: &str) -> Result<f64, BomError> {
    let parsed = parse_optional_number(value, field, part)?;
    if parsed <= 0.0 {
        return Err(BomError::new(format!(
            "part {part} has non-positive {field}: {value}"
        )));
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Bom {
        Bom {
            parts: BTreeMap::from([
                (
                    "A".to_owned(),
                    Part {
                        number: "A".to_owned(),
                        description: "Assembly".to_owned(),
                        fabrication_time: 1.0,
                        material_cost: 10.0,
                        labor_cost: 5.0,
                        weight: 2.0,
                        lead_time_days: 2.0,
                        packaging: "Box".to_owned(),
                        reorder_point: 1.0,
                        components: vec![
                            Component {
                                part: "B".to_owned(),
                                quantity: 2.0,
                            },
                            Component {
                                part: "C".to_owned(),
                                quantity: 1.0,
                            },
                        ],
                    },
                ),
                (
                    "B".to_owned(),
                    Part {
                        number: "B".to_owned(),
                        description: "Subassembly".to_owned(),
                        fabrication_time: 0.5,
                        material_cost: 2.0,
                        labor_cost: 1.0,
                        weight: 0.5,
                        lead_time_days: 3.0,
                        packaging: String::new(),
                        reorder_point: 2.0,
                        components: vec![Component {
                            part: "C".to_owned(),
                            quantity: 3.0,
                        }],
                    },
                ),
                (
                    "C".to_owned(),
                    Part {
                        number: "C".to_owned(),
                        description: "Purchased component".to_owned(),
                        fabrication_time: 0.0,
                        material_cost: 4.0,
                        labor_cost: 0.0,
                        weight: 0.25,
                        lead_time_days: 4.0,
                        packaging: String::new(),
                        reorder_point: 5.0,
                        components: vec![],
                    },
                ),
            ]),
        }
    }

    #[test]
    fn rollup_counts_shared_components_and_parallel_lead_time() {
        let rollup = fixture().rollup("A").unwrap();
        assert_eq!(rollup.cost, 15.0 + 2.0 * (3.0 + 3.0 * 4.0) + 4.0);
        assert_eq!(rollup.weight, 2.0 + 2.0 * (0.5 + 3.0 * 0.25) + 0.25);
        assert_eq!(rollup.lead_time_days, 2.0 + 3.0 + 4.0);
    }

    #[test]
    fn requirements_aggregate_reused_parts() {
        let requirements = fixture().requirements("A", 2.0).unwrap();
        assert_eq!(requirements["B"], 4.0);
        assert_eq!(requirements["C"], 14.0);
    }

    #[test]
    fn material_plan_accounts_for_allocations_orders_and_safety_stock() {
        let inventory = BTreeMap::from([(
            "C".to_owned(),
            InventoryPosition {
                on_hand: 10.0,
                allocated: 3.0,
                on_order: 2.0,
            },
        )]);
        let plan = fixture().material_plan("A", 2.0, &inventory).unwrap();
        let component = plan.iter().find(|line| line.part == "C").unwrap();
        assert_eq!(component.projected_available, 9.0);
        assert_eq!(component.suggested_order, 10.0);
    }

    #[test]
    fn validation_rejects_cycles() {
        let mut bom = fixture();
        bom.parts.get_mut("C").unwrap().components.push(Component {
            part: "A".to_owned(),
            quantity: 1.0,
        });
        let error = bom.validate().unwrap_err();
        assert!(error.to_string().contains("A -> B -> C -> A"));
    }
}
