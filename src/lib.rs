//! sarDa turns sysstat SA files into a Grafana dashboard with one command.
//!
//! The data path, in the order the modules touch the data:
//!
//! * [`inputs`] turns the paths on the command line into SA files;
//! * [`sadf`] checks each input file (one `Verdict` per file) and runs `sadf`
//!   in a container, returning a marker stream of `sadf -d` output;
//! * [`dataset`] merges the files and applies the Observation window, giving
//!   one [`table::Table`] per Activity (the only module that reads `sadf`
//!   cells);
//! * [`store`] turns the tables into labelled Series;
//! * [`api`] and [`promql`] serve them to Grafana, [`dashboard`] generates the
//!   dashboard JSON, [`csvexport`] writes `--export-csv`;
//! * [`session`] orchestrates a run, [`grafana`] starts and probes the Grafana
//!   container, and [`resources`] owns every container and temp path a run
//!   creates, and their removal.
//!
//! [`catalog`] is the one place that knows the Activities.

pub mod api;
pub mod catalog;
pub mod cli;
pub mod csvexport;
pub mod dashboard;
pub mod dataset;
pub mod error;
pub mod grafana;
pub mod inputs;
pub mod promql;
pub mod resources;
pub mod runtime;
pub mod sadf;
pub mod session;
pub mod store;
pub mod table;
pub mod timeutil;

#[cfg(test)]
mod testutil;
