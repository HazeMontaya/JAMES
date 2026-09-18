fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::env;
    use std::path::PathBuf;

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let proto_dir = manifest_dir.join("proto");
    let proto_path = proto_dir.join("james_runtime.proto");

    if !proto_path.exists() {
        return Err(format!("Proto file not found at: {:?}", proto_path).into());
    }

    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    println!("cargo:warning=Using vendored protoc: {}", protoc.display());
    env::set_var("PROTOC", protoc);

    tonic_build::configure()
        .compile(&[&proto_path], &[&proto_dir])?;

    let out_dir = env::var("OUT_DIR")?;
    for entry in std::fs::read_dir(&out_dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".rs") {
            println!("cargo:warning=Generated: {}", name);
        }
    }

    println!("cargo:rerun-if-changed={}", proto_path.display());
    Ok(())
}
