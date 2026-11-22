mod assets;

use clap::Parser;

#[derive(Parser)]
#[command(version, about = "紫幻组件管理器桌面界面")]
struct Args {
    /// 与 CLI / TUI 共用的数据目录
    #[arg(long)]
    data_dir: Option<std::path::PathBuf>,
}

fn main() {
    if let Err(error) = run(Args::parse()) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run(args: Args) -> Result<(), String> {
    let runtime = tokio::runtime::Runtime::new().map_err(|error| error.to_string())?;
    let session =
        runtime.block_on(zihuan_composer::Session::start(args.data_dir, Some(assets::router())))?;
    let address = format!("{}/#{}", session.url, session.token);
    tauri::Builder::default()
        .setup(move |app| {
            tauri::WebviewWindowBuilder::new(
                app,
                "composer",
                tauri::WebviewUrl::External(address.parse()?),
            )
            .title("Zihuan Composer · 组件管理")
            .inner_size(1180.0, 820.0)
            .min_inner_size(820.0, 600.0)
            .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .map_err(|error| error.to_string())?;
    drop(session);
    Ok(())
}
