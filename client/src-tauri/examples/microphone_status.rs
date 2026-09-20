//! Prints what macOS currently thinks about us and the microphone,
//! without asking for anything. Diagnostic only: the interesting case is
//! "Denied", which no amount of retrying inside the app can fix.
//!
//!     cargo run --example microphone_status
fn main() {
    println!("{:?}", client_lib::microphone_permission());
}
