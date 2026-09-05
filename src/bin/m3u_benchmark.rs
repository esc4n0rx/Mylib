//! Local M3U parser/normalizer benchmark. No TMDB, no database — measures the
//! streaming parse + per-entry analysis that must stay linear for 100k+ entry
//! playlists (Task 12 §88/§89).
//!
//! Usage: `cargo run --release --bin m3u_benchmark -- 10000 50000 100000 250000`

use std::{io::Write, time::Instant};

use mylib_server::features::remote_sources::m3u::analyze_stream;
use tokio::io::BufReader;
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sizes: Vec<usize> = {
        let args: Vec<usize> = std::env::args()
            .skip(1)
            .filter_map(|value| value.parse().ok())
            .collect();
        if args.is_empty() {
            vec![10_000, 50_000, 100_000, 250_000]
        } else {
            args
        }
    };

    println!(
        "{:>10}  {:>12}  {:>14}  {:>12}",
        "entries", "bytes", "parse_ms", "entries/s"
    );
    for count in sizes {
        let path = std::env::temp_dir().join(format!("mylib-m3u-benchmark-{}.m3u", Uuid::new_v4()));
        let bytes = write_playlist(&path, count)?;

        let file = tokio::fs::File::open(&path).await?;
        let started = Instant::now();
        let summary = analyze_stream(BufReader::new(file), u64::MAX).await?;
        let elapsed = started.elapsed();
        assert_eq!(summary.total_entries as usize, count);

        println!(
            "{count:>10}  {bytes:>12}  {:>14}  {:>12.0}",
            elapsed.as_millis(),
            count as f64 / elapsed.as_secs_f64(),
        );
        std::fs::remove_file(&path).ok();
    }
    Ok(())
}

fn write_playlist(path: &std::path::Path, count: usize) -> std::io::Result<u64> {
    let categories = [
        "FILMES | LANÇAMENTOS 2025",
        "FILMES | AÇÃO",
        "SERIES | NETFLIX",
    ];
    let mut file = std::io::BufWriter::new(std::fs::File::create(path)?);
    writeln!(file, "#EXTM3U")?;
    for index in 0..count {
        let group = categories[index % categories.len()];
        if group.starts_with("SERIES") {
            let season = index % 8 + 1;
            let episode = index % 24 + 1;
            writeln!(
                file,
                "#EXTINF:-1 tvg-name=\"Serie Exemplo {n} (2019) S{s:02}E{e:02}\" tvg-logo=\"https://logos.example/{n}.png\" group-title=\"{group}\",Serie Exemplo {n} S{s:02}E{e:02} [LEG]",
                n = index % 400,
                s = season,
                e = episode,
            )?;
        } else {
            writeln!(
                file,
                "#EXTINF:-1 tvg-name=\"Filme Exemplo {n} ({y})\" tvg-logo=\"https://logos.example/{n}.png\" group-title=\"{group}\",Filme Exemplo {n} ({y}) 1080p WEB-DL",
                n = index,
                y = 1980 + index % 45,
            )?;
        }
        writeln!(file, "https://origin.example/stream/{index}.mp4?token=demo")?;
    }
    file.flush()?;
    Ok(std::fs::metadata(path)?.len())
}
