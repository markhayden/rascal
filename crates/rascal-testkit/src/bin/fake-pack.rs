//! The fake tool pack as a program; see `rascal_testkit::fake_pack`.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(rascal_testkit::fake_pack::main(&args));
}
