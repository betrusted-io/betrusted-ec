use std::{
    env,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
};

type DynError = Box<dyn std::error::Error>;

const TARGET: &str = "riscv32i-unknown-none-elf";

fn main() {
    if let Err(e) = try_main() {
        eprintln!("{}", e);
        std::process::exit(-1);
    }
}

fn try_main() -> Result<(), DynError> {
    let task = env::args().nth(1);
    match task.as_deref() {
        Some("hw-image") => build_hw_image(false, env::args().nth(2))?,
        Some("docs") => make_docs()?,
        _ => print_help(),
    }
    Ok(())
}

fn print_help() {
    eprintln!(
        "Tasks:
hw-image [soc.svd]      builds an image for real hardware
docs                    Planned: updates the documentation tree
push                    Planned: deploys files to burner Rpi
update                  Planned: burns firmware to a a Precursor via USB
"
    )
}

fn make_docs() -> Result<(), DynError> {
    println!("placeholder function");

    Ok(())
}

fn build_hw_image(debug: bool, svd: Option<String>) -> Result<(), DynError> {
    let svd_file = match svd {
        Some(s) => s,
        None => {println!("Using default soc.svd location of build/software/soc.svd"); "build/software/soc.svd".to_string() },
    };

    let path = std::path::Path::new(&svd_file);
    if !path.exists() {
        return Err("svd file does not exist".into());
    }

    // Tools use this environment variable to know when to rebuild the UTRA crate.
    std::env::set_var("EC_SVD_FILE", path.canonicalize().unwrap());

    let sw = build_sw(debug)?;

    let loaderpath = PathBuf::from("sw/loader.S");
    let gatewarepath = PathBuf::from("build/gateware/top.bin");
    let output_bundle = create_image(&sw, &loaderpath, &gatewarepath)?;
    println!();
    println!(
        "EC software image bundle is available at {}",
        output_bundle.display()
    );

    Ok(())
}


fn build_sw(debug: bool) -> Result<PathBuf, DynError> {
    build("sw", debug, Some(TARGET), Some("sw".into()))
}

fn build(
    project: &str,
    debug: bool,
    target: Option<&str>,
    directory: Option<PathBuf>,
) -> Result<PathBuf, DynError> {
    println!("Building {}...", project);
    let stream = if debug { "debug" } else { "release" };
    let mut args = vec!["build", "--package", project];
    let mut target_path = "".to_owned();
    if let Some(t) = target {
        args.push("--target");
        args.push(t);
        target_path = format!("{}/", t);
    }

    if !debug {
        args.push("--release");
    }

    let mut dir = project_root();
    if let Some(subdir) = &directory {
        dir.push(subdir);
    }

    let status = Command::new(cargo())
        .current_dir(dir)
        .args(&args)
        .status()?;

    if !status.success() {
        return Err("cargo build failed".into());
    }

    Ok(project_root().join(&format!("target/{}{}/{}", target_path, stream, project)))
}

fn create_image(
    kernel: &Path,
    loader: &PathBuf,
    gateware: &PathBuf,
) -> Result<PathBuf, DynError> {
    let loader_bin_path = &format!("target/{}/release/loader.bin", TARGET);
    let kernel_bin_path = &format!("target/{}/release/kernel.bin", TARGET);
    let image_path = &format!("target/{}/release/bt-ec.bin", TARGET);
    // kernel region limit primarily set by the loader copy bytes. Can be grown, at expense of heap.
    const KERNEL_REGION: usize = 48 * 1024;
    // this is defined by size of UP5k bitstream plus rounding to sector erase size of 4k; reset vector points just beyond this
    const GATEWARE_REGION: usize = 104 * 1024;

    //let temp = loader.clone();
    //println!("attempt to assemble {:?}", temp.into_os_string());
    let loader_orig = loader.clone();
    let mut loader_elf = loader.clone();
    loader_elf.pop();
    loader_elf.push("loader.elf");
    // assemble the loader into an ELF file
    Command::new("riscv64-unknown-elf-as")
    .arg("-fpic")
    .arg(loader_orig.into_os_string())
    .arg("-o")
    .arg(loader_elf.into_os_string())
    .output()
    .expect("Failed to assemble the loader");

    // copy the ELF into a bin target
    let tmp = PathBuf::from(loader_bin_path);
    let mut loader_elf = loader.clone();
    loader_elf.pop();
    loader_elf.push("loader.elf");
    Command::new("riscv64-unknown-elf-objcopy")
    .arg("-O")
    .arg("binary")
    .arg(loader_elf.into_os_string())
    .arg(tmp.into_os_string())
    .output()
    .expect("Failed to copy loader binary");

    // extend the loader binary to 4096 bytes by padding with 0's
    let mut loader: [u8; 4096] = [0; 4096];
    std::fs::File::open(PathBuf::from(&loader_bin_path))?.read(&mut loader)?;
    std::fs::write(PathBuf::from(&loader_bin_path), loader)?;

    // objcopy the target sw into a binary format
    Command::new("riscv64-unknown-elf-objcopy")
    .arg("-O").arg("binary")
    .arg(kernel)
    .arg(PathBuf::from(&kernel_bin_path))
    .output()
    .expect("Failed to copy the kernel binary");

    // 104k region for gateware
    let mut gateware_bin: [u8; GATEWARE_REGION] = [0; GATEWARE_REGION];
    // kernel bin can be no longer than 48k, due to limitation on loader size
    let mut kernel_bin: [u8; KERNEL_REGION] = [0; KERNEL_REGION];

    std::fs::File::open(gateware)?.read(&mut gateware_bin)?;
    let kernel_bytes = std::fs::File::open(PathBuf::from(&kernel_bin_path))?.read(&mut kernel_bin);
    match kernel_bytes {
        Ok(bytes) => {
            println!("Read {} kernel bytes into image.", bytes);
            if bytes == KERNEL_REGION {
                println!("WARNING: kernel may be truncated.");
            }
        },
        _ => {
            println!("Error in reading kernel");
        }
    }

    let mut image = std::fs::File::create(PathBuf::from(&image_path))?;
    image.write(&gateware_bin)?;
    image.write(&loader)?;
    image.write(&kernel_bin)?;

    Ok(project_root().join(&image_path))
}

fn cargo() -> String {
    env::var("CARGO").unwrap_or_else(|_| "cargo".to_string())
}

fn project_root() -> PathBuf {
    Path::new(&env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(1)
        .unwrap()
        .to_path_buf()
}
