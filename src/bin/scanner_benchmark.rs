use std::{path::PathBuf, time::Instant};

use mylib_server::{
    libraries::LibraryType,
    scanner::{discover, parse_filename},
};
use tokio::sync::{mpsc, watch};
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let count = std::env::args()
        .nth(1)
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(10_000);
    let root = std::env::temp_dir().join(format!("mylib-scanner-benchmark-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root)?;
    for index in 0..count {
        let folder = root.join(format!("batch-{}", index / 1000));
        std::fs::create_dir_all(&folder)?;
        std::fs::File::create(folder.join(format!(
            "Example.Movie.{:04}.Edition.{index}.1080p.mkv",
            1900 + index % 126
        )))?;
    }
    let started = Instant::now();
    let (sender, mut receiver) = mpsc::channel(500);
    let (_, cancel) = watch::channel(false);
    let discovery_root: PathBuf = root.clone();
    let task = tokio::spawn(async move { discover(discovery_root, sender, cancel).await });
    let mut parsed = 0_usize;
    while let Some(file) = receiver.recv().await {
        let _ = parse_filename(&file.filename, LibraryType::Movie);
        parsed += 1;
    }
    task.await??;
    let elapsed = started.elapsed();
    println!(
        "files={parsed} elapsed_ms={} files_per_second={:.0}",
        elapsed.as_millis(),
        parsed as f64 / elapsed.as_secs_f64()
    );
    std::fs::remove_dir_all(&root)?;
    Ok(())
}
