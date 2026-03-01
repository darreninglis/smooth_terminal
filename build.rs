use std::fs;

fn main() {
    // Rerun whenever Cargo.toml changes (covers version bumps).
    println!("cargo:rerun-if-changed=Cargo.toml");

    // Read the current version from Cargo.toml so we can bake it into the binary
    // and then bump the patch number for the *next* build.
    let cargo_toml = fs::read_to_string("Cargo.toml").expect("could not read Cargo.toml");

    // Extract the current version string (e.g. "0.1.84").
    let version_line = cargo_toml
        .lines()
        .find(|l| l.starts_with("version"))
        .expect("no version in Cargo.toml");
    let version_str = version_line
        .split('"')
        .nth(1)
        .expect("malformed version line");

    // Bake the current version into the binary.
    println!("cargo:rustc-env=APP_VERSION={}", version_str);

    // Parse major.minor.patch and bump patch for the next build.
    let parts: Vec<&str> = version_str.split('.').collect();
    if parts.len() == 3 {
        let major = parts[0];
        let minor = parts[1];
        let patch: u32 = parts[2].parse().unwrap_or(0);
        let next_version = format!("{}.{}.{}", major, minor, patch + 1);

        // Rewrite Cargo.toml with the incremented version.
        let new_toml = cargo_toml.replace(
            &format!("version = \"{}\"", version_str),
            &format!("version = \"{}\"", next_version),
        );
        fs::write("Cargo.toml", new_toml).expect("could not write Cargo.toml");
    }
}
