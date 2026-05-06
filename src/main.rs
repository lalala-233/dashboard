use anyhow::Result;
use scraper::{Html, Selector};

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

    Ok(())
}
