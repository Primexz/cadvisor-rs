//! Process and task observations without executing tools or entering namespaces.

use crate::model::{Processes, Ulimit};
use std::{fs, path::Path};

/// Parse the soft/hard columns of procfs limits with cAdvisor-style names.
#[must_use]
pub fn parse_ulimits(text: &str) -> Vec<Ulimit> {
    const NAMES: &[(&str, &str)] = &[
        ("Max cpu time", "max_cpu_time"),
        ("Max file size", "max_file_size"),
        ("Max data size", "max_data_size"),
        ("Max stack size", "max_stack_size"),
        ("Max core file size", "max_core_file_size"),
        ("Max resident set", "max_resident_set"),
        ("Max processes", "max_processes"),
        ("Max open files", "max_open_files"),
        ("Max locked memory", "max_locked_memory"),
        ("Max address space", "max_address_space"),
        ("Max file locks", "max_file_locks"),
        ("Max pending signals", "max_pending_signals"),
        ("Max msgqueue size", "max_msgqueue_size"),
        ("Max nice priority", "max_nice_priority"),
        ("Max realtime priority", "max_realtime_priority"),
        ("Max realtime timeout", "max_realtime_timeout"),
    ];
    let number = |text: &str| {
        if text == "unlimited" {
            Some(-1)
        } else {
            text.parse().ok()
        }
    };
    text.lines()
        .filter_map(|line| {
            let (prefix, name) = NAMES.iter().find(|(prefix, _)| line.starts_with(prefix))?;
            let mut columns = line.strip_prefix(prefix)?.split_whitespace();
            Some(Ulimit {
                name: (*name).into(),
                soft: number(columns.next()?)?,
                hard: number(columns.next()?)?,
            })
        })
        .collect()
}

fn task_state(text: &str) -> Option<usize> {
    let state = text.rsplit_once(')')?.1.split_whitespace().next()?;
    Some(match state {
        "R" => 0,
        "S" => 1,
        "T" | "t" => 2,
        "D" => 3,
        "Z" => 4,
        "I" => 5,
        _ => 6,
    })
}

fn descriptors(path: &Path, maximum: u64) -> std::io::Result<(u64, u64)> {
    let mut files = 0;
    let mut sockets = 0;
    for entry in fs::read_dir(path.join("fd"))? {
        let entry = entry?;
        files += 1;
        if files > maximum {
            return Err(std::io::Error::other("descriptor sampling bound exceeded"));
        }
        if fs::read_link(entry.path())?
            .to_string_lossy()
            .starts_with("socket:[")
        {
            sockets += 1;
        }
    }
    Ok((files, sockets))
}

/// Sample a bounded set of PIDs. Incomplete FD/task/scheduler sums stay absent.
/// Resource counters from cgroups are independent of these optional procfs reads.
#[must_use]
pub fn read_processes(root: &Path, ids: &[u32], init: Option<u32>, maximum: u32) -> Processes {
    if ids.len() > maximum as usize {
        return Processes {
            errors: 1,
            ..Processes::default()
        };
    }
    let mut result = Processes {
        count: Some(ids.len() as u64),
        ..Processes::default()
    };
    let mut files = 0;
    let mut sockets = 0;
    let mut tasks = [0; 7];
    let mut threads = 0;
    let mut runtime = 0.0;
    let mut waiting = 0.0;
    let mut periods = 0u128;
    let (mut fd_ok, mut task_ok, mut sched_ok) = (true, true, true);
    for pid in ids {
        let path = root.join(pid.to_string());
        if fd_ok {
            match descriptors(&path, 65536 - files) {
                Ok((f, s)) => {
                    files += f;
                    sockets += s;
                }
                Err(_) => {
                    result.errors += 1;
                    fd_ok = false;
                }
            }
        }
        let entries = match fs::read_dir(path.join("task")) {
            Ok(entries) => entries,
            Err(_) => {
                result.errors += 1;
                task_ok = false;
                sched_ok = false;
                continue;
            }
        };
        for entry in entries {
            let Ok(entry) = entry else {
                result.errors += 1;
                task_ok = false;
                sched_ok = false;
                continue;
            };
            if entry.file_name().to_string_lossy().parse::<u32>().is_err() {
                continue;
            }
            threads += 1;
            if threads > u64::from(maximum) {
                result.errors += 1;
                task_ok = false;
                sched_ok = false;
                fd_ok = false;
                break;
            }
            match fs::read_to_string(entry.path().join("stat"))
                .ok()
                .and_then(|v| task_state(&v))
            {
                Some(state) => tasks[state] += 1,
                None => {
                    result.errors += 1;
                    task_ok = false;
                }
            }
            let stat = fs::read_to_string(entry.path().join("schedstat"))
                .ok()
                .and_then(|v| {
                    let mut fields = v.split_whitespace();
                    Some((
                        fields.next()?.parse::<u64>().ok()?,
                        fields.next()?.parse::<u64>().ok()?,
                        fields.next()?.parse::<u64>().ok()?,
                    ))
                });
            match stat {
                Some((run, wait, switches)) => {
                    runtime += run as f64 / 1e9;
                    waiting += wait as f64 / 1e9;
                    periods += u128::from(switches);
                }
                None => {
                    result.errors += 1;
                    sched_ok = false;
                }
            }
        }
        if threads > u64::from(maximum) {
            break;
        }
    }
    if fd_ok {
        result.files = Some(files);
        result.sockets = Some(sockets);
    }
    if task_ok {
        result.threads = Some(threads);
        result.tasks = Some(tasks);
    }
    if sched_ok {
        result.sched_runtime = Some(runtime);
        result.sched_wait = Some(waiting);
        result.sched_periods = periods.try_into().ok();
        if result.sched_periods.is_none() {
            result.errors += 1;
        }
    }
    let init = init
        .filter(|pid| ids.contains(pid))
        .or_else(|| ids.first().copied());
    if let Some(pid) = init
        && let Ok(text) = fs::read_to_string(root.join(pid.to_string()).join("limits"))
    {
        result.ulimits = parse_ulimits(&text);
    }
    result
}
