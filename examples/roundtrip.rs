//! Load a UFO with norad and save it straight back, to measure the
//! on-disk churn of the editor's native save path. Run against a
//! scratch copy under git, then read the diff.
fn main() {
    let path = std::env::args().nth(1).expect("usage: roundtrip <font.ufo>");
    let font = norad::Font::load(&path).expect("load");
    font.save(&path).expect("save");
    println!("round-tripped {path}");
}
