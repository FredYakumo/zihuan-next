use clap::Parser;
use std::path::PathBuf;
use zihuan_composer::{cli, tui, Session};

#[derive(Parser)]
#[command(
    version = concat!(
        env!("CARGO_PKG_VERSION"),
        " · ",
        env!("ZIHUAN_BUILD_VERSION"),
        " · ",
        env!("ZIHUAN_BUILD_TYPE")
    ),
    about = "紫幻 Composer",
    after_help = "示例：zihuan_composer help install"
)]
struct Args {
    /// 指定组件及连接配置的数据目录
    #[arg(long, global = true)]
    data_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<cli::Command>,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let result = async {
        let session = Session::start(args.data_dir, None).await?;
        match args.command {
            None | Some(cli::Command::Tui) => tui::run(&session.url, &session.token).await,
            command => cli::run(command, &session.url, &session.token).await,
        }
    }
    .await;
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
