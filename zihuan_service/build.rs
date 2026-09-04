use std::env;

fn main() {
    println!("cargo:rerun-if-env-changed=GITHUB_REF_NAME");
    println!("cargo:rerun-if-env-changed=GITHUB_REF_TYPE");

    let version = match (env::var("GITHUB_REF_TYPE"), env::var("GITHUB_REF_NAME")) {
        (Ok(reference_type), Ok(reference_name))
            if reference_type == "tag" && !reference_name.trim().is_empty() =>
        {
            reference_name
        }
        _ => env!("CARGO_PKG_VERSION").to_owned(),
    };

    println!("cargo:rustc-env=ZIHUAN_BUILD_VERSION={version}");
}
