//! Renders one project track through the Cecilia adapter.
//! `cargo run -p score --example cecilia_demo -- PROJECT TRACK OUT.pdf [grand]`

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let json = std::fs::read_to_string(&args[1]).expect("project");
    let project = project::ProjectFile::from_json(&json).expect("parse");
    let track: usize = args[2].parse().expect("track");
    let grand = args.get(4).is_none_or(|value| value != "single");
    let pdf = score::cecilia::render_pdf(&project.song, track, &project.song.name, grand)
        .expect("render");
    std::fs::write(&args[3], pdf).expect("write");
}
