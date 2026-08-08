use prodflow_analyzer::{Bom, load_inventory, write_plan_csv};
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::path::PathBuf;

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty()
        || args
            .iter()
            .any(|argument| argument == "-h" || argument == "--help")
    {
        print_help();
        return Ok(());
    }

    let input = option(&args, "--input").ok_or("--input is required")?;
    let bom = Bom::from_csv(input)?;
    if args[0] == "roots" {
        for root in bom.roots() {
            println!("{}\t{}", root.number, root.description);
        }
        return Ok(());
    }
    if args[0] != "analyze" {
        return Err(format!("unknown command: {}", args[0]).into());
    }

    let root = option(&args, "--root")
        .map(str::to_owned)
        .or_else(|| bom.roots().first().map(|part| part.number.clone()))
        .ok_or("the BOM has no root assembly")?;
    let units = option(&args, "--units")
        .unwrap_or("1")
        .parse::<f64>()
        .map_err(|_| "--units must be a number")?;
    let inventory = match option(&args, "--inventory") {
        Some(path) => load_inventory(path)?,
        None => BTreeMap::new(),
    };

    let rollup = bom.rollup(&root)?;
    let plan = bom.material_plan(&root, units, &inventory)?;
    println!("root: {root}");
    println!("finished_units: {units}");
    println!("unit_cost: {:.2}", rollup.cost);
    println!("unit_weight: {:.3}", rollup.weight);
    println!("lead_time_days: {:.2}", rollup.lead_time_days);
    println!("unit_fabrication_time: {:.2}", rollup.fabrication_time);
    println!("planned_parts: {}", plan.len());
    println!(
        "parts_to_order: {}",
        plan.iter()
            .filter(|line| line.suggested_order > 0.0)
            .count()
    );

    if let Some(output) = option(&args, "--output-dir") {
        let output = PathBuf::from(output);
        fs::create_dir_all(&output)?;
        write_plan_csv(output.join("material-plan.csv"), &plan)?;
        serde_json::to_writer_pretty(File::create(output.join("material-plan.json"))?, &plan)?;
        serde_json::to_writer_pretty(File::create(output.join("rollup.json"))?, &rollup)?;
        println!("output_directory: {}", output.display());
    }

    Ok(())
}

fn option<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].as_str())
}

fn print_help() {
    println!(
        "prodflow-analyzer\n\n\
         Usage:\n  \
         prodflow roots --input BOM.csv\n  \
         prodflow analyze --input BOM.csv [--root PART] [--units N] \\\n+             [--inventory INVENTORY.csv] [--output-dir DIR]\n\n\
         Inventory columns: part,on_hand,allocated,on_order"
    );
}
