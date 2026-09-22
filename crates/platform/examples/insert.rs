//! Manual check: `cargo run -p hushtype-platform --example insert -- type "Hello, world."`
//! Focus a text field within 3 seconds.
use hushtype_platform::{insert_text, InsertMethod, InsertOptions, NewlineMode};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let method = match args.get(1).map(|s| s.as_str()) {
        Some("paste") => InsertMethod::Paste,
        _ => InsertMethod::Type,
    };
    let text = args.get(2).cloned().unwrap_or_else(|| "Create a function that fetches the user profile.".into());
    std::thread::sleep(std::time::Duration::from_secs(3));
    let opts = InsertOptions { method, newline: NewlineMode::Enter, terminal_paste: false, restore_clipboard: true };
    println!("{:?}", insert_text(&text, &opts));
}
