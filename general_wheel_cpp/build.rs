use std::env;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("missing manifest dir"));
    let include_dir = manifest_dir
        .join("native")
        .join("general-wheel-cpp")
        .join("include");
    let src_dir = manifest_dir.join("src");

    println!(
        "cargo:rerun-if-changed={}",
        src_dir.join("ffi.cpp").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        include_dir
            .join("linalg_boost")
            .join("linalg_boost.hpp")
            .display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        include_dir
            .join("linalg_boost")
            .join("vec_ops.hpp")
            .display()
    );

    let mut build = cc::Build::new();
    build.cpp(true);
    build.std("c++17");
    build.file(src_dir.join("ffi.cpp"));
    build.include(&include_dir);

    if env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("aarch64") {
        build.define("LINALG_USE_NEON", None);
    }

    build.static_crt(true);
    build.compile("general_wheel_cpp");
}
