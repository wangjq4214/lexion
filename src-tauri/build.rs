fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("windows-common-controls.manifest");
        println!("cargo:rerun-if-changed={}", manifest.display());
        // The library unit-test harness also links Tauri's TaskDialogIndirect import.
        // It needs Common Controls v6 before any test can start.
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        // Tauri's default resource already contains this manifest for the app binary.
        // Use the linker input for both app and unit-test executables, without a
        // duplicate manifest resource in the app binary.
        tauri_build::try_build(
            tauri_build::Attributes::new()
                .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest()),
        )
        .expect("Tauri build failed");
    } else {
        tauri_build::build();
    }
}
