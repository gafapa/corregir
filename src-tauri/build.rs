use sha2::{Digest, Sha256};

fn main() {
    assert_eq!(std::env::var("TARGET").unwrap(), "x86_64-pc-windows-msvc", "This resource bundle supports Windows x64 MSVC only; other platforms need native resources and a separate dependency security review.");
    let root = std::path::Path::new("resources");
    let manifest_path = root.join("manifest.json");
    println!("cargo:rerun-if-changed={}", manifest_path.display());
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(manifest_path).expect("missing bundled resource manifest"),
    )
    .expect("invalid bundled resource manifest");
    for resource in manifest["resources"]
        .as_array()
        .expect("missing resource list")
    {
        let relative = resource["path"].as_str().expect("missing resource path");
        let path = root.join(relative);
        println!("cargo:rerun-if-changed={}", path.display());
        let bytes = std::fs::read(&path).expect("missing bundled resource");
        assert_eq!(
            bytes.len() as u64,
            resource["size_bytes"].as_u64().unwrap(),
            "resource size changed: {relative}"
        );
        assert_eq!(
            format!("{:x}", Sha256::digest(&bytes)),
            resource["sha256"].as_str().unwrap(),
            "resource hash changed: {relative}"
        );
    }
    tauri_build::build()
}
