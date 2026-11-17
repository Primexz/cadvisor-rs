//! Optional HugeTLB fault accounting, without mixing in reservation counters.

use std::{collections::BTreeMap, fs, io, path::Path};

use crate::model::HugeTlb;

use super::{Reader, Version, key_values, limit};

pub(super) fn read(
    path: &Path,
    version: Version,
    reader: &mut Reader,
) -> BTreeMap<String, HugeTlb> {
    let mut values = BTreeMap::new();
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return values,
        Err(_) => {
            reader.errors += 1;
            return values;
        }
    };
    let suffix = match version {
        Version::V1 => ".usage_in_bytes",
        Version::V2 => ".current",
    };
    for entry in entries {
        let Ok(entry) = entry else {
            reader.errors += 1;
            continue;
        };
        let filename = entry.file_name();
        let Some(size) = filename
            .to_str()
            .and_then(|name| name.strip_prefix("hugetlb."))
            .and_then(|name| name.strip_suffix(suffix))
            .filter(|size| valid_size(size))
        else {
            continue;
        };
        let filetype = match entry.file_type() {
            Ok(filetype) => filetype,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(_) => {
                reader.errors += 1;
                continue;
            }
        };
        if !filetype.is_file() {
            continue;
        }
        let filename = |suffix: &str| path.join(format!("hugetlb.{size}.{suffix}"));
        let usage = reader.number(&entry.path());
        let value = match version {
            Version::V1 => HugeTlb {
                usage,
                peak: reader.number(&filename("max_usage_in_bytes")),
                limit: reader
                    .text(&filename("limit_in_bytes"))
                    .and_then(|text| limit(&text)),
                failures: reader.number(&filename("failcnt")),
            },
            Version::V2 => HugeTlb {
                usage,
                peak: None,
                limit: reader.text(&filename("max")).and_then(|text| limit(&text)),
                failures: reader
                    .text(&filename("events"))
                    .and_then(|text| key_values(&text).get("max").copied()),
            },
        };
        values.insert(size.to_owned(), value);
    }
    values
}

fn valid_size(size: &str) -> bool {
    ["KB", "MB", "GB", "TB"].iter().any(|unit| {
        size.strip_suffix(unit)
            .filter(|number| number.bytes().all(|byte| byte.is_ascii_digit()))
            .and_then(|number| number.parse::<u64>().ok())
            .is_some_and(|number| number > 0)
    })
}
