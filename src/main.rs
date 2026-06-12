use anyhow::Result;
use chrono::{SecondsFormat, Utc};
use futures_util::future::join_all;
use scraper::{Html, Selector};
use std::{env::args, fmt::Display, fs::File, io::Write, time::Duration};
use tokio::time::sleep;

const MAX_RETRIES: usize = 3;
type Statistic = (String, Vec<(String, String)>);
async fn retry<T, E, F, Fut>(f: F, max_retries: usize) -> Result<T, E>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: Display,
{
    let mut last_err: Option<E> = None;
    for attempt in 1..=max_retries {
        match f().await {
            Ok(val) => return Ok(val),
            Err(e) => {
                eprintln!("Attempt {attempt}/{max_retries} failed: {e}");
                last_err = Some(e);
                if attempt < max_retries {
                    sleep(Duration::from_millis(100 << attempt)).await;
                }
            }
        }
    }
    Err(last_err.unwrap())
}

// Use API to get official package count
//
// Slower because ArchLinux has rate limit.
async fn _fetch_official_package_count_by_api(repo: &str) -> Result<u32> {
    let url = format!("https://archlinux.org/packages/search/json/?repo={repo}&limit=1");
    let body = retry(async || reqwest::get(&url).await?.text().await, MAX_RETRIES).await?;

    let key = "\"count\":";
    let rest = body
        .rsplit(key)
        .next()
        .ok_or_else(|| anyhow::anyhow!("No `count` field in {repo} json"))?;
    let count_str = rest
        .split(',')
        .next()
        .unwrap_or(rest)
        .trim_start()
        .trim_end_matches('}');
    Ok(count_str.parse()?)
}
async fn fetch_package_count(repo: &str) -> Result<u32> {
    let url = format!("https://archlinux.org/packages/?sort=&repo={repo}&q=&maintainer=&flagged=");
    let body = retry(async || reqwest::get(&url).await?.text().await, MAX_RETRIES).await?;
    let document = Html::parse_document(&body);

    let div_selector = Selector::parse("div.pkglist-stats").unwrap();
    let stats_div = document
        .select(&div_selector)
        .next()
        .ok_or_else(|| anyhow::anyhow!("未找到 pkglist-stats div"))?;

    let p_selector = Selector::parse("p").unwrap();
    let p_text = stats_div
        .select(&p_selector)
        .next()
        .ok_or_else(|| anyhow::anyhow!("未找到 p 标签"))?
        .text()
        .collect::<String>();

    let count = p_text
        .split_whitespace()
        .next()
        .ok_or_else(|| anyhow::anyhow!("文本为空"))?
        .parse::<u32>()?;
    Ok(count)
}

async fn fetch_aur_stats() -> Result<Vec<(String, String)>> {
    let url = "https://aur.archlinux.org/";
    let body = retry(async || reqwest::get(url).await?.text().await, MAX_RETRIES).await?;
    let document = Html::parse_document(&body);

    let div_selector = Selector::parse("div#pkg-stats")
        .map_err(|_| anyhow::anyhow!("Failed to parse aur stats."))?;

    let stats_div = document
        .select(&div_selector)
        .next()
        .ok_or_else(|| anyhow::anyhow!("Could not find div with `id='pkg-stats'`"))?;

    let row_selector = Selector::parse("tr").unwrap();
    let rows = stats_div.select(&row_selector);

    let mut results = Vec::new();
    for row in rows {
        let table_data: Vec<_> = row
            .select(&Selector::parse("td").unwrap())
            .map(|data| data.text().collect::<String>().trim().to_string())
            .collect();
        if table_data.len() == 2 {
            results.push((table_data[0].clone(), table_data[1].clone()));
        } else {
            eprintln!(
                "Error: Unexpected number of columns in row: {:?}",
                table_data.len()
            );
            eprintln!("{table_data:#?}");
        }
    }
    Ok(results)
}

async fn fetch_statistics() -> Result<Vec<Statistic>> {
    let repos = ["Core", "Extra", "Multilib"].map(std::string::ToString::to_string);
    let aur_task = tokio::spawn(fetch_aur_stats());
    let mut repo_tasks = Vec::new();
    for repo in repos {
        let task = async move {
            let result = fetch_package_count(&repo)
                .await
                .map_or_else(|_| "Error".to_string(), |count| count.to_string());
            (repo, result)
        };
        repo_tasks.push(task);
    }

    Ok(vec![
        (
            "Official Repository".to_string(),
            join_all(repo_tasks).await,
        ),
        ("AUR".to_string(), aur_task.await??),
    ])
}
fn print_table(results: Vec<Statistic>) {
    for (name, data) in results {
        println!("{}", "-".repeat(50));
        println!("{name:<35} Value");
        println!("{}", "-".repeat(50));
        for (subname, value) in data {
            println!("{subname:<35} {value}");
        }
        println!("{}\n", "-".repeat(50));
    }
}

fn write_csv(results: Vec<Statistic>, mut file: File) -> Result<()> {
    let now = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    for (name, data) in results {
        for (subname, value) in data {
            writeln!(file, "{now},{name},{subname},{value}")?;
        }
    }
    Ok(())
}
#[tokio::main]
async fn main() -> Result<()> {
    let results = fetch_statistics().await?;
    let mut args = args();
    args.next();
    if args.next() == Some("--csv".to_string()) {
        let path = args
            .next()
            .ok_or_else(|| anyhow::anyhow!("No path provided"))?;
        let file = File::options()
            .append(true)
            .open(&path)
            .map_err(|_| anyhow::anyhow!("Failed to open {path}"))?;
        write_csv(results, file)?;
    } else {
        print_table(results);
    }

    Ok(())
}
