fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(crew_wiki::run_cli(args));
}
