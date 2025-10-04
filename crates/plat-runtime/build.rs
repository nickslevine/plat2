fn main() {
    // Link against system libgc (Boehm GC)
    println!("cargo:rustc-link-lib=gc");

    // Platform-specific library paths
    #[cfg(target_os = "macos")]
    {
        // Homebrew on Apple Silicon
        println!("cargo:rustc-link-search=/opt/homebrew/lib");
        // Homebrew on Intel
        println!("cargo:rustc-link-search=/usr/local/lib");
    }

    #[cfg(target_os = "linux")]
    println!("cargo:rustc-link-search=/usr/lib/x86_64-linux-gnu");
}
