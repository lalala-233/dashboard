use anyhow::Result;
use scraper::{Html, Selector};
use std::{fmt::Display, time::Duration};
use tokio::time::sleep;

const MAX_RETRIES: usize = 3;
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

async fn fetch_aur_stats() -> Result<Vec<String>> {
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
            results.extend(table_data);
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

#[tokio::main]
async fn main() -> Result<()> {
    let repos = ["Core", "Extra", "Multilib"];
    let aur_task = tokio::spawn(fetch_aur_stats());
    let mut repo_tasks = Vec::new();
    for repo in repos {
        let task = tokio::spawn(async move {
            let result = fetch_package_count(repo)
                .await
                .inspect_err(|e| eprintln!("Failed to fetch {repo} count: {e}"))
                .ok();
            (repo, result)
        });
        repo_tasks.push(task);
    }

    let aur_results = aur_task.await??;

    let mut repo_counts = Vec::new();
    for task in repo_tasks {
        repo_counts.push(task.await?);
    }

    println!("{:<35} Value", "Statistic");
    println!("{}", "-".repeat(50));
    for [desc, value] in aur_results.as_chunks().0 {
        println!("{desc:<35} {value}");
    }

    println!("\n{:<35} Value", "Official Repository");
    println!("{}", "-".repeat(50));
    let mut total = 0;
    for (name, count_opt) in repo_counts {
        if let Some(count) = count_opt {
            println!("{name:<35} {count}");
            total += count;
        } else {
            println!("{name:<35} Error");
        }
    }
    println!("{:<35} {}", "Total", total);
    Ok(())
}
