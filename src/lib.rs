//! Linux cgroup collection and cached Prometheus exposition.
//!
//! Collection is independent of HTTP, so fixtures can exercise both Linux cgroup
//! layouts on any development platform. The exporter never writes cgroups.
#![forbid(unsafe_code)]

pub mod cgroup;
pub mod collector;
pub mod config;
pub mod docker;
pub mod metrics;
pub mod model;
pub mod procfs;
pub mod server;
