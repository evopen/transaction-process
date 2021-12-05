use clap::{App, Arg};

fn main() {
    let matches = App::new("transaction process")
        .arg(
            Arg::new("start_date")
                .long("start")
                .default_value("1970-1-1"),
        )
        .arg(
            Arg::new("end_date")
                .long("end")
                .default_value(&chrono::Local::today().format("%F").to_string()),
        )
        .arg(
            Arg::new("files")
                .short('f')
                .long("file")
                .multiple_occurrences(true),
        )
        .get_matches();
}
