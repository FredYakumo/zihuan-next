use std::path::{Path, PathBuf};

use clap::Parser;
use zihuan_core::error::{Error, Result};

#[derive(Debug, Parser)]
#[command(
    author,
    version,
    about = "Execute a zihuan graph from the command line"
)]
struct Args {
    #[arg(long, conflicts_with = "workflow")]
    file: Option<PathBuf>,

    #[arg(long, conflicts_with = "file")]
    workflow: Option<String>,
}

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let args = Args::parse();
    init_node_registry()?;

    let graph_path = resolve_graph_path(&args)?;
    let loaded = zihuan_graph_engine::load_graph_definition_from_json_with_migration(&graph_path)?;
    let mut graph = zihuan_graph_engine::build_node_graph_from_definition(&loaded.graph)?;
    graph.execute()?;
    println!("Graph executed successfully: {}", graph_path.display());
    Ok(())
}

fn resolve_graph_path(args: &Args) -> Result<PathBuf> {
    match (&args.file, &args.workflow) {
        (Some(path), None) => Ok(path.clone()),
        (None, Some(name)) => Ok(Path::new("workflow_set").join(format!("{name}.json"))),
        (None, None) => Err(Error::ValidationError(
            "missing input: pass --file <PATH> or --workflow <NAME>".to_string(),
        )),
        (Some(_), Some(_)) => Err(Error::ValidationError(
            "choose either --file or --workflow".to_string(),
        )),
    }
}

fn init_node_registry() -> Result<()> {
    zihuan_graph_engine::registry::init_node_registry_with_extensions(&[
        storage_handler::init_node_registry,
        ims_bot_adapter::init_node_registry,
        zihuan_core_nodes::init_node_registry,
    ])
}
