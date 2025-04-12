#[macro_use]
extern crate log;

use std::fs::read_dir;
use std::path::Path;
use std::str::FromStr;

use anyhow::{Context, Result};
use deb822_lossless::{Deb822, Paragraph};

#[derive(Eq, PartialEq, Hash)]
struct Release<'a> {
    name: &'a str,
    version: &'a str,
}

#[rustfmt::skip]
const RELEASES: &[Release<'static>] = &[
    Release { name: "precise", version: "12.04" }, Release { name: "quantal", version: "12.10" },
    Release { name: "raring", version: "13.04" }, Release { name: "saucy", version: "13.10" },
    Release { name: "trusty", version: "14.04" }, Release { name: "utopic", version: "14.10" },
    Release { name: "vivid", version: "15.04" }, Release { name: "wily", version: "15.10" },
    Release { name: "xenial", version: "16.04" }, Release { name: "yakkety", version: "16.10" },
    Release { name: "zesty", version: "17.04" }, Release { name: "artful", version: "17.10" },
    Release { name: "bionic", version: "18.04" }, Release { name: "cosmic", version: "18.10" },
    Release { name: "disco", version: "19.04" }, Release { name: "eoan", version: "19.10" },
    Release { name: "focal", version: "20.04" }, Release { name: "groovy", version: "20.10" },
    Release { name: "hirsute", version: "21.04" }, Release { name: "impish", version: "21.10" },
    Release { name: "jammy", version: "22.04" }, Release { name: "kinetic", version: "22.10" },
    Release { name: "lunar", version: "23.04" }, Release { name: "mantic", version: "23.10" },
    Release { name: "noble", version: "24.04" }, Release { name: "oracular", version: "24.10" },
    Release { name: "plucky", version: "25.04" },
    // TODO: Add future releases
];

fn main() -> Result<()> {
    env_logger::init();

    let path = "/etc/apt/sources.list.d/";
    info!("source.list.d: {}", path);

    for entry in read_dir(path)? {
        let path_buf = entry?.path();
        let path = path_buf.to_str().context("path is not utf-8")?;
        if !path.ends_with(".sources") {
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
    let deb822 = Deb822::from_str(&content)?;
    let mut updated = false;
    for mut paragraph in deb822.paragraphs() {
        updated |= try_update_paragraph(&mut paragraph)?;
    }

    if !updated {
        return Ok(());
    }

    println!("Updated {}", path.to_string_lossy());

    let mut bak = path.to_path_buf();
    bak.set_extension("soruces.bak");
    std::fs::copy(path, bak).context("Failed to copy backup")?;
    std::fs::write(path, deb822.to_string().as_bytes()).context("Failed to write new output")?;

    Ok(())
}

fn try_update_paragraph(paragraph: &mut Paragraph) -> Result<bool> {
    if get_http_uri(paragraph).is_none() {
        return Ok(false);
    };

    let mut updated = false;
    if paragraph.contains_key("Enabled") {
        updated = true;
        paragraph.remove("Enabled");
    }

    if try_update_by_suites_name(paragraph)? {
        return Ok(true);
    }

    Ok(updated)
}

fn try_update_by_suites_name(paragraph: &mut Paragraph) -> Result<bool> {
    let Some(uris) = get_http_uri(paragraph) else {
        return Ok(false);
    };

    let Some(mut suites) = paragraph.get("Suites") else {
        return Ok(false);
    };

    let range = get_release_range_by_name(&suites);
    if range.is_empty() {
        return Ok(false);
    }

    let mut updated = false;
    for i in 1..range.len() {
        let prev = &range[i - 1];
        let current = &range[i];
        suites = suites.replace(prev.name, current.name);
        let mut all = true;
        for suite in suites.split_ascii_whitespace() {
            if !is_available(&uris, suite) {
                all = false;
                break;
            }
        }

        if !all {
            break;
        }

        updated = true;
        paragraph.set("Suites", &suites);
    }

    Ok(updated)
}

fn get_release_range_by_name(target: &str) -> &'static [Release<'static>] {
    for i in 0..RELEASES.len() {
        let release = &RELEASES[i];
        if target.contains(release.name) {
            return &RELEASES[i..];
        }
    }

    &[]
}

fn get_http_uri(paragraph: &Paragraph) -> Option<String> {
    let uri = paragraph.get("URIs")?;

    if uri.starts_with("http://") {
        return Some(uri);
    }
    if uri.starts_with("https://") {
        return Some(uri);
    }

    None
}

fn is_available(uri: &str, suite: &str) -> bool {
    let release_url = format!("{uri}/dists/{suite}/Release");

    reqwest::blocking::get(&release_url)
        .and_then(|x| x.error_for_status())
        .is_ok()
}
