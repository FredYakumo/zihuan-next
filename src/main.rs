mod config;
mod error;
mod init_registry;
mod llm;
mod node;
mod ui;
mod util;

use clap::Parser;
use config::load_config;
use lazy_static::lazy_static;
use log::{error, info, warn};
use log_util::log_util::LogUtil;

lazy_static! {
    static ref BASE_LOG: LogUtil = LogUtil::new_with_path("zihuan_next", "logs");
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(
        long = "graph-json",
        value_name = "PATH",
        help = "节点图JSON文件路径（非GUI模式下必需）"
    )]
    graph_json: Option<String>,

    #[arg(
        long = "no-gui",
        help = "以非GUI模式运行节点图（需要--graph-json参数）"
    )]
    no_gui: bool,

    #[arg(
        long = "validate",
        help = "验证节点图JSON文件是否可以安全运行，打印所有错误和警告后退出（退出码：0=通过或仅有警告，1=存在错误）"
    )]
    validate: bool,
}

fn main() {
    ui::log_overlay::CompositeLogger::init(&BASE_LOG).expect("Failed to initialize logger");

    // Initialize node registry
    if let Err(e) = init_registry::init_node_registry() {
        error!("Failed to initialize node registry: {}", e);
    } else {
        info!("Node registry initialized");
    }

    // Parse command line arguments
    let args = Args::parse();

    // Validate mode: check graph JSON and exit
    if args.validate {
        let graph_path = match args.graph_json {
            Some(path) => path,
            None => {
                eprintln!("错误: --validate 需要通过 --graph-json 参数指定节点图文件");
                std::process::exit(2);
            }
        };
        let exit_code = validate_node_graph_json(&graph_path);
        std::process::exit(exit_code);
    }

    // Non-GUI mode: requires graph JSON file
    if args.no_gui {
        let graph_path = match args.graph_json {
            Some(path) => path,
            None => {
                error!("非GUI模式必须通过 --graph-json 参数指定节点图文件");
                return;
            }
        };

        info!("加载节点图文件: {}", graph_path);
        match node::load_graph_definition_from_json(&graph_path) {
            Ok(mut definition) => {
                let hp_values = util::hyperparam_store::load_hyperparameter_values(
                    std::path::Path::new(&graph_path),
                    &definition,
                );
                // Apply hyperparameter bindings before execution
                ui::node_graph_view_inline::apply_hyperparameter_bindings_to_graph(
                    &mut definition,
                    &hp_values,
                );
                if let Err(e) = execute_node_graph(definition) {
                    error!("节点图执行失败: {}", e);
                }
            }
            Err(err) => {
                error!("加载节点图失败: {}", err);
            }
        }
        return;
    }

    // GUI mode: load graph if provided, otherwise start with empty graph
    let mut initial_graph_dirty = false;
    let mut graph = if let Some(path) = args.graph_json.as_ref() {
        match node::load_graph_definition_from_json_with_migration(path) {
            Ok(loaded) => {
                let node::LoadedGraphDefinition { graph, migrated } = loaded;
                initial_graph_dirty = migrated;
                Some(graph)
            }
            Err(err) => {
                error!("加载节点图失败: {}", err);
                return;
            }
        }
    } else {
        None
    };

    if let Some(graph) = graph.as_mut() {
        node::ensure_positions(graph);
    }

    if let Err(err) = ui::node_graph_view::show_graph(
        graph,
        args.graph_json.as_deref().map(std::path::Path::new),
        initial_graph_dirty,
    )
    {
        error!("UI渲染失败: {}", err);
    }
}

/// Execute a node graph loaded from JSON definition
fn execute_node_graph(
    definition: node::NodeGraphDefinition,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("构建节点图");
    let mut graph = node::registry::build_node_graph_from_definition(&definition)?;

    // Load LLM configuration for any LLM nodes that might be in the graph
    let config = load_config();
    if config.agent_model_api.is_none() || config.agent_model_name.is_none() {
        warn!("节点图中的LLM节点可能无法正常工作：缺少 agent_model_api 或 agent_model_name 配置");
    }

    info!("执行节点图");
    graph.execute()?;
    info!("节点图执行完成");

    Ok(())
}

/// Validate a node graph JSON file and print all issues.
/// Returns exit code: 0 = valid (or warnings only), 1 = errors found, 2 = load failure.
fn validate_node_graph_json(graph_path: &str) -> i32 {
    println!("验证节点图: {}", graph_path);

    let definition = match node::load_graph_definition_from_json(graph_path) {
        Ok(def) => def,
        Err(err) => {
            println!("  ✗ 错误: 无法加载或解析文件 — {}", err);
            println!();
            println!("结果: 文件加载失败，节点图无法运行");
            return 2;
        }
    };

    let node_count = definition.nodes.len();
    let edge_count = definition.edges.len();
    println!(
        "  ✓ 文件解析成功（{} 个节点，{} 条连接）",
        node_count, edge_count
    );

    // Collect structural issues from registry validation
    let mut issues = node::graph_io::validate_graph_definition(&definition);

    // Detect cycles (not covered by validate_graph_definition)
    let cycle_nodes = node::graph_io::find_cycle_node_ids(&definition);
    if !cycle_nodes.is_empty() {
        let names: Vec<String> = cycle_nodes
            .iter()
            .filter_map(|id| {
                definition
                    .nodes
                    .iter()
                    .find(|n| &n.id == id)
                    .map(|n| format!("\"{}\"", n.name))
            })
            .collect();
        issues.push(node::graph_io::ValidationIssue {
            severity: "error".into(),
            message: format!("节点图存在环路依赖，涉及节点: {}", names.join(", ")),
        });
    }

    let error_count = issues.iter().filter(|i| i.severity == "error").count();
    let warning_count = issues.iter().filter(|i| i.severity == "warning").count();

    for issue in &issues {
        if issue.severity == "error" {
            println!("  ✗ 错误: {}", issue.message);
        } else {
            println!("  ⚠ 警告: {}", issue.message);
        }
    }

    println!();
    if error_count == 0 && warning_count == 0 {
        println!("结果: ✓ 节点图验证通过，可以安全运行");
        0
    } else if error_count == 0 {
        println!(
            "结果: ⚠ 节点图有 {} 个警告，但可以运行（建议修复警告）",
            warning_count
        );
        0
    } else {
        println!(
            "结果: ✗ {} 个错误，{} 个警告 — 节点图无法安全运行",
            error_count, warning_count
        );
        1
    }
}
