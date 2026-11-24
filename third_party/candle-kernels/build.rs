use cudaforge::{CudaToolkit, KernelBuilder, Result};
use std::env;
use std::path::PathBuf;

fn main() -> Result<()> {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=src/compatibility.cuh");
    println!("cargo::rerun-if-changed=src/cuda_utils.cuh");
    println!("cargo::rerun-if-changed=src/binary_op_macros.cuh");

    let is_target_msvc = env::var("TARGET")
        .map(|target| target.contains("msvc"))
        .unwrap_or(false);
    let needs_conforming_preprocessor = is_target_msvc && needs_zc_preprocessor();

    // Build for PTX
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let ptx_path = out_dir.join("ptx.rs");
    let mut ptx_builder = KernelBuilder::new()
        .source_dir("src") // Scan src/ for .cu files
        .exclude(&["moe_*.cu"]) // Exclude moe kernels for ptx build
        .arg("--expt-relaxed-constexpr")
        .arg("-std=c++17")
        .arg("-O3");
    if needs_conforming_preprocessor {
        ptx_builder = ptx_builder.arg("-Xcompiler").arg("/Zc:preprocessor");
    }
    let bindings = ptx_builder.build_ptx()?;

    bindings.write(&ptx_path)?;

    let mut moe_builder = KernelBuilder::default()
        .source_files(vec![
            "src/moe/moe_gguf.cu",
            "src/moe/moe_wmma.cu",
            "src/moe/moe_wmma_gguf.cu",
        ])
        .arg("--expt-relaxed-constexpr")
        .arg("-std=c++17")
        .arg("-O3");

    // Disable bf16 WMMA kernels on GPUs older than sm_80 (Ampere).
    // bf16 WMMA fragments require compute capability >= 8.0.
    let compute_cap = cudaforge::detect_compute_cap()
        .map(|arch| arch.base())
        .unwrap_or(80);
    if compute_cap < 80 {
        moe_builder = moe_builder.arg("-DNO_BF16_KERNEL");
    }

    if is_target_msvc {
        moe_builder = moe_builder.arg("-D_USE_MATH_DEFINES");
        if needs_conforming_preprocessor {
            moe_builder = moe_builder.arg("-Xcompiler").arg("/Zc:preprocessor");
        }
    } else {
        moe_builder = moe_builder.arg("-Xcompiler").arg("-fPIC");
    }

    moe_builder.build_lib(out_dir.join("libmoe.a"))?;
    println!("cargo:rustc-link-search={}", out_dir.display());
    println!("cargo:rustc-link-lib=moe");
    println!("cargo:rustc-link-lib=dylib=cudart");
    if !is_target_msvc {
        println!("cargo:rustc-link-lib=stdc++");
    }
    Ok(())
}

/// CUDA 13.1+ ships CCCL headers that fail hard when cl.exe runs with the
/// traditional preprocessor, and nvcc does not forward `/Zc:preprocessor` to
/// the host compiler on its own (huggingface/candle#3686). Pass it explicitly
/// for the kernels that include CCCL headers (`reduce.cu` includes
/// `<cuda/std/limits>`, the moe kernels include `<thrust/...>`).
///
/// Keeping the gate at CUDA 13 leaves older toolchains (e.g. the CUDA 12.6 CI
/// image) on their previous flags.
fn needs_zc_preprocessor() -> bool {
    CudaToolkit::detect()
        .ok()
        .and_then(|toolkit| toolkit.version)
        .and_then(|version| version.split('.').next().map(str::to_owned))
        .and_then(|major| major.parse::<u32>().ok())
        .map(|major| major >= 13)
        .unwrap_or(false)
}
