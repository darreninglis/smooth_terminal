use std::fs;
use std::path::Path;

fn main() {
    let path = Path::new("build_number.txt");

    // Rerun whenever build_number.txt changes — which is every build, because
    // we write to it below, guaranteeing the counter always increments.
    println!("cargo:rerun-if-changed=build_number.txt");

    // Read the current build number (seed to 1 if the file is missing).
    let current: u32 = fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(1);

    // Bake the number into the binary as a compile-time env var.
    println!("cargo:rustc-env=BUILD_NUMBER={}", current);

    // Write the incremented value so the next build gets current + 1.
    fs::write(path, format!("{}\n", current + 1))
        .expect("could not write build_number.txt");
}
