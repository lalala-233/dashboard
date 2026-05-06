use anyhow::Result;
use scraper::{Html, Selector};

fn fetch_package_count(url: &str) -> Result<u32> {
    let body = reqwest::blocking::get(url)?.text()?;
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

    // 文本示例："295 matching packages found.\nPage 1 of 3."
    // 按空白字符分割，第一个单词就是数字
    let first_word = p_text
        .split_whitespace()
        .next()
        .ok_or_else(|| anyhow::anyhow!("文本为空"))?;

    let count = first_word.parse::<u32>()?;
    Ok(count)
}
fn main() -> Result<()> {
    let url = "https://aur.archlinux.org/";
    let body = reqwest::blocking::get(url)?.text()?;

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

    println!("{:<35} Value", "Statistic");
    println!("{}", "-".repeat(50));
    for [desc, value] in results.as_chunks().0 {
        println!("{desc:<35} {value}");
    }

    // --- 新增：抓取官方仓库包数量 ---
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

    println!("\n{:<35} Value", "Official Repository");
    println!("{}", "-".repeat(50));
    let mut total = 0;
    for (name, url) in repos {
        match fetch_package_count(url) {
            Ok(count) => {
                println!("{name:<35} {count}");
                total += count;
            }
            Err(e) => eprintln!("Failed to fetch {name} count: {e}"),
        }
    }
    println!("{:<35} {total}", "Total");

    Ok(())
}
