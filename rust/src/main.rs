use anyhow::ensure;
use clap::Clap;
use scraper::{Html, Selector};
use serde::Deserialize;
use std::{fs, fs::create_dir_all, path::PathBuf, thread, time::Duration};
use ureq::*;

#[derive(Debug, Deserialize, PartialEq)]
enum Status {
    OK,
    FAILED,
}

#[derive(Debug, Deserialize)]
struct Contest {
    id: i32,
    name: String,
}

#[derive(Debug, Deserialize)]
struct Problem {
    #[serde(rename(deserialize = "contestId"))]
    contest_id: i32,
    index: String,
    name: String,
    r#type: String,
    points: Option<f32>,
    rating: Option<f32>,
}

#[derive(Debug, Deserialize)]
struct Result {
    contest: Contest,
    problems: Vec<Problem>,
}

#[derive(Debug, Deserialize)]
struct Response {
    status: Status,
    comment: Option<String>,
    result: Option<Result>,
}

#[derive(Clap)]
struct Opts {
    contest: i32,
    path: String,
}

const CARGO_TOML: &str = r#"[package]
name = "main"
version = "0.1.0"
edition = "2018"
"#;
const MAIN_RS: &str = r#"fn main() {

}
"#;

fn main() -> anyhow::Result<()> {
    let opts: Opts = Opts::parse();
    let mut path = PathBuf::from(opts.path);
    path.push(opts.contest.to_string());
    ensure!(
        !path.exists(),
        format!("working path {} already exists", path.display())
    );
    create_dir_all(&path)?;

    let url = format!(
        "https://codeforces.com/api/contest.standings?contestId={}&from=1&count=1",
        opts.contest
    );

    let agent = Agent::new();
    let timeout = Duration::from_millis(500);
    let res = loop {
        let resp: Response = agent.get(&url).call().into_json_deserialize()?;
        if resp.status == Status::OK {
            break resp.result.unwrap();
        }
        thread::sleep(timeout);
    };

    let sample_selector = Selector::parse(".sample-test").unwrap();
    let input_selector = Selector::parse(".input pre").unwrap();
    let output_selector = Selector::parse(".output pre").unwrap();
    for problem in res.problems {
        println!("Processing: {}", problem.index);
        let url = format!(
            "https://codeforces.com/contest/{}/problem/{}",
            opts.contest, problem.index
        );
        let problem_dir = path.join(problem.index);
        let src_dir = problem_dir.join("src");
        create_dir_all(&src_dir)?;
        fs::write(problem_dir.join("Cargo.toml"), CARGO_TOML)?;
        fs::write(src_dir.join("main.rs"), MAIN_RS)?;
        let doc = agent.get(&url).call().into_string()?;
        let doc = Html::parse_document(&doc);
        for sample in doc.select(&sample_selector) {
            for input in sample.select(&input_selector) {
                let _ = input.inner_html();
            }
            for output in sample.select(&output_selector) {
                let _ = output.inner_html();
            }
        }
    }
    Ok(())
}
