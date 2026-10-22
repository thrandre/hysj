//! Checks the FINN parser against a real alert: `cargo run -q -p hysj-core
//! --example finn_alert < alert.html` prints what it reads (kind, title, price,
//! place). For catching template changes; prints no addresses or links.
fn main() {
    let mut html = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut html).expect("stdin");
    for l in hysj_core::finn::listings(&html) {
        println!(
            "{:<6} {:<55.55} {:>14} | {:<20.20} | photo: {}",
            l.kind,
            l.title,
            l.price,
            l.place,
            l.image.is_some()
        );
    }
}
