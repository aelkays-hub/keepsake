//! docs/SAFETY.md lists every code path that writes to disk. This test
//! keeps it honest: it finds every call that can write (create, write,
//! rename, delete, copy, a database, an ffmpeg output) in the non-test
//! code of every crate, and fails unless SAFETY.md has a row naming that
//! file and function. Every test SAFETY.md names must exist.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Calls that write to disk. Helpers that write (`write_json_atomic` and
/// friends) are listed too, so each caller is named in SAFETY.md.
const WRITES: &[&str] = &[
    "fs::write(",
    "File::create(",
    ".write(true)",
    ".create(true)",
    ".create_new(true)",
    ".append(true)",
    "fs::rename(",
    "remove_file(",
    "remove_dir(",
    "remove_dir_all(",
    "create_dir(",
    "create_dir_all(",
    "fs::copy(",
    "set_permissions(",
    "set_modified(",
    ".persist(",
    "write_json_atomic(",
    "write_bytes_atomic(",
    "write_atomic(",
    "Connection::open",
    "VACUUM INTO",
    "\"-y\"",
];

const ROOTS: &[&str] = &[
    "crates/keepsake-core/src",
    "crates/keepsake-platform/src",
    "crates/keepsake-cli/src",
    "app/ui/src-tauri/src",
];

fn rs_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            rs_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// The function a line is in: the nearest `fn name` above it.
fn enclosing_fn(lines: &[&str], at: usize) -> String {
    for l in lines[..=at].iter().rev() {
        let t = l.trim_start();
        let t = t
            .strip_prefix("pub(crate) ")
            .or_else(|| t.strip_prefix("pub "))
            .unwrap_or(t);
        let t = t.strip_prefix("async ").unwrap_or(t);
        if let Some(rest) = t.strip_prefix("fn ") {
            return rest
                .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                .next()
                .unwrap_or("?")
                .to_owned();
        }
    }
    "?".into()
}

/// `(file, function) → [(line, call)]` for every write in non-test code.
fn write_sites(root: &Path) -> BTreeMap<(String, String), Vec<(usize, &'static str)>> {
    let mut out: BTreeMap<(String, String), Vec<(usize, &'static str)>> = BTreeMap::new();
    for r in ROOTS {
        let mut files = Vec::new();
        rs_files(&root.join(r), &mut files);
        for f in files {
            let text = std::fs::read_to_string(&f).unwrap();
            let lines: Vec<&str> = text.lines().collect();
            // Unit tests sit at the end of a file, after `#[cfg(test)] mod`.
            let end = (0..lines.len())
                .find(|&i| {
                    lines[i].starts_with("#[cfg(test)]")
                        && lines.get(i + 1).is_some_and(|n| n.starts_with("mod "))
                })
                .unwrap_or(lines.len());
            let rel = f
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            for (i, l) in lines[..end].iter().enumerate() {
                let code = l.trim_start();
                if code.starts_with("//") {
                    continue;
                }
                for w in WRITES {
                    if code.contains(w)
                        && !code.contains(&format!("fn {}", w.trim_end_matches('(')))
                    {
                        out.entry((rel.clone(), enclosing_fn(&lines, i)))
                            .or_default()
                            .push((i + 1, w));
                    }
                }
            }
        }
    }
    out
}

/// Backticked tokens of each table row in SAFETY.md.
fn rows(doc: &str) -> Vec<Vec<String>> {
    doc.lines()
        .filter(|l| l.starts_with('|'))
        .map(|l| {
            l.split('`')
                .enumerate()
                .filter(|(i, _)| i % 2 == 1)
                .map(|(_, t)| t.to_owned())
                .collect()
        })
        .collect()
}

#[test]
fn every_write_to_disk_is_listed_in_safety_md() {
    let root = common::workspace_root();
    let doc = std::fs::read_to_string(root.join("docs").join("SAFETY.md")).unwrap();
    let rows = rows(&doc);
    let sites = write_sites(&root);
    assert!(
        sites.len() > 40,
        "the scan found too little: {}",
        sites.len()
    );
    let missing: Vec<String> = sites
        .iter()
        .filter(|((file, func), _)| {
            !rows
                .iter()
                .any(|r| r.iter().any(|t| t == file) && r.iter().any(|t| t == func))
        })
        .map(|((file, func), hits)| {
            let at: Vec<String> = hits.iter().map(|(n, w)| format!("{n}: {w}")).collect();
            format!("{file} `{func}` ({})", at.join(", "))
        })
        .collect();
    assert!(
        missing.is_empty(),
        "writes to disk not listed in docs/SAFETY.md (add a row naming the file and function, \
         what it writes, where, and the test):\n{}",
        missing.join("\n")
    );
}

#[test]
fn the_tests_safety_md_names_exist() {
    let root = common::workspace_root();
    let doc = std::fs::read_to_string(root.join("docs").join("SAFETY.md")).unwrap();
    let mut sources = String::new();
    for dir in [
        "crates/keepsake-core",
        "crates/keepsake-cli",
        "crates/keepsake-platform",
        "app/ui/src-tauri",
    ] {
        let mut files = Vec::new();
        rs_files(&root.join(dir), &mut files);
        for f in files {
            sources.push_str(&std::fs::read_to_string(f).unwrap());
        }
    }
    let mut e2e = String::new();
    let mut stack = vec![root.join("app/ui/e2e")];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "ts") {
                e2e.push_str(&std::fs::read_to_string(p).unwrap());
            }
        }
    }
    let mut named = BTreeSet::new();
    for r in rows(&doc) {
        for t in r {
            if let Some((file, test)) = t.split_once("::") {
                named.insert((file.to_owned(), test.to_owned()));
            }
        }
    }
    assert!(
        named.len() > 10,
        "SAFETY.md names its tests as `file::test`"
    );
    let gone: Vec<String> = named
        .iter()
        .filter(|(file, test)| {
            if file.ends_with(".spec.ts") {
                !e2e.contains(test.as_str())
            } else {
                !sources.contains(&format!("fn {test}("))
            }
        })
        .map(|(f, t)| format!("{f}::{t}"))
        .collect();
    assert!(
        gone.is_empty(),
        "SAFETY.md names tests that do not exist:\n{}",
        gone.join("\n")
    );
}
