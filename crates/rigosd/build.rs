fn main() {
    let commit = std::env::var("RIGOS_BUILD_COMMIT").unwrap_or_else(|_| "unknown".into());
    let target = std::env::var("TARGET").unwrap_or_else(|_| "unknown".into());
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "unknown".into());
    println!("cargo:rustc-env=RIGOS_BUILD_COMMIT={commit}");
    println!("cargo:rustc-env=RIGOS_BUILD_TARGET={target}");
    println!("cargo:rustc-env=RIGOS_BUILD_PROFILE={profile}");
    println!("cargo:rerun-if-env-changed=RIGOS_BUILD_COMMIT");
    for name in [
        "RIGOS_PRODUCT_VERSION",
        "RIGOS_IMAGE_ID",
        "RIGOS_IMAGE_VERSION",
        "RIGOS_IMAGE_CHANNEL",
    ] {
        if let Ok(value) = std::env::var(name) {
            println!("cargo:rustc-env={name}={value}");
        }
        println!("cargo:rerun-if-env-changed={name}");
    }
}
