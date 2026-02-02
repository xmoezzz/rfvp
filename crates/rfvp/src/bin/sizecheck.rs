use std::mem::size_of;
fn main() {
    println!("size App: {}", size_of::<rfvp::app::App>());
    println!("size AppBuilder: {}", size_of::<rfvp::app::AppBuilder>());
    println!("size BuiltApp: {}", size_of::<rfvp::app::BuiltApp>());
}
