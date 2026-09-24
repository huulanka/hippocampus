// The app's Rust side calls the Swift plugin directly (see `Speech`); the
// webview only listens, for the Action Button (`record` events), which is
// what these two built-in commands are for.
const COMMANDS: &[&str] = &["register_listener", "remove_listener"];

fn main() {
    tauri_plugin::Builder::new(COMMANDS).ios_path("ios").build();

    // What FluidAudio needs from the system and that Swift's autolinking
    // does not carry across into a Rust link: its clustering code is C++,
    // and the model runs through CoreML with Accelerate underneath.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("ios") {
        println!("cargo:rustc-link-lib=c++");
        for framework in [
            "AVFoundation",
            "Accelerate",
            "CoreML",
            "CoreAudio",
            "AudioToolbox",
        ] {
            println!("cargo:rustc-link-lib=framework={framework}");
        }
    }
}
