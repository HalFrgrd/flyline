
use ratatui::buffer::Cell;
fn main() {
    let c = Cell::default();
    println!("Default symbol: '{}'", c.symbol());
    let mut c2 = Cell::default();
    c2.set_symbol("X");
    c2.reset();
    println!("Reset symbol: '{}'", c2.symbol());
}
