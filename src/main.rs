#[macro_use]
extern crate log;

use std::fs::read_dir;
use std::path::Path;
use std::str::FromStr;

use anyhow::{Context, Result};

const RELEASES: &[&str] = &[
    // 12.04
    "precise", "quantal", "raring", "saucy",
    // 14.04
    "trusty", "utopic", "vivid", "wily",
    // 16.04
    "xenial", "yakkety", "zesty", "artful",
    // 18.04
    "bionic", "cosmic", "disco", "eoan",
    // 20.04
    "focal",
    // TODO: Add future releases
];

fn main() -> Result<()> {
    env_logger::init();

    let path = "/etc/apt/sources.list.d/";
    info!("source.list.d: {}", path);

    for entry in read_dir(path)? {
        let path_buf = entry?.path();
        let path = path_buf.to_str().context("path is not utf-8")?;
        if !path.ends_with(".list") {
            trace!("Skip {}", path);
            continue;
        }
        match update_file(&path_buf) {
            Ok(_) => {}
            Err(err) => {
                error!("Error on: {}", path);
                error!("Reason: {}", err);
            }
        }
    }
    Ok(())
}

fn update_file(path: &Path) -> Result<()> {
    info!("Read {}", path.to_str().unwrap());

    let content = std::fs::read_to_string(path)?;
    let mut output = Vec::new();
    let mut new_sources = Vec::new();
    for line in content.lines() {
        let line = if let Some(line) = disabled_on_upgrade(line) {
            line
        } else {
            line
        };

        match Source::from_str(line)
            .map_err(|_| anyhow::anyhow!("Cannot parse"))
            .and_then(try_update_source)
        {
            Ok(new_source) => {
                output.push(new_source.to_string());
                new_sources.push(new_source);
            }
            Err(_) => output.push(line.to_string()),
        }
    }
    let mut final_output: String = output.join("\n");
    if !final_output.ends_with("\n") {
        final_output.push_str("\n");
    }

    if !new_sources.is_empty() {
        println!("Updated {}", path.to_string_lossy());
        for source in new_sources {
            println!("  {}", source.to_string());
        }
        let mut bak = path.to_path_buf();
        bak.set_extension("list.bak");
        std::fs::copy(path, bak).context("Failed to copy backup")?;
        std::fs::write(path, final_output.as_bytes()).context("Failed to write new output")?;
    }
    Ok(())
}

fn disabled_on_upgrade(line: &str) -> Option<&str> {
    let line = line.trim();
    if line.starts_with("#") && line.contains("disabled on upgrade") {
        Some(line.trim_start_matches("#"))
    } else {
        None
    }
}

fn try_update_source(mut source: Source) -> Result<Source> {
    let old_source = source.clone();
    let mut old_dist = None;
    let mut new_source = None;
    for i in 0..RELEASES.len() - 1 {
        let current = RELEASES[i];
        let next = RELEASES[i + 1];

        if !source.dist.contains(current) {
            continue;
        }
        if old_dist.is_none() {
            old_dist = Some(source.dist.clone());
        }

        source.dist = source.dist.replace(current, next);

        if reqwest::blocking::get(&source.release_url())
            .and_then(|x| x.error_for_status())
            .is_ok()
        {
            new_source = Some(source.clone());
        }
    }

    let new_source = if let Some(new_source) = new_source {
        new_source
    } else {
        return Err(anyhow::anyhow!("not found"));
    };

    // TODO: wrap around problem
    if RELEASES.last().expect("should exist") < &old_dist.expect("should exist").as_str() {
        error!("Built in list of releases is oudated. Distribution might regress");
        return Err(anyhow::anyhow!("possible regression"));
    }

    info!("old: {}", old_source.to_string());
    info!("updated: {} -> {}", old_source.dist, new_source.dist);
    debug!("new: {}", new_source.to_string());
    debug!("new release url: {}", new_source.release_url());
    Ok(new_source)
}

#[derive(Clone)]
struct Source {
    archive_type: String,
    arch: Option<String>,
    url: String,
    dist: String,
    components: Vec<String>,
}

impl Source {
    fn to_string(&self) -> String {
        let mut output: Vec<&str> = Vec::new();
        output.push(&self.archive_type);
        if let Some(arch) = self.arch.as_ref() {
            output.push(&arch);
        }
        output.push(&self.url);
        output.push(&self.dist);
        for items in self.components.iter() {
            output.push(items);
        }
        output.join(" ")
    }

    fn release_url(&self) -> String {
        format!("{}/dists/{}/Release", self.url, self.dist)
    }
}

impl FromStr for Source {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        let s = &s[0..s.find('#').unwrap_or(s.len())];
        if s.is_empty() {
            return Err(());
        }
        let items: Vec<_> = s.split_whitespace().collect();
        if items.len() < 2 // To short to even determine
            || (items[1].starts_with("[") && items.len() < 4) // have arch but too short
            || items.len() < 3 // doesn't have arch but too short
        {
            return Err(());
        }
        if !matches!(items.get(0), Some(&"deb") | Some(&"deb-src")) {
            return Err(());
        }
        let archive_type = items[0].to_string();
        let (arch, rest) = if items[1].starts_with("[") {
            (Some(items[1].to_string()), &items[2..])
        } else {
            (None, &items[1..])
        };
        let url = rest[0].to_string();
        let dist = rest[1].to_string();
        let components = rest[2..].iter().map(|x| x.to_string()).collect();
        Ok(Source {
            archive_type,
            arch,
            url,
            dist,
            components,
        })
    }
}
