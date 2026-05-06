use anyhow::Result;
use scraper::{Html, Selector};

async fn fetch_package_count(url: &str) -> Result<u32> {
    let body = reqwest::get(url).await?.text().await?;
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
    let body = reqwest::get(url).await?.text().await?;
    let document = Html::parse_document(&body);

    let div_selector = Selector::parse("div#pkg-stats").expect("Failed to parse stats.");
    let stats_div = document
        .select(&div_selector)
        .next()
        .ok_or_else(|| anyhow::anyhow!("未找到 id='pkg-stats' 的 div"))?;

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
    let repos = [
        (
            "Core",
            "https://archlinux.org/packages/?sort=&repo=Core&q=&maintainer=&flagged=",
        ),
        (
            "Extra",
            "https://archlinux.org/packages/?sort=&repo=Extra&q=&maintainer=&flagged=",
        ),
        (
            "Multilib",
            "https://archlinux.org/packages/?sort=&repo=Multilib&q=&maintainer=&flagged=",
        ),
    ];

    let aur_task = tokio::spawn(fetch_aur_stats());
    let mut repo_tasks = Vec::new();
    for (name, url) in repos {
        let task = tokio::spawn(async move {
            match fetch_package_count(url).await {
                Ok(count) => (name, Some(count)),
                Err(e) => {
                    eprintln!("Failed to fetch {name} count: {e}");
                    (name, None)
                }
            }
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
