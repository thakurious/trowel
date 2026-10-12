use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn image_sizing_matches_pillow() {
    let root = workspace_root();
    let candidates = [
        root.join("cpp/build-sizing/trowel-image-sizing-test"),
        root.join("cpp/build-vulkan/trowel-image-sizing-test"),
        root.join("cpp/build-cpu/trowel-image-sizing-test"),
    ];
    let Some(bin) = candidates.into_iter().find(|p| p.is_file()) else {
        let status = Command::new("cmake")
            .args([
                "-S",
                &root.join("cpp").display().to_string(),
                "-B",
                &root.join("cpp/build-sizing").display().to_string(),
                "-G",
                "Ninja",
                "-DTROWEL_BUILD_WORKER=OFF",
                "-DCMAKE_BUILD_TYPE=Release",
            ])
            .status()
            .expect("cmake configure");
        assert!(status.success(), "cmake configure failed");
        let status = Command::new("cmake")
            .args([
                "--build",
                &root.join("cpp/build-sizing").display().to_string(),
                "--target",
                "trowel-image-sizing-test",
            ])
            .status()
            .expect("cmake build");
        assert!(status.success(), "cmake build failed");
        let bin = root.join("cpp/build-sizing/trowel-image-sizing-test");
        let out = Command::new(&bin).output().expect("run image-sizing-test");
        assert!(
            out.status.success(),
            "image-sizing-test failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        return;
    };
    let out = Command::new(&bin).output().expect("run image-sizing-test");
    assert!(
        out.status.success(),
        "image-sizing-test failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
