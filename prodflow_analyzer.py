"""
prodflow_analyzer.py

Professional, modular analysis and reporting of Bill of Materials and supply chain networks.

Features:
- BOM import (CSV, SQLite, PostgreSQL)
- Advanced dependency rollups (cost, weight, lead time)
- PO trigger and inventory status
- Graph/tree/Gantt reporting
- Config-driven batch export (JSON, XML, CSV, EDI)
- REST API for real-time data access (optional)
- Extensible schema: add cost, labor, weight, lead time, packaging, quantities, etc.

Author: [Your Name]
"""

import os
import sys
import yaml
import pandas as pd
import networkx as nx
import matplotlib.pyplot as plt
from sqlalchemy import create_engine, inspect
from sqlalchemy.exc import ProgrammingError

# ---- CONFIG LOADING ----
CONFIG_FILE = 'prodflow_config.yaml'

DEFAULT_CONFIG = {
    "db_url": "sqlite:///prodflow_bom.db",  # Or "postgresql://user:pass@localhost/dbname"
    "data_source": "csv",  # 'csv' or 'db'
    "csv_path": "test_robotic_arm_bom.csv",
    "export_formats": ["json", "csv"],
    "export_base": "prodflow_export",
    "units_per_pallet": 12,
    "units_per_cwt": 20,
    "rest_api": False,
    "rest_api_host": "127.0.0.1",
    "rest_api_port": 5000
}

def load_config():
    if os.path.exists(CONFIG_FILE):
        with open(CONFIG_FILE, 'r') as f:
            config = yaml.safe_load(f)
        merged = {**DEFAULT_CONFIG, **(config or {})}
        return merged
    else:
        return DEFAULT_CONFIG

config = load_config()

# ---- DATABASE SCHEMA ----
BOM_SCHEMA = """
CREATE TABLE IF NOT EXISTS bom (
    part TEXT PRIMARY KEY,
    desc TEXT,
    fab_time REAL,
    mat_cost REAL,
    labor_cost REAL,
    weight REAL,
    lead_time INTEGER,
    packaging TEXT,
    reorder_point INTEGER
);
CREATE TABLE IF NOT EXISTS bom_links (
    parent TEXT,
    child TEXT,
    quantity INTEGER,
    PRIMARY KEY (parent, child)
);
"""

def setup_db(engine):
    with engine.connect() as conn:
        for stmt in BOM_SCHEMA.strip().split(';'):
            if stmt.strip():
                try:
                    conn.execute(stmt)
                except ProgrammingError:
                    continue

def import_csv_to_db(csv_path, engine):
    df = pd.read_csv(csv_path, dtype=str)
    # Ensure proper columns exist
    required_fields = ["part", "desc", "fab_time", "subs", "quantities"]
    for f in required_fields:
        if f not in df.columns:
            raise ValueError(f"Missing required field: {f}")
    # Fill in blank optional fields with defaults
    for col in ["mat_cost", "labor_cost", "weight", "lead_time", "packaging", "reorder_point"]:
        if col not in df.columns:
            df[col] = 0
    # Insert bom table
    bom_fields = ["part", "desc", "fab_time", "mat_cost", "labor_cost", "weight", "lead_time", "packaging", "reorder_point"]
    df_bom = df[bom_fields].drop_duplicates()
    df_bom.to_sql('bom', engine, if_exists='replace', index=False)
    # Parse and insert links
    links = []
    for _, row in df.iterrows():
        parent = row["part"]
        if pd.notnull(row["subs"]) and row["subs"].strip():
            subs = [s.strip() for s in str(row["subs"]).split('|') if s.strip()]
            qtys = [int(x) for x in str(row["quantities"]).split('|')] if pd.notnull(row["quantities"]) and row["quantities"].strip() else [1]*len(subs)
            for child, qty in zip(subs, qtys):
                links.append({"parent": parent, "child": child, "quantity": qty})
    df_links = pd.DataFrame(links)
    if not df_links.empty:
        df_links.to_sql('bom_links', engine, if_exists='replace', index=False)
    print(f"Imported {len(df)} parts and {len(links)} BOM links from CSV.")

def load_bom_from_db(engine):
    with engine.connect() as conn:
        bom = pd.read_sql("SELECT * FROM bom", conn)
        links = pd.read_sql("SELECT * FROM bom_links", conn)
    return bom, links

def build_graph(bom, links):
    G = nx.DiGraph()
    for _, row in bom.iterrows():
        G.add_node(row["part"], **row.to_dict())
    for _, row in links.iterrows():
        G.add_edge(row["child"], row["parent"], quantity=row["quantity"])
    return G

# ---- ROLLUPS ----
def rollup_to_end_product(G, end_product):
    """
    Compute total material+labor cost, total weight, and total lead time
    for each part up to end_product.
    """
    cost = {}
    weight = {}
    lead_time = {}
    def recur(n):
        mat = float(G.nodes[n].get("mat_cost",0))
        lab = float(G.nodes[n].get("labor_cost",0))
        wgt = float(G.nodes[n].get("weight",0))
        lt = int(G.nodes[n].get("lead_time",0))
        children = list(G.predecessors(n))
        if not children:
            cost[n] = mat + lab
            weight[n] = wgt
            lead_time[n] = lt
        else:
            cost[n] = mat + lab + sum(recur(c)*G[c][n]["quantity"] for c in children)
            weight[n] = wgt + sum(weight[c]*G[c][n]["quantity"] for c in children)
            lead_time[n] = lt + max(lead_time[c] for c in children) if children else lt
        return cost[n]
    recur(end_product)
    return cost, weight, lead_time

def print_bom_tree(G, n, level=0, visited=None):
    """
    Pretty-print dependency tree for a given part
    """
    if visited is None:
        visited = set()
    q = ""
    for pred in G.predecessors(n):
        qty = G[pred][n]["quantity"]
        q += f" [{qty}x {pred}]"
    print("  " * level + f"{n} ({G.nodes[n]['desc']})" + q)
    for pred in G.predecessors(n):
        if pred not in visited:
            visited.add(pred)
            print_bom_tree(G, pred, level+1, visited)

def draw_bom_graph(G):
    """
    Draw the BOM DAG using graphviz if available.
    """
    try:
        import pydot
        pos = nx.nx_pydot.graphviz_layout(G, prog="dot")
    except Exception as e:
        print("Falling back to spring layout (Graphviz/pydot not available).")
        pos = nx.spring_layout(G, seed=42)
    plt.figure(figsize=(18, 12))
    nx.draw(G, pos, with_labels=True, node_size=3000, node_color='lightgreen', 
            font_size=9, font_weight='bold', arrows=True, arrowstyle='->')
    node_labels = {n: f"{n}\n{G.nodes[n]['desc']}" for n in G.nodes}
    nx.draw_networkx_labels(G, pos, labels=node_labels, font_size=7)
    plt.title("BOM Dependency Graph (DAG)")
    plt.tight_layout()
    plt.show()

# ---- EXPORT ----
def export_bom(bom, links, fmt="json", filename="prodflow_export"):
    records = []
    for _, row in bom.iterrows():
        rec = row.to_dict()
        rec["subs"] = list(links.loc[links["parent"]==row["part"],"child"])
        rec["quantities"] = list(links.loc[links["parent"]==row["part"],"quantity"])
        records.append(rec)
    if fmt == "json":
        with open(filename + ".json", "w", encoding="utf-8") as f:
            json.dump(records, f, indent=2)
        print(f"BOM exported to {filename}.json")
    elif fmt == "csv":
        df = pd.DataFrame(records)
        df["subs"] = df["subs"].apply(lambda l: "|".join(map(str,l)))
        df["quantities"] = df["quantities"].apply(lambda l: "|".join(map(str,l)))
        df.to_csv(filename + ".csv", index=False)
        print(f"BOM exported to {filename}.csv")
    elif fmt == "xml":
        root = ET.Element("BOM")
        for rec in records:
            part = ET.SubElement(root, "Part")
            for k, v in rec.items():
                if k in ["subs", "quantities"]:
                    continue
                ET.SubElement(part, k.title().replace('_','')).text = str(v)
            subs = ET.SubElement(part, "Subcomponents")
            for s, q in zip(rec["subs"], rec["quantities"]):
                subel = ET.SubElement(subs, "Subcomponent")
                subel.set("part", s)
                subel.set("quantity", str(q))
        tree = ET.ElementTree(root)
        tree.write(filename + ".xml", encoding="utf-8", xml_declaration=True)
        print(f"BOM exported to {filename}.xml")
    elif fmt == "edi":
        with open(filename + ".edi", "w", encoding="utf-8") as f:
            f.write("ISA*00*          *00*          *ZZ*YOURID         *ZZ*SUPPLIERID     *210101*0000*U*00401*000000001*0*P*>\n")
            f.write("GS*SC*YOURID*SUPPLIERID*20210101*0000*1*X*004010\n")
            for rec in records:
                f.write(f"LIN**BP*{rec['part']}*PD*{rec['desc']}*CT*{rec['mat_cost']}*LT*{rec['lead_time']}\n")
                if rec["subs"]:
                    f.write(f"    SUBS*{'*'.join([f'{s}:{q}' for s,q in zip(rec['subs'],rec['quantities'])])}\n")
            f.write("GE*1*1\nIEA*1*000000001\n")
        print(f"BOM exported to {filename}.edi")
    else:
        print("Unsupported export format.")

# ---- REST API ----
def start_rest_api(engine):
    from flask import Flask, jsonify, request
    app = Flask(__name__)

    @app.route('/bom/parts', methods=['GET'])
    def get_parts():
        bom, _ = load_bom_from_db(engine)
        return jsonify(bom.to_dict(orient='records'))

    @app.route('/bom/part/<part>', methods=['GET'])
    def get_part(part):
        bom, links = load_bom_from_db(engine)
        row = bom.loc[bom['part'] == part]
        if row.empty:
            return jsonify({'error': 'not found'}), 404
        rec = row.iloc[0].to_dict()
        rec["subs"] = list(links.loc[links["parent"]==part,"child"])
        rec["quantities"] = list(links.loc[links["parent"]==part,"quantity"])
        return jsonify(rec)

    app.run(host=config["rest_api_host"], port=config["rest_api_port"], debug=False)

# ---- MAIN WORKFLOW ----
def main():
    # Database setup
    engine = create_engine(config["db_url"])
    setup_db(engine)
    # Import CSV if data_source=csv
    if config["data_source"] == "csv":
        import_csv_to_db(config["csv_path"], engine)
    # Load BOM and build graph
    bom, links = load_bom_from_db(engine)
    G = build_graph(bom, links)
    # Identify orphans
    sink_nodes = [n for n in G.nodes if G.out_degree(n) == 0]
    used_as_sub = set(links["child"])
    orphans = [n for n in sink_nodes if n not in used_as_sub]
    if orphans:
        print("\nWARNING: Orphaned part(s) detected (not used in any assembly):")
        for n in orphans:
            print(f"  - {n}: {G.nodes[n]['desc']}")
    # End product selection (automatic)
    end_product = sink_nodes[0]
    # Dependency trees
    print("\nDependency trees for all sink (end product) nodes:")
    for sink in sink_nodes:
        print(f"\nEnd product: {sink} ({G.nodes[sink]['desc']})")
        print_bom_tree(G, sink)
    # Visualize
    draw_bom_graph(G)
    # Rollup
    cost, weight, lead_time = rollup_to_end_product(G, end_product)
    print(f"\n== Rollup to end product {end_product} ({G.nodes[end_product]['desc']}) ==")
    print(f"Total Cost: ${cost[end_product]:,.2f}")
    print(f"Total Weight: {weight[end_product]:.2f} kg")
    print(f"Lead Time: {lead_time[end_product]} days")
    print(f"Units per pallet: {config['units_per_pallet']}, per cwt: {config['units_per_cwt']}")
    print(f"Packaged as: {G.nodes[end_product].get('packaging','N/A')}")
    # Export
    for fmt in config["export_formats"]:
        export_bom(bom, links, fmt=fmt, filename=config["export_base"])
    # REST API (optional)
    if config.get("rest_api"):
        print(f"Starting REST API at http://{config['rest_api_host']}:{config['rest_api_port']}/")
        start_rest_api(engine)

if __name__ == "__main__":
    main()
