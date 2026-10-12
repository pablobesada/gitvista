use gitvista_core::{Cancellation, DiffKind, DiffMode};
use gitvista_git::CliGit;
use serde_json::json;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
    time::Instant,
};

/// Headless diagnostics use exactly the shipped Git adapter and never mutate repositories.
pub fn try_run() -> bool {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).is_none_or(|s| s != "--benchmark") {
        return false;
    }
    let Some(root) = args.get(2) else {
        eprintln!("Usage: gitvista --benchmark ROOT [ITERATIONS]");
        std::process::exit(2);
    };
    let root = PathBuf::from(root);
    let iterations = args
        .get(3)
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(10)
        .clamp(1, 100);
    let backend = CliGit::new();
    let control = Cancellation::new();
    let started = Instant::now();
    let repos = match backend.discover(&root, &[], &control) {
        Ok(repos) => repos,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };
    let discovery_ms = started.elapsed().as_secs_f64() * 1000.0;
    let mut timings = Vec::new();
    let mut final_count = 0;
    let mut changed_repos = 0;
    let mut diff_sample = None;
    let mut failures = Vec::new();
    for iteration in 0..iterations {
        let begin = Instant::now();
        let next = AtomicUsize::new(0);
        let results = Mutex::new(Vec::new());
        std::thread::scope(|scope| {
            for _ in 0..3 {
                scope.spawn(|| loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(repo) = repos.get(index) else {
                        break;
                    };
                    let result = backend.status(repo, &root, &control);
                    results.lock().unwrap().push((index, result));
                });
            }
        });
        timings.push(begin.elapsed().as_secs_f64() * 1000.0);
        if iteration + 1 == iterations {
            for (index, result) in results.into_inner().unwrap() {
                match result {
                    Ok(entries) => {
                        if !entries.is_empty() {
                            changed_repos += 1;
                        }
                        final_count += entries.len();
                        if diff_sample.is_none() {
                            if let Some(entry) = entries
                                .iter()
                                .find(|e| e.kind == gitvista_core::EntryKind::File)
                            {
                                diff_sample = Some((repos[index].clone(), entry.clone()));
                            }
                        }
                    }
                    Err(error) => failures
                        .push(json!({"repository": repos[index].relative_path, "error": error})),
                }
            }
        }
    }
    timings.sort_by(f64::total_cmp);
    let median = timings[timings.len() / 2];
    let p95 = timings[((timings.len() as f64 * 0.95).ceil() as usize).saturating_sub(1)];
    let diff = diff_sample.map(|(repo, entry)| {
        let start = Instant::now();
        match backend.diff(&repo, &entry, DiffMode::Total, &control) {
            Ok(result) => json!({"ms": start.elapsed().as_secs_f64() * 1000.0, "bytes": result.patch.len(), "text": result.kind == DiffKind::Text}),
            Err(error) => json!({"error": error}),
        }
    });
    println!("{}", serde_json::to_string_pretty(&json!({"repositories": repos.len(), "changedRepositories": changed_repos, "changedEntries": final_count, "workers": 3, "iterations": iterations, "discoveryMs": discovery_ms, "statusMedianMs": median, "statusP95Ms": p95, "initialMedianMs": discovery_ms + median, "sampleDiff": diff, "errors": failures})).unwrap());
    if !failures.is_empty() {
        std::process::exit(1);
    }
    true
}
