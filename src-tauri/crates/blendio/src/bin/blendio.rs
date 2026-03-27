fn main() {
    if let Err(error) = blendio::cli::run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
