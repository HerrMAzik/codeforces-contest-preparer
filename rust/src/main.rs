use anyhow::ensure;
use clap::Clap;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::{fs, fs::create_dir_all, path::PathBuf, thread, time::Duration};
use tinytemplate::TinyTemplate;
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

#[derive(Serialize)]
struct TestContext<'a> {
    tests: Vec<(String, String)>,
    solve: &'a str,
    main: &'a str,
    scanner: &'a str,
}

static CARGO_TOML: &'static str = r#"
[package]
name = "main"
version = "0.1.0"
edition = "2018"
"#;

static SOLVE: &'static str = r#"
fn solve(sc: &mut scanner::Scanner, out: &mut impl Write) {
    let t = sc.next();
    for _ in 0..t {

        writeln!(out, "{}", 1).unwrap();
    }
}
"#;

static MAIN: &'static str = r#"
fn main() {
    let in_string = scanner::read_string();
    let mut scanner = scanner::Scanner::new(&in_string);
    let out = stdout();
    let mut out = BufWriter::new(out.lock());
    solve(&mut scanner, &mut out);
}
"#;

static SCANNER: &'static str = r#"
#[allow(dead_code)]
mod scanner {
    use std;
    use std::io::{stdin, Read};
    use std::str::{FromStr, SplitWhitespace};
    pub struct Scanner<'a> {
        it: SplitWhitespace<'a>,
    }
    impl<'a> Scanner<'a> {
        pub fn new(s: &'a str) -> Self {
            Self {
                it: s.split_whitespace(),
            }
        }
        pub fn next<T: FromStr>(&mut self) -> T {
            match self.it.next().unwrap().parse::<T>() {
                Ok(v) => v,
                _ => panic!("Scanner error"),
            }
        }
        pub fn next_chars(&mut self) -> Vec<char> {
            self.next::<String>().chars().collect()
        }
        pub fn next_vec<T: FromStr>(&mut self, len: usize) -> Vec<T> {
            (0..len).map(|_| self.next()).collect()
        }
    }
    pub fn read_string() -> String {
        let mut s = String::new();
        stdin().read_to_string(&mut s).unwrap();
        s
    }
}
"#;

static TEMPLATE: &'static str = r##"
use std::io::*;
{solve}
#[cfg(test)]
mod tests \{
    use crate::*;
    use std::io::BufWriter;

    {{ for test in tests }}
    #[test]
    fn test{@index}() \{
        let in_string = r#"{test | vec0}"#.trim_start();
        let expected = r#"{test | vec1}"#;
        let mut sc = scanner::Scanner::new(in_string);
        let buf = Vec::new();
        let mut out = BufWriter::new(buf);
        solve(&mut sc, &mut out);

        let result = unsafe \{ String::from_utf8_unchecked(out.into_inner().unwrap()) };
        assert_eq!(result.trim_end(), expected.trim());
    }
    {{ endfor }}
}
{main}
{scanner}
"##;

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

    let mut tt = TinyTemplate::new();
    tt.set_default_formatter(&tinytemplate::format_unescaped);
    tt.add_template("tests", TEMPLATE)?;

    let array_formatter = |x: usize| {
        move |v: &SerdeValue, s: &mut String| -> tinytemplate::error::Result<()> {
            let v = match v {
                SerdeValue::Array(vec) => &vec[x],
                _ => panic!("1"),
            };
            match v {
                SerdeValue::String(v) => {
                    s.push('\n');
                    s.push_str(v);
                }
                _ => panic!("2"),
            }
            Ok(())
        }
    };

    tt.add_formatter("vec0", array_formatter(0));
    tt.add_formatter("vec1", array_formatter(1));
    let sample_selector = Selector::parse(".sample-test").unwrap();
    let input_selector = Selector::parse(".input pre").unwrap();
    let output_selector = Selector::parse(".output pre").unwrap();
    let mut context = TestContext {
        tests: Vec::new(),
        solve: SOLVE,
        main: MAIN,
        scanner: SCANNER.trim_end(),
    };
    for problem in res.problems {
        println!("Processing: {}", problem.index);
        let url = format!(
            "https://codeforces.com/contest/{}/problem/{}",
            opts.contest, problem.index
        );
        let problem_dir = path.join(problem.index);
        let src_dir = problem_dir.join("src");
        create_dir_all(&src_dir)?;
        fs::write(problem_dir.join("Cargo.toml"), CARGO_TOML.trim_start())?;

        let doc = agent.get(&url).call().into_string()?;
        let doc = Html::parse_document(&doc);
        context.tests = doc
            .select(&sample_selector)
            .flat_map(|s| s.select(&input_selector).zip(s.select(&output_selector)))
            .map(|(input, output)| (input.inner_html(), output.inner_html()))
            .collect();
        let rendered = tt.render("tests", &context)?;
        fs::write(src_dir.join("main.rs"), rendered.trim_start())?;
    }
    Ok(())
}
