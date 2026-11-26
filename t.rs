use std::panic;
fn main() {
    let h = "😀😀";
    let r = panic::catch_unwind(|| {
        if h.len() != 6 { return None::<u8>; }
        let _ = u8::from_str_radix(&h[0..2], 16);
        Some(0u8)
    });
    println!("panic={}", r.is_err());
}
